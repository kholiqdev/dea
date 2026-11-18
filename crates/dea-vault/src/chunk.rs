//! Long values split over several keychain entries. Windows Credential
//! Manager refuses blobs over 2 560 bytes, and an RSA-4096 private key is
//! about 3 200. Values longer than [`CHUNK_BYTES`] are stored as:
//!
//! - `<account>`: `dea1;<n>;` followed by the first chunk;
//! - `<account>#2` to `<account>#<n>`: the following chunks.
//!
//! Shorter values are stored as they are, unless they start with the header
//! text themselves.

use zeroize::Zeroizing;

use crate::VaultError;

/// Largest chunk, in bytes. Leaves room for the header under the Windows
/// limit.
pub(crate) const CHUNK_BYTES: usize = 2000;
const PREFIX: &[u8] = b"dea1;";

/// The entries to store for `value`, first entry first.
pub(crate) fn split(value: &[u8]) -> Vec<Vec<u8>> {
    if value.len() <= CHUNK_BYTES && !value.starts_with(PREFIX) {
        return vec![value.to_vec()];
    }
    let chunks: Vec<&[u8]> = if value.is_empty() {
        vec![&[]]
    } else {
        value.chunks(CHUNK_BYTES).collect()
    };
    let mut first = PREFIX.to_vec();
    first.extend_from_slice(format!("{};", chunks.len()).as_bytes());
    first.extend_from_slice(chunks[0]);
    let mut out = vec![first];
    out.extend(chunks[1..].iter().map(|c| c.to_vec()));
    out
}

/// Where the data starts in a first entry, and how many entries there are.
fn header(first: &[u8]) -> Option<(usize, usize)> {
    let rest = first.strip_prefix(PREFIX)?;
    let end = rest.iter().position(|b| *b == b';')?;
    let count: usize = std::str::from_utf8(&rest[..end]).ok()?.parse().ok()?;
    (count >= 1).then_some((PREFIX.len() + end + 1, count))
}

/// How many entries a value whose first entry is `first` uses.
pub(crate) fn chunk_count(first: &[u8]) -> usize {
    header(first).map_or(1, |(_, count)| count)
}

/// The whole value and its entry count. `read(i)` returns entry `#i`, for
/// `i` from 2.
pub(crate) fn join(
    first: &[u8],
    mut read: impl FnMut(usize) -> Result<Option<Vec<u8>>, VaultError>,
) -> Result<(Zeroizing<Vec<u8>>, usize), VaultError> {
    let Some((start, count)) = header(first) else {
        return Ok((Zeroizing::new(first.to_vec()), 1));
    };
    let mut value = Zeroizing::new(first[start..].to_vec());
    for i in 2..=count {
        let chunk = Zeroizing::new(read(i)?.ok_or_else(|| {
            VaultError::Corrupt(format!(
                "part {i} of a long value is missing from the keychain"
            ))
        })?);
        value.extend_from_slice(&chunk);
    }
    Ok((value, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rebuild(pieces: &[Vec<u8>]) -> Vec<u8> {
        join(&pieces[0], |i| Ok(Some(pieces[i - 1].clone())))
            .unwrap()
            .0
            .to_vec()
    }

    #[test]
    fn short_values_are_one_entry_without_header() {
        let pieces = split(b"abc123");
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0], b"abc123");
        assert_eq!(rebuild(&pieces), b"abc123");
        assert_eq!(chunk_count(&pieces[0]), 1);
    }

    #[test]
    fn long_values_split_at_chunk_bytes_and_join_back() {
        // Multibyte characters land on chunk boundaries; bytes are split
        // as bytes and only joined text is read as UTF-8.
        let value = "é".repeat(2048) + "end";
        let pieces = split(value.as_bytes());
        assert_eq!(pieces.len(), 3);
        assert!(pieces[0].starts_with(b"dea1;3;"));
        assert!(pieces[1..].iter().all(|p| p.len() <= CHUNK_BYTES));
        assert_eq!(chunk_count(&pieces[0]), 3);
        assert_eq!(rebuild(&pieces), value.as_bytes());
    }

    #[test]
    fn a_short_value_that_looks_like_a_header_survives() {
        let pieces = split(b"dea1;7;not really");
        assert_eq!(pieces.len(), 1);
        assert_eq!(rebuild(&pieces), b"dea1;7;not really");
    }

    #[test]
    fn a_missing_chunk_is_damage() {
        let value = vec![b'x'; CHUNK_BYTES * 2 + 1];
        let pieces = split(&value);
        let result = join(&pieces[0], |i| Ok((i == 2).then(|| pieces[1].clone())));
        assert!(matches!(result, Err(VaultError::Corrupt(_))));
    }
}
