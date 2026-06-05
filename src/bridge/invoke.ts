import { invoke } from '@tauri-apps/api/core';

export type BackendCommand =
  | 'add_history_entry'
  | 'cancel_download'
  | 'clear_browser_webview_find'
  | 'clear_completed_downloads'
  | 'clear_history'
  | 'close_browser_webview'
  | 'close_window'
  | 'create_browser_webview'
  | 'delete_bookmark'
  | 'delete_history_entry'
  | 'eval_browser_webview'
  | 'fetch_page_metadata'
  | 'find_in_browser_webview'
  | 'get_bookmarks'
  | 'get_browser_webview_url'
  | 'get_downloads'
  | 'get_history'
  | 'get_settings'
  | 'get_window_state'
  | 'hide_browser_webview'
  | 'is_bookmarked'
  | 'maximize_window'
  | 'minimize_window'
  | 'navigate_browser_webview'
  | 'normalize_url'
  | 'reload_browser_webview'
  | 'restore_window'
  | 'save_bookmark'
  | 'save_settings'
  | 'search_history'
  | 'set_browser_webview_bounds'
  | 'show_browser_webview'
  | 'start_download';

export function invokeCommand<T>(
  command: BackendCommand,
  args?: Record<string, unknown>
): Promise<T> {
  return invoke<T>(command, args);
}
