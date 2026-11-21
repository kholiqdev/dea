//! Large responses, streaming events, Server-Sent Events and decoding,
//! against the local test server.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, Event, HttpError, Options, Request, Response,
    ResponseBody, SendContext, SseEvent, TlsConfig,
};
use dea_testkit::http::{TestServer, Tls};

fn client_with_spill(dir: &Path) -> Client {
    Client::new(ClientConfig {
        spill_dir: dir.to_path_buf(),
        ..ClientConfig::default()
    })
    .unwrap()
}

fn request(url: &str, headers: &[(&str, &str)], options: Options) -> Request {
    Request {
        method: "GET".into(),
        url: url.parse().unwrap(),
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect(),
        credential_headers: Vec::new(),
        body: Body::Empty,
        options,
        tls: TlsConfig::default(),
    }
}

fn get(url: &str) -> Request {
    request(url, &[], Options::default())
}

async fn send(client: &Client, request: Request) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client.send(request, SendContext::new(&cancel)).await
}

/// Sends `request` and records every event with the time it arrived.
async fn send_recording(
    client: &Client,
    request: Request,
) -> (Response, Vec<(Instant, Event)>, Instant) {
    let seen = Mutex::new(Vec::new());
    let sink = |event| seen.lock().unwrap().push((Instant::now(), event));
    let cancel = CancellationToken::new();
    let response = client
        .send(request, SendContext::new(&cancel).events(&sink))
        .await
        .unwrap();
    let finished = Instant::now();
    (response, seen.into_inner().unwrap(), finished)
}

#[tokio::test]
async fn hundred_megabytes_spill_to_a_file() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let client = client_with_spill(dir.path());
    let response = send(&client, get(&server.url("/bytes/104857600")))
        .await
        .unwrap();
    let ResponseBody::File { path, len } = &response.body else {
        panic!("kept in memory");
    };
    assert_eq!(*len, 104_857_600);
    assert_eq!(std::fs::metadata(path).unwrap().len(), 104_857_600);
    assert_eq!(path.parent(), Some(dir.path()));
    assert!(!response.truncated);
    let kept = path.to_path_buf();
    // download covers the writes, and the phases still add up
    let t = response.timing;
    let sum = t.dns + t.connect + t.tls + t.waiting + t.download;
    assert!(t.total.abs_diff(sum) <= Duration::from_millis(2), "{t:?}");
    drop(response);
    assert!(!kept.exists());
}

#[tokio::test]
async fn small_body_stays_in_memory() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let response = send(
        &client_with_spill(dir.path()),
        get(&server.url("/bytes/5000")),
    )
    .await
    .unwrap();
    assert!(matches!(response.body, ResponseBody::Memory(_)));
    assert_eq!(response.body.len(), 5000);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn the_spill_threshold_is_ten_mebibytes() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let client = client_with_spill(dir.path());
    let ten = 10 * 1024 * 1024;
    let at = send(&client, get(&server.url(&format!("/bytes/{ten}"))))
        .await
        .unwrap();
    assert!(matches!(at.body, ResponseBody::Memory(_)));
    let over = send(&client, get(&server.url(&format!("/bytes/{}", ten + 1))))
        .await
        .unwrap();
    assert!(matches!(over.body, ResponseBody::File { len, .. } if len == ten + 1));
}

#[tokio::test]
async fn body_limit_truncates_and_closes_the_connection() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let options = Options {
        max_body_bytes: 1000,
        ..Options::default()
    };
    let response = send(&client, request(&server.url("/bytes/5000"), &[], options))
        .await
        .unwrap();
    assert!(response.truncated);
    assert_eq!(response.body.len(), 1000);
    assert_eq!(response.body.read_all().unwrap(), vec![b'x'; 1000]);
    // the half-read HTTP/1.1 connection must not be reused
    let next = send(&client, get(&server.url("/bytes/10"))).await.unwrap();
    assert!(!next.timing.reused);
    assert_eq!(server.connections(), 2);
}

#[tokio::test]
async fn a_body_exactly_at_the_limit_is_not_truncated() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let options = Options {
        max_body_bytes: 5000,
        ..Options::default()
    };
    let response = send(&client, request(&server.url("/bytes/5000"), &[], options))
        .await
        .unwrap();
    assert!(!response.truncated);
    assert_eq!(response.body.len(), 5000);
}

#[tokio::test]
async fn the_limit_applies_to_a_spilled_body_too() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let options = Options {
        max_body_bytes: 12 * 1024 * 1024,
        ..Options::default()
    };
    let response = send(
        &client_with_spill(dir.path()),
        request(&server.url("/bytes/20000000"), &[], options),
    )
    .await
    .unwrap();
    assert!(response.truncated);
    assert!(matches!(response.body, ResponseBody::File { len, .. } if len == 12 * 1024 * 1024));
}

#[tokio::test]
async fn head_event_comes_before_data() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (_, mut events, _) = send_recording(&client, get(&server.url("/drip/3/10"))).await;
    // The connection phases come first; the head follows them.
    events.retain(|(_, event)| !matches!(event, Event::Phase { .. } | Event::Reused));
    let Some((_, Event::Head { status, headers })) = events.first() else {
        panic!("the first event is not the head: {events:?}");
    };
    assert_eq!(*status, 200);
    assert!(headers.iter().any(|(name, _)| name == "transfer-encoding"));
    let data: Vec<u8> = events
        .iter()
        .skip(1)
        .flat_map(|(_, event)| match event {
            Event::Data(bytes) => bytes.to_vec(),
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(data, b"xxx");
}

#[tokio::test]
async fn chunks_arrive_as_they_come() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (_, events, finished) = send_recording(&client, get(&server.url("/drip/5/100"))).await;
    let first = events
        .iter()
        .find(|(_, event)| matches!(event, Event::Data(_)))
        .map(|(at, _)| *at)
        .unwrap();
    assert!(
        finished.duration_since(first) >= Duration::from_millis(300),
        "first data came {:?} before the end",
        finished.duration_since(first)
    );
}

