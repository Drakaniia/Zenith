//! Page tools via CEF DevTools protocol: reader view, picture-in-picture, switching text
//! direction, screenshots, responsive design mode, and opening DevTools.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use cef::Browser;

use super::{cdp_call, eval_js, open_devtools, with_host};

pub type Done = Box<dyn FnOnce(Result<serde_json::Value, String>)>;

pub fn handles(cmd: &str) -> bool {
    matches!(
        cmd,
        "reader" | "pip" | "textDirection" | "screenshot" | "responsive" | "taskManager" | "devtools"
    )
}

thread_local! {
    static RESPONSIVE: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub fn run(id: &str, cmd: &str, _arg: Option<serde_json::Value>, done: Done) {
    let Some(browser) = with_host(|h| h.tabs.get(id).and_then(|t| t.browser.clone())).flatten() else {
        return done(Err("tab has no browser".into()));
    };
    match cmd {
        "reader" => eval_js(&browser, READER_SCRIPT, false, false, done),
        "textDirection" => eval_js(&browser, TEXT_DIRECTION_SCRIPT, false, false, done),
        "pip" => eval_js(&browser, PIP_SCRIPT, true, true, move |r| {
            done(Ok(r.unwrap_or_else(|_| serde_json::json!("blocked"))))
        }),
        "screenshot" => {
            cdp_call(&browser, "Page.captureScreenshot", r#"{"format":"png"}"#, move |r| {
                done(
                    r.and_then(|json| save_screenshot(&json))
                        .map(|p| serde_json::json!(p.display().to_string())),
                );
            });
        }
        "responsive" => responsive(id, &browser, done),
        "taskManager" => {
            // shortcut: Chromium's built-in Task Manager dialog isn't exposed in stock CEF;
            // open Windows Task Manager or report cleanly until Phase 4 task-manager UI.
            done(Ok(serde_json::Value::Null));
        }
        "devtools" => {
            done(open_devtools(&browser).map(|_| serde_json::Value::Null));
        }
        other => done(Err(format!("unknown tab command {other}"))),
    }
}

fn save_screenshot(json: &str) -> Result<PathBuf, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let data = v["data"].as_str().ok_or("the page could not be captured")?;
    let png = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let dir = crate::media::job::download_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = unique_path(&dir, &screenshot_name(local_time()));
    std::fs::write(&path, png).map_err(|e| format!("the screenshot could not be saved: {e}"))?;
    Ok(path)
}

fn local_time() -> [u16; 6] {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    [t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond]
}

fn screenshot_name([y, mo, d, h, mi, s]: [u16; 6]) -> String {
    format!("Screenshot {y:04}-{mo:02}-{d:02} at {h:02}.{mi:02}.{s:02}.png")
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    (2..1000)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn responsive(id: &str, browser: &Browser, done: Done) {
    let on = RESPONSIVE.with(|r| {
        let mut r = r.borrow_mut();
        if r.remove(id) {
            false
        } else {
            r.insert(id.to_string());
            true
        }
    });
    let (metrics_method, metrics, touch) = if on {
        (
            "Emulation.setDeviceMetricsOverride",
            r#"{"width":390,"height":0,"deviceScaleFactor":0,"mobile":true}"#,
            r#"{"enabled":true,"maxTouchPoints":5}"#,
        )
    } else {
        ("Emulation.clearDeviceMetricsOverride", "{}", r#"{"enabled":false}"#)
    };
    let b2 = browser.clone();
    cdp_call(browser, metrics_method, metrics, move |r| {
        if let Err(e) = r {
            return done(Err(e));
        }
        cdp_call(&b2, "Emulation.setTouchEmulationEnabled", touch, move |_| {
            done(Ok(serde_json::json!(on)))
        });
    });
}

const TEXT_DIRECTION_SCRIPT: &str = r#"(() => {
  let el = document.activeElement;
  while (el && el.shadowRoot && el.shadowRoot.activeElement) el = el.shadowRoot.activeElement;
  const field = el && (el.isContentEditable || el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') ? el : document.documentElement;
  const next = getComputedStyle(field).direction === 'rtl' ? 'ltr' : 'rtl';
  field.dir = next;
  return next;
})()"#;

const PIP_SCRIPT: &str = r#"(async () => {
  if (document.pictureInPictureElement) {
    await document.exitPictureInPicture();
    return 'off';
  }
  const all = [...document.querySelectorAll('video')];
  if (!all.length) return 'none';
  const area = (v) => { const r = v.getBoundingClientRect(); return r.width * r.height; };
  const ready = all.filter((v) => v.readyState > 0 && !v.disablePictureInPicture);
  if (!ready.length || !document.pictureInPictureEnabled) return 'blocked';
  ready.sort((a, b) => (a.paused - b.paused) || (area(b) - area(a)));
  try {
    await ready[0].requestPictureInPicture();
    return 'on';
  } catch (e) {
    return 'blocked';
  }
})()"#;

