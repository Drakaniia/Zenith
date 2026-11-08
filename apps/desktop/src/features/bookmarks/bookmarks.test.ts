import { test } from "node:test";
import assert from "node:assert/strict";
import {
  ROOTS,
  ROOT_MENU,
  ROOT_OTHER,
  ROOT_TITLES,
  ROOT_TOOLBAR,
  ancestors,
  bookmarkTitle,
  bookmarksIn,
  buildIndex,
  canMoveInto,

  folderPath,
  formatTags,
  inSpace,
  isInside,
  keywordMatch,
  kids,
  nodeLabel,
  parseRestriction,
  searchTagged,
  parseTags,
  planMoves,
  recentBookmarks,
  recentFolders,
  scoreBookmark,
  searchBookmarks,
  shortUrl,
  sortedByName,
  tabsFolderName,
  tagCounts,
  toTree,
  type MovePlan,
} from "./bookmarks.ts";
import type { BookmarkNode } from "./types.ts";

type Spec = [id: number, parent: number, kind: BookmarkNode["kind"], title: string, extra?: Partial<BookmarkNode>];

/** The three roots plus the given nodes; positions follow the order each parent's children are listed in. */
function tree(...specs: Spec[]): BookmarkNode[] {
  const next = new Map<number, number>();
  const nodes: BookmarkNode[] = ROOTS.map((id, i) => ({
    id,
    parent: null,
    kind: "folder",
    title: ROOT_TITLES[id],
    url: "",
    position: i,
    created: 0,
    modified: 0,
    tags: [],
    keyword: "",
    spaces: [],
  }));
  for (const [id, parent, kind, title, extra] of specs) {
    const position = next.get(parent) ?? 0;
    next.set(parent, position + 1);
    nodes.push({
      id,
      parent,
      kind,
      title,
      url: kind === "bookmark" ? `https://site${id}.example/` : "",
      position,
      created: id * 1000,
      modified: id * 1000,
      tags: [],
      keyword: "",
      spaces: [],
      ...extra,
    });
  }
  return nodes;
}

/** Toolbar: A, Dev[B, Tools[C]], separator, D. Menu: E. */
const sample = () =>
  tree(
    [10, ROOT_TOOLBAR, "bookmark", "A"],
    [11, ROOT_TOOLBAR, "folder", "Dev"],
    [12, 11, "bookmark", "B"],
    [13, 11, "folder", "Tools"],
    [14, 13, "bookmark", "C"],
    [15, ROOT_TOOLBAR, "separator", ""],
    [16, ROOT_TOOLBAR, "bookmark", "D"],
    [17, ROOT_MENU, "bookmark", "E"],
  );

const ids = (list: readonly { id: number }[]) => list.map((n) => n.id);

test("the index orders children by position and finds bookmarks by address and keyword", () => {
  const nodes = sample().reverse();
  const ix = buildIndex(nodes);
  assert.deepEqual(ids(kids(ix, ROOT_TOOLBAR)), [10, 11, 15, 16]);
  assert.deepEqual(ids(kids(ix, 11)), [12, 13]);
  assert.deepEqual(ids(kids(ix, ROOT_OTHER)), []);
  assert.equal(ix.count, 5);
  assert.equal(ix.byId.get(12)?.title, "B");

  const twice = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "bookmark", "old", { url: "https://same.example/", modified: 5, keyword: "Same" }],
      [21, ROOT_MENU, "bookmark", "new", { url: "https://same.example/", modified: 9 }],
    ),
  );
  assert.deepEqual(ids(twice.byUrl.get("https://same.example/")!), [21, 20], "most recently changed first");
  assert.equal(twice.byKeyword.get("same")?.id, 20, "keywords are looked up in lower case");
});

