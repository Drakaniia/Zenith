// Backend calls for tab webviews and the session.
import { invoke } from "@tauri-apps/api/core";
import type { OpenRequest, TabCreated, TabPatch } from "./types";

export const tabsApi = {
  tabCreate: (args: {
    id: string;
    url?: string | null;
    glance?: boolean;
    muted?: boolean;
    zoom?: number;
    scrollX?: number;
    scrollY?: number;
    /** An extension popup's opener page (what its `tabs.query` should find). */
    openerUrl?: string | null;
  }) => invoke<void>("tab_create", { args }),
  tabClose: (id: string) => invoke<void>("tab_close", { id }),
  tabNavigate: (id: string, url: string) =>
    invoke<void>("tab_navigate", { id, url }),
  tabCommand: <T = unknown>(id: string, cmd: string, arg?: unknown) =>
    invoke<T>("tab_command", { id, cmd, arg: arg ?? null }),
  tabSleep: (id: string) =>
    invoke<{ x: number; y: number } | null>("tab_sleep", { id }),
  tabSetGlance: (id: string, glance: boolean) =>
    invoke<void>("tab_set_glance", { id, glance }),
  tabContentSize: (id: string) =>
    invoke<{ x: number; y: number } | null>("tab_content_size", { id }),
  /** This window's saved tabs. `keep` says whether to restore its regular tabs too: null leaves
   *  it to the "restore previous session" setting, false keeps only spaces, essentials and pinned
   *  tabs (a new window), true restores everything (a reopened window). `fresh` asks for new tab
   *  ids, as the tabs came from another window. */
  sessionLoad: () =>
    invoke<{ data: string | null; keep: boolean | null; fresh: boolean }>("session_load"),
  sessionSave: (data: string) => invoke<void>("session_save", { data }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "tab-update": TabPatch;
    "tab-created": TabCreated;
    "tab-open-request": OpenRequest;
    "tab-close-request": { id: string };
    "tab-focused": { id: string };
    "popup-blocked": { tabId: string; url: string };
  }
}
