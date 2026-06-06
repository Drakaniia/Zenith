import { invoke } from '@tauri-apps/api/core';
import { getElectronBridge } from './runtime';

export type BackendCommand =
  | 'add_history_entry'
  | 'cancel_download'
  | 'clear_all_browsing_data'
  | 'clear_browser_webview_find'
  | 'clear_completed_downloads'
  | 'clear_history'
  | 'close_all_browser_webviews'
  | 'close_all_shell_windows'
  | 'close_browser_webview'
  | 'close_window'
  | 'create_shell_window'
  | 'create_browser_webview'
  | 'delete_bookmark'
  | 'delete_history_entry'
  | 'eval_browser_webview'
  | 'fetch_page_metadata'
  | 'find_in_browser_webview'
  | 'focus_shell'
  | 'get_app_version'
  | 'get_bookmarks'
  | 'get_browser_webview_url'
  | 'get_downloads'
  | 'get_history'
  | 'get_settings'
  | 'get_window_state'
  | 'hide_window'
  | 'hide_browser_webview'
  | 'is_window_fullscreen'
  | 'is_bookmarked'
  | 'maximize_window'
  | 'minimize_window'
  | 'navigate_browser_webview'
  | 'normalize_url'
  | 'open_external_url'
  | 'reload_browser_webview'
  | 'restore_window'
  | 'save_bookmark'
  | 'save_settings'
  | 'search_history'
  | 'set_browser_webview_bounds'
  | 'set_window_fullscreen'
  | 'show_browser_webview'
  | 'start_download'
  | 'toggle_devtools';

export function invokeCommand<T>(
  command: BackendCommand,
  args?: Record<string, unknown>
): Promise<T> {
  const electron = getElectronBridge();
  if (electron) {
    return electron.invoke<T>(command, args);
  }

  return invoke<T>(command, args);
}
