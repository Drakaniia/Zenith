// The bridge to the Rust backend: typed backend events. Each feature's api.ts holds its own
// commands (`invoke`) and declares the events it receives by merging into `BackendEvents`.
// Only the chrome webview can call the backend.
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Event name → payload, filled in by the features (`declare module` in their api.ts). */
export interface BackendEvents {}

export function on<K extends keyof BackendEvents>(
  event: K,
  handler: (payload: BackendEvents[K]) => void,
): Promise<UnlistenFn> {
  // Window-scoped: the backend sends each window its own tab, shortcut and close events (and
  // app-wide ones to all). The global `listen` would also receive the other windows' events.
  return getCurrentWebviewWindow().listen<BackendEvents[K]>(event, (e) =>
    handler(e.payload),
  );
}
