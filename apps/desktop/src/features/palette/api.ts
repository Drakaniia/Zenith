// Search suggestions and instant-answer fetches for the address bar.
import { invoke } from "@tauri-apps/api/core";

export const searchApi = {
  searchSuggest: (engine: string, query: string) =>
    invoke<string[]>("search_suggest", { engine, query }),
  instantFetch: (url: string) => invoke<string>("search_instant_fetch", { url }),
};
