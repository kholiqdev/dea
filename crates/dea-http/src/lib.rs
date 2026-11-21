//! Sends HTTP/1.1 and HTTP/2 requests for Dea. Built on hyper's low-level
//! connections with its own pool, so DNS, connect, TLS, waiting and
//! download are timed apart and reused connections are marked.

mod body;
mod client;
mod connect;
mod cookies;
mod error;
mod pool;
mod proxy;
mod reader;
mod redirect;
mod request;
mod response;
mod sse;
mod timing;
mod tls;

pub use client::{Client, ClientConfig, Event, Phase, SendContext};
pub use cookies::{CookieJar, CookieJarError, StoredCookie};
pub use error::{HttpError, TlsError, TlsErrorKind};
pub use proxy::{NoProxy, ProxyConfig, ProxySettings, proxy_from_env, system_proxy};
pub use request::{
    Body, ClientIdentity, Options, Part, PartValue, Request, TlsConfig, automatic_headers,
};
pub use response::{
    DEFAULT_MAX_BODY_BYTES, HttpVersion, Redirect, Response, ResponseBody, SPILL_THRESHOLD,
};
pub use sse::SseEvent;
pub use timing::Timing;
pub use tokio_util::sync::CancellationToken;
