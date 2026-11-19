//! Which proxy a request goes through (decision 13): the `*_PROXY`
//! variables when any is set, otherwise the settings of the operating
//! system on macOS and Windows. PAC and WPAD are not supported. Loopback
//! never uses a proxy.
//!
//! The URL of a proxy may carry credentials. They are never shown: not by
//! `Debug`, not in an error.

use std::fmt;
use std::net::IpAddr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::percent_decode_str;
use sha2::{Digest as _, Sha256};
use url::{Host, Url};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// How requests reach the network.
// One value per client, so the size of the largest variant costs nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProxyConfig {
    /// The environment variables, else the settings of the operating
    /// system, read again for every request.
    #[default]
    System,
    /// Straight to the server.
    None,
    /// These settings, whatever the machine says.
    Fixed(ProxySettings),
}

/// Proxies by scheme of the target, and the hosts that skip them.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ProxySettings {
    /// For `http://` targets.
    pub http: Option<Url>,
    /// For `https://` targets, reached with `CONNECT`.
    pub https: Option<Url>,
    pub no_proxy: NoProxy,
}

/// Leaves the credentials of the proxy URLs out.
impl fmt::Debug for ProxySettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxySettings")
            .field("http", &self.http.as_ref().map(shown))
            .field("https", &self.https.as_ref().map(shown))
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
}

impl ProxySettings {
    /// The proxy for `target`, or `None` when it goes direct.
    pub(crate) fn for_target(&self, target: &Url) -> Option<&Url> {
        let host = target.host_str()?;
        if self
            .no_proxy
            .matches(host, target.port_or_known_default().unwrap_or_default())
        {
            return None;
        }
        match target.scheme() {
            "http" => self.http.as_ref(),
            "https" => self.https.as_ref(),
            _ => None,
        }
    }
}

/// Whether `target` is on this machine, which never uses a proxy, so the
/// settings need not be read at all.
pub(crate) fn is_loopback(target: &Url) -> bool {
    target
        .host_str()
        .is_some_and(|host| NoProxy::default().matches(host, 0))
}

/// The hosts that skip the proxy, from `NO_PROXY` and the like.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoProxy {
    everything: bool,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    port: Option<u16>,
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// The domain and its subdomains, lower case, without a leading dot.
    Domain(String),
    /// Names without a dot (`<local>` of Windows).
    Local,
    Network {
        address: IpAddr,
        prefix: u8,
    },
}

impl NoProxy {
    /// Reads a list separated by commas, semicolons or spaces. Entries
    /// that are not understood are skipped.
    #[must_use]
    pub fn parse(text: &str) -> NoProxy {
        let mut parsed = NoProxy::default();
        for entry in text.split([',', ';', ' ', '\t']) {
            let entry = entry.trim();
            if entry == "*" {
                parsed.everything = true;
            } else if let Some(rule) = parse_rule(entry) {
                parsed.rules.push(rule);
            }
        }
        parsed
    }

    /// Whether `host` and `port` skip the proxy. Loopback always does.
    #[must_use]
    pub fn matches(&self, host: &str, port: u16) -> bool {
        let host = host.trim_matches(['[', ']']).trim_end_matches('.');
        let host = host.to_ascii_lowercase();
        let address = host.parse::<IpAddr>().ok();
        if self.everything
            || host == "localhost"
            || host.ends_with(".localhost")
            || address.is_some_and(|a| a.is_loopback())
        {
            return true;
        }
        self.rules.iter().any(|rule| {
            rule.port.is_none_or(|p| p == port)
                && match &rule.kind {
                    Kind::Domain(domain) => {
                        address.is_none()
                            && (host == *domain
                                || host
                                    .strip_suffix(domain.as_str())
                                    .is_some_and(|rest| rest.ends_with('.')))
                    }
                    Kind::Local => address.is_none() && !host.contains('.'),
                    Kind::Network {
                        address: network,
                        prefix,
                    } => address.is_some_and(|a| in_network(a, *network, *prefix)),
                }
        })
    }
}

