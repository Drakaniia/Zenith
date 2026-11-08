// Actions that open the address bar / command palette.
import { registerActions } from "../../platform/actions";
import { ui } from "../../shared/ui/ui.svelte";
import { isWebUrl } from "../../shared/url";
import { targetTab } from "../browser/target";

registerActions({
  newTab: () => ui.openPalette("newTab"),
  openPalette: (source) => {
    const tab = targetTab(source);
    return ui.openPalette("navigate", tab && isWebUrl(tab.url) ? tab.url : "");
  },
  commandPalette: () => ui.openPalette("command"),
  focusSearch: () => ui.openPalette("search"),
  showAllTabs: () => {
    // The address bar limited to open tabs (Firefox's % restriction).
    ui.openPalette("navigate", "% ");
    ui.palette.selectAll = false;
  },
});
