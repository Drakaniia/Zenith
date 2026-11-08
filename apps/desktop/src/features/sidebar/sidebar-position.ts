// Where the sidebar sits in the window. Left and right are the vertical Zen sidebar (mirrored on
// the right); top and bottom turn it into a horizontal tab strip, which always keeps a toolbar
// with the address bar above the page.

export type SidebarPosition = "left" | "right" | "top" | "bottom";
export type LayoutMode = "single" | "multiple" | "collapsed";

export const SIDEBAR_POSITIONS: { value: SidebarPosition; label: string }[] = [
  { value: "left", label: "Left" },
  { value: "right", label: "Right" },
  { value: "top", label: "Top" },
  { value: "bottom", label: "Bottom" },
];

export function normalizeSidebarPosition(value: unknown): SidebarPosition {
  return value === "right" || value === "top" || value === "bottom" ? value : "left";
}

/** Top and bottom: the sidebar is a horizontal tab strip. */
export function isStrip(position: SidebarPosition): boolean {
  return position === "top" || position === "bottom";
}

/** The layout actually drawn. A tab strip has no room for the address bar, so the single-toolbar
 *  layout shows the toolbar above the page there; collapsed makes the strip's tabs icon-only. */
export function effectiveMode(mode: LayoutMode, position: SidebarPosition): LayoutMode {
  return isStrip(position) && mode === "single" ? "multiple" : mode;
}


export function positionHint(position: SidebarPosition): string {
  switch (position) {
    case "right":
      return "The sidebar is on the right side of the window.";
    case "top":
      return "Tabs, address bar and tools in a single row at the top.";
    case "bottom":
      return "Tabs in a strip along the bottom of the window.";
    default:
      return "The sidebar is on the left side of the window.";
  }
}
