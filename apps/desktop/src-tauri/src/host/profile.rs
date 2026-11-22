//! CEF browser profile contexts: normal persistent RequestContext, shared non-persistent
//! in-memory incognito RequestContext, page background, `prefers-color-scheme`, and clearing
//! browsing data.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use cef::*;

use super::{cdp_call, with_host};
use crate::app::runtime::post_ui;

/// Returns the shared non-persistent in-memory `RequestContext` for incognito tabs, creating it
/// on demand (no `cache_path` and `persist_session_cookies = 0`).
pub fn incognito_context() -> Option<RequestContext> {
    with_host(|h| {
        if let Some(ctx) = &h.incognito_ctx {
            return Some(ctx.clone());
        }
        let mut settings = RequestContextSettings::default();
        // Empty cache_path + persist_session_cookies=0 => strictly in-memory off-the-record context.
        settings.persist_session_cookies = 0;
        let mut handler = super::tab::create_request_context_handler();
        let ctx = request_context_create_context(Some(&settings), Some(&mut handler));
        if let Some(c) = &ctx {
            h.incognito_ctx = Some(c.clone());
        }
        ctx
    })
    .flatten()
}

/// Destroys the shared incognito `RequestContext` once no incognito tabs or windows remain.
pub fn maybe_drop_incognito_context() {
    with_host(|h| {
        let any_private_win = h.wins.keys().any(|l| super::is_private_window(l));
        let any_private_tab = h.tabs.values().any(|t| t.shared.private);
        if !any_private_win && !any_private_tab {
            h.incognito_ctx = None;
        }
    });
}

pub fn set_page_background(rgb: (u8, u8, u8)) {
    with_host(|h| {
        h.page_bg = rgb;
    });
}

pub fn set_content_color_scheme(scheme: &str) {
    let scheme = scheme.to_string();
    let browsers: Vec<Browser> = with_host(|h| {
        h.content_scheme = scheme.clone();
        h.tabs.values().filter_map(|t| t.browser.clone()).collect()
    })
    .unwrap_or_default();

    let params = match scheme.as_str() {
        "light" => r#"{"features":[{"name":"prefers-color-scheme","value":"light"}]}"#,
        "dark" => r#"{"features":[{"name":"prefers-color-scheme","value":"dark"}]}"#,
        _ => r#"{"features":[]}"#,
    };
    for b in browsers {
        cdp_call(&b, "Emulation.setEmulatedMedia", params, |_| {});
    }
}

thread_local! {
    static PENDING_CLEARS: RefCell<HashMap<u64, Box<dyn FnOnce(Result<(), String>)>>> = RefCell::new(HashMap::new());
}
static NEXT_CLEAR: AtomicU64 = AtomicU64::new(1);

wrap_delete_cookies_callback! {
    struct ClearCookiesCb {
        token: u64,
    }

    impl DeleteCookiesCallback {
        fn on_complete(&self, _num_deleted: ::std::os::raw::c_int) {
            let token = self.token;
            let _ = post_ui(move || {
                if let Some(done) = PENDING_CLEARS.with(|p| p.borrow_mut().remove(&token)) {
                    done(Ok(()));
                }
            });
        }
    }
}

/// Clears CEF browsing data (cookies, cache, site storage) for either the normal profile or the
/// shared in-memory incognito context.
pub fn clear_browsing_data(
    private: bool,
    kinds: &[String],
    _since_ms: Option<i64>,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let ctx = if private {
        with_host(|h| h.incognito_ctx.clone()).flatten()
    } else {
        request_context_get_global_context()
    };

    let Some(ctx) = ctx else {
        done(Ok(()));
        return;
    };

    let want_cookies = kinds.iter().any(|k| matches!(k.as_str(), "cookies" | "siteData"));
    let want_cache = kinds.iter().any(|k| matches!(k.as_str(), "cache" | "siteData"));
    let want_storage = kinds.iter().any(|k| matches!(k.as_str(), "storage" | "siteData"));

    if want_cache || want_storage {
        let target = with_host(|h| {
            h.tabs
                .values()
                .find(|t| t.shared.private == private)
                .and_then(|t| t.browser.clone())
                .or_else(|| (!private).then(|| h.wins.values().find_map(|w| w.chrome.clone())).flatten())
        })
        .flatten();
        if let Some(b) = target {
            if want_cache {
                cdp_call(&b, "Network.clearBrowserCache", "{}", |_| {});
            }
            if want_storage {
                cdp_call(
                    &b,
                    "Storage.clearDataForOrigin",
                    r#"{"origin":"*","storageTypes":"all"}"#,
                    |_| {},
                );
            }
        }
    }

    if want_cookies {
        if let Some(cm) = ctx.cookie_manager(None) {
            let token = NEXT_CLEAR.fetch_add(1, Ordering::Relaxed);
            PENDING_CLEARS.with(|p| p.borrow_mut().insert(token, Box::new(done)));
            let mut cb = ClearCookiesCb::new(token);
            if cm.delete_cookies(None, None, Some(&mut cb)) != 0 {
                return;
            }
            if let Some(done) = PENDING_CLEARS.with(|p| p.borrow_mut().remove(&token)) {
                done(Ok(()));
            }
            return;
        }
    }
    done(Ok(()));
}
