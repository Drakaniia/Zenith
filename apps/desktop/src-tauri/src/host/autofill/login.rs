//! Login forms: the saved-account list under a login field, offers to save a submitted login
//! (including two-step logins) and fill-on-load.

use std::rc::Rc;
use std::time::{Duration, Instant};

use serde::Serialize;
use zeroize::Zeroizing;

use super::{close_suggestions, fill, with_tab, FieldRect, Msg, Suggest};
use crate::host::tab::TabShared;
use crate::host::{config, emit_tab, post};
use crate::vault::{self, crypto, Account, Capture};

const STEP_TTL: Duration = Duration::from_secs(5 * 60);

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

/// The accounts whose username (or label) contains what was typed, ignoring case; all of them when
/// nothing is typed.
pub(super) fn matching_accounts(accounts: Vec<Account>, typed: &str) -> Vec<Account> {
    let q = typed.trim().to_lowercase();
    if q.is_empty() {
        return accounts;
    }
    accounts
        .into_iter()
        .filter(|a| a.username.to_lowercase().contains(&q) || a.label.to_lowercase().contains(&q))
        .collect()
}

pub(super) fn on_field(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
    with_tab(&sh.id, |t| t.muted = false);
    show_list(sh, origin, m, None);
}

/// A username field changed: the open list narrows to the matching accounts, and comes back when
/// the text matches again. Nothing when the user closed the list.
pub(super) fn on_type(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
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

pub(super) fn on_capture(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
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

pub(super) fn on_form(sh: &Rc<TabShared>, origin: &str) {
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
