//! Opening a connection: DNS, TCP connect, TLS and the HTTP handshake,
//! each phase timed.

use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
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
use crate::error::{HttpError, TlsError, describe};
use crate::proxy;
use crate::request::TlsConfig;
use crate::response::HttpVersion;
use crate::timing::Marks;
use crate::tls::{self, TlsConfigs};

/// How often an HTTP/2 connection is pinged, and how long the answer may
/// take, so a connection that died silently (sleep, VPN switch) is closed
/// within about 40 s instead of hanging every request. Pings also run
/// while no request is in flight.
const H2_PING_INTERVAL: Duration = Duration::from_secs(30);
const H2_PING_TIMEOUT: Duration = Duration::from_secs(10);

/// An HTTP connection whose I/O runs in its own task.
pub(crate) struct Connection {
    sender: Sender,
    /// What the I/O has seen, shared by every handle to the connection.
    seen: Arc<Seen>,
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

/// What the I/O of a connection has seen.
#[derive(Default)]
struct Seen {
    /// Bytes read from the server so far, after TLS.
    received: AtomicU64,
    /// The TLS failure a read or write ended with. Under TLS 1.3 a server
    /// turns down the client certificate only after the handshake, so this
    /// is where that shows.
    tls_error: OnceLock<TlsError>,
    /// Whether the server asked for a client certificate in the handshake.
    asked_for_certificate: bool,
}

impl Seen {
    /// Keeps the TLS failure behind `error`, which a read or write of the
    /// connection ended with, if there is one.
    fn note(&self, error: &io::Error) {
        let received = self.received.load(Ordering::SeqCst);
        let tls = tls::tls_error(error)
            .or_else(|| tls::closed_after_asking(error, self.asked_for_certificate, received));
        if let Some(tls) = tls {
            let _ = self.tls_error.set(tls);
        }
    }

