// Bookmark tree helpers: pure functions over the node list the backend returns, so the tree
// logic (lookup, search, keywords, workspace visibility, drag-and-drop planning) can be tested
// without the UI. The reactive store that holds the nodes is bookmarks.svelte.ts. Like the other
// pure modules it has no runtime imports, so Node's test runner can load it as it is.
import type { BookmarkNode, NewBookmarkNode } from "../../shared/types";

/** Ids of the three built-in folders (fixed by the backend). Zen/Firefox call them the
 *  Bookmarks Toolbar (shown in the window), the Bookmarks Menu (reached from the sidebar,
 *  the Library and the app menu) and Other Bookmarks (unsorted ones). */
export const ROOT_TOOLBAR = 1;
export const ROOT_MENU = 2;
export const ROOT_OTHER = 3;
export const ROOTS = [ROOT_TOOLBAR, ROOT_MENU, ROOT_OTHER] as const;
export const ROOT_TITLES: Record<number, string> = {
  [ROOT_TOOLBAR]: "Bookmarks Toolbar",
  [ROOT_MENU]: "Bookmarks Menu",
  [ROOT_OTHER]: "Other Bookmarks",
};

export interface BookmarkIndex {
  byId: Map<number, BookmarkNode>;
  /** Children of each folder, in display order. */
  children: Map<number, BookmarkNode[]>;
  /** Bookmarks by exact address, most recently changed first. */
  byUrl: Map<string, BookmarkNode[]>;
  byKeyword: Map<string, BookmarkNode>;
  /** Number of bookmarks (folders and separators don't count). */
  count: number;
}

export function buildIndex(nodes: readonly BookmarkNode[]): BookmarkIndex {
  const byId = new Map<number, BookmarkNode>();
  const children = new Map<number, BookmarkNode[]>();
  const byUrl = new Map<string, BookmarkNode[]>();
  const byKeyword = new Map<string, BookmarkNode>();
  let count = 0;
  for (const n of nodes) {
    byId.set(n.id, n);
    if (n.parent !== null) {
      const siblings = children.get(n.parent);
      if (siblings) siblings.push(n);
      else children.set(n.parent, [n]);
    }
    if (n.kind === "bookmark") {
      count++;
      const same = byUrl.get(n.url);
      if (same) same.push(n);
      else byUrl.set(n.url, [n]);
      if (n.keyword) byKeyword.set(n.keyword.toLowerCase(), n);
    }
  }
  for (const list of children.values()) list.sort((a, b) => a.position - b.position || a.id - b.id);
  for (const list of byUrl.values()) list.sort((a, b) => b.modified - a.modified || b.id - a.id);
  return { byId, children, byUrl, byKeyword, count };
}

const NONE: readonly BookmarkNode[] = [];

export function kids(ix: BookmarkIndex, id: number): readonly BookmarkNode[] {
  return ix.children.get(id) ?? NONE;
}

export const isRoot = (n: Pick<BookmarkNode, "parent">) => n.parent === null;
export const isFolder = (n: Pick<BookmarkNode, "kind"> | undefined) => n?.kind === "folder";

/** An address without scheme, "www." and trailing slash (url.ts's `prettyUrl`, kept here so this
 *  module stays free of runtime imports). */
