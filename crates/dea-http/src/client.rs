//! The client: sends a request over a pooled or a new connection and reads
//! the response.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::BodyExt as _;
use hyper::header::HeaderValue;
use tokio_util::sync::CancellationToken;

use crate::connect::{self, Connection};
use crate::cookies::CookieJar;
use crate::error::{HttpError, describe};
use crate::pool::{Pool, PoolKey};
use crate::reader::{BodyReader, Encoding, ReaderConfig};
use crate::redirect::next_hop;
use crate::request::{Options, Prepared, Request};
use crate::response::{HttpVersion, Redirect, Response};
use crate::sse::SseEvent;
use crate::timing::Marks;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub user_agent: String,
    /// Where large response bodies are written.
    pub spill_dir: PathBuf,
    pub proxy: ProxyConfig,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            user_agent: concat!("Dea/", env!("CARGO_PKG_VERSION")).into(),
            spill_dir: std::env::temp_dir(),
            proxy: ProxyConfig::None,
        }
    }
}

/// How requests reach the network.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxyConfig {
    /// Straight to the server.
    #[default]
    None,
}

/// Sends requests. Cheap to clone; clones share one connection pool.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    user_agent: HeaderValue,
    spill_dir: PathBuf,
    pool: Pool,
}

/// What a single send may use besides the request.
pub struct SendContext<'a> {
    cancel: &'a CancellationToken,
    events: Option<&'a EventSink<'a>>,
    cookies: Option<&'a CookieJar>,
}

/// Receives the events of the response being read.
pub(crate) type EventSink<'a> = dyn Fn(Event) + Send + Sync + 'a;

/// What `Client::send` reports while the response of the final hop is
/// read, ahead of the finished `Response`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The response head arrived.
    Head {
        status: u16,
        headers: Vec<(String, String)>,
    },
    /// One piece of the body, decoded when the body is, as it arrived.
    Data(Bytes),
    /// A complete event of a `text/event-stream` body.
    Sse(SseEvent),
}

impl<'a> SendContext<'a> {
    pub fn new(cancel: &'a CancellationToken) -> Self {
        SendContext {
            cancel,
            events: None,
            cookies: None,
        }
    }

    /// Calls `sink` for every event of the response. A hop that is
    /// followed as a redirect sends none.
    #[must_use]
    pub fn events(mut self, sink: &'a EventSink<'a>) -> Self {
        self.events = Some(sink);
        self
    }

    /// Sends the cookies of `jar` that match each hop, and keeps the ones
    /// each hop sets. Without a jar no cookie is sent or kept.
    #[must_use]
    pub fn cookies(mut self, jar: &'a CookieJar) -> Self {
        self.cookies = Some(jar);
        self
    }
}

/// What one send reports to, and when it began.
#[derive(Clone, Copy)]
struct Run<'a> {
    events: Option<&'a EventSink<'a>>,
    cookies: Option<&'a CookieJar>,
    started: Instant,
}

/// What one attempt over a connection needs besides the connection.
#[derive(Clone, Copy)]
struct Attempt<'a> {
    prepared: &'a Prepared,
    key: &'a PoolKey,
    options: &'a Options,
    /// The request carries an `Accept-Encoding` header.
    asked_for_encoding: bool,
    run: Run<'a>,
    /// When the response head must have arrived.
    head_deadline: tokio::time::Instant,
}

/// The first non-empty value of header `name`.
fn header<'h>(headers: &'h [(String, String)], name: &str) -> Option<&'h str> {
    headers
        .iter()
        .find(|(n, value)| n.eq_ignore_ascii_case(name) && !value.is_empty())
        .map(|(_, value)| value.as_str())
}

/// `timeout` from now; effectively never for a timeout too big to add.
fn deadline(timeout: Duration) -> tokio::time::Instant {
    let now = tokio::time::Instant::now();
    now.checked_add(timeout)
        .unwrap_or_else(|| now + Duration::from_secs(86_400 * 365 * 30))
}

