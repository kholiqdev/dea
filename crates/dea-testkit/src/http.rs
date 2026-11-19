//! A local HTTP server for network tests, over plain TCP or TLS with a
//! test CA. Every request it receives is recorded.
//!
//! Routes:
//! - `/status/{code}` answers with that status and no body.
//! - `/echo` answers 200 with the body it received.
//! - `/delay/{ms}` sleeps, then answers 200 with `x-slept-us` set to how long
//!   it really slept, in microseconds.
//! - `/close` answers 200 without `Connection: close`, then closes the socket.
//! - Anything else is a 404.

use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt as _, Collected, Full};
use hyper::body::Incoming;
use hyper::header::HeaderValue;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::RootCertStore;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ServerConfig, WebPkiClientVerifier};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio::task::AbortHandle;
use tokio_rustls::TlsAcceptor;

/// Names the test server certificate is valid for.
const SERVER_NAMES: [&str; 3] = ["localhost", "127.0.0.1", "dea-target.test"];

/// How the server talks to clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tls {
    /// Plain TCP, HTTP/1.1 only.
    None,
    /// TLS with a certificate from the test CA, offering `alpn` in order.
    SelfSigned { alpn: Vec<Vec<u8>> },
    /// Like `SelfSigned`, and the client must present a certificate signed
    /// by the test CA.
    Mutual { alpn: Vec<Vec<u8>> },
}

/// A request as the server received it.
#[derive(Debug, Clone)]
pub struct Received {
    pub method: String,
    pub path_and_query: String,
    /// The `:authority` of an HTTP/2 request. HTTP/1.1 requests carry a
    /// `Host` header instead, and have `None` here.
    pub authority: Option<String>,
    /// Header names in lower case, in the order received.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// When the request head reached the server.
    pub at: Instant,
}

impl Received {
    /// The first value of header `name`, matched without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A running test server. It stops accepting connections when dropped.
pub struct TestServer {
    port: u16,
    tls: bool,
    shared: Arc<Shared>,
    accepting: Vec<AbortHandle>,
}

#[derive(Default)]
struct Shared {
    connections: AtomicUsize,
    requests: Mutex<Vec<Received>>,
}

impl TestServer {
    /// Starts a server on a free loopback port. Must run inside a Tokio
    /// runtime.
    pub async fn start(tls: Tls) -> TestServer {
        let acceptor = match tls {
            Tls::None => None,
            Tls::SelfSigned { alpn } => Some(acceptor(alpn, false)),
            Tls::Mutual { alpn } => Some(acceptor(alpn, true)),
        };
        let (listeners, port) = bind_loopback().await;
        let shared = Arc::new(Shared::default());
        let accepting = listeners
            .into_iter()
            .map(|listener| {
                tokio::spawn(accept_loop(listener, acceptor.clone(), Arc::clone(&shared)))
                    .abort_handle()
            })
            .collect();
        TestServer {
            port,
            tls: acceptor.is_some(),
            shared,
            accepting,
        }
    }

    /// `http://127.0.0.1:<port><path>`, or `https://localhost:<port><path>`
    /// for a TLS server.
    pub fn url(&self, path: &str) -> String {
        if self.tls {
            format!("https://localhost:{}{path}", self.port)
        } else {
            format!("http://127.0.0.1:{}{path}", self.port)
        }
    }

    /// The test CA certificate, the same for every server in this process.
    pub fn ca_pem(&self) -> Vec<u8> {
        ca().pem.as_bytes().to_vec()
    }

    /// How many connections the server has accepted.
    pub fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::SeqCst)
    }

    /// Every request received so far, oldest first.
    pub fn requests(&self) -> Vec<Received> {
        self.shared
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        for task in &self.accepting {
            task.abort();
        }
    }
}

struct TestCa {
    issuer: Issuer<'static, KeyPair>,
    der: CertificateDer<'static>,
    pem: String,
}

/// The process-wide test CA, so every server and client identity shares it.
fn ca() -> &'static TestCa {
    static CA: OnceLock<TestCa> = OnceLock::new();
    CA.get_or_init(|| {
        let mut params = CertificateParams::default();
        params
            .distinguished_name
            .push(DnType::CommonName, "Dea Test CA");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        set_validity(&mut params);
        let key = KeyPair::generate().expect("generate the test CA key");
        let cert = params.self_signed(&key).expect("sign the test CA");
        TestCa {
            der: cert.der().clone(),
            pem: cert.pem(),
            issuer: Issuer::new(params, key),
        }
    })
}

struct ServerIdentity {
    cert: CertificateDer<'static>,
    key: Vec<u8>,
}

/// The server certificate, signed by the test CA and made once per process.
fn server_identity() -> &'static ServerIdentity {
    static IDENTITY: OnceLock<ServerIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let names = SERVER_NAMES.map(String::from).to_vec();
        let mut params = CertificateParams::new(names).expect("valid server names");
        params
            .distinguished_name
            .push(DnType::CommonName, "localhost");
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.use_authority_key_identifier_extension = true;
        set_validity(&mut params);
        let key = KeyPair::generate().expect("generate the server key");
        let cert = params
            .signed_by(&key, &ca().issuer)
            .expect("sign the server certificate");
        ServerIdentity {
            cert: cert.der().clone(),
            key: key.serialize_der(),
        }
    })
}

