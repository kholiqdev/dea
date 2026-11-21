//! The last response of each tab, kept in Rust and served by page.

use std::io::Write as _;

use bytes::Bytes;
use dea_engine::{BodyKind, BodyText, Found, ResponseError, ResponseStore, Sent, TextView};
use dea_http::{Response, ResponseBody, Timing};

fn sent(content_type: &str, body: ResponseBody) -> Sent {
    Sent {
        response: Response {
            version: dea_http::HttpVersion::Http11,
            status: 200,
            reason: "OK".into(),
            headers: vec![("content-type".into(), content_type.into())],
            body,
            truncated: false,
            content_encoding: None,
            timing: Timing::default(),
            url: "http://example.test/".into(),
            redirects: Vec::new(),
        },
        url: "http://example.test/".into(),
        cookies_not_saved: None,
    }
}

fn in_memory(content_type: &str, text: &str) -> Sent {
    sent(
        content_type,
        ResponseBody::Memory(Bytes::from(text.to_owned())),
    )
}

fn spilled_bytes(content_type: &str, bytes: &[u8]) -> Sent {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    let path = file.into_temp_path();
    sent(
        content_type,
        ResponseBody::File {
            path,
            len: bytes.len() as u64,
        },
    )
}

fn spilled(text: &str) -> Sent {
    spilled_bytes("text/plain", text.as_bytes())
}

#[test]
fn one_long_line_is_split_into_rows() {
    let body = format!("[{}]", "1,".repeat(6 * 1024 * 1024)); // ~12 MiB, one line
    let store = ResponseStore::default();
    store.insert("w/t", spilled(&body));
    let page = store.rows("w/t", 0, 500).unwrap();
    assert_eq!(page.total, (body.len() as u64).div_ceil(4096));
    assert!(page.rows.iter().all(|r| r.len() <= 4096));
}

#[test]
fn a_body_that_is_not_what_its_type_says_is_shown_as_received() {
    let store = ResponseStore::default();
    store.insert(
        "w/t",
        in_memory("application/json", "<html><body>hi</body></html>"),
    );
    let text = store.text("w/t", TextView::Pretty).unwrap();
    assert_eq!(
        text,
        BodyText {
            text: "<html><body>hi</body></html>".into(),
            formatted: false
        }
    );
}

#[test]
fn rows_follow_newlines_and_crlf() {
    let store = ResponseStore::default();
    store.insert("w/t", in_memory("text/plain", "a\r\nbb\n\nccc"));
    let page = store.rows("w/t", 0, 10).unwrap();
    assert_eq!(page.total, 4);
    assert_eq!(page.rows, ["a", "bb", "", "ccc"]);
    let tail = store.rows("w/t", 2, 10).unwrap();
    assert_eq!((tail.from, tail.rows), (2, vec!["".into(), "ccc".into()]));
    // A trailing newline does not make an empty last row.
    store.insert("w/t", in_memory("text/plain", "a\n"));
    assert_eq!(store.rows("w/t", 0, 10).unwrap().total, 1);
    // Past the end there is nothing, and a count is capped.
    assert!(store.rows("w/t", 9, 10).unwrap().rows.is_empty());
}

#[test]
fn rows_never_split_a_character() {
    // `é` (two bytes) starts at byte 4095, so it ends past the 4096 limit.
    let body = format!("{}é{}", "a".repeat(4095), "b".repeat(10));
    let store = ResponseStore::default();
    store.insert("w/t", in_memory("text/plain", &body));
    let page = store.rows("w/t", 0, 10).unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.rows[0], "a".repeat(4095));
    assert_eq!(page.rows[1], format!("é{}", "b".repeat(10)));
    // Nothing was replaced.
    assert!(page.rows.iter().all(|r| !r.contains('\u{FFFD}')));
}