/// Opens a connection in its own task, which is stopped if this future is
/// dropped or the deadline passes. rustls checks certificates with the
/// operating system synchronously, which can block a worker thread for
/// seconds. Running that in a task of its own keeps the caller's cancel
/// and timeout able to answer from another worker.
async fn open(
    request: &Request,
    deadline: tokio::time::Instant,
) -> Result<(Connection, Marks), HttpError> {
    let (url, tls, verify) = (
        request.url.clone(),
        request.tls.clone(),
        request.options.verify_tls,
    );
    let task = tokio::spawn(async move { connect::open(&url, &tls, verify).await });
    let _stop = StopOnDrop(task.abort_handle());
    match tokio::time::timeout_at(deadline, task).await {
        Err(_) => Err(HttpError::Timeout(request.options.timeout)),
        Ok(Ok(opened)) => opened,
        Ok(Err(e)) => Err(HttpError::Protocol(format!(
            "the connection task failed: {e}"
        ))),
    }
}

struct StopOnDrop(tokio::task::AbortHandle);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A failed exchange, and whether it may be retried on a new connection.
struct Failure {
    error: HttpError,
    retry: bool,
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Client, HttpError> {
        let user_agent = HeaderValue::from_str(&config.user_agent).map_err(|_| {
            HttpError::InvalidRequest("the user agent has a control character".into())
        })?;
        Ok(Client {
            inner: Arc::new(Inner {
                user_agent,
                spill_dir: config.spill_dir,
                pool: Pool::default(),
            }),
        })
    }

    /// Sends `request`, follows redirects when the options say so, and
    /// reads the whole response. Cancelling the token ends the send with
    /// `Cancelled` and closes the connection in use.
    ///
    /// `Options::timeout` limits each hop until its response head arrives,
    /// then the gap between pieces of its body (decision 9). A pooled
    /// connection that fails before any of the response arrives (the
    /// server closed it while idle) is replaced by a new one, once.
    pub async fn send(
        &self,
        request: Request,
        ctx: SendContext<'_>,
    ) -> Result<Response, HttpError> {
        tokio::select! {
            biased;
            () = ctx.cancel.cancelled() => Err(HttpError::Cancelled),
            result = self.follow(request, Run { events: ctx.events, cookies: ctx.cookies, started: Instant::now() }) => result,
        }
    }

    /// Sends hops until one is not a redirect we follow.
    async fn follow(&self, request: Request, run: Run<'_>) -> Result<Response, HttpError> {
        let mut current = request;
        let mut redirects = Vec::new();
        loop {
            // The jar's cookies go on a copy, so they are matched again
            // for every hop and never carried into the next one.
            let mut response = match run.cookies {
                Some(jar) => {
                    let response = self.hop(&jar.apply(&current), run).await?;
                    jar.absorb(&current.url, &response.headers);
                    response
                }
                None => self.hop(&current, run).await?,
            };
            let next = if current.options.follow_redirects {
                match header(&response.headers, "location") {
                    Some(location) => next_hop(&current, response.status, location)?,
                    None => None,
                }
            } else {
                None
            };
            let Some(next) = next else {
                response.redirects = redirects;
                return Ok(response);
            };
            let max = current.options.max_redirects;
            if redirects.len() >= max as usize {
                return Err(HttpError::TooManyRedirects(max));
            }
            redirects.push(Redirect {
                status: response.status,
                url: current.url.to_string(),
            });
            current = next;
        }
    }

    /// One request and its response, over a pooled or a new connection.
    async fn hop(&self, request: &Request, run: Run<'_>) -> Result<Response, HttpError> {
        let prepared = Prepared::new(request, &self.inner.user_agent)?;
        let key = PoolKey::new(request);
        let head_deadline = deadline(request.options.timeout);
        let asked_for_encoding = header(&request.headers, "accept-encoding").is_some();

        if let Some(connection) = self.inner.pool.checkout(&key) {
            let marks = Marks::reused(Instant::now());
            let attempt = Attempt {
                prepared: &prepared,
                key: &key,
                options: &request.options,
                asked_for_encoding,
                run,
                head_deadline,
            };
            match self.exchange(connection, marks, &attempt).await {
                Ok(response) => return Ok(response),
                Err(failure) if failure.retry => {}
                Err(failure) => return Err(failure.error),
            }
        }

        let (connection, marks) = open(request, head_deadline).await?;
        if let Some(shared) = connection.share() {
            self.inner.pool.checkin(key.clone(), shared);
        }
        let attempt = Attempt {
            prepared: &prepared,
            key: &key,
            options: &request.options,
            asked_for_encoding,
            run,
            head_deadline,
        };
        self.exchange(connection, marks, &attempt)
            .await
            .map_err(|failure| failure.error)
    }

