//! Page tools behind Zen's (Firefox's) shortcuts that WebView2 has no built-in command for:
//! reader view, picture-in-picture, switching the text direction, screenshots, responsive design
//! mode, the task manager and opening DevTools on a given panel.
//!
//! Scripts run in the page's own context through `ExecuteScript` or the DevTools protocol, so
//! they can't reach anything the page itself couldn't; nothing they return is trusted beyond a
//! short status word.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use std::result::Result;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING};

use super::{cdp_call, chrome_cdp, with_host};

pub type Done = Box<dyn FnOnce(Result<serde_json::Value, String>)>;

/// The commands this module answers (through a callback, unlike `tab::command`).
pub fn handles(cmd: &str) -> bool {
    matches!(cmd, "reader" | "pip" | "textDirection" | "screenshot" | "responsive" | "taskManager" | "devtools")
}

thread_local! {
    /// Tabs in responsive design mode.
    static RESPONSIVE: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

pub fn run(id: &str, cmd: &str, arg: Option<serde_json::Value>, done: Done) {
    let Some(core) = with_host(|h| h.tabs.get(id).map(|t| t.core())).flatten() else {
        return done(Err("tab has no webview".into()));
    };
    match cmd {
        "reader" => script(&core, READER_SCRIPT, done),
        "textDirection" => script(&core, TEXT_DIRECTION_SCRIPT, done),
        "pip" => {
            // requestPictureInPicture needs a user gesture: Runtime.evaluate can grant one.
            let params = serde_json::json!({
                "expression": PIP_SCRIPT,
                "awaitPromise": true,
                "userGesture": true,
                "returnByValue": true,
            });
            cdp_call(&core, "Runtime.evaluate", &params.to_string(), move |r| {
                done(r.map(|json| evaluated(&json).unwrap_or(serde_json::json!("blocked"))));
            });
        }
        "screenshot" => {
            cdp_call(&core, "Page.captureScreenshot", r#"{"format":"png"}"#, move |r| {
                done(r.and_then(|json| save_screenshot(&json)).map(|p| serde_json::json!(p.display().to_string())));
            });
        }
        "responsive" => responsive(id, &core, done),
        "taskManager" => {
            let opened = core
                .cast::<ICoreWebView2_6>()
                .and_then(|c| unsafe { c.OpenTaskManagerWindow() })
                .map_err(|e| e.to_string());
            done(opened.map(|_| serde_json::Value::Null));
        }
        "devtools" => {
            let opened = unsafe { core.OpenDevToolsWindow() }.map_err(|e| e.to_string());
            if let (Ok(()), Some(panel)) = (&opened, arg.as_ref().and_then(|v| v.as_str())) {
                if PANELS.contains(&panel) {
                    show_panel(panel.to_string(), 0);
                }
            }
            done(opened.map(|_| serde_json::Value::Null));
        }
        other => done(Err(format!("unknown tab command {other}"))),
    }
}

/// Runs a script with `ExecuteScript` and hands back its (JSON) result.
fn script(core: &ICoreWebView2, source: &'static str, done: Done) {
    let done = RefCell::new(Some(done));
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
        if let Some(done) = done.borrow_mut().take() {
            done(
                result
                    .map_err(|e| e.message().to_string())
                    .map(|_| serde_json::from_str(&json).unwrap_or(serde_json::Value::Null)),
            );
        }
        Ok(())
    }));
    if let Err(e) = unsafe { core.ExecuteScript(&HSTRING::from(source), &handler) } {
        eprintln!("[zenith] page tool script failed: {e}");
    }
}

/// The value of a `Runtime.evaluate` result.
fn evaluated(json: &str) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("exceptionDetails").is_some() {
        return None;
    }
    v["result"].get("value").cloned()
}

// ---------------------------------------------------------------------------------------------
// Screenshots

/// Writes a `Page.captureScreenshot` result into the downloads folder; returns the file.
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

/// Year, month, day, hour, minute, second in local time.
fn local_time() -> [u16; 6] {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    [t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond]
}

/// "Screenshot 2026-11-07 at 09.05.03.png", like Firefox's screenshot files.
fn screenshot_name([y, mo, d, h, mi, s]: [u16; 6]) -> String {
    format!("Screenshot {y:04}-{mo:02}-{d:02} at {h:02}.{mi:02}.{s:02}.png")
}

/// `name` in `dir`, numbered "name (2).png" when it is taken.
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

// ---------------------------------------------------------------------------------------------
// Responsive design mode

