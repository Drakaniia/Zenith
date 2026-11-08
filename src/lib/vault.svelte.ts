// Password manager state in the UI: lock status, the account list under a login field and the
// "Save password?" prompt. The backend owns every secret; the UI sees a password only when the
// user reveals one in the manager.
import { invoke } from "@tauri-apps/api/core";
import type {
  EntryInput,
  ImportOpened,
  ImportSummary,
  PasswordEntry,
  PasswordSaveOffer,
  PasswordSuggest,
  VaultStatus,
} from "./passwords";

export const vaultApi = {
  status: () => invoke<VaultStatus>("vault_status"),
  setup: (master: string) => invoke<string>("vault_setup", { master }),
  unlock: (master: string) => invoke<VaultStatus>("vault_unlock", { master }),
  lock: () => invoke<void>("vault_lock"),
  changeMaster: (current: string, next: string) =>
    invoke<void>("vault_change_master", { current, new: next }),
  recover: (recoveryKey: string, newMaster: string) =>
    invoke<string>("vault_recover", { recoveryKey, newMaster }),
  newRecoveryKey: (master: string) =>
    invoke<string>("vault_new_recovery_key", { master }),
  reset: (confirm: string) => invoke<void>("vault_reset", { confirm }),

  list: () => invoke<PasswordEntry[]>("passwords_list"),
  reveal: (id: string, master: string | null = null) =>
    invoke<{ password: string; notes: string }>("password_reveal", { id, master }),
  copy: (id: string, field: "username" | "password", master: string | null = null) =>
    invoke<void>("password_copy", { id, field, master }),
  add: (entry: EntryInput) => invoke<string>("password_add", { entry }),
  update: (id: string, entry: EntryInput) =>
    invoke<void>("password_update", { id, entry }),
  remove: (ids: string[]) => invoke<number>("passwords_delete", { ids }),
  generate: (length = 20) => invoke<string>("password_generate", { length }),
  neverList: () => invoke<string[]>("passwords_never_list"),
  neverRemove: (site: string) => invoke<void>("passwords_never_remove", { site }),

  exportCsv: (master: string) =>
    invoke<string | null>("passwords_export_csv", { master }),
  backup: (master: string) => invoke<string | null>("passwords_backup", { master }),
  importOpen: () => invoke<ImportOpened | null>("passwords_import_open"),
  importUnlock: (token: number, password: string) =>
    invoke<ImportOpened>("passwords_import_unlock", { token, password }),
  importCommit: (token: number, choices: Record<number, string>) =>
    invoke<ImportSummary>("passwords_import_commit", { token, choices }),
  importCancel: (token: number) => invoke<void>("passwords_import_cancel", { token }),

  fill: (tabId: string, entryId: string, master: string | null = null) =>
    invoke<void>("autofill_fill", { tabId, entryId, master }),
  fillGenerated: (tabId: string) => invoke<void>("autofill_fill_generated", { tabId }),
  dismiss: (tabId: string) => invoke<void>("autofill_dismiss", { tabId }),
  respondOffer: (offerId: number, action: "save" | "never" | "dismiss", master: string | null = null) =>
    invoke<string | null>("password_offer_respond", { offerId, action, master }),
};

const EMPTY: VaultStatus = {
  exists: false,
  unlocked: false,
  hasRecovery: false,
  count: 0,
  damaged: null,
  retryAfter: 0,
};

class VaultStore {
  status = $state<VaultStatus>({ ...EMPTY });
  loaded = $state(false);
  /** The account list under a login field. */
  suggest = $state<PasswordSuggest | null>(null);
  /** The list is showing its master password prompt; a fresh list from the page must not
   *  replace it (that would reset the prompt under the user's hands). */
  unlocking = false;
  /** The tab whose list the user just closed, and when: the page re-offers the list as soon as
   *  focus returns to its field, which would make the list impossible to dismiss. */
  #closed: { tabId: string; at: number } | null = null;
  /** "Save password?" prompts, newest last (one per tab). */
  offers = $state<PasswordSaveOffer[]>([]);
  /** Bumped when entries change, so open lists reload. */
  revision = $state(0);

  async refresh() {
    try {
      this.status = await vaultApi.status();
    } catch {
      /* the preview has no backend */
    }
    this.loaded = true;
  }

  apply(s: VaultStatus) {
    const changed = s.count !== this.status.count || s.unlocked !== this.status.unlocked;
    this.status = s;
    this.loaded = true;
    if (changed) this.revision++;
  }

  async lock() {
    await vaultApi.lock();
    await this.refresh();
  }

  pushOffer(o: PasswordSaveOffer) {
    this.offers = [...this.offers.filter((x) => x.tabId !== o.tabId), o];
  }

  dropOffer(offerId: number) {
    this.offers = this.offers.filter((x) => x.offerId !== offerId);
  }

  dropTab(tabId: string) {
    this.offers = this.offers.filter((x) => x.tabId !== tabId);
    if (this.suggest?.tabId === tabId) this.suggest = null;
  }

  /** A list offered by the page. Ignored while the user is typing the master password for the
   *  open list, and for a moment after the user closed the list of that tab. */
  offerSuggest(s: PasswordSuggest) {
    if (this.unlocking && this.suggest?.tabId === s.tabId) return;
    const c = this.#closed;
    if (c && c.tabId === s.tabId && Date.now() - c.at < 1200) return;
    this.suggest = s;
  }

  /** `notify`: the user closed it (the page is told, and does not get to reopen it at once). */
  closeSuggest(notify = true) {
    const s = this.suggest;
    this.suggest = null;
    this.unlocking = false;
    if (s && notify) {
      this.#closed = { tabId: s.tabId, at: Date.now() };
      void vaultApi.dismiss(s.tabId).catch(() => {});
    }
  }
}

export const vault = new VaultStore();
