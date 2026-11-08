// Development-only browser preview of the chrome UI (`pnpm dev`, then open http://127.0.0.1:1420).
// It stands in for the Rust backend with Tauri's own IPC mocks and a seeded session, so the shell
// can be checked visually without building the app. Page webviews do not exist here: live tabs
// show a placeholder page. Query parameters: ?mode=single|multiple|collapsed, ?theme=light|dark,
// ?compact=1, ?split=1, ?collapsedPins=1, ?fre=1 (first run: the setup assistant, with an
// empty session), ?fre=import (also: no other browsers found).
//
// main.ts only imports this module when running under Vite's dev server outside Tauri, so it
// never ships in a build.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { UNHANDLED, type Handler } from "./handler";
import { handleSession } from "./session";
import { handleBookmarks } from "./bookmarks";
import { handleNotes } from "./notes";
import { handleProfiles } from "./profiles";
import { handlePasswords } from "./passwords";
import { handleOther } from "./stubs";

const handlers: Handler[] = [handleSession, handleBookmarks, handleNotes, handleProfiles, handlePasswords, handleOther];

export function installPreviewMock() {
  document.documentElement.dataset.preview = "true";
  const style = document.createElement("style");
  // A stand-in page under each live tab's pane, since there are no native page views here.
  style.textContent = `
    html[data-preview] .web:not(.waiting) {
      background:
        linear-gradient(var(--page-bg), var(--page-bg)) padding-box,
        var(--page-bg);
      background-image:
        linear-gradient(180deg, color-mix(in srgb, var(--text) 6%, transparent) 0 64px, transparent 64px),
        repeating-linear-gradient(180deg, transparent 0 28px, color-mix(in srgb, var(--text) 5%, transparent) 28px 40px);
      background-size: 100% 100%, 60% 100%;
      background-position: 0 0, 40px 110px;
      background-repeat: no-repeat;
    }`;
  document.head.append(style);

  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      for (const handle of handlers) {
        const answer = handle(cmd, a);
        if (answer !== UNHANDLED) return answer;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
}
