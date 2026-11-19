//! Every body mode, byte for byte, against the local test server.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use dea_http::{
    Body, CancellationToken, Client, ClientConfig, HttpError, Options, Part, PartValue, Request,
    Response, SendContext, TlsConfig,
};
use dea_testkit::http::{Received, TestServer, Tls};

fn client() -> Client {
    Client::new(ClientConfig::default()).unwrap()
}

fn post(url: &str, headers: Vec<(String, String)>, body: Body) -> Request {
    Request {
        method: "POST".into(),
        url: url.parse().unwrap(),
        headers,
        body,
        options: Options::default(),
        tls: TlsConfig::default(),
    }
}

async fn send(request: Request) -> Result<Response, HttpError> {
    let cancel = CancellationToken::new();
    client().send(request, SendContext::new(&cancel)).await
}

/// Sends `body` to `/echo` and returns what the server recorded.
async fn post_and_receive(server: &TestServer, body: Body) -> Received {
    send(post(&server.url("/echo"), Vec::new(), body))
        .await
        .unwrap();
    server.requests().pop().unwrap()
}

fn write(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, data).unwrap();
    path
}

fn binary() -> Vec<u8> {
    (0..=255u8).cycle().take(200_000).collect()
}

fn boundary_of(content_type: &str) -> &str {
    content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("a multipart content type")
}

#[tokio::test]
async fn each_body_arrives_byte_for_byte() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let payload = binary();
    let path = write(dir.path(), "payload.bin", &payload);

    let json = Body::Bytes {
        data: Bytes::from_static(b"{\"a\":1}"),
        content_type: Some("application/json".into()),
    };
    let received = post_and_receive(&server, json).await;
    assert_eq!(received.header("content-type"), Some("application/json"));
    assert_eq!(received.body, b"{\"a\":1}");

    let xml = Body::Bytes {
        data: Bytes::from_static(b"<a/>"),
        content_type: Some("application/xml".into()),
    };
    let received = post_and_receive(&server, xml).await;
    assert_eq!(received.header("content-type"), Some("application/xml"));
    assert_eq!(received.body, b"<a/>");

    let form = Body::Form(vec![
        ("a".into(), "1".into()),
        ("b".into(), "x y".into()),
        ("c".into(), "é&=".into()),
    ]);
    let received = post_and_receive(&server, form).await;
    assert_eq!(
        received.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(received.body, b"a=1&b=x+y&c=%C3%A9%26%3D");

    let file = Body::File {
        path: path.clone(),
        content_type: None,
    };
    let received = post_and_receive(&server, file).await;
    assert_eq!(
        received.header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(received.header("content-length"), Some("200000"));
    assert_eq!(received.body, payload);

    let file = Body::File {
        path: path.clone(),
        content_type: Some("image/png".into()),
    };
    let received = post_and_receive(&server, file).await;
    assert_eq!(received.header("content-type"), Some("image/png"));

    let multipart = Body::Multipart(vec![
        Part {
            name: "note".into(),
            value: PartValue::Text("hi".into()),
        },
        Part {
            name: "upload".into(),
            value: PartValue::File {
                path,
                content_type: None,
            },
        },
        Part {
            name: "after".into(),
            value: PartValue::Text("end".into()),
        },
    ]);
    let received = post_and_receive(&server, multipart).await;
    let content_type = received.header("content-type").unwrap();
    let boundary = boundary_of(content_type);
    assert_eq!(boundary.len(), 32);
    assert!(boundary.chars().all(|c| c.is_ascii_alphanumeric()));
    let mut expected = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhi\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"upload\"; \
         filename=\"payload.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    expected.extend_from_slice(&payload);
    expected.extend_from_slice(
        format!(
            "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"after\"\r\n\r\nend\r\n\
             --{boundary}--\r\n"
        )
        .as_bytes(),
    );
    assert_eq!(
        received.header("content-length"),
        Some(&*expected.len().to_string())
    );
    assert_eq!(received.body, expected);
}

#[tokio::test]
async fn fifty_megabyte_file_is_streamed() {
    const SIZE: usize = 50 * 1024 * 1024;
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.bin");
    {
        use std::io::Write as _;
        let pattern: Vec<u8> = (0..=250u8).cycle().take(1024 * 1024).collect();
        let mut file = std::fs::File::create(&path).unwrap();
        for _ in 0..50 {
            file.write_all(&pattern).unwrap();
        }
    }
    let response = send(post(
        &server.url("/echo?quiet"),
        Vec::new(),
        Body::File {
            path,
            content_type: None,
        },
    ))
    .await
    .unwrap();
    assert_eq!(response.status, 200);
    server.with_requests(|requests| {
        let received = &requests[0];
        assert_eq!(received.header("content-length"), Some("52428800"));
        assert_eq!(received.body.len(), SIZE);
        let intact = received
            .body
            .iter()
            .enumerate()
            .all(|(i, byte)| usize::from(*byte) == (i % (1024 * 1024)) % 251);
        assert!(intact, "the body differs from the file");
    });
}

#[tokio::test]
async fn user_content_type_wins() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let path = write(dir.path(), "a.txt", b"abc");
    let bodies = [
        Body::Form(vec![("a".into(), "1".into())]),
        Body::File {
            path: path.clone(),
            content_type: Some("image/png".into()),
        },
        Body::Multipart(vec![Part {
            name: "a".into(),
            value: PartValue::File {
                path,
                content_type: None,
            },
        }]),
    ];
    for body in bodies {
        let headers = vec![("Content-Type".to_owned(), "text/plain".to_owned())];
        send(post(&server.url("/echo"), headers, body))
            .await
            .unwrap();
        let received = server.requests().pop().unwrap();
        let types: Vec<_> = received
            .headers
            .iter()
            .filter(|(name, _)| name == "content-type")
            .collect();
        assert_eq!(types.len(), 1, "{:?}", received.headers);
        assert_eq!(types[0].1, "text/plain");
    }
}

