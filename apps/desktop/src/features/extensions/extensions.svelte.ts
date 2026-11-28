// Installed extensions, install offers from pages, Web Store updates, toolbar icons and popups.
import { extensionsApi } from "./api";
import { browser, needsWebview } from "$features/browser/browser.svelte";
import { tabsApi } from "$features/browser/api";
import { settings } from "$features/settings/settings.svelte";
import { ui } from "$shared/ui/ui.svelte";
import { incognito } from "$features/window/window.svelte";
import { focusAfterLayout, scheduleLayout } from "$platform/layout.svelte";
import type {
  ActionBadge,
  ActiveExtPopup,
  BadgeEvent,
  ExtensionInfo,
  ExtensionUpdate,
  InstallEvent,
  InstallOffer,
  NativeHostInfo,
} from "./types";
import type { PopupAnchor, TabPatch } from "$features/browser/types";

export const WEB_STORE_URL = "https://chromewebstore.google.com/category/extensions";

export function storeUrl(storeId: string) {
  return `https://chromewebstore.google.com/detail/${storeId}`;
}

export type PendingOffer = InstallOffer & { source: string; tabId: string };

/** Extensions can read the pages of incognito windows they run in: the user confirms. */
function confirmIncognito(name?: string) {
  return window.confirm(
    `${name ?? "Extensions"} will be able to see what you browse in incognito windows. Only allow extensions you trust. Allow?`,
  );
}

class ExtensionStore {
  items = $state<ExtensionInfo[]>([]);
  loading = $state(true);
  error = $state("");
  updates = $state<ExtensionUpdate[]>([]);
  checking = $state(false);
  updating = $state<string[]>([]);
  offers = $state<PendingOffer[]>([]);
  /** Action badge states keyed by `<extId>` (global) or `<extId>:<tabId>` (per-tab). */
  badges = $state<Record<string, ActionBadge>>({});
  /** Discovered Windows native messaging hosts and their per-profile consent state. */
  nativeHosts = $state<NativeHostInfo[]>([]);
  /** The toolbar's extensions popover, anchored at a point. */
  popover = $state<PopupAnchor | null>(null);
  /** Dedicated extension toolbar popup (independent from Glance). */
  activePopup = $state<ActiveExtPopup | null>(null);
  /** When the popover last closed (not reactive): lets its toolbar button tell a closing
   *  click from an opening one. */
  private closedAt = 0;
  private popupOpenedAt = 0;
  private popupSeq = 0;

  user = $derived(this.items.filter((e) => !e.builtin));
  /** The extensions this window runs: all of them in a normal window, in an incognito one
   *  only those allowed there. */
  usable = $derived(this.user.filter((e) => this.runsHere(e)));
  builtin = $derived(this.items.filter((e) => e.builtin));
  pinned = $derived(
    settings.value.pinnedExtensions
      .map((id) => this.items.find((e) => e.id === id && !e.builtin))
      .filter((e): e is ExtensionInfo => !!e && e.enabled && this.runsHere(e)),
  );

  runsHere(e: ExtensionInfo) {
    return !incognito || (settings.value.extensionsInIncognito && e.incognito);
  }

  async setIncognito(e: ExtensionInfo, allowed: boolean) {
    if (allowed && !confirmIncognito(e.name)) return;
    try {
      await extensionsApi.extSetIncognito(e.id, allowed);
    } catch (err) {
      ui.toast(String(err), { icon: "alert" });
    }
    await this.refresh();
  }

  /** The global switch; turning it on asks first, since extensions can see incognito pages. */
  setAllowedInIncognito(on: boolean) {
    if (on && !confirmIncognito()) return false;
    settings.update({ extensionsInIncognito: on });
    return true;
  }

