//! The proxy settings of macOS (System Settings, Network, Proxies).

// The one unsafe call turns an array element into a typed object.
#![allow(unsafe_code)]

use system_configuration::core_foundation::array::CFArray;
use system_configuration::core_foundation::base::{CFType, TCFType};
use system_configuration::core_foundation::dictionary::CFDictionary;
use system_configuration::core_foundation::number::CFNumber;
use system_configuration::core_foundation::string::CFString;
use system_configuration::dynamic_store::SCDynamicStoreBuilder;
use url::Url;

use super::{NoProxy, ProxySettings};

pub(super) fn read() -> ProxySettings {
    let Some(proxies) = SCDynamicStoreBuilder::new("dea")
        .build()
        .and_then(|store| store.get_proxies())
    else {
        return ProxySettings::default();
    };
    let no_proxy = proxies
        .find(CFString::from_static_string("ExceptionsList"))
        .and_then(|value| value.downcast::<CFArray>())
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    // SAFETY: every element of a property list array is a
                    // Core Foundation object. `wrap_under_get_rule` retains
                    // it for as long as `item` lives, and `downcast` checks
                    // that it is a string before it is used as one.
                    let item = unsafe { CFType::wrap_under_get_rule(*item) };
                    item.downcast::<CFString>().map(|s| s.to_string())
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    ProxySettings {
        http: scheme(&proxies, "HTTP"),
        https: scheme(&proxies, "HTTPS"),
        no_proxy: NoProxy::parse(&no_proxy),
    }
}

/// The proxy for `prefix` (`HTTP` or `HTTPS`) when its `…Enable` is on.
fn scheme(proxies: &CFDictionary<CFString, CFType>, prefix: &str) -> Option<Url> {
    let get = |suffix: &str| proxies.find(CFString::new(&format!("{prefix}{suffix}")));
    let enabled = get("Enable")?.downcast::<CFNumber>()?.to_i32()? != 0;
    if !enabled {
        return None;
    }
    let host = get("Proxy")?.downcast::<CFString>()?.to_string();
    let port = get("Port")
        .and_then(|v| v.downcast::<CFNumber>())
        .and_then(|n| n.to_i32());
    let text = match port {
        Some(port) => format!("http://{host}:{port}"),
        None => format!("http://{host}"),
    };
    Url::parse(&text).ok()
}
