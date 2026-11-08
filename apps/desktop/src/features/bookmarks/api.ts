// Backend calls for the bookmark tree, its files and imports from other browsers.
import { invoke } from "@tauri-apps/api/core";
import type { BookmarkBackup, BookmarkNode, BookmarkPatch, ImportResult, ImportSource, NewBookmarkNode } from "./types";

export const bookmarksApi = {
  /** Every node of the bookmark tree: the three roots first, then by parent and position. */
  bookmarksList: () => invoke<BookmarkNode[]>("bookmarks_list"),
  /** Adds nodes (folders may carry children) under `parent` at `index` (null: at the end);
   *  returns the ids of the new top-level nodes. */
  bookmarksAdd: (parent: number, index: number | null, nodes: NewBookmarkNode[]) =>
    invoke<number[]>("bookmarks_add", { parent, index, nodes }),
  bookmarkUpdate: (id: number, patch: BookmarkPatch) =>
    invoke<void>("bookmark_update", { id, patch }),
  /** `index` counts the destination's children after the node has left its old place. */
  bookmarkMove: (id: number, parent: number, index: number) =>
    invoke<void>("bookmark_move", { id, parent, index }),
  bookmarksReorder: (parent: number, order: number[]) =>
    invoke<void>("bookmarks_reorder", { parent, order }),
  bookmarksRemove: (ids: number[]) => invoke<void>("bookmarks_remove", { ids }),
  bookmarksRemoveUrl: (url: string) =>
    invoke<void>("bookmarks_remove_url", { url }),
  /** Netscape HTML (Firefox and Chrome read it); null when the dialog is cancelled. */
  bookmarksExport: () => invoke<string | null>("bookmarks_export"),
  /** JSON backup of the whole tree; null when the dialog is cancelled. */
  bookmarksBackup: () => invoke<string | null>("bookmarks_backup"),
  /** Replaces every bookmark with a backup file's; null when the dialog is cancelled. */
  bookmarksRestore: () => invoke<number | null>("bookmarks_restore"),
  bookmarkBackups: () => invoke<BookmarkBackup[]>("bookmark_backups"),
  bookmarksRestoreBackup: (name: string) =>
    invoke<number>("bookmarks_restore_backup", { name }),
  /** Writes today's automatic backup (once a day). */
  bookmarksAutoBackup: () => invoke<void>("bookmarks_auto_backup"),
  bookmarksImport: () => invoke<number>("bookmarks_import"),
  /** Other browsers' profiles on this PC with bookmarks or history to import. */
  importSources: () => invoke<ImportSource[]>("import_sources"),
  importRun: (
    source: string,
    bookmarks: boolean,
    history: boolean,
    historyDays: number,
  ) =>
    invoke<ImportResult>("import_run", {
      source,
      bookmarks,
      history,
      historyDays,
    }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    /** Another window changed the bookmarks. */
    "bookmarks-changed": { origin: string };
  }
}
