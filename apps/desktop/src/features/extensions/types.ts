export interface ExtensionInfo {
  id: string;
  name: string;
  enabled: boolean;
  version: string;
  description: string;
  optionsUrl?: string | null;
  popupUrl?: string | null;
  icon?: string | null;
  builtin: boolean;
  /** Allowed to run in incognito windows (when they are allowed at all). */
  incognito: boolean;
  /** Chrome Web Store id, for extensions installed from the store. */
  storeId?: string | null;
}

/** An extension found in a page (a .crx download, "Add to Chrome" on the Web Store),
 *  staged and waiting for the user to accept or decline it. */
export interface InstallOffer {
  token: number;
  name: string;
  version: string;
  description: string;
  icon?: string | null;
  /** Chrome-style permission warnings. */
  warnings: string[];
  /** Installed version this would replace. */
  replaces?: string | null;
  storeId?: string | null;
}

export interface InstallEvent {
  tabId: string;
  state: "fetching" | "ready" | "failed";
  /** "Chrome Web Store" or the site's host name. */
  source: string;
  offer?: InstallOffer;
  error?: string;
}

export interface ExtensionUpdate {
  id: string;
  storeId: string;
  name: string;
  current: string;
  latest: string;
}

export interface ActionBadge {
  text: string;
  backgroundColor?: string | null;
  textColor?: string | null;
}

export interface BadgeEvent {
  extensionId: string;
  tabId?: string | null;
  badge: ActionBadge;
}

export interface NativeHostInfo {
  name: string;
  description: string;
  path: string;
  allowedOrigins: string[];
  consented: boolean;
  source: string;
}

export interface TabSyncSnapshot {
  id: string;
  url: string;
  title: string;
  favicon: string;
  loading: boolean;
  discarded: boolean;
  audible: boolean;
  muted: boolean;
  pinned: boolean;
  essential: boolean;
  spaceId: string;
  glance: boolean;
}

export interface WindowSyncSnapshot {
  label: string;
  incognito: boolean;
  focused: boolean;
  activeSpaceId: string;
  activeTabId: string | null;
  visibleIds: string[];
  spaceOrder: string[];
  tabs: TabSyncSnapshot[];
}
