//! Password saving and autofill in pages (CEF UI thread).
//! Pages have no privileged IPC; login/card signals arrive through cancelled named popups and
//! filling runs via `eval_js` after verifying `location.origin`.

mod login;
mod payment;
mod scripts;

pub use scripts::PAGE_SCRIPT;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

use login::{on_capture, on_field, on_form, on_type};
use payment::{on_card_capture, on_card_field};
use scripts::{CARD_FILL_SCRIPT, FILL_SCRIPT};
use super::tab::TabShared;
use super::{config, emit_tab, eval_js, with_host};
use crate::vault::cards::{self, CardInfo, FillCard};
use crate::vault::{self, Account};

const MAX_MESSAGE: usize = 16 * 1024;

#[derive(Default)]
struct TabState {
    step_user: Option<(String, String, Instant)>,
    generated: Option<Zeroizing<String>>,
    form_handled: bool,
    suggesting: bool,
    muted: bool,
}

thread_local! {
    static TABS: RefCell<HashMap<String, TabState>> = RefCell::new(HashMap::new());
}

fn with_tab<R>(id: &str, f: impl FnOnce(&mut TabState) -> R) -> R {
    TABS.with(|t| f(t.borrow_mut().entry(id.to_string()).or_default()))
}

#[derive(Deserialize)]
struct Msg {
    #[serde(default)]
    k: String,
    #[serde(default)]
    f: String,
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
    #[serde(default)]
    w: f64,
    #[serde(default)]
    h: f64,
    #[serde(default)]
    dpr: f64,
    #[serde(default)]
    u: String,
    #[serde(default)]
    p: String,
    #[serde(default)]
    cn: String,
    #[serde(default)]
    cm: String,
    #[serde(default)]
    cy: String,
    #[serde(default)]
    cc: String,
    #[serde(default)]
    ch: String,
}

impl Drop for Msg {
    fn drop(&mut self) {
        self.p.zeroize();
        self.cn.zeroize();
        self.cc.zeroize();
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FieldRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Suggest {
    tab_id: String,
    site: String,
    field: String,
    new_password: bool,
    rect: FieldRect,
    dpr: f64,
    accounts: Vec<Account>,
    cards: Vec<CardInfo>,
    generated: Option<String>,
    refresh: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TabRef {
    tab_id: String,
}

fn percent_decode(s: &str) -> Option<Zeroizing<String>> {
    let bytes = s.as_bytes();
    let mut out = Zeroizing::new(Vec::with_capacity(bytes.len()));
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out.to_vec()).ok().map(Zeroizing::new)
}

fn parse_message(uri: &str) -> Option<Msg> {
    if uri.len() > MAX_MESSAGE {
        return None;
    }
    let (_, fragment) = uri.split_once('#')?;
    let json = percent_decode(fragment)?;
    serde_json::from_str(&json).ok()
}

pub fn page_origin(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.origin().ascii_serialization())
}

pub fn on_message(sh: &Arc<TabShared>, name: &str, uri: &str, user: bool, main_frame: bool) {
    let cfg = config();
    if !cfg.passwords_enabled || !main_frame {
        return;
    }
    let Some(origin) = page_origin(&sh.url()) else { return };
    let Some(msg) = parse_message(uri) else { return };
    match name {
        "_zenith_pw_field" if user => on_field(sh, &origin, &msg),
        "_zenith_pw_type" if user => on_type(sh, &origin, &msg),
        "_zenith_pw_dismiss" => close_suggestions(&sh.id),
        "_zenith_pw_capture" if user => on_capture(sh, &origin, &msg),
        "_zenith_pw_form" => on_form(sh, &origin),
        "_zenith_card_field" if user => on_card_field(sh, &origin, &msg),
        "_zenith_card_capture" if user => on_card_capture(sh, &origin, &msg),
        _ => {}
    }
}

pub fn close_suggestions(tab_id: &str) {
    let was = with_tab(tab_id, |t| {
        t.generated = None;
        std::mem::replace(&mut t.suggesting, false)
    });
    if was {
        emit_tab(tab_id, "password-suggest-close", TabRef { tab_id: tab_id.to_string() });
    }
}

pub fn mute_suggestions(tab_id: &str) {
    with_tab(tab_id, |t| t.muted = true);
}

pub fn on_navigation(tab_id: &str) {
    close_suggestions(tab_id);
    with_tab(tab_id, |t| t.form_handled = false);
}

pub fn forget(tab_id: &str) {
    TABS.with(|t| t.borrow_mut().remove(tab_id));
    vault::drop_tab_offers(tab_id);
    cards::drop_tab_card_offers(tab_id);
}

pub fn tab_origin(tab_id: &str) -> Option<String> {
    let url = with_host(|h| h.tabs.get(tab_id).map(|t| t.shared.url())).flatten()?;
    page_origin(&url)
}

pub fn fill(
    tab_id: &str,
    origin: &str,
    username: Option<&str>,
    password: Option<&str>,
    mode: &str,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let payload = Zeroizing::new(serde_json::json!({ "origin": origin, "u": username, "p": password, "mode": mode }).to_string());
    run_fill(tab_id, origin, FILL_SCRIPT, payload, "No login form was found on this page.", done);
}

pub fn fill_card(
    tab_id: &str,
    origin: &str,
    card: &FillCard,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let payload = Zeroizing::new(
        serde_json::json!({
            "origin": origin,
            "n": card.number.as_str(),
            "m": card.exp_month,
            "y": card.exp_year,
            "c": card.cvc.as_str(),
            "h": card.holder,
        })
        .to_string(),
    );
    run_fill(tab_id, origin, CARD_FILL_SCRIPT, payload, "No payment form was found on this page.", done);
}

fn run_fill(
    tab_id: &str,
    origin: &str,
    script: &str,
    payload: Zeroizing<String>,
    nofield: &'static str,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let browser = with_host(|h| h.tabs.get(tab_id).and_then(|t| t.browser.clone())).flatten();
    let Some(browser) = browser else {
        return done(Err("The tab is no longer open.".into()));
    };
    if tab_origin(tab_id).as_deref() != Some(origin) {
        return done(Err("The page changed before it could be filled.".into()));
    }
    let expr = Zeroizing::new(format!("{script}({})", payload.as_str()));
    eval_js(&browser, expr.as_str(), false, true, move |res| {
        let outcome = match res {
            Ok(v) => match v.as_str() {
                Some("ok") => Ok(()),
                Some("origin") => Err("The page changed before it could be filled.".to_string()),
                _ => Err(nofield.to_string()),
            },
            Err(e) => Err(e),
        };
        done(outcome);
    });
}

pub fn fill_generated(tab_id: &str, done: impl FnOnce(Result<(), String>) + 'static) {
    let Some(password) = with_tab(tab_id, |t| t.generated.take()) else {
        return done(Err("That suggestion is no longer available.".into()));
    };
    let Some(origin) = tab_origin(tab_id) else {
        return done(Err("This page can't be filled.".into()));
    };
    close_suggestions(tab_id);
    fill(tab_id, &origin, None, Some(&password), "new", done);
}

#[cfg(test)]
mod tests {
    use super::login::matching_accounts;
    use super::*;

