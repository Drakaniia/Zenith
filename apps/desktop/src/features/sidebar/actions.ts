// Actions for the sidebar's layout: compact mode, the floating sidebar and toolbar, collapsing.
import { registerActions } from "../../platform/actions";
import { ui } from "../../shared/ui/ui.svelte";
import { settings } from "../settings/settings.svelte";
import { shortcutHint } from "../shortcuts/hints";

registerActions({
  toggleCompact: () => {
    // Left from the floating sidebar, it docks where it is (Zen): App clears it after the layout
    // motion has seen it.

    ui.topReveal = false;
    return settings.update({ compact: !settings.value.compact });
  },
  toggleFloatingSidebar: () => {
    // Zen: shows the floating sidebar of compact mode and keeps it until pressed again.
    if (!settings.value.compact)
      return ui.toast(`The floating sidebar is part of compact mode${shortcutHint("toggleCompact") ? ` (${shortcutHint("toggleCompact")})` : ""}`, { icon: "sidebar" });
    ui.flyoutPinned = !(ui.flyout && ui.flyoutPinned);
    ui.flyout = ui.flyoutPinned;
  },
  toggleFloatingToolbar: () => {
    // Zen: compact mode hides the toolbar too; it then floats in from the top edge.
    if (settings.layoutMode === "single")
      return ui.toast("With the single toolbar the address bar is in the sidebar", { icon: "sidebar" });
    ui.topReveal = false;
    return settings.update({
      compact: true,
      compactHideToolbar: !(settings.value.compact && settings.value.compactHideToolbar),
    });
  },
  toggleSidebar: () => {
    // Zen: expand or collapse the sidebar (single/multiple toolbars <-> collapsed toolbar).
    const collapsed = settings.value.toolbarMode === "collapsed";
    const mode = settings.value.toolbarMode;
    // Compact mode hides the sidebar entirely, so changing its width would show nothing: leave
    // compact mode in the same step so the click always has a visible effect.
    const wasCompact = settings.value.compact;
    if (wasCompact) ui.flyout = false;
    if (collapsed)
      return settings.update({
        toolbarMode: settings.value.lastExpandedMode ?? "single",
        ...(wasCompact ? { compact: false } : {}),
      });
    return settings.update({
      toolbarMode: "collapsed",
      lastExpandedMode: mode === "collapsed" ? "single" : mode,
      ...(wasCompact ? { compact: false } : {}),
    });
  },
});
