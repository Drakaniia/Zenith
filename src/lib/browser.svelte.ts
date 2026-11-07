// The browser model: spaces, tabs, essentials, split view and Glance. The UI owns this state;
// the backend owns the webviews and reports their state through `tab-update` patches.
import { api } from "./ipc";
import { settings } from "./settings.svelte";
import type {
  ClosedTab,
  Folder,
  GlanceState,
  PopupAnchor,
  Space,
  SpaceId,
  Tab,
  TabId,
  TabPatch,
} from "./types";
import {
  NEW_TAB_URL,
  hostOf,
  isInternal,
  isNewTab,
  siteOf,
  titleFor,
} from "./url";
import { focusAfterLayout, scheduleLayout } from "./layout.svelte";

export const SPACE_COLORS = [
  "#8b7cf6",
  "#5b8def",
  "#2bb5a8",
  "#4fb86a",
  "#e0a83a",
  "#ec7f43",
  "#e8607a",
  "#d36ad6",
  "#8f9bb3",
];
/** The single space an incognito window starts with. */
export const INCOGNITO_SPACE = {
  name: "Incognito",
  icon: "🕶️",
  color: "#7a6fa8",
};
export const SPACE_ICONS = [
  "🏠",
  "💼",
  "🎨",
  "📚",
  "🧪",
  "🎮",
  "🎵",
  "✈️",
  "🛒",
  "💡",
  "🌿",
  "🔥",
  "⭐",
  "🧠",
  "📷",
  "🍿",
];

const uid = (prefix: string) =>
  prefix +
  Math.random().toString(36).slice(2, 8) +
  Date.now().toString(36).slice(-4);

export function needsWebview(url: string): boolean {
  return !isInternal(url) && !isNewTab(url);
}

/** The label a tab shows: the user's name for it, or its page title. */
export function tabTitle(
  tab: Pick<Tab, "url" | "title" | "customTitle">,
): string {
  return tab.customTitle || titleFor(tab.url, tab.title);
}

function makeTab(url: string, spaceId: SpaceId, extra: Partial<Tab> = {}): Tab {
  return {
    id: uid("t"),
    url,
    title: "",
    favicon: undefined,
    spaceId,
    pinned: false,
    essential: false,
    lastActive: Date.now(),
    zoom: 1,
    muted: false,
    live: false,
    suspended: false,
    loading: false,
    canGoBack: false,
    canGoForward: false,
    audible: false,
    blocked: 0,
    crashed: "",
    error: "",
    fullscreen: false,
    media: 0,
    ...extra,
  };
}

interface SessionFile {
  v: number;
  activeSpaceId: SpaceId;
  essentials: TabId[];
  spaces: Space[];
  tabs: Partial<Tab>[];
  closed: ClosedTab[];
}

class BrowserStore {
  tabs = $state<Record<TabId, Tab>>({});
  spaces = $state<Space[]>([]);
  essentials = $state<TabId[]>([]);
  activeSpaceId = $state<SpaceId>("");
  glance = $state<GlanceState | null>(null);
  closed = $state<ClosedTab[]>([]);
  fullscreenTabId = $state<TabId | null>(null);
  restored = $state(false);
  /** When each live tab stopped being visible (for sleeping). Not reactive on purpose. */
  hiddenAt = new Map<TabId, number>();
  /** When each tab was last reported audible. */
  audibleAt = new Map<TabId, number>();

  /** Audible now or within the last minute. WebView2's audible flag can drop for a moment
   *  (silence between tracks, an audio glitch under load); such tabs must not be slept. */
  recentlyAudible(id: TabId, now = Date.now()): boolean {
    return (
      !!this.tabs[id]?.audible ||
      now - (this.audibleAt.get(id) ?? -Infinity) < 60_000
    );
  }

  activeSpace = $derived<Space | undefined>(
    this.spaces.find((s) => s.id === this.activeSpaceId) ?? this.spaces[0],
  );
  activeTab = $derived<Tab | undefined>(
    this.activeSpace?.activeTabId
      ? this.tabs[this.activeSpace.activeTabId]
      : undefined,
  );
  activeSplit = $derived.by(() => {
    const s = this.activeSpace;
    const id = s?.activeTabId;
    if (!s?.split || !id || !s.split.ids.includes(id)) return undefined;
    if (!this.tabs[s.split.ids[0]] || !this.tabs[s.split.ids[1]])
      return undefined;
    return s.split;
  });
  /** Tabs currently on screen (one, or two in split view). */
  visibleIds = $derived<TabId[]>(
    this.activeSplit
      ? [...this.activeSplit.ids]
      : this.activeTab
        ? [this.activeTab.id]
        : [],
  );

