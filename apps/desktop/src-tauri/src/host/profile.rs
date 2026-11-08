//! The pages' WebView2 profiles: page background, `prefers-color-scheme` and clearing browsing
//! data.

use std::result::Result;

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::Interface;

use super::{with_host, Host};

impl Host {
    /// The WebView2 profile pages use: the user's profile (which every chrome uses too), or for
    /// incognito windows the InPrivate profile their tabs share (None while no such tab exists).
    pub fn page_profile(&self, private: bool) -> Option<ICoreWebView2Profile> {
        let core = if private {
            self.tabs.values().find(|t| t.shared.private)?.core()
        } else {
            self.wins.values().next()?.chrome_core.clone()
        };
        unsafe { core.cast::<ICoreWebView2_13>().and_then(|w| w.Profile()).ok() }
    }
}

// ---------------------------------------------------------------------------------------------
// Appearance

pub fn set_page_background(rgb: (u8, u8, u8)) {
    with_host(|h| {
        h.page_bg = rgb;
        for t in h.tabs.values().filter(|t| !t.shared.light_bg) {
            let _ = t.view.set_background_color((rgb.0, rgb.1, rgb.2, 255));
        }
    });
}

/// Sets `prefers-color-scheme` for web content ("system", "light" or "dark").
pub fn set_content_color_scheme(scheme: &str) {
    let value = match scheme {
        "light" => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_LIGHT,
        "dark" => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_DARK,
        _ => COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO,
    };
    with_host(|h| {
        h.content_scheme = value;
        // InPrivate tabs (incognito windows) live in their own profile.
        for private in [false, true] {
            if let Some(profile) = h.page_profile(private) {
                unsafe {
                    let _ = profile.SetPreferredColorScheme(value);
                }
            }
        }
    });
}

/// Clears WebView2 browsing data of the user's profile, or of the InPrivate profile when asked
/// from an incognito window (`private`).
pub fn clear_browsing_data(
    private: bool,
    kinds: &[String],
    since_ms: Option<i64>,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let mut mask = COREWEBVIEW2_BROWSING_DATA_KINDS(0);
    for k in kinds {
        mask.0 |= match k.as_str() {
            "cookies" => COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0,
            "cache" => COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0 | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0,
            "storage" => COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE.0,
            "siteData" => COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE.0,
            "downloads" => COREWEBVIEW2_BROWSING_DATA_KINDS_DOWNLOAD_HISTORY.0,
            "autofill" => COREWEBVIEW2_BROWSING_DATA_KINDS_GENERAL_AUTOFILL.0
                | COREWEBVIEW2_BROWSING_DATA_KINDS_PASSWORD_AUTOSAVE.0,
            "permissions" => COREWEBVIEW2_BROWSING_DATA_KINDS_SETTINGS.0,
            _ => 0,
        };
    }
    let profile = with_host(|h| h.page_profile(private))
        .flatten()
        .and_then(|p| p.cast::<ICoreWebView2Profile2>().ok());
    let Some(profile) = profile else {
        // Incognito windows without open pages have nothing stored to clear.
        if private {
            done(Ok(()));
        } else {
            done(Err("profile unavailable".into()));
        }
        return;
    };
    let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
        done(result.map_err(|e| e.to_string()));
        Ok(())
    }));
    unsafe {
        let r = match since_ms {
            Some(since) => profile.ClearBrowsingDataInTimeRange(
                mask,
                since as f64 / 1000.0,
                crate::storage::now_ms() as f64 / 1000.0 + 60.0,
                &handler,
            ),
            None => profile.ClearBrowsingData(mask, &handler),
        };
        if let Err(e) = r {
            eprintln!("[zenith] clear browsing data failed: {e}");
        }
    }
}
