//! Password saving and autofill in pages (UI thread).
//!
//! Pages have no IPC. The injected script (main frame only) reports three things through named
//! `window.open` calls that NewWindowRequested recognises and cancels, like Zenith's other page
//! scripts: a login field was clicked (with its position, so the chrome UI can show saved
//! accounts under it), a login was submitted (username and password, offered for saving), and a
//! login form appeared (for optional fill-on-load). Clicks and submissions only count when
//! WebView2 reports a user gesture and the message comes from the top-level document.
//!
//! The page never receives a credential it did not get by the user choosing it in Zenith's own
//! UI (or, with fill-on-load, the single account saved for exactly this origin while unlocked).
//! Filling runs `ExecuteScript` in the top-level document after checking the current address
//! against the credential's site; the script checks `location.origin` again before touching
//! the form, so a navigation in between fills nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::result::Result;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::Url;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING};
use zeroize::{Zeroize, Zeroizing};

use super::tab::TabShared;
use super::{config, emit_tab, post, with_host};
use crate::vault::{self, crypto, Account, Capture};

/// Field helpers shared by the page script and the fill script.
macro_rules! helpers {
    () => {
        r#"
  const USER = /user|e-?mail|login|account|ident|phone|member|nick|handle|uname/i;
  const SKIP = /search|query|captcha|otp|one-time|code|coupon|promo|zip|postal|city|address|first.?name|last.?name|card|cvc/i;
  const textual = (el) => el instanceof HTMLInputElement && ['text', 'email', 'tel', ''].includes(el.type);
  const hint = (el) => [el.name, el.id, el.autocomplete, el.placeholder, el.getAttribute('aria-label')].join(' ');
  const shown = (el) => {
    if (!el.isConnected || el.disabled) return false;
    const r = el.getBoundingClientRect();
    return r.width > 1 && r.height > 1 && getComputedStyle(el).visibility !== 'hidden';
  };
  const isPass = (el) => el instanceof HTMLInputElement && el.type === 'password';
  const isUser = (el) => textual(el) && (/\b(username|email)\b/.test(el.autocomplete) || el.type === 'email' || (USER.test(hint(el)) && !SKIP.test(hint(el))));
  const scopeOf = (el) => el.form || (el.closest && el.closest('form')) || document;
  const passes = (root) => [...root.querySelectorAll('input[type=password]')].filter(shown);
  const userFor = (root, pw) => {
    const inputs = [...root.querySelectorAll('input')].filter(textual);
    let best = null;
    for (const i of inputs) {
      if (pw && (pw.compareDocumentPosition(i) & Node.DOCUMENT_POSITION_FOLLOWING)) break;
      if (shown(i) && !i.readOnly && !SKIP.test(hint(i))) best = i;
    }
    if (best && (pw || isUser(best))) return best;
    return inputs.find((i) => /\busername\b/.test(i.autocomplete)) || inputs.find((i) => shown(i) && isUser(i)) || null;
  };
"#
    };
}

