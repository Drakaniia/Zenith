// BrowserStore folders in the pinned section (Zen: zen-folder).
import type { BrowserStore } from "./browser.svelte";
import type { Folder, TabId } from "./types";
import { uid } from "./model";

/** Creates a folder at the top of the pinned section, pinning the given tabs into it. */
export function createFolder(
  store: BrowserStore,
  tabIds: TabId[],
  spaceId = store.activeSpaceId,
): Folder | undefined {
  const space = store.space(spaceId);
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
  for (const id of tabIds) store.moveToFolder(id, folder.id);
  store.renamingFolderId = folder.id;
  return folder;
}

/** Puts a tab into a folder (pinning it), before `beforeId` or at the end; `null` takes it out. */
export function moveToFolder(store: BrowserStore, id: TabId, folderId: string | null, beforeId?: TabId) {
  const tab = store.tabs[id];
  if (!tab || tab.essential) return;
  const space = store.space(tab.spaceId);
  if (!space) return;
  if (!tab.pinned) store.togglePin(id);
  for (const f of space.folders ?? [])
    f.tabIds = f.tabIds.filter((x) => x !== id);
  if (folderId === null) return;
  const folder = space.folders?.find((f) => f.id === folderId);
  if (!folder) return;
  const at = beforeId ? folder.tabIds.indexOf(beforeId) : -1;
  folder.tabIds.splice(at >= 0 ? at : folder.tabIds.length, 0, id);
}

export function toggleFolder(store: BrowserStore, folderId: string, collapsed?: boolean) {
  const folder = store.activeSpace?.folders?.find((f) => f.id === folderId);
  if (folder) folder.collapsed = collapsed ?? !folder.collapsed;
}

export function renameFolder(store: BrowserStore, folderId: string, name: string) {
  const folder = store.spaces
    .flatMap((s) => s.folders ?? [])
    .find((f) => f.id === folderId);
  if (folder && name.trim()) folder.name = name.trim();
}

/** Removes a folder. Its tabs stay pinned, unless `closeTabs` is set. */
export function deleteFolder(store: BrowserStore, folderId: string, closeTabs = false) {
  for (const space of store.spaces) {
    const folder = space.folders?.find((f) => f.id === folderId);
    if (!folder) continue;
    const members = store.folderMembers(space, folder);
    space.folders = space.folders!.filter((f) => f.id !== folderId);
    if (closeTabs) for (const id of members) store.removeTab(id, true);
  }
}
