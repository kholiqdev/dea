//! A local HTTP server for network tests, over plain TCP or TLS with a
//! test CA. Every request it receives is recorded.
//!
//! Routes:
//! - `/status/{code}` answers with that status and no body.
//! - `/echo` answers 200 with the body it received, or with none for
//!   `/echo?quiet`, which also avoids a second copy of a large body.
//! - `/delay/{ms}` sleeps, then answers 200 with `x-slept-us` set to how long
//!   it really slept, in microseconds.
//! - `/cookies/set?name=value&Attribute` answers 200 with that `Set-Cookie`
//!   (path `/` unless given); `/cookies/set-later/{ms}?...` does so after `ms`.
//! - `/close` answers 200 without `Connection: close`, then closes the socket.
//! - `/hang-up` reads the request, then closes the socket without answering.
//! - `/redirect/{n}` answers 302 to `/redirect/{n-1}`, and 200 at 0.
//! - `/redirect-to?url=&status=` answers `status` (default 302) with
//!   `Location: url`.
//! - `/drip/{count}/{ms}` answers 200 at once, then sends `count` one-byte
//!   chunks, `ms` apart.
//! - `/stall-body` answers 200, sends one byte and then nothing.
//! - `/bytes/{n}` answers 200 with `n` bytes of `x`, generated while sending.
//! - `/throttle/{bytes_per_s}` reads the request body no faster than that,
//!   then answers 200 with the number of bytes it read. `0` never reads the
//!   body and never answers. The recorded body is empty.
//! - `/sse` answers a `text/event-stream` of three events 100 ms apart, with a
//!   comment first; the second has two data lines and an id.
//! - `/gzip-cut` answers the `/gzip` body without its last bytes.
//! - `/bomb/{mib}` answers `mib` MiB of zeros, gzipped, with
//!   `Content-Encoding: gzip`.
//! - `/gzip` and `/deflate` answer a short text in that `Content-Encoding`
//!   whatever the request asked for; `/brotli` claims `br` over plain bytes.
//!
//! [`TestProxy`] is a forward proxy that records what it is asked for.
//!
//! [`HoldServer`] is a raw TCP listener for tests that need a request to
//! hang.
//! - Anything else is a 404.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::future::Future;
use std::io::{self, Write as _};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt as _, Full};
use hyper::body::{Body, Frame, Incoming};
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
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, watch};
use tokio::task::AbortHandle;
use tokio::time::Sleep;
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
    /// HTTP/1.1 over TLS with a certificate from the test CA for the same
    /// names as `SelfSigned`, whose end date has passed.
    Expired,
    /// HTTP/1.1 over TLS with a certificate from the test CA for
    /// `other.test` only.
    OtherHost,
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
    /// The subject of the client certificate a `Mutual` server was shown,
    /// such as `CN=Dea Test Client`. `None` without one.
    pub client_subject: Option<String>,
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
        let http1 = || vec![b"http/1.1".to_vec()];
        let acceptor = match tls {
            Tls::None => None,
            Tls::SelfSigned { alpn } => Some(acceptor(alpn, false, server_identity())),
            Tls::Mutual { alpn } => Some(acceptor(alpn, true, server_identity())),
            Tls::Expired => Some(acceptor(http1(), false, expired_identity())),
            Tls::OtherHost => Some(acceptor(http1(), false, other_host_identity())),
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

    /// The port the server listens on, on loopback.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// How many connections the server has accepted.
    pub fn connections(&self) -> usize {
        self.shared.connections.load(Ordering::SeqCst)
    }

    /// Runs `f` on the requests received so far, without copying them. For
    /// tests with large bodies.
    pub fn with_requests<R>(&self, f: impl FnOnce(&[Received]) -> R) -> R {
        f(&self
            .shared
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner))
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
    IDENTITY.get_or_init(|| server_certificate(&SERVER_NAMES, set_validity))
}

/// Like the server certificate, but it ended yesterday.
fn expired_identity() -> &'static ServerIdentity {
    static IDENTITY: OnceLock<ServerIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        server_certificate(&SERVER_NAMES, |params| {
            let now = time::OffsetDateTime::now_utc();
            params.not_before = now - time::Duration::days(30);
            params.not_after = now - time::Duration::days(1);
        })
    })
}

/// A server certificate for `other.test` only.
fn other_host_identity() -> &'static ServerIdentity {
    static IDENTITY: OnceLock<ServerIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| server_certificate(&["other.test"], set_validity))
}

