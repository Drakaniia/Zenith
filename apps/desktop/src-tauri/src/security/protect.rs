//! Protect warnings: an open (unencrypted) Wi-Fi network, and a password or card field on a
//! plain-HTTP page. Both only inform; nothing is blocked.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use url::{Host, Url};

/// Tab id to the page it was last warned on (one warning per page).
static WARNED_PAGES: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);
/// Open networks already warned about this session.
static WARNED_WIFI: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// A focused field of this kind (`pass`, `user`, `card`) on this page warrants a warning:
/// a password or card field on plain HTTP that isn't served by this computer.
pub fn insecure_form(url: &str, field: &str) -> bool {
    let Ok(u) = Url::parse(url) else { return false };
    if u.scheme() != "http" || !matches!(field, "pass" | "card") {
        return false;
    }
    match u.host() {
        Some(Host::Domain(d)) => {
            let d = d.trim_end_matches('.');
            d != "localhost" && !d.ends_with(".localhost")
        }
        Some(Host::Ipv4(ip)) => !ip.is_loopback(),
        Some(Host::Ipv6(ip)) => !ip.is_loopback(),
        None => false,
    }
}

/// A password or card field was focused in a tab: warn its window once per page.
pub fn on_form_field(tab_id: &str, url: &str, field: &str) {
    if !insecure_form(url, field) {
        return;
    }
    let first = WARNED_PAGES
        .lock()
        .map(|mut m| m.insert(tab_id.to_string(), url.to_string()).as_deref() != Some(url))
        .unwrap_or(false);
    if first {
        crate::host::emit_tab(tab_id, "insecure-form", serde_json::json!({ "tabId": tab_id }));
    }
}

pub fn drop_tab(tab_id: &str) {
    if let Ok(mut m) = WARNED_PAGES.lock() {
        m.remove(tab_id);
    }
}

/// The open Wi-Fi network this computer is on, the first time it is seen this session.
pub fn open_wifi_once() -> Option<String> {
    let ssid = open_wifi()?;
    WARNED_WIFI.lock().ok()?.insert(ssid.clone()).then_some(ssid)
}

/// The SSID of a connected Wi-Fi network without encryption, if any. `None` without a Wi-Fi
/// adapter or the WLAN service.
#[cfg(windows)]
fn open_wifi() -> Option<String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::*;

    unsafe {
        let (mut ver, mut h) = (0u32, HANDLE::default());
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return None;
        }
        let mut out = None;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let l = &*list;
            let ifaces = std::slice::from_raw_parts(l.InterfaceInfo.as_ptr(), l.dwNumberOfItems as usize);
            for i in ifaces.iter().filter(|i| i.isState == wlan_interface_state_connected) {
                let (mut size, mut data) = (0u32, std::ptr::null_mut());
                if WlanQueryInterface(h, &i.InterfaceGuid, wlan_intf_opcode_current_connection, None, &mut size, &mut data, None) != 0 || data.is_null() {
                    continue;
                }
                let c = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                if !c.wlanSecurityAttributes.bSecurityEnabled.as_bool() {
                    let s = &c.wlanAssociationAttributes.dot11Ssid;
                    out = Some(String::from_utf8_lossy(&s.ucSSID[..(s.uSSIDLength as usize).min(32)]).into_owned());
                }
                WlanFreeMemory(data);
                if out.is_some() {
                    break;
                }
            }
            WlanFreeMemory(list as *const _);
        }
        WlanCloseHandle(h, None);
        out
    }
}

#[cfg(not(windows))]
fn open_wifi() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::insecure_form;

    #[test]
    fn warns_only_for_secrets_on_remote_http() {
        assert!(insecure_form("http://shop.example/login", "pass"));
        assert!(insecure_form("http://10.0.0.5/pay", "card"));
        assert!(!insecure_form("http://shop.example/login", "user"));
        assert!(!insecure_form("https://shop.example/login", "pass"));
        assert!(!insecure_form("http://localhost:3000/", "pass"));
        assert!(!insecure_form("http://app.localhost/", "card"));
        assert!(!insecure_form("http://127.0.0.1:8080/", "pass"));
        assert!(!insecure_form("http://[::1]/", "pass"));
        assert!(!insecure_form("file:///C:/x.html", "pass"));
        assert!(!insecure_form("not a url", "pass"));
    }
}