#[tokio::test]
async fn events_of_a_redirected_hop_are_not_sent() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (response, events, _) = send_recording(&client, get(&server.url("/redirect/2"))).await;
    assert_eq!(response.redirects.len(), 2);
    let heads = events
        .iter()
        .filter(|(_, event)| matches!(event, Event::Head { .. }))
        .count();
    assert_eq!(heads, 1);
}

#[tokio::test]
async fn server_sent_events_are_split() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (_, events, _) = send_recording(&client, get(&server.url("/sse"))).await;
    let events: Vec<SseEvent> = events
        .into_iter()
        .filter_map(|(_, event)| match event {
            Event::Sse(sse) => Some(sse),
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event.as_deref(), Some("tick"));
    assert_eq!(events[0].data, "one");
    assert_eq!(events[1].data, "line one\nline two");
    assert_eq!(events[1].id.as_deref(), Some("2"));
    assert_eq!(events[2].data, "three");
    assert!(events[0].at < events[1].at && events[1].at < events[2].at);
    assert!(events[2].at - events[0].at >= Duration::from_millis(150));
}

#[tokio::test]
async fn other_content_types_send_no_sse_events() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (_, events, _) = send_recording(&client, get(&server.url("/drip/2/1"))).await;
    assert!(events.iter().all(|(_, e)| !matches!(e, Event::Sse(_))));
}

const ASK: [(&str, &str); 1] = [("Accept-Encoding", "gzip, deflate")];

#[tokio::test]
async fn gzip_response_is_decoded() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let response = send(
        &client,
        request(
            &server.url("/gzip"),
            &[("Accept-Encoding", "gzip")],
            Options::default(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(response.body.read_all().unwrap(), b"hello from gzip");
    assert_eq!(response.content_encoding.as_deref(), Some("gzip"));
}

#[tokio::test]
async fn deflate_response_is_decoded() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let response = send(
        &client,
        request(&server.url("/deflate"), &ASK, Options::default()),
    )
    .await
    .unwrap();
    assert_eq!(response.body.read_all().unwrap(), b"hello from deflate");
    assert_eq!(response.content_encoding.as_deref(), Some("deflate"));
}

#[tokio::test]
async fn decoded_bytes_are_what_events_carry() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (_, events, _) = send_recording(
        &client,
        request(&server.url("/gzip"), &ASK, Options::default()),
    )
    .await;
    let data: Vec<u8> = events
        .iter()
        .filter_map(|(_, event)| match event {
            Event::Data(bytes) => Some(bytes.to_vec()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(data, b"hello from gzip");
}

#[tokio::test]
async fn other_encodings_are_kept_and_named() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let response = send(
        &client,
        request(&server.url("/brotli"), &ASK, Options::default()),
    )
    .await
    .unwrap();
    assert_eq!(response.body.read_all().unwrap(), b"not really brotli");
    assert_eq!(response.content_encoding.as_deref(), Some("br"));
}

#[tokio::test]
async fn without_accept_encoding_nothing_is_asked_or_decoded() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let response = send(&client, get(&server.url("/gzip"))).await.unwrap();
    assert_eq!(server.requests()[0].header("accept-encoding"), None);
    assert_eq!(response.body.read_all().unwrap()[..2], [31, 139]);
    assert_eq!(response.content_encoding.as_deref(), Some("gzip"));
}

#[tokio::test]
async fn a_plain_response_has_no_content_encoding() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let response = send(
        &client,
        request(&server.url("/bytes/5"), &ASK, Options::default()),
    )
    .await
    .unwrap();
    assert_eq!(response.content_encoding, None);
}

#[tokio::test]
async fn a_gzip_head_response_has_an_empty_body_and_no_error() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let mut head = request(&server.url("/gzip"), &ASK, Options::default());
    head.method = "HEAD".into();
    let response = send(&client, head).await.unwrap();
    assert_eq!(response.status, 200);
    assert!(response.body.is_empty());
    assert_eq!(response.content_encoding.as_deref(), Some("gzip"));
}

#[tokio::test]
async fn a_truncated_gzip_stream_is_a_clear_error() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let error = send(
        &client,
        request(&server.url("/gzip-cut"), &ASK, Options::default()),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, HttpError::Protocol(m) if m.contains("gzip")),
        "{error}"
    );
}

#[tokio::test]
async fn a_gzip_bomb_stops_at_the_limit() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let options = Options {
        max_body_bytes: 1000,
        ..Options::default()
    };
    let response = send(&client, request(&server.url("/bomb/64"), &ASK, options))
        .await
        .unwrap();
    assert!(response.truncated);
    assert_eq!(response.body.read_all().unwrap(), vec![0u8; 1000]);
    let next = send(&client, get(&server.url("/bytes/10"))).await.unwrap();
    assert!(!next.timing.reused);
}

#[tokio::test]
async fn a_decoded_body_spills_like_any_other() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let response = send(
        &client_with_spill(dir.path()),
        request(&server.url("/bomb/12"), &ASK, Options::default()),
    )
    .await
    .unwrap();
    assert!(matches!(response.body, ResponseBody::File { len, .. } if len == 12 * 1024 * 1024));
}
