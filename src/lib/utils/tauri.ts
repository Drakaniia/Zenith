import { invoke, isTauri } from '@tauri-apps/api/core';

export interface Tab {
  id: string;
  title: string;
  url: string;
  favicon_url: string | null;
  is_loading: boolean;
  can_go_back: boolean;
  can_go_forward: boolean;
}

export interface Bookmark {
  id: string;
  title: string;
  url: string;
  created_at: string;
}

export interface HistoryEntry {
  id: string;
  title: string;
  url: string;
  visited_at: string;
}

export interface DownloadItem {
  id: string;
  filename: string;
  url: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: string;
  started_at: string;
}

export interface AppSettings {
  default_search_engine: string;
  homepage: string;
  theme: string;
  javascript_enabled: boolean;
  show_status_bar: boolean;
}

export interface PageMetadata {
  title: string;
  favicon_url: string | null;
}

export interface DownloadProgress {
  id: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: string;
}

export interface WebviewBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface FindResult {
  active_match: number;
  total_matches: number;
}

export type FindDirection = 'next' | 'previous';
export type BrowserWebviewLoadState = 'started' | 'finished';

export const BROWSER_WEBVIEW_NAVIGATION_EVENT = 'browser-webview-navigation';
export const BROWSER_WEBVIEW_TITLE_EVENT = 'browser-webview-title';
export const BROWSER_WEBVIEW_LOAD_EVENT = 'browser-webview-load';

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

export interface CreateBrowserWebviewOptions {
  label: string;
  url: string;
  bounds: WebviewBounds;
  javascriptEnabled: boolean;
}

export interface FindInBrowserWebviewOptions {
  label: string;
  query: string;
  direction: FindDirection;
  caseSensitive: boolean;
}

interface WebviewFindRequest {
  label: string;
  query: string;
  direction: FindDirection;
  case_sensitive: boolean;
}

export function isTauriRuntime(): boolean {
  return isTauri();
}

export function getBookmarks(): Promise<Bookmark[]> {
  return invoke<Bookmark[]>('get_bookmarks');
}

export function saveBookmark(title: string, url: string): Promise<Bookmark> {
  return invoke<Bookmark>('save_bookmark', { title, url });
}

export function deleteBookmark(id: string): Promise<void> {
  return invoke<void>('delete_bookmark', { id });
}

export function isBookmarked(url: string): Promise<boolean> {
  return invoke<boolean>('is_bookmarked', { url });
}

export function getDownloads(): Promise<DownloadItem[]> {
  return invoke<DownloadItem[]>('get_downloads');
}

export function startDownload(url: string, filename: string): Promise<string> {
  return invoke<string>('start_download', { url, filename });
}

export function cancelDownload(id: string): Promise<void> {
  return invoke<void>('cancel_download', { id });
}

export function clearCompletedDownloads(): Promise<void> {
  return invoke<void>('clear_completed_downloads');
}

export function getHistory(): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>('get_history');
}

export function addHistoryEntry(title: string, url: string): Promise<void> {
  return invoke<void>('add_history_entry', { title, url });
}

export function searchHistory(query: string): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>('search_history', { query });
}

export function clearHistory(): Promise<void> {
  return invoke<void>('clear_history');
}

export function deleteHistoryEntry(id: string): Promise<void> {
  return invoke<void>('delete_history_entry', { id });
}

export function normalizeUrl(url: string): Promise<string> {
  return invoke<string>('normalize_url', { url });
}

export function fetchPageMetadata(url: string): Promise<PageMetadata> {
  return invoke<PageMetadata>('fetch_page_metadata', { url });
}

export function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>('get_settings');
}

export function saveSettings(settings: AppSettings): Promise<void> {
  return invoke<void>('save_settings', { settings });
}

export function createBrowserWebview(options: CreateBrowserWebviewOptions): Promise<void> {
  return invoke<void>('create_browser_webview', {
    label: options.label,
    url: options.url,
    bounds: options.bounds,
    javascriptEnabled: options.javascriptEnabled
  });
}

export function navigateBrowserWebview(label: string, url: string): Promise<void> {
  return invoke<void>('navigate_browser_webview', { label, url });
}

export function reloadBrowserWebview(label: string): Promise<void> {
  return invoke<void>('reload_browser_webview', { label });
}

export function closeBrowserWebview(label: string): Promise<void> {
  return invoke<void>('close_browser_webview', { label });
}

export function showBrowserWebview(label: string): Promise<void> {
  return invoke<void>('show_browser_webview', { label });
}

export function hideBrowserWebview(label: string): Promise<void> {
  return invoke<void>('hide_browser_webview', { label });
}

export function setBrowserWebviewBounds(label: string, bounds: WebviewBounds): Promise<void> {
  return invoke<void>('set_browser_webview_bounds', { label, bounds });
}

export function getBrowserWebviewUrl(label: string): Promise<string> {
  return invoke<string>('get_browser_webview_url', { label });
}

export function evalBrowserWebview(label: string, script: string): Promise<void> {
  return invoke<void>('eval_browser_webview', { label, script });
}

export function findInBrowserWebview(options: FindInBrowserWebviewOptions): Promise<FindResult> {
  const request: WebviewFindRequest = {
    label: options.label,
    query: options.query,
    direction: options.direction,
    case_sensitive: options.caseSensitive
  };

  return invoke<FindResult>('find_in_browser_webview', { request });
}

export function clearBrowserWebviewFind(label: string): Promise<void> {
  return invoke<void>('clear_browser_webview_find', { label });
}
