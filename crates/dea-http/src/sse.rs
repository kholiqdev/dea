//! Server-Sent Events, split incrementally as the WHATWG specification
//! describes.

use std::time::Duration;

/// One event of a `text/event-stream` body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// When the event was complete, counted from the start of the request.
    pub at: Duration,
    pub event: Option<String>,
    /// The `data` lines joined with `\n`.
    pub data: String,
    pub id: Option<String>,
}

/// Splits a stream into events as bytes arrive, in pieces of any size.
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    /// The bytes of the line being read.
    line: Vec<u8>,
    /// The last byte was a CR, so a LF right after it belongs to it.
    after_cr: bool,
    /// A line was read, so a byte order mark can no longer come.
    started: bool,
    /// `Some` once the event has a `data` line.
    data: Option<String>,
    event: Option<String>,
    id: Option<String>,
}

impl SseParser {
    /// Feeds `bytes`, which arrived `at` after the request began, and
    /// returns the events they completed.
    pub(crate) fn push(&mut self, bytes: &[u8], at: Duration) -> Vec<SseEvent> {
        let mut events = Vec::new();
        for &byte in bytes {
            if std::mem::take(&mut self.after_cr) && byte == b'\n' {
                continue;
            }
            match byte {
                b'\n' | b'\r' => {
                    self.after_cr = byte == b'\r';
                    let line = std::mem::take(&mut self.line);
                    if let Some(event) = self.line_done(&line, at) {
                        events.push(event);
                    }
                }
                _ => self.line.push(byte),
            }
        }
        events
    }

    fn line_done(&mut self, line: &[u8], at: Duration) -> Option<SseEvent> {
        let mut line = String::from_utf8_lossy(line);
        if !std::mem::replace(&mut self.started, true)
            && let Some(rest) = line.strip_prefix('\u{feff}')
        {
            line = rest.to_owned().into();
        }
        if line.is_empty() {
            return self.dispatch(at);
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = line.split_once(':').unwrap_or((&line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "data" => match &mut self.data {
                Some(data) => {
                    data.push('\n');
                    data.push_str(value);
                }
                None => self.data = Some(value.to_owned()),
            },
            "event" => self.event = Some(value.to_owned()),
            "id" if !value.contains('\0') => self.id = Some(value.to_owned()),
            _ => {}
        }
        None
    }

    fn dispatch(&mut self, at: Duration) -> Option<SseEvent> {
        let data = self.data.take();
        let event = self.event.take();
        let id = self.id.take();
        data.map(|data| SseEvent {
            at,
            event,
            data,
            id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Duration = Duration::from_millis(5);

    fn parse(input: &[u8]) -> Vec<SseEvent> {
        SseParser::default().push(input, AT)
    }

    fn data(events: &[SseEvent]) -> Vec<&str> {
        events.iter().map(|e| e.data.as_str()).collect()
    }

    #[test]
    fn data_lines_are_joined_with_a_newline() {
        let events = parse(b"data: one\ndata: two\n\n");
        assert_eq!(data(&events), ["one\ntwo"]);
        assert_eq!(events[0].at, AT);
    }

    #[test]
    fn comments_and_unknown_fields_are_ignored() {
        let events = parse(b": hi\nretry: 10\nfoo: bar\ndata: x\n\n");
        assert_eq!(data(&events), ["x"]);
        assert_eq!(events[0].event, None);
        assert_eq!(events[0].id, None);
    }

    #[test]
    fn event_and_id_are_kept() {
        let events = parse(b"event: tick\nid: 7\ndata: x\n\n");
        assert_eq!(events[0].event.as_deref(), Some("tick"));
        assert_eq!(events[0].id.as_deref(), Some("7"));
    }

    #[test]
    fn only_one_leading_space_is_removed_and_a_bare_field_is_empty() {
        let events = parse(b"data:  two\ndata\n\n");
        assert_eq!(data(&events), [" two\n"]);
    }

    #[test]
    fn a_blank_line_without_data_sends_nothing() {
        assert!(parse(b"event: tick\n\n\n").is_empty());
        // and the event name does not leak into the next event
        let events = parse(b"event: tick\n\ndata: x\n\n");
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn every_line_ending_works_even_split_across_pushes() {
        let mut parser = SseParser::default();
        let mut events = parser.push(b"data: a\r", AT);
        events.extend(parser.push(b"\n\r", AT));
        events.extend(parser.push(b"\ndata: b\r\rdata: c\n\n", AT));
        assert_eq!(data(&events), ["a", "b", "c"]);
    }

    #[test]
    fn bytes_may_arrive_one_at_a_time_and_split_a_character() {
        let input = "data: h\u{e9}llo\n\n".as_bytes();
        let mut parser = SseParser::default();
        let mut events = Vec::new();
        for byte in input {
            events.extend(parser.push(&[*byte], AT));
        }
        assert_eq!(data(&events), ["h\u{e9}llo"]);
    }

    #[test]
    fn an_unfinished_event_is_not_sent() {
        assert!(parse(b"data: x\n").is_empty());
    }

    #[test]
    fn a_leading_byte_order_mark_is_skipped() {
        let events = parse(b"\xEF\xBB\xBFdata: x\n\n");
        assert_eq!(data(&events), ["x"]);
    }

    #[test]
    fn events_keep_the_time_of_the_push_that_completed_them() {
        let mut parser = SseParser::default();
        let first = parser.push(b"data: a\n", Duration::from_millis(1));
        let second = parser.push(b"\ndata: b\n\n", Duration::from_millis(9));
        assert!(first.is_empty());
        assert_eq!(second[0].at, Duration::from_millis(9));
        assert_eq!(second[1].at, Duration::from_millis(9));
    }
}
