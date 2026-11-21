//! Connection phases reported while a request runs, against the local
//! test server.

use std::sync::Mutex;
use std::time::Duration;

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, Event, Options, Phase, Request, Response,
    SendContext, TlsConfig,
};
use dea_testkit::http::{TestServer, Tls};

fn get(server: &TestServer, path: &str) -> Request {
    Request {
        method: "GET".into(),
        url: server.url(path).parse().unwrap(),
        headers: Vec::new(),
        credential_headers: Vec::new(),
        body: Body::Empty,
        options: Options::default(),
        tls: TlsConfig {
            extra_roots_pem: vec![server.ca_pem()],
            client_identity: None,
        },
    }
}

async fn https_server() -> TestServer {
    TestServer::start(Tls::SelfSigned {
        alpn: vec![b"http/1.1".to_vec()],
    })
    .await
}

async fn send_collecting(client: &Client, request: Request) -> (Response, Vec<Event>) {
    let seen = Mutex::new(Vec::new());
    let sink = |event| seen.lock().unwrap().push(event);
    let cancel = CancellationToken::new();
    let response = client
        .send(request, SendContext::new(&cancel).events(&sink))
        .await
        .unwrap();
    (response, seen.into_inner().unwrap())
}

fn kind(event: &Event) -> &'static str {
    match event {
        Event::Phase {
            phase: Phase::Dns, ..
        } => "dns",
        Event::Phase {
            phase: Phase::Connect,
            ..
        } => "connect",
        Event::Phase {
            phase: Phase::Tls, ..
        } => "tls",
        Event::Reused => "reused",
        Event::Head { .. } => "head",
        Event::Data(_) => "data",
        Event::Sse(_) => "sse",
    }
}

fn kinds(events: &[Event]) -> Vec<&'static str> {
    events
        .iter()
        .map(kind)
        .filter(|kind| *kind != "data")
        .collect()
}

fn took(events: &[Event], wanted: Phase) -> Duration {
    events
        .iter()
        .find_map(|event| match event {
            Event::Phase { phase, took } if *phase == wanted => Some(*took),
            _ => None,
        })
        .expect("phase was reported")
}

#[tokio::test]
async fn phases_arrive_before_the_head() {
    let server = https_server().await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (response, events) = send_collecting(&client, get(&server, "/status/200")).await;
    assert_eq!(kinds(&events), ["dns", "connect", "tls", "head"]);
    assert_eq!(took(&events, Phase::Dns), response.timing.dns);
    assert_eq!(took(&events, Phase::Connect), response.timing.connect);
    assert_eq!(took(&events, Phase::Tls), response.timing.tls);
}

#[tokio::test]
async fn plain_http_has_no_tls_phase() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (_, events) = send_collecting(&client, get(&server, "/status/200")).await;
    assert_eq!(kinds(&events), ["dns", "connect", "head"]);
}

#[tokio::test]
async fn a_reused_connection_reports_reused() {
    let server = https_server().await;
    let client = Client::new(ClientConfig::default()).unwrap();
    send_collecting(&client, get(&server, "/status/200")).await;
    let (response, events) = send_collecting(&client, get(&server, "/status/200")).await;
    assert!(response.timing.reused);
    assert_eq!(kinds(&events), ["reused", "head"]);
}

#[tokio::test]
async fn a_followed_hop_reports_its_phases_but_no_head() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (response, events) = send_collecting(&client, get(&server, "/redirect/1")).await;
    assert_eq!(response.redirects.len(), 1);
    // The first hop opens the connection; the second reuses it.
    assert_eq!(kinds(&events), ["dns", "connect", "reused", "head"]);
}

#[tokio::test]
async fn the_head_carries_the_reason_phrase() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let (response, events) = send_collecting(&client, get(&server, "/status/404")).await;
    let reason = events.iter().find_map(|event| match event {
        Event::Head { reason, .. } => Some(reason.as_str()),
        _ => None,
    });
    assert_eq!(reason, Some("Not Found"));
    assert_eq!(response.reason, "Not Found");
}
