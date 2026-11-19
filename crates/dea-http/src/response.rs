//! The response, as read from the final hop.

use std::{fmt, fs, io};

use bytes::Bytes;

use crate::timing::Timing;

/// Bodies larger than this are written to a temporary file (decision 10).
pub const SPILL_THRESHOLD: u64 = 10 * 1024 * 1024;

/// Reading stops at this many body bytes unless the request says otherwise.
pub const DEFAULT_MAX_BODY_BYTES: u64 = 1024 * 1024 * 1024;

pub struct Response {
    pub version: HttpVersion,
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: ResponseBody,
    /// The body stopped at `Options::max_body_bytes`.
    pub truncated: bool,
    /// The encoding the server sent the body in, before decoding.
    pub content_encoding: Option<String>,
    pub timing: Timing,
    /// Earlier hops, oldest first.
    pub redirects: Vec<Redirect>,
}

/// Shows header names and the body size; values may be secrets.
impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("Response")
            .field("version", &self.version)
            .field("status", &self.status)
            .field("reason", &self.reason)
            .field("headers", &names)
            .field("body", &self.body)
            .field("truncated", &self.truncated)
            .field("content_encoding", &self.content_encoding)
            .field("timing", &self.timing)
            .field("redirects", &self.redirects.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVersion {
    Http11,
    Http2,
}

impl fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HttpVersion::Http11 => "HTTP/1.1",
            HttpVersion::Http2 => "HTTP/2",
        })
    }
}

/// The response body, in memory or, when large, in a temporary file that
/// is deleted when the body is dropped.
pub enum ResponseBody {
    Memory(Bytes),
    File { path: tempfile::TempPath, len: u64 },
}

impl ResponseBody {
    pub fn len(&self) -> u64 {
        match self {
            ResponseBody::Memory(bytes) => bytes.len() as u64,
            ResponseBody::File { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_all(&self) -> io::Result<Vec<u8>> {
        match self {
            ResponseBody::Memory(bytes) => Ok(bytes.to_vec()),
            ResponseBody::File { path, .. } => fs::read(path),
        }
    }
}

/// Shows the size, not the bytes.
impl fmt::Debug for ResponseBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseBody::Memory(bytes) => {
                f.debug_struct("Memory").field("len", &bytes.len()).finish()
            }
            ResponseBody::File { path, len } => f
                .debug_struct("File")
                .field("path", &path.display())
                .field("len", len)
                .finish(),
        }
    }
}

/// An earlier hop that answered with a redirect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirect {
    pub status: u16,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_leaves_out_header_values_and_body() {
        let response = Response {
            version: HttpVersion::Http11,
            status: 200,
            reason: "OK".into(),
            headers: vec![("set-cookie".into(), "session=cookie-secret".into())],
            body: ResponseBody::Memory(Bytes::from_static(b"body-secret")),
            truncated: false,
            content_encoding: None,
            timing: Timing::default(),
            redirects: Vec::new(),
        };
        let shown = format!("{response:?}");
        assert!(shown.contains("set-cookie"), "{shown}");
        assert!(!shown.contains("cookie-secret"), "{shown}");
        assert!(!shown.contains("body-secret"), "{shown}");
    }
}
