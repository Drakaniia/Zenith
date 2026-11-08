//! Tab sleeping (capture the scroll position, then free the webview) and content measuring.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use webview2_com::*;
use windows::core::HSTRING;

use super::close;
use crate::host::{app, post, with_host};

type SleepDone = Box<dyn FnOnce(Option<(f64, f64)>)>;

thread_local! {
    static PENDING_SLEEPS: RefCell<std::collections::HashMap<u64, SleepDone>> = RefCell::new(Default::default());
}
static NEXT_SLEEP: AtomicU64 = AtomicU64::new(1);

/// Completes a sleep request exactly once: whichever of the script result, a script error or
/// the timeout arrives first closes the webview and answers the caller.
fn finish_sleep(token: u64, id: &str, scroll: Option<(f64, f64)>) {
    let done = PENDING_SLEEPS.with(|p| p.borrow_mut().remove(&token));
    if let Some(done) = done {
        close(id);
        done(scroll);
    }
}

/// Captures the scroll position, then frees the webview (tab sleeping). A crashed or hung
/// renderer never answers the script, so the webview is freed after 1.5 s regardless.
pub fn sleep(id: &str, done: impl FnOnce(Option<(f64, f64)>) + 'static) {
    let core = with_host(|h| h.tabs.get(id).map(|t| t.core())).flatten();
    let Some(core) = core else {
        done(None);
        return;
    };
    let token = NEXT_SLEEP.fetch_add(1, Ordering::Relaxed);
    PENDING_SLEEPS.with(|p| p.borrow_mut().insert(token, Box::new(done)));
    let id = id.to_string();
    {
        let id = id.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let _ = app().run_on_main_thread(move || finish_sleep(token, &id, None));
        });
    }
    let script_id = id.clone();
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |_, json| {
        let scroll = serde_json::from_str::<Vec<f64>>(&json)
            .ok()
            .filter(|v| v.len() == 2)
            .map(|v| (v[0], v[1]));
        post(move || finish_sleep(token, &script_id, scroll));
        Ok(())
    }));
    unsafe {
        if core.ExecuteScript(&HSTRING::from("[window.scrollX, window.scrollY]"), &handler).is_err() {
            post(move || finish_sleep(token, &id, None));
        }
    }
}

/// The natural size of a page's content (CSS px), laid out shrink-to-fit the way Chrome sizes
/// an extension's toolbar popup. None when the page cannot run the script.
pub fn content_size(id: &str, done: impl FnOnce(Option<(f64, f64)>) + 'static) {
    const SCRIPT: &str = r#"(() => {
  const d = document.documentElement, b = document.body;
  if (!b) return null;
  const saved = d.style.cssText;
  d.style.width = 'max-content';
  d.style.height = 'max-content';
  const r = d.getBoundingClientRect(), br = b.getBoundingClientRect();
  const w = Math.max(r.width, br.width, b.scrollWidth), h = Math.max(r.height, br.height, b.scrollHeight);
  d.style.cssText = saved;
  return [Math.ceil(w), Math.ceil(h)];
})()"#;
    let Some(core) = with_host(|h| h.tabs.get(id).map(|t| t.core())).flatten() else {
        return done(None);
    };
    let done = RefCell::new(Some(done));
    let handler = ExecuteScriptCompletedHandler::create(Box::new(move |result, json| {
        let size = result
            .ok()
            .and_then(|_| serde_json::from_str::<Vec<f64>>(&json).ok())
            .filter(|v| v.len() == 2)
            .map(|v| (v[0], v[1]));
        if let Some(done) = done.borrow_mut().take() {
            done(size);
        }
        Ok(())
    }));
    unsafe {
        if let Err(e) = core.ExecuteScript(&HSTRING::from(SCRIPT), &handler) {
            eprintln!("[zenith] content size script failed: {e}");
        }
    }
}