export function shortUrl(url: string): string {
  return url
    .replace(/^https?:\/\//, "")
    .replace(/^www\./, "")
    .replace(/\/$/, "");
}

/** What a node is called in lists: its title, or its address when it has none. */
export function nodeLabel(n: Pick<BookmarkNode, "title" | "url" | "kind">): string {
  if (n.title.trim()) return n.title;
  if (n.kind === "bookmark") return shortUrl(n.url);
  return n.kind === "folder" ? "Untitled folder" : "";
}

/** The title to bookmark a page under: the page's own, or its address. */
export function bookmarkTitle(title: string | undefined, url: string): string {
  return title?.trim() || shortUrl(url);
}

/** The folders above a node, from its root down to its parent. */
export function ancestors(ix: BookmarkIndex, id: number): BookmarkNode[] {
  const out: BookmarkNode[] = [];
  const seen = new Set<number>([id]);
  let cur = ix.byId.get(id);
  while (cur && cur.parent !== null && !seen.has(cur.parent)) {
    const up = ix.byId.get(cur.parent);
    if (!up) break;
    seen.add(up.id);
    out.push(up);
    cur = up;
  }
  return out.reverse();
}

/** "Bookmarks Toolbar / Dev / Tools" for a folder; for any other node, the folder holding it. */
export function folderPath(ix: BookmarkIndex, id: number, sep = " / "): string {
  const n = ix.byId.get(id);
  if (!n) return "";
  const chain = ancestors(ix, id).map((a) => a.title || ROOT_TITLES[a.id] || "");
  if (n.kind === "folder") chain.push(n.title || ROOT_TITLES[n.id] || "Untitled folder");
  return chain.join(sep);
}

/** True when `id` is the folder itself or lies anywhere below it. */
export function isInside(ix: BookmarkIndex, id: number, folderId: number): boolean {
  const seen = new Set<number>();
  let cur = ix.byId.get(id);
  while (cur && !seen.has(cur.id)) {
    if (cur.id === folderId) return true;
    seen.add(cur.id);
    cur = cur.parent === null ? undefined : ix.byId.get(cur.parent);
  }
  return false;
}

/** Everything below a folder, parents before their children, in display order. */
export function descendants(ix: BookmarkIndex, id: number): BookmarkNode[] {
  const out: BookmarkNode[] = [];
  const walk = (folder: number) => {
    for (const c of kids(ix, folder)) {
      out.push(c);
      if (c.kind === "folder") walk(c.id);
    }
  };
  walk(id);
  return out;
}

/** The bookmarks directly in a folder (what "Open all in tabs" opens). */
export function bookmarksIn(ix: BookmarkIndex, id: number): BookmarkNode[] {
  return kids(ix, id).filter((n) => n.kind === "bookmark");
}

/** Bookmarks in a folder and all its subfolders. */
export function countBookmarks(ix: BookmarkIndex, id: number): number {
  return descendants(ix, id).filter((n) => n.kind === "bookmark").length;
}

// ---------------------------------------------------------------------------------------------
// Workspaces

/** Zen's workspace bookmarks: one that names workspaces shows only in those. Ids of workspaces
 *  that no longer exist are ignored, so a bookmark whose workspaces were all deleted (or that
 *  came from a backup made elsewhere) never disappears from the toolbar. */
export function inSpace(
  n: Pick<BookmarkNode, "spaces">,
  spaceId: string | undefined,
  existing: ReadonlySet<string>,
): boolean {
  if (!n.spaces.length) return true;
  const live = n.spaces.filter((s) => existing.has(s));
  if (!live.length) return true;
  return !!spaceId && live.includes(spaceId);
}

// ---------------------------------------------------------------------------------------------
// Tags

/** "news, Work ,news" -> ["news", "Work"]: trimmed, no empty ones, case-insensitive duplicates
 *  dropped (the first spelling stays). The backend enforces the same limits. */
export function parseTags(input: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const raw of input.split(/[,\n]/)) {
    const tag = raw.trim().replace(/\s+/g, " ").slice(0, 64);
    const key = tag.toLowerCase();
    if (!tag || seen.has(key)) continue;
    seen.add(key);
    out.push(tag);
    if (out.length >= 50) break;
  }
  return out;
}

export const formatTags = (tags: readonly string[]) => tags.join(", ");

/** Every tag in use with how many bookmarks carry it, alphabetical. */
export function tagCounts(nodes: readonly BookmarkNode[]): { tag: string; count: number }[] {
  const map = new Map<string, { tag: string; count: number }>();
  for (const n of nodes) {
    if (n.kind !== "bookmark") continue;
    for (const tag of n.tags) {
      const key = tag.toLowerCase();
      const hit = map.get(key);
      if (hit) hit.count++;
      else map.set(key, { tag, count: 1 });
    }
  }
  return [...map.values()].sort((a, b) => a.tag.localeCompare(b.tag, undefined, { sensitivity: "base" }));
}

export const hasTag = (n: Pick<BookmarkNode, "tags">, tag: string) =>
  n.tags.some((t) => t.toLowerCase() === tag.toLowerCase());

// ---------------------------------------------------------------------------------------------
// Search

