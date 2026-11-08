// The browser model: spaces, tabs, essentials, split view and Glance. The UI owns this state;
// the backend owns the webviews and reports their state through `tab-update` patches.
import type { ClosedTab, Folder, GlanceState, PopupAnchor, Space, SpaceId, SplitLayout, Tab, TabId, TabPatch } from "./types";
import { NEW_TAB_URL } from "$shared/url";
import { createSpace, switchSpace, cycleSpace, deleteSpace, unloadSpace, moveSpace } from "./spaces";
import { createFolder, moveToFolder, toggleFolder, renameFolder, deleteFolder } from "./folders";
import { openTab, adoptTab, wake, activate, activateIndex, cycleTab, navigate, sleep, unload, resetPinned, resetZoom, setMuted } from "./tabs";
import { closeTab, removeTab, closeOthers, closeBelow, clearSpace, reopenClosed, duplicate } from "./tab-close";
import { togglePin, toggleEssential, moveToSpace, moveTab } from "./tab-move";
import { splitWith, splitTabs, setSplitLayout, toggleSplit, unsplit } from "./split";
import { openGlance, adoptPopup, closeGlance, expandGlance } from "./glance";
import { applyPatch, setMediaCount, reviveCrashed } from "./tab-events";
import { snapshot, startPrivate, restore } from "./session";

