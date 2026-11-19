//! TLS for HTTPS connections: rustls with the ring provider, verified
//! against the operating system's trust store plus the request's extra CAs.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::pki_types::{PrivateKeyDer, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest as _, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use url::Host;

use crate::error::{HttpError, describe};
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

/// Runs the TLS handshake over `stream` and reports whether the server
/// chose HTTP/2.
pub(crate) async fn handshake(
    stream: TcpStream,
    host: &Host<String>,
    config: &TlsConfig,
    verify: bool,
) -> Result<(TlsStream<TcpStream>, bool), HttpError> {
    let connector = TlsConnector::from(Arc::new(client_config(config, verify)?));
    let server_name = match host {
        Host::Domain(name) => ServerName::try_from(name.clone())
            .map_err(|_| HttpError::Tls(format!("{name} is not a valid TLS server name")))?,
        Host::Ipv4(ip) => ServerName::from(*ip),
        Host::Ipv6(ip) => ServerName::from(*ip),
    };
    let stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| HttpError::Tls(describe(&e)))?;
    let h2 = stream.get_ref().1.alpn_protocol() == Some(b"h2");
    Ok((stream, h2))
}

fn client_config(config: &TlsConfig, verify: bool) -> Result<ClientConfig, HttpError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier: Arc<dyn ServerCertVerifier> = if verify {
        Arc::new(platform_verifier(config, &provider)?)
    } else {
        Arc::new(AcceptAny(provider.signature_verification_algorithms))
    };
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| HttpError::Tls(describe(&e)))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);
    let mut client = match &config.client_identity {
        Some(identity) => {
            let certs = CertificateDer::pem_slice_iter(&identity.certificate_pem)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    HttpError::Tls(format!("could not read the client certificate: {e}"))
                })?;
            // The key's own error text stays out of the message.
            let key = PrivateKeyDer::from_pem_slice(&identity.key_pem)
                .map_err(|_| HttpError::Tls("could not read the client private key".into()))?;
            builder
                .with_client_auth_cert(certs, key)
                .map_err(|e| HttpError::Tls(format!("the client certificate is not usable: {e}")))?
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
                HttpError::Tls(format!("could not read an extra CA certificate: {e}"))
            })?);
        }
    }
    rustls_platform_verifier::Verifier::new_with_extra_roots(roots, Arc::clone(provider))
        .map_err(|e| HttpError::Tls(describe(&e)))
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
