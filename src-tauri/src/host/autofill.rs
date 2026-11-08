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
use crate::vault::cards::{self, CardInfo, FillCard};
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
  const CVC = /cvc|cvv|cvn|csc|security.?code|card.?(verification|code)/i;
  const CNUM = /card.?(number|num|no)\b|cardnumber|cc.?(num|number)|\bpan\b/i;
  const CMON = /(exp|expiry|expiration).?(month|mm)\b|exp.?mo\b|card.?month/i;
  const CYEAR = /(exp|expiry|expiration).?(year|yy|yyyy)\b|exp.?yr|card.?year/i;
  const CEXP = /(exp|expiry|expiration)(.?date)?\b|valid.?(thru|until)/i;
  const CNAME = /card.?holder|name.?on.?card|cc.?name|card.?name/i;
  // What a payment form field is: num, exp, mm, yy, cvc or name ('' for anything else).
  const ccKind = (el) => {
    const sel = el instanceof HTMLSelectElement;
    if (!sel && !(el instanceof HTMLInputElement)) return '';
    if (!sel && !['text', 'tel', 'number', 'password', 'email', ''].includes(el.type)) return '';
    const a = (el.autocomplete || '').toLowerCase();
    let k = /\bcc-number\b/.test(a) ? 'num' : /\bcc-csc\b/.test(a) ? 'cvc' : /\bcc-exp-month\b/.test(a) ? 'mm' : /\bcc-exp-year\b/.test(a) ? 'yy' : /\bcc-exp\b/.test(a) ? 'exp' : /\bcc-name\b/.test(a) ? 'name' : '';
    if (!k) {
      const h = [el.name, el.id, el.placeholder, el.getAttribute('aria-label')].join(' ');
      k = CVC.test(h) ? 'cvc' : CNUM.test(h) ? 'num' : CMON.test(h) ? 'mm' : CYEAR.test(h) ? 'yy' : CEXP.test(h) ? 'exp' : CNAME.test(h) ? 'name' : '';
    }
    if (sel && k !== 'mm' && k !== 'yy') return '';
    if (el.type === 'password' && k !== 'cvc') return '';
    return k;
  };
  const cardFields = (root) => {
    const f = {};
    for (const i of root.querySelectorAll('input, select')) {
      const k = ccKind(i);
      if (k && !f[k] && shown(i)) f[k] = i;
    }
    return f;
  };
  // A security code field of type password is not a login password.
  const passes = (root) => [...root.querySelectorAll('input[type=password]')].filter((p) => shown(p) && ccKind(p) !== 'cvc');
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
  const info = (el) => {
    const r = el.getBoundingClientRect();
    return { k: kind(el, scopeOf(el)), f: isPass(el) ? 'pass' : 'user', x: r.left, y: r.top, w: r.width, h: r.height, dpr: window.devicePixelRatio || 1 };
  };
  const offer = (el) => {
    const now = Date.now();
    if (el === lastEl && now - lastAt < 500) return;
    lastEl = el; lastAt = now; suggesting = true;
    send('_zenith_pw_field', info(el));
  };
  const target = (e) => e.composedPath()[0];
  addEventListener('focusin', (e) => { if (loginField(target(e))) offer(target(e)); }, true);
  addEventListener('click', (e) => { if (e.isTrusted && loginField(target(e))) offer(target(e)); }, true);
  const dismiss = () => { if (suggesting) { suggesting = false; send('_zenith_pw_dismiss', {}); } };
  // Typing in a username field keeps the list open and narrows it to the matching accounts (the
  // typed text goes to the browser only, which compares it with the saved usernames). Typing a
  // password closes it.
  addEventListener('input', (e) => {
    const el = target(e);
    if (!e.isTrusted || !loginField(el)) return;
    if (isPass(el)) return dismiss();
    if (suggesting) send('_zenith_pw_type', { ...info(el), u: el.value.slice(0, 256) });
  }, true);

  // Payment forms: clicking the card number, name or expiry shows the saved cards; a submitted
  // card is offered for saving.
  const cardField = (el) => el instanceof HTMLInputElement && !el.readOnly && shown(el) && ['num', 'name', 'exp'].includes(ccKind(el));
  const offerCard = (el) => {
    const now = Date.now();
    if (el === lastEl && now - lastAt < 500) return;
    lastEl = el; lastAt = now; suggesting = true;
    const r = el.getBoundingClientRect();
    send('_zenith_card_field', { f: 'card', x: r.left, y: r.top, w: r.width, h: r.height, dpr: window.devicePixelRatio || 1 });
  };
  addEventListener('focusin', (e) => { if (cardField(target(e))) offerCard(target(e)); }, true);
  addEventListener('click', (e) => { if (e.isTrusted && cardField(target(e))) offerCard(target(e)); }, true);
  addEventListener('input', (e) => { if (e.isTrusted && cardField(target(e))) dismiss(); }, true);
  let lastCard = '';
  const optText = (el) => el instanceof HTMLSelectElement && el.selectedOptions[0] ? el.value + ' ' + el.selectedOptions[0].text : el.value;
  const captureCard = (root) => {
    const f = cardFields(root);
    if (!f.num) return;
    const n = f.num.value.replace(/[\s-]/g, '');
    if (!/^\d{12,19}$/.test(n)) return;
    let m = '', y = '';
    if (f.mm && f.yy) { m = optText(f.mm).trim(); y = optText(f.yy).trim(); }
    else if (f.exp) {
      const x = f.exp.value.match(/(\d{1,2})\s*[\/\-. ]?\s*(\d{4}|\d{2})\s*$/);
      if (x) { m = x[1]; y = x[2]; }
    }
    if (!m || !y) return;
    const c = f.cvc ? f.cvc.value.trim().slice(0, 4) : '';
    const key = n + '\n' + m + '\n' + y;
    if (key === lastCard) return;
    lastCard = key;
    setTimeout(() => { if (lastCard === key) lastCard = ''; }, 4000);
    send('_zenith_card_capture', { cn: n, cm: m.slice(0, 40), cy: y.slice(0, 40), cc: c, ch: f.name ? f.name.value.trim().slice(0, 200) : '' });
  };

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
  addEventListener('submit', (e) => { if (e.target instanceof HTMLFormElement) { capture(e.target); captureCard(e.target); } }, true);
  addEventListener('click', (e) => {
    if (!e.isTrusted) return;
    const b = e.composedPath().find((n) => n instanceof Element && n.matches('button, input[type=submit], input[type=button], [role=button]'));
    if (!b) return;
    const root = scopeOf(b);
    if (root !== document || passes(root).some((p) => p.value)) capture(root);
    captureCard(root);
  }, true);
  addEventListener('keydown', (e) => {
    if (e.key === 'Escape') return dismiss();
    if (e.key !== 'Enter' || !e.isTrusted) return;
    const el = target(e);
    if (el instanceof HTMLInputElement && (isPass(el) || isUser(el))) capture(scopeOf(el));
    if (ccKind(el)) captureCard(scopeOf(el));
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

/// Called as `(CARD_FILL_SCRIPT)(payload)` in the top-level document. Same results as `FILL_SCRIPT`.
const CARD_FILL_SCRIPT: &str = concat!(
    "((d) => {\n  if (location.origin !== d.origin) return 'origin';\n",
    helpers!(),
    r#"
  const desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
  const fire = (el) => {
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const setText = (el, v) => { el.focus(); desc.set.call(el, v); fire(el); };
  // A month or year drop-down: the option whose value or text starts with the number.
  const setSelect = (el, v) => {
    const want = (s) => { const m = String(s).match(/\d+/); return m ? parseInt(m[0], 10) : NaN; };
    const o = [...el.options].find((x) => want(x.value) === v || want(x.text) === v);
    if (!o) return false;
    el.value = o.value;
    fire(el);
    return true;
  };
  const a = document.activeElement;
  let f = a instanceof Element ? cardFields(scopeOf(a)) : {};
  if (!f.num) f = cardFields(document);
  if (!f.num) return 'nofield';
  const two = (n) => String(n).padStart(2, '0');
  const long = (el) => /yyyy/i.test(el.placeholder || '') || (el.maxLength > 0 && el.maxLength >= 7);
  setText(f.num, d.n);
  if (f.exp) setText(f.exp, two(d.m) + '/' + (long(f.exp) ? d.y : two(d.y % 100)));
  if (f.mm) { if (f.mm instanceof HTMLSelectElement) setSelect(f.mm, d.m); else setText(f.mm, two(d.m)); }
  if (f.yy) {
    if (f.yy instanceof HTMLSelectElement) { if (!setSelect(f.yy, d.y)) setSelect(f.yy, d.y % 100); }
    else setText(f.yy, f.yy.maxLength === 2 || /yy$/i.test(f.yy.placeholder || '') && !/yyyy/i.test(f.yy.placeholder || '') ? two(d.y % 100) : String(d.y));
  }
  if (f.name && d.h && !f.name.value) setText(f.name, d.h);
  if (f.cvc && d.c) setText(f.cvc, d.c);
  return 'ok';
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
    /// The user closed the list: typing in the field does not bring it back until the next click.
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
    // A card captured on a payment form: number, expiry month and year (as the page shows
    // them), security code and cardholder name.
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
    /// Sign-up or change-password form.
    new_password: bool,
    /// The field's rectangle in the page's CSS pixels, and the page's devicePixelRatio.
    rect: FieldRect,
    dpr: f64,
    accounts: Vec<Account>,
    /// Saved cards, when `field` is "card" (a payment form).
    cards: Vec<CardInfo>,
    generated: Option<String>,
    /// The same list as already shown, narrowed by what was typed: it is updated in place.
    refresh: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CardSaveOffer {
    offer_id: u64,
    tab_id: String,
    site: String,
    brand: String,
    last4: String,
    exp_month: u8,
    exp_year: u16,
    /// The page also sent a security code, which is only saved when the user ticks the box.
    has_cvc: bool,
    /// A saved card would get the new expiry.
    update: bool,
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
        "_zenith_pw_type" if user => on_type(sh, &origin, &msg),
        "_zenith_pw_dismiss" => close_suggestions(&sh.id),
        "_zenith_pw_capture" if user => on_capture(sh, &origin, &msg),
        "_zenith_pw_form" => on_form(sh, &origin),
        "_zenith_card_field" if user => on_card_field(sh, &origin, &msg),
        "_zenith_card_capture" if user => on_card_capture(sh, &origin, &msg),
        _ => {}
    }
}

/// The accounts whose username (or label) contains what was typed, ignoring case; all of them when
/// nothing is typed.
fn matching_accounts(accounts: Vec<Account>, typed: &str) -> Vec<Account> {
    let q = typed.trim().to_lowercase();
    if q.is_empty() {
        return accounts;
    }
    accounts
        .into_iter()
        .filter(|a| a.username.to_lowercase().contains(&q) || a.label.to_lowercase().contains(&q))
        .collect()
}

fn on_field(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    with_tab(&sh.id, |t| t.muted = false);
    show_list(sh, origin, m, None);
}

/// A username field changed: the open list narrows to the matching accounts, and comes back when
/// the text matches again. Nothing when the user closed the list.
fn on_type(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    if m.f != "user" || with_tab(&sh.id, |t| t.muted) {
        return;
    }
    show_list(sh, origin, m, Some(m.u.as_str()));
}

fn show_list(sh: &Rc<TabShared>, origin: &str, m: &Msg, typed: Option<&str>) {
    let cfg = config();
    let new_password = m.k == "new";
    let mut accounts = if cfg.autofill_suggest && (cfg.autofill_usernames || cfg.autofill_passwords) && !(new_password && m.f == "pass") {
        vault::try_with(|v| v.accounts_for(origin, cfg.autofill_same_site)).unwrap_or_default()
    } else {
        Vec::new()
    };
    if let Some(typed) = typed {
        accounts = matching_accounts(accounts, typed);
    }
    let generated = (typed.is_none() && new_password && m.f == "pass" && cfg.password_generator).then(|| crypto::generate_password(20));
    if accounts.is_empty() && generated.is_none() {
        // Typing that matches nothing closes the list; it returns when the text matches again.
        if typed.is_some() {
            close_suggestions(&sh.id);
        }
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
            cards: Vec::new(),
            generated: shown,
            refresh: typed.is_some(),
        },
    );
}

/// A card number, name or expiry field was clicked: the saved cards are listed under it. Cards
/// are only offered on HTTPS pages.
fn on_card_field(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    with_tab(&sh.id, |t| t.muted = false);
    let cfg = config();
    if !cfg.autofill_cards || !origin.starts_with("https://") {
        return;
    }
    let list = vault::try_with(|v| v.cards_for_page()).unwrap_or_default();
    if list.is_empty() {
        return;
    }
    with_tab(&sh.id, |t| {
        t.generated = None;
        t.suggesting = true;
    });
    let finite = |v: f64| if v.is_finite() { v } else { 0.0 };
    emit_tab(
        &sh.id,
        "password-suggest",
        Suggest {
            tab_id: sh.id.clone(),
            site: vault::display_site(origin),
            field: "card".into(),
            new_password: false,
            rect: FieldRect { x: finite(m.x), y: finite(m.y), w: finite(m.w).max(0.0), h: finite(m.h).max(0.0) },
            dpr: if m.dpr.is_finite() && m.dpr > 0.0 { m.dpr } else { 1.0 },
            accounts: Vec::new(),
            cards: list,
            generated: None,
            refresh: false,
        },
    );
}

/// A card was submitted on a payment form: offer to save it (or its new expiry). The number never
/// reaches the chrome UI; the offer tells it only the brand, last four digits and expiry.
fn on_card_capture(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    let cfg = config();
    if !cfg.cards_offer_save || sh.private || !origin.starts_with("https://") {
        return;
    }
    let Some(number) = cards::digits_only(&m.cn) else { return };
    if !cards::valid_number(&number) {
        return;
    }
    let Some((month, year)) = cards::parse_expiry(&m.cm, &m.cy) else { return };
    let has_cvc = (3..=4).contains(&m.cc.len()) && m.cc.bytes().all(|b| b.is_ascii_digit());
    let cvc = Zeroizing::new(if has_cvc { m.cc.clone() } else { String::new() });
    let holder: String = m.ch.trim().chars().take(200).collect();
    let Some(decision) = vault::try_with(|v| {
        if v.status().damaged.is_some() || v.is_never(origin) {
            None
        } else {
            Some(v.classify_card(&number, month, year))
        }
    })
    .flatten() else {
        return;
    };
    let update = match decision {
        cards::CardCapture::Saved => return,
        cards::CardCapture::Update(id) => Some(id),
        cards::CardCapture::New => None,
    };
    let payload = CardSaveOffer {
        offer_id: 0,
        tab_id: sh.id.clone(),
        site: vault::display_site(origin),
        brand: cards::brand_of(&number).to_string(),
        last4: number[number.len() - 4..].to_string(),
        exp_month: month,
        exp_year: year,
        has_cvc,
        update: update.is_some(),
    };
    let offer = cards::CardOffer::new(&sh.id, origin, number, cvc, &holder, month, year, update);
    let payload = CardSaveOffer { offer_id: cards::add_card_offer(offer), ..payload };
    // Let the page react first (a declined payment re-renders; a good one navigates).
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(700));
        if cards::has_card_offer(payload.offer_id) {
            emit_tab(&payload.tab_id.clone(), "card-save-offer", payload);
        }
    });
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

