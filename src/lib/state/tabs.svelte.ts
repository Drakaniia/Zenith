import type { Tab } from '../utils/tauri';

export type InternalUrl = 'browser://settings';
export type BrowserWebviewLabel = `tab_${string}`;

export interface BrowserTab extends Tab {
  webviewLabel: BrowserWebviewLabel;
  isInternal: boolean;
  history: string[];
  historyIndex: number;
}

const DEFAULT_HOME_URL = 'https://yandex.com';
const DEFAULT_TAB_TITLE = 'New Tab';
const SETTINGS_TITLE = 'Settings';

class TabsState {
  tabs = $state<BrowserTab[]>([]);
  activeTabId = $state<string | null>(null);
  reloadRequests = $state<Record<string, number>>({});
  activeTab = $derived(this.tabs.find((tab) => tab.id === this.activeTabId) ?? null);
  activeIndex = $derived(this.tabs.findIndex((tab) => tab.id === this.activeTabId));

  openTab = (url = DEFAULT_HOME_URL, title = DEFAULT_TAB_TITLE): BrowserTab => {
    const tab = createTab(url, title, false);
    this.tabs = [...this.tabs, tab];
    this.activeTabId = tab.id;
    return tab;
  };

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
    this.tabs = this.tabs.filter((tab) => tab.id !== id);
    this.reloadRequests = withoutReloadRequests(this.reloadRequests, [id]);

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

    const closedIds = this.tabs.filter((tab) => tab.id !== id).map((tab) => tab.id);
    this.tabs = this.tabs.filter((tab) => tab.id === id);
    this.reloadRequests = withoutReloadRequests(this.reloadRequests, closedIds);
    this.activeTabId = id;
    return closedIds;
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

function internalTitle(url: InternalUrl): string {
  if (url === 'browser://settings') {
    return SETTINGS_TITLE;
  }

  return DEFAULT_TAB_TITLE;
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
