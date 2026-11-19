//! Timeouts, cancel, redirects, TLS options and error kinds, against the
//! local test servers.

use std::time::{Duration, Instant};

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, ClientIdentity, HttpError, Options, Request,
    Response, SendContext, TlsConfig,
};
use dea_testkit::http::{HoldServer, TestServer, Tls, client_identity_pem};
use zeroize::Zeroizing;

fn client() -> Client {
    Client::new(ClientConfig {
        user_agent: "Dea/test".into(),
        ..ClientConfig::default()
    })
    .unwrap()
}

fn request(method: &str, url: &str, options: Options) -> Request {
    Request {
        method: method.into(),
        url: url.parse().unwrap(),
        headers: Vec::new(),
        body: Body::Empty,
        options,
        tls: TlsConfig::default(),
    }
}

fn get(url: &str) -> Request {
    request("GET", url, Options::default())
}

fn with_timeout(ms: u64) -> Options {
    Options {
        timeout: Duration::from_millis(ms),
        ..Options::default()
    }
}

async fn send(client: &Client, request: Request) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client.send(request, SendContext::new(&cancel)).await
}

fn redirect_to(base: &str, target: &str, status: u16) -> String {
    let mut url: url::Url = format!("{base}/redirect-to").parse().unwrap();
    url.query_pairs_mut()
        .append_pair("url", target)
        .append_pair("status", &status.to_string());
    url.into()
}

fn trusting(server: &TestServer) -> TlsConfig {
    TlsConfig {
        extra_roots_pem: vec![server.ca_pem()],
        client_identity: None,
    }
}

fn h1_tls() -> Tls {
    Tls::SelfSigned {
        alpn: vec![b"http/1.1".to_vec()],
    }
}

fn mutual() -> Tls {
    Tls::Mutual {
        alpn: vec![b"http/1.1".to_vec()],
    }
}

