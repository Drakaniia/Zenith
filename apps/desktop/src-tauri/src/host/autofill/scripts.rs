//! The scripts autofill injects into pages: the page script that reports login and payment
//! fields, and the fill scripts run through `ExecuteScript`.

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
pub(super) const FILL_SCRIPT: &str = concat!(
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
pub(super) const CARD_FILL_SCRIPT: &str = concat!(
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
