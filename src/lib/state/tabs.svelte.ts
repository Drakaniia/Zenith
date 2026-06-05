import type { Tab } from '../utils/tauri';
import {
  BROWSER_HOME_URL,
  BROWSER_SETTINGS_URL,
  isInternalBrowserUrl,
  titleForInternalUrl,
  type InternalUrl
} from '../utils/internalUrls';

export type BrowserWebviewLabel = `tab_${string}`;

export interface BrowserTab extends Tab {
  webviewLabel: BrowserWebviewLabel;
  isInternal: boolean;
  history: string[];
  historyIndex: number;
}

const DEFAULT_TAB_TITLE = 'New Tab';
const MAX_CLOSED_TABS = 10;

type ClosedTabSnapshot = Omit<BrowserTab, 'id' | 'webviewLabel'>;

class TabsState {
  tabs = $state<BrowserTab[]>([]);
  activeTabId = $state<string | null>(null);
  selectedTabIds = $state<string[]>([]);
  reloadRequests = $state<Record<string, number>>({});
  closedTabs = $state<ClosedTabSnapshot[]>([]);
  activeTab = $derived(this.tabs.find((tab) => tab.id === this.activeTabId) ?? null);
  activeIndex = $derived(this.tabs.findIndex((tab) => tab.id === this.activeTabId));

  openTab = (url = BROWSER_HOME_URL, title = DEFAULT_TAB_TITLE): BrowserTab => {
    if (isInternalBrowserUrl(url)) {
      return this.openInternalTab(url);
    }

    const tab = createTab(url, title, false);
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.id;
    return tab;
  };

  openHomeTab = (): BrowserTab => this.openInternalTab(BROWSER_HOME_URL);

  openInternalTab = (url: InternalUrl): BrowserTab => {
    const tab = createTab(url, internalTitle(url), true);
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.id;
    return tab;
  };

  closeTab = (id: string): BrowserTab | null => {
    const index = this.tabs.findIndex((tab) => tab.id === id);
    if (index === -1) {
      return null;
    }

    const closedTab = this.tabs[index];
    this.rememberClosedTab(closedTab);
    this.tabs = this.tabs.filter((tab) => tab.id !== id);
    this.reloadRequests = withoutReloadRequests(this.reloadRequests, [id]);
    this.selectedTabIds = this.selectedTabIds.filter((tabId) => tabId !== id);

    if (this.activeTabId === id) {
      this.activeTabId = this.tabs[index]?.id ?? this.tabs[index - 1]?.id ?? null;
    }

    return closedTab;
  };

  setActiveTab = (id: string): void => {
    if (this.tabs.some((tab) => tab.id === id)) {
      this.activeTabId = id;
    }
  };

  updateTab = (id: string, patch: Partial<BrowserTab>): void => {
    this.tabs = this.tabs.map((tab) =>
      tab.id === id
        ? {
            ...tab,
            ...patch,
            id: tab.id,
            webviewLabel: tab.webviewLabel
          }
        : tab
    );
  };

  duplicateTab = (id: string): BrowserTab | null => {
    const source = this.tabs.find((tab) => tab.id === id);
    if (!source) {
      return null;
    }

    const duplicate = createTab(source.url, source.title, source.isInternal);
    duplicate.favicon_url = source.favicon_url;
    this.tabs = [...this.tabs, duplicate];
    this.activeTabId = duplicate.id;
    return duplicate;
  };

  closeOtherTabs = (id: string): string[] => {
    if (!this.tabs.some((tab) => tab.id === id)) {
      return [];
    }

    const closedTabs = this.tabs.filter((tab) => tab.id !== id);
    const closedIds = closedTabs.map((tab) => tab.id);
    closedTabs.forEach((tab) => this.rememberClosedTab(tab));
    this.tabs = this.tabs.filter((tab) => tab.id === id);
    this.reloadRequests = withoutReloadRequests(this.reloadRequests, closedIds);
    this.selectedTabIds = this.selectedTabIds.filter((tabId) => tabId === id);
    this.activeTabId = id;
    return closedIds;
  };

  restoreClosedTab = (): BrowserTab | null => {
    const snapshot = this.closedTabs.at(-1);

    if (!snapshot) {
      return null;
    }

    this.closedTabs = this.closedTabs.slice(0, -1);
    const tab = createTabFromSnapshot(snapshot);
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.id;
    return tab;
  };

