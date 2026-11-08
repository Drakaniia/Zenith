// Installed extensions, install offers from pages, Web Store updates and pinned toolbar icons.
import { api } from "$platform/ipc";
import { browser } from "$features/browser/browser.svelte";
import { settings } from "$features/settings/settings.svelte";
import { ui } from "$shared/ui/ui.svelte";
import type { ExtensionInfo, ExtensionUpdate, InstallEvent, InstallOffer } from "./types";
import type { PopupAnchor } from "$features/browser/types";

export const WEB_STORE_URL = "https://chromewebstore.google.com/category/extensions";

export function storeUrl(storeId: string) {
  return `https://chromewebstore.google.com/detail/${storeId}`;
}

export type PendingOffer = InstallOffer & { source: string; tabId: string };

class ExtensionStore {
  items = $state<ExtensionInfo[]>([]);
  loading = $state(true);
  error = $state("");
  updates = $state<ExtensionUpdate[]>([]);
  checking = $state(false);
  updating = $state<string[]>([]);
  offers = $state<PendingOffer[]>([]);
  /** The toolbar's extensions popover, anchored at a point. */
  popover = $state<PopupAnchor | null>(null);
  /** When the popover last closed (not reactive): lets its toolbar button tell a closing
   *  click from an opening one. */
  private closedAt = 0;

  user = $derived(this.items.filter((e) => !e.builtin));
  builtin = $derived(this.items.filter((e) => e.builtin));
  pinned = $derived(
    settings.value.pinnedExtensions
      .map((id) => this.items.find((e) => e.id === id && !e.builtin))
      .filter((e): e is ExtensionInfo => !!e && e.enabled),
  );

  async refresh() {
    try {
      this.items = await api.extList();
      this.error = "";
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  isPinned(id: string) {
    return settings.value.pinnedExtensions.includes(id);
  }

  togglePin(id: string) {
    const list = settings.value.pinnedExtensions;
    settings.update({
      pinnedExtensions: list.includes(id)
        ? list.filter((x) => x !== id)
        : [...list, id],
    });
  }

  close() {
    if (!this.popover) return;
    this.popover = null;
    this.closedAt = performance.now();
  }

  justClosed() {
    return performance.now() - this.closedAt < 350;
  }

  /** Opens what the extension shows from the toolbar: its popup (a small card anchored at the
   *  toolbar, like Chrome's), else its options page, else the extensions page. */
  open(ext: ExtensionInfo, anchor?: PopupAnchor) {
    this.close();
    if (ext.popupUrl) browser.openGlance(ext.popupUrl, browser.activeTab?.id, anchor);
    else if (ext.optionsUrl) browser.openTab(ext.optionsUrl);
    else browser.openTab("zenith://extensions");
  }

  openStore(storeId?: string | null) {
    this.close();
    browser.openTab(storeId ? storeUrl(storeId) : WEB_STORE_URL);
  }

  // ------------------------------------------------------------------ offers

  handleEvent(e: InstallEvent) {
    if (e.state === "fetching") {
      ui.toast(`Getting extension from ${e.source}…`, { icon: "puzzle" });
    } else if (e.state === "failed") {
      ui.toast(e.error || "The extension could not be installed", {
        icon: "alert",
        timeout: 8000,
      });
    } else if (e.offer) {
      this.offers = [
        ...this.offers,
        { ...e.offer, source: e.source, tabId: e.tabId },
      ];
    }
  }

  async accept(offer: PendingOffer) {
    this.offers = this.offers.filter((o) => o.token !== offer.token);
    try {
      const ext = await api.extOfferAccept(offer.token);
      await this.refresh();
      this.updates = this.updates.filter((u) => u.id !== ext.id);
      ui.toast(`Added ${ext.name}`, {
        icon: "puzzle",
        timeout: 6000,
        action:
          ext.popupUrl && !this.isPinned(ext.id)
            ? { label: "Pin", run: () => this.togglePin(ext.id) }
            : undefined,
      });
    } catch (e) {
      ui.toast(String(e), { icon: "alert", timeout: 8000 });
    }
  }

  decline(offer: PendingOffer) {
    this.offers = this.offers.filter((o) => o.token !== offer.token);
    void api.extOfferDiscard(offer.token).catch(() => {});
  }

  // ----------------------------------------------------------------- updates

  async checkUpdates(silent = false) {
    if (this.checking) return;
    this.checking = true;
    try {
      this.updates = await api.extCheckUpdates();
      if (silent && this.updates.length) {
        const n = this.updates.length;
        ui.toast(
          n === 1
            ? `An update is available for ${this.updates[0].name}`
            : `${n} extension updates are available`,
          {
            icon: "puzzle",
            timeout: 10000,
            action: { label: "Update", run: () => void this.updateAll() },
          },
        );
      } else if (!silent && !this.updates.length) {
        ui.toast("Your extensions are up to date", { icon: "check" });
      }
    } catch (e) {
      if (!silent) ui.toast(String(e), { icon: "alert", timeout: 7000 });
    } finally {
      this.checking = false;
    }
  }

  async update(u: ExtensionUpdate) {
    if (this.updating.includes(u.id)) return;
    this.updating = [...this.updating, u.id];
    try {
      const ext = await api.extUpdate(u.id);
      this.updates = this.updates.filter((x) => x.id !== u.id);
      // Reinstalling keeps the Chrome id, so pins stay; follow the id if it changed anyway.
      if (ext.id !== u.id && this.isPinned(u.id)) {
        settings.update({
          pinnedExtensions: settings.value.pinnedExtensions.map((x) =>
            x === u.id ? ext.id : x,
          ),
        });
      }
      ui.toast(`Updated ${ext.name} to ${ext.version}`, { icon: "check" });
    } catch (e) {
      ui.toast(`${u.name}: ${e}`, { icon: "alert", timeout: 7000 });
    } finally {
      this.updating = this.updating.filter((x) => x !== u.id);
      await this.refresh();
    }
  }

  async updateAll() {
    for (const u of [...this.updates]) await this.update(u);
  }

  updateFor(id: string) {
    return this.updates.find((u) => u.id === id);
  }
}

export const extensions = new ExtensionStore();
