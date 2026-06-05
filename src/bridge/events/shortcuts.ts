import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { NativeShortcutCommand, NativeShortcutEvent } from '../../shared/types/events';

export const BROWSER_SHORTCUT_EVENT = 'browser-shortcut';

export async function listenNativeShortcutEvents(
  handler: (command: NativeShortcutCommand) => void
): Promise<UnlistenFn> {
  return listen<NativeShortcutEvent>(BROWSER_SHORTCUT_EVENT, (event) => {
    handler(event.payload.command);
  });
}
