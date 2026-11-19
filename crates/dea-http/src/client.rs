//! The client: sends a request over a pooled or a new connection and reads
//! the response.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use http_body_util::BodyExt as _;
use hyper::header::HeaderValue;
use tokio_util::sync::CancellationToken;

use crate::connect::{self, Connection};
use crate::error::{HttpError, describe};
use crate::pool::{Pool, PoolKey};
use crate::request::{Prepared, Request};
use crate::response::{HttpVersion, Response, ResponseBody};
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

    /// Sends `request` and reads the whole response. A pooled connection
    /// that fails before any of the response arrives (the server closed it
    /// while idle) is replaced by a new one, once.
    pub async fn send(
        &self,
        request: Request,
        ctx: SendContext<'_>,
    ) -> Result<Response, HttpError> {
        if ctx.cancel.is_cancelled() {
            return Err(HttpError::Cancelled);
        }
        let prepared = Prepared::new(&request, &self.inner.user_agent)?;
        let key = PoolKey::new(&request);

        if let Some(connection) = self.inner.pool.checkout(&key) {
            let marks = Marks::reused(Instant::now());
            match self.exchange(connection, marks, &prepared, &key).await {
                Ok(response) => return Ok(response),
                Err(failure) if failure.retry => {}
                Err(failure) => return Err(failure.error),
            }
        }

        let (connection, marks) =
            connect::open(&request.url, &request.tls, request.options.timeout).await?;
        if let Some(shared) = connection.share() {
            self.inner.pool.checkin(key.clone(), shared);
        }
        self.exchange(connection, marks, &prepared, &key)
            .await
            .map_err(|failure| failure.error)
    }

    /// Sends one attempt over `connection` and reads the response. An
    /// HTTP/1.1 connection goes back to the pool once the body is read.
    async fn exchange(
        &self,
        mut connection: Connection,
        marks: Marks,
        prepared: &Prepared,
        key: &PoolKey,
    ) -> Result<Response, Failure> {
        let version = connection.version();
        let received_before = connection.bytes_received();
        let head = match connection.send(prepared.to_wire(version)).await {
            Ok(head) => head,
            Err(e) => {
                let nothing_arrived = connection.bytes_received() == received_before;
                return Err(Failure {
                    error: HttpError::Protocol(describe(&e)),
                    retry: marks.reused && nothing_arrived,
                });
            }
        };
        let head_at = Instant::now();
        let (parts, body) = head.into_parts();
        let body = body.collect().await.map_err(|e| Failure {
            error: HttpError::Protocol(describe(&e)),
            retry: false,
        })?;
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
            body: ResponseBody::Memory(body.to_bytes()),
            truncated: false,
            content_encoding: None,
            timing: marks.finish(head_at, done_at),
            redirects: Vec::new(),
        })
    }
}
