//! The proxy settings of Windows (Internet Settings of the current user).

use windows_registry::CURRENT_USER;

use super::{NoProxy, ProxySettings, parse_proxy_server};

pub(super) fn read() -> ProxySettings {
    let Ok(key) = CURRENT_USER.open(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
    else {
        return ProxySettings::default();
    };
    if key.get_u32("ProxyEnable").unwrap_or(0) == 0 {
        return ProxySettings::default();
    }
    let (http, https) = key
        .get_string("ProxyServer")
        .map(|text| parse_proxy_server(&text))
        .unwrap_or_default();
    let no_proxy = key.get_string("ProxyOverride").unwrap_or_default();
    ProxySettings {
        http,
        https,
        no_proxy: NoProxy::parse(&no_proxy),
    }
}