  // ------------------------------------------------------------------------------------------
  // Lookups

  space(id: SpaceId | undefined): Space | undefined {
    return this.spaces.find((s) => s.id === id);
  }

  /** Essentials, then pinned (folders first, as drawn), then regular tabs: the order keyboard
   *  navigation follows. */
  order(space = this.activeSpace): TabId[] {
    if (!space) return [...this.essentials];
    return [
      ...this.essentials,
      ...this.pinnedOrder(space),
      ...space.tabIds,
    ].filter((id) => this.tabs[id]);
  }

  /** Pinned tabs in display order: each folder's tabs, then the loose pinned tabs. */
  pinnedOrder(space: Space): TabId[] {
    const inFolders = this.folderTabs(space);
    return [
      ...inFolders,
      ...space.pinnedIds.filter((id) => !inFolders.includes(id)),
    ];
  }

  /** Tabs of a folder that still belong to the space's pinned list, in folder order. */
  folderMembers(space: Space, folder: Folder): TabId[] {
    return folder.tabIds.filter(
      (id) => space.pinnedIds.includes(id) && this.tabs[id],
    );
  }

  folderTabs(space: Space): TabId[] {
    return (space.folders ?? []).flatMap((f) => this.folderMembers(space, f));
  }

  folderOf(id: TabId): Folder | undefined {
    const tab = this.tabs[id];
    const space = tab && this.space(tab.spaceId);
    if (!space || !tab.pinned) return undefined;
    return space.folders?.find((f) => f.tabIds.includes(id));
  }

  isVisible(id: TabId) {
    return this.visibleIds.includes(id) || this.glance?.tabId === id;
  }

  // ------------------------------------------------------------------------------------------
  // Spaces

  createSpace(name?: string, icon?: string, color?: string): Space {
    const n = this.spaces.length;
    const space: Space = {
      id: uid("s"),
      name: name || (n === 0 ? "Home" : `Space ${n + 1}`),
      icon: icon || SPACE_ICONS[n % SPACE_ICONS.length],
      color: color || SPACE_COLORS[n % SPACE_COLORS.length],
      tabIds: [],
      pinnedIds: [],
    };
    this.spaces.push(space);
    return this.spaces[this.spaces.length - 1];
  }

  switchSpace(id: SpaceId) {
    if (id === this.activeSpaceId || !this.space(id)) return;
    const previous = this.activeSpace;
    for (const v of this.visibleIds) this.hiddenAt.set(v, Date.now());
    this.activeSpaceId = id;
    const next = this.activeSpace;
    if (next?.activeTabId && this.tabs[next.activeTabId])
      this.activate(next.activeTabId);
    else focusAfterLayout(null);
    if (previous && settings.value.sleepOnSpaceSwitch) {
      for (const tid of [...previous.pinnedIds, ...previous.tabIds])
        void this.sleep(tid);
    }
    scheduleLayout();
  }

  cycleSpace(delta: number) {
    if (this.spaces.length < 2) return;
    const i = this.spaces.findIndex((s) => s.id === this.activeSpaceId);
    const next =
      this.spaces[(i + delta + this.spaces.length) % this.spaces.length];
    this.switchSpace(next.id);
  }

  deleteSpace(id: SpaceId) {
    if (this.spaces.length <= 1) return;
    const space = this.space(id);
    if (!space) return;
    for (const tid of [...space.pinnedIds, ...space.tabIds])
      this.removeTab(tid, false);
    const i = this.spaces.findIndex((s) => s.id === id);
    this.spaces.splice(i, 1);
    if (this.activeSpaceId === id)
      this.switchSpace(this.spaces[Math.max(0, i - 1)].id);
  }

  // ------------------------------------------------------------------------------------------
  // Folders (Zen: zen-folder)

