// Backend calls for browsing history.
import { invoke } from "@tauri-apps/api/core";
import type { HistoryEntry } from "./types";

export const historyApi = {
  historySearch: (text: string, limit = 50, offset = 0) =>
    invoke<HistoryEntry[]>("history_search", { text, limit, offset }),
  historyTop: (limit = 40) => invoke<HistoryEntry[]>("history_top", { limit }),
  historyDelete: (ids: number[]) => invoke<number>("history_delete", { ids }),
  historyClear: (sinceMs: number | null) =>
    invoke<number>("history_clear", { sinceMs }),
  historyPrune: (days: number) => invoke<void>("history_prune", { days }),
};
