// The backend's events, wired to the stores. Returns the unlisten handles (App's onMount drops
// them on unmount).
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { shell } from '../platform/shell';
import { on } from '../platform/ipc';
import { runAction } from '../platform/actions';
import { browser } from '../features/browser/browser.svelte';
import { settings } from '../features/settings/settings.svelte';
import { bookmarks } from '../features/bookmarks/bookmarks.svelte';
import { downloads } from '../features/downloads/downloads.svelte';
import { permissions } from '../features/page/permissions.svelte';
import { ui } from '../shared/ui/ui.svelte';
import { incognito } from '../features/window/window.svelte';
import { updates } from '../features/updates/updates.svelte';
import { vault } from '../features/passwords/vault.svelte';
import { extensions } from '../features/extensions/extensions.svelte';
import { onboarding } from '../features/onboarding/onboarding.svelte';

export function listenBackend(flushAndClose: () => Promise<void>): Promise<() => void>[] {
  return [
    on('tab-update', (p) => browser.applyPatch(p)),
    on('tab-created', (e) => {
      if (e.popup) browser.adoptPopup(e.id, e.url, e.openerId);
      else browser.adoptTab(e.id, e.url, e.openerId, true);
    }),
    on('tab-open-request', (r) => {
      if (r.kind === 'glance') browser.openGlance(r.url, r.openerId);
      else if (r.kind === 'split' && r.openerId) {
        const t = browser.openTab(r.url, { activate: false, openerId: r.openerId });
        browser.splitWith(r.openerId, t.id);
      } else browser.openTab(r.url, { activate: r.kind === 'foreground', openerId: r.openerId });
    }),
    on('tab-close-request', ({ id }) => {
      if (browser.glance?.tabId === id) browser.closeGlance();
      else browser.removeTab(id, false);
      permissions.dropTab(id);
      vault.dropTab(id);
    }),
    on('tab-focused', ({ id }) => {
      if (browser.activeSplit?.ids.includes(id) && browser.activeTab?.id !== id) browser.activate(id, { focus: false });
      if (ui.flyout && !ui.flyoutPinned) ui.flyout = false;
      if (ui.topReveal) ui.topReveal = false;
    }),
    // Another normal window changed the settings (incognito windows keep their own copy).
    on('settings-changed', ({ origin }) => {
      if (!incognito && origin !== getCurrentWebviewWindow().label) void settings.reload();
    }),
    on('popup-blocked', ({ tabId, url }) => {
      ui.toast('Pop-up blocked', {
        icon: 'shield',
        timeout: 6000,
        action: { label: 'Open', run: () => browser.openTab(url, { openerId: tabId }) },
      });
    }),
    // The setup assistant is modal: only Quit gets through.
    on('shortcut', (s) => {
      if (onboarding.open && s.action !== 'quit') return;
      void runAction(s.action, s.source, s.repeat);
    }),
    on('permission-request', (p) => permissions.push(p)),
    on('vault-status', (s) => vault.apply(s)),
    on('password-suggest', (s) => {
      if (browser.isVisible(s.tabId) && !onboarding.open) vault.offerSuggest(s);
    }),
    on('password-suggest-close', ({ tabId }) => {
      if (vault.suggest?.tabId === tabId) vault.closeSuggest(false);
    }),
    on('password-save-offer', (o) => vault.pushOffer(o)),
    on('card-save-offer', (o) => vault.pushCardOffer(o)),
    on('ext-install', (e) => extensions.handleEvent(e)),
    on('download', (d) => {
      const known = downloads.items.some((x) => x.id === d.id);
      downloads.upsert(d);
      if (!known && d.state === 'progress') {
        ui.toast(`Downloading ${d.fileName ?? 'file'}`, {
          icon: 'download',
          action: { label: 'Show', run: () => (ui.downloadsOpen = true) },
        });
      } else if (known && d.state === 'completed') {
        ui.toast(`Downloaded ${d.fileName ?? 'file'}`, {
          icon: 'check',
          action: { label: 'Open', run: () => void shell.openPath(d.path) },
        });
      }
    }),
    on('find-result', (r) => {
      if (r.id === ui.find.tabId) {
        ui.find.matches = r.matches;
        ui.find.active = r.active;
      }
    }),
    on('open-urls', (urls) => {
      for (const url of urls) browser.openTab(url);
    }),
    on('update-status', (s) => updates.apply(s)),
    on('media-changed', ({ tabId, count }) => browser.setMediaCount(tabId, count)),
    on('media-request', ({ tabId, hint }) => {
      // From the button over a video or the page's context menu.
      if (browser.tabs[tabId]) ui.openMedia(tabId, hint);
    }),
    on('app-close-requested', () => void flushAndClose()),
    // Another window changed the bookmarks.
    bookmarks.listen(),
  ];
}