/// Injected into the top-level document of every page.
pub const PAGE_SCRIPT: &str = concat!(
    "(() => {\n  if (window.top !== window) return;\n  const open = window.open.bind(window);\n",
    "  const send = (name, data) => { try { open('https://zenith.invalid/#' + encodeURIComponent(JSON.stringify(data)), name); } catch (e) {} };\n",
    helpers!(),
    r#"
  const kind = (el, root) => {
    const ps = passes(root);
    if (isPass(el)) {
      if (/new-password/.test(el.autocomplete)) return 'new';
      if (/current-password/.test(el.autocomplete)) return 'login';
      return ps.length >= 2 ? 'new' : 'login';
    }
    return ps.some((p) => /new-password/.test(p.autocomplete)) || ps.length >= 2 ? 'new' : 'login';
  };
  const loginField = (el) => {
    if (!(el instanceof HTMLInputElement) || el.readOnly || !shown(el)) return false;
    if (isPass(el)) return true;
    if (!isUser(el)) return false;
    const root = scopeOf(el);
    return passes(root).length > 0 || root !== document || /\busername\b/.test(el.autocomplete);
  };
  let lastEl = null, lastAt = 0, suggesting = false;
  const offer = (el) => {
    const now = Date.now();
    if (el === lastEl && now - lastAt < 500) return;
    lastEl = el; lastAt = now; suggesting = true;
    const r = el.getBoundingClientRect();
    send('_zenith_pw_field', { k: kind(el, scopeOf(el)), f: isPass(el) ? 'pass' : 'user', x: r.left, y: r.top, w: r.width, h: r.height, dpr: window.devicePixelRatio || 1 });
  };
  const target = (e) => e.composedPath()[0];
  addEventListener('focusin', (e) => { if (loginField(target(e))) offer(target(e)); }, true);
  addEventListener('click', (e) => { if (e.isTrusted && loginField(target(e))) offer(target(e)); }, true);
  const dismiss = () => { if (suggesting) { suggesting = false; send('_zenith_pw_dismiss', {}); } };
  addEventListener('input', (e) => { if (e.isTrusted && loginField(target(e))) dismiss(); }, true);

  let lastCap = '';
  const capture = (root) => {
    const ps = passes(root).filter((p) => p.value);
    let pw = null;
    if (ps.length === 1) pw = ps[0];
    else if (ps.length === 2) pw = ps[0].value === ps[1].value ? ps[0] : ps[1];
    else if (ps.length >= 3) pw = ps.find((p) => /new-password/.test(p.autocomplete)) || ps[1];
    const u = userFor(root, pw || null);
    const user = u && u.value ? u.value.trim() : '';
    const pass = pw ? pw.value : '';
    if (!pass && !(u && isUser(u) && user)) return;
    const key = user + '\n' + pass;
    if (key === lastCap) return;
    lastCap = key;
    setTimeout(() => { if (lastCap === key) lastCap = ''; }, 4000);
    send('_zenith_pw_capture', { u: user, p: pass });
  };
  addEventListener('submit', (e) => { if (e.target instanceof HTMLFormElement) capture(e.target); }, true);
  addEventListener('click', (e) => {
    if (!e.isTrusted) return;
    const b = e.composedPath().find((n) => n instanceof Element && n.matches('button, input[type=submit], input[type=button], [role=button]'));
    if (!b) return;
    const root = scopeOf(b);
    if (root !== document || passes(root).some((p) => p.value)) capture(root);
  }, true);
  addEventListener('keydown', (e) => {
    if (e.key === 'Escape') return dismiss();
    if (e.key !== 'Enter' || !e.isTrusted) return;
    const el = target(e);
    if (el instanceof HTMLInputElement && (isPass(el) || isUser(el))) capture(scopeOf(el));
  }, true);

  let told = false;
  const check = () => { if (!told && passes(document).length) { told = true; send('_zenith_pw_form', {}); } };
  addEventListener('DOMContentLoaded', check);
  addEventListener('load', check);
  setTimeout(check, 1500);
  setTimeout(check, 4000);
})();"#
);

/// Called as `(FILL_SCRIPT)(payload)` in the top-level document. Returns "ok", "origin" (the
/// page moved to another origin) or "nofield".
const FILL_SCRIPT: &str = concat!(
    "((d) => {\n  if (location.origin !== d.origin) return 'origin';\n",
    helpers!(),
    r#"
  const desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
  const set = (el, v) => {
    el.focus();
    desc.set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const a = document.activeElement;
  let root = a instanceof HTMLInputElement ? scopeOf(a) : null;
  if (!root || (!passes(root).length && !userFor(root, null))) {
    const p = passes(document)[0];
    root = p ? scopeOf(p) : document;
  }
  const ps = passes(root);
  if (d.mode === 'new') {
    let t = ps.filter((p) => !/current-password/.test(p.autocomplete));
    if (t.length >= 3 && !ps.some((p) => /current-password/.test(p.autocomplete))) t = t.slice(1);
    if (!t.length) return 'nofield';
    t.forEach((p) => set(p, d.p));
    return 'ok';
  }
  const pw = ps[0] || null;
  const u = userFor(root, pw);
  let did = false;
  if (d.u && u && shown(u)) { set(u, d.u); did = true; }
  if (d.p && pw) { set(pw, d.p); did = true; }
  return did ? 'ok' : 'nofield';
})"#
);

const STEP_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_MESSAGE: usize = 16 * 1024;

/// Per-tab state of the page in it.
#[derive(Default)]
struct TabState {
    /// Username submitted on the first step of a two-step login (origin, username, when).
    step_user: Option<(String, String, Instant)>,
    /// A generated password offered in the open suggestion list.
    generated: Option<Zeroizing<String>>,
    /// Fill-on-load already ran for this document.
    form_handled: bool,
    suggesting: bool,
}

thread_local! {
    static TABS: RefCell<HashMap<String, TabState>> = RefCell::new(HashMap::new());
}

fn with_tab<R>(id: &str, f: impl FnOnce(&mut TabState) -> R) -> R {
    TABS.with(|t| f(t.borrow_mut().entry(id.to_string()).or_default()))
}

