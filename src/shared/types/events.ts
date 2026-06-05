export type FindDirection = 'next' | 'previous';
export type BrowserWebviewLoadState = 'started' | 'finished';
export type NativeShortcutCommand =
  | 'new-tab'
  | 'new-window'
  | 'new-incognito-window'
  | 'restore-tab'
  | 'close-tab'
  | 'close-window'
  | 'quit-browser'
  | 'next-tab'
  | 'previous-tab'
  | 'move-tab-left'
  | 'move-tab-right'
  | 'tab-1'
  | 'tab-2'
  | 'tab-3'
  | 'tab-4'
  | 'tab-5'
  | 'tab-6'
  | 'tab-7'
  | 'tab-8'
  | 'last-tab'
  | 'focus-address'
  | 'reload'
  | 'hard-reload'
  | 'back'
  | 'forward'
  | 'home'
  | 'find'
  | 'find-next'
  | 'find-previous'
  | 'history'
  | 'hide-window'
  | 'minimize-window'
  | 'downloads'
  | 'bookmarks'
  | 'toggle-bookmarks'
  | 'settings'
  | 'bookmark-page'
  | 'bookmark-all'
  | 'print'
  | 'zoom-in'
  | 'zoom-out'
  | 'zoom-reset'
  | 'toggle-fullscreen'
  | 'focus-web-content'
  | 'focus-next-shell-area'
  | 'focus-toolbar-start'
  | 'focus-toolbar-end'
  | 'open-browser-menu'
  | 'keyboard-help'
  | 'caret-browsing'
  | 'devtools'
  | 'save-page'
  | 'open-file'
  | 'view-source'
  | 'profile-switcher';

export interface NativeShortcutEvent {
  command: NativeShortcutCommand;
}

export interface BrowserWebviewNavigationEvent {
  label: string;
  url: string;
}

export interface BrowserWebviewTitleEvent {
  label: string;
  title: string;
}

export interface BrowserWebviewLoadEvent {
  label: string;
  url: string;
  state: BrowserWebviewLoadState;
}
