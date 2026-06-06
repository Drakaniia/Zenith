import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { randomUUID } from 'node:crypto';

import { DEFAULT_SETTINGS } from './navigation.mjs';

export function createStore({ dataDir }) {
  const settingsPath = join(dataDir, 'settings.json');
  const bookmarksPath = join(dataDir, 'bookmarks.json');
  const historyPath = join(dataDir, 'history.json');
  const fileQueues = new Map();
  const withLock = (path, operation) => withFileLock(fileQueues, path, operation);

  return {
    getSettings: () => readSettings(settingsPath),
    saveSettings: (settings) =>
      withLock(settingsPath, () => writeJsonFile(settingsPath, normalizeSettings(settings))),
    getBookmarks: () => readArrayFile(bookmarksPath),
    saveBookmark: (title, url) =>
      withLock(bookmarksPath, () => saveBookmarkAtPath(bookmarksPath, title, url)),
    deleteBookmark: (id) => withLock(bookmarksPath, () => deleteBookmarkAtPath(bookmarksPath, id)),
    isBookmarked: (url) => isBookmarkedAtPath(bookmarksPath, url),
    getHistory: () => getHistoryAtPath(historyPath),
    addHistoryEntry: (title, url) =>
      withLock(historyPath, () => addHistoryEntryAtPath(historyPath, title, url)),
    searchHistory: (query) => searchHistoryAtPath(historyPath, query),
    clearHistory: () => withLock(historyPath, () => writeJsonFile(historyPath, [])),
    deleteHistoryEntry: (id) =>
      withLock(historyPath, () => deleteHistoryEntryAtPath(historyPath, id))
  };
}

export async function readSettings(path) {
  const decoded = await readJsonFile(path, DEFAULT_SETTINGS, { fallbackOnInvalid: true });
  return normalizeSettings(decoded);
}

export async function readArrayFile(path) {
  const decoded = await readJsonFile(path, []);
  return Array.isArray(decoded) ? decoded : [];
}

export async function readJsonFile(path, fallback, options = {}) {
  try {
    const contents = await readFile(path, 'utf8');
    return JSON.parse(contents);
  } catch (error) {
    if (error.code === 'ENOENT' || options.fallbackOnInvalid) {
      return clone(fallback);
    }

    throw error;
  }
}

export async function writeJsonFile(path, value) {
  await mkdir(dirname(path), { recursive: true });

  const tempPath = `${path}.${process.pid}.${Date.now()}.${randomUUID()}.tmp`;
  const backupPath = `${path}.bak`;
  await writeFile(tempPath, `${JSON.stringify(value, null, 2)}\n`, 'utf8');

  try {
    await rm(backupPath, { force: true });
    await rename(path, backupPath).catch((error) => {
      if (error.code !== 'ENOENT') {
        throw error;
      }
    });
    await rename(tempPath, path);
    await rm(backupPath, { force: true });
  } catch (error) {
    await rm(tempPath, { force: true });
    await rename(backupPath, path).catch(() => {});
    throw error;
  }
}

async function withFileLock(fileQueues, path, operation) {
  const previous = fileQueues.get(path) ?? Promise.resolve();
  const run = previous.catch(() => {}).then(operation);
  fileQueues.set(path, run);

  try {
    return await run;
  } finally {
    if (fileQueues.get(path) === run) {
      fileQueues.delete(path);
    }
  }
}

async function saveBookmarkAtPath(path, title, url) {
  const bookmarks = await readArrayFile(path);
  const existing = bookmarks.find((bookmark) => bookmark.url === url);

  if (existing) {
    existing.title = title;
    await writeJsonFile(path, bookmarks);
    return existing;
  }

  const bookmark = {
    id: randomUUID(),
    title,
    url,
    created_at: new Date().toISOString()
  };

  bookmarks.push(bookmark);
  await writeJsonFile(path, bookmarks);
  return bookmark;
}

async function deleteBookmarkAtPath(path, id) {
  const bookmarks = await readArrayFile(path);
  await writeJsonFile(
    path,
    bookmarks.filter((bookmark) => bookmark.id !== id)
  );
}

async function isBookmarkedAtPath(path, url) {
  const bookmarks = await readArrayFile(path);
  return bookmarks.some((bookmark) => bookmark.url === url);
}

async function getHistoryAtPath(path) {
  const entries = await readArrayFile(path);
  return sortHistory(entries);
}

async function addHistoryEntryAtPath(path, title, url) {
  const entries = await readArrayFile(path);
  const entry = {
    id: randomUUID(),
    title,
    url,
    visited_at: new Date().toISOString()
  };

  entries.push(entry);
  await writeJsonFile(path, entries);
  return entry;
}

async function searchHistoryAtPath(path, query) {
  const normalizedQuery = String(query ?? '').toLocaleLowerCase();
  const entries = await getHistoryAtPath(path);

  return entries.filter(
    (entry) =>
      entry.title.toLocaleLowerCase().includes(normalizedQuery) ||
      entry.url.toLocaleLowerCase().includes(normalizedQuery)
  );
}

async function deleteHistoryEntryAtPath(path, id) {
  const entries = await readArrayFile(path);
  await writeJsonFile(
    path,
    entries.filter((entry) => entry.id !== id)
  );
}

function normalizeSettings(settings) {
  return {
    ...DEFAULT_SETTINGS,
    ...settings,
    default_search_engine:
      settings?.default_search_engine === 'google'
        ? 'google'
        : DEFAULT_SETTINGS.default_search_engine,
    homepage:
      typeof settings?.homepage === 'string' ? settings.homepage : DEFAULT_SETTINGS.homepage,
    theme: settings?.theme === 'light' ? 'light' : DEFAULT_SETTINGS.theme,
    javascript_enabled:
      typeof settings?.javascript_enabled === 'boolean'
        ? settings.javascript_enabled
        : DEFAULT_SETTINGS.javascript_enabled,
    show_status_bar:
      typeof settings?.show_status_bar === 'boolean'
        ? settings.show_status_bar
        : DEFAULT_SETTINGS.show_status_bar
  };
}

function sortHistory(entries) {
  return [...entries].sort((left, right) => right.visited_at.localeCompare(left.visited_at));
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}
