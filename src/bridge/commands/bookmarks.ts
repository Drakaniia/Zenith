import type { Bookmark } from '../../shared/types/backend';
import { invokeCommand } from '../invoke';

export function getBookmarks(): Promise<Bookmark[]> {
  return invokeCommand<Bookmark[]>('get_bookmarks');
}

export function saveBookmark(title: string, url: string): Promise<Bookmark> {
  return invokeCommand<Bookmark>('save_bookmark', { title, url });
}

export function deleteBookmark(id: string): Promise<void> {
  return invokeCommand<void>('delete_bookmark', { id });
}

export function isBookmarked(url: string): Promise<boolean> {
  return invokeCommand<boolean>('is_bookmarked', { url });
}