  moveActiveTab = (direction: -1 | 1): void => {
    const activeTabId = this.activeTabId;

    if (!activeTabId) {
      return;
    }

    const existingTabIds = new Set(this.tabs.map((tab) => tab.id));
    const selectedIds = this.selectedTabIds.filter((tabId) => existingTabIds.has(tabId));
    this.selectedTabIds = selectedIds;
    const movingIds =
      selectedIds.includes(activeTabId) && selectedIds.length > 1
        ? new Set(selectedIds)
        : new Set([activeTabId]);

    if (movingIds.size === 0) {
      return;
    }

    const reordered = [...this.tabs];

    if (direction < 0) {
      for (let index = 1; index < reordered.length; index += 1) {
        if (movingIds.has(reordered[index].id) && !movingIds.has(reordered[index - 1].id)) {
          [reordered[index - 1], reordered[index]] = [reordered[index], reordered[index - 1]];
        }
      }
    } else {
      for (let index = reordered.length - 2; index >= 0; index -= 1) {
        if (movingIds.has(reordered[index].id) && !movingIds.has(reordered[index + 1].id)) {
          [reordered[index], reordered[index + 1]] = [reordered[index + 1], reordered[index]];
        }
      }
    }

    this.tabs = reordered;
  };

  isTabSelected = (id: string): boolean => this.selectedTabIds.includes(id);

  toggleTabSelection = (id: string): void => {
    if (!this.tabs.some((tab) => tab.id === id)) {
      return;
    }

    this.selectedTabIds = this.selectedTabIds.includes(id)
      ? this.selectedTabIds.filter((tabId) => tabId !== id)
      : [...this.selectedTabIds, id];
  };

  clearTabSelection = (): void => {
    this.selectedTabIds = [];
  };

  requestReload = (id: string): number | null => {
    if (!this.tabs.some((tab) => tab.id === id)) {
      return null;
    }

    const nextRequest = (this.reloadRequests[id] ?? 0) + 1;
    this.reloadRequests = {
      ...this.reloadRequests,
      [id]: nextRequest
    };
    this.activeTabId = id;
    return nextRequest;
  };

  nextTab = (): void => {
    if (this.tabs.length === 0) {
      return;
    }

    const index = this.activeIndex === -1 ? 0 : (this.activeIndex + 1) % this.tabs.length;
    this.activeTabId = this.tabs[index].id;
  };

  previousTab = (): void => {
    if (this.tabs.length === 0) {
      return;
    }

    const index =
      this.activeIndex === -1
        ? this.tabs.length - 1
        : (this.activeIndex - 1 + this.tabs.length) % this.tabs.length;
    this.activeTabId = this.tabs[index].id;
  };

  tabByIndex = (index: number): void => {
    const tab = this.tabs[index];
    if (tab) {
      this.activeTabId = tab.id;
    }
  };

  lastTab = (): void => {
    const tab = this.tabs.at(-1);
    if (tab) {
      this.activeTabId = tab.id;
    }
  };

  private rememberClosedTab(tab: BrowserTab): void {
    this.closedTabs = [...this.closedTabs, snapshotTab(tab)].slice(-MAX_CLOSED_TABS);
  }
}

function createTab(url: string, title: string, isInternal: boolean): BrowserTab {
  const id = createTabId();

  return {
    id,
    title,
    url,
    favicon_url: null,
    is_loading: false,
    can_go_back: false,
    can_go_forward: false,
    webviewLabel: `tab_${id}`,
    isInternal,
    history: [url],
    historyIndex: 0
  };
}

function createTabFromSnapshot(snapshot: ClosedTabSnapshot): BrowserTab {
  const tab = createTab(snapshot.url, snapshot.title, snapshot.isInternal);

  return {
    ...tab,
    favicon_url: snapshot.favicon_url,
    is_loading: false,
    can_go_back: snapshot.can_go_back,
    can_go_forward: snapshot.can_go_forward,
    history: [...snapshot.history],
    historyIndex: snapshot.historyIndex
  };
}

function snapshotTab(tab: BrowserTab): ClosedTabSnapshot {
  return {
    title: tab.title,
    url: tab.url,
    favicon_url: tab.favicon_url,
    is_loading: false,
    can_go_back: tab.can_go_back,
    can_go_forward: tab.can_go_forward,
    isInternal: tab.isInternal,
    history: [...tab.history],
    historyIndex: tab.historyIndex
  };
}

function internalTitle(url: InternalUrl): string {
  if (url === BROWSER_SETTINGS_URL) {
    return titleForInternalUrl(BROWSER_SETTINGS_URL);
  }

  return titleForInternalUrl(BROWSER_HOME_URL);
}

function createTabId(): string {
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function withoutReloadRequests(
  reloadRequests: Record<string, number>,
  tabIds: string[]
): Record<string, number> {
  const removedIds = new Set(tabIds);

  return Object.fromEntries(
    Object.entries(reloadRequests).filter(([tabId]) => !removedIds.has(tabId))
  );
}

export const tabsState = new TabsState();
