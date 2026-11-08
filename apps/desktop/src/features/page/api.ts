// Backend calls for find in page and permission prompts.
import { invoke } from "@tauri-apps/api/core";
import type { PermissionRequest } from "./types";

export const pageApi = {
  find: (
    id: string,
    op: "start" | "next" | "prev" | "stop",
    text = "",
    caseSensitive = false,
    wholeWord = false,
  ) => invoke<void>("find_in_page", { id, op, text, caseSensitive, wholeWord }),
  permissionRespond: (requestId: number, allow: boolean, remember: boolean) =>
    invoke<void>("permission_respond", { requestId, allow, remember }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "permission-request": PermissionRequest;
    "find-result": { id: string; matches: number; active: number };
  }
}
