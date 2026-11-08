// Browser model basics shared by the store and its helper modules: constants, tab factory and
// the session file shape.
import type { ClosedTab, Space, SpaceId, Tab, TabId } from "./types";
import { isInternal, isNewTab, titleFor } from "../../shared/url";

export const SPACE_COLORS = [
  "#8b7cf6",
  "#5b8def",
  "#2bb5a8",
  "#4fb86a",
  "#e0a83a",
  "#ec7f43",
  "#e8607a",
  "#d36ad6",
  "#8f9bb3",
];
/** The single space an incognito window starts with. */
export const INCOGNITO_SPACE = {
  name: "Incognito",
  icon: "🕶️",
  color: "#7a6fa8",
};
export const SPACE_ICONS = [
  "🏠",
  "💼",
  "🎨",
  "📚",
  "🧪",
  "🎮",
  "🎵",
  "✈️",
  "🛒",
  "💡",
  "🌿",
  "🔥",
  "⭐",
  "🧠",
  "📷",
  "🍿",
];

export const uid = (prefix: string) =>
  prefix +
  Math.random().toString(36).slice(2, 8) +
  Date.now().toString(36).slice(-4);

export function needsWebview(url: string): boolean {
  return !isInternal(url) && !isNewTab(url);
}

/** The label a tab shows: the user's name for it, or its page title. */
export function tabTitle(
  tab: Pick<Tab, "url" | "title" | "customTitle">,
): string {
  return tab.customTitle || titleFor(tab.url, tab.title);
}

export function makeTab(url: string, spaceId: SpaceId, extra: Partial<Tab> = {}): Tab {
  return {
    id: uid("t"),
    url,
    title: "",
    favicon: undefined,
    spaceId,
    pinned: false,
    essential: false,
    lastActive: Date.now(),
    zoom: 1,
    muted: false,
    live: false,
    suspended: false,
    loading: false,
    canGoBack: false,
    canGoForward: false,
    audible: false,
    blocked: 0,
    crashed: "",
    error: "",
    fullscreen: false,
    media: 0,
    ...extra,
  };
}

export interface SessionFile {
  v: number;
  activeSpaceId: SpaceId;
  essentials: TabId[];
  spaces: Space[];
  tabs: Partial<Tab>[];
  closed: ClosedTab[];
}
