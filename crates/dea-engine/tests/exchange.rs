//! What the UI is told about a send: progress events, the response, and
//! failures with their titles and codes.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bytes::Bytes;
use dea_engine::requests::SendError;
use dea_engine::{
    BodyKind, CookieInfo, EventForwarder, FailureKind, SendEvent, SendFailure, SendOutcome,
    UseError, body_kind, outcome_of_error,
};
use dea_http::{Event, HttpError, Phase, SseEvent, StoredCookie, TlsError, TlsErrorKind};
use dea_project::RequestError;
use serde::Serialize;
use time::OffsetDateTime;

#[test]
fn forwarder_stops_sending_text_after_one_mebibyte() {
    let mut f = EventForwarder::new();
    let t0 = Instant::now();
    let first = f.forward(Event::Data(Bytes::from(vec![b'a'; 1024 * 1024])), t0);
    assert!(matches!(first, Some(SendEvent::Body { text: Some(_), .. })));
    assert!(matches!(
        f.forward(Event::Data(Bytes::from_static(b"b")), t0),
        Some(SendEvent::Body { text: None, .. })
    ));
    assert!(
        f.forward(
            Event::Data(Bytes::from_static(b"c")),
            t0 + Duration::from_millis(50)
        )
        .is_none()
    );
    // A tenth of a second later, the count goes out again.
    let later = f.forward(
        Event::Data(Bytes::from_static(b"d")),
        t0 + Duration::from_millis(100),
    );
    assert!(
        matches!(later, Some(SendEvent::Body { received, text: None }) if received == 1024 * 1024 + 3),
        "{later:?}"
    );
}

#[test]
fn split_utf8_is_carried_to_the_next_chunk() {
    let mut f = EventForwarder::new();
    let now = Instant::now();
    let text = |event: Option<SendEvent>| match event {
        Some(SendEvent::Body {
            text: Some(text), ..
        }) => text,
        other => panic!("not body text: {other:?}"),
    };
    // `é` is C3 A9.
    let first = f.forward(Event::Data(Bytes::from_static(b"caf\xC3")), now);
    assert_eq!(text(first), "caf");
    let second = f.forward(Event::Data(Bytes::from_static(b"\xA9 ok")), now);
    assert_eq!(text(second), "é ok");
}

#[test]
fn head_phases_and_reuse_are_forwarded() {
    let mut f = EventForwarder::new();
    let now = Instant::now();
    let phase = f.forward(
        Event::Phase {
            phase: Phase::Tls,
            took: Duration::from_micros(1500),
        },
        now,
    );
    assert!(
        matches!(&phase, Some(SendEvent::Phase { phase, ms }) if phase == "tls" && *ms == 1.5),
        "{phase:?}"
    );
    assert!(matches!(
        f.forward(Event::Reused, now),
        Some(SendEvent::Reused)
    ));
    let head = f.forward(
        Event::Head {
            status: 201,
            reason: "Created".into(),
            headers: vec![("x-id".into(), "7".into())],
        },
        now,
    );
    let Some(SendEvent::Head {
        status,
        reason,
        headers,
    }) = head
    else {
        panic!("not a head: {head:?}");
    };
    assert_eq!((status, reason.as_str()), (201, "Created"));
    assert_eq!(
        (headers[0].name.as_str(), headers[0].value.as_str()),
        ("x-id", "7")
    );
}

#[test]
fn event_streams_send_events_not_text() {
    let mut f = EventForwarder::new();
    let now = Instant::now();
    f.forward(
        Event::Head {
            status: 200,
            reason: "OK".into(),
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
        },
        now,
    );
    let data = f.forward(Event::Data(Bytes::from_static(b"data: hi\n\n")), now);
    assert!(
        matches!(
            data,
            Some(SendEvent::Body {
                received: 10,
                text: None
            })
        ),
        "{data:?}"
    );
    let event = f.forward(
        Event::Sse(SseEvent {
            at: Duration::from_millis(250),
            event: Some("tick".into()),
            data: "hi".into(),
            id: None,
        }),
        now,
    );
    assert!(
        matches!(&event, Some(SendEvent::Event { at_ms, event: Some(name), data, id: None })
            if *at_ms == 250.0 && name == "tick" && data == "hi"),
        "{event:?}"
    );
}

