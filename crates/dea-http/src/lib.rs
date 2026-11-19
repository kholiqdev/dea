//! Sends HTTP/1.1 and HTTP/2 requests for Dea. Built on hyper's low-level
//! connections with its own pool, so DNS, connect, TLS, waiting and
//! download are timed apart and reused connections are marked.

mod client;
mod connect;
mod error;
mod pool;
mod redirect;
mod request;
mod response;
mod timing;
mod tls;

pub use client::{Client, ClientConfig, ProxyConfig, SendContext};
pub use error::HttpError;
pub use request::{Body, ClientIdentity, Options, Request, TlsConfig};
pub use response::{HttpVersion, Redirect, Response, ResponseBody};
pub use timing::Timing;
pub use tokio_util::sync::CancellationToken;
