//! Reads a response body: decodes it, keeps it in memory or in a temporary
//! file, and reports events as the pieces arrive.

use std::io::{self, Write as _};
use std::path::Path;
use std::time::Instant;

use bytes::{Bytes, BytesMut};
use flate2::write::{DeflateDecoder, GzDecoder, ZlibDecoder};
use tokio::io::AsyncWriteExt as _;

use crate::client::{Event, EventSink};
use crate::error::HttpError;
use crate::response::{ResponseBody, SPILL_THRESHOLD};
use crate::sse::SseParser;

/// How much a spilled body collects before it is written to the file.
const FLUSH_SIZE: usize = 256 * 1024;

/// What the reader needs to know about the response and the request.
pub(crate) struct ReaderConfig<'a> {
    pub(crate) spill_dir: &'a Path,
    pub(crate) max_body_bytes: u64,
    /// The `Content-Encoding` to decode, when it is one we decode.
    pub(crate) decode: Option<Encoding>,
    pub(crate) sse: bool,
    pub(crate) events: Option<&'a EventSink<'a>>,
    /// When the request began, for the time of Server-Sent Events.
    pub(crate) started: Instant,
}

/// The content encodings Dea decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Gzip,
    Deflate,
}

impl Encoding {
    /// The encoding named by a `Content-Encoding` value, if we decode it.
    pub(crate) fn from_header(value: &str) -> Option<Encoding> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gzip" | "x-gzip" => Some(Encoding::Gzip),
            "deflate" => Some(Encoding::Deflate),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Encoding::Gzip => "gzip",
            Encoding::Deflate => "deflate",
        }
    }
}

pub(crate) struct BodyReader<'a> {
    spill_dir: &'a Path,
    limit: u64,
    decoder: Option<Decoder>,
    sse: Option<SseParser>,
    events: Option<&'a EventSink<'a>>,
    started: Instant,
    /// The whole body while small, the part not yet written once spilled.
    memory: BytesMut,
    file: Option<Spilled>,
    len: u64,
    truncated: bool,
}

struct Spilled {
    file: tokio::fs::File,
    path: tempfile::TempPath,
}

impl<'a> BodyReader<'a> {
    pub(crate) fn new(config: ReaderConfig<'a>) -> Self {
        BodyReader {
            spill_dir: config.spill_dir,
            limit: config.max_body_bytes,
            decoder: config.decode.map(Decoder::new),
            sse: config.sse.then(SseParser::default),
            events: config.events,
            started: config.started,
            memory: BytesMut::new(),
            file: None,
            len: 0,
            truncated: false,
        }
    }

    /// Whether the limit was reached; the rest of the body is not wanted.
    pub(crate) fn is_full(&self) -> bool {
        self.truncated
    }

    /// Takes the next piece of the body as it came off the wire.
    pub(crate) async fn push(&mut self, chunk: Bytes) -> Result<(), HttpError> {
        let chunk = match &mut self.decoder {
            Some(decoder) => Bytes::from(decoder.feed(&chunk)?),
            None => chunk,
        };
        self.keep(chunk).await
    }

    /// Ends the body: what the decoder still holds is kept, a spilled file
    /// is written out. Returns the body and whether it was cut short.
    pub(crate) async fn finish(mut self) -> Result<(ResponseBody, bool), HttpError> {
        if !self.truncated
            && let Some(decoder) = self.decoder.take()
        {
            let rest = decoder.finish()?;
            self.keep(Bytes::from(rest)).await?;
        }
        let truncated = self.truncated;
        if self.file.is_none() {
            return Ok((ResponseBody::Memory(self.memory.freeze()), truncated));
        }
        self.flush().await?;
        let Some(mut spilled) = self.file else {
            unreachable!("checked above");
        };
        spilled
            .file
            .flush()
            .await
            .map_err(|e| file_error(&spilled.path, &e))?;
        let body = ResponseBody::File {
            path: spilled.path,
            len: self.len,
        };
        Ok((body, truncated))
    }

    /// Stores `chunk` up to the limit and reports it.
    async fn keep(&mut self, mut chunk: Bytes) -> Result<(), HttpError> {
        let room = self.limit.saturating_sub(self.len);
        if chunk.len() as u64 > room {
            self.truncated = true;
            chunk.truncate(usize::try_from(room).unwrap_or(usize::MAX));
        }
        if chunk.is_empty() {
            return Ok(());
        }
        self.len += chunk.len() as u64;
        self.memory.extend_from_slice(&chunk);
        self.store().await?;

        if let Some(sink) = self.events {
            sink(Event::Data(chunk.clone()));
            if let Some(parser) = &mut self.sse {
                for event in parser.push(&chunk, self.started.elapsed()) {
                    sink(Event::Sse(event));
                }
            }
        }
        Ok(())
    }

    /// Moves a body that outgrew memory to its file, and keeps a spilled
    /// body flowing to it in large writes.
    async fn store(&mut self) -> Result<(), HttpError> {
        if self.file.is_none() {
            if self.len <= SPILL_THRESHOLD {
                return Ok(());
            }
            self.file = Some(self.create_file()?);
            return self.flush().await;
        }
        if self.memory.len() >= FLUSH_SIZE {
            return self.flush().await;
        }
        Ok(())
    }

