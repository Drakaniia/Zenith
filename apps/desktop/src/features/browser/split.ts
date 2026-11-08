// BrowserStore split view.
import type { BrowserStore } from "./browser.svelte";
import type { SplitLayout, TabId } from "./types";
import { NEW_TAB_URL } from "../../shared/url";
import { scheduleLayout } from "../../platform/layout.svelte";
import { needsWebview } from "./model";

export function splitWith(store: BrowserStore, a: TabId, b: TabId) {
  store.splitTabs([a, b]);
}

/** Shows two to four tabs of the active space together (Zen allows four in a grid). */
export function splitTabs(store: BrowserStore, ids: TabId[], layout: SplitLayout = "vsep") {
  const space = store.activeSpace;
  const members = [...new Set(ids)].filter((id) => store.tabs[id]).slice(0, 4);
  if (!space || members.length < 2) return;
  for (const id of members) {
    const t = store.tabs[id];
    if (!t.essential && t.spaceId !== space.id)
      store.moveToSpace(id, space.id);
  }
  space.split = { ids: members, ratio: 0.5, layout };
  store.activate(members[0]);
  for (const id of members.slice(1)) {
    const t = store.tabs[id];
    if (needsWebview(t.url)) void store.wake(t);
  }
  scheduleLayout();
}

/** Changes how the current split view is laid out. */
export function setSplitLayout(store: BrowserStore, layout: SplitLayout) {
  const split = store.activeSplit;
  if (!split) return;
  split.layout = layout;
  split.ratio = 0.5;
  scheduleLayout();
}

export function toggleSplit(store: BrowserStore) {
  const space = store.activeSpace;
  const tab = store.activeTab;
  if (!space || !tab) return;
  if (store.activeSplit) {
    space.split = undefined;
    scheduleLayout();
    return;
  }
  const ids = store.order().filter((x) => x !== tab.id);
  const i = store.order().indexOf(tab.id);
  const other = store.order()[i + 1] ?? ids[ids.length - 1];
  if (other) store.splitWith(tab.id, other);
  else
    store.splitWith(tab.id, store.openTab(NEW_TAB_URL, { activate: false }).id);
}

export function unsplit(store: BrowserStore) {
  const space = store.activeSpace;
  if (space) space.split = undefined;
  scheduleLayout();
}

export function isSplitAnywhere(store: BrowserStore, id: TabId) {
  return store.spaces.some((s) => s.split?.ids.includes(id));
}
