//! The request to send, and its checked form ready for the wire.

use std::fmt;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{ACCEPT, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use hyper::{Method, Uri};
use url::Url;
use zeroize::Zeroizing;

use crate::error::HttpError;
use crate::response::HttpVersion;

/// One request. `headers` are sent in order and always win over the
/// headers Dea adds on its own (`Host`, `User-Agent`, `Accept`,
/// `Content-Type`, `Content-Length`).
#[derive(Clone)]
pub struct Request {
    pub method: String,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Body,
    pub options: Options,
    pub tls: TlsConfig,
}

/// Shows only where the request goes. Query, headers and body may hold
/// secrets.
impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request")
            .field("method", &self.method)
            .field("scheme", &self.url.scheme())
            .field("host", &self.url.host_str())
            .field("port", &self.url.port_or_known_default())
            .field("path", &self.url.path())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub timeout: Duration,
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub verify_tls: bool,
    pub use_system_proxy: bool,
    pub max_body_bytes: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            timeout: Duration::from_secs(30),
            follow_redirects: true,
            max_redirects: 10,
            verify_tls: true,
            use_system_proxy: true,
            max_body_bytes: 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsConfig {
    /// CA certificates to trust on top of the operating system's, in PEM.
    pub extra_roots_pem: Vec<Vec<u8>>,
    pub client_identity: Option<ClientIdentity>,
}

/// A client certificate and its private key, both in PEM.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    pub certificate_pem: Vec<u8>,
    pub key_pem: Zeroizing<Vec<u8>>,
}

/// Leaves the key out.
impl fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientIdentity")
            .field(
                "certificate_pem",
                &format_args!("{} bytes", self.certificate_pem.len()),
            )
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Default)]
pub enum Body {
    #[default]
    Empty,
    Bytes {
        data: Bytes,
        content_type: Option<String>,
    },
}

/// Shows the size of the body, not its bytes.
impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Body::Empty => f.write_str("Empty"),
            Body::Bytes { data, content_type } => f
                .debug_struct("Bytes")
                .field("len", &data.len())
                .field("content_type", content_type)
                .finish(),
        }
    }
}

/// The body type the connections carry.
pub(crate) type WireBody = Full<Bytes>;

/// A request checked once and turned into a wire request for each attempt.
pub(crate) struct Prepared {
    method: Method,
    /// `path?query`, for HTTP/1.1.
    origin_form: Uri,
    /// `scheme://host:port/path?query`, for HTTP/2. No user info or fragment.
    absolute_form: Uri,
    /// `None` when the user wrote their own `Host`.
    host: Option<HeaderValue>,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Bytes,
}

