//! The request to send, and its checked form ready for the wire.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use bytes::Bytes;
use hyper::header::{
    ACCEPT, CONTENT_TYPE, HOST, HeaderMap, HeaderName, HeaderValue, PROXY_AUTHORIZATION, USER_AGENT,
};
use hyper::{Method, Uri};
use url::Url;
use zeroize::Zeroizing;

use crate::body::{Plan, WireBody};
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
    /// Names of the headers that carry credentials, such as an API key
    /// header or one whose value came from the Vault. Matched without
    /// regard to case. `Authorization`, `Proxy-Authorization` and `Cookie`
    /// always count. Credentials are dropped on a redirect to another
    /// origin and kept out of HTTP/2 header compression.
    pub credential_headers: Vec<String>,
    pub body: Body,
    pub options: Options,
    pub tls: TlsConfig,
}

/// Headers that always carry credentials.
const ALWAYS_CREDENTIALS: [&str; 3] = ["authorization", "proxy-authorization", "cookie"];

impl Request {
    /// Whether header `name` carries credentials.
    pub(crate) fn is_credential(&self, name: &str) -> bool {
        ALWAYS_CREDENTIALS
            .iter()
            .any(|always| name.eq_ignore_ascii_case(always))
            || self
                .credential_headers
                .iter()
                .any(|marked| name.eq_ignore_ascii_case(marked))
    }
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
    /// The longest wait for the response head, then between two pieces of
    /// the body. `None` waits as long as it takes.
    pub timeout: Option<Duration>,
    pub follow_redirects: bool,
    pub max_redirects: u32,
    pub verify_tls: bool,
    pub use_system_proxy: bool,
    pub max_body_bytes: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            timeout: Some(Duration::from_secs(30)),
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
    /// A file, read again and streamed on every send.
    File {
        path: PathBuf,
        content_type: Option<String>,
    },
    /// `application/x-www-form-urlencoded` fields.
    Form(Vec<(String, String)>),
    /// `multipart/form-data` parts, in order.
    Multipart(Vec<Part>),
}

#[derive(Clone)]
pub struct Part {
    pub name: String,
    pub value: PartValue,
}

#[derive(Clone)]
pub enum PartValue {
    Text(String),
    File {
        path: PathBuf,
        content_type: Option<String>,
    },
}

/// Leaves the value out.
impl fmt::Debug for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Part")
            .field("name", &self.name)
            .field("value", &self.value)
            .finish()
    }
}

/// Shows the size of a text, not its content.
impl fmt::Debug for PartValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PartValue::Text(text) => f.debug_struct("Text").field("len", &text.len()).finish(),
            PartValue::File { path, content_type } => f
                .debug_struct("File")
                .field("path", path)
                .field("content_type", content_type)
                .finish(),
        }
    }
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
            Body::File { path, content_type } => f
                .debug_struct("File")
                .field("path", path)
                .field("content_type", content_type)
                .finish(),
            Body::Form(fields) => f
                .debug_struct("Form")
                .field("fields", &fields.len())
                .finish(),
            Body::Multipart(parts) => f.debug_tuple("Multipart").field(parts).finish(),
        }
    }
}

/// A request checked once and turned into a wire request for each attempt.
pub(crate) struct Prepared {
    method: Method,
    /// `path?query`, for HTTP/1.1.
    origin_form: Uri,
    /// `scheme://authority/path?query`, for HTTP/2. The authority is the
    /// user's `Host` when they wrote one, as curl does, otherwise the URL's
    /// without user info. `None` when the user's `Host` can't be one.
    absolute_form: Option<Uri>,
    /// `scheme://authority/path?query` of the URL itself, for a proxy.
    url_form: Uri,
    /// `None` when the user wrote their own `Host`.
    host: Option<HeaderValue>,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Plan,
    /// Set when the request goes to an HTTP proxy in absolute form.
    plain_proxy: Option<PlainProxy>,
}

/// What a request to an HTTP target sends to its proxy.
struct PlainProxy {
    /// `scheme://host:port/path?query` of the target, without user info.
    target: Uri,
    authorization: Option<HeaderValue>,
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
        let absolute_form = |authority: &[u8]| {
            Uri::builder()
                .scheme(scheme)
                .authority(authority)
                .path_and_query(path_and_query.as_str())
                .build()
        };