test("ancestors, paths and containment", () => {
  const ix = buildIndex(sample());
  assert.deepEqual(ids(ancestors(ix, 14)), [ROOT_TOOLBAR, 11, 13]);
  assert.deepEqual(ancestors(ix, ROOT_TOOLBAR), []);
  assert.equal(folderPath(ix, 14), "Bookmarks Toolbar / Dev / Tools", "a bookmark: the folder holding it");
  assert.equal(folderPath(ix, 13), "Bookmarks Toolbar / Dev / Tools", "a folder: itself included");
  assert.equal(folderPath(ix, ROOT_MENU), "Bookmarks Menu");
  assert.equal(folderPath(ix, 999), "");
  assert.ok(isInside(ix, 14, 11));
  assert.ok(isInside(ix, 11, 11), "a folder is inside itself");
  assert.ok(!isInside(ix, 11, 14));
  assert.ok(!isInside(ix, 10, 11));

  assert.deepEqual(ids(bookmarksIn(ix, ROOT_TOOLBAR)), [10, 16], "only direct bookmarks");

});

test("workspace bookmarks show only in their workspaces", () => {
  const live = new Set(["home", "work"]);
  assert.ok(inSpace({ spaces: [] }, "home", live), "no workspace named: everywhere");
  assert.ok(inSpace({ spaces: ["work"] }, "work", live));
  assert.ok(!inSpace({ spaces: ["work"] }, "home", live));
  assert.ok(inSpace({ spaces: ["work", "home"] }, "home", live));
  assert.ok(inSpace({ spaces: ["gone"] }, "home", live), "a deleted workspace doesn't hide the bookmark");
  assert.ok(!inSpace({ spaces: ["gone", "work"] }, "home", live), "only the live ones count");
  assert.ok(!inSpace({ spaces: ["work"] }, undefined, live));
});

test("tags are trimmed, de-duplicated without regard to case and counted", () => {
  assert.deepEqual(parseTags("news, Work ,news, , WORK\nreading list"), ["news", "Work", "reading list"]);
  assert.deepEqual(parseTags(""), []);
  assert.equal(parseTags("x".repeat(100))[0].length, 64);
  assert.equal(parseTags(Array.from({ length: 80 }, (_, i) => `t${i}`).join(",")).length, 50);
  assert.equal(formatTags(["a", "b"]), "a, b");

  const nodes = tree(
    [20, ROOT_TOOLBAR, "bookmark", "x", { tags: ["News", "work"] }],
    [21, ROOT_TOOLBAR, "bookmark", "y", { tags: ["news"] }],
    [22, ROOT_TOOLBAR, "folder", "f", { tags: ["ignored"] }],
  );
  assert.deepEqual(tagCounts(nodes), [
    { tag: "News", count: 2 },
    { tag: "work", count: 1 },
  ]);
});

test("search needs every word, in any order, as a substring of the title, address, tags or keyword", () => {
  const zen = { title: "Browser for Zen", url: "https://zen-browser.app/", tags: ["calm"], keyword: "" };
  assert.ok(scoreBookmark("zen browser", zen) > 0, "words in any order");
  assert.equal(scoreBookmark("zb", zen), 0, "no subsequence matching");
  assert.equal(scoreBookmark("zen nothing", zen), 0, "every word must match");
  assert.ok(scoreBookmark("calm", zen) > 0, "tags");
  assert.ok(scoreBookmark("zen-browser.app", zen) > 0, "address");
  assert.equal(scoreBookmark("", zen), 1);

  const gh = { title: "GitHub", url: "https://github.com/", tags: [], keyword: "" };
  const digital = { title: "Digital garden", url: "https://example.com/", tags: [], keyword: "" };
  assert.ok(scoreBookmark("git", gh) > scoreBookmark("git", digital), "a word start beats the middle of a word");

  const kw = { title: "Wikipedia", url: "https://en.wikipedia.org/", tags: [], keyword: "w" };
  const other = { title: "w3schools", url: "https://w3schools.com/", tags: [], keyword: "" };
  assert.ok(scoreBookmark("w", kw) > scoreBookmark("w", other), "an exact keyword ranks first");
});

