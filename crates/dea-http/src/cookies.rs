//! A browser-like cookie jar (decision 12). One jar belongs to a project
//! and an environment. Session cookies are kept too, like Postman does.
//! The public suffix list is not used: Dea only sends to hosts the user
//! picks.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use cookie_store::{CookieDomain, CookieExpiration, CookieStore, RawCookie};
use time::OffsetDateTime;
use url::Url;

use crate::request::Request;

/// Why a jar could not be loaded or a cookie could not be kept. The text
/// never holds a cookie value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CookieJarError(String);

/// One cookie as the cookie manager shows and edits it.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// `None` for a session cookie.
    pub expires: Option<OffsetDateTime>,
    pub secure: bool,
    pub http_only: bool,
    /// Sent to `domain` only, not to its subdomains.
    pub host_only: bool,
}

// The value is a secret.
impl fmt::Debug for StoredCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredCookie")
            .field("name", &self.name)
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("expires", &self.expires)
            .field("secure", &self.secure)
            .field("http_only", &self.http_only)
            .field("host_only", &self.host_only)
            .finish_non_exhaustive()
    }
}

/// Cookies kept between requests. Shared by reference; every method locks
/// only for its own short, synchronous work.
#[derive(Default)]
pub struct CookieJar {
    store: Mutex<CookieStore>,
    changed: AtomicBool,
}

impl fmt::Debug for CookieJar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieJar").finish_non_exhaustive()
    }
}

impl CookieJar {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads a jar written by [`CookieJar::to_json`], expired cookies left
    /// out.
    pub fn from_json(text: &str) -> Result<Self, CookieJarError> {
        // The parser's own message can quote the text.
        let store = cookie_store::serde::json::load(text.as_bytes())
            .map_err(|_| CookieJarError("the cookie jar file is not valid".into()))?;
        Ok(CookieJar {
            store: Mutex::new(store),
            changed: AtomicBool::new(false),
        })
    }

    /// Every cookie, session cookies included, for saving.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = Vec::new();
        let saved =
            cookie_store::serde::json::save_incl_expired_and_nonpersistent(&self.lock(), &mut out);
        match saved {
            Ok(()) => String::from_utf8(out).unwrap_or_default(),
            Err(_) => String::new(),
        }
    }

    /// The cookies that have not expired, in a steady order.
    #[must_use]
    pub fn list(&self) -> Vec<StoredCookie> {
        let mut cookies: Vec<StoredCookie> = self
            .lock()
            .iter_unexpired()
            .map(|cookie| StoredCookie {
                name: cookie.name().to_owned(),
                value: cookie.value().to_owned(),
                domain: cookie
                    .domain
                    .as_cow()
                    .map(std::borrow::Cow::into_owned)
                    .unwrap_or_default(),
                path: String::from(&cookie.path),
                expires: match cookie.expires {
                    // Cookies carry whole seconds; a saved jar loses the rest.
                    CookieExpiration::AtUtc(at) => at.replace_nanosecond(0).ok(),
                    CookieExpiration::SessionEnd => None,
                },
                secure: cookie.secure().unwrap_or(false),
                http_only: cookie.http_only().unwrap_or(false),
                host_only: matches!(cookie.domain, CookieDomain::HostOnly(_)),
            })
            .collect();
        cookies.sort_by(|a, b| (&a.domain, &a.path, &a.name).cmp(&(&b.domain, &b.path, &b.name)));
        cookies
    }

    /// Keeps `cookie`, replacing one with the same domain, path and name.
    /// A cookie that is already expired removes the one it would replace.
    pub fn insert(&self, cookie: StoredCookie) -> Result<(), CookieJarError> {
        let bad = |what: &str| CookieJarError(format!("the cookie has an invalid {what}"));
        let plain = |text: &str, extra: &[char]| {
            !text.chars().any(|c| c.is_control() || extra.contains(&c))
        };
        if cookie.name.is_empty() || !plain(&cookie.name, &[' ', ';', '=']) {
            return Err(bad("name"));
        }
        if !plain(&cookie.value, &[';']) {
            return Err(bad("value"));
        }
        if !cookie.path.starts_with('/') || !plain(&cookie.path, &[';']) {
            return Err(bad("path"));
        }
        let scheme = if cookie.secure { "https" } else { "http" };
        let origin: Url = format!("{scheme}://{}{}", cookie.domain, cookie.path)
            .parse()
            .map_err(|_| bad("domain"))?;
        let mut raw = RawCookie::build((cookie.name, cookie.value))
            .path(cookie.path)
            .secure(cookie.secure)
            .http_only(cookie.http_only);
        if !cookie.host_only {
            raw = raw.domain(cookie.domain);
        }
        if let Some(at) = cookie.expires {
            raw = raw.expires(at);
        }
        let mut store = self.lock();
        match store.insert_raw(&raw.build(), &origin) {
            Ok(_) | Err(cookie_store::CookieError::Expired) => {
                drop(store);
                self.changed.store(true, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => Err(CookieJarError(error.to_string())),
        }
    }

    /// Removes one cookie. `true` when there was one.
    pub fn remove(&self, domain: &str, path: &str, name: &str) -> bool {
        let removed = self.lock().remove(domain, path, name).is_some();
        if removed {
            self.changed.store(true, Ordering::Relaxed);
        }
        removed
    }

    pub fn clear(&self) {
        let mut store = self.lock();
        let had_any = store.iter_any().next().is_some();
        store.clear();
        drop(store);
        if had_any {
            self.changed.store(true, Ordering::Relaxed);
        }
    }

    /// Whether the jar changed since the last call, so the caller knows to
    /// save it.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }

    /// Keeps the cookies of `Set-Cookie` header values received from `url`.
    /// Values that are not cookies, or that `url` may not set, are ignored.
    pub fn store(&self, url: &Url, set_cookie: &[&str]) {
        let mut store = self.lock();
        let mut changed = false;
        for value in set_cookie {
            if let Ok(raw) = RawCookie::parse(*value) {
                changed |= store.insert_raw(&raw, url).is_ok();
            }
        }
        drop(store);
        if changed {
            self.changed.store(true, Ordering::Relaxed);
        }
    }

    /// The `Cookie` header value for a request to `url`, if any cookie
    /// matches.
    #[must_use]
    pub fn header_for(&self, url: &Url) -> Option<String> {
        let store = self.lock();
        let pairs: Vec<String> = store
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }

    /// `request` with the matching cookies added, after the user's own
    /// `Cookie` header when there is one.
    pub(crate) fn apply(&self, request: &Request) -> Request {
        let mut request = request.clone();
        let Some(jar) = self.header_for(&request.url) else {
            return request;
        };
        match request
            .headers
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case("cookie"))
        {
            Some((_, value)) if value.trim().is_empty() => *value = jar,
            Some((_, value)) => {
                let trimmed = value.trim_end().trim_end_matches(';').len();
                value.truncate(trimmed);
                value.push_str("; ");
                value.push_str(&jar);
            }
            None => request.headers.push(("Cookie".into(), jar)),
        }
        request
    }

    /// Keeps the cookies a response to `url` set.
    pub(crate) fn absorb(&self, url: &Url, headers: &[(String, String)]) {
        let values: Vec<&str> = headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
            .map(|(_, value)| value.as_str())
            .collect();
        if !values.is_empty() {
            self.store(url, &values);
        }
    }

    fn lock(&self) -> MutexGuard<'_, CookieStore> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
