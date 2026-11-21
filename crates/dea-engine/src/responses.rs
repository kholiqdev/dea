//! The last response of each tab, kept in Rust. The body stays in memory
//! (up to 10 MiB) or in the temporary file `dea-http` spilled it to; the UI
//! asks only for what it shows: the whole text (small bodies only), the raw
//! bytes (previews), a page of rows (the virtual viewer), a search, a
//! JSONPath filter, or a copy to a file.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write as _};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use dea_http::ResponseBody;
use serde::{Deserialize, Serialize};

use crate::exchange::{BodyKind, ResponseInfo, response_info};
use crate::json_filter::{FilterError, Filtered, filter_json};
use crate::pretty::{pretty_json, pretty_markup};
use crate::requests::Sent;

/// The most bytes in one row of the viewer.
pub const ROW_BYTES: usize = 4096;

/// The most rows one [`ResponseStore::rows`] call returns.
const MAX_ROWS: u32 = 500;

/// How much of a file is read at once while indexing it.
const CHUNK: usize = 256 * 1024;

/// How the text of a body is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum TextView {
    Raw,
    Pretty,
}

/// The text of a body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct BodyText {
    pub text: String,
    /// The text was reformatted; `false` when it is as received (a body
    /// that is not what its type says, or a type with nothing to format).
    pub formatted: bool,
}

/// A page of rows of a body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct RowsPage {
    /// Rows in the whole body.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub from: u64,
    pub rows: Vec<String>,
}

/// Where a search matched; `start` and `end` count characters of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct Found {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub row: u64,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResponseError {
    #[error("the response is no longer kept")]
    NotFound,
    #[error("the body is too large to load whole")]
    TooLarge,
    #[error("the body is not text")]
    NotText,
    #[error("the body is not JSON")]
    NotJson,
    #[error("invalid JSONPath: {0}")]
    InvalidPath(String),
    #[error("could not read or write the body: {0}")]
    Io(String),
}

impl From<io::Error> for ResponseError {
    fn from(error: io::Error) -> Self {
        ResponseError::Io(error.to_string())
    }
}

/// One kept body, with what is worked out about it once.
struct Entry {
    body: ResponseBody,
    kind: BodyKind,
    large: bool,
    /// The byte offset where each row starts, built by the first call that
    /// needs rows.
    index: Mutex<Option<Arc<Vec<u64>>>>,
}

impl Entry {
    fn bytes_in(&self, start: u64, end: u64) -> io::Result<Vec<u8>> {
        match &self.body {
            ResponseBody::Memory(bytes) => Ok(bytes[start as usize..end as usize].to_vec()),
            ResponseBody::File { path, .. } => {
                let mut file = File::open(path)?;
                file.seek(SeekFrom::Start(start))?;
                let mut out = vec![0; (end - start) as usize];
                file.read_exact(&mut out)?;
                Ok(out)
            }
        }
    }

    fn row_starts(&self) -> io::Result<Arc<Vec<u64>>> {
        let mut slot = lock(&self.index);
        if let Some(index) = &*slot {
            return Ok(Arc::clone(index));
        }
        let index = Arc::new(match &self.body {
            ResponseBody::Memory(bytes) => index_rows(&bytes[..])?,
            ResponseBody::File { path, .. } => index_rows(File::open(path)?)?,
        });
        *slot = Some(Arc::clone(&index));
        Ok(index)
    }

    fn text_of_rows(&self, from: u64, count: u64) -> io::Result<(u64, Vec<String>)> {
        let starts = self.row_starts()?;
        let total = starts.len() as u64;
        let last = from.saturating_add(count).min(total);
        if from >= last {
            return Ok((total, Vec::new()));
        }
        let byte_at = |row: u64| starts.get(row as usize).copied().unwrap_or(self.body.len());
        let base = byte_at(from);
        let bytes = self.bytes_in(base, byte_at(last))?;
        let rows = (from..last)
            .map(|row| {
                let slice =
                    &bytes[(byte_at(row) - base) as usize..(byte_at(row + 1) - base) as usize];
                row_text(slice)
            })
            .collect();
        Ok((total, rows))
    }
}

/// A row without its line ending, as text.
fn row_text(slice: &[u8]) -> String {
    let slice = slice.strip_suffix(b"\n").unwrap_or(slice);
    let slice = slice.strip_suffix(b"\r").unwrap_or(slice);
    String::from_utf8_lossy(slice).into_owned()
}

fn is_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

