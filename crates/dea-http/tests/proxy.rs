//! Sending through a proxy, against the local test proxy. These tests never
//! read the real proxy settings of the machine: they use `Fixed` or `None`.

use std::time::Duration;

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, HttpError, NoProxy, Options, ProxyConfig,
    ProxySettings, Request, Response, SendContext, TlsConfig, proxy_from_env, system_proxy,
};
use dea_testkit::http::{TestProxy, TestServer, Tls};

fn via(proxy_url: &str, no_proxy: &str) -> Client {
    Client::new(ClientConfig {
        user_agent: "Dea/test".into(),
        proxy: ProxyConfig::Fixed(ProxySettings {
            http: Some(proxy_url.parse().unwrap()),
            https: Some(proxy_url.parse().unwrap()),
            no_proxy: NoProxy::parse(no_proxy),
        }),
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
        tls: TlsConfig {
            extra_roots_pem: vec![server.ca_pem()],
            client_identity: None,
        },
        ..get(url, Options::default())
    }
}

async fn send(client: &Client, request: Request) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client.send(request, SendContext::new(&cancel)).await
}

async fn tls_server() -> TestServer {
    TestServer::start(Tls::SelfSigned { alpn: Vec::new() }).await
}

#[test]
fn env_proxy_variables() {
    let env = |name: &str| match name {
        "HTTPS_PROXY" => Some("http://user:pw@proxy.local:3128".into()),
        "http_proxy" => Some("http://other:8080".into()),
        "NO_PROXY" => Some(".internal, 10.0.0.0/8, api.test:8443".into()),
        _ => None,
    };
    let settings = proxy_from_env(env).unwrap();
    assert_eq!(settings.https.unwrap().host_str(), Some("proxy.local"));
    assert_eq!(settings.http.unwrap().host_str(), Some("other"));
    assert!(settings.no_proxy.matches("a.internal", 443));
    assert!(settings.no_proxy.matches("10.1.2.3", 80));
    assert!(
        settings.no_proxy.matches("api.test", 8443) && !settings.no_proxy.matches("api.test", 443)
    );
    assert!(settings.no_proxy.matches("localhost", 3000));
}

#[test]
fn upper_case_wins_and_all_proxy_fills_the_gaps() {
    let env = |name: &str| match name {
        "HTTP_PROXY" => Some("http://upper:1".into()),
        "http_proxy" => Some("http://lower:2".into()),
        "ALL_PROXY" => Some("http://all:3".into()),
        "no_proxy" => Some("*".into()),
        _ => None,
    };
    let settings = proxy_from_env(env).unwrap();
    assert_eq!(settings.http.unwrap().host_str(), Some("upper"));
    assert_eq!(settings.https.unwrap().host_str(), Some("all"));
    assert!(settings.no_proxy.matches("anything.example", 443));
    assert!(proxy_from_env(|_| None).is_none());
    assert!(proxy_from_env(|_| Some(String::new())).is_none());
}

#[test]
fn no_proxy_rules() {
    let rules = NoProxy::parse("example.com, .corp.test ,192.168.0.0/16,::1,[fd00::]/8,db:5432");
    for (host, port, expected) in [
        ("example.com", 80, true),
        ("www.example.com", 443, true),
        ("notexample.com", 80, false),
        ("corp.test", 80, true),
        ("a.b.corp.test", 80, true),
        ("192.168.4.5", 80, true),
        ("192.169.0.1", 80, false),
        ("fd00::1", 80, true),
        ("db", 5432, true),
        ("db", 80, false),
        ("127.0.0.1", 80, true),
        ("localhost", 80, true),
        ("::1", 80, true),
        ("[::1]", 80, true),
        ("other.test", 80, false),
    ] {
        assert_eq!(rules.matches(host, port), expected, "{host}:{port}");
    }
    assert!(!NoProxy::parse("").matches("example.com", 80));
    assert!(NoProxy::parse("").matches("127.5.5.5", 80));
}

#[test]
fn debug_hides_proxy_credentials() {
    let settings = ProxySettings {
        http: Some("http://user:pw-secret@proxy.local:3128".parse().unwrap()),
        https: None,
        no_proxy: NoProxy::default(),
    };
    let shown = format!("{settings:?} {:?}", ProxyConfig::Fixed(settings.clone()));
    assert!(shown.contains("proxy.local"), "{shown}");
    assert!(
        !shown.contains("pw-secret") && !shown.contains("user:"),
        "{shown}"
    );
}

