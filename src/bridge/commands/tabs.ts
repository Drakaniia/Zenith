import { Webview } from '@tauri-apps/api/webview';
import type { FindResult, WebviewBounds } from '../../shared/types/backend';
import type { FindDirection } from '../../shared/types/events';
import { invokeCommand } from '../invoke';

export interface CreateBrowserWebviewOptions {
  label: string;
  url: string;
  bounds: WebviewBounds;
  javascriptEnabled: boolean;
  incognito: boolean;
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

export function createBrowserWebview(options: CreateBrowserWebviewOptions): Promise<void> {
  return invokeCommand<void>('create_browser_webview', {
    label: options.label,
    url: options.url,
    bounds: options.bounds,
    javascriptEnabled: options.javascriptEnabled,
    incognito: options.incognito
  });
}

export function navigateBrowserWebview(label: string, url: string): Promise<void> {
  return invokeCommand<void>('navigate_browser_webview', { label, url });
}

export function reloadBrowserWebview(label: string): Promise<void> {
  return invokeCommand<void>('reload_browser_webview', { label });
}

export function closeBrowserWebview(label: string): Promise<void> {
  return invokeCommand<void>('close_browser_webview', { label });
}

export function closeAllBrowserWebviews(): Promise<void> {
  return invokeCommand<void>('close_all_browser_webviews');
}

export function showBrowserWebview(label: string): Promise<void> {
  return invokeCommand<void>('show_browser_webview', { label });
}

export function hideBrowserWebview(label: string): Promise<void> {
  return invokeCommand<void>('hide_browser_webview', { label });
}

export function setBrowserWebviewBounds(label: string, bounds: WebviewBounds): Promise<void> {
  return invokeCommand<void>('set_browser_webview_bounds', { label, bounds });
}

export async function setBrowserWebviewZoom(label: string, scaleFactor: number): Promise<void> {
  const webview = await Webview.getByLabel(label);

  if (!webview) {
    throw new Error(`Browser webview not found: ${label}`);
  }

  await webview.setZoom(scaleFactor);
}

export async function focusBrowserWebview(label: string): Promise<void> {
  const webview = await Webview.getByLabel(label);

  if (!webview) {
    throw new Error(`Browser webview not found: ${label}`);
  }

  await webview.setFocus();
}

export function getBrowserWebviewUrl(label: string): Promise<string> {
  return invokeCommand<string>('get_browser_webview_url', { label });
}

export function evalBrowserWebview(label: string, script: string): Promise<void> {
  return invokeCommand<void>('eval_browser_webview', { label, script });
}

export function findInBrowserWebview(options: FindInBrowserWebviewOptions): Promise<FindResult> {
  const request: WebviewFindRequest = {
    label: options.label,
    query: options.query,
    direction: options.direction,
    case_sensitive: options.caseSensitive
  };

  return invokeCommand<FindResult>('find_in_browser_webview', { request });
}

export function clearBrowserWebviewFind(label: string): Promise<void> {
  return invokeCommand<void>('clear_browser_webview_find', { label });
}
