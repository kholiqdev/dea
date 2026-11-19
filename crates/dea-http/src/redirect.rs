//! Deciding where a redirect leads and what the next request carries
//! (decision 17).

use url::Url;

use crate::error::HttpError;
use crate::request::{Body, Request};

/// The request to send after `current` answered `status` with `location`,
/// or `None` when `status` is not a redirect.
///
/// - 301, 302 and 303 turn any method but `GET` and `HEAD` into `GET`
///   without a body, and drop `Content-Type` and `Content-Length`.
/// - 307 and 308 keep the method and the body.
/// - A hop to another scheme, host or port drops every header that carries
///   credentials (see [`Request::credential_headers`]), and the TLS client
///   identity.
pub(crate) fn next_hop(
    current: &Request,
    status: u16,
    location: &str,
) -> Result<Option<Request>, HttpError> {
    if !matches!(status, 301 | 302 | 303 | 307 | 308) {
        return Ok(None);
    }
    // The messages leave the location out; it may hold a token.
    let url = current
        .url
        .join(location)
        .map_err(|_| HttpError::Protocol("the Location header is not a valid URL".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(HttpError::Protocol(format!(
            "redirect to unsupported scheme {}",
            url.scheme()
        )));
    }

    let mut next = current.clone();
    let keeps_method = matches!(status, 307 | 308)
        || current.method.eq_ignore_ascii_case("GET")
        || current.method.eq_ignore_ascii_case("HEAD");
    if !keeps_method {
        next.method = "GET".into();
        next.body = Body::Empty;
        drop_headers(&mut next, &["content-type", "content-length"]);
    }
    if origin(&current.url) != origin(&url) {
        next.headers
            .retain(|(name, _)| !current.is_credential(name));
        // Like the headers, the client certificate belongs to the origin
        // the user sent it to.
        next.tls.client_identity = None;
    }
    next.url = url;
    Ok(Some(next))
}

fn origin(url: &Url) -> (&str, Option<url::Host<&str>>, Option<u16>) {
    (url.scheme(), url.host(), url.port_or_known_default())
}

fn drop_headers(request: &mut Request, names: &[&str]) {
    request
        .headers
        .retain(|(name, _)| !names.iter().any(|n| name.eq_ignore_ascii_case(n)));
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;
    use crate::request::{ClientIdentity, Options, TlsConfig};

    fn request(method: &str, url: &str) -> Request {
        Request {
            method: method.into(),
            url: url.parse().unwrap(),
            headers: vec![
                ("Content-Type".into(), "text/plain".into()),
                ("content-length".into(), "3".into()),
                ("Authorization".into(), "Bearer t".into()),
                ("COOKIE".into(), "s=1".into()),
                ("X-Keep".into(), "1".into()),
            ],
            credential_headers: Vec::new(),
            body: Body::Bytes {
                data: Bytes::from_static(b"abc"),
                content_type: Some("text/plain".into()),
            },
            options: Options::default(),
            tls: TlsConfig {
                extra_roots_pem: vec![b"root".to_vec()],
                client_identity: Some(ClientIdentity {
                    certificate_pem: b"cert".to_vec(),
                    key_pem: zeroize::Zeroizing::new(b"key".to_vec()),
                }),
            },
        }
    }

    fn names(request: &Request) -> Vec<String> {
        request
            .headers
            .iter()
            .map(|(name, _)| name.to_ascii_lowercase())
            .collect()
    }

    fn hop(method: &str, status: u16, location: &str) -> Request {
        next_hop(&request(method, "http://a.test/x/y?q=1"), status, location)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn only_redirect_statuses_lead_anywhere() {
        let current = request("GET", "http://a.test/");
        for status in [200, 204, 300, 304, 305, 404, 500] {
            assert!(next_hop(&current, status, "/z").unwrap().is_none());
        }
    }

    #[test]
    fn post_becomes_get_without_body_for_301_302_303() {
        for status in [301, 302, 303] {
            let next = hop("POST", status, "/z");
            assert_eq!(next.method, "GET");
            assert!(matches!(next.body, Body::Empty));
            assert_eq!(names(&next), ["authorization", "cookie", "x-keep"]);
        }
    }

    #[test]
    fn get_and_head_keep_their_method() {
        for method in ["GET", "HEAD", "head"] {
            for status in [301, 302, 303] {
                let next = hop(method, status, "/z");
                assert_eq!(next.method, method);
            }
        }
    }

    #[test]
    fn temporary_and_permanent_redirects_keep_method_and_body() {
        for status in [307, 308] {
            let next = hop("POST", status, "/z");
            assert_eq!(next.method, "POST");
            assert!(matches!(&next.body, Body::Bytes { data, .. } if data.as_ref() == b"abc"));
            assert!(names(&next).contains(&"content-type".to_owned()));
        }
    }

    #[test]
    fn relative_locations_resolve_against_the_current_url() {
        assert_eq!(hop("GET", 302, "/z").url.as_str(), "http://a.test/z");
        assert_eq!(hop("GET", 302, "z").url.as_str(), "http://a.test/x/z");
        assert_eq!(
            hop("GET", 302, "?r=2").url.as_str(),
            "http://a.test/x/y?r=2"
        );
        assert_eq!(
            hop("GET", 302, "//b.test/p").url.as_str(),
            "http://b.test/p"
        );
        assert_eq!(
            hop("GET", 302, "https://b.test/p").url.as_str(),
            "https://b.test/p"
        );
    }

    #[test]
    fn credentials_survive_only_within_one_origin() {
        let kept = |location: &str| names(&hop("GET", 302, location));
        assert!(kept("/z").contains(&"authorization".to_owned()));
        assert!(kept("http://a.test:80/z").contains(&"cookie".to_owned()));
        for location in [
            "http://b.test/z",
            "https://a.test/z",
            "http://a.test:8080/z",
        ] {
            let names = kept(location);
            assert!(!names.contains(&"authorization".to_owned()), "{location}");
            assert!(!names.contains(&"cookie".to_owned()), "{location}");
            assert!(names.contains(&"x-keep".to_owned()), "{location}");
        }
    }

    #[test]
    fn marked_and_proxy_credentials_survive_only_within_one_origin() {
        let mut current = request("GET", "http://a.test/x");
        current.headers = vec![
            ("X-Api-Key".into(), "k".into()),
            ("x-from-vault".into(), "v".into()),
            ("Proxy-Authorization".into(), "Basic p".into()),
            ("X-Keep".into(), "1".into()),
        ];
        current.credential_headers = vec!["x-api-key".into(), "X-From-Vault".into()];
        let all = ["x-api-key", "x-from-vault", "proxy-authorization", "x-keep"];

        let same = next_hop(&current, 302, "/z").unwrap().unwrap();
        assert_eq!(names(&same), all);
        for location in [
            "http://b.test/z",
            "https://a.test/z",
            "http://a.test:8080/z",
        ] {
            let next = next_hop(&current, 302, location).unwrap().unwrap();
            assert_eq!(names(&next), ["x-keep"], "{location}");
        }
    }

    #[test]
    fn client_identity_survives_only_within_one_origin() {
        let same = hop("GET", 302, "/z");
        assert!(same.tls.client_identity.is_some());
        for location in [
            "http://b.test/z",
            "https://a.test/z",
            "http://a.test:8080/z",
        ] {
            let next = hop("GET", 302, location);
            assert!(next.tls.client_identity.is_none(), "{location}");
            assert_eq!(next.tls.extra_roots_pem.len(), 1, "{location}");
        }
    }

    #[test]
    fn bad_locations_are_protocol_errors_that_do_not_echo_them() {
        let current = request("GET", "http://a.test/");
        let invalid = next_hop(&current, 302, "http://[bad").unwrap_err();
        assert!(matches!(invalid, HttpError::Protocol(_)));
        let scheme = next_hop(&current, 302, "ftp://b.test/secret").unwrap_err();
        assert!(matches!(&scheme, HttpError::Protocol(m) if !m.contains("secret")));
    }
}
