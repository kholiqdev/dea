//! The client: sends a request over a pooled or a new connection and reads
//! the response.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::BytesMut;
use http_body_util::BodyExt as _;
use hyper::header::HeaderValue;
use tokio_util::sync::CancellationToken;

use crate::connect::{self, Connection};
use crate::error::{HttpError, describe};
use crate::pool::{Pool, PoolKey};
use crate::redirect::next_hop;
use crate::request::{Prepared, Request};
use crate::response::{HttpVersion, Redirect, Response, ResponseBody};
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
    pool: Pool,
}

/// What a single send may use besides the request.
pub struct SendContext<'a> {
    cancel: &'a CancellationToken,
}

impl<'a> SendContext<'a> {
    pub fn new(cancel: &'a CancellationToken) -> Self {
        SendContext { cancel }
    }
}

/// What one attempt over a connection needs besides the connection.
#[derive(Clone, Copy)]
struct Attempt<'a> {
    prepared: &'a Prepared,
    key: &'a PoolKey,
    timeout: Duration,
    /// When the response head must have arrived.
    head_deadline: tokio::time::Instant,
}

/// The first non-empty `Location` header.
fn location(response: &Response) -> Option<&str> {
    response
        .headers
        .iter()
        .find(|(name, value)| name.eq_ignore_ascii_case("location") && !value.is_empty())
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
            result = self.follow(request) => result,
        }
    }

    /// Sends hops until one is not a redirect we follow.
    async fn follow(&self, request: Request) -> Result<Response, HttpError> {
        let mut current = request;
        let mut redirects = Vec::new();
        loop {
            let mut response = self.hop(&current).await?;
            let next = if current.options.follow_redirects {
                match location(&response) {
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
    async fn hop(&self, request: &Request) -> Result<Response, HttpError> {
        let prepared = Prepared::new(request, &self.inner.user_agent)?;
        let key = PoolKey::new(request);
        let timeout = request.options.timeout;
        let head_deadline = deadline(timeout);

        if let Some(connection) = self.inner.pool.checkout(&key) {
            let marks = Marks::reused(Instant::now());
            let attempt = Attempt {
                prepared: &prepared,
                key: &key,
                timeout,
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
            timeout,
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
            timeout,
            head_deadline,
        } = *attempt;
        let version = connection.version();
        let received_before = connection.bytes_received();
        let wire = prepared.to_wire(version).map_err(|error| Failure {
            error,
            retry: false,
        })?;
        let sent = tokio::time::timeout_at(head_deadline, connection.send(wire)).await;
        let head = match sent {
            Err(_) => {
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
        let mut data = BytesMut::new();
        // From here the timeout is the longest wait for the next piece.
        loop {
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
                        data.extend_from_slice(&chunk);
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
        let done_at = Instant::now();
        if version == HttpVersion::Http11 {
            self.inner.pool.checkin(key.clone(), connection);
        }

        let reason = match parts.extensions.get::<hyper::ext::ReasonPhrase>() {
            Some(reason) => String::from_utf8_lossy(reason.as_bytes()).into_owned(),
            None => parts
                .status
                .canonical_reason()
                .unwrap_or_default()
                .to_owned(),
        };
        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| {
                let value = String::from_utf8_lossy(value.as_bytes()).into_owned();
                (name.as_str().to_owned(), value)
            })
            .collect();
        Ok(Response {
            version,
            status: parts.status.as_u16(),
            reason,
            headers,
            body: ResponseBody::Memory(data.freeze()),
            truncated: false,
            content_encoding: None,
            timing: marks.finish(head_at, done_at),
            redirects: Vec::new(),
        })
    }
}
