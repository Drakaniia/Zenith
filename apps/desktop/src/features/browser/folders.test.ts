import test from "node:test";
import assert from "node:assert/strict";
import {
  MAX_FOLDER_DEPTH,
  canNestFolder,
  childFolders,
  folderDepth,
  folderSubtreeHeight,
  isDescendantFolder,
  subtreeFolderIds,
  walkFolders,
} from "./folder-tree.ts";
import type { Folder } from "./types.ts";

const mk = (id: string, parentId?: string): Folder => ({
  id,
  name: id,
  collapsed: false,
  tabIds: [],
  parentId,
});

test("folderDepth and childFolders handle nested chains and orphan parents", () => {
  const folders = [mk("a"), mk("b", "a"), mk("c", "b"), mk("orphan", "missing")];
  assert.equal(folderDepth(folders, "a"), 0);
  assert.equal(folderDepth(folders, "b"), 1);
  assert.equal(folderDepth(folders, "c"), 2);
  assert.deepEqual(childFolders(folders).map((f) => f.id), ["a", "orphan"]);
  assert.deepEqual(childFolders(folders, "a").map((f) => f.id), ["b"]);
});

test("canNestFolder rejects cycles and enforces max depth", () => {
  const chain = Array.from({ length: MAX_FOLDER_DEPTH }, (_, i) =>
    mk(`f${i}`, i > 0 ? `f${i - 1}` : undefined),
  );
  const extra = mk("extra");
  const all = [...chain, extra];

  assert.equal(isDescendantFolder(all, "f3", "f1"), true);
  assert.equal(canNestFolder(all, "f0", "f2"), false, "rejects cycle");
  assert.equal(canNestFolder(all, "f1", "f1"), false, "rejects self");
  assert.equal(canNestFolder(all, "extra", `f${MAX_FOLDER_DEPTH - 1}`), false, "rejects exceeding depth 5");
  assert.equal(canNestFolder(all, "extra", `f${MAX_FOLDER_DEPTH - 2}`), true, "allows depth 4");

  const pair = [mk("p1"), mk("p2", "p1")];
  assert.equal(folderSubtreeHeight(pair, "p1"), 1);
  assert.equal(canNestFolder([...chain, ...pair], "p1", `f${MAX_FOLDER_DEPTH - 2}`), false, "counts subtree height");
  assert.equal(canNestFolder([...chain, ...pair], "p1", `f${MAX_FOLDER_DEPTH - 3}`), true);
});

test("subtreeFolderIds and walkFolders follow visual tree order", () => {
  const folders = [mk("a"), mk("b"), mk("a1", "a"), mk("a1i", "a1"), mk("a2", "a")];
  assert.deepEqual(subtreeFolderIds(folders, "a"), ["a", "a1", "a1i", "a2"]);
  assert.deepEqual(walkFolders(folders).map((f) => f.id), ["a", "a1", "a1i", "a2", "b"]);
});
