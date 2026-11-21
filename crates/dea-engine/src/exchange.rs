//! What the UI is told about a send: progress while the response arrives
//! ([`EventForwarder`]), the response once it is complete
//! ([`response_info`]), or why there is none ([`outcome_of_error`]).
//! Secret values never get here: [`RequestService::send`] hides them in
//! errors and redirect URLs first.
//!
//! [`RequestService::send`]: crate::RequestService::send

use std::fmt;
use std::fs::File;
use std::io::{self, Read as _};
use std::path::Path;
use std::time::{Duration, Instant};

use dea_http::{
    CookieJar, Event, HttpError, Phase, ResponseBody, StoredCookie, Timing, TlsError, TlsErrorKind,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use url::Url;

use crate::error::UseError;
use crate::requests::{SendError, Sent};

/// Body bytes sent to the UI as text while a response arrives. Past this,
/// only the count of bytes received is.
pub const TEXT_PREVIEW_LIMIT: u64 = 1024 * 1024;

/// How often the count of bytes received goes out once the text stopped.
const COUNT_EVERY: Duration = Duration::from_millis(100);

/// How much of a body [`body_kind`] looks at.
const SNIFF_BYTES: usize = 512;

/// One response header.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct HeaderRow {
    pub name: String,
    pub value: String,
}

/// Shows the name only; the value may be a secret.
impl fmt::Debug for HeaderRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeaderRow")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// An earlier hop that answered with a redirect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct RedirectInfo {
    pub status: u16,
    /// Every secret shown as `••••••••`.
    pub url: String,
}

/// How long each phase of the final hop took, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct TimingInfo {
    pub dns_ms: f64,
    pub connect_ms: f64,
    pub tls_ms: f64,
    pub waiting_ms: f64,
    pub download_ms: f64,
    pub total_ms: f64,
    /// The connection came from the pool: DNS, connect and TLS are zero.
    pub reused: bool,
}

impl From<Timing> for TimingInfo {
    fn from(t: Timing) -> Self {
        TimingInfo {
            dns_ms: ms(t.dns),
            connect_ms: ms(t.connect),
            tls_ms: ms(t.tls),
            waiting_ms: ms(t.waiting),
            download_ms: ms(t.download),
            total_ms: ms(t.total),
            reused: t.reused,
        }
    }
}

/// How the response pane shows a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum BodyKind {
    Empty,
    Json,
    Xml,
    Html,
    Text,
    Image,
    Pdf,
    Binary,
    /// A `text/event-stream`, shown as its events.
    Events,
}

/// One cookie, as a response set it or as the cookie manager edits it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// RFC 3339, `None` for a session cookie.
    pub expires: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    /// Sent to `domain` only, not to its subdomains.
    pub host_only: bool,
}

/// Names one cookie of a jar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct CookieKey {
    pub domain: String,
    pub path: String,
    pub name: String,
}

/// The value is a secret.
impl fmt::Debug for CookieInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieInfo")
            .field("name", &self.name)
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("expires", &self.expires)
            .field("secure", &self.secure)
            .field("http_only", &self.http_only)
            .field("host_only", &self.host_only)
            .finish_non_exhaustive()
    }
}

impl From<&StoredCookie> for CookieInfo {
    fn from(cookie: &StoredCookie) -> Self {
        CookieInfo {
            name: cookie.name.clone(),
            value: cookie.value.clone(),
            domain: cookie.domain.clone(),
            path: cookie.path.clone(),
            expires: cookie.expires.and_then(|at| at.format(&Rfc3339).ok()),
            secure: cookie.secure,
            http_only: cookie.http_only,
            host_only: cookie.host_only,
        }
    }
}

impl CookieInfo {
    /// The cookie as the jar keeps it.
    ///
    /// # Errors
    ///
    /// When `expires` is not an RFC 3339 date.
    pub fn to_stored(&self) -> Result<StoredCookie, String> {
        let expires = match &self.expires {
            None => None,
            Some(text) => Some(
                OffsetDateTime::parse(text, &Rfc3339)
                    .map_err(|_| "the cookie has an invalid expiry date".to_string())?,
            ),
        };
        Ok(StoredCookie {
            name: self.name.clone(),
            value: self.value.clone(),
            domain: self.domain.clone(),
            path: self.path.clone(),
            expires,
            secure: self.secure,
            http_only: self.http_only,
            host_only: self.host_only,
        })
    }
}

