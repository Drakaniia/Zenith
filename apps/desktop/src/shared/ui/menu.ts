// The menu model every context menu and dropdown builds: items, the keys held when one is
// chosen, and the rectangle a popup hangs from.
import type { IconName } from "./icons";

/** Keys held (or the middle button) when a menu item was chosen: a bookmark in a menu opens in
 *  a new tab with Ctrl or the middle button, in Glance with Shift. */
export interface MenuMods {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  middle: boolean;
}

export interface MenuItem {
  label?: string;
  icon?: IconName;
  /** A page's own icon (a bookmark in a menu) in place of a drawn one. */
  site?: { url: string; favicon?: string };
  /**
   * A coloured round badge with a letter (a browser profile's avatar) in place of an icon.
   * `img` replaces the letter (an emoji on the colour); with `photo` it fills the whole badge.
   */
  swatch?: { color: string; fg: string; text: string; img?: string; photo?: boolean };
  hint?: string;
  danger?: boolean;
  checked?: boolean;
  disabled?: boolean;
  separator?: boolean;
  /** A greyed label with no action (a heading, or "(Empty)"). */
  heading?: boolean;
  /** Zen's app-menu zoom row (− 100% + and full screen) for the active tab. */
  zoom?: boolean;
  run?: (mods?: MenuMods) => void;
  /** A submenu, to any depth. A function builds it when it opens (big bookmark folders). */
  children?: MenuItem[] | (() => MenuItem[]);
}

/** A rectangle on screen (CSS px) that a popup hangs from. */
export interface AnchorRect {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

export function rectOf(el: Element): AnchorRect {
  const r = el.getBoundingClientRect();
  return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
}