  async refresh() {
    try {
      const [items, badgeList] = await Promise.all([
        extensionsApi.extList(),
        extensionsApi.extBadges().catch(() => [] as BadgeEvent[]),
      ]);
      this.items = items;
      const map: Record<string, ActionBadge> = {};
      for (const b of badgeList) {
        map[b.tabId ? `${b.extensionId}:${b.tabId}` : b.extensionId] = b.badge;
      }
      this.badges = map;
      this.error = "";
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  handleBadge(e: BadgeEvent) {
    const key = e.tabId ? `${e.extensionId}:${e.tabId}` : e.extensionId;
    const next = { ...this.badges };
    if (!e.badge.text && !e.badge.backgroundColor && !e.badge.textColor) {
      delete next[key];
    } else {
      next[key] = e.badge;
    }
    this.badges = next;
  }

  badgeFor(extId: string, tabId?: string | null): ActionBadge | null {
    return (tabId ? this.badges[`${extId}:${tabId}`] : undefined) ?? this.badges[extId] ?? null;
  }

  async refreshNativeHosts() {
    try {
      this.nativeHosts = await extensionsApi.extNativeHosts();
    } catch {
      this.nativeHosts = [];
    }
  }

  async setNativeHostConsent(hostName: string, allowed: boolean) {
    try {
      await extensionsApi.extNativeHostConsent(hostName, allowed);
      await this.refreshNativeHosts();
    } catch (e) {
      ui.toast(String(e), { icon: "alert" });
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

  justOpenedPopup() {
    return performance.now() - this.popupOpenedAt < 450;
  }

  /** Triggers an extension's real action on the focused web tab (waking it if asleep):
   *  opens its dynamic/default popup in the dedicated popup card, dispatches `action.onClicked`
   *  for popup-less extensions, or falls back to options / extensions page. */
  async open(ext: ExtensionInfo, anchor?: PopupAnchor, opts: { inspect?: boolean } = {}) {
    this.close();
    if (!opts.inspect && this.activePopup?.extId === ext.id) {
      this.closePopup();
      return;
    }
    const target = browser.activeTab;
    if (target && !target.live && needsWebview(target.url)) {
      await browser.wake(target);
    }
    const tabId = target?.live ? target.id : null;
    const resolvedAnchor: PopupAnchor = anchor ?? {
      x: Math.max(16, window.innerWidth - 404),
      y: 44,
      below: true,
    };
    try {
      const outcome = await extensionsApi.extTriggerAction(ext.id, tabId);
      if (outcome.kind === "popup") {
        await this.openPopupUrl(ext.id, outcome.url, resolvedAnchor, target?.url ?? null, opts.inspect);
        return;
      }
      if (outcome.kind === "dispatched") return;
    } catch {
      if (ext.popupUrl) {
        await this.openPopupUrl(ext.id, ext.popupUrl, resolvedAnchor, target?.url ?? null, opts.inspect);
        return;
      }
    }
    if (ext.popupUrl) {
      await this.openPopupUrl(ext.id, ext.popupUrl, resolvedAnchor, target?.url ?? null, opts.inspect);
    } else if (ext.optionsUrl) {
      browser.openTab(ext.optionsUrl);
    } else {
      browser.openTab("zenith://extensions");
    }
  }

  private async openPopupUrl(
    extId: string,
    url: string,
    anchor: PopupAnchor,
    openerUrl: string | null,
    inspect = false,
  ) {
    if (this.activePopup) this.closePopup();
    const id = `extpop-${++this.popupSeq}`;
    this.popupOpenedAt = performance.now();
    this.activePopup = { extId, tabId: id, url, anchor, live: true, loading: true };
    try {
      await tabsApi.tabCreate({ id, url, glance: true, openerUrl });
      if (this.activePopup?.tabId !== id) {
        void tabsApi.tabClose(id);
        return;
      }
      focusAfterLayout(id);
      scheduleLayout();
      if (inspect) {
        setTimeout(() => {
          if (this.activePopup?.tabId === id) {
            void tabsApi.tabCommand(id, "devtoolsPopup").catch(() => {});
          }
        }, 250);
      }
    } catch {
      if (this.activePopup?.tabId === id) this.closePopup();
    }
  }

  closePopup() {
    const p = this.activePopup;
    if (!p) return;
    this.activePopup = null;
    void tabsApi.tabClose(p.tabId).catch(() => {});
    focusAfterLayout(browser.activeTab?.live ? browser.activeTab.id : null);
    scheduleLayout();
  }

  applyPopupPatch(patch: TabPatch) {
    if (!this.activePopup || this.activePopup.tabId !== patch.id) return;
    if (patch.loading !== undefined) this.activePopup.loading = patch.loading;
  }

  async setEnabled(ext: ExtensionInfo, enabled: boolean) {
    if (!enabled && this.activePopup?.extId === ext.id) this.closePopup();
    try {
      await extensionsApi.extSetEnabled(ext.id, enabled);
    } catch (e) {
      ui.toast(String(e), { icon: "alert" });
    }
    await this.refresh();
  }

  async remove(ext: ExtensionInfo) {
    if (this.activePopup?.extId === ext.id) this.closePopup();
    try {
      await extensionsApi.extRemove(ext.id);
      if (this.isPinned(ext.id)) this.togglePin(ext.id);
      ui.toast(`Removed ${ext.name}`, { icon: "trash" });
    } catch (e) {
      ui.toast(String(e), { icon: "alert" });
    }
    await this.refresh();
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
      const ext = await extensionsApi.extOfferAccept(offer.token);
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
    void extensionsApi.extOfferDiscard(offer.token).catch(() => {});
  }

  // ----------------------------------------------------------------- updates

  async checkUpdates(silent = false) {
    if (this.checking) return;
    this.checking = true;
    try {
      this.updates = await extensionsApi.extCheckUpdates();
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
      const ext = await extensionsApi.extUpdate(u.id);
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
