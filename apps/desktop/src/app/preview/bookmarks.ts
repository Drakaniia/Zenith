import { rememberFavicon } from "../../shared/favicons";
import { favicon, params } from "./fixtures";
import { UNHANDLED } from "./handler";

// ---------------------------------------------------------------------------------------------
// Bookmarks: an in-memory tree with the backend's rules (roots 1-3, contiguous positions, a move's
// index counting the children after the node left). ?bookmarks=empty starts without any.

interface MockNode {
  id: number;
  parent: number | null;
  kind: "bookmark" | "folder" | "separator";
  title: string;
  url: string;
  position: number;
  created: number;
  modified: number;
  tags: string[];
  keyword: string;
  spaces: string[];
}

interface MockSpec {
  kind: MockNode["kind"];
  title?: string;
  url?: string;
  tags?: string[];
  keyword?: string;
  spaces?: string[];
  children?: MockSpec[];
}

const bmNodes: MockNode[] = [
  [1, "Bookmarks Toolbar"],
  [2, "Bookmarks Menu"],
  [3, "Other Bookmarks"],
].map(([id, title], i) => ({
  id: id as number,
  parent: null,
  kind: "folder" as const,
  title: title as string,
  url: "",
  position: i,
  created: 0,
  modified: 0,
  tags: [],
  keyword: "",
  spaces: [],
}));
let bmNext = 10;

const bmKids = (parent: number) => bmNodes.filter((n) => n.parent === parent).sort((a, b) => a.position - b.position);

function bmWrite(parent: number, ids: number[]) {
  ids.forEach((id, i) => {
    const n = bmNodes.find((x) => x.id === id)!;
    n.parent = parent;
    n.position = i;
  });
}

function bmAdd(parent: number, index: number | null, specs: MockSpec[]): number[] {
  const made: number[] = [];
  const make = (p: number, s: MockSpec): number => {
    const id = bmNext++;
    const now = Date.now();
    bmNodes.push({
      id,
      parent: p,
      kind: s.kind,
      title: s.title ?? "",
      url: s.url ?? "",
      position: bmKids(p).length,
      created: now,
      modified: now,
      tags: s.tags ?? [],
      keyword: (s.keyword ?? "").toLowerCase(),
      spaces: s.spaces ?? [],
    });
    for (const c of s.children ?? []) make(id, c);
    return id;
  };
  for (const s of specs) made.push(make(parent, s));
  if (index !== null) {
    const order = bmKids(parent).map((n) => n.id).filter((id) => !made.includes(id));
    const at = Math.max(0, Math.min(index, order.length));
    order.splice(at, 0, ...made);
    bmWrite(parent, order);
  }
  return made;
}

function bmRemove(ids: number[]) {
  const drop = (id: number) => {
    for (const c of bmKids(id)) drop(c.id);
    const i = bmNodes.findIndex((n) => n.id === id);
    if (i >= 0) bmNodes.splice(i, 1);
  };
  const parents = new Set<number>();
  for (const id of ids) {
    const n = bmNodes.find((x) => x.id === id);
    if (!n || n.parent === null) continue;
    parents.add(n.parent);
    drop(id);
  }
  for (const p of parents) bmWrite(p, bmKids(p).map((n) => n.id));
}