const BOUNDARY = /[\s/._\-:?&=#]/;

/** A word's score in a text: it must occur as a substring; word starts and early hits rank higher. */
function termScore(term: string, text: string): number {
  if (!text) return 0;
  const t = text.toLowerCase();
  const at = t.indexOf(term);
  if (at < 0) return 0;
  const boundary = at === 0 || BOUNDARY.test(t[at - 1]);
  return 500 - Math.min(at, 200) + (boundary ? 200 : 0) - Math.min(t.length, 300) * 0.1;
}

/** How well a bookmark matches a query (0: not at all). Every word must be found in the title,
 *  address, tags or keyword, in any order, like the address bar of Firefox and Zen. */
export function scoreBookmark(
  query: string,
  n: Pick<BookmarkNode, "title" | "url" | "tags" | "keyword">,
): number {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return 1;
  const tags = n.tags.join(" ");
  let total = 0;
  for (const term of terms) {
    const best = Math.max(
      termScore(term, n.title),
      termScore(term, tags) * 0.9,
      termScore(term, n.keyword),
      termScore(term, n.url) * 0.7,
    );
    if (best <= 0) return 0;
    total += best;
  }
  if (n.keyword && n.keyword.toLowerCase() === query.trim().toLowerCase()) total += 5000;
  return total;
}

export function searchBookmarks(
  ix: BookmarkIndex,
  query: string,
  opts: { limit?: number; within?: number } = {},
): BookmarkNode[] {
  const scored: { n: BookmarkNode; s: number }[] = [];
  for (const n of ix.byId.values()) {
    if (n.kind !== "bookmark") continue;
    if (opts.within !== undefined && !isInside(ix, n.id, opts.within)) continue;
    const s = scoreBookmark(query, n);
    if (s > 0) scored.push({ n, s });
  }
  scored.sort((a, b) => b.s - a.s || compareText(nodeLabel(a.n), nodeLabel(b.n)));
  const list = scored.map((x) => x.n);
  return opts.limit ? list.slice(0, opts.limit) : list;
}

/** Firefox's address-bar restriction tokens: a lone `*`, `+`, `^` or `%` before (or after) the words
 *  limits the results to bookmarks, tagged bookmarks, history or open tabs. */
export type Restriction = "bookmarks" | "tagged" | "history" | "tabs";
const TOKENS: Record<string, Restriction> = { "*": "bookmarks", "+": "tagged", "^": "history", "%": "tabs" };

export function parseRestriction(query: string): { restriction: Restriction | null; rest: string } {
  const words = query.trim().split(/\s+/).filter(Boolean);
  for (const at of [0, words.length - 1]) {
    const r = TOKENS[words[at] ?? ""];
    if (r) return { restriction: r, rest: words.filter((_, i) => i !== at).join(" ") };
  }
  // "*foo": the token glued to the first word, as the palette always allowed for `*`.
  const first = words[0]?.[0] ?? "";
  if (words.length && TOKENS[first] && words[0].length > 1) {
    return { restriction: TOKENS[first], rest: [words[0].slice(1), ...words.slice(1)].join(" ") };
  }
  return { restriction: null, rest: query.trim() };
}

/** Bookmarks whose tags contain the words ("+ news" finds the ones tagged "newsletter" too); with no
 *  words, every tagged bookmark. */
export function searchTagged(ix: BookmarkIndex, words: string, limit = 12): BookmarkNode[] {
  const terms = words.toLowerCase().split(/\s+/).filter(Boolean);
  const out: BookmarkNode[] = [];
  for (const n of ix.byId.values()) {
    if (n.kind !== "bookmark" || !n.tags.length) continue;
    const tags = n.tags.map((t) => t.toLowerCase());
    if (terms.every((t) => tags.some((g) => g.includes(t)))) out.push(n);
  }
  out.sort((a, b) => compareText(nodeLabel(a), nodeLabel(b)));
  return out.slice(0, limit);
}

export interface KeywordHit {
  node: BookmarkNode;
  /** Where it goes: the bookmark's address, with `%s` replaced by the text after the keyword. */
  url: string;
}

/** Typing a bookmark's keyword in the address bar opens it ("w" or "w some search" for a
 *  bookmark whose address has a %s placeholder). */
export function keywordMatch(ix: BookmarkIndex, text: string): KeywordHit | null {
  const input = text.trim();
  if (!input || !ix.byKeyword.size) return null;
  const space = input.search(/\s/);
  const word = (space < 0 ? input : input.slice(0, space)).toLowerCase();
  const rest = space < 0 ? "" : input.slice(space).trim();
  const node = ix.byKeyword.get(word);
  if (!node) return null;
  const placeholder = node.url.includes("%s");
  if (rest && !placeholder) return null;
  return { node, url: placeholder ? node.url.split("%s").join(encodeURIComponent(rest)) : node.url };
}

// ---------------------------------------------------------------------------------------------
// Lists

/** Newest bookmarks first (the app menu's "Recent bookmarks"). */
export function recentBookmarks(ix: BookmarkIndex, count: number): BookmarkNode[] {
  return [...ix.byId.values()]
    .filter((n) => n.kind === "bookmark")
    .sort((a, b) => b.created - a.created || b.id - a.id)
    .slice(0, count);
}

/** Folders the user filed bookmarks into lately (not the built-in ones), newest first. */
export function recentFolders(ix: BookmarkIndex, count: number): BookmarkNode[] {
  const seen = new Set<number>();
  const out: BookmarkNode[] = [];
  for (const b of recentBookmarks(ix, 200)) {
    if (b.parent === null || seen.has(b.parent)) continue;
    seen.add(b.parent);
    const folder = ix.byId.get(b.parent);
    if (folder && folder.kind === "folder" && folder.parent !== null) out.push(folder);
    if (out.length >= count) break;
  }
  return out;
}

export interface FolderOption {
  id: number;
  title: string;
  depth: number;
}

/** Every folder, indented as a tree (roots first), for the location pickers. `exclude` leaves out
 *  a folder and everything below it (a folder can't be moved into itself). */
export function folderOptions(ix: BookmarkIndex, opts: { exclude?: number } = {}): FolderOption[] {
  const out: FolderOption[] = [];
  const walk = (id: number, depth: number) => {
    const n = ix.byId.get(id);
    if (!n || n.kind !== "folder" || id === opts.exclude) return;
    out.push({ id, title: n.title || ROOT_TITLES[id] || "Untitled folder", depth });
    for (const c of kids(ix, id)) if (c.kind === "folder") walk(c.id, depth + 1);
  };
  for (const root of ROOTS) walk(root, 0);
  return out;
}

export function compareText(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: "base", numeric: true });
}

