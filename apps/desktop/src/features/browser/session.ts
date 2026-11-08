// BrowserStore session: snapshot, restore and private (incognito) start.
import type { BrowserStore } from "./browser.svelte";
import type { Tab, TabId } from "./types";
import type { SessionFile } from "./model";
import { SPACE_COLORS, INCOGNITO_SPACE, uid, makeTab } from "./model";

export function snapshot(store: BrowserStore): string {
  const persistent = Object.values(store.tabs)
    .filter(
      (t) =>
        store.glance?.tabId !== t.id && (t.essential || store.space(t.spaceId)),
    )
    .map((t) => ({
      id: t.id,
      url: t.url,
      title: t.title,
      customTitle: t.customTitle,
      favicon: t.favicon,
      spaceId: t.spaceId,
      pinned: t.pinned,
      essential: t.essential,
      pinnedUrl: t.pinnedUrl,
      openerId: t.openerId,
      lastActive: t.lastActive,
      scroll: t.scroll,
      zoom: t.zoom,
      muted: t.muted,
    }));
  const file: SessionFile = {
    v: 1,
    activeSpaceId: store.activeSpaceId,
    essentials: [...store.essentials],
    spaces: store.spaces.map((s) => ({
      ...s,
      tabIds: [...s.tabIds],
      pinnedIds: [...s.pinnedIds],
      folders: (s.folders ?? []).map((f) => ({
        ...f,
        tabIds: f.tabIds.filter((id) => s.pinnedIds.includes(id)),
      })),
      split: s.split
        ? { ids: [...s.split.ids], ratio: s.split.ratio, layout: s.split.layout }
        : undefined,
    })),
    tabs: persistent,
    closed: store.closed.slice(0, 25).map((c) => ({ ...c })),
  };
  return JSON.stringify(file);
}

/** Incognito windows start empty and never restore (or save) a session. */
export function startPrivate(store: BrowserStore) {
  const space = store.createSpace(
    INCOGNITO_SPACE.name,
    INCOGNITO_SPACE.icon,
    INCOGNITO_SPACE.color,
  );
  store.activeSpaceId = space.id;
  store.restored = true;
}

/** Restores a session. `fresh`: the session is another window's (a new window gets that window's
 *  spaces, essentials and pinned tabs): its tabs get new ids, and nothing is selected, so the
 *  window opens on the start page. */
export function restore(store: BrowserStore, json: string | null, keepTabs: boolean, fresh = false) {
  let file: SessionFile | null = null;
  try {
    file = json ? (JSON.parse(json) as SessionFile) : null;
  } catch {
    file = null;
  }
  if (file && fresh) file = renumberTabs(file, !keepTabs);
  if (!file || !Array.isArray(file.spaces) || !file.spaces.length) {
    const home = store.createSpace("Home", "🏠", SPACE_COLORS[0]);
    store.activeSpaceId = home.id;
    store.restored = true;
    return;
  }
  const tabs: Record<TabId, Tab> = {};
  for (const t of file.tabs ?? []) {
    if (!t.id || typeof t.url !== "string") continue;
    const keep = keepTabs || t.pinned || t.essential;
    if (!keep) continue;
    tabs[t.id] = makeTab(t.url, t.spaceId ?? "", {
      ...t,
      live: false,
      loading: false,
      zoom: t.zoom ?? 1,
      url:
        (t.pinned || t.essential) && !keepTabs && t.pinnedUrl
          ? t.pinnedUrl
          : t.url,
    } as Partial<Tab>);
  }
  const spaces = file.spaces.map((s) => ({
    ...s,
    tabIds: (s.tabIds ?? []).filter((id) => tabs[id] && !tabs[id].essential),
    pinnedIds: (s.pinnedIds ?? []).filter(
      (id) => tabs[id] && !tabs[id].essential,
    ),
    folders: Array.isArray(s.folders)
      ? s.folders.map((f) => ({
          ...f,
          tabIds: (f.tabIds ?? []).filter(
            (id) => tabs[id] && !tabs[id].essential,
          ),
        }))
      : [],
    split:
      s.split &&
      Array.isArray(s.split.ids) &&
      s.split.ids.length >= 2 &&
      s.split.ids.every((id) => tabs[id])
        ? s.split
        : undefined,
  }));
  for (const s of spaces) {
    if (s.activeTabId && !tabs[s.activeTabId])
      s.activeTabId = [...s.pinnedIds, ...s.tabIds][0];
  }
  // A new window opens on its start page, not on one of the copied pinned tabs.
  if (fresh && !keepTabs) for (const s of spaces) s.activeTabId = undefined;
  store.tabs = tabs;
  store.spaces = spaces;
  store.essentials = (file.essentials ?? []).filter((id) => tabs[id]);
  store.closed = Array.isArray(file.closed) ? file.closed.slice(0, 25) : [];
  store.activeSpaceId = spaces.some((s) => s.id === file!.activeSpaceId)
    ? file.activeSpaceId
    : spaces[0].id;
  store.restored = true;
  const active = store.activeTab;
  if (active) store.activate(active.id);
}

/** Gives every tab of a session a new id (the same pages may still be open in the window the
 *  session came from, and the backend keys webviews by tab id). `forNewWindow` also drops the
 *  closed-tab list and split views. */
function renumberTabs(file: SessionFile, forNewWindow: boolean): SessionFile {
  const ids = new Map<TabId, TabId>();
  const map = (id: TabId) => {
    let n = ids.get(id);
    if (!n) ids.set(id, (n = uid("t")));
    return n;
  };
  const mapAll = (list: TabId[] | undefined) => (list ?? []).map(map);
  return {
    ...file,
    essentials: mapAll(file.essentials),
    tabs: (file.tabs ?? []).map((t) => ({
      ...t,
      id: t.id ? map(t.id) : t.id,
      openerId: t.openerId ? map(t.openerId) : undefined,
    })),
    spaces: file.spaces.map((s) => ({
      ...s,
      tabIds: mapAll(s.tabIds),
      pinnedIds: mapAll(s.pinnedIds),
      activeTabId: s.activeTabId ? map(s.activeTabId) : undefined,
      folders: (s.folders ?? []).map((f) => ({ ...f, tabIds: mapAll(f.tabIds) })),
      split: s.split && !forNewWindow ? { ...s.split, ids: mapAll(s.split.ids) } : undefined,
    })),
    closed: forNewWindow ? [] : file.closed,
  };
}