/// A complete response, without its body, which the UI asks for apart.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct ResponseInfo {
    pub status: u16,
    pub reason: String,
    /// `HTTP/1.1` or `HTTP/2`.
    pub version: String,
    /// In the order received, repeats kept.
    pub headers: Vec<HeaderRow>,
    /// The cookies the final response set.
    pub cookies: Vec<CookieInfo>,
    /// Body bytes, after decoding.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size: u64,
    pub content_type: Option<String>,
    pub kind: BodyKind,
    /// The body is in a temporary file, not in memory.
    pub large: bool,
    /// The body stopped at the size limit.
    pub truncated: bool,
    pub timing: TimingInfo,
    /// Earlier hops, oldest first.
    pub redirects: Vec<RedirectInfo>,
    /// Why the cookies of this exchange could not be saved.
    pub cookies_not_saved: Option<String>,
}

/// Progress of a send, for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum SendEvent {
    /// The request is about to leave. Sent by the caller, before any other.
    Started {
        method: String,
        /// Every secret hidden.
        url: String,
    },
    /// A phase of opening a connection is done.
    Phase {
        /// `dns`, `connect` or `tls`.
        phase: String,
        ms: f64,
    },
    /// The hop uses a connection from the pool.
    Reused,
    Head {
        status: u16,
        reason: String,
        headers: Vec<HeaderRow>,
    },
    /// More of the body arrived.
    Body {
        /// Body bytes so far.
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        received: u64,
        /// The text of this piece, while the body is within
        /// [`TEXT_PREVIEW_LIMIT`] and not an event stream.
        text: Option<String>,
    },
    /// One event of a `text/event-stream`.
    Event {
        /// Counted from the start of the request.
        at_ms: f64,
        event: Option<String>,
        data: String,
        id: Option<String>,
    },
}

/// Why a send ended without a response, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub struct SendFailure {
    pub kind: FailureKind,
    pub title: String,
    pub message: String,
    /// The Node.js style code, such as `ENOTFOUND`.
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum FailureKind {
    Dns,
    Refused,
    Connect,
    Tls,
    Timeout,
    Redirects,
    Proxy,
    File,
    Protocol,
    InvalidUrl,
    Unresolved,
    Vault,
    Request,
}

/// How a send ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(
    feature = "ts",
    derive(ts_rs::TS),
    ts(export, export_to = "../../../apps/desktop/src/lib/ipc/bindings/")
)]
pub enum SendOutcome {
    Response {
        response: ResponseInfo,
    },
    Failed {
        failure: SendFailure,
    },
    /// The person cancelled the send while it ran.
    Cancelled,
    /// The person cancelled the production confirmation or the Vault
    /// dialog, so nothing was sent.
    NotConfirmed,
}

/// What the UI shows about a response that arrived. Received cookies are
/// read from the final response's `Set-Cookie` headers the way the jar
/// reads them, a cookie without `Domain` belonging to the host of
/// [`Sent::url`]. A large body's kind is read from the start of its file.
#[must_use]
pub fn response_info(sent: &Sent) -> ResponseInfo {
    let response = &sent.response;
    let content_type = header(&response.headers, "content-type").map(str::to_string);
    let kind = match &response.body {
        ResponseBody::Memory(bytes) => body_kind(
            content_type.as_deref(),
            &bytes[..bytes.len().min(SNIFF_BYTES)],
        ),
        ResponseBody::File { path, .. } => match first_bytes(path) {
            Ok(head) => body_kind(content_type.as_deref(), &head),
            // A viewer could not read it either.
            Err(_) => content_type
                .as_deref()
                .and_then(kind_of_type)
                .unwrap_or(BodyKind::Binary),
        },
    };
    ResponseInfo {
        status: response.status,
        reason: response.reason.clone(),
        version: response.version.to_string(),
        headers: rows(&response.headers),
        cookies: received_cookies(&sent.url, &response.headers),
        size: response.body.len(),
        content_type,
        kind,
        large: matches!(response.body, ResponseBody::File { .. }),
        truncated: response.truncated,
        timing: response.timing.into(),
        redirects: response
            .redirects
            .iter()
            .map(|r| RedirectInfo {
                status: r.status,
                url: r.url.clone(),
            })
            .collect(),
        cookies_not_saved: sent.cookies_not_saved.as_ref().map(ToString::to_string),
    }
}

