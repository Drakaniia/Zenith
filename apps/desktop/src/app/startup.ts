// Window startup: settings, appearance, the setup assistant, the session (or a private window),
// ui_ready and the background loads that follow. `onMotionReady` fires on the first frame, from
// when layout changes animate.
import { tick } from 'svelte';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { blockerApi } from '../features/blocker/api';
import { bookmarksApi } from '../features/bookmarks/api';
import { historyApi } from '../features/history/api';
import { tabsApi } from '../features/browser/api';
import { windowApi } from '../features/window/api';
import { browser } from '../features/browser/browser.svelte';
import { settings } from '../features/settings/settings.svelte';
import { bookmarks } from '../features/bookmarks/bookmarks.svelte';
import { downloads } from '../features/downloads/downloads.svelte';
import { scheduleLayout } from '../platform/layout.svelte';
import { startSleepScheduler } from '../features/browser/sleep';
import { appInfo, incognito } from '../features/window/window.svelte';
import { updates } from '../features/updates/updates.svelte';
import { promptDefaultBrowser } from '../features/default-browser/default-browser';
import { vault } from '../features/passwords/vault.svelte';
import { extensions } from '../features/extensions/extensions.svelte';
import { onboarding } from '../features/onboarding/onboarding.svelte';
import { shouldOnboard } from '../features/onboarding/onboarding-flow';

export async function startup(onMotionReady: () => void) {
  await settings.load();
  // Appearance must be applied before the window is first shown.
  const pushed = settings.push().catch((e) => console.warn('apply_settings failed', e));
  void settings.pushShortcuts();
  const s = settings.value;
  // First launch (or setup not finished yet): the setup assistant shows before the window
  // does, so returning users never see it flash by.
  if (shouldOnboard(s, incognito)) onboarding.begin();
  const weekly = Date.now() - s.filterListsUpdated > 7 * 86_400_000;
  void blockerApi
    .blockerConfigure(settings.enabledLists(), weekly && s.blockerEnabled, s.blockerDisabledSites)
    .then(() => {
      if (weekly) settings.update({ filterListsUpdated: Date.now() });
    })
    .catch(() => {});
  if (s.historyDays > 0) void historyApi.historyPrune(s.historyDays).catch(() => {});

  requestAnimationFrame(() => onMotionReady());
  if (incognito) browser.startPrivate();
  else {
    const saved = await tabsApi.sessionLoad().catch(() => null);
    browser.restore(saved?.data ?? null, saved?.keep ?? s.restoreSession, saved?.fresh ?? false);
    // The main window brings back the other windows that were open when Zenith quit.
    void windowApi.windowsRestore(s.restoreSession).catch(() => {});
  }
  await pushed;
  await tick();
  requestAnimationFrame(async () => {
    const info = await windowApi.uiReady();
    appInfo.info = info;
    for (const url of info.startupUrls) browser.openTab(url);
    // A window with nothing to show opens the home page.
    if (!incognito && !info.startupUrls.length && s.homepage && !browser.activeTab) browser.openTab(s.homepage);
    scheduleLayout();
    // Every launch (the main window only) asks to become the default browser while Zenith
    // isn't; the setup assistant asks on its own last step.
    if (!incognito && !onboarding.open && getCurrentWebviewWindow().label === 'main') {
      setTimeout(() => void promptDefaultBrowser(), 1200);
    }
  });
  startSleepScheduler();
  void bookmarks.load();
  // Today's automatic backup of the bookmarks (the first start of the day writes it).
  if (!incognito) void bookmarksApi.bookmarksAutoBackup().catch(() => {});
  void vault.refresh();
  void downloads.load();
  void updates.load();
  // Pinned extension icons need the list; store updates are checked once a day at most.
  setTimeout(() => void extensions.refresh(), 1500);
  if (!incognito && s.extensionUpdates && Date.now() - (s.extensionUpdatesChecked ?? 0) > 86_400_000) {
    setTimeout(() => {
      settings.update({ extensionUpdatesChecked: Date.now() });
      void extensions.checkUpdates(true);
    }, 20_000);
  }
}