#[test]
fn body_kind_follows_type_then_bytes() {
    let some = Some;
    // The type decides first, parameters and case aside.
    assert_eq!(body_kind(some("text/event-stream"), b"x"), BodyKind::Events);
    assert_eq!(
        body_kind(some("application/json; charset=utf-8"), b"x"),
        BodyKind::Json
    );
    assert_eq!(
        body_kind(some("application/problem+json"), b"x"),
        BodyKind::Json
    );
    assert_eq!(body_kind(some("application/xml"), b"x"), BodyKind::Xml);
    assert_eq!(body_kind(some("Text/XML"), b"x"), BodyKind::Xml);
    assert_eq!(body_kind(some("application/atom+xml"), b"x"), BodyKind::Xml);
    assert_eq!(body_kind(some("image/svg+xml"), b"<svg/>"), BodyKind::Image);
    assert_eq!(body_kind(some("text/html"), b"x"), BodyKind::Html);
    assert_eq!(body_kind(some("image/webp"), b"x"), BodyKind::Image);
    assert_eq!(body_kind(some("application/pdf"), b"x"), BodyKind::Pdf);
    assert_eq!(body_kind(some("text/plain"), b"\x89PNG"), BodyKind::Text);
    assert_eq!(body_kind(some("text/csv"), b"a,b"), BodyKind::Text);
    // An empty body has nothing to show, whatever its type.
    assert_eq!(body_kind(None, b""), BodyKind::Empty);
    assert_eq!(body_kind(some("application/json"), b""), BodyKind::Empty);
    // Otherwise the first bytes decide.
    let unknown = some("application/octet-stream");
    assert_eq!(body_kind(unknown, b"%PDF-1.7"), BodyKind::Pdf);
    assert_eq!(
        body_kind(None, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"),
        BodyKind::Image
    );
    assert_eq!(body_kind(None, b"\xFF\xD8\xFF\xE0"), BodyKind::Image);
    assert_eq!(body_kind(None, b"GIF89a"), BodyKind::Image);
    assert_eq!(body_kind(None, b"GIF87a"), BodyKind::Image);
    assert_eq!(body_kind(None, b"RIFF\x24\0\0\0WEBPVP8 "), BodyKind::Image);
    assert_eq!(
        body_kind(unknown, "plain text é".as_bytes()),
        BodyKind::Text
    );
    assert_eq!(body_kind(None, b"\0\x01\x02\xFF"), BodyKind::Binary);
    // A character cut at the end of the first bytes is still text.
    let mut head = vec![b'a'; 511];
    head.push(0xC3);
    assert_eq!(body_kind(None, &head), BodyKind::Text);
}

fn failure(error: SendError) -> SendFailure {
    match outcome_of_error(&error) {
        SendOutcome::Failed { failure } => failure,
        other => panic!("not a failure: {other:?}"),
    }
}

fn http(error: HttpError) -> SendFailure {
    failure(SendError::Http(error))
}

fn tls(kind: TlsErrorKind) -> SendFailure {
    http(HttpError::Tls(TlsError {
        kind,
        message: "the handshake said no".into(),
    }))
}

/// The kind, title and code of `failure`.
fn summary(failure: &SendFailure) -> (FailureKind, &str, Option<&str>) {
    (failure.kind, &failure.title, failure.code.as_deref())
}

#[test]
fn failures_follow_the_table() {
    use FailureKind as K;
    let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();

    let dns = http(HttpError::Dns {
        host: "api.example.test".into(),
        message: "nodename nor servname provided".into(),
    });
    assert_eq!(
        summary(&dns),
        (K::Dns, "DNS: host not found", Some("ENOTFOUND"))
    );
    assert_eq!(
        dns.message,
        "api.example.test could not be resolved. Check the host name."
    );

    let refused = http(HttpError::ConnectionRefused { addr });
    assert_eq!(
        summary(&refused),
        (K::Refused, "Connection refused", Some("ECONNREFUSED"))
    );
    assert_eq!(refused.message, "Nothing is listening on 127.0.0.1:9.");

    let connect = http(HttpError::Connect {
        addr,
        message: "network is unreachable".into(),
    });
    assert_eq!(summary(&connect), (K::Connect, "Could not connect", None));
    assert_eq!(connect.message, "127.0.0.1:9: network is unreachable");

    let expired = tls(TlsErrorKind::Expired);
    assert_eq!(
        summary(&expired),
        (K::Tls, "TLS: certificate expired", Some("CERT_HAS_EXPIRED"))
    );
    assert_eq!(expired.message, "the handshake said no");
    assert_eq!(
        summary(&tls(TlsErrorKind::NotYetValid)),
        (
            K::Tls,
            "TLS: certificate not valid yet",
            Some("CERT_NOT_YET_VALID")
        )
    );
    let untrusted = tls(TlsErrorKind::UnknownIssuer);
    assert_eq!(
        summary(&untrusted),
        (
            K::Tls,
            "TLS: certificate not trusted",
            Some("UNABLE_TO_VERIFY_LEAF_SIGNATURE")
        )
    );
    assert_eq!(
        untrusted.message,
        "Add its CA in the collection settings, or turn off Verify TLS for this request."
    );
    assert_eq!(
        summary(&tls(TlsErrorKind::WrongHost)),
        (
            K::Tls,
            "TLS: certificate is for another host",
            Some("ERR_TLS_CERT_ALTNAME_INVALID")
        )
    );
    let rejected = tls(TlsErrorKind::ClientCertificateRejected);
    assert_eq!(
        summary(&rejected),
        (K::Tls, "TLS: client certificate rejected", None)
    );
    assert_eq!(
        rejected.message,
        "The server did not accept the client certificate. Check it in the collection settings."
    );
    let other = tls(TlsErrorKind::Other);
    assert_eq!(summary(&other), (K::Tls, "TLS handshake failed", None));
    assert_eq!(other.message, "the handshake said no");

    let timeout = http(HttpError::Timeout(Duration::from_secs(8)));
    assert_eq!(
        summary(&timeout),
        (K::Timeout, "Timed out", Some("ETIMEDOUT"))
    );
    assert_eq!(
        timeout.message,
        "No answer after 8,000 ms, the timeout for this request."
    );
    assert_eq!(
        http(HttpError::Timeout(Duration::from_millis(1_234_567))).message,
        "No answer after 1,234,567 ms, the timeout for this request."
    );
    assert_eq!(
        http(HttpError::Timeout(Duration::from_millis(500))).message,
        "No answer after 500 ms, the timeout for this request."
    );

    let redirects = http(HttpError::TooManyRedirects(10));
    assert_eq!(
        summary(&redirects),
        (K::Redirects, "Too many redirects", None)
    );
    assert_eq!(redirects.message, "Stopped after 10 redirects.");

    let proxy = http(HttpError::Proxy("the proxy answered 407".into()));
    assert_eq!(summary(&proxy), (K::Proxy, "Proxy error", None));
    assert_eq!(proxy.message, "the proxy answered 407");

    let http_file = http(HttpError::File {
        path: PathBuf::from("/tmp/body.bin"),
        message: "permission denied".into(),
    });
    let send_file = failure(SendError::File {
        path: PathBuf::from("/tmp/ca.pem"),
        message: "no such file".into(),
    });
    assert_eq!(summary(&http_file), (K::File, "File not readable", None));
    assert_eq!(http_file.message, "/tmp/body.bin: permission denied");
    assert_eq!(summary(&send_file), (K::File, "File not readable", None));
    assert_eq!(send_file.message, "/tmp/ca.pem: no such file");

    let protocol = http(HttpError::Protocol("the header block is too large".into()));
    assert_eq!(
        summary(&protocol),
        (K::Protocol, "The answer could not be read", None)
    );
    assert_eq!(protocol.message, "the header block is too large");

    let invalid = http(HttpError::InvalidRequest("bad header name".into()));
    assert_eq!(summary(&invalid), (K::Request, "Request not valid", None));
    assert_eq!(invalid.message, "bad header name");
    let request = failure(SendError::Request(RequestError::NotFound(PathBuf::from(
        "a.yaml",
    ))));
    assert_eq!(summary(&request), (K::Request, "Request not valid", None));
    assert_eq!(request.message, "there is no request at a.yaml");

    let url = failure(SendError::InvalidUrl("http://exa mple/".into()));
    assert_eq!(summary(&url), (K::InvalidUrl, "URL not valid", None));
    assert_eq!(url.message, "http://exa mple/ is not a valid URL.");

    let one = failure(SendError::Unresolved(vec!["baseUrl".into()]));
    assert_eq!(
        summary(&one),
        (K::Unresolved, "The URL has undefined variables", None)
    );
    assert_eq!(one.message, "baseUrl has no value, so the URL has no host.");
    let two = failure(SendError::Unresolved(vec!["scheme".into(), "host".into()]));
    assert_eq!(
        two.message,
        "scheme and host have no value, so the URL has no host."
    );

    let vault = failure(SendError::Secret(UseError::AuthFailed));
    assert_eq!(summary(&vault), (K::Vault, "Vault", None));
    assert_eq!(vault.message, "authentication failed");
}

#[test]
fn cancelling_is_not_a_failure() {
    assert!(matches!(
        outcome_of_error(&SendError::Http(HttpError::Cancelled)),
        SendOutcome::Cancelled
    ));
    assert!(matches!(
        outcome_of_error(&SendError::NotConfirmed),
        SendOutcome::NotConfirmed
    ));
    assert!(matches!(
        outcome_of_error(&SendError::Secret(UseError::Cancelled)),
        SendOutcome::NotConfirmed
    ));
}

fn json(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn the_ui_gets_tagged_camel_case_json() {
    assert_eq!(
        json(SendOutcome::NotConfirmed),
        serde_json::json!({ "type": "notConfirmed" })
    );
    assert_eq!(
        json(SendEvent::Event {
            at_ms: 1.5,
            event: None,
            data: "hi".into(),
            id: None,
        }),
        serde_json::json!({ "type": "event", "atMs": 1.5, "event": null, "data": "hi", "id": null })
    );
    let failed = outcome_of_error(&SendError::InvalidUrl("x".into()));
    assert_eq!(json(failed)["failure"]["kind"], "invalidUrl");
    assert_eq!(json(BodyKind::Json), "json");
}

#[test]
fn cookie_info_converts_both_ways() {
    let stored = StoredCookie {
        name: "session".into(),
        value: "abc".into(),
        domain: "example.test".into(),
        path: "/".into(),
        expires: Some(OffsetDateTime::from_unix_timestamp(1_793_786_400).unwrap()),
        secure: true,
        http_only: true,
        host_only: false,
    };
    let info = CookieInfo::from(&stored);
    assert_eq!(info.expires.as_deref(), Some("2026-11-04T10:00:00Z"));
    assert_eq!(info.to_stored(), Ok(stored));

    let session = CookieInfo {
        expires: None,
        ..info.clone()
    };
    assert_eq!(session.to_stored().unwrap().expires, None);
    let broken = CookieInfo {
        expires: Some("next week".into()),
        ..info
    };
    assert!(broken.to_stored().is_err());
    // The value is a secret.
    assert!(!format!("{broken:?}").contains("abc"));
}