/// How to show a body of `content_type` that starts with `head`, its first
/// bytes (up to 512). An empty body is [`BodyKind::Empty`] whatever its
/// type. Otherwise the type decides when it names one, then the bytes:
/// `%PDF-`, the PNG, JPEG, GIF and WebP signatures, valid UTF-8, or
/// [`BodyKind::Binary`]. A plain `text/*` body whose first bytes are not
/// UTF-8 is [`BodyKind::Binary`], so it is not shown as garbled text.
#[must_use]
pub fn body_kind(content_type: Option<&str>, head: &[u8]) -> BodyKind {
    if head.is_empty() {
        return BodyKind::Empty;
    }
    match content_type.and_then(kind_of_type) {
        Some(BodyKind::Text) if !is_text(head) => return BodyKind::Binary,
        Some(kind) => return kind,
        None => {}
    }
    const IMAGES: [&[u8]; 4] = [b"\x89PNG\r\n\x1a\n", b"\xFF\xD8\xFF", b"GIF87a", b"GIF89a"];
    let webp = head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP";
    if head.starts_with(b"%PDF-") {
        BodyKind::Pdf
    } else if webp || IMAGES.iter().any(|magic| head.starts_with(magic)) {
        BodyKind::Image
    } else if is_text(head) {
        BodyKind::Text
    } else {
        BodyKind::Binary
    }
}

/// The kind a `Content-Type` names, parameters and case aside.
fn kind_of_type(content_type: &str) -> Option<BodyKind> {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let (top, sub) = essence.split_once('/')?;
    Some(match (top, sub) {
        ("text", "event-stream") => BodyKind::Events,
        ("application", "json") => BodyKind::Json,
        (_, sub) if sub.ends_with("+json") => BodyKind::Json,
        // An image first, though it is XML.
        ("image", "svg+xml") => BodyKind::Image,
        ("application" | "text", "xml") => BodyKind::Xml,
        (_, sub) if sub.ends_with("+xml") => BodyKind::Xml,
        ("text", "html") => BodyKind::Html,
        ("image", _) => BodyKind::Image,
        ("application", "pdf") => BodyKind::Pdf,
        ("text", _) => BodyKind::Text,
        _ => return None,
    })
}

/// Valid UTF-8, allowing a character cut off at the end.
fn is_text(head: &[u8]) -> bool {
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none(),
    }
}

fn first_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(SNIFF_BYTES);
    File::open(path)?
        .take(SNIFF_BYTES as u64)
        .read_to_end(&mut head)?;
    Ok(head)
}

/// The cookies `headers` set, parsed by a jar of their own so they read
/// as the project's jar keeps them.
fn received_cookies(url: &str, headers: &[(String, String)]) -> Vec<CookieInfo> {
    let set_cookie: Vec<&str> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.as_str())
        .collect();
    if set_cookie.is_empty() {
        return Vec::new();
    }
    let Ok(url) = Url::parse(url) else {
        return Vec::new();
    };
    let jar = CookieJar::new();
    jar.store(&url, &set_cookie);
    jar.list().iter().map(CookieInfo::from).collect()
}

/// Turns the client's events into [`SendEvent`]s, one send at a time:
/// connection phases, the head and event-stream events always; body text
/// while the body is within [`TEXT_PREVIEW_LIMIT`], then the count of
/// bytes received at most every 100 ms.
#[derive(Debug, Default)]
pub struct EventForwarder {
    /// Body bytes so far.
    received: u64,
    /// The start of a character split between pieces, until the rest
    /// arrives.
    split: Vec<u8>,
    /// The response is a `text/event-stream`: its events go out, its text
    /// does not.
    stream: bool,
    /// When the last count without text went out.
    counted_at: Option<Instant>,
}