#[derive(Deserialize)]
struct Msg {
    /// "login" | "new"
    #[serde(default)]
    k: String,
    /// "user" | "pass"
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
}

impl Drop for Msg {
    fn drop(&mut self) {
        self.p.zeroize();
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
    /// Sign-up or change-password form.
    new_password: bool,
    /// The field's rectangle in the page's CSS pixels, and the page's devicePixelRatio.
    rect: FieldRect,
    dpr: f64,
    accounts: Vec<Account>,
    generated: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SaveOffer {
    offer_id: u64,
    tab_id: String,
    site: String,
    username: String,
    /// An existing entry would get the new password.
    update: bool,
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

/// The origin of a page address (http and https only).
pub fn page_origin(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.origin().ascii_serialization())
}

/// Handles a `_zenith_pw_*` message from a page. `user`: WebView2 saw a user gesture.
/// `main_frame`: it came from the top-level document.
pub fn on_message(sh: &Rc<TabShared>, name: &str, uri: &str, user: bool, main_frame: bool) {
    let cfg = config();
    if !cfg.passwords_enabled || !main_frame {
        return;
    }
    let Some(origin) = page_origin(&sh.url.borrow()) else { return };
    let Some(msg) = parse_message(uri) else { return };
    match name {
        "_zenith_pw_field" if user => on_field(sh, &origin, &msg),
        "_zenith_pw_dismiss" => close_suggestions(&sh.id),
        "_zenith_pw_capture" if user => on_capture(sh, &origin, &msg),
        "_zenith_pw_form" => on_form(sh, &origin),
        _ => {}
    }
}

fn on_field(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    let cfg = config();
    let new_password = m.k == "new";
    let accounts = if cfg.autofill_suggest && (cfg.autofill_usernames || cfg.autofill_passwords) && !(new_password && m.f == "pass") {
        vault::try_with(|v| v.accounts_for(origin, cfg.autofill_same_site)).unwrap_or_default()
    } else {
        Vec::new()
    };
    let generated = (new_password && m.f == "pass" && cfg.password_generator).then(|| crypto::generate_password(20));
    if accounts.is_empty() && generated.is_none() {
        return;
    }
    let shown = generated.as_ref().map(|g| g.to_string());
    with_tab(&sh.id, |t| {
        t.generated = generated;
        t.suggesting = true;
    });
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    emit_tab(
        &sh.id,
        "password-suggest",
        Suggest {
            tab_id: sh.id.clone(),
            site: vault::display_site(origin),
            field: if m.f == "pass" { "pass".into() } else { "user".into() },
            new_password,
            rect: FieldRect { x: finite(m.x), y: finite(m.y), w: finite(m.w).max(0.0), h: finite(m.h).max(0.0) },
            dpr: if m.dpr.is_finite() && m.dpr > 0.0 { m.dpr } else { 1.0 },
            accounts,
            generated: shown,
        },
    );
}

fn on_capture(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    let cfg = config();
    if !cfg.passwords_offer_save || sh.private {
        return;
    }
    if m.u.chars().count() > 512 || m.p.chars().count() > 1024 {
        return;
    }
    let typed = m.u.trim().to_string();
    if m.p.is_empty() {
        // First step of a two-step login: remember who is signing in.
        if !typed.is_empty() {
            with_tab(&sh.id, |t| t.step_user = Some((origin.to_string(), typed, Instant::now())));
        }
        return;
    }
    let username = if typed.is_empty() {
        with_tab(&sh.id, |t| match &t.step_user {
            Some((o, u, at)) if at.elapsed() < STEP_TTL && vault::origin_match(o, origin, true).is_some() => u.clone(),
            _ => String::new(),
        })
    } else {
        typed
    };
    let Some(decision) = vault::try_with(|v| {
        if v.status().damaged.is_some() || v.is_never(origin) {
            None
        } else {
            Some(v.classify(origin, &username, &m.p))
        }
    })
    .flatten() else {
        return;
    };
    let update = match decision {
        Capture::Saved => return,
        Capture::Update(id) => Some(id),
        Capture::New => None,
    };
    let is_update = update.is_some();
    let offer_id = vault::add_offer(&sh.id, origin, &username, Zeroizing::new(m.p.clone()), update);
    let payload = SaveOffer { offer_id, tab_id: sh.id.clone(), site: vault::display_site(origin), username, update: is_update };
    // Let the page react first (a failed login re-renders; a good one navigates).
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        if vault::peek_offer(payload.offer_id).is_some() {
            emit_tab(&payload.tab_id.clone(), "password-save-offer", payload);
        }
    });
}