/// Valid from yesterday for a year. macOS rejects server certificates valid
/// for more than 825 days, even from a private CA.
fn set_validity(params: &mut CertificateParams) {
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(365);
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn acceptor(alpn: Vec<Vec<u8>>, mutual: bool) -> TlsAcceptor {
    let identity = server_identity();
    let builder = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions");
    let builder = if mutual {
        let mut roots = RootCertStore::empty();
        roots.add(ca().der.clone()).expect("add the test CA");
        let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider())
            .build()
            .expect("build the client certificate verifier");
        builder.with_client_cert_verifier(verifier)
    } else {
        builder.with_no_client_auth()
    };
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone()));
    let mut config = builder
        .with_single_cert(vec![identity.cert.clone()], key)
        .expect("server certificate and key match");
    config.alpn_protocols = alpn;
    TlsAcceptor::from(Arc::new(config))
}

/// Binds 127.0.0.1 and, when the machine has it, ::1 on the same free port.
/// `localhost` may resolve to ::1 first; without a listener there, Windows
/// takes about two seconds to report the refused connection.
async fn bind_loopback() -> (Vec<TcpListener>, u16) {
    for _ in 0..10 {
        let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind 127.0.0.1");
        let port = v4.local_addr().expect("listener address").port();
        match TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await {
            Ok(v6) => return (vec![v4, v6], port),
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {}
            Err(_) => return (vec![v4], port),
        }
    }
    let v4 = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind 127.0.0.1");
    let port = v4.local_addr().expect("listener address").port();
    (vec![v4], port)
}

async fn accept_loop(listener: TcpListener, tls: Option<TlsAcceptor>, shared: Arc<Shared>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        shared.connections.fetch_add(1, Ordering::SeqCst);
        let _ = stream.set_nodelay(true);
        tokio::spawn(serve_tcp(stream, tls.clone(), Arc::clone(&shared)));
    }
}

async fn serve_tcp(stream: TcpStream, tls: Option<TlsAcceptor>, shared: Arc<Shared>) {
    match tls {
        None => serve(stream, false, shared).await,
        Some(acceptor) => {
            let Ok(stream) = acceptor.accept(stream).await else {
                return;
            };
            let h2 = stream.get_ref().1.alpn_protocol() == Some(b"h2");
            serve(stream, h2, shared).await;
        }
    }
}

async fn serve<S>(io: S, h2: bool, shared: Arc<Shared>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let close = Arc::new(Notify::new());
    let service = {
        let close = Arc::clone(&close);
        service_fn(move |request| handle(request, Arc::clone(&shared), Arc::clone(&close)))
    };
    let io = TokioIo::new(io);
    if h2 {
        let conn = http2::Builder::new(TokioExecutor::new()).serve_connection(io, service);
        drive(conn, &close, http2::Connection::graceful_shutdown).await;
    } else {
        let conn = http1::Builder::new().serve_connection(io, service);
        drive(conn, &close, http1::Connection::graceful_shutdown).await;
    }
}

/// Runs a connection until it ends, or shuts it down once a handler asks
/// for the socket to close. The response in flight is still written.
async fn drive<C, E>(conn: C, close: &Notify, shut_down: impl FnOnce(Pin<&mut C>))
where
    C: Future<Output = Result<(), E>>,
{
    let mut conn = pin!(conn);
    tokio::select! {
        _ = conn.as_mut() => {}
        () = close.notified() => {
            shut_down(conn.as_mut());
            let _ = conn.await;
        }
    }
}

type Reply = Response<Full<Bytes>>;

async fn handle(
    request: Request<Incoming>,
    shared: Arc<Shared>,
    close: Arc<Notify>,
) -> Result<Reply, Infallible> {
    let at = Instant::now();
    let (parts, body) = request.into_parts();
    let Ok(body) = body.collect().await.map(Collected::to_bytes) else {
        return Ok(reply(StatusCode::BAD_REQUEST, Bytes::new()));
    };
    let received = Received {
        method: parts.method.to_string(),
        path_and_query: parts
            .uri
            .path_and_query()
            .map_or("/", |p| p.as_str())
            .to_owned(),
        authority: parts.uri.authority().map(|a| a.as_str().to_owned()),
        headers: parts
            .headers
            .iter()
            .map(|(name, value)| {
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                (name.as_str().to_owned(), value)
            })
            .collect(),
        body: body.to_vec(),
        at,
    };
    shared
        .requests
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(received);
    Ok(route(parts.uri.path(), body, &close).await)
}

async fn route(path: &str, body: Bytes, close: &Notify) -> Reply {
    if let Some(code) = path.strip_prefix("/status/") {
        let status = code
            .parse()
            .ok()
            .and_then(|code| StatusCode::from_u16(code).ok());
        return reply(status.unwrap_or(StatusCode::BAD_REQUEST), Bytes::new());
    }
    if let Some(ms) = path.strip_prefix("/delay/") {
        let Ok(ms) = ms.parse() else {
            return reply(StatusCode::BAD_REQUEST, Bytes::new());
        };
        let started = Instant::now();
        tokio::time::sleep(Duration::from_millis(ms)).await;
        let slept = started.elapsed().as_micros().to_string();
        let mut response = reply(StatusCode::OK, Bytes::new());
        response.headers_mut().insert(
            "x-slept-us",
            HeaderValue::from_str(&slept).expect("digits are a valid header value"),
        );
        return response;
    }
    match path {
        "/echo" => reply(StatusCode::OK, body),
        "/close" => {
            close.notify_one();
            reply(StatusCode::OK, Bytes::new())
        }
        _ => reply(StatusCode::NOT_FOUND, Bytes::new()),
    }
}

fn reply(status: StatusCode, body: Bytes) -> Reply {
    let mut response = Response::new(Full::new(body));
    *response.status_mut() = status;
    response
}
