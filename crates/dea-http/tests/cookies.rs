//! The cookie jar against RFC 6265 vectors, and sending with a jar against
//! the local test server.

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, CookieJar, HttpError, Options, Request,
    Response, SendContext, StoredCookie, TlsConfig,
};
use dea_testkit::http::{TestServer, Tls};
use url::Url;

fn url(text: &str) -> Url {
    text.parse().unwrap()
}

/// The `Cookie` header value the jar gives for `at`, if any.
fn sent(jar: &CookieJar, at: &str) -> Option<String> {
    jar.header_for(&url(at))
}

#[test]
fn host_only_cookie_matches_only_its_host() {
    let jar = CookieJar::new();
    jar.store(&url("http://example.test/"), &["a=1"]);
    assert_eq!(sent(&jar, "http://example.test/x").as_deref(), Some("a=1"));
    assert_eq!(sent(&jar, "http://www.example.test/"), None);
    assert_eq!(sent(&jar, "http://other.test/"), None);
    assert!(!jar.list()[0].domain.is_empty());
    assert!(jar.list()[0].host_only);
}

#[test]
fn domain_cookie_matches_subdomains() {
    let jar = CookieJar::new();
    jar.store(
        &url("http://www.example.test/"),
        &["a=1; Domain=example.test"],
    );
    for at in [
        "http://example.test/",
        "http://www.example.test/",
        "http://deep.www.example.test/",
    ] {
        assert_eq!(sent(&jar, at).as_deref(), Some("a=1"), "{at}");
    }
    assert_eq!(sent(&jar, "http://notexample.test/"), None);
    assert!(!jar.list()[0].host_only);
    // A page may not set a cookie for a domain it is not inside.
    jar.store(
        &url("http://www.example.test/"),
        &["b=1; Domain=other.test"],
    );
    assert_eq!(jar.list().len(), 1);
}

#[test]
fn path_matching() {
    let jar = CookieJar::new();
    jar.store(&url("http://example.test/"), &["a=1; Path=/docs"]);
    for at in [
        "http://example.test/docs",
        "http://example.test/docs/",
        "http://example.test/docs/web",
    ] {
        assert_eq!(sent(&jar, at).as_deref(), Some("a=1"), "{at}");
    }
    for at in ["http://example.test/docsets", "http://example.test/"] {
        assert_eq!(sent(&jar, at), None, "{at}");
    }
}

#[test]
fn secure_cookie_only_over_https() {
    let jar = CookieJar::new();
    jar.store(&url("https://example.test/"), &["a=1; Secure"]);
    assert_eq!(sent(&jar, "https://example.test/").as_deref(), Some("a=1"));
    assert_eq!(sent(&jar, "http://example.test/"), None);
}

#[test]
fn max_age_zero_deletes() {
    let jar = CookieJar::new();
    jar.store(&url("http://example.test/"), &["a=1", "b=2"]);
    assert_eq!(jar.list().len(), 2);
    assert!(jar.take_changed());
    // Expiring a cookie the jar never had is no change.
    jar.store(&url("http://example.test/"), &["never=; Max-Age=0"]);
    assert!(!jar.take_changed());
    jar.store(&url("http://example.test/"), &["a=; Max-Age=0"]);
    assert!(jar.take_changed());
    assert_eq!(sent(&jar, "http://example.test/").as_deref(), Some("b=2"));
    assert_eq!(jar.list().len(), 1);
}

#[test]
fn expired_cookie_is_not_sent() {
    let jar = CookieJar::new();
    jar.store(
        &url("http://example.test/"),
        &["a=1; Expires=Thu, 01 Jan 1970 00:00:01 GMT", "b=2"],
    );
    assert_eq!(sent(&jar, "http://example.test/").as_deref(), Some("b=2"));
    // One that expires later is kept, with its date.
    jar.store(&url("http://example.test/"), &["c=3; Max-Age=3600"]);
    let kept = jar.list().into_iter().find(|c| c.name == "c").unwrap();
    assert!(kept.expires.is_some());
    // One that expired while stored is not sent either.
    let jar = CookieJar::new();
    jar.insert(StoredCookie {
        name: "old".into(),
        value: "1".into(),
        domain: "example.test".into(),
        path: "/".into(),
        expires: Some(time::OffsetDateTime::now_utc() - time::Duration::hours(1)),
        secure: false,
        http_only: false,
        host_only: true,
    })
    .unwrap();
    assert_eq!(sent(&jar, "http://example.test/"), None);
}

#[test]
fn session_cookies_survive_json() {
    let jar = CookieJar::new();
    jar.store(
        &url("https://example.test/app/"),
        &["s=abc; HttpOnly; Secure", "p=2; Max-Age=3600; Path=/"],
    );
    let json = jar.to_json().unwrap();
    let loaded = CookieJar::from_json(&json).unwrap();
    let (mut before, mut after) = (jar.list(), loaded.list());
    before.sort_by(|a, b| a.name.cmp(&b.name));
    after.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(before.len(), 2);
    assert_eq!(before, after);
    let session = after.iter().find(|c| c.name == "s").unwrap();
    assert!(session.expires.is_none() && session.http_only && session.secure);
    let header = sent(&loaded, "https://example.test/app/x").unwrap();
    let mut pairs: Vec<&str> = header.split("; ").collect();
    pairs.sort_unstable();
    assert_eq!(pairs, ["p=2", "s=abc"]);
}