  /** Creates a folder at the top of the pinned section, pinning the given tabs into it. */
  createFolder(
    tabIds: TabId[],
    spaceId = this.activeSpaceId,
  ): Folder | undefined {
    const space = this.space(spaceId);
    if (!space) return undefined;
    space.folders ??= [];
    const n = space.folders.length;
    space.folders.unshift({
      id: uid("f"),
      name: n ? `New Folder ${n + 1}` : "New Folder",
      collapsed: false,
      tabIds: [],
    });
    const folder = space.folders[0];
    for (const id of tabIds) this.moveToFolder(id, folder.id);
    this.renamingFolderId = folder.id;
    return folder;
  }

  /** Puts a tab into a folder (pinning it), before `beforeId` or at the end; `null` takes it out. */
  moveToFolder(id: TabId, folderId: string | null, beforeId?: TabId) {
    const tab = this.tabs[id];
    if (!tab || tab.essential) return;
    const space = this.space(tab.spaceId);
    if (!space) return;
    if (!tab.pinned) this.togglePin(id);
    for (const f of space.folders ?? [])
      f.tabIds = f.tabIds.filter((x) => x !== id);
    if (folderId === null) return;
    const folder = space.folders?.find((f) => f.id === folderId);
    if (!folder) return;
    const at = beforeId ? folder.tabIds.indexOf(beforeId) : -1;
    folder.tabIds.splice(at >= 0 ? at : folder.tabIds.length, 0, id);
  }

  toggleFolder(folderId: string, collapsed?: boolean) {
    const folder = this.activeSpace?.folders?.find((f) => f.id === folderId);
    if (folder) folder.collapsed = collapsed ?? !folder.collapsed;
  }

  renameFolder(folderId: string, name: string) {
    const folder = this.spaces
      .flatMap((s) => s.folders ?? [])
      .find((f) => f.id === folderId);
    if (folder && name.trim()) folder.name = name.trim();
  }

  /** Removes a folder. Its tabs stay pinned, unless `closeTabs` is set. */
  deleteFolder(folderId: string, closeTabs = false) {
    for (const space of this.spaces) {
      const folder = space.folders?.find((f) => f.id === folderId);
      if (!folder) continue;
      const members = this.folderMembers(space, folder);
      space.folders = space.folders!.filter((f) => f.id !== folderId);
      if (closeTabs) for (const id of members) this.removeTab(id, true);
    }
  }

  /** Name being edited inline in the sidebar (a just-created folder starts in rename). */
  renamingFolderId = $state<string | null>(null);

  /** Unloads every tab of a space (Zen: unload all tabs in space). */
  unloadSpace(spaceId: SpaceId) {
    const space = this.space(spaceId);
    if (!space) return;
    for (const id of [...space.pinnedIds, ...space.tabIds]) void this.sleep(id);
  }

  moveSpace(id: SpaceId, toIndex: number) {
    const from = this.spaces.findIndex((s) => s.id === id);
    if (from < 0) return;
    const [s] = this.spaces.splice(from, 1);
    this.spaces.splice(
      Math.max(0, Math.min(toIndex, this.spaces.length)),
      0,
      s,
    );
  }

  // ------------------------------------------------------------------------------------------
  // Tabs

  /** Opens a tab. Background tabs still start loading so they are ready when visited. */
  openTab(
    url: string = NEW_TAB_URL,
    opts: {
      activate?: boolean;
      spaceId?: SpaceId;
      openerId?: TabId;
      index?: number;
      pinned?: boolean;
    } = {},
  ): Tab {
    const space =
      this.space(opts.spaceId) ?? this.activeSpace ?? this.createSpace();
    const draft = makeTab(url, space.id, {
      openerId: opts.openerId,
      pinned: !!opts.pinned,
    });
    this.tabs[draft.id] = draft;
    const tab = this.tabs[draft.id];
    const list = opts.pinned ? space.pinnedIds : space.tabIds;
    let index = opts.index ?? (settings.value.newTabsAtTop ? 0 : list.length);
    if (opts.index === undefined && opts.openerId) {
      const at = list.indexOf(opts.openerId);
      if (at >= 0) {
        index = at + 1;
        // Keep tabs opened from the same page together, in opening order.
        while (
          index < list.length &&
          this.tabs[list[index]]?.openerId === opts.openerId
        )
          index++;
      }
    }
    list.splice(Math.max(0, Math.min(index, list.length)), 0, tab.id);
    if (opts.pinned) tab.pinnedUrl = url;
    if (opts.activate !== false) this.activate(tab.id);
    else if (needsWebview(url)) void this.wake(tab);
    return tab;
  }

