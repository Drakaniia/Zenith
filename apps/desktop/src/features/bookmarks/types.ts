// The bookmark tree as the backend stores it, and imports from other browsers.

export type BookmarkKind = "bookmark" | "folder" | "separator";

/** One node of the bookmark tree (Firefox's Places model, as in Zen): a bookmark, a folder or
 *  a separator. Three built-in folders are the roots (see `ROOT_*` in bookmarks.ts). */
export interface BookmarkNode {
  id: number;
  /** Null only for the three built-in roots. */
  parent: number | null;
  kind: BookmarkKind;
  title: string;
  /** Empty for folders and separators. */
  url: string;
  /** Contiguous 0..n-1 among the siblings. */
  position: number;
  created: number;
  modified: number;
  tags: string[];
  /** Typing it in the address bar opens the bookmark. */
  keyword: string;
  /** Workspaces the bookmark is shown in on the toolbar (Zen: workspace bookmarks). Empty: all. */
  spaces: string[];
}

/** A node to create; folders may carry children. */
export interface NewBookmarkNode {
  kind: BookmarkKind;
  title?: string;
  url?: string;
  tags?: string[];
  keyword?: string;
  spaces?: string[];
  children?: NewBookmarkNode[];
}

/** Fields to change on a node; omitted ones stay as they are. */
export interface BookmarkPatch {
  title?: string;
  url?: string;
  tags?: string[];
  keyword?: string;
  spaces?: string[];
}

/** An automatic or pre-restore backup file of the bookmarks. */
export interface BookmarkBackup {
  name: string;
  created: number;
  count: number;
}

/** Another browser's profile that can be imported (`import_sources`). */
export interface ImportSource {
  id: string;
  /** "chrome", "edge", "brave", "vivaldi", "chromium", "opera", "operagx", "firefox", "zen" */
  browser: string;
  name: string;
  /** Empty when the browser has a single profile folder. */
  profile: string;
  bookmarks: boolean;
  history: boolean;
}

export interface ImportResult {
  /** New bookmarks; ones already present are skipped. */
  bookmarks: number;
  /** History entries merged. */
  history: number;
  errors: string[];
}
