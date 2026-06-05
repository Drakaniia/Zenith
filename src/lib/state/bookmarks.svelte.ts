import {
  deleteBookmark,
  getBookmarks,
  isBookmarked,
  saveBookmark,
  type Bookmark
} from '../utils/tauri';
import { toastsState } from './toasts.svelte.js';

class BookmarksState {
  items = $state<Bookmark[]>([]);
  isLoading = $state(false);
  isSaving = $state(false);
  bookmarkedUrls = $derived(this.items.map((bookmark) => bookmark.url));

  load = async (): Promise<void> => {
    this.isLoading = true;

    try {
      this.items = await getBookmarks();
    } catch (error) {
      toastsState.error(`Failed to load bookmarks: ${errorMessage(error)}`);
    } finally {
      this.isLoading = false;
    }
  };

  addBookmark = async (title: string, url: string): Promise<Bookmark | null> => {
    this.isSaving = true;

    try {
      const bookmark = await saveBookmark(title, url);
      this.items = upsertBookmark(this.items, bookmark);
      return bookmark;
    } catch (error) {
      toastsState.error(`Failed to save bookmark: ${errorMessage(error)}`);
      return null;
    } finally {
      this.isSaving = false;
    }
  };

  removeBookmark = async (id: string): Promise<void> => {
    try {
      await deleteBookmark(id);
      this.items = this.items.filter((bookmark) => bookmark.id !== id);
    } catch (error) {
      toastsState.error(`Failed to delete bookmark: ${errorMessage(error)}`);
    }
  };

  findByUrl = (url: string): Bookmark | null =>
    this.items.find((bookmark) => bookmark.url === url) ?? null;

  removeBookmarkByUrl = async (url: string): Promise<boolean> => {
    let bookmark = this.findByUrl(url);

    if (!bookmark) {
      await this.load();
      bookmark = this.findByUrl(url);
    }

    if (!bookmark) {
      return false;
    }

    await this.removeBookmark(bookmark.id);
    return !this.items.some((item) => item.id === bookmark.id);
  };

  checkIsBookmarked = async (url: string): Promise<boolean> => {
    try {
      return await isBookmarked(url);
    } catch (error) {
      toastsState.error(`Failed to check bookmark: ${errorMessage(error)}`);
      return false;
    }
  };

  isBookmarkedLocally = (url: string): boolean => this.bookmarkedUrls.includes(url);
}

function upsertBookmark(bookmarks: Bookmark[], bookmark: Bookmark): Bookmark[] {
  const index = bookmarks.findIndex((item) => item.id === bookmark.id);
  if (index === -1) {
    return [bookmark, ...bookmarks];
  }

  return bookmarks.map((item) => (item.id === bookmark.id ? bookmark : item));
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  return 'Unknown error';
}

export const bookmarksState = new BookmarksState();