    /// `error` from hyper as a TLS error when a read failed on one, and as
    /// a protocol error otherwise.
    fn error(&self, error: &hyper::Error) -> HttpError {
        match self.tls_error.get() {
            Some(tls) => HttpError::Tls(tls.clone()),
            None => HttpError::Protocol(describe(error)),
        }
    }
}

/// A request that got no response head.
pub(crate) struct Unanswered {
    pub(crate) error: hyper::Error,
    /// Whether the connection took the whole request, head and body.
    pub(crate) written: bool,
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
                seen: Arc::clone(&self.seen),
                task: None,
            }),
        }
    }

    /// Whether both handles belong to the same underlying connection.
    pub(crate) fn same_connection(&self, other: &Connection) -> bool {
        Arc::ptr_eq(&self.seen, &other.seen)
    }

    pub(crate) fn bytes_received(&self) -> u64 {
        self.seen.received.load(Ordering::SeqCst)
    }

    /// `error`, which hyper reported for this connection, as Dea reports
    /// it.
    pub(crate) fn error(&self, error: &hyper::Error) -> HttpError {
        self.seen.error(error)
    }

    /// Waits until the connection can take a request, sends it and returns
    /// the response head.
    pub(crate) async fn send(
        &mut self,
        request: hyper::Request<WireBody>,
    ) -> Result<hyper::Response<Incoming>, Unanswered> {
        let body = request.body().progress();
        let unsent = |error| Unanswered {
            error,
            written: false,
        };
        let sent = match &mut self.sender {
            Sender::Http1(sender) => {
                sender.ready().await.map_err(unsent)?;
                sender.try_send_request(request).await
            }
            Sender::Http2(sender) => {
                sender.ready().await.map_err(unsent)?;
                sender.try_send_request(request).await
            }
        };
        sent.map_err(|mut failed| Unanswered {
            // hyper hands the request back when it wrote none of it.
            written: failed.take_message().is_none() && body.all_taken(),
            error: failed.into_error(),
        })
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
    configs: &TlsConfigs,
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
        let config = configs.get(tls, verify)?;
        let tls = tls::handshake(stream, &host, config).await?;
        let secured = Instant::now();
        let seen = Seen {
            asked_for_certificate: tls.asked_for_certificate,
            ..Seen::default()
        };
        (handshake(tls.stream, tls.h2, seen).await?, secured)
    } else {
        (handshake(stream, false, Seen::default()).await?, connected)
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

/// How long an attempt to connect runs alone before the next address is
/// tried beside it, as RFC 8305 recommends.
const ATTEMPT_DELAY: Duration = Duration::from_millis(250);

/// Connects to whichever of `addrs` answers first.
async fn connect(addrs: &[SocketAddr]) -> Result<TcpStream, HttpError> {
    let stream = race(addrs, TcpStream::connect).await?;
    // Requests are written in more than one piece; without this, Nagle's
    // algorithm holds the last one back.
    let _ = stream.set_nodelay(true);
    Ok(stream)
}

/// Happy Eyeballs (RFC 8305): the address families take turns, each
/// attempt runs alone for [`ATTEMPT_DELAY`] or until it fails before the
/// next one starts beside it, and the first connection wins. The other
/// attempts are dropped. On Windows `localhost` gives `::1` first, which
/// takes about two seconds to refuse; an IPv6 address that never answers
/// would take the whole timeout.
///
/// When every attempt fails: `ConnectionRefused` if all were refused,
/// otherwise `Connect` with the last other failure.
async fn race<F, A>(addrs: &[SocketAddr], attempt: F) -> Result<TcpStream, HttpError>
where
    F: Fn(SocketAddr) -> A,
    A: Future<Output = io::Result<TcpStream>> + Send,
{
    let mut waiting = interleave(addrs).into_iter();
    let mut running: Vec<(SocketAddr, Pin<Box<A>>)> = Vec::new();
    let mut next_start = tokio::time::Instant::now();
    let mut refused = None;
    let mut failed = None;
    loop {
        if running.is_empty() || tokio::time::Instant::now() >= next_start {
            match waiting.next() {
                Some(addr) => {
                    running.push((addr, Box::pin(attempt(addr))));
                    next_start = tokio::time::Instant::now() + ATTEMPT_DELAY;
                }
                None if running.is_empty() => break,
                None => {}
            }
        }
        let more = waiting.len() > 0;
        tokio::select! {
            (addr, result) = first_done(&mut running) => match result {
                Ok(stream) => return Ok(stream),
                Err(e) => {
                    if e.kind() == io::ErrorKind::ConnectionRefused {
                        refused = Some(addr);
                    } else {
                        failed = Some((addr, e.to_string()));
                    }
                    // The next address need not wait its turn.
                    next_start = tokio::time::Instant::now();
                }
            },
            () = tokio::time::sleep_until(next_start), if more => {}
        }
    }
    Err(match (failed, refused) {
        (Some((addr, message)), _) => HttpError::Connect {
            addr: addr.to_string(),
            message,
        },
        (None, Some(addr)) => HttpError::ConnectionRefused {
            addr: addr.to_string(),
        },
        (None, None) => HttpError::InvalidRequest("no address to connect to".into()),
    })
}

/// Waits for the first of `running` to finish and takes it out.
async fn first_done<A>(running: &mut Vec<(SocketAddr, Pin<Box<A>>)>) -> (SocketAddr, A::Output)
where
    A: Future,
{
    std::future::poll_fn(|cx| {
        for i in 0..running.len() {
            if let Poll::Ready(output) = running[i].1.as_mut().poll(cx) {
                let (addr, _) = running.swap_remove(i);
                return Poll::Ready((addr, output));
            }
        }
        Poll::Pending
    })
    .await
}

/// `addrs` with the families taking turns, starting with the family of the
/// first address; within a family the order stays.
fn interleave(addrs: &[SocketAddr]) -> Vec<SocketAddr> {
    let Some(first) = addrs.first() else {
        return Vec::new();
    };
    let (mut same, mut other): (VecDeque<SocketAddr>, VecDeque<SocketAddr>) = addrs
        .iter()
        .partition(|addr| addr.is_ipv4() == first.is_ipv4());
    let mut turns = Vec::with_capacity(addrs.len());
    while !same.is_empty() || !other.is_empty() {
        turns.extend(same.pop_front());
        turns.extend(other.pop_front());
    }
    turns
}

/// Starts HTTP/1.1 or HTTP/2 over `stream` and spawns the task that drives
/// the connection, which notes what it sees in `seen`.
async fn handshake<S>(stream: S, h2: bool, seen: Seen) -> Result<Connection, HttpError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let seen = Arc::new(seen);
    let io = TokioIo::new(Counted {
        inner: stream,
        seen: Arc::clone(&seen),
    });
    let protocol = |e: hyper::Error| seen.error(&e);
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
    Ok(Connection { sender, seen, task })
}

/// Counts the bytes read, so a failed request on a pooled connection can
/// tell whether any of the response arrived before it is retried, and
/// keeps the TLS failure a read or write ended with. hyper keeps only the
/// text of an HTTP/2 connection's I/O error.
struct Counted<S> {
    inner: S,
    seen: Arc<Seen>,
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
            this.seen.received.fetch_add(read as u64, Ordering::SeqCst);
        }
        this.noted(poll)
    }
}

