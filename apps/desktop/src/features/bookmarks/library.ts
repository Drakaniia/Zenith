// Shared pieces of the Bookmarks Library page (BookmarksPage and its Library* parts).

/** What the Library shows on the right: a folder, a tag, everything, or History / Downloads. */
export type Scope =
  | { kind: 'all' }
  | { kind: 'folder'; id: number }
  | { kind: 'tag'; tag: string }
  | { kind: 'history' }
  | { kind: 'downloads' };

export type Col = 'tags' | 'location' | 'keyword' | 'added' | 'modified';
export type SortBy = 'none' | 'name' | Col;
export const COLS: { id: Col; label: string }[] = [
  { id: 'tags', label: 'Tags' },
  { id: 'location', label: 'Location' },
  { id: 'keyword', label: 'Keyword' },
  { id: 'added', label: 'Added' },
  { id: 'modified', label: 'Last modified' },
];

export const date = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
