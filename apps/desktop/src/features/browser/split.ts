// BrowserStore split view: multiple split groups per space, 2-4 panes, arbitrary resizing and pane reorder.
import type { BrowserStore } from "./browser.svelte";
import type { SplitLayout, SplitState, TabId } from "./types";
import { NEW_TAB_URL } from "../../shared/url";
import { scheduleLayout } from "../../platform/layout.svelte";
import { needsWebview } from "./model";
import {
  MIN_PANE_SHARE,
  defaultRatios,
  detachFromSplits,
  effectiveRatios,
  ensureSplits,
  resizeAdjacent,
  resizeGridAxis,
  spaceSplits,
} from "./split-math";

export {
  MIN_PANE_SHARE,
  defaultRatios,
  detachFromSplits,
  effectiveRatios,
  resizeAdjacent,
  resizeGridAxis,
  spaceSplits,
};

export function splitWith(store: BrowserStore, a: TabId, b: TabId, layout: SplitLayout = "vsep") {
  store.splitTabs([a, b], layout);
}

/** Shows two to four tabs of the active space together as an independent split group. */
export function splitTabs(store: BrowserStore, ids: TabId[], layout: SplitLayout = "vsep") {
  const space = store.activeSpace;
  const members = [...new Set(ids)].filter((id) => store.tabs[id]).slice(0, 4);
  if (!space || members.length < 2) return;
  for (const id of members) {
    const t = store.tabs[id];
    if (!t.essential && t.spaceId !== space.id) store.moveToSpace(id, space.id);
    for (const s of store.spaces) detachFromSplits(s, id);
  }
  const ratios = defaultRatios(members.length, layout);
  ensureSplits(space).push({ ids: members, ratio: ratios[0] ?? 0.5, ratios, layout });
  store.activate(members[0]);
  for (const id of members.slice(1)) {
    const t = store.tabs[id];
    if (needsWebview(t.url)) void store.wake(t);
  }
  scheduleLayout();
}

/** Drops a tab onto one edge of the content area: adds to the active split (up to 4) or starts one. */
export function addToSplit(
  store: BrowserStore,
  tabId: TabId,
  side: "left" | "right" | "top" | "bottom",
) {
  const space = store.activeSpace;
  const current = store.activeTab;
  const tab = store.tabs[tabId];
  if (!space || !current || !tab || current.id === tabId) return;
  const vertical = side === "top" || side === "bottom";
  const first = side === "left" || side === "top";
  const active = store.activeSplit;

  if (active && !active.ids.includes(tabId) && active.ids.length < 4) {
    if (!tab.essential && tab.spaceId !== space.id) store.moveToSpace(tabId, space.id);
    for (const s of store.spaces) {
      if (s !== space) detachFromSplits(s, tabId);
      else {
        for (const g of ensureSplits(s)) {
          if (g !== active && g.ids.includes(tabId)) detachFromSplits(s, tabId);
        }
      }
    }
    const curLayout = active.layout ?? "vsep";
    const perpendicular = (vertical && curLayout === "vsep") || (!vertical && curLayout === "hsep");
    if (perpendicular && active.ids.length >= 2) active.layout = "grid";
    if (first) active.ids.unshift(tabId);
    else active.ids.push(tabId);
    active.ratios = defaultRatios(active.ids.length, active.layout ?? "vsep");
    active.ratio = active.ratios[0] ?? 0.5;
    if (needsWebview(tab.url)) void store.wake(tab);
    scheduleLayout();
    return;
  }

  const layout: SplitLayout = vertical ? "hsep" : "vsep";
  store.splitTabs(first ? [tabId, current.id] : [current.id, tabId], layout);
}

/** Changes how the active split view is laid out. */
export function setSplitLayout(store: BrowserStore, layout: SplitLayout) {
  const split = store.activeSplit;
  if (!split) return;
  split.layout = layout;
  split.ratios = defaultRatios(split.ids.length, layout);
  split.ratio = split.ratios[0] ?? 0.5;
  scheduleLayout();
}

export function resetSplitRatios(store: BrowserStore) {
  const split = store.activeSplit;
  if (!split) return;
  split.ratios = defaultRatios(split.ids.length, split.layout ?? "vsep");
  split.ratio = split.ratios[0] ?? 0.5;
  scheduleLayout();
}

/** Moves or swaps panes within the active split view. */
export function swapSplitPanes(store: BrowserStore, from: number, to: number) {
  const split = store.activeSplit;
  if (!split || from === to || from < 0 || to < 0 || from >= split.ids.length || to >= split.ids.length) return;
  const ids = [...split.ids];
  const [moved] = ids.splice(from, 1);
  ids.splice(to, 0, moved);
  split.ids = ids;
  if ((split.layout ?? "vsep") !== "grid" && split.ratios?.length === ids.length) {
    const r = [...split.ratios];
    const [mr] = r.splice(from, 1);
    r.splice(to, 0, mr);
    split.ratios = r;
    split.ratio = r[0] ?? 0.5;
  }
  scheduleLayout();
}

export function toggleSplit(store: BrowserStore) {
  const space = store.activeSpace;
  const tab = store.activeTab;
  if (!space || !tab) return;
  if (store.activeSplit) {
    store.unsplit();
    return;
  }
  const ids = store.order().filter((x) => x !== tab.id);
  const i = store.order().indexOf(tab.id);
  const other = store.order()[i + 1] ?? ids[ids.length - 1];
  if (other) store.splitWith(tab.id, other);
  else store.splitWith(tab.id, store.openTab(NEW_TAB_URL, { activate: false }).id);
}

/** Exits only the active split group (or `target`), keeping any other split groups in the space. */
export function unsplit(store: BrowserStore, target?: SplitState) {
  const space = store.activeSpace;
  const group = target ?? store.activeSplit;
  if (!space || !group) return;
  const splits = ensureSplits(space);
  space.splits = splits.filter((g) => g !== group);
  scheduleLayout();
}

export function isSplitAnywhere(store: BrowserStore, id: TabId) {
  return store.spaces.some((s) => spaceSplits(s).some((g) => g.ids.includes(id)));
}
