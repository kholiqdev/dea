//! Connections, the pool and per-phase timing, against the local test server.

use std::time::Duration;

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, HttpError, HttpVersion, Options, Request,
    Response, SendContext, TlsConfig,
};
use dea_testkit::http::{TestServer, Tls};

fn client() -> Client {
    Client::new(ClientConfig {
        user_agent: "Dea/test".into(),
        ..ClientConfig::default()
    })
    .unwrap()
}

fn get(url: &str, options: Options) -> Request {
    Request {
        method: "GET".into(),
        url: url.parse().unwrap(),
        headers: Vec::new(),
        body: Body::Empty,
        options,
        tls: TlsConfig::default(),
    }
}

fn get_trusting(url: &str, server: &TestServer) -> Request {
    Request {
        tls: trusting(server),
        ..get(url, Options::default())
    }
}

fn trusting(server: &TestServer) -> TlsConfig {
    TlsConfig {
        extra_roots_pem: vec![server.ca_pem()],
        client_identity: None,
    }
}

fn tls(alpn: &[&[u8]]) -> Tls {
    Tls::SelfSigned {
        alpn: alpn.iter().map(|p| p.to_vec()).collect(),
    }
}

async fn send(client: &Client, request: Request) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client.send(request, SendContext::new(&cancel)).await
}

fn header<'a>(response: &'a Response, name: &str) -> &'a str {
    let (_, value) = response
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no {name} header"));
    value
}

#[tokio::test]
async fn phases_add_up_to_the_total() {
    let server = TestServer::start(tls(&[b"h2", b"http/1.1"])).await;
    let response = send(&client(), get_trusting(&server.url("/status/204"), &server))
        .await
        .unwrap();
    let t = response.timing;
    let sum = t.dns + t.connect + t.tls + t.waiting + t.download;
    assert!(sum.abs_diff(t.total) <= Duration::from_millis(2), "{t:?}");
    assert!(t.tls > Duration::ZERO, "{t:?}");
}

#[tokio::test]
async fn server_delay_shows_as_waiting() {
    let server = TestServer::start(Tls::None).await;
    let response = send(
        &client(),
        get(&server.url("/delay/200"), Options::default()),
    )
    .await
    .unwrap();
    let slept = Duration::from_micros(header(&response, "x-slept-us").parse().unwrap());
    assert!(
        response.timing.waiting.abs_diff(slept) <= Duration::from_millis(10),
        "{:?} vs {slept:?}",
        response.timing
    );
}

#[tokio::test]
async fn second_request_reuses_the_connection() {
    for (server_tls, version) in [
        (Tls::None, HttpVersion::Http11),
        (tls(&[b"h2", b"http/1.1"]), HttpVersion::Http2),
    ] {
        let server = TestServer::start(server_tls).await;
        let client = client();
        let url = server.url("/status/200");
        let first = send(&client, get_trusting(&url, &server)).await.unwrap();
        assert_eq!(first.version, version);
        assert!(!first.timing.reused);
        let second = send(&client, get_trusting(&url, &server)).await.unwrap();
        assert!(second.timing.reused, "{version}");
        assert_eq!(
            (second.timing.dns, second.timing.connect, second.timing.tls),
            (Duration::ZERO, Duration::ZERO, Duration::ZERO)
        );
        assert_eq!(server.connections(), 1, "{version}");
    }
}