#[test]
fn rows_count_is_capped_at_500() {
    let body = "x\n".repeat(1000);
    let store = ResponseStore::default();
    store.insert("w/t", in_memory("text/plain", &body));
    let page = store.rows("w/t", 0, 100_000).unwrap();
    assert_eq!((page.total, page.rows.len()), (1000, 500));
}

#[test]
fn a_new_response_replaces_and_deletes_the_old_file() {
    let store = ResponseStore::default();
    let old = spilled("old");
    let ResponseBody::File { path, .. } = &old.response.body else {
        unreachable!()
    };
    let old_path = path.to_path_buf();
    store.insert("w/t", old);
    assert!(old_path.exists());
    store.insert("w/t", in_memory("text/plain", "new"));
    assert!(!old_path.exists());
    assert_eq!(store.text("w/t", TextView::Raw).unwrap().text, "new");
}

#[test]
fn release_drops_one_key_or_a_prefix() {
    let store = ResponseStore::default();
    for key in ["w1/a", "w1/b", "w2/a"] {
        store.insert(key, in_memory("text/plain", "x"));
    }
    store.release("w1/a");
    assert_eq!(
        store.text("w1/a", TextView::Raw).unwrap_err(),
        ResponseError::NotFound
    );
    store.release_prefix("w1/");
    assert!(store.text("w1/b", TextView::Raw).is_err());
    assert!(store.text("w2/a", TextView::Raw).is_ok());
    assert_eq!(
        ResponseError::NotFound.to_string(),
        "the response is no longer kept"
    );
}

#[test]
fn large_bodies_refuse_full_text() {
    let store = ResponseStore::default();
    store.insert("w/t", spilled("{\"a\":1}"));
    assert_eq!(
        store.text("w/t", TextView::Raw).unwrap_err(),
        ResponseError::TooLarge
    );
    assert_eq!(
        store.filter("w/t", "$.a").unwrap_err(),
        ResponseError::TooLarge
    );
    assert_eq!(store.bytes("w/t").unwrap_err(), ResponseError::TooLarge);
}

#[test]
fn pretty_formats_json_and_markup_and_refuses_binary() {
    let store = ResponseStore::default();
    let info = store.insert("w/t", in_memory("application/json", "{\"a\":[1,2]}"));
    assert_eq!(info.kind, BodyKind::Json);
    let pretty = store.text("w/t", TextView::Pretty).unwrap();
    assert!(pretty.formatted && pretty.text.contains("\n"));
    assert_eq!(
        store.text("w/t", TextView::Raw).unwrap().text,
        "{\"a\":[1,2]}"
    );

    store.insert("w/t", in_memory("text/html", "<div><p>hi</p></div>"));
    assert!(store.text("w/t", TextView::Pretty).unwrap().formatted);

    store.insert("w/t", in_memory("text/plain", "plain"));
    let plain = store.text("w/t", TextView::Pretty).unwrap();
    assert_eq!((plain.text.as_str(), plain.formatted), ("plain", false));

    store.insert("w/t", in_memory("image/png", "\u{89}PNG"));
    assert_eq!(
        store.text("w/t", TextView::Raw).unwrap_err(),
        ResponseError::NotText
    );
    assert!(store.bytes("w/t").is_ok());
}

#[test]
fn invalid_utf8_is_replaced() {
    // The first 512 bytes are text, so the body is; a bad byte after them
    // is replaced.
    let text = "ok ".repeat(200);
    let mut body = text.clone().into_bytes();
    body.push(0xFF);
    let store = ResponseStore::default();
    store.insert(
        "w/t",
        sent(
            "text/plain; charset=utf-8",
            ResponseBody::Memory(Bytes::from(body)),
        ),
    );
    assert_eq!(
        store.text("w/t", TextView::Raw).unwrap().text,
        format!("{text}\u{FFFD}")
    );
}

