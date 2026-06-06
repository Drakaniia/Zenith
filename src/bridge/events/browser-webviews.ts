import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getElectronBridge } from '../runtime';
import type {
  BrowserWebviewLoadEvent,
  BrowserWebviewNavigationEvent,
  BrowserWebviewTitleEvent
} from '../../shared/types/events';

export const BROWSER_WEBVIEW_NAVIGATION_EVENT = 'browser-webview-navigation';
export const BROWSER_WEBVIEW_TITLE_EVENT = 'browser-webview-title';
export const BROWSER_WEBVIEW_LOAD_EVENT = 'browser-webview-load';

export interface BrowserWebviewEventHandlers {
  onNavigation: (event: BrowserWebviewNavigationEvent) => void;
  onTitle: (event: BrowserWebviewTitleEvent) => void;
  onLoad: (event: BrowserWebviewLoadEvent) => void;
}

export async function listenBrowserWebviewEvents(
  handlers: BrowserWebviewEventHandlers
): Promise<UnlistenFn> {
  const electron = getElectronBridge();
  if (electron) {
    const unlisteners = [
      electron.on<BrowserWebviewNavigationEvent>(
        BROWSER_WEBVIEW_NAVIGATION_EVENT,
        handlers.onNavigation
      ),
      electron.on<BrowserWebviewTitleEvent>(BROWSER_WEBVIEW_TITLE_EVENT, handlers.onTitle),
      electron.on<BrowserWebviewLoadEvent>(BROWSER_WEBVIEW_LOAD_EVENT, handlers.onLoad)
    ];

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }

  const unlisteners = await Promise.all([
    listen<BrowserWebviewNavigationEvent>(BROWSER_WEBVIEW_NAVIGATION_EVENT, (event) => {
      handlers.onNavigation(event.payload);
    }),
    listen<BrowserWebviewTitleEvent>(BROWSER_WEBVIEW_TITLE_EVENT, (event) => {
      handlers.onTitle(event.payload);
    }),
    listen<BrowserWebviewLoadEvent>(BROWSER_WEBVIEW_LOAD_EVENT, (event) => {
      handlers.onLoad(event.payload);
    })
  ]);

  return () => {
    unlisteners.forEach((unlisten) => unlisten());
  };
}
