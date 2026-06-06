import { createDownloadManager } from './downloads.mjs';
import { fetchPageMetadata, normalizeUrlWithSettings } from './navigation.mjs';
import { createStore } from './store.mjs';

export function createCommandHandlers(options) {
  const store = options.store ?? createStore({ dataDir: options.dataDir });
  const downloads =
    options.downloads ??
    createDownloadManager({
      downloadsDir: options.downloadsDir ?? options.dataDir,
      emit: options.emit
    });

  const currentWindow = () => options.getFocusedWindow?.() ?? options.appWindow ?? null;

  return {
    async invoke(command, args = {}) {
      switch (command) {
        case 'get_settings':
          return store.getSettings();
        case 'save_settings':
          return store.saveSettings(args.settings);
        case 'get_bookmarks':
          return store.getBookmarks();
        case 'save_bookmark':
          return store.saveBookmark(args.title, args.url);
        case 'delete_bookmark':
          return store.deleteBookmark(args.id);
        case 'is_bookmarked':
          return store.isBookmarked(args.url);
        case 'get_history':
          return store.getHistory();
        case 'add_history_entry':
          await store.addHistoryEntry(args.title, args.url);
          return undefined;
        case 'search_history':
          return store.searchHistory(args.query);
        case 'clear_history':
          return store.clearHistory();
        case 'delete_history_entry':
          return store.deleteHistoryEntry(args.id);
        case 'normalize_url':
          return normalizeUrlWithSettings(args.url, await store.getSettings());
        case 'fetch_page_metadata':
          return fetchPageMetadata(args.url);
        case 'get_downloads':
          return downloads.getDownloads();
        case 'start_download':
          return downloads.startDownload(args.url, args.filename);
        case 'cancel_download':
          return downloads.cancelDownload(args.id);
        case 'clear_completed_downloads':
          return downloads.clearCompletedDownloads();
        case 'get_window_state':
          return windowState(currentWindow());
        case 'minimize_window':
          currentWindow()?.minimize?.();
          return windowState(currentWindow());
        case 'maximize_window':
          currentWindow()?.maximize?.();
          return windowState(currentWindow());
        case 'restore_window':
          currentWindow()?.restore?.();
          currentWindow()?.unmaximize?.();
          return windowState(currentWindow());
        case 'close_window':
          currentWindow()?.close?.();
          return undefined;
        case 'hide_window':
          currentWindow()?.hide?.();
          return undefined;
        case 'is_window_fullscreen':
          return Boolean(currentWindow()?.isFullScreen?.());
        case 'set_window_fullscreen':
          currentWindow()?.setFullScreen?.(Boolean(args.fullscreen));
          return undefined;
        case 'close_all_shell_windows':
          for (const window of options.getAllWindows?.() ?? []) {
            window.close();
          }
          return undefined;
        case 'create_shell_window':
          await options.createWindow?.(args.options ?? args);
          return undefined;
        case 'open_external_url':
          if (!isSafeExternalUrl(args.url)) {
            throw new Error(`Refused to open unsupported external URL: ${args.url}`);
          }

          await options.shell?.openExternal?.(args.url);
          return undefined;
        case 'get_app_version':
          return options.app?.getVersion?.() ?? null;
        case 'clear_all_browsing_data':
          await options.clearAllBrowsingData?.();
          return undefined;
        case 'focus_shell':
          currentWindow()?.focus?.();
          return undefined;
        case 'toggle_devtools':
          toggleDevtools(currentWindow());
          return undefined;
        default:
          throw new Error(`Unsupported Electron command: ${command}`);
      }
    }
  };
}

export function registerElectronCommands({ ipcMain, ...options }) {
  const handlers = createCommandHandlers(options);

  ipcMain.handle('ampui:invoke', (_event, command, args) => handlers.invoke(command, args));
  return handlers;
}

function windowState(window) {
  return {
    is_minimized: Boolean(window?.isMinimized?.()),
    is_maximized: Boolean(window?.isMaximized?.())
  };
}

function toggleDevtools(window) {
  const webContents = window?.webContents;

  if (!webContents) {
    return;
  }

  if (webContents.isDevToolsOpened()) {
    webContents.closeDevTools();
  } else {
    webContents.openDevTools({ mode: 'detach' });
  }
}

function isSafeExternalUrl(url) {
  try {
    const parsed = new URL(url);
    return ['http:', 'https:', 'mailto:'].includes(parsed.protocol);
  } catch {
    return false;
  }
}