/// Toggles a phone-sized viewport with touch input (DevTools' device mode). Answers whether it
/// is now on.
fn responsive(id: &str, core: &ICoreWebView2, done: Done) {
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
    let core2 = core.clone();
    cdp_call(core, metrics_method, metrics, move |r| {
        if let Err(e) = r {
            return done(Err(e));
        }
        cdp_call(&core2, "Emulation.setTouchEmulationEnabled", touch, move |_| done(Ok(serde_json::json!(on))));
    });
}

// ---------------------------------------------------------------------------------------------
// DevTools panels

/// Panels `show_panel` may be asked for (Chromium's panel names).
const PANELS: &[&str] = &["elements", "console", "sources", "network", "timeline", "resources", "memory"];

/// Switches the DevTools window that just opened to `panel`. WebView2 can only open DevTools, so
/// this finds the DevTools front end among the browser's targets, attaches to it and asks it to
/// show the panel, retrying while the window is still loading. If any of that fails, DevTools
/// simply stays on its last panel.
fn show_panel(panel: String, attempt: u32) {
    const ATTEMPTS: u32 = 20;
    if attempt >= ATTEMPTS {
        return;
    }
    let retry = move |panel: String| {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            let _ = super::app().run_on_main_thread(move || show_panel(panel, attempt + 1));
        });
    };
    chrome_cdp("Target.getTargets", r#"{"filter":[{}]}"#, move |r| {
        let target = r.ok().and_then(|json| devtools_target(&json));
        let Some(target) = target else { return retry(panel) };
        let params = serde_json::json!({ "targetId": target, "flatten": true }).to_string();
        chrome_cdp("Target.attachToTarget", &params, move |r| {
            let session = r
                .ok()
                .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
                .and_then(|v| v["sessionId"].as_str().map(str::to_string));
            let Some(session) = session else { return retry(panel) };
            let expression = format!("({SHOW_PANEL_SCRIPT})({})", serde_json::json!(panel));
            let params = serde_json::json!({ "expression": expression, "returnByValue": true, "awaitPromise": true }).to_string();
            let s2 = session.clone();
            session_cdp(&session, "Runtime.evaluate", &params, move |r| {
                let shown = r.ok().and_then(|json| evaluated(&json)).and_then(|v| v.as_bool()).unwrap_or(false);
                let detach = serde_json::json!({ "sessionId": s2 }).to_string();
                chrome_cdp("Target.detachFromTarget", &detach, |_| {});
                if !shown {
                    retry(panel);
                }
            });
        });
    });
}

/// The newest DevTools front end among `Target.getTargets`' targets.
fn devtools_target(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v["targetInfos"]
        .as_array()?
        .iter()
        // Each DevTools window is listed twice: as a page (the front end) and as its tab.
        .filter(|t| t["type"] == "page" && t["url"].as_str().is_some_and(|u| u.starts_with("devtools://devtools/")))
        .last()
        .and_then(|t| t["targetId"].as_str().map(str::to_string))
}

/// A DevTools protocol call into an attached session of the chrome webview (the one
/// `chrome_cdp` uses, which owns the session).
fn session_cdp(session: &str, method: &str, params: &str, done: impl FnOnce(Result<String, String>) + 'static) {
    let core = with_host(|h| h.wins.get(&h.target_owner).or_else(|| h.wins.values().next()).map(|w| w.chrome_core.clone())).flatten();
    let Some(c11) = core.and_then(|c| c.cast::<ICoreWebView2_11>().ok()) else {
        return done(Err("browser is not ready".into()));
    };
    let done = RefCell::new(Some(done));
    let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
        if let Some(done) = done.borrow_mut().take() {
            done(result.map(|_| json).map_err(|e| e.message().to_string()));
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) =
            c11.CallDevToolsProtocolMethodForSession(&HSTRING::from(session), &HSTRING::from(method), &HSTRING::from(params), &handler)
        {
            eprintln!("[zenith] {method} failed: {e}");
        }
    }
}

/// Runs in the DevTools front end: once it has drawn its panel tabs (so its own first panel,
/// such as Edge's Welcome tab, doesn't replace ours), opens `panel` with the embedder API
/// Chromium itself uses. False while DevTools is still loading.
const SHOW_PANEL_SCRIPT: &str = r#"async (panel) => {
  const selectedTab = () => {
    const walk = (n) => {
      if (n.matches && n.matches('[role=tab][aria-selected=true]')) return n;
      for (const c of [...(n.shadowRoot ? [n.shadowRoot] : []), ...(n.children || [])]) {
        const hit = walk(c);
        if (hit) return hit;
      }
      return null;
    };
    return walk(document.documentElement);
  };
  if (document.readyState !== 'complete' || !window.DevToolsAPI || !selectedTab()) return false;
  try {
    DevToolsAPI.showPanel(panel);
  } catch (e) {
    return false;
  }
  await new Promise((r) => setTimeout(r, 300));
  return true;
}"#;