/// A server certificate for `names`, signed by the test CA. `dates` sets
/// its validity.
fn server_certificate(
    names: &[&str],
    dates: impl FnOnce(&mut CertificateParams),
) -> ServerIdentity {
    let names = names
        .iter()
        .map(|&name| name.to_owned())
        .collect::<Vec<_>>();
    let mut params = CertificateParams::new(names.clone()).expect("valid server names");
    params
        .distinguished_name
        .push(DnType::CommonName, names[0].as_str());
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    dates(&mut params);
    let key = KeyPair::generate().expect("generate the server key");
    let cert = params
        .signed_by(&key, &ca().issuer)
        .expect("sign the server certificate");
    ServerIdentity {
        cert: cert.der().clone(),
        key: key.serialize_der(),
    }
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

fn acceptor(alpn: Vec<Vec<u8>>, mutual: bool, identity: &ServerIdentity) -> TlsAcceptor {
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
        None => serve(stream, false, None, shared).await,
        Some(acceptor) => {
            let Ok(stream) = acceptor.accept(stream).await else {
                return;
            };
            let session = stream.get_ref().1;
            let h2 = session.alpn_protocol() == Some(b"h2");
            let subject = session
                .peer_certificates()
                .and_then(|certs| certs.first())
                .map(subject_of);
            serve(stream, h2, subject, shared).await;
        }
    }
}

/// How a handler asks for its connection to end.
#[derive(Default)]
struct Ending {
    /// Close once the response in flight is written.
    close: Notify,
    /// Close at once, without answering.
    hang_up: Notify,
}

async fn serve<S>(io: S, h2: bool, subject: Option<String>, shared: Arc<Shared>)
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let ending = Arc::new(Ending::default());
    let service = {
        let ending = Arc::clone(&ending);
        service_fn(move |request| {
            handle(
                request,
                subject.clone(),
                Arc::clone(&shared),
                Arc::clone(&ending),
            )
        })
    };
    let io = TokioIo::new(io);
    if h2 {
        let conn = http2::Builder::new(TokioExecutor::new()).serve_connection(io, service);
        drive(conn, &ending, http2::Connection::graceful_shutdown).await;
    } else {
        let conn = http1::Builder::new().serve_connection(io, service);
        drive(conn, &ending, http1::Connection::graceful_shutdown).await;
    }
}

/// Runs a connection until it ends, or ends it as a handler asks: after
/// the response in flight is written, or at once.
async fn drive<C, E>(conn: C, ending: &Ending, shut_down: impl FnOnce(Pin<&mut C>))
where
    C: Future<Output = Result<(), E>>,
{
    let mut conn = pin!(conn);
    tokio::select! {
        _ = conn.as_mut() => {}
        () = ending.close.notified() => {
            shut_down(conn.as_mut());
            let _ = conn.await;
        }
        // Dropping the connection closes the socket.
        () = ending.hang_up.notified() => {}
    }
}

/// The whole request body, copied once into the vector the record keeps.
async fn read_body(mut body: Incoming) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    while let Some(frame) = body.frame().await {
        if let Ok(chunk) = frame.ok()?.into_data() {
            data.extend_from_slice(&chunk);
        }
    }
    Some(data)
}

type Reply = Response<UnsyncBoxBody<Bytes, Infallible>>;

async fn handle(
    request: Request<Incoming>,
    client_subject: Option<String>,
    shared: Arc<Shared>,
    ending: Arc<Ending>,
) -> Result<Reply, Infallible> {
    let at = Instant::now();
    let (parts, body) = request.into_parts();
    let throttle = parts
        .uri
        .path()
        .strip_prefix("/throttle/")
        .map(str::parse::<u64>);
    let (body, answer) = match throttle {
        Some(Ok(rate)) => {
            let Some(read) = read_slowly(body, rate).await else {
                return Ok(reply(StatusCode::BAD_REQUEST, Bytes::new()));
            };
            (Vec::new(), Some(Bytes::from(read.to_string())))
        }
        Some(Err(_)) => return Ok(reply(StatusCode::BAD_REQUEST, Bytes::new())),
        None => match read_body(body).await {
            Some(body) => (body, None),
            None => return Ok(reply(StatusCode::BAD_REQUEST, Bytes::new())),
        },
    };
    let echo = if parts.uri.path() == "/echo" && parts.uri.query() != Some("quiet") {
        Bytes::copy_from_slice(&body)
    } else {
        Bytes::new()
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
        body,
        client_subject,
        at,
    };
    shared
        .requests
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(received);
    if let Some(answer) = answer {
        return Ok(reply(StatusCode::OK, answer));
    }
    Ok(route(parts.uri.path(), parts.uri.query(), echo, &ending).await)
}