fn parse_rule(entry: &str) -> Option<Rule> {
    if entry.is_empty() {
        return None;
    }
    if entry.eq_ignore_ascii_case("<local>") {
        return Some(Rule {
            port: None,
            kind: Kind::Local,
        });
    }
    // `[fd00::]/8` and `[::1]:80` keep their address in brackets.
    let (text, port) = match entry.rsplit_once(':') {
        Some((head, tail)) if !head.contains(':') || head.ends_with(']') => {
            match tail.parse::<u16>() {
                Ok(port) => (head, Some(port)),
                Err(_) => (entry, None),
            }
        }
        _ => (entry, None),
    };
    let text = text.trim_start_matches("*.").trim_start_matches('.');
    let (address, prefix) = match text.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix.parse::<u8>().ok()?)),
        None => (text, None),
    };
    let address = address.trim_matches(['[', ']']);
    let kind = if let Some(ip) = parse_ip(address, prefix.is_some()) {
        let width = if ip.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(width);
        if prefix > width {
            return None;
        }
        Kind::Network {
            address: ip,
            prefix,
        }
    } else if prefix.is_none() && !address.is_empty() && !address.contains('*') {
        Kind::Domain(address.trim_end_matches('.').to_ascii_lowercase())
    } else {
        return None;
    };
    Some(Rule { port, kind })
}

/// An IP address; for a network, `169.254/16` stands for `169.254.0.0/16`.
fn parse_ip(text: &str, network: bool) -> Option<IpAddr> {
    if let Ok(ip) = text.parse::<IpAddr>() {
        return Some(ip);
    }
    let parts: Vec<&str> = text.split('.').collect();
    if network
        && parts.len() < 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        let padded = format!("{text}{}", ".0".repeat(4 - parts.len()));
        return padded.parse::<IpAddr>().ok();
    }
    None
}

fn in_network(address: IpAddr, network: IpAddr, prefix: u8) -> bool {
    let (a, n, width) = match (address, network) {
        (IpAddr::V4(a), IpAddr::V4(n)) => (u128::from(u32::from(a)), u128::from(u32::from(n)), 32),
        (IpAddr::V6(a), IpAddr::V6(n)) => (u128::from(a), u128::from(n), 128),
        _ => return false,
    };
    let host_bits = u32::from(width - prefix);
    host_bits >= 128 || a >> host_bits == n >> host_bits
}

/// The proxies the variables name. The upper-case name is read before the
/// lower-case one, and `ALL_PROXY` fills the scheme with none of its own.
/// `None` when no variable names a proxy.
pub fn proxy_from_env(get: impl Fn(&str) -> Option<String>) -> Option<ProxySettings> {
    let read = |upper: &str| {
        get(upper)
            .or_else(|| get(&upper.to_ascii_lowercase()))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let proxy = |name: &str| {
        read(name)
            .or_else(|| read("ALL_PROXY"))
            .and_then(|value| parse_proxy_url(&value))
    };
    let http = proxy("HTTP_PROXY");
    let https = proxy("HTTPS_PROXY");
    if http.is_none() && https.is_none() {
        return None;
    }
    Some(ProxySettings {
        http,
        https,
        no_proxy: read("NO_PROXY").map_or_else(NoProxy::default, |text| NoProxy::parse(&text)),
    })
}

/// A proxy URL; `host:port` without a scheme means `http://host:port`.
fn parse_proxy_url(text: &str) -> Option<Url> {
    let url = if text.contains("://") {
        Url::parse(text)
    } else {
        Url::parse(&format!("http://{text}"))
    };
    url.ok().filter(|url| url.host().is_some())
}

/// The proxy settings now: the variables when any names a proxy,
/// otherwise the operating system's. Cheap enough to read for every
/// request, so a change of network applies to the next one.
#[must_use]
pub fn system_proxy() -> ProxySettings {
    proxy_from_env(|name| std::env::var(name).ok()).unwrap_or_else(os_proxy)
}

#[cfg(target_os = "macos")]
fn os_proxy() -> ProxySettings {
    macos::read()
}

#[cfg(windows)]
fn os_proxy() -> ProxySettings {
    windows::read()
}

/// Linux has the variables only.
#[cfg(not(any(target_os = "macos", windows)))]
fn os_proxy() -> ProxySettings {
    ProxySettings::default()
}

/// `ProxyServer` of the Windows registry: `host:port` for every scheme, or
/// `http=host:port;https=host:port`.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_proxy_server(text: &str) -> (Option<Url>, Option<Url>) {
    if !text.contains('=') {
        let url = parse_proxy_url(text.trim());
        return (url.clone(), url);
    }
    let (mut http, mut https) = (None, None);
    for entry in text.split(';') {
        match entry.split_once('=') {
            Some((scheme, value)) if scheme.trim().eq_ignore_ascii_case("http") => {
                http = parse_proxy_url(value.trim());
            }
            Some((scheme, value)) if scheme.trim().eq_ignore_ascii_case("https") => {
                https = parse_proxy_url(value.trim());
            }
            _ => {}
        }
    }
    (http, https)
}