fn identity() -> ClientIdentity {
    let (certificate_pem, key_pem) = client_identity_pem();
    ClientIdentity {
        certificate_pem,
        key_pem: Zeroizing::new(key_pem),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_closes_the_connection_within_200_ms() {
    let hold = HoldServer::start().await;
    let client = client();
    let cancel = CancellationToken::new();
    let task = {
        let (client, cancel, url) = (client.clone(), cancel.clone(), hold.url());
        tokio::spawn(async move { client.send(get(&url), SendContext::new(&cancel)).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let cancelled_at = Instant::now();
    cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(HttpError::Cancelled)));
    assert!(cancelled_at.elapsed() <= Duration::from_millis(200));
    let closed = hold
        .wait_closed(Duration::from_secs(1))
        .await
        .expect("connection closed");
    assert!(closed.duration_since(cancelled_at) <= Duration::from_millis(200));
}

#[tokio::test]
async fn cancel_before_the_connection_is_open_returns_cancelled() {
    // 192.0.2.0/24 is reserved for documentation; connecting hangs or fails
    // fast, and either way the cancel must win or the connect must fail.
    let cancel = CancellationToken::new();
    let task = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            client()
                .send(get("http://192.0.2.1/"), SendContext::new(&cancel))
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("send returns promptly")
        .unwrap();
    assert!(
        matches!(
            result,
            Err(HttpError::Cancelled | HttpError::Connect { .. })
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn timeout_counts_until_the_response_head() {
    let server = TestServer::start(Tls::None).await;
    let started = Instant::now();
    let result = send(
        &client(),
        request("GET", &server.url("/delay/2000"), with_timeout(300)),
    )
    .await;
    let elapsed = started.elapsed();
    assert!(matches!(result, Err(HttpError::Timeout(_))), "{result:?}");
    assert!(
        elapsed >= Duration::from_millis(290) && elapsed <= Duration::from_millis(400),
        "{elapsed:?}"
    );
}

#[tokio::test]
async fn slow_but_steady_body_is_not_a_timeout() {
    let server = TestServer::start(Tls::None).await;
    let response = send(
        &client(),
        request("GET", &server.url("/drip/10/100"), with_timeout(300)),
    )
    .await
    .unwrap();
    assert_eq!(response.body.read_all().unwrap(), b"xxxxxxxxxx");
    assert!(response.timing.download >= Duration::from_millis(800));
}

#[tokio::test]
async fn stalled_body_times_out() {
    let server = TestServer::start(Tls::None).await;
    let result = send(
        &client(),
        request("GET", &server.url("/stall-body"), with_timeout(300)),
    )
    .await;
    assert!(matches!(result, Err(HttpError::Timeout(_))), "{result:?}");
}

#[tokio::test]
async fn follows_redirects_up_to_the_limit() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    let response = send(&client, get(&server.url("/redirect/10")))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.redirects.len(), 10);
    assert_eq!(response.redirects[0].status, 302);
    assert!(response.redirects[0].url.ends_with("/redirect/10"));
    assert!(response.redirects[9].url.ends_with("/redirect/1"));

    let result = send(&client, get(&server.url("/redirect/11"))).await;
    assert!(
        matches!(result, Err(HttpError::TooManyRedirects(10))),
        "{result:?}"
    );
}

#[tokio::test]
async fn follow_redirects_off_returns_the_redirect() {
    let server = TestServer::start(Tls::None).await;
    let options = Options {
        follow_redirects: false,
        ..Options::default()
    };
    let response = send(
        &client(),
        request("GET", &server.url("/redirect/3"), options),
    )
    .await
    .unwrap();
    assert_eq!(response.status, 302);
    assert!(response.redirects.is_empty());
    assert_eq!(server.requests().len(), 1);
}

fn post(url: &str, body: &'static [u8]) -> Request {
    Request {
        headers: vec![("X-Keep".into(), "1".into())],
        body: Body::Bytes {
            data: bytes::Bytes::from_static(body),
            content_type: Some("text/plain".into()),
        },
        ..request("POST", url, Options::default())
    }
}

#[tokio::test]
async fn see_other_turns_post_into_get() {
    for status in [301, 302, 303] {
        let a = TestServer::start(Tls::None).await;
        let b = TestServer::start(Tls::None).await;
        let url = redirect_to(&a.url(""), &b.url("/echo"), status);
        let response = send(&client(), post(&url, b"payload")).await.unwrap();
        assert_eq!(response.status, 200, "{status}");
        assert!(response.body.is_empty(), "{status}");
        let received = &b.requests()[0];
        assert_eq!(received.method, "GET", "{status}");
        assert!(received.body.is_empty(), "{status}");
        assert!(received.header("content-type").is_none(), "{status}");
        assert!(received.header("content-length").is_none(), "{status}");
        assert_eq!(received.header("x-keep"), Some("1"), "{status}");
    }
}

#[tokio::test]
async fn temporary_redirect_keeps_method_and_body() {
    for status in [307, 308] {
        let a = TestServer::start(Tls::None).await;
        let b = TestServer::start(Tls::None).await;
        let url = redirect_to(&a.url(""), &b.url("/echo"), status);
        let response = send(&client(), post(&url, b"payload")).await.unwrap();
        assert_eq!(response.body.read_all().unwrap(), b"payload", "{status}");
        let received = &b.requests()[0];
        assert_eq!(received.method, "POST", "{status}");
        assert_eq!(received.body, b"payload", "{status}");
        assert_eq!(received.header("content-type"), Some("text/plain"));
    }
}

#[tokio::test]
async fn authorization_is_dropped_on_cross_host_redirect() {
    let a = TestServer::start(Tls::None).await;
    let b = TestServer::start(Tls::None).await;
    // Same machine, different host name: A is 127.0.0.1, B is localhost.
    let b_url = b.url("/echo").replace("127.0.0.1", "localhost");
    let request = Request {
        headers: vec![
            ("Authorization".into(), "Bearer t".into()),
            ("Cookie".into(), "s=1".into()),
            ("X-Keep".into(), "1".into()),
        ],
        ..get(&redirect_to(&a.url(""), &b_url, 302))
    };
    send(&client(), request).await.unwrap();
    let first = &a.requests()[0];
    assert_eq!(first.header("authorization"), Some("Bearer t"));
    let received = &b.requests()[0];
    assert!(received.header("authorization").is_none() && received.header("cookie").is_none());
    assert_eq!(received.header("x-keep"), Some("1"));
}

#[tokio::test]
async fn authorization_is_kept_on_same_origin_redirect() {
    let server = TestServer::start(Tls::None).await;
    let request = Request {
        headers: vec![("Authorization".into(), "Bearer t".into())],
        ..get(&server.url("/redirect/1"))
    };
    send(&client(), request).await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].header("authorization"), Some("Bearer t"));
}

#[tokio::test]
async fn verify_off_accepts_a_self_signed_certificate() {
    let server = TestServer::start(h1_tls()).await;
    // No extra roots, and 127.0.0.1 is not among the certificate's IP
    // names, so both the chain and the host name are wrong.
    let url = server.url("/status/204").replace("localhost", "127.0.0.1");
    let options = Options {
        verify_tls: false,
        ..Options::default()
    };
    let response = send(&client(), request("GET", &url, options))
        .await
        .unwrap();
    assert_eq!(response.status, 204);
}

#[tokio::test]
async fn untrusted_certificate_is_a_tls_error() {
    let server = TestServer::start(h1_tls()).await;
    let result = send(&client(), get(&server.url("/status/204"))).await;
    assert!(matches!(result, Err(HttpError::Tls(_))), "{result:?}");
}

#[tokio::test]
async fn client_certificate_is_presented() {
    let server = TestServer::start(mutual()).await;
    let client = client();
    let without = Request {
        tls: trusting(&server),
        ..get(&server.url("/status/200"))
    };
    assert!(send(&client, without).await.is_err());

    let with = Request {
        tls: TlsConfig {
            client_identity: Some(identity()),
            ..trusting(&server)
        },
        ..get(&server.url("/status/200"))
    };
    let response = send(&client, with).await.unwrap();
    assert_eq!(response.status, 200);
    let seen = server.requests();
    assert_eq!(
        seen.last().unwrap().client_subject.as_deref(),
        Some("CN=Dea Test Client")
    );
}

#[tokio::test]
async fn unknown_host_is_a_dns_error() {
    let result = send(&client(), get("http://dea-test.invalid/")).await;
    assert!(matches!(result, Err(HttpError::Dns { .. })), "{result:?}");
}

#[tokio::test]
async fn closed_port_is_connection_refused() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let result = send(&client(), get(&format!("http://127.0.0.1:{port}/"))).await;
    assert!(
        matches!(result, Err(HttpError::ConnectionRefused { .. })),
        "{result:?}"
    );
}
