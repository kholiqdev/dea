//! Opening a connection: DNS, TCP connect, TLS and the HTTP handshake,
//! each phase timed.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use hyper::body::Incoming;
use hyper::client::conn::{http1, http2};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf};
use tokio::net::TcpStream;
use tokio::task::AbortHandle;
use url::{Host, Url};

use crate::body::WireBody;
use crate::error::{HttpError, describe};
use crate::proxy;
use crate::request::TlsConfig;
use crate::response::HttpVersion;
use crate::timing::Marks;
use crate::tls;

/// How often an HTTP/2 connection is pinged, and how long the answer may
/// take, so a connection that died silently (sleep, VPN switch) is closed
/// within about 40 s instead of hanging every request. Pings also run
/// while no request is in flight.
const H2_PING_INTERVAL: Duration = Duration::from_secs(30);
const H2_PING_TIMEOUT: Duration = Duration::from_secs(10);

/// An HTTP connection whose I/O runs in its own task.
pub(crate) struct Connection {
    sender: Sender,
    /// Bytes read from the server so far, after TLS.
    received: Arc<AtomicU64>,
    /// Stops the task of an HTTP/1.1 connection when it is dropped, so the
    /// socket closes at once (a cancelled request, a timeout). `None` for
    /// HTTP/2, whose handles share one connection.
    task: Option<AbortHandle>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

enum Sender {
    Http1(http1::SendRequest<WireBody>),
    Http2(http2::SendRequest<WireBody>),
}

impl Connection {
    pub(crate) fn version(&self) -> HttpVersion {
        match self.sender {
            Sender::Http1(_) => HttpVersion::Http11,
            Sender::Http2(_) => HttpVersion::Http2,
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match &self.sender {
            Sender::Http1(sender) => sender.is_closed(),
            Sender::Http2(sender) => sender.is_closed(),
        }
    }

    /// Another handle to an HTTP/2 connection, which carries many requests
    /// at once. `None` for HTTP/1.1.
    pub(crate) fn share(&self) -> Option<Connection> {
        match &self.sender {
            Sender::Http1(_) => None,
            Sender::Http2(sender) => Some(Connection {
                sender: Sender::Http2(sender.clone()),
                received: Arc::clone(&self.received),
                task: None,
            }),
        }
    }

    /// Whether both handles belong to the same underlying connection.
    pub(crate) fn same_connection(&self, other: &Connection) -> bool {
        Arc::ptr_eq(&self.received, &other.received)
    }

    pub(crate) fn bytes_received(&self) -> u64 {
        self.received.load(Ordering::SeqCst)
    }

    /// Waits until the connection can take a request, sends it and returns
    /// the response head.
    pub(crate) async fn send(
        &mut self,
        request: hyper::Request<WireBody>,
    ) -> hyper::Result<hyper::Response<Incoming>> {
        match &mut self.sender {
            Sender::Http1(sender) => {
                sender.ready().await?;
                sender.send_request(request).await
            }
            Sender::Http2(sender) => {
                sender.ready().await?;
                sender.send_request(request).await
            }
        }
    }
}

/// Opens a new connection to the origin of `url`, through `proxy` when
/// there is one. Speaks HTTP/2 only when the server picks it with ALPN. It
/// has no time limit of its own: the caller runs it as a task it can
/// abandon.
///
/// Through a proxy, DNS and connect are the proxy's, and the connect phase
/// also holds the `CONNECT` exchange of an `https` target. TLS is the
/// origin's, inside the tunnel.
pub(crate) async fn open(
    url: &Url,
    proxy: Option<&Url>,
    tls: &TlsConfig,
    verify: bool,
) -> Result<(Connection, Marks), HttpError> {
    let start = Instant::now();
    let host = url
        .host()
        .ok_or_else(|| HttpError::InvalidRequest("the URL has no host".into()))?
        .to_owned();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| HttpError::InvalidRequest("the URL has no port".into()))?;