#[tokio::test]
async fn stale_pooled_connection_is_replaced() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    let closing = send(&client, get(&server.url("/close"), Options::default()))
        .await
        .unwrap();
    assert!(closing.headers.iter().all(|(name, _)| name != "connection"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    let again = send(&client, get(&server.url("/status/200"), Options::default()))
        .await
        .unwrap();
    assert_eq!(again.status, 200);
    assert!(!again.timing.reused);
    assert_eq!(server.connections(), 2);
}

/// Without the pause, the close often lands after the connection was taken
/// from the pool, so the request fails there and is retried.
#[tokio::test]
async fn request_right_after_the_server_closes_is_retried() {
    for _ in 0..20 {
        let server = TestServer::start(Tls::None).await;
        let client = client();
        send(&client, get(&server.url("/close"), Options::default()))
            .await
            .unwrap();
        let again = send(&client, get(&server.url("/status/200"), Options::default()))
            .await
            .unwrap();
        assert_eq!(again.status, 200);
        assert!(!again.timing.reused);
        assert_eq!(server.connections(), 2);
    }
}

#[tokio::test]
async fn http1_over_plain_tcp() {
    let server = TestServer::start(Tls::None).await;
    let response = send(
        &client(),
        get(&server.url("/status/200"), Options::default()),
    )
    .await
    .unwrap();
    assert_eq!(response.version, HttpVersion::Http11);
    assert_eq!(response.status, 200);
    assert_eq!(response.reason, "OK");
    assert_eq!(response.timing.dns, Duration::ZERO);
    assert_eq!(response.timing.tls, Duration::ZERO);
}

#[tokio::test]
async fn http2_is_negotiated_with_alpn() {
    let server = TestServer::start(tls(&[b"h2", b"http/1.1"])).await;
    let response = send(&client(), get_trusting(&server.url("/status/201"), &server))
        .await
        .unwrap();
    assert_eq!(response.version, HttpVersion::Http2);
    assert_eq!(response.version.to_string(), "HTTP/2");
    assert_eq!(response.status, 201);
}

#[tokio::test]
async fn http1_when_the_server_offers_only_http1() {
    let server = TestServer::start(tls(&[b"http/1.1"])).await;
    let response = send(&client(), get_trusting(&server.url("/status/200"), &server))
        .await
        .unwrap();
    assert_eq!(response.version, HttpVersion::Http11);
    assert_eq!(response.version.to_string(), "HTTP/1.1");
    assert!(response.timing.tls > Duration::ZERO);
}

#[tokio::test]
async fn auto_headers_are_sent_and_user_headers_replace_them() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    send(&client, get(&server.url("/echo"), Options::default()))
        .await
        .unwrap();
    let mut request = get(&server.url("/echo"), Options::default());
    request.headers = vec![("User-Agent".into(), "mine".into())];
    send(&client, request).await.unwrap();

    let received = server.requests();
    assert_eq!(received[0].header("user-agent"), Some("Dea/test"));
    assert_eq!(received[0].header("accept"), Some("*/*"));
    assert_eq!(
        received[0].header("host"),
        Some(server.url("").trim_start_matches("http://"))
    );
    let user_agents: Vec<_> = received[1]
        .headers
        .iter()
        .filter(|(name, _)| name == "user-agent")
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(user_agents, ["mine"]);
    assert_eq!(received[1].header("accept"), Some("*/*"));
}

#[tokio::test]
async fn body_and_its_content_type_are_sent() {
    let server = TestServer::start(Tls::None).await;
    let mut request = get(&server.url("/echo"), Options::default());
    request.method = "POST".into();
    request.body = Body::Bytes {
        data: bytes::Bytes::from_static(b"{\"a\":1}"),
        content_type: Some("application/json".into()),
    };
    let response = send(&client(), request).await.unwrap();
    assert_eq!(response.body.read_all().unwrap(), b"{\"a\":1}");
    let received = &server.requests()[0];
    assert_eq!(received.method, "POST");
    assert_eq!(received.header("content-type"), Some("application/json"));
    assert_eq!(received.header("content-length"), Some("7"));
}

#[tokio::test]
async fn user_host_becomes_the_http2_authority() {
    let server = TestServer::start(tls(&[b"h2"])).await;
    let mut request = get_trusting(&server.url("/echo"), &server);
    let host = server
        .url("")
        .replace("https://localhost", "dea-target.test");
    request.headers = vec![("Host".into(), host.clone())];
    let response = send(&client(), request).await.unwrap();
    assert_eq!(response.version, HttpVersion::Http2);
    let received = &server.requests()[0];
    assert_eq!(received.authority.as_deref(), Some(host.as_str()));
    assert_eq!(received.header("host"), None);
}

#[tokio::test]
async fn invalid_header_value_names_the_header() {
    let server = TestServer::start(Tls::None).await;
    let mut request = get(&server.url("/echo"), Options::default());
    request.headers = vec![("X-Token".into(), "abc\nsecret-value".into())];
    let error = send(&client(), request).await.unwrap_err();
    let HttpError::InvalidRequest(message) = &error else {
        panic!("expected InvalidRequest, got {error:?}");
    };
    assert!(message.contains("X-Token"), "{message}");
    assert!(!message.contains("secret-value"), "{message}");
    assert!(!error.to_string().contains("secret-value"));
    assert_eq!(server.connections(), 0);
}

#[test]
fn client_is_shared_across_tasks() {
    fn shared<T: Send + Sync + Clone + 'static>() {}
    fn sendable<T: Send>(_: &T) {}
    shared::<Client>();
    let client = client();
    let cancel = CancellationToken::new();
    let url = "http://127.0.0.1:9/";
    sendable(&client.send(get(url, Options::default()), SendContext::new(&cancel)));
}
