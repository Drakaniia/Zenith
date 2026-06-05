import { getAllWindows, getCurrentWindow } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import type { WindowState } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export interface ShellWindowOptions {
  label: string;
  url: string;
  title: string;
  incognito: boolean;
}

export function getWindowState(): Promise<WindowState> {
  return invokeCommand<WindowState>('get_window_state');
}

export function minimizeWindow(): Promise<WindowState> {
  return invokeCommand<WindowState>('minimize_window');
}

export function maximizeWindow(): Promise<WindowState> {
  return invokeCommand<WindowState>('maximize_window');
}

export function restoreWindow(): Promise<WindowState> {
  return invokeCommand<WindowState>('restore_window');
}

export function closeWindow(): Promise<void> {
  return invokeCommand<void>('close_window');
}

export async function isCurrentWindowFullscreen(): Promise<boolean> {
  return getCurrentWindow().isFullscreen();
}

export function setCurrentWindowFullscreen(fullscreen: boolean): Promise<void> {
  return getCurrentWindow().setFullscreen(fullscreen);
}

export function hideCurrentWindow(): Promise<void> {
  return getCurrentWindow().hide();
}

export async function closeAllShellWindows(): Promise<void> {
  const windows = await getAllWindows();

  if (windows.length === 0) {
    await closeWindow();
    return;
  }

  await Promise.all(windows.map((appWindow) => appWindow.close()));
}

export async function createShellWindow(options: ShellWindowOptions): Promise<void> {
  const shellWindow = new WebviewWindow(options.label, {
    url: options.url,
    title: options.title,
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 600,
    decorations: false,
    resizable: true,
    center: true,
    focus: true,
    incognito: options.incognito
  });

  await new Promise<void>((resolve, reject) => {
    void shellWindow.once('tauri://created', () => {
      resolve();
    });
    void shellWindow.once('tauri://error', (event) => {
      reject(new Error(String(event.payload || 'Failed to create browser window.')));
    });
  });
}

export function openExternalUrl(url: string): Promise<void> {
  return openExternal(url);
}