#[test]
fn system_proxy_reads_without_error() {
    // The only test that reads the real settings; it checks nothing about
    // their value.
    let _ = system_proxy();
}

#[tokio::test]
async fn https_tunnels_with_connect() {
    let server = tls_server().await;
    let proxy = TestProxy::start().await;
    let url = format!("https://dea-target.test:{}/status/204", server.port());
    let response = send(&via(&proxy.url(), ""), get_trusting(&url, &server))
        .await
        .unwrap();
    assert_eq!(response.status, 204);
    assert_eq!(
        proxy.connects(),
        [format!("dea-target.test:{}", server.port())]
    );
    assert!(proxy.forwarded().is_empty());
    assert!(response.timing.tls > Duration::ZERO);
}

#[tokio::test]
async fn tunnel_is_reused_for_the_next_request() {
    let server = tls_server().await;
    let proxy = TestProxy::start().await;
    let client = via(&proxy.url(), "");
    let url = format!("https://dea-target.test:{}/status/204", server.port());
    send(&client, get_trusting(&url, &server)).await.unwrap();
    let second = send(&client, get_trusting(&url, &server)).await.unwrap();
    assert!(second.timing.reused);
    assert_eq!(proxy.connects().len(), 1);
}

#[tokio::test]
async fn http_goes_through_the_proxy_in_absolute_form() {
    let server = TestServer::start(Tls::None).await;
    let proxy = TestProxy::start().await;
    let url = format!("http://dea-target.test:{}/status/204?a=1", server.port());
    let response = send(&via(&proxy.url(), ""), get(&url, Options::default()))
        .await
        .unwrap();
    assert_eq!(response.status, 204);
    assert_eq!(proxy.forwarded(), [url]);
    assert!(proxy.connects().is_empty());
    assert_eq!(server.requests()[0].path_and_query, "/status/204?a=1");
}

#[tokio::test]
async fn proxy_credentials_are_sent() {
    let plain = TestServer::start(Tls::None).await;
    let secure = tls_server().await;
    let proxy = TestProxy::start().await;
    let client = via(&proxy.url_with_userinfo("user:pw"), "");
    let http = format!("http://dea-target.test:{}/status/204", plain.port());
    let https = format!("https://dea-target.test:{}/status/204", secure.port());
    send(&client, get(&http, Options::default())).await.unwrap();
    send(&client, get_trusting(&https, &secure)).await.unwrap();
    let seen = proxy.seen();
    assert_eq!(seen.len(), 2);
    for request in &seen {
        assert_eq!(
            request.header("proxy-authorization"),
            Some("Basic dXNlcjpwdw==")
        );
    }
    // The credentials are for the proxy, not for the server behind it.
    assert!(plain.requests()[0].header("proxy-authorization").is_none());
}

#[tokio::test]
async fn refused_connect_is_a_proxy_error() {
    let server = tls_server().await;
    let proxy = TestProxy::refusing_connect().await;
    let url = format!("https://dea-target.test:{}/status/204", server.port());
    let client = via(&proxy.url_with_userinfo("user:pw-secret"), "");
    let Err(HttpError::Proxy(message)) = send(&client, get_trusting(&url, &server)).await else {
        panic!("expected a proxy error");
    };
    assert!(message.contains("403"), "{message}");
    assert!(!message.contains("pw-secret"), "{message}");
}

#[tokio::test]
async fn unreachable_proxy_is_a_proxy_error_without_credentials() {
    let gone = {
        let proxy = TestProxy::start().await;
        proxy.url_with_userinfo("user:pw-secret")
    };
    let Err(HttpError::Proxy(message)) = send(
        &via(&gone, ""),
        get("http://dea-target.test:9/", Options::default()),
    )
    .await
    else {
        panic!("expected a proxy error");
    };
    assert!(!message.contains("pw-secret"), "{message}");
}

