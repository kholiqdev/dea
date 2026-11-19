//! Reads a response body: decodes it, keeps it in memory or in a temporary
//! file, and reports events as the pieces arrive.

use std::io;
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

    /// Takes the next piece of the body as it came off the wire. A
    /// decoded piece goes through the decoder in small slices, so a piece
    /// that expands a lot is never held whole.
    pub(crate) async fn push(&mut self, chunk: Bytes) -> Result<(), HttpError> {
        if self.decoder.is_none() {
            return self.keep(chunk).await;
        }
        for slice in chunk.chunks(INPUT_SLICE) {
            let room = room(self.limit, self.len);
            let Some(decoder) = &mut self.decoder else {
                break;
            };
            let decoded = decoder.feed(slice, room)?;
            self.truncated |= decoded.full;
            self.keep(Bytes::from(decoded.bytes)).await?;
            if self.truncated {
                break;
            }
        }
        Ok(())
    }

    /// Ends the body: what the decoder still holds is kept, a spilled file
    /// is written out. Returns the body and whether it was cut short.
    pub(crate) async fn finish(mut self) -> Result<(ResponseBody, bool), HttpError> {
        if !self.truncated
            && let Some(decoder) = self.decoder.take()
        {
            let decoded = decoder.finish(room(self.limit, self.len))?;
            self.truncated |= decoded.full;
            self.keep(Bytes::from(decoded.bytes)).await?;
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

/// How many more body bytes fit under `limit`.
fn room(limit: u64, len: u64) -> usize {
    usize::try_from(limit.saturating_sub(len)).unwrap_or(usize::MAX)
}

/// The error text names the path and the OS reason, never the body.
fn file_error(path: &Path, error: &io::Error) -> HttpError {
    HttpError::File {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

/// How much compressed input goes into the decoder at a time. Deflate
/// expands at most about 1000 to 1, so this bounds what one step holds.
const INPUT_SLICE: usize = 4 * 1024;

/// Decoder output that refuses to grow past `room` bytes.
#[derive(Default)]
struct Capped {
    out: Vec<u8>,
    room: usize,
    full: bool,
}

impl io::Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let free = self.room.saturating_sub(self.out.len());
        if free == 0 && !buf.is_empty() {
            self.full = true;
            return Err(io::Error::other("the body limit was reached"));
        }
        let take = buf.len().min(free);
        self.out.extend_from_slice(&buf[..take]);
        Ok(take)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A streaming decoder with bounded output.
struct Decoder {
    encoding: Encoding,
    /// Compressed bytes taken so far; none means there was no body.
    fed: u64,
    state: State,
}

enum State {
    /// `deflate` may be zlib-wrapped, as HTTP defines it, or raw, as some
    /// servers send it. The first two bytes tell.
    Undecided(Vec<u8>),
    Gzip(GzDecoder<Capped>),
    Zlib(ZlibDecoder<Capped>),
    Raw(DeflateDecoder<Capped>),
}

/// What a decoding step produced.
struct Decoded {
    bytes: Vec<u8>,
    /// The step stopped because `room` was used up.
    full: bool,
}

impl Decoder {
    fn new(encoding: Encoding) -> Self {
        let state = match encoding {
            Encoding::Gzip => State::Gzip(GzDecoder::new(Capped::default())),
            Encoding::Deflate => State::Undecided(Vec::new()),
        };
        Decoder {
            encoding,
            fed: 0,
            state,
        }
    }

    /// Decodes `chunk` into at most `room` bytes.
    fn feed(&mut self, chunk: &[u8], room: usize) -> Result<Decoded, HttpError> {
        self.fed += chunk.len() as u64;
        self.step(Some(chunk), room)
            .map_err(|e| decode_error(self.encoding, &e))
    }

    /// Ends the body. A body that never began (HEAD, 204, 304) ends
    /// quietly; a stream that stopped early is an error.
    fn finish(mut self, room: usize) -> Result<Decoded, HttpError> {
        if self.fed == 0 {
            return Ok(Decoded {
                bytes: Vec::new(),
                full: false,
            });
        }
        let encoding = self.encoding;
        self.step(None, room)
            .map_err(|e| decode_error(encoding, &e))
    }

    /// Writes `chunk`, or ends the stream when there is none.
    fn step(&mut self, chunk: Option<&[u8]>, room: usize) -> io::Result<Decoded> {
        let mut held = Vec::new();
        if let State::Undecided(first) = &mut self.state {
            first.extend_from_slice(chunk.unwrap_or_default());
            if first.len() < 2 {
                if chunk.is_none() && !first.is_empty() {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
                }
                return Ok(Decoded {
                    bytes: Vec::new(),
                    full: false,
                });
            }
            held = std::mem::take(first);
            let zlib =
                held[0] & 0x0f == 8 && (u16::from(held[0]) * 256 + u16::from(held[1])) % 31 == 0;
            self.state = if zlib {
                State::Zlib(ZlibDecoder::new(Capped::default()))
            } else {
                State::Raw(DeflateDecoder::new(Capped::default()))
            };
        }
        let input = if held.is_empty() {
            chunk.unwrap_or_default()
        } else {
            &held
        };
        let finishing = chunk.is_none();
        match &mut self.state {
            State::Gzip(d) => run(d, Ends::GZIP, input, finishing, room),
            State::Zlib(d) => run(d, Ends::ZLIB, input, finishing, room),
            State::Raw(d) => run(d, Ends::RAW, input, finishing, room),
            State::Undecided(_) => unreachable!("decided above"),
        }
    }
}

/// What `run` needs from each kind of write decoder.
struct Ends<D> {
    sink: fn(&mut D) -> &mut Capped,
    end: fn(&mut D) -> io::Result<()>,
}

impl Ends<GzDecoder<Capped>> {
    const GZIP: Self = Ends {
        sink: GzDecoder::get_mut,
        end: GzDecoder::try_finish,
    };
}

impl Ends<ZlibDecoder<Capped>> {
    const ZLIB: Self = Ends {
        sink: ZlibDecoder::get_mut,
        end: ZlibDecoder::try_finish,
    };
}

impl Ends<DeflateDecoder<Capped>> {
    const RAW: Self = Ends {
        sink: DeflateDecoder::get_mut,
        end: DeflateDecoder::try_finish,
    };
}

/// Feeds `input` to a write decoder, or ends it, collecting at most `room`
/// bytes of output.
fn run<D: io::Write>(
    decoder: &mut D,
    ends: Ends<D>,
    input: &[u8],
    finishing: bool,
    room: usize,
) -> io::Result<Decoded> {
    (ends.sink)(decoder).room = room;
    let result = if finishing {
        (ends.end)(decoder)
    } else {
        decoder.write_all(input)
    };
    let capped = (ends.sink)(decoder);
    let (bytes, full) = (std::mem::take(&mut capped.out), capped.full);
    match result {
        Err(_) if full => Ok(Decoded { bytes, full }),
        Err(e) => Err(e),
        Ok(()) => Ok(Decoded { bytes, full }),
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
    use std::io::Write as _;

    fn decode_by_byte(encoding: Encoding, compressed: &[u8]) -> Result<Vec<u8>, HttpError> {
        let mut decoder = Decoder::new(encoding);
        let mut out = Vec::new();
        for byte in compressed {
            out.extend(decoder.feed(&[*byte], usize::MAX)?.bytes);
        }
        out.extend(decoder.finish(usize::MAX)?.bytes);
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

    fn gzip_of_zeros(mib: usize) -> Vec<u8> {
        let mut gzip = GzEncoder::new(Vec::new(), Compression::best());
        let zeros = vec![0u8; 1024 * 1024];
        for _ in 0..mib {
            gzip.write_all(&zeros).unwrap();
        }
        gzip.finish().unwrap()
    }

    #[test]
    fn output_never_passes_the_room_it_was_given() {
        let bomb = gzip_of_zeros(64);
        let mut decoder = Decoder::new(Encoding::Gzip);
        let mut total = 0;
        let mut full = false;
        for slice in bomb.chunks(INPUT_SLICE) {
            let decoded = decoder.feed(slice, 1000 - total).unwrap();
            total += decoded.bytes.len();
            assert!(total <= 1000);
            if decoded.full {
                full = true;
                break;
            }
        }
        assert!(full);
        assert_eq!(total, 1000);
    }

    #[test]
    fn a_slice_of_a_bomb_stays_small_even_with_a_large_room() {
        let bomb = gzip_of_zeros(64);
        let mut decoder = Decoder::new(Encoding::Gzip);
        for slice in bomb.chunks(INPUT_SLICE) {
            let decoded = decoder.feed(slice, usize::MAX).unwrap();
            assert!(
                decoded.bytes.len() <= 8 * 1024 * 1024,
                "{}",
                decoded.bytes.len()
            );
        }
    }

    #[test]
    fn a_body_that_never_began_ends_quietly_but_a_cut_stream_does_not() {
        for encoding in [Encoding::Gzip, Encoding::Deflate] {
            let decoded = Decoder::new(encoding).finish(10).unwrap();
            assert!(decoded.bytes.is_empty() && !decoded.full);
        }
        let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
        gzip.write_all(b"hello hello hello").unwrap();
        let whole = gzip.finish().unwrap();
        let mut decoder = Decoder::new(Encoding::Gzip);
        decoder.feed(&whole[..whole.len() - 4], 100).unwrap();
        assert!(decoder.finish(100).is_err());
    }
}
