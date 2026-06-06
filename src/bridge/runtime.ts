import { getVersion } from '@tauri-apps/api/app';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';

export function getElectronBridge(): AmpuiElectronBridge | null {
  if (typeof window === 'undefined') {
    return null;
  }

  return window.ampuiElectron ?? null;
}

export function isElectronRuntime(): boolean {
  return getElectronBridge() !== null;
}

export function isTauriRuntime(): boolean {
  if (isElectronRuntime()) {
    return false;
  }

  return isTauri();
}

export function isDesktopRuntime(): boolean {
  return isElectronRuntime() || isTauriRuntime();
}

export async function getAppVersion(): Promise<string | null> {
  const electron = getElectronBridge();
  if (electron) {
    return electron.invoke<string | null>('get_app_version');
  }

  if (isTauriRuntime()) {
    return getVersion();
  }

  return Promise.resolve(null);
}

export async function clearAllBrowsingData(): Promise<void> {
  const electron = getElectronBridge();
  if (electron) {
    await electron.invoke<void>('clear_all_browsing_data');
    return;
  }

  if (!isTauriRuntime()) {
    return Promise.reject(new Error('Browsing data can only be cleared in the desktop app.'));
  }

  return getCurrentWebview().clearAllBrowsingData();
}