#[tokio::test]
async fn unsupported_proxy_scheme_is_an_error_not_a_direct_request() {
    let server = TestServer::start(Tls::None).await;
    let client = via("socks5://user:pw-secret@127.0.0.1:1", "");
    let url = format!("http://dea-target.test:{}/status/204", server.port());
    let Err(HttpError::Proxy(message)) = send(&client, get(&url, Options::default())).await else {
        panic!("expected a proxy error");
    };
    assert!(message.contains("socks5"), "{message}");
    assert!(!message.contains("pw-secret"), "{message}");
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn loopback_skips_the_proxy() {
    let server = TestServer::start(Tls::None).await;
    let proxy = TestProxy::start().await;
    let response = send(
        &via(&proxy.url(), ""),
        get(&server.url("/status/204"), Options::default()),
    )
    .await
    .unwrap();
    assert_eq!(response.status, 204);
    assert!(proxy.seen().is_empty());
}

#[tokio::test]
async fn no_proxy_entry_skips_the_proxy() {
    let server = TestServer::start(Tls::None).await;
    let proxy = TestProxy::start().await;
    let client = via(&proxy.url(), "dea-target.test");
    let options = Options {
        timeout: Duration::from_secs(2),
        ..Options::default()
    };
    let url = format!("http://dea-target.test:{}/status/204", server.port());
    // The name only resolves through the proxy, so a direct send fails.
    let _ = send(&client, get(&url, options)).await;
    assert!(proxy.seen().is_empty());
}

#[tokio::test]
async fn use_system_proxy_off_goes_direct() {
    let proxy = TestProxy::start().await;
    let options = Options {
        use_system_proxy: false,
        timeout: Duration::from_secs(2),
        ..Options::default()
    };
    let _ = send(
        &via(&proxy.url(), ""),
        get("http://dea-target.test:9/", options),
    )
    .await;
    assert!(proxy.seen().is_empty());
}

#[tokio::test]
async fn proxy_config_none_goes_direct() {
    let proxy = TestProxy::start().await;
    let client = Client::new(ClientConfig {
        proxy: ProxyConfig::None,
        ..ClientConfig::default()
    })
    .unwrap();
    let options = Options {
        timeout: Duration::from_secs(2),
        ..Options::default()
    };
    let _ = send(&client, get("http://dea-target.test:9/", options)).await;
    assert!(proxy.seen().is_empty());
}

#[tokio::test]
async fn cancel_ends_a_connect_that_never_answers() {
    // A listener that accepts and stays silent never answers CONNECT.
    let hold = dea_testkit::http::HoldServer::start().await;
    let client = via(hold.url().trim_end_matches('/'), "");
    let cancel = CancellationToken::new();
    let stopper = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        stopper.cancel();
    });
    let request = get("https://dea-target.test:443/", Options::default());
    let result = client.send(request, SendContext::new(&cancel)).await;
    assert!(matches!(result, Err(HttpError::Cancelled)), "{result:?}");
    assert!(hold.wait_closed(Duration::from_secs(5)).await.is_some());
}

#[tokio::test]
async fn timeout_covers_the_connect_exchange() {
    let hold = dea_testkit::http::HoldServer::start().await;
    let client = via(hold.url().trim_end_matches('/'), "");
    let options = Options {
        timeout: Duration::from_millis(300),
        ..Options::default()
    };
    let result = send(&client, get("https://dea-target.test:443/", options)).await;
    assert!(matches!(result, Err(HttpError::Timeout(_))), "{result:?}");
}

#[tokio::test]
async fn cross_origin_redirect_through_the_proxy_drops_credentials() {
    let server = TestServer::start(Tls::None).await;
    let proxy = TestProxy::start().await;
    let port = server.port();
    let start = format!(
        "http://dea-target.test:{port}/redirect-to?url={}",
        url_encode(&format!("http://127.0.0.1:{port}/status/204"))
    );
    let mut request = get(&start, Options::default());
    request.headers = vec![("Authorization".into(), "Bearer t".into())];
    let response = send(&via(&proxy.url(), ""), request).await.unwrap();
    assert_eq!(response.status, 204);
    // The first hop went through the proxy, the loopback hop did not.
    assert_eq!(proxy.forwarded().len(), 1);
    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].header("authorization").is_some());
    assert!(seen[1].header("authorization").is_none());
}

fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
