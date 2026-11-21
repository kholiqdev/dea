//! TLS for HTTPS connections: rustls with the ring provider, verified
//! against the operating system's trust store plus the request's extra CAs.

use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::verify_server_cert_signed_by_trust_anchor;
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, ServerName, SignatureVerificationAlgorithm};
use rustls::pki_types::{PrivateKeyDer, UnixTime};
use rustls::server::ParsedCertificate;
use rustls::{
    AlertDescription, CertificateError, ClientConfig, DigitallySignedStruct, RootCertStore,
    SignatureScheme,
};
use sha2::{Digest as _, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use url::Host;

use crate::error::{HttpError, TlsError, TlsErrorKind, describe};
use crate::request::TlsConfig;

/// ALPN protocols offered, preferred first. HTTP/2 is only used over TLS
/// (decision 15).
const ALPN: [&[u8]; 2] = [b"h2", b"http/1.1"];

/// What makes two TLS settings give different connections. Requests with
/// equal fingerprints may share a pooled connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TlsFingerprint {
    verify: bool,
    extra_roots: [u8; 32],
    client_identity: Option<[u8; 32]>,
}

impl TlsFingerprint {
    pub(crate) fn new(config: &TlsConfig, verify: bool) -> Self {
        TlsFingerprint {
            verify,
            extra_roots: hash(config.extra_roots_pem.iter().map(Vec::as_slice)),
            client_identity: config.client_identity.as_ref().map(|identity| {
                hash([
                    identity.certificate_pem.as_slice(),
                    identity.key_pem.as_slice(),
                ])
            }),
        }
    }
}

/// SHA-256 over each part with its length, so the split between parts
/// counts.
fn hash<'a>(parts: impl IntoIterator<Item = &'a [u8]>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// How many TLS settings keep their client config. A person uses a few at
/// a time.
const MAX_CONFIGS: usize = 16;

/// Client configs by TLS settings, so a new connection reuses the verifier
/// (which may read the operating system's trust store when it is made) and
/// the TLS sessions it can resume. The least recently used goes first.
#[derive(Default)]
pub(crate) struct TlsConfigs {
    built: Mutex<Vec<(TlsFingerprint, Arc<ClientConfig>)>>,
}

impl TlsConfigs {
    /// The client config for `config` and `verify`.
    pub(crate) fn get(
        &self,
        config: &TlsConfig,
        verify: bool,
    ) -> Result<Arc<ClientConfig>, HttpError> {
        let key = TlsFingerprint::new(config, verify);
        if let Some(found) = self.find(&key) {
            return Ok(found);
        }
        // Made without the lock: the verifier may read the trust store.
        let made = Arc::new(client_config(config, verify)?);
        let mut built = self.lock();
        if let Some((_, found)) = built.iter().find(|(k, _)| *k == key) {
            // Another connection made it meanwhile.
            return Ok(Arc::clone(found));
        }
        if built.len() >= MAX_CONFIGS {
            built.remove(0);
        }
        built.push((key, Arc::clone(&made)));
        Ok(made)
    }