const READER_SCRIPT: &str = r#"(() => {
  if (window.__zenithReader) {
    location.reload();
    return 'off';
  }
  const doc = document;
  const textOf = (el) => [...el.querySelectorAll('p')].reduce((n, p) => n + p.textContent.trim().length, 0);
  let best = null;
  let score = 0;
  for (const el of doc.querySelectorAll('article, main, [role="main"], [itemprop="articleBody"], .post-content, .entry-content, .article-body, .article-content, #content')) {
    const n = textOf(el);
    if (n > score) { score = n; best = el; }
  }
  if (score < 500) {
    const parents = new Map();
    for (const p of doc.querySelectorAll('p')) {
      const len = p.textContent.trim().length;
      if (len < 40 || !p.parentElement) continue;
      parents.set(p.parentElement, (parents.get(p.parentElement) || 0) + len);
    }
    for (const [el, n] of parents) if (n > score) { score = n; best = el; }
  }
  if (!best || score < 400) return 'none';
  for (;;) {
    const inner = [...best.children].find((c) => textOf(c) >= score * 0.8);
    if (!inner) break;
    best = inner;
  }
  const meta = (sel) => doc.querySelector(sel)?.getAttribute('content')?.trim() || '';
  const heading = doc.querySelector('h1')?.textContent?.trim() || '';
  const pageTitle = meta('meta[property="og:title"]') || doc.title.trim();
  const title = heading && heading.length < 200 && pageTitle.includes(heading) ? heading : pageTitle;
  const site = meta('meta[property="og:site_name"]') || location.hostname.replace(/^www\./, '');
  const byline = meta('meta[name="author"]') || doc.querySelector('[rel="author"], .byline, .author')?.textContent?.trim().slice(0, 120) || '';
  const article = best.cloneNode(true);
  article.querySelectorAll('script, style, link, noscript, iframe, form, button, input, select, textarea, nav, aside, footer, svg, canvas, video, audio, object, embed, dialog, [hidden], [aria-hidden="true"], [role="navigation"], [role="complementary"], .ad, .ads, .advert, .share, .social, .related, .comments, .newsletter, .mw-editsection, sup.reference, .navbox, .infobox, .hatnote, .shortdescription').forEach((n) => n.remove());
  for (const el of [...article.querySelectorAll('ul, ol, table, div')]) {
    const text = el.textContent.trim().length;
    if (!text || !el.isConnected) continue;
    const links = [...el.querySelectorAll('a')].reduce((n, a) => n + a.textContent.trim().length, 0);
    if (links / text > 0.6 && el.querySelectorAll('p').length < 2) el.remove();
  }
  const keep = new Set(['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'UL', 'OL', 'LI', 'BLOCKQUOTE', 'PRE', 'CODE', 'EM', 'STRONG', 'B', 'I', 'U', 'S', 'A', 'IMG', 'FIGURE', 'FIGCAPTION', 'BR', 'HR', 'TABLE', 'THEAD', 'TBODY', 'TFOOT', 'TR', 'TD', 'TH', 'CAPTION', 'SUP', 'SUB', 'DL', 'DT', 'DD', 'MARK', 'SMALL', 'TIME', 'ABBR', 'Q', 'CITE', 'KBD', 'SAMP', 'VAR', 'DEL', 'INS']);
  const attrs = { A: ['href'], IMG: ['src', 'alt', 'srcset', 'width', 'height'], TD: ['colspan', 'rowspan'], TH: ['colspan', 'rowspan'] };
  for (const el of [...article.querySelectorAll('*')]) {
    if (!keep.has(el.tagName)) { el.replaceWith(...el.childNodes); continue; }
    if (el.tagName === 'IMG') {
      const lazy = el.getAttribute('data-src') || el.getAttribute('data-lazy-src') || el.getAttribute('data-original');
      if (lazy && (!el.getAttribute('src') || el.getAttribute('src').startsWith('data:'))) el.setAttribute('src', lazy);
    }
    const allowed = attrs[el.tagName] || [];
    for (const a of [...el.attributes]) if (!allowed.includes(a.name)) el.removeAttribute(a.name);
    if (el.tagName === 'A' && /^\s*javascript:/i.test(el.getAttribute('href') || '')) el.removeAttribute('href');
  }
  const h1 = article.querySelector('h1');
  if (h1 && h1.textContent.trim() === title) h1.remove();
  const words = (article.textContent || '').trim().split(/\s+/).length;
  const make = (tag, cls, text) => { const el = doc.createElement(tag); if (cls) el.className = cls; if (text) el.textContent = text; return el; };
  const head = doc.createElement('head');
  const viewport = doc.createElement('meta'); viewport.name = 'viewport'; viewport.content = 'width=device-width, initial-scale=1';
  head.append(viewport, make('title', '', title));
  const main = make('main', 'zr');
  main.append(make('p', 'zr-site', `${site} · ${Math.max(1, Math.round(words / 230))} min read`), make('h1', 'zr-title', title));
  if (byline) main.append(make('p', 'zr-byline', byline));
  main.append(article);
  const body = doc.createElement('body'); body.append(main);
  const root = doc.documentElement;
  for (const a of [...root.attributes]) if (a.name !== 'lang' && a.name !== 'dir') root.removeAttribute(a.name);
  root.replaceChildren(head, body);
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(`:root{color-scheme:light dark}body{margin:0;background:#fbfaf7;color:#1f1f1f;font:20px/1.65 Charter,Georgia,serif}@media(prefers-color-scheme:dark){body{background:#1c1b22;color:#e6e4e0}.zr a{color:#8ab4f8}}.zr{max-width:42rem;margin:0 auto;padding:56px 24px 120px;overflow-wrap:break-word}`);
  doc.adoptedStyleSheets = [sheet];
  window.__zenithReader = true;
  scrollTo(0, 0);
  return 'on';
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_names_sort_by_time() {
        assert_eq!(screenshot_name([2026, 11, 7, 9, 5, 3]), "Screenshot 2026-11-07 at 09.05.03.png");
    }

    #[test]
    fn taken_names_are_numbered() {
        let dir = std::env::temp_dir().join(format!("zenith-shot-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = "Screenshot 2026-01-01 at 00.00.00.png";
        assert_eq!(unique_path(&dir, name), dir.join(name));
        std::fs::write(dir.join(name), b"x").unwrap();
        assert_eq!(unique_path(&dir, name), dir.join("Screenshot 2026-01-01 at 00.00.00 (2).png"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