/// Finds where each row starts, reading `source` once in chunks. A row ends
/// after a `\n`, or after [`ROW_BYTES`] bytes without cutting a UTF-8
/// character (a row may be cut mid-character only when the bytes are not
/// valid UTF-8).
fn index_rows(mut source: impl Read) -> io::Result<Vec<u64>> {
    let mut starts: Vec<u64> = Vec::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut position: u64 = 0;
    let mut row_start: u64 = 0;
    // Where the last character that began in this row begins.
    let mut char_start: u64 = 0;
    loop {
        let read = match source.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if starts.is_empty() {
            starts.push(0);
        }
        for &byte in &buffer[..read] {
            let length = position - row_start;
            if length == ROW_BYTES as u64 {
                // The row is full and this byte is the one after it.
                if byte == b'\n' {
                    row_start = position + 1;
                } else if is_continuation(byte) && char_start > row_start {
                    row_start = char_start;
                } else {
                    row_start = position;
                }
                starts.push(row_start);
                if !is_continuation(byte) {
                    char_start = position;
                }
            } else if byte == b'\n' {
                row_start = position + 1;
                starts.push(row_start);
            } else if !is_continuation(byte) {
                char_start = position;
            }
            position += 1;
        }
    }
    // A newline at the very end starts no row.
    if starts.last() == Some(&position) && position > 0 {
        starts.pop();
    }
    Ok(starts)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where a row matched `query`, as characters of the row. Both sides are
/// lowercased with Unicode rules one character at a time, so positions map
/// back to the row even when a character lowercases to several.
fn find_in_row(row: &str, query: &[char]) -> Option<(u32, u32)> {
    let mut lowered: Vec<char> = Vec::new();
    // `owner[i]` is the index in `row` of the character `lowered[i]` came from.
    let mut owner: Vec<u32> = Vec::new();
    for (index, c) in row.chars().enumerate() {
        for l in c.to_lowercase() {
            lowered.push(l);
            owner.push(index as u32);
        }
    }
    if query.is_empty() || lowered.len() < query.len() {
        return None;
    }
    let at = lowered.windows(query.len()).position(|w| w == query)?;
    Some((owner[at], owner[at + query.len() - 1] + 1))
}

/// The last response of each key, where a key means nothing here (the
/// desktop uses `"<window>/<tab>"`).
#[derive(Default)]
pub struct ResponseStore {
    entries: Mutex<HashMap<String, Arc<Entry>>>,
}

impl ResponseStore {
    /// Keeps the body of `sent` under `key`, dropping the one it had (its
    /// temporary file is deleted), and says what to show of the response.
    pub fn insert(&self, key: &str, sent: Sent) -> ResponseInfo {
        let info = response_info(&sent);
        let entry = Arc::new(Entry {
            body: sent.response.body,
            kind: info.kind,
            large: info.large,
            index: Mutex::new(None),
        });
        let old = lock(&self.entries).insert(key.to_owned(), entry);
        drop(old);
        info
    }

    pub fn release(&self, key: &str) {
        let old = lock(&self.entries).remove(key);
        drop(old);
    }

    /// Releases every key that starts with `prefix` (a closed window).
    pub fn release_prefix(&self, prefix: &str) {
        let mut entries = lock(&self.entries);
        let gone: Vec<String> = entries
            .keys()
            .filter(|key| key.starts_with(prefix))
            .cloned()
            .collect();
        let old: Vec<_> = gone.iter().filter_map(|key| entries.remove(key)).collect();
        drop(entries);
        drop(old);
    }

    fn entry(&self, key: &str) -> Result<Arc<Entry>, ResponseError> {
        lock(&self.entries)
            .get(key)
            .cloned()
            .ok_or(ResponseError::NotFound)
    }

    /// The whole text of a body that fits in memory. `Pretty` formats JSON,
    /// XML and HTML; text that does not parse, and kinds with nothing to
    /// format, come back as received with `formatted: false`. Invalid UTF-8
    /// is replaced.
    pub fn text(&self, key: &str, view: TextView) -> Result<BodyText, ResponseError> {
        let entry = self.entry(key)?;
        if entry.large {
            return Err(ResponseError::TooLarge);
        }
        if matches!(
            entry.kind,
            BodyKind::Image | BodyKind::Pdf | BodyKind::Binary
        ) {
            return Err(ResponseError::NotText);
        }
        let bytes = entry.bytes_in(0, entry.body.len())?;
        let raw = String::from_utf8_lossy(&bytes).into_owned();
        let formatted = match (view, entry.kind) {
            (TextView::Raw, _) => None,
            (TextView::Pretty, BodyKind::Json) => pretty_json(&raw),
            (TextView::Pretty, BodyKind::Xml) => Some(pretty_markup(&raw, false)),
            (TextView::Pretty, BodyKind::Html) => Some(pretty_markup(&raw, true)),
            (TextView::Pretty, _) => None,
        };
        Ok(match formatted {
            Some(text) => BodyText {
                text,
                formatted: true,
            },
            None => BodyText {
                text: raw,
                formatted: false,
            },
        })
    }

    /// The raw bytes, for the preview of an image or a PDF.
    pub fn bytes(&self, key: &str) -> Result<Vec<u8>, ResponseError> {
        let entry = self.entry(key)?;
        if entry.large {
            return Err(ResponseError::TooLarge);
        }
        Ok(entry.bytes_in(0, entry.body.len())?)
    }

    /// Up to 500 rows from row `from`. A row ends at a line break (`\r\n`
    /// or `\n`, not included) or after [`ROW_BYTES`] bytes, never in the
    /// middle of a character. The first call reads the body once to find
    /// where the rows start.
    pub fn rows(&self, key: &str, from: u64, count: u32) -> Result<RowsPage, ResponseError> {
        let entry = self.entry(key)?;
        let (total, rows) = entry.text_of_rows(from, u64::from(count.min(MAX_ROWS)))?;
        Ok(RowsPage { total, from, rows })
    }

    /// The first row at or after `from_row` (at or before it when
    /// `backwards`) that holds `query`, ignoring case. One match per row:
    /// the first in it. A match that runs across two rows is not found,
    /// since a row is searched on its own. Case is folded one character at
    /// a time, so `ß` does not match `ss`.
    pub fn find(
        &self,
        key: &str,
        query: &str,
        from_row: u64,
        backwards: bool,
    ) -> Result<Option<Found>, ResponseError> {
        let entry = self.entry(key)?;
        let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
        if query.is_empty() {
            return Ok(None);
        }
        let total = entry.row_starts()?.len() as u64;
        if total == 0 {
            return Ok(None);
        }
        let batch = u64::from(MAX_ROWS);
        let search = |row: u64, text: &str| {
            find_in_row(text, &query).map(|(start, end)| Found { row, start, end })
        };
        if backwards {
            // `next` rows are left, counting down from the one after `top`.
            let mut next = from_row.min(total - 1) + 1;
            while next > 0 {
                let first = next.saturating_sub(batch);
                let (_, rows) = entry.text_of_rows(first, next - first)?;
                for (offset, text) in rows.iter().enumerate().rev() {
                    if let Some(found) = search(first + offset as u64, text) {
                        return Ok(Some(found));
                    }
                }
                next = first;
            }
        } else {
            let mut row = from_row;
            while row < total {
                let (_, rows) = entry.text_of_rows(row, batch)?;
                for (offset, text) in rows.iter().enumerate() {
                    if let Some(found) = search(row + offset as u64, text) {
                        return Ok(Some(found));
                    }
                }
                row += batch;
            }
        }
        Ok(None)
    }

    /// Runs a JSONPath over a JSON body that fits in memory.
    pub fn filter(&self, key: &str, path: &str) -> Result<Filtered, ResponseError> {
        let entry = self.entry(key)?;
        if entry.large {
            return Err(ResponseError::TooLarge);
        }
        if entry.kind != BodyKind::Json {
            return Err(ResponseError::NotJson);
        }
        let bytes = entry.bytes_in(0, entry.body.len())?;
        filter_json(&String::from_utf8_lossy(&bytes), path).map_err(|e| match e {
            FilterError::NotJson => ResponseError::NotJson,
            FilterError::InvalidPath(message) => ResponseError::InvalidPath(message),
        })
    }

    /// Writes the body, byte for byte, to `dest`. The bytes go to a
    /// temporary file in the same folder first and replace `dest` only when
    /// all are written, so a failure leaves `dest` as it was.
    pub fn save_to(&self, key: &str, dest: &Path) -> Result<(), ResponseError> {
        let entry = self.entry(key)?;
        let folder = match dest.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        let mut temp = tempfile::NamedTempFile::new_in(folder)?;
        match &entry.body {
            ResponseBody::Memory(bytes) => temp.write_all(bytes)?,
            ResponseBody::File { path, .. } => {
                io::copy(&mut File::open(path)?, &mut temp)?;
            }
        }
        temp.as_file().sync_all()?;
        temp.persist(dest)
            .map_err(|e| ResponseError::from(e.error))?;
        Ok(())
    }
}
