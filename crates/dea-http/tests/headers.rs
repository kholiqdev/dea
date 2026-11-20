//! The headers Dea adds on its own are the ones it reports.

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, Options, Request, SendContext, TlsConfig,
    automatic_headers,
};
use dea_testkit::http::{TestServer, Tls};

fn json_request(url: &str) -> Request {
    Request {
        method: "POST".into(),
        url: url.parse().unwrap(),
        headers: Vec::new(),
        credential_headers: Vec::new(),
        body: Body::Bytes {
            data: bytes::Bytes::from_static(b"{\"a\":1}"),
            content_type: Some("application/json".into()),
        },
        options: Options::default(),
        tls: TlsConfig::default(),
    }
}

#[tokio::test]
async fn automatic_headers_match_what_is_sent() {
    let server = TestServer::start(Tls::None).await;
    let client = Client::new(ClientConfig::default()).unwrap();
    let request = json_request(&server.url("/echo"));
    let listed = automatic_headers(&request.url, Some("application/json"), client.user_agent());

    let cancel = CancellationToken::new();
    client
        .send(request, SendContext::new(&cancel))
        .await
        .unwrap();
    let received = server.requests().pop().unwrap();

    let on_the_wire: Vec<&str> = received.headers.iter().map(|(n, _)| n.as_str()).collect();
    let names: Vec<String> = listed.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
    assert_eq!(on_the_wire, names);
    for (name, value) in &listed {
        match value {
            Some(value) => assert_eq!(received.header(name), Some(value.as_str()), "{name}"),
            None => assert_eq!(received.header(name), Some("7"), "{name}"),
        }
    }
}

#[test]
fn automatic_headers_are_listed_in_order_and_skip_a_missing_content_type() {
    let url: url::Url = "http://api.test:8080/x".parse().unwrap();
    let names = |content_type| -> Vec<&'static str> {
        automatic_headers(&url, content_type, "Dea/1")
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    };
    assert_eq!(
        names(None),
        ["Host", "User-Agent", "Accept", "Content-Length"]
    );
    assert_eq!(
        names(Some("text/plain")),
        [
            "Host",
            "User-Agent",
            "Accept",
            "Content-Type",
            "Content-Length"
        ]
    );
    let listed = automatic_headers(&url, None, "Dea/1");
    assert_eq!(listed[0], ("Host", Some("api.test:8080".to_string())));
    assert_eq!(listed[3], ("Content-Length", None));
}