    let (dialed, dialed_port) = match proxy {
        Some(proxy) => (
            proxy
                .host()
                .ok_or_else(|| HttpError::Proxy("the proxy URL has no host".into()))?
                .to_owned(),
            proxy.port_or_known_default().unwrap_or(80),
        ),
        None => (host.clone(), port),
    };
    let reach = |e: HttpError| match proxy {
        Some(proxy) => HttpError::Proxy(format!("could not reach {}: {e}", proxy::shown(proxy))),
        None => e,
    };
    let addrs = resolve(&dialed, dialed_port).await.map_err(reach)?;
    // An IP literal skips DNS, so its DNS phase is zero, not the time
    // taken to wrap the address.
    let resolved = match dialed {
        Host::Domain(_) => Instant::now(),
        Host::Ipv4(_) | Host::Ipv6(_) => start,
    };
    let mut stream = connect(&addrs).await.map_err(reach)?;
    if let (Some(proxy), "https") = (proxy, url.scheme()) {
        tunnel(&mut stream, proxy, &host, port).await?;
    }
    let connected = Instant::now();
    let (connection, secured) = if url.scheme() == "https" {
        let (stream, h2) = tls::handshake(stream, &host, tls, verify).await?;
        let secured = Instant::now();
        (handshake(stream, h2).await?, secured)
    } else {
        (handshake(stream, false).await?, connected)
    };
    let marks = Marks {
        start,
        resolved,
        connected,
        secured,
        reused: false,
    };
    Ok((connection, marks))
}

/// The longest answer to `CONNECT` read before giving up.
const MAX_CONNECT_ANSWER: usize = 16 * 1024;

/// Asks `proxy` over `stream` for a tunnel to `host:port`. Reads the answer
/// byte by byte so nothing sent through the tunnel is consumed.
async fn tunnel(
    stream: &mut TcpStream,
    proxy: &Url,
    host: &Host<String>,
    port: u16,
) -> Result<(), HttpError> {
    let target = match host {
        Host::Ipv6(ip) => format!("[{ip}]:{port}"),
        _ => format!("{host}:{port}"),
    };
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(value) = proxy::authorization(proxy) {
        request.push_str("Proxy-Authorization: ");
        request.push_str(&value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    let proxy_error = |what: &str| HttpError::Proxy(format!("{} {what}", proxy::shown(proxy)));
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| proxy_error(&format!("did not take the CONNECT request: {e}")))?;

    let mut answer = Vec::new();
    let mut byte = [0u8; 1];
    while !answer.ends_with(b"\r\n\r\n") {
        if answer.len() >= MAX_CONNECT_ANSWER {
            return Err(proxy_error("sent an oversized answer to CONNECT"));
        }
        match stream.read(&mut byte).await {
            Ok(0) => {
                return Err(proxy_error(
                    "closed the connection before answering CONNECT",
                ));
            }
            Ok(_) => answer.push(byte[0]),
            Err(e) => return Err(proxy_error(&format!("failed while answering CONNECT: {e}"))),
        }
    }
    let status = std::str::from_utf8(&answer)
        .ok()
        .and_then(|text| text.lines().next())
        .filter(|line| line.starts_with("HTTP/1."))
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| proxy_error("did not answer CONNECT with an HTTP status"))?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(HttpError::Proxy(format!(
            "{} refused CONNECT to {target} with status {status}",
            proxy::shown(proxy)
        )))
    }
}

/// The addresses of `host`. An IP literal skips DNS.
async fn resolve(host: &Host<String>, port: u16) -> Result<Vec<SocketAddr>, HttpError> {
    let name = match host {
        Host::Ipv4(ip) => return Ok(vec![SocketAddr::from((*ip, port))]),
        Host::Ipv6(ip) => return Ok(vec![SocketAddr::from((*ip, port))]),
        Host::Domain(name) => name,
    };
    let dns_error = |message: String| HttpError::Dns {
        host: name.clone(),
        message,
    };
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), port))
        .await
        .map_err(|e| dns_error(e.to_string()))?
        .collect();
    if addrs.is_empty() {
        return Err(dns_error("no addresses found".into()));
    }
    Ok(addrs)
}

/// Tries each address in order and reports the last failure.
async fn connect(addrs: &[SocketAddr]) -> Result<TcpStream, HttpError> {
    let mut failure = HttpError::InvalidRequest("no address to connect to".into());
    for &addr in addrs {
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                // Requests are written in more than one piece; without
                // this, Nagle's algorithm holds the last one back.
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                failure = HttpError::ConnectionRefused { addr };
            }
            Err(e) => {
                failure = HttpError::Connect {
                    addr,
                    message: e.to_string(),
                };
            }
        }
    }
    Err(failure)
}

/// Starts HTTP/1.1 or HTTP/2 over `stream` and spawns the task that drives
/// the connection.
async fn handshake<S>(stream: S, h2: bool) -> Result<Connection, HttpError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let received = Arc::new(AtomicU64::new(0));
    let io = TokioIo::new(Counted {
        inner: stream,
        received: Arc::clone(&received),
    });
    let protocol = |e: hyper::Error| HttpError::Protocol(describe(&e));
    let (sender, task) = if h2 {
        let (sender, conn) = http2::Builder::new(TokioExecutor::new())
            .timer(TokioTimer::new())
            .keep_alive_interval(H2_PING_INTERVAL)
            .keep_alive_timeout(H2_PING_TIMEOUT)
            .keep_alive_while_idle(true)
            .handshake(io)
            .await
            .map_err(protocol)?;
        tokio::spawn(conn);
        (Sender::Http2(sender), None)
    } else {
        let (sender, conn) = http1::handshake(io).await.map_err(protocol)?;
        let task = tokio::spawn(conn).abort_handle();
        (Sender::Http1(sender), Some(task))
    };
    Ok(Connection {
        sender,
        received,
        task,
    })
}

/// Counts the bytes read, so a failed request on a pooled connection can
/// tell whether any of the response arrived before it is retried.
struct Counted<S> {
    inner: S,
    received: Arc<AtomicU64>,
}

impl<S: AsyncRead + Unpin> AsyncRead for Counted<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let poll = Pin::new(&mut this.inner).poll_read(cx, buf);
        let read = buf.filled().len() - before;
        if read > 0 {
            this.received.fetch_add(read as u64, Ordering::SeqCst);
        }
        poll
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Counted<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