impl<S> Counted<S> {
    /// `poll`, after noting the TLS failure it ended with.
    fn noted<T>(&self, poll: Poll<io::Result<T>>) -> Poll<io::Result<T>> {
        if let Poll::Ready(Err(e)) = &poll {
            self.seen.note(e);
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
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_write(cx, buf);
        this.noted(poll)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_write_vectored(cx, bufs);
        this.noted(poll)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let poll = Pin::new(&mut this.inner).poll_flush(cx);
        this.noted(poll)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::atomic::AtomicBool;

    use super::*;
    use crate::error::TlsErrorKind;

    /// An address nothing listens on.
    fn refused() -> SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    }

    /// Sets its flag when dropped.
    struct Dropped(Arc<AtomicBool>);

    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Connects for real, except to `silent`, which never answers, and to
    /// `failing`, which fails at once with `kind`.
    fn attempts(
        silent: SocketAddr,
        failing: Option<(SocketAddr, io::ErrorKind)>,
        dropped: &Arc<AtomicBool>,
    ) -> impl Fn(SocketAddr) -> Pin<Box<dyn Future<Output = io::Result<TcpStream>> + Send>> {
        let dropped = Arc::clone(dropped);
        move |addr| {
            let dropped = Arc::clone(&dropped);
            Box::pin(async move {
                if addr == silent {
                    let _dropped = Dropped(dropped);
                    pending::<()>().await;
                }
                if let Some((failing, kind)) = failing
                    && addr == failing
                {
                    return Err(io::Error::from(kind));
                }
                TcpStream::connect(addr).await
            })
        }
    }

    #[tokio::test]
    async fn a_silent_address_gives_way_to_the_next() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let working = listener.local_addr().unwrap();
        let silent: SocketAddr = "[::1]:9".parse().unwrap();
        let dropped = Arc::new(AtomicBool::new(false));

        let addrs = [silent, working];
        let started = Instant::now();
        let raced = race(&addrs, attempts(silent, None, &dropped));
        let stream = tokio::time::timeout(Duration::from_secs(2), raced)
            .await
            .expect("the next address was never tried")
            .unwrap();

        assert_eq!(stream.peer_addr().unwrap(), working);
        let elapsed = started.elapsed();
        assert!(
            elapsed >= ATTEMPT_DELAY && elapsed < Duration::from_secs(1),
            "{elapsed:?}"
        );
        // The losing attempt was dropped, not left running.
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_failed_attempt_starts_the_next_at_once() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let working = listener.local_addr().unwrap();
        let failing: SocketAddr = "[::1]:9".parse().unwrap();
        let unused: SocketAddr = "[::1]:7".parse().unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let failure = Some((failing, io::ErrorKind::ConnectionRefused));

        let started = Instant::now();
        let stream = race(&[failing, working], attempts(unused, failure, &dropped))
            .await
            .unwrap();

        assert_eq!(stream.peer_addr().unwrap(), working);
        assert!(started.elapsed() < ATTEMPT_DELAY, "{:?}", started.elapsed());
    }

    #[tokio::test]
    async fn a_refused_address_gives_way_to_the_next() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let working = listener.local_addr().unwrap();
        let started = Instant::now();
        let stream = connect(&[refused(), working]).await.unwrap();
        assert_eq!(stream.peer_addr().unwrap(), working);
        // Windows takes about two seconds to report a refused connection.
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn every_address_refused_is_connection_refused() {
        let result = connect(&[refused(), refused()]).await;
        assert!(
            matches!(result, Err(HttpError::ConnectionRefused { .. })),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn any_other_failure_is_a_connect_error() {
        let (timed_out, refused) = (refused(), refused());
        let dropped = Arc::new(AtomicBool::new(false));
        let unused: SocketAddr = "[::1]:7".parse().unwrap();
        let failure = Some((timed_out, io::ErrorKind::TimedOut));
        for addrs in [[timed_out, refused], [refused, timed_out]] {
            let result = race(&addrs, attempts(unused, failure, &dropped)).await;
            assert!(
                matches!(&result, Err(HttpError::Connect { addr, .. }) if *addr == timed_out.to_string()),
                "{result:?}"
            );
        }
    }

    /// Fails every read and write with `kind`.
    struct Failing(io::ErrorKind);

    impl AsyncRead for Failing {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::from(self.0)))
        }
    }

    impl AsyncWrite for Failing {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::Error::from(self.0)))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::from(self.0)))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn a_connection_cut_after_the_server_asked_for_a_certificate_is_a_rejection() {
        for (asked, write) in [(true, false), (true, true), (false, false), (false, true)] {
            let seen = Arc::new(Seen {
                asked_for_certificate: asked,
                ..Seen::default()
            });
            let mut counted = Counted {
                inner: Failing(io::ErrorKind::ConnectionAborted),
                seen: Arc::clone(&seen),
            };
            let failed = if write {
                counted.write_all(b"GET / HTTP/1.1\r\n").await
            } else {
                counted.read(&mut [0; 8]).await.map(drop)
            };
            assert!(failed.is_err());
            let kind = seen.tls_error.get().map(|tls| tls.kind);
            let expected = asked.then_some(TlsErrorKind::ClientCertificateRejected);
            assert_eq!(kind, expected, "asked {asked}, write {write}");
        }
    }

    #[test]
    fn address_families_take_turns() {
        let addr = |text: &str| text.parse::<SocketAddr>().unwrap();
        let (a6, b6, c6) = (addr("[::1]:1"), addr("[::2]:1"), addr("[::3]:1"));
        let (a4, b4) = (addr("10.0.0.1:1"), addr("10.0.0.2:1"));
        assert_eq!(interleave(&[a6, b6, c6, a4, b4]), [a6, a4, b6, b4, c6]);
        assert_eq!(interleave(&[a4, a6, b6]), [a4, a6, b6]);
        assert_eq!(interleave(&[a4, b4]), [a4, b4]);
        assert!(interleave(&[]).is_empty());
    }
}
