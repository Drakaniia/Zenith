import { getVersion } from '@tauri-apps/api/app';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';

export function isTauriRuntime(): boolean {
  return isTauri();
}

export function getAppVersion(): Promise<string | null> {
  if (!isTauriRuntime()) {
    return Promise.resolve(null);
  }

  return getVersion();
}

export function clearAllBrowsingData(): Promise<void> {
  if (!isTauriRuntime()) {
    return Promise.reject(new Error('Browsing data can only be cleared in the Tauri app.'));
  }

  return getCurrentWebview().clearAllBrowsingData();
}