/** Child ids of a folder in name order: folders first, then bookmarks. Separators stay where
 *  they are and each stretch between two of them is sorted on its own. */
export function sortedByName(ix: BookmarkIndex, parent: number): number[] {
  const out: number[] = [];
  let run: BookmarkNode[] = [];
  const flush = () => {
    run.sort(
      (a, b) =>
        (a.kind === b.kind ? 0 : a.kind === "folder" ? -1 : 1) || compareText(nodeLabel(a), nodeLabel(b)),
    );
    for (const n of run) out.push(n.id);
    run = [];
  };
  for (const n of kids(ix, parent)) {
    if (n.kind === "separator") {
      flush();
      out.push(n.id);
    } else run.push(n);
  }
  flush();
  return out;
}

/** The default name for a folder made from open tabs. */
export function tabsFolderName(date = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `Tabs ${date.getFullYear()}-${p(date.getMonth() + 1)}-${p(date.getDate())}`;
}

/** A node and everything below it as a creation request (for Undo after deleting). */
export function toTree(ix: BookmarkIndex, id: number): NewBookmarkNode | null {
  const n = ix.byId.get(id);
  if (!n) return null;
  const tree: NewBookmarkNode = { kind: n.kind, title: n.title };
  if (n.kind === "bookmark") {
    tree.url = n.url;
    tree.tags = [...n.tags];
    tree.keyword = n.keyword;
    tree.spaces = [...n.spaces];
  } else if (n.kind === "folder") {
    tree.children = kids(ix, id)
      .map((c) => toTree(ix, c.id))
      .filter((c): c is NewBookmarkNode => !!c);
  }
  return tree;
}

// ---------------------------------------------------------------------------------------------
// Drag and drop

export interface MovePlan {
  id: number;
  parent: number;
  /** Position among the destination's children once the node has left its old place. */
  index: number;
}

/** A node can go into a folder unless it is a root or the folder lies inside it. */
export function canMoveInto(ix: BookmarkIndex, id: number, parent: number): boolean {
  const n = ix.byId.get(id);
  const dest = ix.byId.get(parent);
  if (!n || !dest || n.parent === null || dest.kind !== "folder") return false;
  return !isInside(ix, parent, id);
}

/** The moves that make `ids` land together, in the order given, at `index` of folder `parent`.
 *  `index` counts the folder's children as drawn, before any of the nodes leaves its place (the
 *  drop position under the pointer). Each move's own index follows the backend's rule (children
 *  after the node left), so the moves can be sent one after the other. Nodes that can't go
 *  there, or are already in place, produce no move. */
export function planMoves(
  ix: BookmarkIndex,
  ids: readonly number[],
  parent: number,
  index: number,
): MovePlan[] {
  const wanted = ids.filter((id, i) => ids.indexOf(id) === i && canMoveInto(ix, id, parent));
  if (!wanted.length) return [];
  const moving = new Set(wanted);
  const lists = new Map<number, number[]>();
  const listOf = (p: number) => {
    let l = lists.get(p);
    if (!l) {
      l = kids(ix, p).map((n) => n.id);
      lists.set(p, l);
    }
    return l;
  };
  // The nodes go in front of the first node at or after the drop position that isn't moving.
  const shown = listOf(parent);
  let anchor: number | null = null;
  for (let i = Math.max(0, index); i < shown.length; i++) {
    if (!moving.has(shown[i])) {
      anchor = shown[i];
      break;
    }
  }
  const plans: MovePlan[] = [];
  for (const id of wanted) {
    const node = ix.byId.get(id)!;
    const from = listOf(node.parent!);
    const fromAt = from.indexOf(id);
    from.splice(fromAt, 1);
    const to = listOf(parent);
    const at = anchor === null ? to.length : to.indexOf(anchor);
    to.splice(at, 0, id);
    if (node.parent === parent && fromAt === at) continue;
    plans.push({ id, parent, index: at });
  }
  // Several nodes can end up exactly where they started (dropped back among their own siblings).
  const unchanged = [...lists].every(([p, l]) => {
    const before = kids(ix, p);
    return l.length === before.length && l.every((id, i) => id === before[i].id);
  });
  return unchanged ? [] : plans;
}