#[test]
fn to_json_reports_failure_instead_of_an_empty_file() {
    // The result type is the contract: a caller can not mistake an error
    // for a saved jar.
    let jar = CookieJar::new();
    let json: Result<String, dea_http::CookieJarError> = jar.to_json();
    assert!(
        CookieJar::from_json(&json.unwrap())
            .unwrap()
            .list()
            .is_empty()
    );
}

#[test]
fn bad_json_is_an_error_without_the_text() {
    let error = CookieJar::from_json("{\"secret-value\": nope").unwrap_err();
    assert!(!error.to_string().contains("secret-value"));
}

#[test]
fn stored_cookie_debug_hides_the_value() {
    let jar = CookieJar::new();
    jar.store(&url("http://example.test/"), &["a=hunter2"]);
    let shown = format!("{:?}", jar.list()[0]);
    assert!(!shown.contains("hunter2"));
    assert!(shown.contains("a"));
}

#[test]
fn insert_remove_clear_and_changed() {
    let jar = CookieJar::new();
    assert!(!jar.take_changed());
    jar.insert(StoredCookie {
        name: "a".into(),
        value: "1".into(),
        domain: "example.test".into(),
        path: "/".into(),
        expires: None,
        secure: false,
        http_only: false,
        host_only: false,
    })
    .unwrap();
    assert!(jar.take_changed());
    assert!(!jar.take_changed());
    assert_eq!(
        sent(&jar, "http://sub.example.test/").as_deref(),
        Some("a=1")
    );
    assert!(!jar.remove("example.test", "/", "missing"));
    assert!(!jar.take_changed());
    assert!(jar.remove("example.test", "/", "a"));
    assert!(jar.take_changed());
    jar.store(&url("http://example.test/"), &["b=1"]);
    jar.clear();
    assert!(jar.list().is_empty());
    assert!(jar.take_changed());
    let bad = StoredCookie {
        name: "bad name".into(),
        value: "topsecret".into(),
        domain: "example.test".into(),
        path: "/".into(),
        expires: None,
        secure: false,
        http_only: false,
        host_only: true,
    };
    let error = jar.insert(bad).unwrap_err();
    assert!(!error.to_string().contains("topsecret"));
}

#[test]
fn the_jar_can_be_shared() {
    fn needs<T: Send + Sync>() {}
    needs::<CookieJar>();
}

fn client() -> Client {
    Client::new(ClientConfig {
        user_agent: "Dea/test".into(),
        ..ClientConfig::default()
    })
    .unwrap()
}

fn get(url: &str) -> Request {
    Request {
        method: "GET".into(),
        url: url.parse().unwrap(),
        headers: Vec::new(),
        body: Body::Empty,
        options: Options::default(),
        tls: TlsConfig::default(),
    }
}

async fn send_with_jar(
    client: &Client,
    request: Request,
    jar: &CookieJar,
) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client
        .send(request, SendContext::new(&cancel).cookies(jar))
        .await
}

#[tokio::test]
async fn cookies_round_trip_with_a_server() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    let jar = CookieJar::new();
    send_with_jar(&client, get(&server.url("/cookies/set?session=abc")), &jar)
        .await
        .unwrap();
    assert!(jar.take_changed());
    send_with_jar(&client, get(&server.url("/echo")), &jar)
        .await
        .unwrap();
    assert_eq!(
        server.requests().last().unwrap().header("cookie"),
        Some("session=abc")
    );
    assert!(!jar.take_changed());
}

#[tokio::test]
async fn cookies_set_on_a_redirect_are_sent_on_the_next_hop() {
    let server = TestServer::start(Tls::None).await;
    let jar = CookieJar::new();
    let response = send_with_jar(&client(), get(&server.url("/cookies/redirect")), &jar)
        .await
        .unwrap();
    assert_eq!(response.redirects.len(), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("cookie"), None);
    assert_eq!(requests[1].path_and_query, "/echo");
    assert_eq!(requests[1].header("cookie"), Some("hop=1"));
}

#[tokio::test]
async fn user_cookie_header_is_extended() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    let jar = CookieJar::new();
    send_with_jar(&client, get(&server.url("/cookies/set?session=abc")), &jar)
        .await
        .unwrap();
    let mut request = get(&server.url("/echo"));
    request.headers.push(("Cookie".into(), "a=1".into()));
    send_with_jar(&client, request, &jar).await.unwrap();
    let requests = server.requests();
    let last = requests.last().unwrap();
    assert_eq!(last.header("cookie"), Some("a=1; session=abc"));
    assert_eq!(
        last.headers.iter().filter(|(n, _)| n == "cookie").count(),
        1
    );
}

#[tokio::test]
async fn no_jar_sends_and_keeps_nothing() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    let cancel = CancellationToken::new();
    client
        .send(
            get(&server.url("/cookies/set?session=abc")),
            SendContext::new(&cancel),
        )
        .await
        .unwrap();
    // A jar that was not given to the first send has nothing from it.
    let jar = CookieJar::new();
    send_with_jar(&client, get(&server.url("/echo")), &jar)
        .await
        .unwrap();
    assert_eq!(server.requests().last().unwrap().header("cookie"), None);
    assert!(!jar.take_changed());
    // And without a jar the user's own header goes out untouched.
    let mut request = get(&server.url("/echo"));
    request.headers.push(("Cookie".into(), "a=1".into()));
    client
        .send(request, SendContext::new(&cancel))
        .await
        .unwrap();
    assert_eq!(
        server.requests().last().unwrap().header("cookie"),
        Some("a=1")
    );
}