export { INCOGNITO_SPACE, SPACE_COLORS, SPACE_ICONS, needsWebview, tabTitle } from "./model";
export type { BrowserStore };

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
    if (s.split.ids.length < 2 || !s.split.ids.every((t) => this.tabs[t]))
      return undefined;
    return s.split;
  });
  /** Tabs currently on screen (one, or two to four in split view). */
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
    return createSpace(this, name, icon, color);
  }

  switchSpace(id: SpaceId) {
    return switchSpace(this, id);
  }

  cycleSpace(delta: number) {
    return cycleSpace(this, delta);
  }

  deleteSpace(id: SpaceId) {
    return deleteSpace(this, id);
  }

  // ------------------------------------------------------------------------------------------
  // Folders (Zen: zen-folder)

  /** Creates a folder at the top of the pinned section, pinning the given tabs into it. */
  createFolder(
    tabIds: TabId[],
    spaceId = this.activeSpaceId,
  ): Folder | undefined {
    return createFolder(this, tabIds, spaceId);
  }

  /** Puts a tab into a folder (pinning it), before `beforeId` or at the end; `null` takes it out. */
  moveToFolder(id: TabId, folderId: string | null, beforeId?: TabId) {
    return moveToFolder(this, id, folderId, beforeId);
  }

  toggleFolder(folderId: string, collapsed?: boolean) {
    return toggleFolder(this, folderId, collapsed);
  }

  renameFolder(folderId: string, name: string) {
    return renameFolder(this, folderId, name);
  }

  /** Removes a folder. Its tabs stay pinned, unless `closeTabs` is set. */
  deleteFolder(folderId: string, closeTabs = false) {
    return deleteFolder(this, folderId, closeTabs);
  }

  /** Name being edited inline in the sidebar (a just-created folder starts in rename). */
  renamingFolderId = $state<string | null>(null);

  /** Unloads every tab of a space (Zen: unload all tabs in space). */
  unloadSpace(spaceId: SpaceId) {
    return unloadSpace(this, spaceId);
  }

  moveSpace(id: SpaceId, toIndex: number) {
    return moveSpace(this, id, toIndex);
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
    return openTab(this, url, opts);
  }

  /** Adopts a webview the backend already created (popup opened with an opener). */
  adoptTab(
    id: TabId,
    url: string,
    openerId: TabId | undefined,
    activate: boolean,
  ) {
    return adoptTab(this, id, url, openerId, activate);
  }

  wake(tab: Tab) {
    return wake(this, tab);
  }

  activate(id: TabId, opts: { focus?: boolean } = {}) {
    return activate(this, id, opts);
  }

  activateIndex(n: number) {
    return activateIndex(this, n);
  }

  cycleTab(delta: number) {
    return cycleTab(this, delta);
  }

  navigate(tab: Tab, url: string) {
    return navigate(this, tab, url);
  }

  /** Ctrl+W. Pinned and essential tabs are unloaded (kept) instead of removed, as in Zen. */
  closeTab(id: TabId) {
    return closeTab(this, id);
  }

  removeTab(id: TabId, remember: boolean) {
    return removeTab(this, id, remember);
  }

  closeOthers(id: TabId) {
    return closeOthers(this, id);
  }

  closeBelow(id: TabId) {
    return closeBelow(this, id);
  }

  /** Zen's "Clear" button: close every regular (unpinned) tab of the space. */
  clearSpace(spaceId = this.activeSpaceId) {
    return clearSpace(this, spaceId);
  }

  reopenClosed() {
    return reopenClosed(this);
  }

  duplicate(id: TabId) {
    return duplicate(this, id);
  }

  /** Frees the webview but keeps the tab (tab sleeping). Returns false if exempt. */
  sleep(id: TabId, force = false): Promise<boolean> {
    return sleep(this, id, force);
  }

  /** Unloads a pinned/essential tab and optionally resets it to its pinned URL. */
  unload(id: TabId, reset: boolean) {
    return unload(this, id, reset);
  }

  resetPinned(id: TabId) {
    return resetPinned(this, id);
  }

  togglePin(id: TabId) {
    return togglePin(this, id);
  }

  toggleEssential(id: TabId) {
    return toggleEssential(this, id);
  }

  moveToSpace(id: TabId, spaceId: SpaceId) {
    return moveToSpace(this, id, spaceId);
  }

  /** Moves a tab within the sidebar: into essentials, pinned or regular lists. */
  moveTab(id: TabId, zone: "essentials" | "pinned" | "tabs", index: number) {
    return moveTab(this, id, zone, index);
  }

  resetZoom(id: TabId) {
    return resetZoom(this, id);
  }

  setMuted(id: TabId, muted: boolean) {
    return setMuted(this, id, muted);
  }

  // ------------------------------------------------------------------------------------------
  // Split view

  splitWith(a: TabId, b: TabId) {
    return splitWith(this, a, b);
  }

  /** Shows two to four tabs of the active space together (Zen allows four in a grid). */
  splitTabs(ids: TabId[], layout: SplitLayout = "vsep") {
    return splitTabs(this, ids, layout);
  }

  /** Changes how the current split view is laid out. */
  setSplitLayout(layout: SplitLayout) {
    return setSplitLayout(this, layout);
  }

  toggleSplit() {
    return toggleSplit(this);
  }

  unsplit() {
    return unsplit(this);
  }

  // ------------------------------------------------------------------------------------------
  // Glance

  openGlance(url: string, openerId?: TabId, anchor?: PopupAnchor) {
    return openGlance(this, url, openerId, anchor);
  }

  /** Shows a page-opened popup (window.open with a size) as a floating card over its opener. */
  adoptPopup(id: TabId, url: string, openerId: TabId) {
    return adoptPopup(this, id, url, openerId);
  }

  closeGlance() {
    return closeGlance(this);
  }

  /** Turns the Glance card into a regular tab without reloading the page. */
  expandGlance(split = false) {
    return expandGlance(this, split);
  }

  // ------------------------------------------------------------------------------------------
  // Backend events

  applyPatch(p: TabPatch) {
    return applyPatch(this, p);
  }

  /** The media downloader found (or forgot) videos in a tab. */
  setMediaCount(id: TabId, count: number) {
    return setMediaCount(this, id, count);
  }

  /** The backend reports a crashed page; reload by recreating its webview. */
  reviveCrashed(id: TabId) {
    return reviveCrashed(this, id);
  }

  // ------------------------------------------------------------------------------------------
  // Session

  snapshot(): string {
    return snapshot(this);
  }

  /** Incognito windows start empty and never restore (or save) a session. */
  startPrivate() {
    return startPrivate(this);
  }

  /** Restores a session. `fresh`: the session is another window's (a new window gets that window's
   *  spaces, essentials and pinned tabs): its tabs get new ids, and nothing is selected, so the
   *  window opens on the start page. */
  restore(json: string | null, keepTabs: boolean, fresh = false) {
    return restore(this, json, keepTabs, fresh);
  }
}

export const browser = new BrowserStore();
