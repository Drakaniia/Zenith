// Backend events of the password manager and wallet (its commands are in vault.svelte.ts).
import type { CardSaveOffer } from "./cards/cards";
import type { PasswordSaveOffer, PasswordSuggest, VaultStatus } from "./passwords";

declare module "../../platform/ipc" {
  interface BackendEvents {
    "vault-status": VaultStatus;
    "password-suggest": PasswordSuggest;
    "password-suggest-close": { tabId: string };
    "password-save-offer": PasswordSaveOffer;
    "card-save-offer": CardSaveOffer;
  }
}