impl EventForwarder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The event for the UI, if `event`, arriving at `now`, makes one.
    pub fn forward(&mut self, event: Event, now: Instant) -> Option<SendEvent> {
        match event {
            Event::Phase { phase, took } => Some(SendEvent::Phase {
                phase: match phase {
                    Phase::Dns => "dns",
                    Phase::Connect => "connect",
                    Phase::Tls => "tls",
                }
                .to_string(),
                ms: ms(took),
            }),
            Event::Reused => Some(SendEvent::Reused),
            Event::Head {
                status,
                reason,
                headers,
            } => {
                self.stream = header(&headers, "content-type")
                    .and_then(kind_of_type)
                    .is_some_and(|kind| kind == BodyKind::Events);
                Some(SendEvent::Head {
                    status,
                    reason,
                    headers: rows(&headers),
                })
            }
            Event::Sse(event) => Some(SendEvent::Event {
                at_ms: ms(event.at),
                event: event.event,
                data: event.data,
                id: event.id,
            }),
            Event::Data(bytes) => self.data(&bytes, now),
        }
    }

    fn data(&mut self, bytes: &[u8], now: Instant) -> Option<SendEvent> {
        self.received += bytes.len() as u64;
        if !self.stream && self.received <= TEXT_PREVIEW_LIMIT {
            return Some(SendEvent::Body {
                received: self.received,
                text: Some(self.text(bytes)),
            });
        }
        self.split = Vec::new();
        if self
            .counted_at
            .is_some_and(|at| now.saturating_duration_since(at) < COUNT_EVERY)
        {
            return None;
        }
        self.counted_at = Some(now);
        Some(SendEvent::Body {
            received: self.received,
            text: None,
        })
    }

    /// The text of `bytes` after what was held back, holding back a
    /// character that is not complete yet.
    fn text(&mut self, bytes: &[u8]) -> String {
        self.split.extend_from_slice(bytes);
        let whole = self.split.len() - incomplete_tail(&self.split);
        let text = String::from_utf8_lossy(&self.split[..whole]).into_owned();
        self.split.drain(..whole);
        text
    }
}

/// How many bytes at the end of `bytes` begin a character whose other
/// bytes have not arrived.
fn incomplete_tail(bytes: &[u8]) -> usize {
    // A character takes at most 4 bytes, so it starts in the last 3.
    for back in 1..=bytes.len().min(3) {
        let byte = bytes[bytes.len() - back];
        if byte & 0b1100_0000 == 0b1000_0000 {
            continue;
        }
        let width = match byte {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => 1,
        };
        return if width > back { back } else { 0 };
    }
    0
}

/// How the UI ends a send that returned `error`.
#[must_use]
pub fn outcome_of_error(error: &SendError) -> SendOutcome {
    let failure = match error {
        SendError::NotConfirmed | SendError::Secret(UseError::Cancelled) => {
            return SendOutcome::NotConfirmed;
        }
        SendError::Http(error) => match http_failure(error) {
            Some(failure) => failure,
            None => return SendOutcome::Cancelled,
        },
        SendError::Request(error) => failure(
            FailureKind::Request,
            "Request not valid",
            error.to_string(),
            None,
        ),
        SendError::Unresolved(names) => failure(
            FailureKind::Unresolved,
            "The URL has undefined variables",
            unresolved_message(names),
            None,
        ),
        SendError::InvalidUrl(url) => failure(
            FailureKind::InvalidUrl,
            "URL not valid",
            format!("{url} is not a valid URL."),
            None,
        ),
        SendError::Secret(error) => failure(FailureKind::Vault, "Vault", error.to_string(), None),
        SendError::File { path, message } => failure(
            FailureKind::File,
            "File not readable",
            format!("{}: {message}", path.display()),
            None,
        ),
    };
    SendOutcome::Failed { failure }
}