#[tokio::test]
async fn multipart_names_are_escaped() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    // `"` is not allowed in file names on Windows.
    let file_name = if cfg!(windows) {
        "plain.txt"
    } else {
        "say \"hi\".txt"
    };
    let path = write(dir.path(), file_name, b"abc");
    let body = Body::Multipart(vec![
        Part {
            name: "na\"me\r\nX: y".into(),
            value: PartValue::Text("v".into()),
        },
        Part {
            name: "f".into(),
            value: PartValue::File {
                path,
                content_type: Some("text/plain".into()),
            },
        },
    ]);
    let received = post_and_receive(&server, body).await;
    let text = String::from_utf8(received.body).unwrap();
    assert!(
        text.contains("name=\"na%22me%0D%0AX: y\"\r\n\r\nv\r\n"),
        "{text}"
    );
    let escaped = file_name.replace('"', "%22");
    assert!(
        text.contains(&format!(
            "name=\"f\"; filename=\"{escaped}\"\r\nContent-Type: text/plain\r\n\r\nabc\r\n"
        )),
        "{text}"
    );
}

#[tokio::test]
async fn missing_body_file_names_the_file() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("gone.bin");
    let bodies = [
        Body::File {
            path: missing.clone(),
            content_type: None,
        },
        Body::Multipart(vec![Part {
            name: "f".into(),
            value: PartValue::File {
                path: missing.clone(),
                content_type: None,
            },
        }]),
    ];
    for body in bodies {
        let error = send(post(&server.url("/echo"), Vec::new(), body))
            .await
            .unwrap_err();
        let HttpError::File { path, .. } = &error else {
            panic!("expected File, got {error}");
        };
        assert_eq!(path, &missing);
        assert!(error.to_string().contains("gone.bin"), "{error}");
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn post_body_is_replayed_after_a_stale_connection() {
    let dir = tempfile::tempdir().unwrap();
    let payload = binary();
    let path = write(dir.path(), "payload.bin", &payload);
    for _ in 0..20 {
        let server = TestServer::start(Tls::None).await;
        let client = client();
        let cancel = CancellationToken::new();
        client
            .send(
                post(&server.url("/close"), Vec::new(), Body::Empty),
                SendContext::new(&cancel),
            )
            .await
            .unwrap();
        let body = Body::File {
            path: path.clone(),
            content_type: None,
        };
        let response = client
            .send(
                post(&server.url("/echo"), Vec::new(), body),
                SendContext::new(&cancel),
            )
            .await
            .unwrap();
        assert_eq!(response.body.read_all().unwrap(), payload);
        assert_eq!(server.requests().pop().unwrap().body, payload);
        assert_eq!(server.connections(), 2);
    }
}

#[tokio::test]
async fn redirect_307_streams_the_file_again() {
    let server = TestServer::start(Tls::None).await;
    let dir = tempfile::tempdir().unwrap();
    let payload = binary();
    let path = write(dir.path(), "payload.bin", &payload);
    let target = server.url("/echo");
    let hop = format!(
        "/redirect-to?status=307&url={}",
        url::form_urlencoded::byte_serialize(target.as_bytes()).collect::<String>()
    );
    let body = Body::File {
        path,
        content_type: None,
    };
    let response = send(post(&server.url(&hop), Vec::new(), body))
        .await
        .unwrap();
    assert_eq!(response.redirects.len(), 1);
    assert_eq!(response.body.read_all().unwrap(), payload);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| r.body == payload && r.method == "POST")
    );
}