/// Reads `body` no faster than `rate` bytes a second and returns how many
/// bytes it read. With `rate` 0 it never reads, and never returns.
async fn read_slowly(mut body: Incoming, rate: u64) -> Option<u64> {
    if rate == 0 {
        std::future::pending::<()>().await;
    }
    let started = tokio::time::Instant::now();
    let mut read = 0u64;
    while let Some(frame) = body.frame().await {
        if let Ok(chunk) = frame.ok()?.into_data() {
            read += chunk.len() as u64;
        }
        let due = Duration::from_secs_f64(read as f64 / rate as f64);
        tokio::time::sleep_until(started + due).await;
    }
    Some(read)
}

async fn route(path: &str, query: Option<&str>, echo: Bytes, ending: &Ending) -> Reply {
    if let Some(n) = path.strip_prefix("/redirect/") {
        let Ok(n) = n.parse::<u32>() else {
            return reply(StatusCode::BAD_REQUEST, Bytes::new());
        };
        if n == 0 {
            return reply(StatusCode::OK, Bytes::new());
        }
        return redirect(StatusCode::FOUND, &format!("/redirect/{}", n - 1));
    }
    if path == "/cookies/set" || path.starts_with("/cookies/set-later/") {
        if let Some(ms) = path.strip_prefix("/cookies/set-later/") {
            let Ok(ms) = ms.parse() else {
                return reply(StatusCode::BAD_REQUEST, Bytes::new());
            };
            tokio::time::sleep(Duration::from_millis(ms)).await;
        }
        // `?name=value&Attribute&Attribute=value`: one Set-Cookie header.
        // The path is `/` unless the query says otherwise, so the cookie
        // reaches every other route.
        let mut parts: Vec<&str> = query.unwrap_or_default().split('&').collect();
        if !parts
            .iter()
            .skip(1)
            .any(|p| p.to_ascii_lowercase().starts_with("path="))
        {
            parts.push("Path=/");
        }
        return set_cookie(reply(StatusCode::OK, Bytes::new()), &parts.join("; "));
    }
    if path == "/cookies/redirect" {
        let response = redirect(StatusCode::FOUND, "/echo");
        return set_cookie(response, "hop=1; Path=/");
    }
    if path == "/redirect-to" {
        let param = |name: &str| {
            url::form_urlencoded::parse(query.unwrap_or_default().as_bytes())
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        let status = match param("status") {
            Some(code) => code
                .parse()
                .ok()
                .and_then(|code| StatusCode::from_u16(code).ok()),
            None => Some(StatusCode::FOUND),
        };
        return match (status, param("url")) {
            (Some(status), Some(url)) => redirect(status, &url),
            _ => reply(StatusCode::BAD_REQUEST, Bytes::new()),
        };
    }
    if let Some(rest) = path.strip_prefix("/drip/") {
        let parsed = rest
            .split_once('/')
            .and_then(|(count, ms)| Some((count.parse::<usize>().ok()?, ms.parse::<u64>().ok()?)));
        let Some((count, ms)) = parsed else {
            return reply(StatusCode::BAD_REQUEST, Bytes::new());
        };
        let gap = Duration::from_millis(ms);
        return streamed(
            (0..count)
                .map(|_| (gap, Bytes::from_static(b"x")))
                .collect(),
        );
    }
    if let Some(n) = path.strip_prefix("/bytes/") {
        let Ok(n) = n.parse::<u64>() else {
            return reply(StatusCode::BAD_REQUEST, Bytes::new());
        };
        return Response::new(Generated { left: n }.boxed_unsync());
    }
    if path == "/sse" {
        let part = |ms, text: &'static str| {
            (
                Duration::from_millis(ms),
                Bytes::from_static(text.as_bytes()),
            )
        };
        let mut response = streamed(VecDeque::from([
            part(0, ": a comment\n"),
            part(0, "event: tick\ndata: one\n\n"),
            part(100, "id: 2\ndata: line one\ndata: line two\n\n"),
            part(100, "data: three\n\n"),
        ]));
        response.headers_mut().insert(
            "content-type",
            HeaderValue::from_static("text/event-stream"),
        );
        return response;
    }
    if let Some(mib) = path.strip_prefix("/bomb/") {
        let Ok(mib) = mib.parse::<usize>() else {
            return reply(StatusCode::BAD_REQUEST, Bytes::new());
        };
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        let zeros = vec![0u8; 1024 * 1024];
        for _ in 0..mib {
            gzip.write_all(&zeros).expect("write to a vector");
        }
        let body = gzip.finish().expect("finish a vector");
        let mut response = reply(StatusCode::OK, Bytes::from(body));
        response
            .headers_mut()
            .insert("content-encoding", HeaderValue::from_static("gzip"));
        return response;
    }
    if let Some((encoding, body)) = encoded(path) {
        let mut response = reply(StatusCode::OK, Bytes::from_static(body));
        response
            .headers_mut()
            .insert("content-encoding", HeaderValue::from_static(encoding));
        return response;
    }
    if path == "/stall-body" {
        return streamed(VecDeque::from([
            (Duration::ZERO, Bytes::from_static(b"x")),
            (Duration::from_secs(3600), Bytes::from_static(b"y")),
        ]));
    }
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
        "/echo" => reply(StatusCode::OK, echo),
        "/close" => {
            ending.close.notify_one();
            reply(StatusCode::OK, Bytes::new())
        }
        "/hang-up" => {
            ending.hang_up.notify_one();
            std::future::pending().await
        }
        _ => reply(StatusCode::NOT_FOUND, Bytes::new()),
    }
}