/// `None` for [`HttpError::Cancelled`], which is not a failure.
fn http_failure(error: &HttpError) -> Option<SendFailure> {
    Some(match error {
        HttpError::Dns { host, .. } => failure(
            FailureKind::Dns,
            "DNS: host not found",
            format!("{host} could not be resolved. Check the host name."),
            Some("ENOTFOUND"),
        ),
        HttpError::ConnectionRefused { addr } => failure(
            FailureKind::Refused,
            "Connection refused",
            format!("Nothing is listening on {addr}."),
            Some("ECONNREFUSED"),
        ),
        HttpError::Connect { addr, message } => failure(
            FailureKind::Connect,
            "Could not connect",
            format!("{addr}: {message}"),
            None,
        ),
        HttpError::Tls(error) => tls_failure(error),
        HttpError::Timeout(after) => failure(
            FailureKind::Timeout,
            "Timed out",
            format!(
                "No answer after {} ms, the timeout for this request.",
                thousands(after.as_millis())
            ),
            Some("ETIMEDOUT"),
        ),
        HttpError::TooManyRedirects(n) => failure(
            FailureKind::Redirects,
            "Too many redirects",
            format!("Stopped after {n} redirects."),
            None,
        ),
        HttpError::Proxy(message) => {
            failure(FailureKind::Proxy, "Proxy error", message.clone(), None)
        }
        HttpError::File { path, message } => failure(
            FailureKind::File,
            "File not readable",
            format!("{}: {message}", path.display()),
            None,
        ),
        HttpError::Protocol(message) => failure(
            FailureKind::Protocol,
            "The answer could not be read",
            message.clone(),
            None,
        ),
        HttpError::InvalidRequest(message) => failure(
            FailureKind::Request,
            "Request not valid",
            message.clone(),
            None,
        ),
        HttpError::Cancelled => return None,
    })
}

fn tls_failure(error: &TlsError) -> SendFailure {
    let (title, message, code) = match error.kind {
        TlsErrorKind::Expired => (
            "TLS: certificate expired",
            error.message.clone(),
            Some("CERT_HAS_EXPIRED"),
        ),
        TlsErrorKind::NotYetValid => (
            "TLS: certificate not valid yet",
            error.message.clone(),
            Some("CERT_NOT_YET_VALID"),
        ),
        TlsErrorKind::UnknownIssuer => (
            "TLS: certificate not trusted",
            "Add its CA in the collection settings, or turn off Verify TLS for this request."
                .to_string(),
            Some("UNABLE_TO_VERIFY_LEAF_SIGNATURE"),
        ),
        TlsErrorKind::WrongHost => (
            "TLS: certificate is for another host",
            error.message.clone(),
            Some("ERR_TLS_CERT_ALTNAME_INVALID"),
        ),
        TlsErrorKind::ClientCertificateRejected => (
            "TLS: client certificate rejected",
            "The server did not accept the client certificate. Check it in the collection \
             settings."
                .to_string(),
            None,
        ),
        TlsErrorKind::Other => ("TLS handshake failed", error.message.clone(), None),
    };
    failure(FailureKind::Tls, title, message, code)
}

fn failure(kind: FailureKind, title: &str, message: String, code: Option<&str>) -> SendFailure {
    SendFailure {
        kind,
        title: title.to_string(),
        message,
        code: code.map(str::to_string),
    }
}

/// `a has no value…`, or `a, b and c have no value…`.
fn unresolved_message(names: &[String]) -> String {
    match names {
        [one] => format!("{one} has no value, so the URL has no host."),
        [first @ .., last] => format!(
            "{} and {last} have no value, so the URL has no host.",
            first.join(", ")
        ),
        [] => "The URL has no host.".to_string(),
    }
}

/// `n` with `,` between thousands: `8000` is `8,000`.
fn thousands(n: u128) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn rows(headers: &[(String, String)]) -> Vec<HeaderRow> {
    headers
        .iter()
        .map(|(name, value)| HeaderRow {
            name: name.clone(),
            value: value.clone(),
        })
        .collect()
}