// ---------------------------------------------------------------------------------------------
// Scripts

/// Firefox's "Switch Text Direction": the focused text field, else the whole page, flips between
/// left-to-right and right-to-left.
const TEXT_DIRECTION_SCRIPT: &str = r#"(() => {
  let el = document.activeElement;
  while (el && el.shadowRoot && el.shadowRoot.activeElement) el = el.shadowRoot.activeElement;
  const field = el && (el.isContentEditable || el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') ? el : document.documentElement;
  const next = getComputedStyle(field).direction === 'rtl' ? 'ltr' : 'rtl';
  field.dir = next;
  return next;
})()"#;

/// Picture-in-picture for the page's main video (playing first, then the largest); a second
/// press closes it. Answers "on", "off", "none" (no video) or "blocked".
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

/// Reader view: the page's main text, cleaned of everything else, in a readable column. The
/// page is rebuilt in place from its own nodes (no markup strings, so Trusted Types pages work
/// too) and styled with a constructed style sheet (not subject to the page's style CSP). Running
/// it again reloads the original page. Answers "on", "off" or "none" (no article found).
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
  // Narrow a page-wide container (main, #content) down to the element that holds the text.
  for (;;) {
    const inner = [...best.children].find((c) => textOf(c) >= score * 0.8);
    if (!inner) break;
    best = inner;
  }

  const meta = (sel) => doc.querySelector(sel)?.getAttribute('content')?.trim() || '';
  // The article's own heading beats the page title ("Zen" over "Zen - Wikipedia").
  const heading = doc.querySelector('h1')?.textContent?.trim() || '';
  const pageTitle = meta('meta[property="og:title"]') || doc.title.trim();
  const title = heading && heading.length < 200 && pageTitle.includes(heading) ? heading : pageTitle;
  const site = meta('meta[property="og:site_name"]') || location.hostname.replace(/^www\./, '');
  const byline = meta('meta[name="author"]') || doc.querySelector('[rel="author"], .byline, .author')?.textContent?.trim().slice(0, 120) || '';

  const article = best.cloneNode(true);
  article.querySelectorAll('script, style, link, noscript, iframe, form, button, input, select, textarea, nav, aside, footer, svg, canvas, video, audio, object, embed, dialog, [hidden], [aria-hidden="true"], [role="navigation"], [role="complementary"], .ad, .ads, .advert, .share, .social, .related, .comments, .newsletter, .mw-editsection, sup.reference, .navbox, .infobox, .hatnote, .shortdescription').forEach((n) => n.remove());
  // Link lists and navigation boxes: blocks whose text is mostly links.
  for (const el of [...article.querySelectorAll('ul, ol, table, div')]) {
    const text = el.textContent.trim().length;
    if (!text || !el.isConnected) continue;
    const links = [...el.querySelectorAll('a')].reduce((n, a) => n + a.textContent.trim().length, 0);
    if (links / text > 0.6 && el.querySelectorAll('p').length < 2) el.remove();
  }
  const keep = new Set(['P', 'H1', 'H2', 'H3', 'H4', 'H5', 'H6', 'UL', 'OL', 'LI', 'BLOCKQUOTE', 'PRE', 'CODE', 'EM', 'STRONG', 'B', 'I', 'U', 'S', 'A', 'IMG', 'FIGURE', 'FIGCAPTION', 'BR', 'HR', 'TABLE', 'THEAD', 'TBODY', 'TFOOT', 'TR', 'TD', 'TH', 'CAPTION', 'SUP', 'SUB', 'DL', 'DT', 'DD', 'MARK', 'SMALL', 'TIME', 'ABBR', 'Q', 'CITE', 'KBD', 'SAMP', 'VAR', 'DEL', 'INS']);
  const attrs = { A: ['href'], IMG: ['src', 'alt', 'srcset', 'width', 'height'], TD: ['colspan', 'rowspan'], TH: ['colspan', 'rowspan'] };
  for (const el of [...article.querySelectorAll('*')]) {
    if (!keep.has(el.tagName)) {
      el.replaceWith(...el.childNodes);
      continue;
    }
    if (el.tagName === 'IMG') {
      const lazy = el.getAttribute('data-src') || el.getAttribute('data-lazy-src') || el.getAttribute('data-original');
      if (lazy && (!el.getAttribute('src') || el.getAttribute('src').startsWith('data:'))) el.setAttribute('src', lazy);
    }
    const allowed = attrs[el.tagName] || [];
    for (const a of [...el.attributes]) if (!allowed.includes(a.name)) el.removeAttribute(a.name);
    if (el.tagName === 'A') {
      const href = el.getAttribute('href') || '';
      if (/^\s*javascript:/i.test(href)) el.removeAttribute('href');
    }
  }
  const h1 = article.querySelector('h1');
  if (h1 && h1.textContent.trim() === title) h1.remove();

  const words = (article.textContent || '').trim().split(/\s+/).length;
  const make = (tag, cls, text) => {
    const el = doc.createElement(tag);
    if (cls) el.className = cls;
    if (text) el.textContent = text;
    return el;
  };
  const head = doc.createElement('head');
  const viewport = doc.createElement('meta');
  viewport.name = 'viewport';
  viewport.content = 'width=device-width, initial-scale=1';
  head.append(viewport, make('title', '', title));
  const main = make('main', 'zr');
  main.append(make('p', 'zr-site', `${site} · ${Math.max(1, Math.round(words / 230))} min read`), make('h1', 'zr-title', title));
  if (byline) main.append(make('p', 'zr-byline', byline));
  main.append(article);
  const body = doc.createElement('body');
  body.append(main);
  const root = doc.documentElement;
  for (const a of [...root.attributes]) if (a.name !== 'lang' && a.name !== 'dir') root.removeAttribute(a.name);
  root.replaceChildren(head, body);
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(`
    :root { color-scheme: light dark; }
    body { margin: 0; background: #fbfaf7; color: #1f1f1f; font: 20px/1.65 Charter, 'Bitstream Charter', Georgia, 'Times New Roman', serif; }
    @media (prefers-color-scheme: dark) { body { background: #1c1b22; color: #e6e4e0; } .zr a { color: #8ab4f8; } }
    .zr { max-width: 42rem; margin: 0 auto; padding: 56px 24px 120px; overflow-wrap: break-word; }
    .zr-site { margin: 0 0 8px; font: 600 14px/1.4 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif; opacity: 0.65; }
    .zr-title { margin: 0 0 12px; font: 700 2.1em/1.2 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif; }
    .zr-byline { margin: 0 0 32px; font: 15px/1.4 'Segoe UI', system-ui, sans-serif; opacity: 0.7; }
    .zr h2, .zr h3, .zr h4 { font-family: 'Segoe UI Variable Display', 'Segoe UI', system-ui, sans-serif; line-height: 1.3; margin: 1.6em 0 0.5em; }
    .zr img { display: block; max-width: 100%; height: auto; margin: 1.2em auto; border-radius: 6px; }
    .zr figure { margin: 1.5em 0; }
    .zr figcaption { font-size: 0.8em; opacity: 0.7; text-align: center; }
    .zr a { color: #0b57d0; }
    .zr blockquote { margin: 1.2em 0; padding-left: 1em; border-left: 3px solid rgba(127, 127, 127, 0.45); }
    .zr pre { overflow: auto; padding: 12px 16px; border-radius: 8px; background: rgba(127, 127, 127, 0.12); font-size: 0.8em; line-height: 1.5; }
    .zr code, .zr kbd, .zr samp { font-family: 'Cascadia Mono', Consolas, monospace; font-size: 0.9em; }
    .zr table { border-collapse: collapse; display: block; overflow: auto; font-size: 0.85em; }
    .zr td, .zr th { border: 1px solid rgba(127, 127, 127, 0.3); padding: 4px 8px; }
    .zr hr { border: 0; border-top: 1px solid rgba(127, 127, 127, 0.3); margin: 2em 0; }
  `);
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

    #[test]
    fn the_newest_devtools_front_end_is_picked() {
        let json = r#"{"targetInfos":[
            {"targetId":"a","type":"page","url":"https://example.com/"},
            {"targetId":"b","type":"other","url":"devtools://devtools/bundled/devtools_app.html?x"},
            {"targetId":"c","type":"page","url":"devtools://devtools/bundled/devtools_app.html?y"},
            {"targetId":"d","type":"tab","url":"devtools://devtools/bundled/devtools_app.html?y"}
        ]}"#;
        assert_eq!(devtools_target(json).as_deref(), Some("c"));
        assert_eq!(devtools_target(r#"{"targetInfos":[]}"#), None);
    }

    #[test]
    fn evaluate_results_and_exceptions() {
        assert_eq!(evaluated(r#"{"result":{"type":"string","value":"on"}}"#), Some(serde_json::json!("on")));
        assert_eq!(evaluated(r#"{"result":{"type":"object"},"exceptionDetails":{}}"#), None);
    }
}
