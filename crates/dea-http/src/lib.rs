//! Sends HTTP/1.1 and HTTP/2 requests for Dea. Built on hyper's low-level
//! connections with its own pool, so DNS, connect, TLS, waiting and
//! download are timed apart and reused connections are marked.

mod body;
mod client;
mod connect;
mod error;
mod pool;
mod reader;
mod redirect;
mod request;
mod response;
mod sse;
mod timing;
mod tls;

pub use client::{Client, ClientConfig, Event, ProxyConfig, SendContext};
pub use error::HttpError;
pub use request::{Body, ClientIdentity, Options, Part, PartValue, Request, TlsConfig};
pub use response::{
    DEFAULT_MAX_BODY_BYTES, HttpVersion, Redirect, Response, ResponseBody, SPILL_THRESHOLD,
};
pub use sse::SseEvent;
pub use timing::Timing;
pub use tokio_util::sync::CancellationToken;