/// The encoding and body of the `/gzip`, `/deflate` and `/brotli` routes.
fn encoded(path: &str) -> Option<(&'static str, &'static [u8])> {
    // "hello from gzip" and "hello from deflate" compressed offline; deflate
    // is zlib-wrapped, as HTTP defines it.
    const GZIP: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 2, 19, 203, 72, 205, 201, 201, 87, 72, 43, 202, 207, 85, 72,
        175, 202, 44, 0, 0, 100, 170, 142, 181, 15, 0, 0, 0,
    ];
    const DEFLATE: &[u8] = &[
        120, 156, 203, 72, 205, 201, 201, 87, 72, 43, 202, 207, 85, 72, 73, 77, 203, 73, 44, 73, 5,
        0, 64, 238, 6, 222,
    ];
    match path {
        "/gzip" => Some(("gzip", GZIP)),
        "/gzip-cut" => Some(("gzip", &GZIP[..GZIP.len() - 10])),
        "/deflate" => Some(("deflate", DEFLATE)),
        "/brotli" => Some(("br", b"not really brotli")),
        _ => None,
    }
}

fn reply(status: StatusCode, body: Bytes) -> Reply {
    let mut response = Response::new(Full::new(body).boxed_unsync());
    *response.status_mut() = status;
    response
}

fn redirect(status: StatusCode, location: &str) -> Reply {
    let mut response = reply(status, Bytes::new());
    match HeaderValue::from_str(location) {
        Ok(value) => {
            response.headers_mut().insert("location", value);
        }
        Err(_) => *response.status_mut() = StatusCode::BAD_REQUEST,
    }
    response
}

fn set_cookie(mut response: Reply, value: &str) -> Reply {
    match HeaderValue::from_str(value) {
        Ok(value) => {
            response.headers_mut().append("set-cookie", value);
        }
        Err(_) => *response.status_mut() = StatusCode::BAD_REQUEST,
    }
    response
}

fn streamed(schedule: VecDeque<(Duration, Bytes)>) -> Reply {
    Response::new(Paced::new(schedule).boxed_unsync())
}

/// A body of `left` bytes of `x`, made as it is polled.
struct Generated {
    left: u64,
}

impl Body for Generated {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        const CHUNK: usize = 64 * 1024;
        if self.left == 0 {
            return Poll::Ready(None);
        }
        let size = usize::try_from(self.left).map_or(CHUNK, |left| left.min(CHUNK));
        self.left -= size as u64;
        Poll::Ready(Some(Ok(Frame::data(Bytes::from(vec![b'x'; size])))))
    }
}

/// A body that sends each chunk after its own delay.
struct Paced {
    schedule: VecDeque<(Duration, Bytes)>,
    sleep: Option<Pin<Box<Sleep>>>,
}

impl Paced {
    fn new(schedule: VecDeque<(Duration, Bytes)>) -> Self {
        Paced {
            schedule,
            sleep: None,
        }
    }
}