    /// The config made for `key`, now the most recently used.
    fn find(&self, key: &TlsFingerprint) -> Option<Arc<ClientConfig>> {
        let mut built = self.lock();
        let at = built.iter().position(|(k, _)| k == key)?;
        let entry = built.remove(at);
        let config = Arc::clone(&entry.1);
        built.push(entry);
        Some(config)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(TlsFingerprint, Arc<ClientConfig>)>> {
        self.built.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Runs the TLS handshake over `stream` and reports whether the server
/// chose HTTP/2.
pub(crate) async fn handshake(
    stream: TcpStream,
    host: &Host<String>,
    config: Arc<ClientConfig>,
) -> Result<(TlsStream<TcpStream>, bool), HttpError> {
    let connector = TlsConnector::from(config);
    let server_name = match host {
        Host::Domain(name) => ServerName::try_from(name.clone()).map_err(|_| {
            HttpError::Tls(TlsError::other(format!(
                "{name} is not a valid TLS server name"
            )))
        })?,
        Host::Ipv4(ip) => ServerName::from(*ip),
        Host::Ipv6(ip) => ServerName::from(*ip),
    };
    let stream = connector.connect(server_name, stream).await.map_err(|e| {
        HttpError::Tls(tls_error(&e).unwrap_or_else(|| TlsError::other(describe(&e))))
    })?;
    let h2 = stream.get_ref().1.alpn_protocol() == Some(b"h2");
    Ok((stream, h2))
}

/// The TLS error behind `error`, an error of a TLS stream. `None` unless
/// rustls caused it.
pub(crate) fn tls_error(error: &io::Error) -> Option<TlsError> {
    let cause = error.get_ref()?.downcast_ref::<rustls::Error>()?;
    Some(TlsError {
        kind: kind_of(cause),
        message: describe(error),
    })
}

/// What `error` is about, told from its type alone.
fn kind_of(error: &rustls::Error) -> TlsErrorKind {
    match error {
        rustls::Error::InvalidCertificate(certificate) => match certificate {
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                TlsErrorKind::Expired
            }
            CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                TlsErrorKind::NotYetValid
            }
            CertificateError::UnknownIssuer => TlsErrorKind::UnknownIssuer,
            CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. } => {
                TlsErrorKind::WrongHost
            }
            _ => TlsErrorKind::Other,
        },
        // Sent by a server about the client's certificate, or its lack.
        rustls::Error::AlertReceived(
            AlertDescription::CertificateRequired
            | AlertDescription::BadCertificate
            | AlertDescription::UnknownCA
            | AlertDescription::CertificateUnknown
            | AlertDescription::AccessDenied,
        ) => TlsErrorKind::ClientCertificateRejected,
        _ => TlsErrorKind::Other,
    }
}

fn client_config(config: &TlsConfig, verify: bool) -> Result<ClientConfig, HttpError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier: Arc<dyn ServerCertVerifier> = if verify {
        Arc::new(Platform {
            verifier: platform_verifier(config, &provider)?,
            algorithms: provider.signature_verification_algorithms,
        })
    } else {
        Arc::new(AcceptAny(provider.signature_verification_algorithms))
    };
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| HttpError::Tls(TlsError::other(describe(&e))))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);
    let mut client = match &config.client_identity {
        Some(identity) => {
            let certs = CertificateDer::pem_slice_iter(&identity.certificate_pem)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    HttpError::Tls(TlsError::other(format!(
                        "could not read the client certificate: {e}"
                    )))
                })?;
            // The key's own error text stays out of the message.
            let key = PrivateKeyDer::from_pem_slice(&identity.key_pem).map_err(|_| {
                HttpError::Tls(TlsError::other("could not read the client private key"))
            })?;
            builder.with_client_auth_cert(certs, key).map_err(|e| {
                HttpError::Tls(TlsError::other(format!(
                    "the client certificate is not usable: {e}"
                )))
            })?
        }
        None => builder.with_no_client_auth(),
    };
    client.alpn_protocols = ALPN.iter().map(|p| p.to_vec()).collect();
    Ok(client)
}

/// Trusts the operating system's roots plus the request's extra CAs.
fn platform_verifier(
    config: &TlsConfig,
    provider: &Arc<rustls::crypto::CryptoProvider>,
) -> Result<rustls_platform_verifier::Verifier, HttpError> {
    let mut roots = Vec::new();
    for pem in &config.extra_roots_pem {
        for cert in CertificateDer::pem_slice_iter(pem) {
            roots.push(cert.map_err(|e| {
                HttpError::Tls(TlsError::other(format!(
                    "could not read an extra CA certificate: {e}"
                )))
            })?);
        }
    }
    rustls_platform_verifier::Verifier::new_with_extra_roots(roots, Arc::clone(provider))
        .map_err(|e| HttpError::Tls(TlsError::other(describe(&e))))
}