test("searching ranks results and can stay inside a folder", () => {
  const ix = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "bookmark", "Cooking notes", { tags: ["recipes"] }],
      [21, ROOT_MENU, "bookmark", "Recipes I like"],
      [22, ROOT_MENU, "bookmark", "Unrelated"],
      [23, ROOT_MENU, "folder", "Recipes"],
    ),
  );
  assert.deepEqual(ids(searchBookmarks(ix, "recipes")).sort(), [20, 21], "folders are never results");
  assert.deepEqual(ids(searchBookmarks(ix, "recipes", { within: ROOT_MENU })), [21]);
  assert.equal(searchBookmarks(ix, "recipes", { limit: 1 }).length, 1);
  assert.deepEqual(searchBookmarks(ix, "nothing here"), []);
});

test("restriction tokens limit the address bar to one kind of result", () => {
  assert.deepEqual(parseRestriction("* zen"), { restriction: "bookmarks", rest: "zen" });
  assert.deepEqual(parseRestriction("zen *"), { restriction: "bookmarks", rest: "zen" });
  assert.deepEqual(parseRestriction("*zen"), { restriction: "bookmarks", rest: "zen" });
  assert.deepEqual(parseRestriction("*"), { restriction: "bookmarks", rest: "" });
  assert.deepEqual(parseRestriction("^ news site"), { restriction: "history", rest: "news site" });
  assert.deepEqual(parseRestriction("% mail"), { restriction: "tabs", rest: "mail" });
  assert.deepEqual(parseRestriction("+ css"), { restriction: "tagged", rest: "css" });
  // A token in the middle of the words, or a plain query, is just text.
  assert.deepEqual(parseRestriction("a * b"), { restriction: null, rest: "a * b" });
  assert.deepEqual(parseRestriction("  hello  world "), { restriction: null, rest: "hello  world" }, "a plain query is left as typed");
  assert.deepEqual(parseRestriction(""), { restriction: null, rest: "" });
});

test("+ finds bookmarks by part of a tag", () => {
  const ix = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "bookmark", "B", { tags: ["Newsletter", "daily"] }],
      [21, ROOT_TOOLBAR, "bookmark", "A", { tags: ["css"] }],
      [22, ROOT_TOOLBAR, "bookmark", "C"],
    ),
  );
  assert.deepEqual(ids(searchTagged(ix, "news")), [20]);
  assert.deepEqual(ids(searchTagged(ix, "news daily")), [20]);
  assert.deepEqual(ids(searchTagged(ix, "")), [21, 20], "every tagged one, by name");
  assert.deepEqual(searchTagged(ix, "zzz"), []);
});

test("a keyword opens its bookmark, and fills a %s placeholder from the rest of the line", () => {
  const ix = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "bookmark", "Wikipedia", {
        url: "https://en.wikipedia.org/wiki/Special:Search?search=%s",
        keyword: "w",
      }],
      [21, ROOT_TOOLBAR, "bookmark", "Mail", { url: "https://mail.example.com/", keyword: "Mail" }],
    ),
  );
  assert.equal(keywordMatch(ix, "w")?.url, "https://en.wikipedia.org/wiki/Special:Search?search=");
  assert.equal(
    keywordMatch(ix, "  W cats & dogs ")?.url,
    "https://en.wikipedia.org/wiki/Special:Search?search=cats%20%26%20dogs",
  );
  assert.equal(keywordMatch(ix, "mail")?.node.id, 21);
  assert.equal(keywordMatch(ix, "mail inbox"), null, "extra words need a placeholder");
  assert.equal(keywordMatch(ix, "wiki"), null);
  assert.equal(keywordMatch(ix, ""), null);
});

