// The bookmark tree for the UI: the nodes the backend returns, indexed, and the operations the
// toolbar, sidebar, Library and popups share. Every change goes through the backend and the list
// is reloaded after it; other windows reload on `bookmarks-changed`. The tree logic itself
// (lookup, search, drag-move planning) is in bookmarks.ts.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { bookmarksApi } from "./api";
import { faviconsApi } from "$shared/favicons-api";
import { on } from "$platform/ipc";
import { browser } from "$features/browser/browser.svelte";
import { settings } from "$features/settings/settings.svelte";
import { fetchMissingFavicons } from "$shared/favicons";
import {
  ROOT_TOOLBAR,
  ancestors,
  bookmarkTitle,
  buildIndex,
  inSpace,
  kids,
  planMoves,
  recentBookmarks,
  sortedByName,
  tagCounts,
  toTree,
} from "./bookmarks";
import type { BookmarkNode, BookmarkPatch, NewBookmarkNode } from "./types";

/** What deleting a node took away, so that "Undo" can put it back where it was. */
export interface RemovedNode {
  /** The id the node had (later entries that name it follow it to the new one). */
  id?: number;
  parent: number;
  index: number;
  tree: NewBookmarkNode;
}

export interface NewBookmarkOptions {
  parent?: number;
  index?: number;
  tags?: string[];
  keyword?: string;
  spaces?: string[];
}

interface Entry {
  label: string;
  undo(): Promise<unknown>;
  redo(): Promise<unknown>;
  /** The deletion this entry belongs to, to find it again from its toast. */
  ref?: RemovedNode[];
}

class BookmarkStore {
  /** Replaced as a whole on every load, so it needn't be a deep proxy. */
  nodes = $state.raw<BookmarkNode[]>([]);
  loaded = $state(false);
  index = $derived(buildIndex(this.nodes));
  /** Number of bookmarks (folders and separators don't count). */
  count = $derived(this.index.count);
  /** Every bookmarked address, for the star buttons. */
  urls = $derived(new Set(this.index.byUrl.keys()));
  tags = $derived(tagCounts(this.nodes));

  #loading: Promise<void> | null = null;
  #seq = 0;

  /** Loads the tree once per window; `force` loads it again (after a change). */
  load(force = false): Promise<void> {
    if (!force && this.#loading) return this.#loading;
    this.#loading = this.#fetch();
    return this.#loading;
  }

  async #fetch() {
    // Overlapping loads can finish out of order: only the latest request's answer counts.
    const mine = ++this.#seq;
    try {
      const list = await bookmarksApi.bookmarksList();
      if (mine === this.#seq) {
        this.nodes = list;
        this.loaded = true;
        void this.#icons(list);
      }
    } catch (e) {
      console.warn("bookmarks unavailable", e);
    }
  }