function seedBookmarks() {
  if (params.get("bookmarks") === "empty") return;
  const fav = (host: string, letter: string, bg: string) => rememberFavicon(`https://${host}/`, favicon(letter, bg));
  fav("zen-browser.app", "Z", "#f76f53");
  fav("docs.zen-browser.app", "D", "#f76f53");
  fav("developer.mozilla.org", "M", "#000");
  fav("svelte.dev", "S", "#ff3e00");
  fav("tauri.app", "T", "#24c8db");
  fav("doc.rust-lang.org", "R", "#b7410e");
  fav("github.com", "G", "#24292f");
  fav("tracker.example.com", "J", "#0052cc");
  fav("support.mozilla.org", "F", "#ff7139");
  fav("www.mozilla.org", "m", "#111");
  fav("en.wikipedia.org", "W", "#636466");
  fav("example.com", "E", "#5b8def");
  bmAdd(1, null, [
    { kind: "bookmark", title: "Zen Browser", url: "https://zen-browser.app/", tags: ["browser", "calm"], keyword: "zen" },
    { kind: "bookmark", title: "Documentation", url: "https://docs.zen-browser.app/" },
    {
      kind: "folder",
      title: "Dev",
      children: [
        { kind: "bookmark", title: "corner-shape - CSS | MDN", url: "https://developer.mozilla.org/en-US/docs/Web/CSS/corner-shape", tags: ["css"] },
        { kind: "bookmark", title: "Svelte Docs", url: "https://svelte.dev/docs", tags: ["svelte", "docs"] },
        { kind: "bookmark", title: "Tauri", url: "https://tauri.app/" },
        { kind: "folder", title: "Rust", children: [{ kind: "bookmark", title: "The Rust Book", url: "https://doc.rust-lang.org/book/", tags: ["rust", "docs"] }] },
      ],
    },
    { kind: "separator" },
    { kind: "bookmark", title: "zen-browser/desktop", url: "https://github.com/zen-browser/desktop", tags: ["code"] },
    { kind: "bookmark", title: "Sprint board", url: "https://tracker.example.com/board", spaces: ["work"] },
  ]);
  bmAdd(2, null, [
    {
      kind: "folder",
      title: "Mozilla Firefox",
      children: [
        { kind: "bookmark", title: "Get Help", url: "https://support.mozilla.org/products/firefox" },
        { kind: "bookmark", title: "Customize Firefox", url: "https://support.mozilla.org/kb/customize-firefox" },
        { kind: "bookmark", title: "Get Involved", url: "https://www.mozilla.org/contribute/" },
        { kind: "bookmark", title: "About Us", url: "https://www.mozilla.org/about/" },
      ],
    },
    { kind: "bookmark", title: "Wikipedia", url: "https://en.wikipedia.org/wiki/Special:Search?search=%s", keyword: "w", tags: ["reference"] },
  ]);
  bmAdd(3, null, [
    { kind: "bookmark", title: "Example Domain", url: "https://example.com/" },
    { kind: "bookmark", title: "On squircles", url: "https://read.example.com/a", tags: ["css", "reading"] },
  ]);
  // Spread the dates out so "recent bookmarks" and the Added column have something to show.
  const hour = 3_600_000;
  bmNodes.filter((n) => n.parent !== null).forEach((n, i) => (n.created = n.modified = Date.now() - i * 7 * hour));
}
seedBookmarks();

const bmBackups = [
  { name: "bookmarks-2026-11-07.json", created: Date.now() - 3 * 3_600_000, count: 17 },
  { name: "bookmarks-2026-11-06.json", created: Date.now() - 27 * 3_600_000, count: 16 },
];

/** Bookmark tree, import/export and backup commands. */
export function handleBookmarks(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "bookmarks_list":
      return JSON.parse(JSON.stringify(bmNodes));
    case "bookmarks_add":
      return bmAdd(a.parent as number, (a.index as number | null) ?? null, a.nodes as MockSpec[]);
    case "bookmark_update": {
      const n = bmNodes.find((x) => x.id === a.id);
      const p = a.patch as Partial<MockNode>;
      if (n) {
        if (p.title !== undefined && n.parent !== null) n.title = p.title;
        if (p.url !== undefined && n.kind === "bookmark") n.url = p.url;
        if (p.tags !== undefined) n.tags = p.tags;
        if (p.keyword !== undefined) n.keyword = p.keyword.toLowerCase();
        if (p.spaces !== undefined) n.spaces = p.spaces;
        n.modified = Date.now();
      }
      return null;
    }
    case "bookmark_move": {
      const n = bmNodes.find((x) => x.id === a.id);
      if (!n || n.parent === null) return Promise.reject("The built-in folders can't be moved.");
      let up: number | null = a.parent as number;
      while (up !== null) {
        if (up === n.id) return Promise.reject("A folder can't be moved into itself.");
        up = bmNodes.find((x) => x.id === up)?.parent ?? null;
      }
      const old = n.parent;
      const order = bmKids(a.parent as number).map((x) => x.id).filter((id) => id !== n.id);
      order.splice(Math.max(0, Math.min(a.index as number, order.length)), 0, n.id);
      bmWrite(a.parent as number, order);
      if (old !== a.parent) bmWrite(old, bmKids(old).map((x) => x.id));
      return null;
    }
    case "bookmarks_reorder":
      bmWrite(a.parent as number, a.order as number[]);
      return null;
    case "bookmarks_remove":
      bmRemove(a.ids as number[]);
      return null;
    case "bookmarks_remove_url":
      bmRemove(bmNodes.filter((n) => n.kind === "bookmark" && n.url === a.url).map((n) => n.id));
      return null;
    case "bookmarks_export":
      return "C:\\Users\\you\\Documents\\zenith-bookmarks.html";
    case "bookmarks_backup":
      return "C:\\Users\\you\\Documents\\zenith-bookmarks-2026-11-07.json";
    case "bookmarks_restore":
      return null;
    case "bookmark_backups":
      return bmBackups;
    case "bookmarks_restore_backup":
      return 17;
    case "bookmarks_auto_backup":
      return null;
    case "bookmarks_import":
      return 12;
    default:
      return UNHANDLED;
  }
}
