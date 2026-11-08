// Search suggestions for the address bar.
import { invoke } from "@tauri-apps/api/core";

export const searchApi = {
  searchSuggest: (engine: string, query: string) =>
    invoke<string[]>("search_suggest", { engine, query }),
};