impl Prepared {
    pub(crate) fn new(request: &Request, user_agent: &HeaderValue) -> Result<Self, HttpError> {
        let method = Method::from_bytes(request.method.as_bytes()).map_err(|_| {
            HttpError::InvalidRequest(format!("invalid method {:?}", request.method))
        })?;
        let url = &request.url;
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(HttpError::InvalidRequest(format!(
                "unsupported URL scheme {scheme}"
            )));
        }
        let Some(host) = url.host_str() else {
            return Err(HttpError::InvalidRequest("the URL has no host".into()));
        };
        let authority = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        let path_and_query = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };
        let not_sendable = || HttpError::InvalidRequest("the URL cannot be sent".into());
        let origin_form = Uri::try_from(path_and_query.as_str()).map_err(|_| not_sendable())?;
        let absolute_form = Uri::try_from(format!("{scheme}://{authority}{path_and_query}"))
            .map_err(|_| not_sendable())?;

        let mut user = Vec::with_capacity(request.headers.len());
        for (name, value) in &request.headers {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| HttpError::InvalidRequest(format!("invalid header name {name:?}")))?;
            let value = HeaderValue::from_bytes(value.as_bytes()).map_err(|_| {
                HttpError::InvalidRequest(format!(
                    "the value of header {name} has a line break or control character"
                ))
            })?;
            user.push((header, value));
        }
        let user_wrote = |name: &HeaderName| user.iter().any(|(n, _)| n == name);

        let (body, content_type) = match &request.body {
            Body::Empty => (Bytes::new(), None),
            Body::Bytes { data, content_type } => (data.clone(), content_type.as_deref()),
        };
        let mut headers = Vec::with_capacity(user.len() + 3);
        if !user_wrote(&USER_AGENT) {
            headers.push((USER_AGENT, user_agent.clone()));
        }
        if !user_wrote(&ACCEPT) {
            headers.push((ACCEPT, HeaderValue::from_static("*/*")));
        }
        if let Some(content_type) = content_type
            && !user_wrote(&CONTENT_TYPE)
        {
            let value = HeaderValue::from_str(content_type).map_err(|_| {
                HttpError::InvalidRequest("the body content type has a control character".into())
            })?;
            headers.push((CONTENT_TYPE, value));
        }
        let host = if user_wrote(&HOST) {
            None
        } else {
            Some(HeaderValue::from_str(&authority).map_err(|_| not_sendable())?)
        };
        headers.extend(user);

        Ok(Prepared {
            method,
            origin_form,
            absolute_form,
            host,
            headers,
            body,
        })
    }

    /// The request for one attempt. HTTP/1.1 gets the origin form and a
    /// `Host` header; HTTP/2 gets the full URI, which becomes `:authority`.
    /// `Content-Length` is added by hyper from the body unless the user
    /// wrote one.
    pub(crate) fn to_wire(&self, version: HttpVersion) -> hyper::Request<WireBody> {
        let mut headers = HeaderMap::with_capacity(self.headers.len() + 1);
        let uri = match version {
            HttpVersion::Http11 => {
                if let Some(host) = &self.host {
                    headers.insert(HOST, host.clone());
                }
                self.origin_form.clone()
            }
            HttpVersion::Http2 => self.absolute_form.clone(),
        };
        for (name, value) in &self.headers {
            headers.append(name.clone(), value.clone());
        }
        let mut request = hyper::Request::new(Full::new(self.body.clone()));
        *request.method_mut() = self.method.clone();
        *request.uri_mut() = uri;
        *request.headers_mut() = headers;
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_only_where_the_request_goes() {
        let request = Request {
            method: "POST".into(),
            url: "https://api.example.com:8443/v1/items?token=query-secret#frag"
                .parse()
                .unwrap(),
            headers: vec![("Authorization".into(), "Bearer header-secret".into())],
            body: Body::Bytes {
                data: Bytes::from_static(b"body-secret"),
                content_type: None,
            },
            options: Options::default(),
            tls: TlsConfig {
                extra_roots_pem: Vec::new(),
                client_identity: Some(ClientIdentity {
                    certificate_pem: b"cert".to_vec(),
                    key_pem: Zeroizing::new(b"key-secret".to_vec()),
                }),
            },
        };
        let shown = format!("{request:?}");
        for part in ["POST", "https", "api.example.com", "8443", "/v1/items"] {
            assert!(shown.contains(part), "{shown}");
        }
        for secret in ["query-secret", "header-secret", "body-secret", "key-secret"] {
            assert!(!shown.contains(secret), "{shown}");
        }
        let tls = format!("{:?} {:?}", request.tls, request.body);
        for secret in ["key-secret", "body-secret"] {
            assert!(!tls.contains(secret), "{tls}");
        }
    }

    #[test]
    fn user_headers_replace_automatic_ones() {
        let request = Request {
            method: "GET".into(),
            url: "http://example.com/a?b=1".parse().unwrap(),
            headers: vec![
                ("host".into(), "other.example".into()),
                ("ACCEPT".into(), "text/plain".into()),
            ],
            body: Body::Empty,
            options: Options::default(),
            tls: TlsConfig::default(),
        };
        let prepared = Prepared::new(&request, &HeaderValue::from_static("Dea/test")).unwrap();
        let wire = prepared.to_wire(HttpVersion::Http11);
        assert_eq!(wire.uri(), "/a?b=1");
        assert_eq!(wire.headers().get_all(HOST).iter().count(), 1);
        assert_eq!(wire.headers()[HOST], "other.example");
        assert_eq!(wire.headers().get_all(ACCEPT).iter().count(), 1);
        assert_eq!(wire.headers()[ACCEPT], "text/plain");
        assert_eq!(wire.headers()[USER_AGENT], "Dea/test");
        let wire = prepared.to_wire(HttpVersion::Http2);
        assert_eq!(wire.uri(), "http://example.com/a?b=1");
    }

    #[test]
    fn http2_target_has_no_user_info() {
        let request = Request {
            method: "GET".into(),
            url: "https://user:pw@example.com:8443/".parse().unwrap(),
            headers: Vec::new(),
            body: Body::Empty,
            options: Options::default(),
            tls: TlsConfig::default(),
        };
        let prepared = Prepared::new(&request, &HeaderValue::from_static("Dea/test")).unwrap();
        assert_eq!(
            prepared.to_wire(HttpVersion::Http2).uri(),
            "https://example.com:8443/"
        );
        assert_eq!(
            prepared.to_wire(HttpVersion::Http11).headers()[HOST],
            "example.com:8443"
        );
    }
}