test("recent bookmarks and folders are newest first; the built-in folders aren't offered", () => {
  const ix = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "folder", "Old"],
      [21, ROOT_TOOLBAR, "folder", "New"],
      [30, 20, "bookmark", "in old", { created: 100 }],
      [31, 21, "bookmark", "in new", { created: 300 }],
      [32, ROOT_MENU, "bookmark", "in menu", { created: 200 }],
    ),
  );
  assert.deepEqual(ids(recentBookmarks(ix, 2)), [31, 32]);
  assert.deepEqual(ids(recentFolders(ix, 5)), [21, 20]);
  assert.deepEqual(ids(recentFolders(ix, 1)), [21]);
});


test("sorting by name puts folders first and sorts each stretch between separators on its own", () => {
  const ix = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "bookmark", "b"],
      [21, ROOT_TOOLBAR, "bookmark", "a"],
      [22, ROOT_TOOLBAR, "folder", "z"],
      [23, ROOT_TOOLBAR, "separator", ""],
      [24, ROOT_TOOLBAR, "bookmark", "d"],
      [25, ROOT_TOOLBAR, "bookmark", "C"],
      [26, ROOT_TOOLBAR, "bookmark", "item 10"],
      [27, ROOT_TOOLBAR, "bookmark", "item 9"],
    ),
  );
  assert.deepEqual(sortedByName(ix, ROOT_TOOLBAR), [22, 21, 20, 23, 25, 24, 27, 26]);
});

test("labels fall back to the address", () => {
  assert.equal(shortUrl("https://www.example.com/path/"), "example.com/path");
  assert.equal(nodeLabel({ kind: "bookmark", title: "", url: "https://www.example.com/" }), "example.com");
  assert.equal(nodeLabel({ kind: "bookmark", title: "Hi", url: "https://x.example/" }), "Hi");
  assert.equal(nodeLabel({ kind: "folder", title: " ", url: "" }), "Untitled folder");
  assert.equal(bookmarkTitle("  ", "https://example.com/"), "example.com");
  assert.equal(bookmarkTitle("Page", "https://example.com/"), "Page");
  assert.equal(tabsFolderName(new Date(2026, 10, 7)), "Tabs 2026-11-07");
});

test("a folder becomes a creation request again with everything below it", () => {
  const ix = buildIndex(
    tree(
      [20, ROOT_TOOLBAR, "folder", "Dev", { tags: ["not kept on folders"] }],
      [21, 20, "bookmark", "Docs", { tags: ["ref"], keyword: "d", spaces: ["work"] }],
      [22, 20, "separator", ""],
    ),
  );
  assert.deepEqual(toTree(ix, 20), {
    kind: "folder",
    title: "Dev",
    children: [
      { kind: "bookmark", title: "Docs", url: "https://site21.example/", tags: ["ref"], keyword: "d", spaces: ["work"] },
      { kind: "separator", title: "" },
    ],
  });
  assert.equal(toTree(ix, 999), null);
});

test("what can be dropped where", () => {
  const ix = buildIndex(sample());
  assert.ok(canMoveInto(ix, 10, 11));
  assert.ok(canMoveInto(ix, 12, ROOT_MENU));
  assert.ok(!canMoveInto(ix, 11, 11), "not into itself");
  assert.ok(!canMoveInto(ix, 11, 13), "not into its own subtree");
  assert.ok(!canMoveInto(ix, ROOT_MENU, ROOT_TOOLBAR), "roots stay put");
  assert.ok(!canMoveInto(ix, 10, 16), "only into folders");
  assert.ok(!canMoveInto(ix, 10, 999));
});

/** What the backend does with a move: take the node out, then insert it at the index. */
function simulate(nodes: BookmarkNode[], plans: MovePlan[]): Map<number, number[]> {
  const ix = buildIndex(nodes);
  const lists = new Map<number, number[]>();
  for (const [p, list] of ix.children) lists.set(p, list.map((n) => n.id));
  const parentOf = new Map(nodes.map((n) => [n.id, n.parent]));
  for (const { id, parent, index } of plans) {
    const from = lists.get(parentOf.get(id)!)!;
    from.splice(from.indexOf(id), 1);
    const to = lists.get(parent) ?? [];
    lists.set(parent, to);
    to.splice(Math.min(Math.max(index, 0), to.length), 0, id);
    parentOf.set(id, parent);
  }
  return lists;
}