    #[test]
    fn messages_decode() {
        let m = parse_message("https://zenith.invalid/#%7B%22u%22%3A%22ann%40x.com%22%2C%22p%22%3A%22p%25w%20%E2%9C%93%22%7D").unwrap();
        assert_eq!(m.u, "ann@x.com");
        assert_eq!(m.p, "p%w \u{2713}");
        assert!(parse_message("https://zenith.invalid/").is_none());
        assert!(parse_message("https://zenith.invalid/#%7").is_none());
        assert!(parse_message("https://zenith.invalid/#%ZZ").is_none());
        let long = format!("https://zenith.invalid/#{}", "a".repeat(MAX_MESSAGE));
        assert!(parse_message(&long).is_none());
    }

    #[test]
    fn typing_narrows_the_accounts() {
        let acc = |u: &str, l: &str| Account { id: u.into(), username: u.into(), label: l.into(), site: "x.com".into(), exact: true };
        let all = vec![acc("ann@x.com", ""), acc("Bob@y.org", "work"), acc("cy@x.com", "")];
        let names = |v: Vec<Account>| v.into_iter().map(|a| a.username).collect::<Vec<_>>();
        assert_eq!(names(matching_accounts(all.clone(), "")).len(), 3);
        assert_eq!(names(matching_accounts(all.clone(), "  ")).len(), 3);
        assert_eq!(names(matching_accounts(all.clone(), "AN")), ["ann@x.com"]);
        assert_eq!(names(matching_accounts(all.clone(), "x.com")), ["ann@x.com", "cy@x.com"]);
        assert_eq!(names(matching_accounts(all, "zzz")).len(), 0);
    }

    #[test]
    fn page_origins() {
        assert_eq!(page_origin("https://www.facebook.com/login?x=1").as_deref(), Some("https://www.facebook.com"));
        assert_eq!(page_origin("http://localhost:3000/").as_deref(), Some("http://localhost:3000"));
        assert_eq!(page_origin("file:///C:/x.html"), None);
        assert_eq!(page_origin("zenith://settings"), None);
    }

    #[test]
    fn scripts_are_self_contained() {
        assert!(PAGE_SCRIPT.starts_with("(() => {") && PAGE_SCRIPT.ends_with("})();"));
        assert!(FILL_SCRIPT.starts_with("((d) => {") && FILL_SCRIPT.ends_with("})"));
        assert!(PAGE_SCRIPT.contains("const userFor") && FILL_SCRIPT.contains("const userFor"));
        assert!(CARD_FILL_SCRIPT.starts_with("((d) => {") && CARD_FILL_SCRIPT.ends_with("})"));
        assert!(PAGE_SCRIPT.contains("const cardFields") && CARD_FILL_SCRIPT.contains("const cardFields"));
    }
}