        let url_form = absolute_form(authority.as_bytes()).map_err(|_| not_sendable())?;
        let mut user = Vec::with_capacity(request.headers.len());
        for (name, value) in &request.headers {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| HttpError::InvalidRequest(format!("invalid header name {name:?}")))?;
            let mut value = HeaderValue::from_bytes(value.as_bytes()).map_err(|_| {
                HttpError::InvalidRequest(format!(
                    "the value of header {name} has a line break or control character"
                ))
            })?;
            // Never indexed by HPACK, and never printed by hyper.
            value.set_sensitive(request.is_credential(name));
            user.push((header, value));
        }
        let user_wrote = |name: &HeaderName| user.iter().any(|(n, _)| n == name);
        let absolute_form = match user.iter().find(|(name, _)| name == HOST) {
            Some((_, host)) => absolute_form(host.as_bytes()).ok(),
            None => Some(absolute_form(authority.as_bytes()).map_err(|_| not_sendable())?),
        };

        let body = Plan::new(&request.body)?;
        let mut headers = Vec::with_capacity(user.len() + 3);
        if !user_wrote(&USER_AGENT) {
            headers.push((USER_AGENT, user_agent.clone()));
        }
        if !user_wrote(&ACCEPT) {
            headers.push((ACCEPT, HeaderValue::from_static("*/*")));
        }
        if let Some(content_type) = body.content_type()
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
            url_form,
            host,
            headers,
            body,
            plain_proxy: None,
        })
    }

    /// Makes the HTTP/1.1 request go to an HTTP proxy: absolute form, and
    /// `Proxy-Authorization` unless the user wrote one. The value is marked
    /// sensitive so hyper never prints it.
    pub(crate) fn through_http_proxy(mut self, authorization: Option<String>) -> Self {
        let authorization = authorization
            .and_then(|value| HeaderValue::from_str(&value).ok())
            .map(|mut value| {
                value.set_sensitive(true);
                value
            })
            .filter(|_| {
                !self
                    .headers
                    .iter()
                    .any(|(name, _)| name == PROXY_AUTHORIZATION)
            });
        self.plain_proxy = Some(PlainProxy {
            target: self.url_form.clone(),
            authorization,
        });
        self
    }

    /// The request for one attempt. HTTP/1.1 gets the origin form and a
    /// `Host` header; HTTP/2 gets the full URI, whose authority becomes
    /// `:authority`, and no `Host` header. The body's size is known, so hyper adds
    /// `Content-Length` unless the user wrote one.
    pub(crate) fn to_wire(
        &self,
        version: HttpVersion,
    ) -> Result<hyper::Request<WireBody>, HttpError> {
        let mut headers = HeaderMap::with_capacity(self.headers.len() + 1);
        let uri = match version {
            HttpVersion::Http11 => {
                if let Some(host) = &self.host {
                    headers.insert(HOST, host.clone());
                }
                match &self.plain_proxy {
                    Some(proxy) => proxy.target.clone(),
                    None => self.origin_form.clone(),
                }
            }
            HttpVersion::Http2 => self.absolute_form.clone().ok_or_else(|| {
                HttpError::InvalidRequest(
                    "the value of header Host is not a host and port, which HTTP/2 needs".into(),
                )
            })?,
        };
        for (name, value) in &self.headers {
            if version == HttpVersion::Http2 && name == HOST {
                continue;
            }
            headers.append(name.clone(), value.clone());
        }
        if let Some(value) = self
            .plain_proxy
            .as_ref()
            .and_then(|proxy| proxy.authorization.clone())
            && version == HttpVersion::Http11
        {
            headers.insert(PROXY_AUTHORIZATION, value);
        }
        let mut request = hyper::Request::new(self.body.open()?);
        *request.method_mut() = self.method.clone();
        *request.uri_mut() = uri;
        *request.headers_mut() = headers;
        Ok(request)
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
            credential_headers: Vec::new(),
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
    fn debug_of_form_and_multipart_leaves_values_out() {
        let form = Body::Form(vec![("token".into(), "form-secret".into())]);
        let multipart = Body::Multipart(vec![Part {
            name: "field".into(),
            value: PartValue::Text("part-secret".into()),
        }]);
        let shown = format!("{form:?} {multipart:?}");
        assert!(shown.contains("field"), "{shown}");
        for secret in ["form-secret", "part-secret"] {
            assert!(!shown.contains(secret), "{shown}");
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
            credential_headers: Vec::new(),
            body: Body::Empty,
            options: Options::default(),
            tls: TlsConfig::default(),
        };
        let prepared = Prepared::new(&request, &HeaderValue::from_static("Dea/test")).unwrap();
        let wire = prepared.to_wire(HttpVersion::Http11).unwrap();
        assert_eq!(wire.uri(), "/a?b=1");
        assert_eq!(wire.headers().get_all(HOST).iter().count(), 1);
        assert_eq!(wire.headers()[HOST], "other.example");
        assert_eq!(wire.headers().get_all(ACCEPT).iter().count(), 1);
        assert_eq!(wire.headers()[ACCEPT], "text/plain");
        assert_eq!(wire.headers()[USER_AGENT], "Dea/test");
        let wire = prepared.to_wire(HttpVersion::Http2).unwrap();
        assert_eq!(wire.uri(), "http://other.example/a?b=1");
        assert_eq!(wire.headers().get(HOST), None);
        assert_eq!(wire.headers()[ACCEPT], "text/plain");
    }

    #[test]
    fn http2_target_has_no_user_info() {
        let request = Request {
            method: "GET".into(),
            url: "https://user:pw@example.com:8443/".parse().unwrap(),
            headers: Vec::new(),
            credential_headers: Vec::new(),
            body: Body::Empty,
            options: Options::default(),
            tls: TlsConfig::default(),
        };
        let prepared = Prepared::new(&request, &HeaderValue::from_static("Dea/test")).unwrap();
        assert_eq!(
            prepared.to_wire(HttpVersion::Http2).unwrap().uri(),
            "https://example.com:8443/"
        );
        assert_eq!(
            prepared.to_wire(HttpVersion::Http11).unwrap().headers()[HOST],
            "example.com:8443"
        );
    }

    #[test]
    fn host_that_is_not_an_authority_fails_only_on_http2() {
        let request = Request {
            method: "GET".into(),
            url: "https://example.com/".parse().unwrap(),
            headers: vec![("Host".into(), "odd host/secret-path".into())],
            credential_headers: Vec::new(),
            body: Body::Empty,
            options: Options::default(),
            tls: TlsConfig::default(),
        };
        let prepared = Prepared::new(&request, &HeaderValue::from_static("Dea/test")).unwrap();
        let wire = prepared.to_wire(HttpVersion::Http11).unwrap();
        assert_eq!(wire.headers()[HOST], "odd host/secret-path");
        let Err(HttpError::InvalidRequest(message)) = prepared.to_wire(HttpVersion::Http2) else {
            panic!("expected InvalidRequest");
        };
        assert!(message.contains("Host"), "{message}");
        assert!(!message.contains("secret-path"), "{message}");
    }

    #[test]
    fn credential_values_are_sensitive() {
        let request = Request {
            method: "GET".into(),
            url: "https://example.com/".parse().unwrap(),
            headers: vec![
                ("Authorization".into(), "Bearer t".into()),
                ("Cookie".into(), "s=1".into()),
                ("Proxy-Authorization".into(), "Basic p".into()),
                ("X-Api-Key".into(), "k".into()),
                ("X-Keep".into(), "1".into()),
            ],
            credential_headers: vec!["x-api-key".into()],
            body: Body::Empty,
            options: Options::default(),
            tls: TlsConfig::default(),
        };
        let prepared = Prepared::new(&request, &HeaderValue::from_static("Dea/test")).unwrap();
        for version in [HttpVersion::Http11, HttpVersion::Http2] {
            let wire = prepared.to_wire(version).unwrap();
            let sensitive = |name: &str| wire.headers()[name].is_sensitive();
            for name in [
                "authorization",
                "cookie",
                "proxy-authorization",
                "x-api-key",
            ] {
                assert!(sensitive(name), "{name}");
            }
            assert!(!sensitive("x-keep"));
        }
    }
}
