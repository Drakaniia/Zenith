//! Extensions in incognito windows.
//!
//! Incognito tabs live in their own in-memory WebView2 profile, which starts without any
//! extension. `sync` loads into it the user's enabled extensions that are allowed in incognito
//! (the global setting plus the per-extension switch) and unloads the ones that no longer are.
//! The profile keeps nothing on disk, so extension storage and cookies seen from incognito
//! pages are separate from the normal profile's and are discarded with the last incognito tab.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use windows::core::HSTRING;

use super::*;

/// When the running sync started (0: none). A sync whose callbacks never arrive (the profile
/// went away with the last incognito tab) is abandoned after `STALE_MS`.
static BUSY_SINCE: AtomicU64 = AtomicU64::new(0);
static AGAIN: AtomicBool = AtomicBool::new(false);
const STALE_MS: u64 = 10_000;

/// Extension id -> folder it was loaded from into the incognito profile.
static LOADED: LazyLock<Mutex<HashMap<String, PathBuf>>> = LazyLock::new(Default::default);

/// Brings the incognito profile's extensions in line with the settings. Callable from any
/// thread; the work runs on the UI thread. Does nothing while no incognito tab exists.
pub fn sync() {
    let _ = crate::host::app().run_on_main_thread(sync_now);
}

fn sync_now() {
    let now = crate::storage::now_ms().max(1) as u64;
    let since = BUSY_SINCE.load(Ordering::SeqCst);
    if since != 0 && now.saturating_sub(since) < STALE_MS {
        AGAIN.store(true, Ordering::SeqCst);
        return;
    }
    BUSY_SINCE.store(now, Ordering::SeqCst);
    run(Box::new(|| {
        BUSY_SINCE.store(0, Ordering::SeqCst);
        if AGAIN.swap(false, Ordering::SeqCst) {
            sync_now();
        }
    }));
}

fn private_profile() -> Option<ICoreWebView2Profile7> {
    with_host(|h| h.page_profile(true))
        .flatten()
        .and_then(|p| p.cast::<ICoreWebView2Profile7>().ok())
}

fn loaded() -> std::sync::MutexGuard<'static, HashMap<String, PathBuf>> {
    LOADED.lock().unwrap_or_else(|e| e.into_inner())
}

fn run(finish: Box<dyn FnOnce()>) {
    let (Some(private), Ok(main)) = (private_profile(), profile()) else {
        return finish();
    };
    list_in(main, move |main_list| {
        let Ok(main_list) = main_list else { return finish() };
        let registry = load_registry();
        let allowed = crate::host::config().extensions_in_incognito;
        let mut want: HashMap<String, PathBuf> = HashMap::new();
        let mut builtin: HashSet<String> = HashSet::new();
        for ext in &main_list {
            let (id, _, enabled) = ext_fields(ext);
            match registry.folders.get(&id) {
                Some(dir) if allowed && enabled && !registry.incognito_off.contains(&id) => {
                    want.insert(id, dir.clone());
                }
                Some(_) => {}
                None => {
                    builtin.insert(id);
                }
            }
        }
        let profile = private.clone();
        list_in(private, move |have| {
            let Ok(have) = have else { return finish() };
            apply(&profile, have, want, builtin, finish);
        });
    });
}

fn apply(
    profile: &ICoreWebView2Profile7,
    have: Vec<ICoreWebView2BrowserExtension>,
    mut want: HashMap<String, PathBuf>,
    builtin: HashSet<String>,
    finish: Box<dyn FnOnce()>,
) {
    // Counts the jobs still running; the extra 1 is released once all are started.
    let left = Rc::new(Cell::new(1usize));
    let finish = Rc::new(RefCell::new(Some(finish)));
    let tick: Rc<dyn Fn()> = {
        let (left, finish) = (left.clone(), finish.clone());
        Rc::new(move || {
            left.set(left.get().saturating_sub(1));
            if left.get() == 0 {
                if let Some(f) = finish.borrow_mut().take() {
                    f();
                }
            }
        })
    };
    let start = |left: &Rc<Cell<usize>>| left.set(left.get() + 1);

    for ext in have {
        let (id, _, _) = ext_fields(&ext);
        if builtin.contains(&id) {
            want.remove(&id);
            continue;
        }
        match want.remove(&id) {
            // Still wanted from the same folder: nothing to do.
            Some(dir) if loaded().get(&id) == Some(&dir) => {}
            // Wanted from a new folder (the extension was updated): load the new copy.
            Some(dir) => {
                start(&left);
                let (p, t, id2) = (profile.clone(), tick.clone(), id.clone());
                let next: Rc<dyn Fn()> = Rc::new(move || add(&p, id2.clone(), dir.clone(), t.clone()));
                remove(&ext, id, next);
            }
            // No longer wanted.
            None => {
                start(&left);
                remove(&ext, id, tick.clone());
            }
        }
    }
    for (id, dir) in want {
        start(&left);
        add(profile, id, dir, tick.clone());
    }
    tick();
}

fn add(profile: &ICoreWebView2Profile7, id: String, dir: PathBuf, done: Rc<dyn Fn()>) {
    let (done2, dir2) = (done.clone(), dir.clone());
    let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |result, _| {
        match result {
            Ok(()) => {
                loaded().insert(id.clone(), dir2.clone());
            }
            Err(e) => eprintln!("[zenith] could not load extension {id} in incognito: {}", e.message()),
        }
        done2();
        Ok(())
    }));
    unsafe {
        if profile.AddBrowserExtension(&HSTRING::from(dir.as_os_str()), &handler).is_err() {
            done();
        }
    }
}

fn remove(ext: &ICoreWebView2BrowserExtension, id: String, done: Rc<dyn Fn()>) {
    let done2 = done.clone();
    let handler = BrowserExtensionRemoveCompletedHandler::create(Box::new(move |_| {
        loaded().remove(&id);
        done2();
        Ok(())
    }));
    unsafe {
        if ext.Remove(&handler).is_err() {
            done();
        }
    }
}
