//! Turns a [`Body`] into the bytes on the wire. The plan (sizes, headers,
//! multipart framing) is made once per hop. Each attempt opens the files
//! again and streams them in pieces, so a retry or a 307/308 redirect
//! replays the body without holding a file in memory.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use hyper::body::{Frame, SizeHint};
use tokio_util::io::poll_read_buf;

use crate::error::HttpError;
use crate::request::{Body, Part, PartValue};

/// Files are read this much at a time.
const CHUNK: u64 = 64 * 1024;
const OCTET_STREAM: &str = "application/octet-stream";
const BOUNDARY_LEN: usize = 32;
const ALPHANUMERIC: &[u8; 62] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Clone)]
enum Piece {
    Bytes(Bytes),
    File { path: PathBuf, len: u64 },
}

/// What a body sends: its pieces, total size and automatic content type.
pub(crate) struct Plan {
    pieces: Vec<Piece>,
    len: u64,
    content_type: Option<String>,
}

impl Plan {
    pub(crate) fn new(body: &Body) -> Result<Plan, HttpError> {
        match body {
            Body::Empty => Ok(Plan::of(Vec::new(), None)),
            Body::Bytes { data, content_type } => Ok(Plan::of(
                vec![Piece::Bytes(data.clone())],
                content_type.clone(),
            )),
            Body::File { path, content_type } => {
                let len = file_len(path)?;
                let content_type = content_type.clone().unwrap_or_else(|| OCTET_STREAM.into());
                Ok(Plan::of(
                    vec![Piece::File {
                        path: path.clone(),
                        len,
                    }],
                    Some(content_type),
                ))
            }
            Body::Form(fields) => {
                let encoded = url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(fields)
                    .finish();
                Ok(Plan::of(
                    vec![Piece::Bytes(Bytes::from(encoded))],
                    Some("application/x-www-form-urlencoded".into()),
                ))
            }
            Body::Multipart(parts) => {
                let boundary = boundary();
                let pieces = multipart(parts, &boundary)?;
                Ok(Plan::of(
                    pieces,
                    Some(format!("multipart/form-data; boundary={boundary}")),
                ))
            }
        }
    }

    fn of(pieces: Vec<Piece>, content_type: Option<String>) -> Plan {
        let len = pieces
            .iter()
            .map(|piece| match piece {
                Piece::Bytes(data) => data.len() as u64,
                Piece::File { len, .. } => *len,
            })
            .sum();
        Plan {
            pieces,
            len,
            content_type,
        }
    }

    /// The `Content-Type` Dea adds unless the user wrote one.
    pub(crate) fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// The body for one attempt, with its files opened again.
    pub(crate) fn open(&self) -> Result<WireBody, HttpError> {
        let mut queue = VecDeque::with_capacity(self.pieces.len());
        for piece in &self.pieces {
            queue.push_back(match piece {
                Piece::Bytes(data) => Source::Bytes(data.clone()),
                Piece::File { path, len } => {
                    let file = std::fs::File::open(path).map_err(|e| file_error(path, &e))?;
                    Source::File {
                        file: tokio::fs::File::from_std(file),
                        left: *len,
                    }
                }
            });
        }
        Ok(WireBody {
            queue,
            remaining: self.len,
        })
    }
}

fn file_error(path: &Path, error: &io::Error) -> HttpError {
    HttpError::File {
        path: path.to_owned(),
        message: error.to_string(),
    }
}

fn file_len(path: &Path) -> Result<u64, HttpError> {
    let meta = std::fs::metadata(path).map_err(|e| file_error(path, &e))?;
    if !meta.is_file() {
        return Err(HttpError::File {
            path: path.to_owned(),
            message: "not a regular file".into(),
        });
    }
    Ok(meta.len())
}

fn boundary() -> String {
    (0..BOUNDARY_LEN)
        .map(|_| char::from(ALPHANUMERIC[rand::random_range(0..ALPHANUMERIC.len())]))
        .collect()
}

/// A name or file name inside a quoted header parameter: `"` and line
/// breaks would end it early, so they are percent-encoded as browsers do.
fn quoted(value: &str) -> String {
    value
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn multipart(parts: &[Part], boundary: &str) -> Result<Vec<Piece>, HttpError> {
    let mut pieces = Vec::new();
    let mut head: Vec<u8> = Vec::new();
    for part in parts {
        let name = quoted(&part.name);
        head.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"").as_bytes(),
        );
        match &part.value {
            PartValue::Text(text) => {
                head.extend_from_slice(b"\r\n\r\n");
                head.extend_from_slice(text.as_bytes());
            }
            PartValue::File { path, content_type } => {
                let len = file_len(path)?;
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let content_type = content_type.as_deref().unwrap_or(OCTET_STREAM);
                head.extend_from_slice(
                    format!(
                        "; filename=\"{}\"\r\nContent-Type: {content_type}\r\n\r\n",
                        quoted(&file_name)
                    )
                    .as_bytes(),
                );
                pieces.push(Piece::Bytes(Bytes::from(std::mem::take(&mut head))));
                pieces.push(Piece::File {
                    path: path.clone(),
                    len,
                });
            }
        }
        head.extend_from_slice(b"\r\n");
    }
    head.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    pieces.push(Piece::Bytes(Bytes::from(head)));
    Ok(pieces)
}

enum Source {
    Bytes(Bytes),
    File { file: tokio::fs::File, left: u64 },
}

/// The body type the connections carry. Its size is known up front, so
/// hyper sends `Content-Length` and never chunks.
pub(crate) struct WireBody {
    queue: VecDeque<Source>,
    remaining: u64,
}

impl hyper::body::Body for WireBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        let this = self.get_mut();
        loop {
            match this.queue.front_mut() {
                None => return Poll::Ready(None),
                Some(Source::Bytes(data)) => {
                    let data = std::mem::take(data);
                    this.queue.pop_front();
                    if data.is_empty() {
                        continue;
                    }
                    this.remaining -= data.len() as u64;
                    return Poll::Ready(Some(Ok(Frame::data(data))));
                }
                Some(Source::File { file, left }) => {
                    if *left == 0 {
                        this.queue.pop_front();
                        continue;
                    }
                    let want = (*left).min(CHUNK);
                    let mut buf = BytesMut::with_capacity(usize::try_from(want).unwrap_or(0));
                    let read = match poll_read_buf(Pin::new(file), cx, &mut buf) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(e))),
                        Poll::Ready(Ok(read)) => read as u64,
                    };
                    if read == 0 {
                        return Poll::Ready(Some(Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "the file ended before its size at the start of the send",
                        ))));
                    }
                    let read = read.min(want);
                    buf.truncate(usize::try_from(read).unwrap_or(0));
                    *left -= read;
                    this.remaining -= read;
                    return Poll::Ready(Some(Ok(Frame::data(buf.freeze()))));
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.remaining == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}
