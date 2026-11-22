//! Tab sleeping (capture scroll position via CEF DevTools `Runtime.evaluate`, then close the
//! browser) and extension popup content measuring.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::close;
use crate::app::runtime::post_ui_delayed;
use crate::host::{eval_js, with_host};

type SleepDone = Box<dyn FnOnce(Option<(f64, f64)>)>;

thread_local! {
    static PENDING_SLEEPS: RefCell<HashMap<u64, SleepDone>> = RefCell::new(HashMap::new());
}
static NEXT_SLEEP: AtomicU64 = AtomicU64::new(1);

fn finish_sleep(token: u64, id: &str, scroll: Option<(f64, f64)>) {
    let done = PENDING_SLEEPS.with(|p| p.borrow_mut().remove(&token));
    if let Some(done) = done {
        close(id);
        done(scroll);
    }
}

/// Captures `[window.scrollX, window.scrollY]`, then closes the browser (or after 1.5 s if the
/// renderer is hung or crashed).
pub fn sleep(id: &str, done: impl FnOnce(Option<(f64, f64)>) + 'static) {
    let browser = with_host(|h| h.tabs.get(id).and_then(|t| t.browser.clone())).flatten();
    let Some(browser) = browser else {
        close(id);
        done(None);
        return;
    };
    let token = NEXT_SLEEP.fetch_add(1, Ordering::Relaxed);
    PENDING_SLEEPS.with(|p| p.borrow_mut().insert(token, Box::new(done)));
    let id_timeout = id.to_string();
    post_ui_delayed(1500, move || finish_sleep(token, &id_timeout, None));

    let id_eval = id.to_string();
    eval_js(&browser, "[window.scrollX, window.scrollY]", false, false, move |res| {
        let scroll = res.ok().and_then(|v| {
            let arr = v.as_array()?;
            if arr.len() == 2 {
                Some((arr[0].as_f64()?, arr[1].as_f64()?))
            } else {
                None
            }
        });
        finish_sleep(token, &id_eval, scroll);
    });
}

/// Measures natural content size (CSS px) for shrink-to-fit extension toolbar popups.
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
    let Some(browser) = with_host(|h| h.tabs.get(id).and_then(|t| t.browser.clone())).flatten() else {
        return done(None);
    };
    eval_js(&browser, SCRIPT, false, false, move |res| {
        let size = res.ok().and_then(|v| {
            let arr = v.as_array()?;
            if arr.len() == 2 {
                Some((arr[0].as_f64()?, arr[1].as_f64()?))
            } else {
                None
            }
        });
        done(size);
    });
}