test("a node dragged within its folder lands before the node under the pointer", () => {
  const nodes = sample();
  const ix = buildIndex(nodes);
  // Drop A (position 0) before D (index 3): the others close up, so it ends at index 2.
  const plans = planMoves(ix, [10], ROOT_TOOLBAR, 3);
  assert.deepEqual(plans, [{ id: 10, parent: ROOT_TOOLBAR, index: 2 }]);
  assert.deepEqual(simulate(nodes, plans).get(ROOT_TOOLBAR), [11, 15, 10, 16]);
  // Dropping a node onto its own place does nothing.
  assert.deepEqual(planMoves(ix, [10], ROOT_TOOLBAR, 0), []);
  assert.deepEqual(planMoves(ix, [10], ROOT_TOOLBAR, 1), []);
  // At the end.
  const toEnd = planMoves(ix, [10], ROOT_TOOLBAR, 4);
  assert.deepEqual(simulate(nodes, toEnd).get(ROOT_TOOLBAR), [11, 15, 16, 10]);
  // Backwards: D (position 3) to the front.
  const toFront = planMoves(ix, [16], ROOT_TOOLBAR, 0);
  assert.deepEqual(simulate(nodes, toFront).get(ROOT_TOOLBAR), [16, 10, 11, 15]);
});

test("nodes can be dragged into another folder, at a position or into the middle of its list", () => {
  const nodes = sample();
  const ix = buildIndex(nodes);
  const out = planMoves(ix, [12], ROOT_TOOLBAR, 2);
  assert.deepEqual(out, [{ id: 12, parent: ROOT_TOOLBAR, index: 2 }]);
  const lists = simulate(nodes, out);
  assert.deepEqual(lists.get(ROOT_TOOLBAR), [10, 11, 12, 15, 16]);
  assert.deepEqual(lists.get(11), [13]);
  // Into an empty folder.
  const other = planMoves(ix, [10], ROOT_OTHER, 0);
  assert.deepEqual(simulate(nodes, other).get(ROOT_OTHER), [10]);
});

test("several nodes dragged together keep their order and land side by side", () => {
  const nodes = sample();
  const ix = buildIndex(nodes);
  const plans = planMoves(ix, [10, 16], ROOT_MENU, 0);
  const lists = simulate(nodes, plans);
  assert.deepEqual(lists.get(ROOT_MENU), [10, 16, 17]);
  assert.deepEqual(lists.get(ROOT_TOOLBAR), [11, 15]);
  // Both inside one folder, dropped among their own siblings: the anchor is the first one that stays.
  const together = planMoves(ix, [10, 11], ROOT_TOOLBAR, 1);
  assert.deepEqual(together, [], "already adjacent and in order");
  const swap = planMoves(ix, [11, 10], ROOT_TOOLBAR, 2);
  assert.deepEqual(simulate(nodes, swap).get(ROOT_TOOLBAR), [11, 10, 15, 16]);
  // Duplicates are ignored.
  assert.equal(planMoves(ix, [10, 10], ROOT_MENU, 0).length, 1);
});

test("a move that would break the tree produces nothing", () => {
  const ix = buildIndex(sample());
  assert.deepEqual(planMoves(ix, [11], 13, 0), [], "a folder into its own subfolder");
  assert.deepEqual(planMoves(ix, [11], 11, 0), [], "a folder into itself");
  assert.deepEqual(planMoves(ix, [ROOT_MENU], ROOT_TOOLBAR, 0), [], "roots can't move");
  assert.deepEqual(planMoves(ix, [10], 16, 0), [], "into a bookmark");
  assert.deepEqual(planMoves(ix, [], ROOT_TOOLBAR, 0), []);
  // The valid ones among several still go.
  assert.equal(planMoves(ix, [11, 10], 13, 0).length, 1);
});
