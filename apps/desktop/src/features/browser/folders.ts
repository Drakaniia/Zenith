// BrowserStore folders in the pinned section (Zen: zen-folder), supporting up to 5 nesting levels.
import type { BrowserStore } from "./browser.svelte";
import type { Folder, Space, TabId } from "./types";
import { uid } from "./model";
import {
  MAX_FOLDER_DEPTH,
  canNestFolder,
  childFolders,
  folderDepth,
  folderSubtreeHeight,
  isDescendantFolder,
  subtreeFolderIds,
  walkFolders,
} from "./folder-tree";

export {
  MAX_FOLDER_DEPTH,
  canNestFolder,
  childFolders,
  folderDepth,
  folderSubtreeHeight,
  isDescendantFolder,
  subtreeFolderIds,
  walkFolders,
};

/** Creates a folder at the top of the pinned section (or inside `parentId`), pinning `tabIds` into it. */
export function createFolder(
  store: BrowserStore,
  tabIds: TabId[],
  spaceId = store.activeSpaceId,
  parentId?: string,
): Folder | undefined {
  const space = store.space(spaceId);
  if (!space) return undefined;
  space.folders ??= [];
  if (parentId) {
    const parent = space.folders.find((f) => f.id === parentId);
    if (!parent || folderDepth(space.folders, parentId) + 1 >= MAX_FOLDER_DEPTH) return undefined;
    parent.collapsed = false;
  }
  const n = space.folders.length;
  space.folders.unshift({
    id: uid("f"),
    name: n ? `New Folder ${n + 1}` : "New Folder",
    collapsed: false,
    tabIds: [],
    ...(parentId ? { parentId } : {}),
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

/** Moves a folder to `targetParentId` (undefined = top level) and optionally before/after a sibling. */
export function moveFolder(
  store: BrowserStore,
  folderId: string,
  targetParentId?: string,
  anchorId?: string,
  where: "before" | "after" = "before",
): boolean {
  const space = store.spaces.find((s) => s.folders?.some((f) => f.id === folderId));
  if (!space?.folders) return false;
  if (!canNestFolder(space.folders, folderId, targetParentId)) return false;
  const from = space.folders.findIndex((f) => f.id === folderId);
  if (from < 0) return false;
  const [folder] = space.folders.splice(from, 1);
  folder.parentId = targetParentId;
  if (targetParentId) {
    const parent = space.folders.find((f) => f.id === targetParentId);
    if (parent) parent.collapsed = false;
  }
  if (anchorId && anchorId !== folderId) {
    const idx = space.folders.findIndex((f) => f.id === anchorId);
    if (idx >= 0) {
      space.folders.splice(where === "after" ? idx + 1 : idx, 0, folder);
      return true;
    }
  }
  space.folders.push(folder);
  return true;
}

export function toggleFolder(store: BrowserStore, folderId: string, collapsed?: boolean) {
  const folder = store.spaces.flatMap((s) => s.folders ?? []).find((f) => f.id === folderId);
  if (folder) folder.collapsed = collapsed ?? !folder.collapsed;
}

export function renameFolder(store: BrowserStore, folderId: string, name: string) {
  const folder = store.spaces
    .flatMap((s) => s.folders ?? [])
    .find((f) => f.id === folderId);
  if (folder && name.trim()) folder.name = name.trim();
}

export function setFolderIcon(store: BrowserStore, folderId: string, icon?: string) {
  const folder = store.spaces
    .flatMap((s) => s.folders ?? [])
    .find((f) => f.id === folderId);
  if (folder) folder.icon = icon?.trim() || undefined;
}

/** Tabs inside `folder` and all of its nested subfolders, in display order. */
export function folderDescendantTabs(store: BrowserStore, space: Space, folder: Folder): TabId[] {
  const folders = space.folders ?? [];
  const byId = new Map(folders.map((f) => [f.id, f]));
  const out: TabId[] = [];
  for (const id of subtreeFolderIds(folders, folder.id)) {
    const f = byId.get(id);
    if (f) out.push(...store.folderMembers(space, f));
  }
  return out;
}

/** Removes a folder. If `closeTabs` is false, child folders promote up and tabs stay pinned;
 *  if `closeTabs` is true, the folder, its subfolders, and all their tabs are deleted. */
export function deleteFolder(store: BrowserStore, folderId: string, closeTabs = false) {
  for (const space of store.spaces) {
    const folders = space.folders;
    const folder = folders?.find((f) => f.id === folderId);
    if (!folder || !folders) continue;
    if (closeTabs) {
      const ids = new Set(subtreeFolderIds(folders, folderId));
      const members = folderDescendantTabs(store, space, folder);
      space.folders = folders.filter((f) => !ids.has(f.id));
      for (const id of members) store.removeTab(id, true);
    } else {
      for (const child of folders) {
        if (child.parentId === folderId) child.parentId = folder.parentId;
      }
      space.folders = folders.filter((f) => f.id !== folderId);
    }
  }
}