  /** Adopts a webview the backend already created (popup opened with an opener). */
  adoptTab(
    id: TabId,
    url: string,
    openerId: TabId | undefined,
    activate: boolean,
  ) {
    const opener = openerId ? this.tabs[openerId] : undefined;
    const spaceId =
      opener && !opener.essential ? opener.spaceId : this.activeSpaceId;
    const space = this.space(spaceId) ?? this.activeSpace!;
    this.tabs[id] = makeTab(url, space.id, {
      id,
      openerId,
      live: true,
      loading: true,
    });
    const at = openerId ? space.tabIds.indexOf(openerId) : -1;
    space.tabIds.splice(at >= 0 ? at + 1 : 0, 0, id);
    if (activate) this.activate(id);
  }

  async wake(tab: Tab) {
    if (tab.live || !needsWebview(tab.url)) return;
    tab.live = true;
    tab.crashed = "";
    tab.error = "";
    tab.loading = true;
    tab.media = 0;
    const site = siteOf(hostOf(tab.url));
    const zoom =
      settings.value.siteZoom[site] ?? (tab.zoom !== 1 ? tab.zoom : undefined);
    const scroll = tab.scroll;
    tab.scroll = undefined;
    try {
      await api.tabCreate({
        id: tab.id,
        url: tab.url,
        muted: tab.muted,
        zoom,
        scrollX: scroll?.x,
        scrollY: scroll?.y,
      });
    } catch (e) {
      tab.live = false;
      tab.loading = false;
      tab.error = String(e);
    }
    scheduleLayout();
  }

  activate(id: TabId, opts: { focus?: boolean } = {}) {
    const tab = this.tabs[id];
    if (!tab) return;
    if (
      !tab.essential &&
      tab.spaceId &&
      tab.spaceId !== this.activeSpaceId &&
      this.space(tab.spaceId)
    ) {
      for (const v of this.visibleIds) this.hiddenAt.set(v, Date.now());
      this.activeSpaceId = tab.spaceId;
    }
    const space = this.activeSpace;
    if (!space) return;
    const before = this.visibleIds;
    space.activeTabId = id;
    tab.lastActive = Date.now();
    tab.suspended = false;
    for (const v of before)
      if (!this.visibleIds.includes(v)) this.hiddenAt.set(v, Date.now());
    for (const v of this.visibleIds) this.hiddenAt.delete(v);
    if (needsWebview(tab.url)) void this.wake(tab);
    if (opts.focus !== false)
      focusAfterLayout(tab.live || needsWebview(tab.url) ? id : null);
    scheduleLayout();
  }

  activateIndex(n: number) {
    const ids = this.order();
    const id = n < 0 ? ids[ids.length - 1] : ids[n];
    if (id) this.activate(id);
  }

  cycleTab(delta: number) {
    const ids = this.order();
    if (!ids.length) return;
    const cur = this.activeTab ? ids.indexOf(this.activeTab.id) : -1;
    this.activate(ids[(cur + delta + ids.length) % ids.length]);
  }

  navigate(tab: Tab, url: string) {
    if (!needsWebview(url)) {
      if (tab.live) {
        this.openTab(url, { openerId: tab.id });
        return;
      }
      tab.url = url;
      tab.title = "";
      tab.favicon = undefined;
      scheduleLayout();
      return;
    }
    tab.error = "";
    if (tab.live) {
      void api.tabNavigate(tab.id, url).catch((e) => (tab.error = String(e)));
    } else {
      tab.url = url;
      tab.title = "";
      tab.favicon = undefined;
      void this.wake(tab);
    }
    focusAfterLayout(tab.id);
  }