/// The user closed the list: typing in the field leaves it closed until the field is clicked again.
pub fn mute_suggestions(tab_id: &str) {
    with_tab(tab_id, |t| t.muted = true);
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
    cards::drop_tab_card_offers(tab_id);
}

/// The origin of the page in a tab, if it can receive credentials.
pub fn tab_origin(tab_id: &str) -> Option<String> {
    let url = with_host(|h| h.tabs.get(tab_id).map(|t| t.shared.url.borrow().clone())).flatten()?;
    page_origin(&url)
}

/// Fills a login (or, with mode "new", a new password) into the tab's top-level document,
/// provided it is still at `origin`.
pub fn fill(tab_id: &str, origin: &str, username: Option<&str>, password: Option<&str>, mode: &str, done: impl FnOnce(Result<(), String>) + 'static) {
    let payload = Zeroizing::new(serde_json::json!({ "origin": origin, "u": username, "p": password, "mode": mode }).to_string());
    run_fill(tab_id, origin, FILL_SCRIPT, payload, "No login form was found on this page.", done);
}

/// Fills a saved card into the payment form of the tab's top-level document, provided it is
/// still at `origin`.
pub fn fill_card(tab_id: &str, origin: &str, card: &FillCard, done: impl FnOnce(Result<(), String>) + 'static) {
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

fn run_fill(tab_id: &str, origin: &str, script: &str, payload: Zeroizing<String>, nofield: &'static str, done: impl FnOnce(Result<(), String>) + 'static) {
    let core = with_host(|h| h.tabs.get(tab_id).map(|t| t.core())).flatten();
    let Some(core) = core else {
        return done(Err("The tab is no longer open.".into()));
    };
    if tab_origin(tab_id).as_deref() != Some(origin) {
        return done(Err("The page changed before it could be filled.".into()));
    }
    let script = Zeroizing::new(format!("{script}({})", payload.as_str()));
    let done = RefCell::new(Some(done));
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
        let outcome = match (result, json.trim_matches('"')) {
            (Ok(()), "ok") => Ok(()),
            (Ok(()), "origin") => Err("The page changed before it could be filled.".to_string()),
            (Ok(()), _) => Err(nofield.to_string()),
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
    fn typing_narrows_the_accounts() {
        let acc = |u: &str, l: &str| Account { id: u.into(), username: u.into(), label: l.into(), site: "x.com".into(), exact: true };
        let all = vec![acc("ann@x.com", ""), acc("Bob@y.org", "work"), acc("cy@x.com", "")];
        let names = |v: Vec<Account>| v.into_iter().map(|a| a.username).collect::<Vec<_>>();
        assert_eq!(names(matching_accounts(all.clone(), "")).len(), 3);
        assert_eq!(names(matching_accounts(all.clone(), "  ")).len(), 3);
        assert_eq!(names(matching_accounts(all.clone(), "AN")), ["ann@x.com"]);
        assert_eq!(names(matching_accounts(all.clone(), "x.com")), ["ann@x.com", "cy@x.com"]);
        assert_eq!(names(matching_accounts(all.clone(), "work")), ["Bob@y.org"]);
        assert!(matching_accounts(all, "zzz").is_empty());
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
