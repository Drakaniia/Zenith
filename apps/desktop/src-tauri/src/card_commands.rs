//! Tauri commands for the wallet (saved payment cards). See `vault::cards`.
//!
//! Cards are stricter than passwords: revealing, copying and filling a card always needs the master
//! password typed again, even when the vault is unlocked. A card number reaches the chrome UI only
//! when the user reveals it in the manager; copying and filling happen here, in Rust.

use tauri::AppHandle;
use zeroize::Zeroizing;

use crate::host::{self, autofill};
use crate::password_commands::{blocking, on_main, on_main_cb, secret, vault_call, CmdResult, CLIPBOARD_CLEAR};
use crate::platform::win;
use crate::vault;

/// The master password, typed for this action. The UI answers `NEED_MASTER` by asking for it.
fn fresh_master(v: &mut vault::Vault, master: Option<&Zeroizing<String>>) -> CmdResult<()> {
    match master {
        Some(m) => v.verify(m),
        None => Err(vault::NEED_MASTER.into()),
    }
}

#[tauri::command]
pub async fn cards_list() -> CmdResult<Vec<vault::cards::CardInfo>> {
    vault_call(|v| v.cards_list()).await
}

#[tauri::command]
pub async fn card_reveal(id: String, master: Option<String>) -> CmdResult<vault::cards::CardRevealed> {
    let master = secret(master);
    let r = vault_call(move |v| {
        fresh_master(v, master.as_ref())?;
        v.card_reveal(&id)
    })
    .await;
    vault::broadcast();
    r
}

/// Copies the cardholder name, or the number or security code (kept out of clipboard history and
/// cleared after 30 seconds).
#[tauri::command]
pub async fn card_copy(id: String, field: String, master: Option<String>) -> CmdResult<()> {
    let master = secret(master);
    let r = vault_call(move |v| {
        if field == "holder" {
            let holder = v.cards_list()?.into_iter().find(|c| c.id == id).map(|c| c.holder).ok_or("That card no longer exists.")?;
            return win::set_clipboard_text(&holder);
        }
        fresh_master(v, master.as_ref())?;
        let revealed = v.card_reveal(&id)?;
        match field.as_str() {
            "number" => win::copy_secret(&revealed.number, CLIPBOARD_CLEAR),
            "cvc" if !revealed.cvc.is_empty() => win::copy_secret(&revealed.cvc, CLIPBOARD_CLEAR),
            "cvc" => Err("No security code is saved for this card.".into()),
            _ => Err("unknown field".into()),
        }
    })
    .await;
    vault::broadcast();
    r
}

#[tauri::command]
pub async fn card_add(card: vault::cards::CardInput) -> CmdResult<String> {
    let r = vault_call(move |v| v.card_add(&card)).await;
    vault::broadcast();
    r
}

#[tauri::command]
pub async fn card_update(id: String, card: vault::cards::CardInput) -> CmdResult<()> {
    let r = vault_call(move |v| v.card_update(&id, &card)).await;
    vault::broadcast();
    r
}

#[tauri::command]
pub async fn cards_delete(ids: Vec<String>) -> CmdResult<usize> {
    let r = vault_call(move |v| v.cards_delete(&ids)).await;
    vault::broadcast();
    r
}

/// Answers a "Save card?" prompt: "save", "never" (for this site) or "dismiss". Saving may need the
/// master password; on any error the offer stays so the user can try again. The security code is
/// only kept when `save_cvc` is set.
#[tauri::command]
pub async fn card_offer_respond(offer_id: u64, action: String, save_cvc: Option<bool>, master: Option<String>) -> CmdResult<Option<String>> {
    let master = secret(master);
    match action.as_str() {
        "dismiss" => {
            vault::cards::take_card_offer(offer_id);
            Ok(None)
        }
        "never" => {
            let Some(offer) = vault::cards::take_card_offer(offer_id) else { return Ok(None) };
            vault_call(move |v| v.set_never(&vault::never_key(&offer.origin), true)).await?;
            Ok(None)
        }
        "save" => {
            let offer = vault::cards::take_card_offer(offer_id).ok_or("This card is no longer waiting to be saved.")?;
            let save_cvc = save_cvc.unwrap_or(false);
            let result = blocking(move || {
                let r = vault::with(|v| {
                    if !v.exists() {
                        return Err("Set up a master password first.".to_string());
                    }
                    if let Some(m) = &master {
                        v.unlock(m)?;
                    }
                    if !v.unlocked() {
                        return Err(vault::NEED_MASTER.to_string());
                    }
                    v.save_card(&offer, save_cvc)
                });
                if r.is_err() {
                    vault::cards::restore_card_offer(offer_id, offer);
                }
                r
            })
            .await?;
            vault::broadcast();
            result.map(Some)
        }
        _ => Err("unknown action".into()),
    }
}

/// Fills a saved card into the tab's payment form, after checking the page is HTTPS and the master
/// password was typed for this fill.
#[tauri::command]
pub async fn autofill_fill_card(app: AppHandle, tab_id: String, card_id: String, master: Option<String>) -> CmdResult<()> {
    let cfg = host::config();
    if !cfg.passwords_enabled || !cfg.autofill_cards {
        return Err("Filling cards is turned off.".into());
    }
    let master = secret(master).ok_or_else(|| vault::NEED_MASTER.to_string())?;
    let tab = tab_id.clone();
    let origin = on_main(&app, move || autofill::tab_origin(&tab)).await?.ok_or("This page can't be filled.")?;
    if !origin.starts_with("https://") {
        return Err("Cards are only filled on secure (https) pages.".into());
    }
    let card = vault_call(move |v| {
        v.verify(&master)?;
        v.card_for_fill(&card_id)
    })
    .await;
    vault::broadcast();
    let card = card?;
    on_main_cb(&app, move |done| {
        autofill::close_suggestions(&tab_id);
        autofill::fill_card(&tab_id, &origin, &card, move |r| done(r));
    })
    .await?
}
