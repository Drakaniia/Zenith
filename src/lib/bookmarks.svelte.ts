import { api } from "./ipc";
import type { Bookmark } from "./types";

class BookmarkStore {
  items = $state<Bookmark[]>([]);
  urls = $derived(new Set(this.items.map((b) => b.url)));
  #loaded = false;

  async load(force = false) {
    if (this.#loaded && !force) return;
    this.#loaded = true;
    try {
      this.items = await api.bookmarksList();
    } catch (e) {
      console.warn("bookmarks unavailable", e);
    }
  }

  has(url: string | undefined) {
    return !!url && this.urls.has(url);
  }

  async add(url: string, title: string, folder = "") {
    await api.bookmarkAdd({ url, title, folder });
    await this.load(true);
  }

  async remove(b: { id?: number; url?: string }) {
    await api.bookmarkRemove(b.id ?? null, b.id ? null : (b.url ?? null));
    await this.load(true);
  }

  async update(b: Bookmark) {
    await api.bookmarkUpdate(b);
    await this.load(true);
  }

  /** Star button: bookmark the page, or remove every bookmark of it. */
  async toggle(url: string, title: string) {
    if (this.has(url)) await this.remove({ url });
    else await this.add(url, title);
    return this.has(url);
  }

  folders = $derived(
    [...new Set(this.items.map((b) => b.folder).filter(Boolean))].sort(),
  );
}

export const bookmarks = new BookmarkStore();
