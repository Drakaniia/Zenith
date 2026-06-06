import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { NativeShortcutCommand, NativeShortcutEvent } from '../../shared/types/events';
import { getElectronBridge } from '../runtime';

export const BROWSER_SHORTCUT_EVENT = 'browser-shortcut';

export async function listenNativeShortcutEvents(
  handler: (command: NativeShortcutCommand) => void
): Promise<UnlistenFn> {
  const electron = getElectronBridge();
  if (electron) {
    return electron.on<NativeShortcutEvent>(BROWSER_SHORTCUT_EVENT, (event) => {
      handler(event.command);
    });
  }

  return listen<NativeShortcutEvent>(BROWSER_SHORTCUT_EVENT, (event) => {
    handler(event.payload.command);
  });
}
