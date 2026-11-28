//! Tauri commands for saved addresses. See `vault::addresses` and `host::autofill::address`.

use crate::app::command::{on_main_cb, CmdResult};
use crate::app::runtime::AppHandle;
use crate::host::{self, autofill};
use crate::vault::{self, addresses::Address};

use super::commands::vault_call;

pub async fn addresses_list() -> CmdResult<Vec<Address>> {
    vault_call(|v| v.addresses_list()).await
}

pub async fn address_add(address: Address) -> CmdResult<String> {
    let r = vault_call(move |v| v.address_add(&address)).await;
    vault::broadcast();
    r
}

pub async fn address_update(id: String, address: Address) -> CmdResult<()> {
    let r = vault_call(move |v| v.address_update(&id, &address)).await;
    vault::broadcast();
    r
}

pub async fn addresses_delete(ids: Vec<String>) -> CmdResult<usize> {
    let r = vault_call(move |v| v.addresses_delete(&ids)).await;
    vault::broadcast();
    r
}

/// Fills a saved address into the form whose field the user clicked (no master password needed:
/// addresses are readable while the vault is locked).
pub async fn autofill_fill_address(app: AppHandle, tab_id: String, address_id: String) -> CmdResult<()> {
    if !host::config().passwords_enabled {
        return Err("Autofill is turned off.".into());
    }
    let address = vault_call(move |v| Ok(v.address(&address_id))).await?.ok_or("That address no longer exists.")?;
    on_main_cb(&app, move |done| {
        autofill::close_suggestions(&tab_id);
        autofill::fill_address(&tab_id, &address, move |r| done(r));
    })
    .await?
}