fn on_form(sh: &Rc<TabShared>, origin: &str) {
    let cfg = config();
    if !cfg.autofill_on_load || cfg.vault_ask_every_time || !(cfg.autofill_usernames || cfg.autofill_passwords) {
        return;
    }
    if with_tab(&sh.id, |t| std::mem::replace(&mut t.form_handled, true)) {
        return;
    }
    // Only the single account saved for exactly this origin, and only while unlocked.
    let creds = vault::try_with(|v| {
        if !v.unlocked() {
            return None;
        }
        let accounts = v.accounts_for(origin, false);
        match accounts.as_slice() {
            [a] if a.exact => v.credentials_for_fill(&a.id, origin, false).ok(),
            _ => None,
        }
    })
    .flatten();
    let Some((user, pass)) = creds else { return };
    let (tab, origin) = (sh.id.clone(), origin.to_string());
    post(move || {
        let u = cfg.autofill_usernames.then_some(user.as_str());
        let p = cfg.autofill_passwords.then_some(pass.as_str());
        fill(&tab, &origin, u, p, "login", |_| {});
    });
}

/// Hides the account list under a login field.
pub fn close_suggestions(tab_id: &str) {
    let was = with_tab(tab_id, |t| {
        t.generated = None;
        std::mem::replace(&mut t.suggesting, false)
    });
    if was {
        emit_tab(tab_id, "password-suggest-close", TabRef { tab_id: tab_id.to_string() });
    }
}

/// A new document is loading in the tab.
pub fn on_navigation(tab_id: &str) {
    close_suggestions(tab_id);
    with_tab(tab_id, |t| t.form_handled = false);
}

/// The tab closed.
pub fn forget(tab_id: &str) {
    TABS.with(|t| t.borrow_mut().remove(tab_id));
    vault::drop_tab_offers(tab_id);
}

/// The origin of the page in a tab, if it can receive credentials.
pub fn tab_origin(tab_id: &str) -> Option<String> {
    let url = with_host(|h| h.tabs.get(tab_id).map(|t| t.shared.url.borrow().clone())).flatten()?;
    page_origin(&url)
}

/// Fills a login (or, with mode "new", a new password) into the tab's top-level document,
/// provided it is still at `origin`.
pub fn fill(tab_id: &str, origin: &str, username: Option<&str>, password: Option<&str>, mode: &str, done: impl FnOnce(Result<(), String>) + 'static) {
    let core = with_host(|h| h.tabs.get(tab_id).map(|t| t.core())).flatten();
    let Some(core) = core else {
        return done(Err("The tab is no longer open.".into()));
    };
    if tab_origin(tab_id).as_deref() != Some(origin) {
        return done(Err("The page changed before it could be filled.".into()));
    }
    let payload = Zeroizing::new(serde_json::json!({ "origin": origin, "u": username, "p": password, "mode": mode }).to_string());
    let script = Zeroizing::new(format!("{FILL_SCRIPT}({})", payload.as_str()));
    let done = RefCell::new(Some(done));
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
        let outcome = match (result, json.trim_matches('"')) {
            (Ok(()), "ok") => Ok(()),
            (Ok(()), "origin") => Err("The page changed before it could be filled.".to_string()),
            (Ok(()), _) => Err("No login form was found on this page.".to_string()),
            (Err(e), _) => Err(e.message().to_string()),
        };
        if let Some(done) = done.borrow_mut().take() {
            done(outcome);
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = core.ExecuteScript(&HSTRING::from(script.as_str()), &handler) {
            eprintln!("[zenith] autofill script failed: {}", e.message());
        }
    }
}

/// Fills the generated password from the open suggestion list.
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

/// True when a NewWindowRequested event came from the top-level document (true when WebView2 is
/// too old to say).
pub fn from_main_frame(args: &ICoreWebView2NewWindowRequestedEventArgs) -> bool {
    unsafe {
        let Ok(a3) = args.cast::<ICoreWebView2NewWindowRequestedEventArgs3>() else { return true };
        let Ok(frame) = a3.OriginalSourceFrameInfo() else { return true };
        let Ok(f2) = frame.cast::<ICoreWebView2FrameInfo2>() else { return true };
        let mut kind = COREWEBVIEW2_FRAME_KIND::default();
        if f2.FrameKind(&mut kind).is_err() {
            return true;
        }
        kind == COREWEBVIEW2_FRAME_KIND_MAIN_FRAME
    }
}

#[cfg(test)]
mod tests {
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
    }
}
