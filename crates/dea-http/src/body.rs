//! Turns a [`Body`] into the bytes on the wire. The plan (sizes, headers,
//! multipart framing) is made once per hop. Each attempt opens the files
//! again and streams them in pieces, so a retry or a 307/308 redirect
//! replays the body without holding a file in memory.

use std::collections::VecDeque;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use bytes::{Bytes, BytesMut};
use hyper::body::{Frame, SizeHint};
use tokio_util::io::poll_read_buf;

use crate::error::HttpError;
use crate::request::{Body, Part, PartValue};

/// The most a frame of the body holds. Files are read this much at a time,
/// and bytes in memory are handed out in pieces this size, so upload
/// progress moves with every piece the connection takes.
const CHUNK: usize = 64 * 1024;
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

    /// Whether any bytes are sent.
    pub(crate) fn has_bytes(&self) -> bool {
        self.len > 0
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
            progress: Progress::new(self.len == 0),
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
    progress: Progress,
}

impl WireBody {
    /// How far the connection got in taking this body.
    pub(crate) fn progress(&self) -> Progress {
        self.progress.clone()
    }
}

/// When the connection last took a piece of a body, and whether it took
/// them all. The connection takes the next piece only once it has room for
/// it, so while the body is sent this moves as fast as the upload does.
#[derive(Clone)]
pub(crate) struct Progress(Arc<Moments>);

struct Moments {
    since: Instant,
    /// Nanoseconds after `since`, plus one; 0 before the first piece.
    last: AtomicU64,
    /// Every piece was taken; from the start for an empty body.
    all_taken: AtomicBool,
}

impl Progress {
    fn new(empty: bool) -> Self {
        Progress(Arc::new(Moments {
            since: Instant::now(),
            last: AtomicU64::new(0),
            all_taken: AtomicBool::new(empty),
        }))
    }

    /// Notes that a piece was taken, the last one when `last`.
    fn mark(&self, last: bool) {
        let nanos = u64::try_from(self.0.since.elapsed().as_nanos()).unwrap_or(u64::MAX - 1);
        self.0.last.store(nanos + 1, Ordering::Release);
        if last {
            self.0.all_taken.store(true, Ordering::Release);
        }
    }

    /// Whether the connection took the whole body.
    pub(crate) fn all_taken(&self) -> bool {
        self.0.all_taken.load(Ordering::Acquire)
    }

    /// `None` until the first piece is taken.
    pub(crate) fn last(&self) -> Option<Instant> {
        match self.0.last.load(Ordering::Acquire) {
            0 => None,
            nanos => Some(self.0.since + std::time::Duration::from_nanos(nanos - 1)),
        }
    }
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
                    if data.is_empty() {
                        this.queue.pop_front();
                        continue;
                    }
                    let frame = data.split_to(data.len().min(CHUNK));
                    this.remaining -= frame.len() as u64;
                    this.progress.mark(this.remaining == 0);
                    return Poll::Ready(Some(Ok(Frame::data(frame))));
                }
                Some(Source::File { file, left }) => {
                    if *left == 0 {
                        this.queue.pop_front();
                        continue;
                    }
                    let want = (*left).min(CHUNK as u64);
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
                    this.progress.mark(this.remaining == 0);
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
