//! Why a request failed. Messages name headers and hosts but never hold
//! header values or body bytes, which may be secrets.

use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("could not resolve {host}: {message}")]
    Dns { host: String, message: String },
    #[error("connection refused by {addr}")]
    ConnectionRefused { addr: SocketAddr },
    #[error("could not connect to {addr}: {message}")]
    Connect { addr: SocketAddr, message: String },
    #[error("{0}")]
    Tls(TlsError),
    #[error("no response within {0:?}")]
    Timeout(Duration),
    #[error("the request was cancelled")]
    Cancelled,
    #[error("more than {0} redirects")]
    TooManyRedirects(u32),
    #[error("proxy error: {0}")]
    Proxy(String),
    #[error("could not read {}: {message}", path.display())]
    File { path: PathBuf, message: String },
    #[error("HTTP protocol error: {0}")]
    Protocol(String),
}

/// A failed TLS handshake, or a TLS failure on an open connection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("TLS: {message}")]
pub struct TlsError {
    pub kind: TlsErrorKind,
    pub message: String,
}

/// What a TLS failure was about, as far as the user can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsErrorKind {
    /// The server's certificate is past its end date.
    Expired,
    /// The server's certificate is not valid before a later date.
    NotYetValid,
    /// The server's certificate does not lead to a trusted CA.
    UnknownIssuer,
    /// The server's certificate is for other host names.
    WrongHost,
    /// The server turned down the client certificate, or the lack of one.
    ClientCertificateRejected,
    /// Anything else, such as a handshake the server broke off.
    Other,
}

impl TlsError {
    pub(crate) fn other(message: impl Into<String>) -> Self {
        TlsError {
            kind: TlsErrorKind::Other,
            message: message.into(),
        }
    }
}

/// An error and its causes on one line, each cause once.
pub(crate) fn describe(error: &(dyn Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}