/// The operating system's verifier, which also names a certificate out of
/// its dates. On macOS rustls-platform-verifier reports one only as a
/// platform error code.
#[derive(Debug)]
struct Platform {
    verifier: rustls_platform_verifier::Verifier,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Platform {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.verifier
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
            .map_err(|error| match error {
                rustls::Error::InvalidCertificate(CertificateError::Other(_)) => {
                    out_of_date(end_entity, now, self.algorithms.all).unwrap_or(error)
                }
                error => error,
            })
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.verifier.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.verifier.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.verifier.supported_verify_schemes()
    }
}

/// The error for `end_entity` when `now` is outside its dates. Without
/// trust anchors webpki checks the dates before it looks for an issuer.
fn out_of_date(
    end_entity: &CertificateDer<'_>,
    now: UnixTime,
    algorithms: &[&dyn SignatureVerificationAlgorithm],
) -> Option<rustls::Error> {
    let parsed = ParsedCertificate::try_from(end_entity).ok()?;
    let roots = RootCertStore::empty();
    match verify_server_cert_signed_by_trust_anchor(&parsed, &roots, &[], now, algorithms) {
        Err(error)
            if matches!(
                kind_of(&error),
                TlsErrorKind::Expired | TlsErrorKind::NotYetValid
            ) =>
        {
            Some(error)
        }
        _ => None,
    }
}

/// The verifier for `verify_tls: false`: any certificate and any host name
/// pass. The handshake signatures are still checked, so the peer must own
/// the key of the certificate it shows.
#[derive(Debug)]
struct AcceptAny(WebPkiSupportedAlgorithms);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.0)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.0)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use dea_testkit::http::client_identity_pem;
    use zeroize::Zeroizing;

    use super::*;
    use crate::request::ClientIdentity;

    #[test]
    fn same_settings_share_one_config_and_others_do_not() {
        let configs = TlsConfigs::default();
        let (certificate_pem, key_pem) = client_identity_pem();
        let plain = TlsConfig::default();
        let rooted = TlsConfig {
            extra_roots_pem: vec![certificate_pem.clone()],
            client_identity: None,
        };
        let identified = TlsConfig {
            extra_roots_pem: Vec::new(),
            client_identity: Some(ClientIdentity {
                certificate_pem,
                key_pem: Zeroizing::new(key_pem),
            }),
        };
        let first = configs.get(&plain, true).unwrap();
        assert!(Arc::ptr_eq(&first, &configs.get(&plain, true).unwrap()));
        let others = [
            configs.get(&plain, false).unwrap(),
            configs.get(&rooted, true).unwrap(),
            configs.get(&identified, true).unwrap(),
        ];
        for (i, other) in others.iter().enumerate() {
            assert!(!Arc::ptr_eq(&first, other), "{i}");
        }
        assert!(Arc::ptr_eq(
            &others[2],
            &configs.get(&identified, true).unwrap()
        ));
        assert_eq!(configs.len(), 4);
    }

    #[test]
    fn the_least_recently_used_config_goes_first() {
        let configs = TlsConfigs::default();
        // Unverified, so the roots are only told apart, never read.
        let settings = |n: usize| TlsConfig {
            extra_roots_pem: vec![b"root".to_vec(); n],
            client_identity: None,
        };
        let first = configs.get(&settings(0), false).unwrap();
        let second = configs.get(&settings(1), false).unwrap();
        for n in 2..MAX_CONFIGS {
            configs.get(&settings(n), false).unwrap();
        }
        // Using the first makes the second the oldest.
        configs.get(&settings(0), false).unwrap();
        configs.get(&settings(MAX_CONFIGS), false).unwrap();
        assert_eq!(configs.len(), MAX_CONFIGS);
        assert!(Arc::ptr_eq(
            &first,
            &configs.get(&settings(0), false).unwrap()
        ));
        assert!(!Arc::ptr_eq(
            &second,
            &configs.get(&settings(1), false).unwrap()
        ));
    }
}