  /** Tab to show after `id` goes away, following the order of the tab's own list. */
  #neighbor(id: TabId, space: Space): TabId | undefined {
    const tab = this.tabs[id];
    if (
      tab?.openerId &&
      this.tabs[tab.openerId] &&
      tab.lastActive > (this.tabs[tab.openerId]?.lastActive ?? 0)
    ) {
      const opener = this.tabs[tab.openerId];
      if (opener.essential || opener.spaceId === space.id) return opener.id;
    }
    const ids = this.order(space);
    const i = ids.indexOf(id);
    const rest = ids.filter((x) => x !== id);
    if (!rest.length) return undefined;
    return rest[Math.min(Math.max(i, 0), rest.length - 1)];
  }

  /** Ctrl+W. Pinned and essential tabs are unloaded (kept) instead of removed, as in Zen. */
  closeTab(id: TabId) {
    const tab = this.tabs[id];
    if (!tab) return;
    if (this.glance?.tabId === id) return this.closeGlance();
    if (tab.pinned || tab.essential) {
      this.#deactivateEverywhere(id);
      void this.unload(id, true);
      return;
    }
    this.removeTab(id, true);
  }

  #deactivateEverywhere(id: TabId) {
    for (const space of this.spaces) {
      if (space.split?.ids.includes(id)) space.split = undefined;
      if (space.activeTabId === id) {
        const next = this.#neighbor(id, space);
        space.activeTabId = next;
        if (space.id === this.activeSpaceId && next) this.activate(next);
      }
    }
    if (this.activeSpace && !this.activeSpace.activeTabId)
      focusAfterLayout(null);
  }

  removeTab(id: TabId, remember: boolean) {
    const tab = this.tabs[id];
    if (!tab) return;
    if (remember && !isNewTab(tab.url)) {
      const space = this.space(tab.spaceId);
      this.closed.unshift({
        url: tab.url,
        title: tab.title,
        favicon: tab.favicon,
        spaceId: tab.spaceId,
        index: space ? space.tabIds.indexOf(id) : 0,
        pinned: tab.pinned,
        closedAt: Date.now(),
      });
      if (this.closed.length > 25) this.closed.length = 25;
    }
    this.#deactivateEverywhere(id);
    this.essentials = this.essentials.filter((x) => x !== id);
    for (const space of this.spaces) {
      space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
      space.tabIds = space.tabIds.filter((x) => x !== id);
    }
    if (tab.live) void api.tabClose(id);
    this.hiddenAt.delete(id);
    this.audibleAt.delete(id);
    delete this.tabs[id];
    scheduleLayout();
  }

  closeOthers(id: TabId) {
    const tab = this.tabs[id];
    const space = this.space(tab?.spaceId) ?? this.activeSpace;
    if (!space) return;
    for (const other of [...space.tabIds])
      if (other !== id) this.removeTab(other, true);
    this.activate(id);
  }

  closeBelow(id: TabId) {
    const space = this.space(this.tabs[id]?.spaceId) ?? this.activeSpace;
    if (!space) return;
    const i = space.tabIds.indexOf(id);
    for (const other of space.tabIds.slice(i + 1)) this.removeTab(other, true);
  }

  /** Zen's "Clear" button: close every regular (unpinned) tab of the space. */
  clearSpace(spaceId = this.activeSpaceId) {
    const space = this.space(spaceId);
    if (!space) return;
    for (const id of [...space.tabIds]) this.removeTab(id, true);
  }

  reopenClosed() {
    const c = this.closed.shift();
    if (!c) return;
    const space = this.space(c.spaceId) ?? this.activeSpace;
    this.openTab(c.url, {
      spaceId: space?.id,
      index: c.index,
      pinned: c.pinned,
    });
  }

  duplicate(id: TabId) {
    const tab = this.tabs[id];
    if (tab)
      this.openTab(tab.url, {
        openerId: tab.id,
        spaceId: tab.essential ? undefined : tab.spaceId,
      });
  }

  /** Frees the webview but keeps the tab (tab sleeping). Returns false if exempt. */
  async sleep(id: TabId, force = false): Promise<boolean> {
    const tab = this.tabs[id];
    if (!tab?.live) return false;
    if (
      !force &&
      (this.isVisible(id) ||
        this.recentlyAudible(id) ||
        isSplitAnywhere(this, id))
    )
      return false;
    tab.live = false;
    tab.loading = false;
    tab.audible = false;
    tab.suspended = false;
    tab.media = 0;
    try {
      const scroll = await api.tabSleep(id);
      if (scroll && (scroll.x > 0 || scroll.y > 0)) tab.scroll = scroll;
    } catch {
      /* the webview may already be gone */
    }
    this.hiddenAt.delete(id);
    scheduleLayout();
    return true;
  }

  /** Unloads a pinned/essential tab and optionally resets it to its pinned URL. */
  async unload(id: TabId, reset: boolean) {
    const tab = this.tabs[id];
    if (!tab) return;
    if (tab.live) await this.sleep(id, true);
    if (reset && tab.pinnedUrl) {
      tab.url = tab.pinnedUrl;
      tab.scroll = undefined;
    }
  }

  resetPinned(id: TabId) {
    const tab = this.tabs[id];
    if (tab?.pinnedUrl) this.navigate(tab, tab.pinnedUrl);
  }

  togglePin(id: TabId) {
    const tab = this.tabs[id];
    if (!tab || tab.essential) return;
    const space = this.space(tab.spaceId);
    if (!space) return;
    if (tab.pinned) {
      space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
      for (const f of space.folders ?? [])
        f.tabIds = f.tabIds.filter((x) => x !== id);
      space.tabIds.unshift(id);
      tab.pinned = false;
      tab.pinnedUrl = undefined;
    } else {
      space.tabIds = space.tabIds.filter((x) => x !== id);
      space.pinnedIds.push(id);
      tab.pinned = true;
      tab.pinnedUrl = tab.url;
    }
  }

  toggleEssential(id: TabId) {
    const tab = this.tabs[id];
    if (!tab) return;
    if (tab.essential) {
      this.essentials = this.essentials.filter((x) => x !== id);
      const space = this.activeSpace!;
      tab.essential = false;
      tab.spaceId = space.id;
      tab.pinned = true;
      space.pinnedIds.push(id);
    } else {
      if (this.essentials.length >= 12) return;
      for (const space of this.spaces) {
        space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
        space.tabIds = space.tabIds.filter((x) => x !== id);
        if (space.split?.ids.includes(id)) space.split = undefined;
      }
      tab.essential = true;
      tab.pinned = false;
      tab.spaceId = "";
      tab.pinnedUrl = tab.url;
      this.essentials.push(id);
    }
  }

  moveToSpace(id: TabId, spaceId: SpaceId) {
    const tab = this.tabs[id];
    const target = this.space(spaceId);
    if (!tab || !target || tab.spaceId === spaceId) return;
    if (tab.essential) this.toggleEssential(id);
    const from = this.space(tab.spaceId);
    if (from) {
      if (from.activeTabId === id) {
        from.activeTabId = this.#neighbor(id, from);
        if (from.id === this.activeSpaceId && from.activeTabId)
          this.activate(from.activeTabId);
      }
      if (from.split?.ids.includes(id)) from.split = undefined;
      from.pinnedIds = from.pinnedIds.filter((x) => x !== id);
      from.tabIds = from.tabIds.filter((x) => x !== id);
    }
    tab.spaceId = spaceId;
    (tab.pinned ? target.pinnedIds : target.tabIds).unshift(id);
    scheduleLayout();
  }

  /** Moves a tab within the sidebar: into essentials, pinned or regular lists. */
  moveTab(id: TabId, zone: "essentials" | "pinned" | "tabs", index: number) {
    const tab = this.tabs[id];
    const space = this.activeSpace;
    if (!tab || !space) return;
    if (zone === "essentials") {
      if (!tab.essential) this.toggleEssential(id);
      const list = this.essentials.filter((x) => x !== id);
      list.splice(Math.min(index, list.length), 0, id);
      this.essentials = list;
      return;
    }
    if (tab.essential) {
      this.essentials = this.essentials.filter((x) => x !== id);
      tab.essential = false;
    }
    if (tab.spaceId !== space.id) {
      const from = this.space(tab.spaceId);
      if (from) {
        from.pinnedIds = from.pinnedIds.filter((x) => x !== id);
        from.tabIds = from.tabIds.filter((x) => x !== id);
      }
      tab.spaceId = space.id;
    }
    space.pinnedIds = space.pinnedIds.filter((x) => x !== id);
    space.tabIds = space.tabIds.filter((x) => x !== id);
    const list = zone === "pinned" ? space.pinnedIds : space.tabIds;
    list.splice(Math.min(index, list.length), 0, id);
    const nowPinned = zone === "pinned";
    if (nowPinned && !tab.pinned) tab.pinnedUrl = tab.url;
    if (!nowPinned) tab.pinnedUrl = undefined;
    tab.pinned = nowPinned;
  }

  resetZoom(id: TabId) {
    if (this.tabs[id]?.live) return api.tabCommand(id, "zoom", 1);
  }

  setMuted(id: TabId, muted: boolean) {
    const tab = this.tabs[id];
    if (!tab) return;
    tab.muted = muted;
    if (tab.live) void api.tabCommand(id, muted ? "mute" : "unmute");
  }

  // ------------------------------------------------------------------------------------------
  // Split view

  splitWith(a: TabId, b: TabId) {
    const space = this.activeSpace;
    if (!space || a === b || !this.tabs[a] || !this.tabs[b]) return;
    for (const id of [a, b]) {
      const t = this.tabs[id];
      if (!t.essential && t.spaceId !== space.id)
        this.moveToSpace(id, space.id);
    }
    space.split = { ids: [a, b], ratio: 0.5 };
    this.activate(a);
    if (needsWebview(this.tabs[b].url)) void this.wake(this.tabs[b]);
  }

  toggleSplit() {
    const space = this.activeSpace;
    const tab = this.activeTab;
    if (!space || !tab) return;
    if (this.activeSplit) {
      space.split = undefined;
      scheduleLayout();
      return;
    }
    const ids = this.order().filter((x) => x !== tab.id);
    const i = this.order().indexOf(tab.id);
    const other = this.order()[i + 1] ?? ids[ids.length - 1];
    if (other) this.splitWith(tab.id, other);
    else
      this.splitWith(tab.id, this.openTab(NEW_TAB_URL, { activate: false }).id);
  }

  unsplit() {
    const space = this.activeSpace;
    if (space) space.split = undefined;
    scheduleLayout();
  }

  // ------------------------------------------------------------------------------------------
  // Glance

  openGlance(url: string, openerId?: TabId, anchor?: PopupAnchor) {
    if (this.glance) this.closeGlance();
    const draft = makeTab(url, "", { openerId, live: true, loading: true });
    this.tabs[draft.id] = draft;
    this.glance = { tabId: draft.id, openerId, popup: false, anchor };
    void api
      .tabCreate({ id: draft.id, url, glance: true })
      .catch(() => this.closeGlance());
    focusAfterLayout(draft.id);
    scheduleLayout();
  }

  /** Shows a page-opened popup (window.open with a size) as a floating card over its opener. */
  adoptPopup(id: TabId, url: string, openerId: TabId) {
    this.tabs[id] = makeTab(url, "", {
      id,
      openerId,
      live: true,
      loading: true,
    });
    if (this.glance) this.closeGlance();
    this.glance = { tabId: id, openerId, popup: true };
    void api.tabSetGlance(id, true);
    focusAfterLayout(id);
    scheduleLayout();
  }

  closeGlance() {
    const g = this.glance;
    if (!g) return;
    this.glance = null;
    if (this.tabs[g.tabId]) {
      void api.tabClose(g.tabId);
      delete this.tabs[g.tabId];
    }
    focusAfterLayout(this.activeTab?.live ? this.activeTab.id : null);
    scheduleLayout();
  }

  /** Turns the Glance card into a regular tab without reloading the page. */
  expandGlance(split = false) {
    const g = this.glance;
    const tab = g && this.tabs[g.tabId];
    if (!g || !tab) return;
    this.glance = null;
    void api.tabSetGlance(tab.id, false);
    const space = this.activeSpace!;
    tab.spaceId = space.id;
    tab.openerId = g.openerId;
    const at = g.openerId ? space.tabIds.indexOf(g.openerId) : -1;
    space.tabIds.splice(at >= 0 ? at + 1 : 0, 0, tab.id);
    if (split && g.openerId && this.tabs[g.openerId])
      this.splitWith(g.openerId, tab.id);
    else this.activate(tab.id);
  }

  // ------------------------------------------------------------------------------------------
  // Backend events

  applyPatch(p: TabPatch) {
    const tab = this.tabs[p.id];
    if (!tab) return;
    if (p.url !== undefined && p.url !== tab.url) {
      const oldSite = siteOf(hostOf(tab.url));
      tab.url = p.url;
      const site = siteOf(hostOf(p.url));
      if (site && site !== oldSite) {
        const want = settings.value.siteZoom[site] ?? 1;
        if (Math.abs(want - tab.zoom) > 0.001)
          void api.tabCommand(tab.id, "zoom", want);
      }
    }
    if (p.title !== undefined) tab.title = p.title;
    if (p.loading !== undefined) tab.loading = p.loading;
    if (p.favicon !== undefined) tab.favicon = p.favicon || undefined;
    if (p.canGoBack !== undefined) tab.canGoBack = p.canGoBack;
    if (p.canGoForward !== undefined) tab.canGoForward = p.canGoForward;
    if (p.audible !== undefined) {
      if (p.audible || tab.audible) this.audibleAt.set(tab.id, Date.now());
      tab.audible = p.audible;
    }
    if (p.muted !== undefined) tab.muted = p.muted;
    if (p.blocked !== undefined) tab.blocked = p.blocked;
    if (p.error !== undefined) tab.error = p.error;
    if (p.crashed !== undefined && p.crashed !== tab.crashed) {
      tab.crashed = p.crashed;
      scheduleLayout();
    }
    if (p.zoom !== undefined) {
      tab.zoom = p.zoom;
      const site = siteOf(hostOf(tab.url));
      if (site) {
        const zooms = settings.value.siteZoom;
        if (Math.abs(p.zoom - 1) < 0.001) delete zooms[site];
        else zooms[site] = Math.round(p.zoom * 100) / 100;
        settings.save(false);
      }
    }
    if (p.fullscreen !== undefined) {
      tab.fullscreen = p.fullscreen;
      this.fullscreenTabId = p.fullscreen
        ? tab.id
        : this.fullscreenTabId === tab.id
          ? null
          : this.fullscreenTabId;
    }
  }

  /** The media downloader found (or forgot) videos in a tab. */
  setMediaCount(id: TabId, count: number) {
    const tab = this.tabs[id];
    if (tab && tab.media !== count) tab.media = count;
  }

  /** The backend reports a crashed page; reload by recreating its webview. */
  async reviveCrashed(id: TabId) {
    const tab = this.tabs[id];
    if (!tab) return;
    if (tab.live) {
      // The renderer is gone, so there is no scroll position to save: just free the view.
      tab.live = false;
      await api.tabClose(id).catch(() => {});
    }
    tab.crashed = "";
    await this.wake(tab);
    focusAfterLayout(id);
  }

  // ------------------------------------------------------------------------------------------
  // Session

  snapshot(): string {
    const persistent = Object.values(this.tabs)
      .filter(
        (t) =>
          this.glance?.tabId !== t.id && (t.essential || this.space(t.spaceId)),
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
      activeSpaceId: this.activeSpaceId,
      essentials: [...this.essentials],
      spaces: this.spaces.map((s) => ({
        ...s,
        tabIds: [...s.tabIds],
        pinnedIds: [...s.pinnedIds],
        folders: (s.folders ?? []).map((f) => ({
          ...f,
          tabIds: f.tabIds.filter((id) => s.pinnedIds.includes(id)),
        })),
        split: s.split
          ? { ids: [...s.split.ids] as [TabId, TabId], ratio: s.split.ratio }
          : undefined,
      })),
      tabs: persistent,
      closed: this.closed.slice(0, 25).map((c) => ({ ...c })),
    };
    return JSON.stringify(file);
  }

  /** Incognito windows start empty and never restore (or save) a session. */
  startPrivate() {
    const space = this.createSpace(
      INCOGNITO_SPACE.name,
      INCOGNITO_SPACE.icon,
      INCOGNITO_SPACE.color,
    );
    this.activeSpaceId = space.id;
    this.restored = true;
  }

  restore(json: string | null, keepTabs: boolean) {
    let file: SessionFile | null = null;
    try {
      file = json ? (JSON.parse(json) as SessionFile) : null;
    } catch {
      file = null;
    }
    if (!file || !Array.isArray(file.spaces) || !file.spaces.length) {
      const home = this.createSpace("Home", "🏠", SPACE_COLORS[0]);
      this.activeSpaceId = home.id;
      this.restored = true;
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
        s.split && s.split.ids.every((id) => tabs[id]) ? s.split : undefined,
    }));
    for (const s of spaces) {
      if (s.activeTabId && !tabs[s.activeTabId])
        s.activeTabId = [...s.pinnedIds, ...s.tabIds][0];
    }
    this.tabs = tabs;
    this.spaces = spaces;
    this.essentials = (file.essentials ?? []).filter((id) => tabs[id]);
    this.closed = Array.isArray(file.closed) ? file.closed.slice(0, 25) : [];
    this.activeSpaceId = spaces.some((s) => s.id === file!.activeSpaceId)
      ? file.activeSpaceId
      : spaces[0].id;
    this.restored = true;
    const active = this.activeTab;
    if (active) this.activate(active.id);
  }
}

function isSplitAnywhere(store: BrowserStore, id: TabId) {
  return store.spaces.some((s) => s.split?.ids.includes(id));
}

export const browser = new BrowserStore();
