//! TLS failures named by kind, and which failed requests are sent again on
//! a new connection, against the local test server.

use dea_http::{
    Body, CancellationToken, Client, ClientConfig, HttpError, Options, Request, Response,
    SendContext, TlsConfig, TlsError, TlsErrorKind,
};
use dea_testkit::http::{TestServer, Tls};

fn client() -> Client {
    Client::new(ClientConfig {
        user_agent: "Dea/test".into(),
        ..ClientConfig::default()
    })
    .unwrap()
}

fn request(method: &str, url: &str, body: Body) -> Request {
    Request {
        method: method.into(),
        url: url.parse().unwrap(),
        headers: Vec::new(),
        credential_headers: Vec::new(),
        body,
        options: Options::default(),
        tls: TlsConfig::default(),
    }
}

fn get(url: &str) -> Request {
    request("GET", url, Body::Empty)
}

fn post(url: &str, body: &'static str) -> Request {
    let body = Body::Bytes {
        data: bytes::Bytes::from_static(body.as_bytes()),
        content_type: None,
    };
    request("POST", url, body)
}

/// Trusts the test CA, with no client certificate.
fn trusting(server: &TestServer) -> TlsConfig {
    TlsConfig {
        extra_roots_pem: vec![server.ca_pem()],
        client_identity: None,
    }
}

async fn send(client: &Client, request: Request) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client.send(request, SendContext::new(&cancel)).await
}

/// How many requests for `path` the server received.
fn received(server: &TestServer, path: &str) -> usize {
    server
        .requests()
        .iter()
        .filter(|r| r.path_and_query == path)
        .count()
}

#[tokio::test]
async fn client_certificate_rejection_is_a_tls_error() {
    for alpn in ["http/1.1", "h2"] {
        let server = TestServer::start(Tls::Mutual {
            alpn: vec![alpn.as_bytes().to_vec()],
        })
        .await;
        let request = Request {
            tls: trusting(&server),
            ..get(&server.url("/status/200"))
        };
        let err = send(&client(), request).await.unwrap_err();
        assert!(
            matches!(
                err,
                HttpError::Tls(TlsError {
                    kind: TlsErrorKind::ClientCertificateRejected,
                    ..
                })
            ),
            "{alpn}: {err:?}"
        );
    }
}

#[tokio::test]
async fn unknown_issuer_is_named() {
    let server = TestServer::start(Tls::SelfSigned {
        alpn: vec![b"http/1.1".to_vec()],
    })
    .await;
    let err = send(&client(), get(&server.url("/status/200")))
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::Tls(TlsError {
                kind: TlsErrorKind::UnknownIssuer,
                ..
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn expired_certificate_is_named() {
    let server = TestServer::start(Tls::Expired).await;
    let request = Request {
        tls: trusting(&server),
        ..get(&server.url("/status/200"))
    };
    let err = send(&client(), request).await.unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::Tls(TlsError {
                kind: TlsErrorKind::Expired,
                ..
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn wrong_host_is_named() {
    let server = TestServer::start(Tls::OtherHost).await;
    let url = server.url("/status/200");
    assert!(url.starts_with("https://localhost:"), "{url}");
    let request = Request {
        tls: trusting(&server),
        ..get(&url)
    };
    let err = send(&client(), request).await.unwrap_err();
    assert!(
        matches!(
            err,
            HttpError::Tls(TlsError {
                kind: TlsErrorKind::WrongHost,
                ..
            })
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_post_is_not_resent_after_the_server_closes_without_answering() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    // Leaves a connection in the pool.
    send(&client, get(&server.url("/status/200")))
        .await
        .unwrap();
    let err = send(&client, post(&server.url("/hang-up"), "x"))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::Protocol(_)), "{err:?}");
    assert_eq!(received(&server, "/hang-up"), 1);
}

#[tokio::test]
async fn a_get_is_still_resent_after_the_server_closes_without_answering() {
    let server = TestServer::start(Tls::None).await;
    let client = client();
    send(&client, get(&server.url("/status/200")))
        .await
        .unwrap();
    let err = send(&client, get(&server.url("/hang-up")))
        .await
        .unwrap_err();
    assert!(matches!(err, HttpError::Protocol(_)), "{err:?}");
    assert_eq!(received(&server, "/hang-up"), 2);
}