/// The proxy shown without its credentials.
pub(crate) fn shown(proxy: &Url) -> String {
    let host = match proxy.host() {
        Some(Host::Ipv6(ip)) => format!("[{ip}]"),
        Some(host) => host.to_string(),
        None => String::new(),
    };
    match proxy.port() {
        Some(port) => format!("{}://{host}:{port}", proxy.scheme()),
        None => format!("{}://{host}", proxy.scheme()),
    }
}

/// The `Proxy-Authorization` value for the credentials in `proxy`.
pub(crate) fn authorization(proxy: &Url) -> Option<String> {
    if proxy.username().is_empty() && proxy.password().is_none() {
        return None;
    }
    let decode = |text: &str| percent_decode_str(text).collect::<Vec<u8>>();
    let mut credentials = decode(proxy.username());
    credentials.push(b':');
    credentials.extend(decode(proxy.password().unwrap_or_default()));
    Some(format!("Basic {}", STANDARD.encode(credentials)))
}

/// Which connections may be shared: the proxy and its credentials, the
/// latter only as a digest so a debug print of the pool holds no secret.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProxyKey {
    address: String,
    credentials: Option<[u8; 32]>,
}

impl ProxyKey {
    pub(crate) fn new(proxy: &Url) -> Self {
        ProxyKey {
            address: shown(proxy),
            credentials: authorization(proxy).map(|value| Sha256::digest(value).into()),
        }
    }
}

impl fmt::Debug for ProxyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyKey")
            .field("address", &self.address)
            .field("has_credentials", &self.credentials.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_proxy_server_forms() {
        let (http, https) = parse_proxy_server("proxy.corp:8080");
        assert_eq!(http.unwrap().host_str(), Some("proxy.corp"));
        assert_eq!(https.unwrap().port(), Some(8080));
        let (http, https) = parse_proxy_server("http=a:1;https=b:2;ftp=c:3");
        assert_eq!(http.unwrap().host_str(), Some("a"));
        assert_eq!(https.unwrap().host_str(), Some("b"));
        let (http, https) = parse_proxy_server("https=b:2");
        assert!(http.is_none() && https.is_some());
    }

    #[test]
    fn windows_and_mac_exception_forms() {
        let rules = NoProxy::parse("*.local;169.254/16;<local>;*.example.com");
        assert!(rules.matches("printer.local", 80));
        assert!(rules.matches("169.254.9.9", 80));
        assert!(rules.matches("intranet", 80));
        assert!(rules.matches("a.example.com", 80));
        assert!(!rules.matches("example.org", 80));
    }

    #[test]
    fn credentials_are_decoded_and_never_shown() {
        let url = Url::parse("http://us%40er:p%3Aw@proxy.local:3128").unwrap();
        assert_eq!(
            authorization(&url).unwrap(),
            format!("Basic {}", STANDARD.encode("us@er:p:w"))
        );
        assert_eq!(shown(&url), "http://proxy.local:3128");
        assert!(!format!("{:?}", ProxyKey::new(&url)).contains("p%3Aw"));
        assert!(authorization(&Url::parse("http://proxy.local").unwrap()).is_none());
    }
}