    /// Sends one attempt over `connection` and reads the response. An
    /// HTTP/1.1 connection goes back to the pool once the body is read.
    async fn exchange(
        &self,
        mut connection: Connection,
        marks: Marks,
        attempt: &Attempt<'_>,
    ) -> Result<Response, Failure> {
        let Attempt {
            prepared,
            key,
            options,
            asked_for_encoding,
            run,
            head_deadline,
        } = *attempt;
        let timeout = options.timeout;
        let version = connection.version();
        let received_before = connection.bytes_received();
        let wire = prepared.to_wire(version).map_err(|error| Failure {
            error,
            retry: false,
        })?;
        let sent = tokio::time::timeout_at(head_deadline, connection.send(wire)).await;
        let head = match sent {
            Err(_) => {
                // A silent h2 connection would otherwise be picked again
                // and make the next request wait as long.
                if version == HttpVersion::Http2 {
                    self.inner.pool.evict(key, &connection);
                }
                return Err(Failure {
                    error: HttpError::Timeout(timeout),
                    retry: false,
                });
            }
            Ok(Ok(head)) => head,
            Ok(Err(e)) => {
                let nothing_arrived = connection.bytes_received() == received_before;
                return Err(Failure {
                    error: HttpError::Protocol(describe(&e)),
                    retry: marks.reused && nothing_arrived,
                });
            }
        };
        let head_at = Instant::now();
        let (parts, mut body) = head.into_parts();
        let status = parts.status.as_u16();
        let headers: Vec<(String, String)> = parts
            .headers
            .iter()
            .map(|(name, value)| {
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                (name.as_str().to_owned(), value)
            })
            .collect();

        // A hop we follow is not the response; it reports nothing.
        let followed = options.follow_redirects
            && matches!(status, 301 | 302 | 303 | 307 | 308)
            && header(&headers, "location").is_some();
        let events = if followed { None } else { run.events };
        if let Some(sink) = events {
            sink(Event::Head {
                status,
                headers: headers.clone(),
            });
        }
        let content_encoding = header(&headers, "content-encoding")
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.eq_ignore_ascii_case("identity"));
        let decode = content_encoding
            .as_deref()
            .and_then(Encoding::from_header)
            .filter(|_| asked_for_encoding);
        let sse = header(&headers, "content-type").is_some_and(|value| {
            value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("text/event-stream")
        });
        let mut reader = BodyReader::new(ReaderConfig {
            spill_dir: &self.inner.spill_dir,
            max_body_bytes: options.max_body_bytes,
            decode,
            sse,
            events,
            started: run.started,
        });
        // From here the timeout is the longest wait for the next piece.
        while !reader.is_full() {
            let frame = tokio::time::timeout(timeout, body.frame())
                .await
                .map_err(|_| Failure {
                    error: HttpError::Timeout(timeout),
                    retry: false,
                })?;
            match frame {
                None => break,
                Some(Ok(frame)) => {
                    if let Ok(chunk) = frame.into_data() {
                        reader.push(chunk).await.map_err(|error| Failure {
                            error,
                            retry: false,
                        })?;
                    }
                }
                Some(Err(e)) => {
                    return Err(Failure {
                        error: HttpError::Protocol(describe(&e)),
                        retry: false,
                    });
                }
            }
        }
        let (body, truncated) = reader.finish().await.map_err(|error| Failure {
            error,
            retry: false,
        })?;
        let done_at = Instant::now();
        // A half-read HTTP/1.1 connection can't carry another request.
        if version == HttpVersion::Http11 {
            if !truncated {
                self.inner.pool.checkin(key.clone(), connection);
            }
        } else {
            self.inner.pool.touch(key, &connection);
        }

        let reason = match parts.extensions.get::<hyper::ext::ReasonPhrase>() {
            Some(reason) => String::from_utf8_lossy(reason.as_bytes()).into_owned(),
            None => parts
                .status
                .canonical_reason()
                .unwrap_or_default()
                .to_owned(),
        };
        Ok(Response {
            version,
            status,
            reason,
            headers,
            body,
            truncated,
            content_encoding,
            timing: marks.finish(head_at, done_at),
            redirects: Vec::new(),
        })
    }
}