  /** Icons for bookmarks whose site has none yet; the lists redraw when some arrive. */
  async #icons(list: BookmarkNode[]) {
    const urls = list.filter((n) => n.kind === "bookmark").map((n) => n.url);
    const got = await fetchMissingFavicons(urls, (u) => faviconsApi.faviconFetch(u));
    // A new array of the same nodes: everything drawn from the list asks for its icon again.
    if (got > 0) this.nodes = [...this.nodes];
  }

  /** Reloads when another window changes the bookmarks. Returns the unsubscribe. */
  async listen(): Promise<() => void> {
    const self = getCurrentWebviewWindow().label;
    return on("bookmarks-changed", (e) => {
      if (e.origin !== self) void this.load(true);
    });
  }

  // ------------------------------------------------------------------------------------------
  // Lookups

  has(url: string | undefined | null): boolean {
    return !!url && this.urls.has(url);
  }

  /** The bookmarks of an address, most recently changed first. */
  find(url: string | undefined | null): readonly BookmarkNode[] {
    return (url && this.index.byUrl.get(url)) || [];
  }

  node(id: number | null | undefined): BookmarkNode | undefined {
    return id === null || id === undefined ? undefined : this.index.byId.get(id);
  }

  kids(id: number): readonly BookmarkNode[] {
    return kids(this.index, id);
  }

  /** The children of a folder that show in the active workspace (Zen: workspace bookmarks). */
  visibleKids(id: number): BookmarkNode[] {
    const spaceId = browser.activeSpaceId;
    const live = new Set(browser.spaces.map((s) => s.id));
    return kids(this.index, id).filter((n) => inSpace(n, spaceId, live));
  }

  recent(count: number): BookmarkNode[] {
    return recentBookmarks(this.index, count);
  }

  /** Where a new bookmark goes: the folder picked last, if it still exists. */
  defaultParent(): number {
    const folder = this.index.byId.get(settings.value.lastBookmarkFolder);
    return folder?.kind === "folder" ? folder.id : ROOT_TOOLBAR;
  }

  rememberFolder(id: number) {
    if (settings.value.lastBookmarkFolder !== id) settings.update({ lastBookmarkFolder: id });
  }

  // ------------------------------------------------------------------------------------------
  // Changes

  // ------------------------------------------------------------------------------------------
  // Undo and redo (the Library's Ctrl+Z / Ctrl+Y): each change keeps how to take it back and do it
  // again. Ids change when a deleted node is put back, so the entries that name them keep the
  // current ones in a list they update.

  /** A node put back (an undone delete, a redone add) gets a new id: later entries that name the old
   *  one follow it here. */
  #alias = new Map<number, number>();

  #id(id: number): number {
    let cur = id;
    for (let i = 0; i < 40 && this.#alias.has(cur); i++) cur = this.#alias.get(cur)!;
    return cur;
  }

  #undone: Entry[] = [];
  #redone: Entry[] = [];
  #replaying = false;
  canUndo = $state(false);
  canRedo = $state(false);
  /** What the next undo or redo would do, for the menu ("Undo move"). */
  undoLabel = $state("");
  redoLabel = $state("");

  #sync() {
    this.canUndo = this.#undone.length > 0;
    this.canRedo = this.#redone.length > 0;
    this.undoLabel = this.#undone[this.#undone.length - 1]?.label ?? "";
    this.redoLabel = this.#redone[this.#redone.length - 1]?.label ?? "";
  }

  #record(entry: Entry) {
    if (this.#replaying) return;
    this.#undone.push(entry);
    if (this.#undone.length > 60) this.#undone.shift();
    this.#redone = [];
    this.#sync();
  }

  async #replay(from: Entry[], to: Entry[], way: "undo" | "redo"): Promise<boolean> {
    const entry = from.pop();
    if (!entry) return false;
    this.#replaying = true;
    try {
      await entry[way]();
      to.push(entry);
    } catch (e) {
      // The nodes it names are gone (another window, a restore): this entry can't be replayed.
      console.warn(`bookmark ${way} failed`, e);
    } finally {
      this.#replaying = false;
      await this.load(true);
      this.#sync();
    }
    return true;
  }

  undo() {
    return this.#replay(this.#undone, this.#redone, "undo");
  }

  redo() {
    return this.#replay(this.#redone, this.#undone, "redo");
  }

  /** Takes back one particular deletion (the toast's "Undo"), wherever it is in the history. */
  async undoRemoval(removed: RemovedNode[]): Promise<void> {
    const at = this.#undone.findIndex((e) => e.ref === removed);
    if (at < 0) return this.restore(removed);
    const [entry] = this.#undone.splice(at, 1);
    this.#replaying = true;
    try {
      await entry.undo();
      this.#redone.push(entry);
    } finally {
      this.#replaying = false;
      await this.load(true);
      this.#sync();
    }
  }

  async add(parent: number, nodes: NewBookmarkNode[], index?: number): Promise<number[]> {
    const ids = await bookmarksApi.bookmarksAdd(parent, index ?? null, nodes);
    await this.load(true);
    if (ids.length) {
      let now = ids;
      let at = this.index.byId.get(ids[0])?.position ?? null;
      this.#record({
        label: nodes.length === 1 && nodes[0].kind === "folder" ? "new folder" : "add",
        undo: () => bookmarksApi.bookmarksRemove(now.map((n) => this.#id(n))),
        redo: async () => {
          const fresh = await bookmarksApi.bookmarksAdd(this.#id(parent), at, nodes);
          fresh.forEach((n, i) => now[i] !== undefined && this.#alias.set(now[i], n));
          now = fresh;
          at = null;
        },
      });
    }
    return ids;
  }

  async addBookmark(url: string, title: string, opts: NewBookmarkOptions = {}): Promise<number> {
    const [id] = await this.add(
      opts.parent ?? this.defaultParent(),
      [
        {
          kind: "bookmark",
          url,
          title: bookmarkTitle(title, url),
          tags: opts.tags,
          keyword: opts.keyword,
          spaces: opts.spaces,
        },
      ],
      opts.index,
    );
    return id;
  }

  async addFolder(parent: number, title: string, index?: number): Promise<number> {
    const [id] = await this.add(parent, [{ kind: "folder", title }], index);
    return id;
  }

  async addSeparator(parent: number, index?: number): Promise<number> {
    const [id] = await this.add(parent, [{ kind: "separator" }], index);
    return id;
  }

  async update(id: number, patch: BookmarkPatch) {
    const n = this.index.byId.get(id);
    const before: BookmarkPatch = {};
    if (n) {
      if (patch.title !== undefined) before.title = n.title;
      if (patch.url !== undefined) before.url = n.url;
      if (patch.tags !== undefined) before.tags = [...n.tags];
      if (patch.keyword !== undefined) before.keyword = n.keyword;
      if (patch.spaces !== undefined) before.spaces = [...n.spaces];
    }
    await bookmarksApi.bookmarkUpdate(id, patch);
    await this.load(true);
    if (n) this.#record({ label: "edit", undo: () => bookmarksApi.bookmarkUpdate(this.#id(id), before), redo: () => bookmarksApi.bookmarkUpdate(this.#id(id), patch) });
  }

  async move(id: number, parent: number, index: number) {
    const n = this.index.byId.get(id);
    await bookmarksApi.bookmarkMove(id, parent, index);
    await this.load(true);
    if (n?.parent != null) {
      const from = { parent: n.parent, index: n.position };
      this.#record({
        label: "move",
        undo: () => bookmarksApi.bookmarkMove(this.#id(id), this.#id(from.parent), from.index),
        redo: () => bookmarksApi.bookmarkMove(this.#id(id), this.#id(parent), index),
      });
    }
  }

  /** Drag and drop: the nodes land together at `index` (as drawn, before they leave) of `parent`.
   *  False when nothing needed to move. */
  async moveTo(ids: number[], parent: number, index: number): Promise<boolean> {
    const plans = planMoves(this.index, ids, parent, index);
    if (!plans.length) return false;
    const from = plans.map((p) => ({ id: p.id, parent: this.index.byId.get(p.id)?.parent ?? p.parent, index: this.index.byId.get(p.id)?.position ?? 0 }));
    try {
      for (const p of plans) await bookmarksApi.bookmarkMove(p.id, p.parent, p.index);
    } finally {
      await this.load(true);
    }
    this.#record({
      label: "move",
      undo: async () => {
        for (const f of [...from].reverse()) await bookmarksApi.bookmarkMove(this.#id(f.id), this.#id(f.parent), f.index);
      },
      redo: async () => {
        for (const p of plans) await bookmarksApi.bookmarkMove(this.#id(p.id), this.#id(p.parent), p.index);
      },
    });
    return true;
  }

  async reorder(parent: number, order: number[]) {
    await bookmarksApi.bookmarksReorder(parent, order);
    await this.load(true);
  }

  async sortByName(parent: number) {
    const order = sortedByName(this.index, parent);
    const current = kids(this.index, parent).map((n) => n.id);
    if (order.every((id, i) => id === current[i])) return;
    await this.reorder(parent, order);
  }

  /** Deletes nodes (folders with everything in them) and returns what "Undo" needs. */
  async remove(ids: number[]): Promise<RemovedNode[]> {
    const picked = new Set(ids);
    const removed: RemovedNode[] = [];
    for (const id of ids) {
      const node = this.index.byId.get(id);
      if (!node || node.parent === null) continue;
      // A node inside another picked folder goes with it and is part of its snapshot.
      if (ancestors(this.index, id).some((a) => picked.has(a.id))) continue;
      const tree = toTree(this.index, id);
      if (tree) removed.push({ id, parent: node.parent, index: node.position, tree });
    }
    await bookmarksApi.bookmarksRemove(ids);
    await this.load(true);
    if (removed.length) {
      let gone = ids;
      this.#record({
        label: "delete",
        ref: removed,
        undo: async () => {
          const back: number[] = [];
          for (const r of [...removed].sort((a, b) => a.index - b.index)) {
            const [fresh] = await bookmarksApi.bookmarksAdd(this.#id(r.parent), r.index, [r.tree]);
            if (r.id !== undefined && fresh !== undefined) this.#alias.set(r.id, fresh);
            back.push(fresh);
          }
          gone = back;
        },
        redo: () => bookmarksApi.bookmarksRemove(gone.map((n) => this.#id(n))),
      });
    }
    return removed;
  }

  /** Puts deleted nodes back, each at the position it had. */
  async restore(removed: RemovedNode[]) {
    for (const r of [...removed].sort((a, b) => a.index - b.index)) {
      await bookmarksApi.bookmarksAdd(r.parent, r.index, [r.tree]);
    }
    await this.load(true);
  }

  // ------------------------------------------------------------------------------------------
  // Cut, copy and paste (inside the app: a cut moves, a copy makes new ones)

  clip = $state<{ trees: NewBookmarkNode[]; cut: number[] | null } | null>(null);

  copyNodes(ids: number[], cut = false) {
    const keep = ids.filter((id) => this.index.byId.get(id)?.parent != null);
    const trees = keep.map((id) => toTree(this.index, id)).filter((t): t is NewBookmarkNode => !!t);
    this.clip = trees.length ? { trees, cut: cut ? keep : null } : null;
    return trees.length;
  }

  /** Pastes into a folder (at `index`, or the end). A cut moves the nodes, once. */
  async paste(parent: number, index?: number): Promise<number> {
    const clip = this.clip;
    if (!clip) return 0;
    if (clip.cut) {
      const ids = clip.cut.filter((id) => this.index.byId.has(id));
      this.clip = null;
      const at = index ?? this.kids(parent).length;
      return (await this.moveTo(ids, parent, at)) ? ids.length : 0;
    }
    const ids = await this.add(parent, clip.trees, index);
    return ids.length;
  }

  /** The star button: removes every bookmark of an address. */
  async removeUrl(url: string) {
    await bookmarksApi.bookmarksRemoveUrl(url);
    await this.load(true);
  }

  // ------------------------------------------------------------------------------------------
  // Files

  async importHtml(): Promise<number> {
    const n = await bookmarksApi.bookmarksImport();
    if (n > 0) await this.load(true);
    return n;
  }

  /** Replaces everything with a backup file's; null when the dialog was cancelled. */
  async restoreFile(): Promise<number | null> {
    const n = await bookmarksApi.bookmarksRestore();
    if (n !== null) await this.load(true);
    return n;
  }

  async restoreBackup(name: string): Promise<number> {
    const n = await bookmarksApi.bookmarksRestoreBackup(name);
    await this.load(true);
    return n;
  }
}

export const bookmarks = new BookmarkStore();
