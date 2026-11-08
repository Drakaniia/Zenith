// Bookmark and sidebar-panel actions (the work is in bookmark-actions.ts).
import { registerActions } from "../../platform/actions";
import { targetTab } from "../browser/target";
import { settings } from "../settings/settings.svelte";
import {
  bookmarkAllTabs,
  bookmarkPage,
  toggleBookmarksBar,
  toggleBookmarksPanel,
} from "./bookmark-actions";

registerActions({
  bookmark: (source) => bookmarkPage(targetTab(source)),
  bookmarkAllTabs: () => bookmarkAllTabs(),
  toggleBookmarksSidebar: () => toggleBookmarksPanel(),
  toggleBookmarksBar: () => toggleBookmarksBar(),
  historySidebar: () => {
    // Ctrl+H: the sidebar panel on its history view, or closed again.
    const s = settings.value;
    if (s.bookmarksPanel && s.bookmarksPanelView === "history") return settings.update({ bookmarksPanel: false });
    return settings.update({ bookmarksPanel: true, bookmarksPanelView: "history" });
  },
  // Firefox's sidebar toggle: the panel on whichever view it showed last.
  toggleSidebarPanel: () => settings.update({ bookmarksPanel: !settings.value.bookmarksPanel }),
});