#[test]
fn filter_runs_a_path_over_json_only() {
    let store = ResponseStore::default();
    store.insert("w/t", in_memory("application/json", "{\"a\":[1,2]}"));
    let filtered = store.filter("w/t", "$.a[*]").unwrap();
    assert_eq!(filtered.count, 2);
    assert!(matches!(
        store.filter("w/t", "$[").unwrap_err(),
        ResponseError::InvalidPath(_)
    ));
    store.insert("w/t", in_memory("text/plain", "hello"));
    assert_eq!(
        store.filter("w/t", "$.a").unwrap_err(),
        ResponseError::NotJson
    );
}

#[test]
fn find_walks_rows_both_ways() {
    let store = ResponseStore::default();
    store.insert(
        "w/t",
        in_memory("text/plain", "alpha\nBeta needle\ngamma\nÉcole NEEDLE\nend"),
    );
    let found = |from, back| store.find("w/t", "needle", from, back).unwrap();
    assert_eq!(
        found(0, false),
        Some(Found {
            row: 1,
            start: 5,
            end: 11
        })
    );
    assert_eq!(
        found(2, false),
        Some(Found {
            row: 3,
            start: 6,
            end: 12
        })
    );
    assert_eq!(found(4, false), None);
    assert_eq!(found(2, true).map(|f| f.row), Some(1));
    assert_eq!(found(4, true).map(|f| f.row), Some(3));
    assert_eq!(found(0, true), None);
    // Matches the accent and the case.
    assert_eq!(
        store.find("w/t", "école", 0, false).unwrap().map(|f| f.row),
        Some(3)
    );
    assert_eq!(store.find("w/t", "", 0, false).unwrap(), None);
    // A match across two rows is not found.
    assert_eq!(store.find("w/t", "alpha\nbeta", 0, false).unwrap(), None);
}

#[test]
fn save_to_copies_the_whole_body() {
    let dir = tempfile::tempdir().unwrap();
    let store = ResponseStore::default();
    store.insert("m", in_memory("text/plain", "in memory"));
    let big = vec![b'z'; 3 * 1024 * 1024];
    store.insert("f", spilled_bytes("application/octet-stream", &big));

    let a = dir.path().join("a.txt");
    store.save_to("m", &a).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), b"in memory");
    let b = dir.path().join("b.bin");
    store.save_to("f", &b).unwrap();
    assert_eq!(std::fs::read(&b).unwrap(), big);
    // Saving over an existing file replaces it.
    store.save_to("m", &b).unwrap();
    assert_eq!(std::fs::read(&b).unwrap(), b"in memory");
    assert_eq!(
        store.save_to("nope", &a).unwrap_err(),
        ResponseError::NotFound
    );
}

#[test]
fn a_100_mb_body_is_paged_without_loading_it() {
    // 1,048,576 lines of 100 bytes: `line NNNNNNN` padded, then a newline.
    let mut file = tempfile::NamedTempFile::new().unwrap();
    let mut block = String::new();
    for n in 0..1024 {
        block.push_str(&format!("{:<99}\n", format!("line {n:07}")));
    }
    for _ in 0..1024 {
        file.write_all(block.as_bytes()).unwrap();
    }
    let len = file.as_file().metadata().unwrap().len();
    assert_eq!(len, 100 * 1024 * 1024);
    let path = file.into_temp_path();
    let store = ResponseStore::default();
    let info = store.insert("w/t", sent("text/plain", ResponseBody::File { path, len }));
    assert!(info.large);
    let page = store.rows("w/t", 500_000, 3).unwrap();
    assert_eq!(page.total, 1_048_576);
    assert_eq!(page.rows.len(), 3);
    // Rows repeat every 1024 lines, so row 500_000 is line 500_000 % 1024.
    assert_eq!(
        page.rows[0].trim_end(),
        format!("line {:07}", 500_000 % 1024)
    );
    assert_eq!(
        store.text("w/t", TextView::Raw).unwrap_err(),
        ResponseError::TooLarge
    );
    let found = store.find("w/t", "line 0001023", 700_000, false).unwrap();
    assert!(found.is_some_and(|f| f.row >= 700_000));
}