impl Body for Paced {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        let this = &mut *self;
        let Some((delay, _)) = this.schedule.front() else {
            return Poll::Ready(None);
        };
        let sleep = this
            .sleep
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(*delay)));
        if sleep.as_mut().poll(cx).is_pending() {
            return Poll::Pending;
        }
        this.sleep = None;
        let chunk = this.schedule.pop_front().map(|(_, chunk)| chunk);
        Poll::Ready(chunk.map(|chunk| Ok(Frame::data(chunk))))
    }
}

/// The subject of a client certificate this module issued. Only
/// [`client_identity_pem`] makes client certificates, and the `Mutual`
/// server accepts nothing else, so a match on the DER is enough.
fn subject_of(cert: &CertificateDer<'_>) -> String {
    if cert.as_ref() == client_identity().cert.as_ref() {
        format!("CN={CLIENT_COMMON_NAME}")
    } else {
        "unknown".into()
    }
}

const CLIENT_COMMON_NAME: &str = "Dea Test Client";

struct ClientIdentity {
    cert: CertificateDer<'static>,
    cert_pem: String,
    key_pem: String,
}

fn client_identity() -> &'static ClientIdentity {
    static IDENTITY: OnceLock<ClientIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let mut params = CertificateParams::default();
        params
            .distinguished_name
            .push(DnType::CommonName, CLIENT_COMMON_NAME);
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        params.use_authority_key_identifier_extension = true;
        set_validity(&mut params);
        let key = KeyPair::generate().expect("generate the client key");
        let cert = params
            .signed_by(&key, &ca().issuer)
            .expect("sign the client certificate");
        ClientIdentity {
            cert: cert.der().clone(),
            cert_pem: cert.pem(),
            key_pem: key.serialize_pem(),
        }
    })
}

/// A client certificate and its private key in PEM, signed by the test CA
/// (so a `Mutual` server accepts it). The same for the whole process.
pub fn client_identity_pem() -> (Vec<u8>, Vec<u8>) {
    let identity = client_identity();
    (
        identity.cert_pem.as_bytes().to_vec(),
        identity.key_pem.as_bytes().to_vec(),
    )
}

/// A raw TCP listener that reads whatever the client sends and never
/// answers, then notes when the client closes the connection.
pub struct HoldServer {
    port: u16,
    closed: watch::Receiver<Option<Instant>>,
    accepting: AbortHandle,
}

impl HoldServer {
    /// Starts on a free 127.0.0.1 port. Must run inside a Tokio runtime.
    pub async fn start() -> HoldServer {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind 127.0.0.1");
        let port = listener.local_addr().expect("listener address").port();
        let (closed_tx, closed) = watch::channel(None);
        let accepting = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    continue;
                };
                let closed_tx = closed_tx.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    while matches!(stream.read(&mut buf).await, Ok(n) if n > 0) {}
                    closed_tx.send_modify(|at| {
                        at.get_or_insert_with(Instant::now);
                    });
                });
            }
        })
        .abort_handle();
        HoldServer {
            port,
            closed,
            accepting,
        }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    /// When the client closed its connection, waiting up to `limit` for it.
    /// `None` if it stayed open.
    pub async fn wait_closed(&self, limit: Duration) -> Option<Instant> {
        let mut closed = self.closed.clone();
        tokio::time::timeout(limit, closed.wait_for(Option::is_some))
            .await
            .ok()?
            .ok()?
            .as_ref()
            .copied()
    }
}

impl Drop for HoldServer {
    fn drop(&mut self) {
        self.accepting.abort();
    }
}

/// The host name [`TestProxy`] maps to 127.0.0.1, so tests can reach a local
/// server by a name that is not loopback. The test certificate covers it.
pub const PROXY_TARGET_HOST: &str = "dea-target.test";

/// What a [`TestProxy`] was asked for: a `CONNECT` or an absolute-form request.
#[derive(Debug, Clone)]
pub struct ProxyRequest {
    pub method: String,
    /// `host:port` for `CONNECT`, the full URL otherwise.
    pub target: String,
    /// Header names in lower case, in the order received.
    pub headers: Vec<(String, String)>,
}

impl ProxyRequest {
    /// The first value of header `name`, matched without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Default)]
struct ProxyShared {
    refuse_connect: bool,
    seen: Mutex<Vec<ProxyRequest>>,
}

/// An HTTP proxy on a free loopback port. `CONNECT` opens a tunnel and an
/// absolute-form request is forwarded, both to `dea-target.test:<port>` as
/// 127.0.0.1:<port>. It closes the connection after each forwarded request.
pub struct TestProxy {
    port: u16,
    shared: Arc<ProxyShared>,
    accepting: AbortHandle,
}

impl TestProxy {
    /// Must run inside a Tokio runtime.
    pub async fn start() -> TestProxy {
        Self::start_with(false).await
    }