    /// Writes what memory holds to the file.
    async fn flush(&mut self) -> Result<(), HttpError> {
        let Some(spilled) = &mut self.file else {
            return Ok(());
        };
        spilled
            .file
            .write_all(&self.memory)
            .await
            .map_err(|e| file_error(&spilled.path, &e))?;
        self.memory.clear();
        Ok(())
    }

    fn create_file(&self) -> Result<Spilled, HttpError> {
        let (file, path) = tempfile::Builder::new()
            .prefix("dea-body-")
            .tempfile_in(self.spill_dir)
            .map_err(|e| file_error(self.spill_dir, &e))?
            .into_parts();
        Ok(Spilled {
            file: tokio::fs::File::from_std(file),
            path,
        })
    }
}

/// The error text names the path and the OS reason, never the body.
fn file_error(path: &Path, error: &io::Error) -> HttpError {
    HttpError::File {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// A streaming decoder that collects its output in a vector.
struct Decoder {
    encoding: Encoding,
    state: State,
}

enum State {
    /// `deflate` may be zlib-wrapped, as HTTP defines it, or raw, as some
    /// servers send it. The first two bytes tell.
    Undecided(Vec<u8>),
    Gzip(GzDecoder<Vec<u8>>),
    Zlib(ZlibDecoder<Vec<u8>>),
    Raw(DeflateDecoder<Vec<u8>>),
}

impl Decoder {
    fn new(encoding: Encoding) -> Self {
        let state = match encoding {
            Encoding::Gzip => State::Gzip(GzDecoder::new(Vec::new())),
            Encoding::Deflate => State::Undecided(Vec::new()),
        };
        Decoder { encoding, state }
    }

    fn feed(&mut self, chunk: &[u8]) -> Result<Vec<u8>, HttpError> {
        self.feed_inner(chunk)
            .map_err(|e| decode_error(self.encoding, &e))
    }

    fn feed_inner(&mut self, chunk: &[u8]) -> io::Result<Vec<u8>> {
        if let State::Undecided(held) = &mut self.state {
            held.extend_from_slice(chunk);
            if held.len() < 2 {
                return Ok(Vec::new());
            }
            let held = std::mem::take(held);
            let zlib =
                held[0] & 0x0f == 8 && (u16::from(held[0]) * 256 + u16::from(held[1])) % 31 == 0;
            self.state = if zlib {
                State::Zlib(ZlibDecoder::new(Vec::new()))
            } else {
                State::Raw(DeflateDecoder::new(Vec::new()))
            };
            return self.feed_inner(&held);
        }
        match &mut self.state {
            State::Gzip(decoder) => {
                decoder.write_all(chunk)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            State::Zlib(decoder) => {
                decoder.write_all(chunk)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            State::Raw(decoder) => {
                decoder.write_all(chunk)?;
                Ok(std::mem::take(decoder.get_mut()))
            }
            State::Undecided(_) => unreachable!("decided above"),
        }
    }

    /// The output the decoder still held at the end of the body.
    fn finish(self) -> Result<Vec<u8>, HttpError> {
        let encoding = self.encoding;
        let result = match self.state {
            State::Undecided(held) if held.is_empty() => Ok(Vec::new()),
            State::Undecided(_) => Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
            State::Gzip(decoder) => decoder.finish(),
            State::Zlib(decoder) => decoder.finish(),
            State::Raw(decoder) => decoder.finish(),
        };
        result.map_err(|e| decode_error(encoding, &e))
    }
}

fn decode_error(encoding: Encoding, error: &io::Error) -> HttpError {
    HttpError::Protocol(format!(
        "could not decode the {} body: {error}",
        encoding.name()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};

    fn decode_by_byte(encoding: Encoding, compressed: &[u8]) -> Result<Vec<u8>, HttpError> {
        let mut decoder = Decoder::new(encoding);
        let mut out = Vec::new();
        for byte in compressed {
            out.extend(decoder.feed(&[*byte])?);
        }
        out.extend(decoder.finish()?);
        Ok(out)
    }

    #[test]
    fn gzip_zlib_and_raw_deflate_decode_in_tiny_pieces() {
        let text = b"some text, some text, some text".repeat(50);
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(&text).unwrap();
        let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
        zlib.write_all(&text).unwrap();
        let mut raw = DeflateEncoder::new(Vec::new(), Compression::default());
        raw.write_all(&text).unwrap();

        let gzip = gzip.finish().unwrap();
        let zlib = zlib.finish().unwrap();
        let raw = raw.finish().unwrap();
        assert_eq!(decode_by_byte(Encoding::Gzip, &gzip).unwrap(), text);
        assert_eq!(decode_by_byte(Encoding::Deflate, &zlib).unwrap(), text);
        assert_eq!(decode_by_byte(Encoding::Deflate, &raw).unwrap(), text);
    }

    #[test]
    fn garbage_is_a_protocol_error_that_does_not_echo_it() {
        let error = decode_by_byte(Encoding::Gzip, b"garbage-secret-bytes").unwrap_err();
        let shown = error.to_string();
        assert!(shown.contains("gzip"), "{shown}");
        assert!(!shown.contains("secret"), "{shown}");
    }
}
