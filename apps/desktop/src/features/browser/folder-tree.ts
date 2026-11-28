// Pure folder tree math (nested pinned folders, max 5 levels). Tested directly under Node.
import type { Folder } from "./types.ts";

export const MAX_FOLDER_DEPTH = 5;

/** Depth of a folder (0 for top-level). Guards against malformed cycles. */
export function folderDepth(folders: readonly Folder[], folderId: string): number {
  const byId = new Map(folders.map((f) => [f.id, f]));
  let depth = 0;
  let cur = byId.get(folderId)?.parentId;
  const seen = new Set<string>([folderId]);
  while (cur && !seen.has(cur) && depth < MAX_FOLDER_DEPTH + 1) {
    seen.add(cur);
    depth++;
    cur = byId.get(cur)?.parentId;
  }
  return depth;
}

/** Direct children of `parentId` (or top-level folders when omitted), in stored order. */
export function childFolders(folders: readonly Folder[], parentId?: string): Folder[] {
  const ids = new Set(folders.map((f) => f.id));
  return folders.filter((f) =>
    parentId ? f.parentId === parentId : !f.parentId || !ids.has(f.parentId),
  );
}

/** Maximum depth below `folderId` (0 if it has no child folders). */
export function folderSubtreeHeight(
  folders: readonly Folder[],
  folderId: string,
  seen = new Set<string>(),
): number {
  if (seen.has(folderId)) return 0;
  seen.add(folderId);
  let max = 0;
  for (const child of childFolders(folders, folderId)) {
    max = Math.max(max, 1 + folderSubtreeHeight(folders, child.id, seen));
  }
  return max;
}

/** True if `candidateId` is `ancestorId` itself or nested inside it. */
export function isDescendantFolder(
  folders: readonly Folder[],
  candidateId: string,
  ancestorId: string,
): boolean {
  const byId = new Map(folders.map((f) => [f.id, f]));
  let cur: string | undefined = candidateId;
  const seen = new Set<string>();
  while (cur && !seen.has(cur)) {
    if (cur === ancestorId) return true;
    seen.add(cur);
    cur = byId.get(cur)?.parentId;
  }
  return false;
}

/** True if `folderId` can be placed under `targetParentId` without cycles or exceeding max depth. */
export function canNestFolder(
  folders: readonly Folder[],
  folderId: string,
  targetParentId: string | undefined,
): boolean {
  if (!targetParentId) return true;
  if (isDescendantFolder(folders, targetParentId, folderId)) return false;
  const newDepth = folderDepth(folders, targetParentId) + 1;
  return newDepth + folderSubtreeHeight(folders, folderId) < MAX_FOLDER_DEPTH;
}

/** Folder id followed by every descendant folder id in display order. */
export function subtreeFolderIds(
  folders: readonly Folder[],
  folderId: string,
  seen = new Set<string>(),
): string[] {
  if (seen.has(folderId)) return [];
  seen.add(folderId);
  const out = [folderId];
  for (const child of childFolders(folders, folderId)) {
    out.push(...subtreeFolderIds(folders, child.id, seen));
  }
  return out;
}

/** All folders of a space flattened in visual tree order (roots then their descendants). */
export function walkFolders(folders: readonly Folder[]): Folder[] {
  const byId = new Map(folders.map((f) => [f.id, f]));
  const seen = new Set<string>();
  const out: Folder[] = [];
  for (const root of childFolders(folders)) {
    for (const id of subtreeFolderIds(folders, root.id, seen)) {
      const f = byId.get(id);
      if (f) out.push(f);
    }
  }
  return out;
}