    /// A proxy that answers every `CONNECT` with 403.
    pub async fn refusing_connect() -> TestProxy {
        Self::start_with(true).await
    }

    async fn start_with(refuse_connect: bool) -> TestProxy {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind 127.0.0.1");
        let port = listener.local_addr().expect("listener address").port();
        let shared = Arc::new(ProxyShared {
            refuse_connect,
            ..ProxyShared::default()
        });
        let state = Arc::clone(&shared);
        let accepting = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let _ = stream.set_nodelay(true);
                tokio::spawn(proxy_connection(stream, Arc::clone(&state)));
            }
        })
        .abort_handle();
        TestProxy {
            port,
            shared,
            accepting,
        }
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// `http://<userinfo>@127.0.0.1:<port>`.
    pub fn url_with_userinfo(&self, userinfo: &str) -> String {
        format!("http://{userinfo}@127.0.0.1:{}", self.port)
    }

    /// The target of every `CONNECT` received, oldest first.
    pub fn connects(&self) -> Vec<String> {
        self.requests("CONNECT")
    }

    /// The URL of every absolute-form request received, oldest first.
    pub fn forwarded(&self) -> Vec<String> {
        self.seen()
            .into_iter()
            .filter(|r| r.method != "CONNECT")
            .map(|r| r.target)
            .collect()
    }

    /// Everything received, oldest first.
    pub fn seen(&self) -> Vec<ProxyRequest> {
        self.shared
            .seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn requests(&self, method: &str) -> Vec<String> {
        self.seen()
            .into_iter()
            .filter(|r| r.method == method)
            .map(|r| r.target)
            .collect()
    }
}

impl Drop for TestProxy {
    fn drop(&mut self) {
        self.accepting.abort();
    }
}

/// The request head, read byte by byte so nothing of a tunnel is consumed.
async fn read_head(stream: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 || stream.read(&mut byte).await.ok()? == 0 {
            return None;
        }
        head.push(byte[0]);
    }
    String::from_utf8(head).ok()
}

/// `host:port` with the test name mapped to loopback.
fn upstream(host: &str, port: u16) -> (String, u16) {
    if host.eq_ignore_ascii_case(PROXY_TARGET_HOST) {
        ("127.0.0.1".into(), port)
    } else {
        (host.into(), port)
    }
}

async fn proxy_connection(mut client: TcpStream, shared: Arc<ProxyShared>) {
    let Some(head) = read_head(&mut client).await else {
        return;
    };
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (
        request_line.next().unwrap_or_default().to_owned(),
        request_line.next().unwrap_or_default().to_owned(),
    );
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    shared
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(ProxyRequest {
            method: method.clone(),
            target: target.clone(),
            headers: headers.clone(),
        });

    if method == "CONNECT" {
        if shared.refuse_connect {
            let _ = client
                .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n")
                .await;
            return;
        }
        let Some((host, port)) = target
            .rsplit_once(':')
            .and_then(|(h, p)| Some((h, p.parse::<u16>().ok()?)))
        else {
            return;
        };
        let Ok(mut server) = TcpStream::connect(upstream(host, port)).await else {
            let _ = client
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
                .await;
            return;
        };
        if client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .is_err()
        {
            return;
        }
        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
        return;
    }

    // An absolute-form request: forward it in origin form and relay the
    // answer until the server closes.
    let Ok(url) = url::Url::parse(&target) else {
        let _ = client
            .write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n")
            .await;
        return;
    };
    let length = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    if client.read_exact(&mut body).await.is_err() {
        return;
    }
    let host = url.host_str().unwrap_or_default();
    let port = url.port_or_known_default().unwrap_or(80);
    let Ok(mut server) = TcpStream::connect(upstream(host, port)).await else {
        let _ = client
            .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
            .await;
        return;
    };
    let mut path = url.path().to_owned();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let mut forwarded = format!("{method} {path} HTTP/1.1\r\nconnection: close\r\n");
    for (name, value) in &headers {
        if !name.starts_with("proxy-") && name != "connection" {
            forwarded.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    forwarded.push_str("\r\n");
    if server.write_all(forwarded.as_bytes()).await.is_err()
        || server.write_all(&body).await.is_err()
    {
        return;
    }
    let _ = tokio::io::copy(&mut server, &mut client).await;
}
