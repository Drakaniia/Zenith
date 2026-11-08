//! Payment forms: the saved-card list under a card field and offers to save a submitted card.

use std::rc::Rc;
use std::time::Duration;

use serde::Serialize;
use zeroize::Zeroizing;

use super::{with_tab, FieldRect, Msg, Suggest};
use crate::host::tab::TabShared;
use crate::host::{config, emit_tab};
use crate::vault::{self, cards};

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

/// A card number, name or expiry field was clicked: the saved cards are listed under it. Cards
/// are only offered on HTTPS pages.
pub(super) fn on_card_field(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
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
pub(super) fn on_card_capture(sh: &Rc<TabShared>, origin: &str, m: &Msg) {
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
