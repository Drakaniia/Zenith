import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, it } from 'node:test';

import { createCommandHandlers } from './commands.mjs';
import { createStore } from './store.mjs';

const tempDirs = [];

async function tempDataDir() {
  const dir = await mkdtemp(join(tmpdir(), 'ampui-electron-commands-'));
  tempDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

describe('Electron backend command handlers', () => {
  it('updates duplicate bookmark URLs while preserving the existing id', async () => {
    const dataDir = await tempDataDir();
    const commands = createCommandHandlers({ dataDir });

    const first = await commands.invoke('save_bookmark', {
      title: 'Original title',
      url: 'https://example.com'
    });
    const updated = await commands.invoke('save_bookmark', {
      title: 'Updated title',
      url: 'https://example.com'
    });

    assert.equal(updated.id, first.id);
    assert.equal(updated.title, 'Updated title');
    assert.equal(updated.url, 'https://example.com');

    const bookmarks = await commands.invoke('get_bookmarks');
    assert.equal(bookmarks.length, 1);
    assert.deepEqual(bookmarks[0], updated);
  });

  it('searches history by title and URL case-insensitively', async () => {
    const dataDir = await tempDataDir();
    const commands = createCommandHandlers({ dataDir, store: createStore({ dataDir }) });

    await commands.invoke('add_history_entry', {
      title: 'Svelte Docs',
      url: 'https://svelte.dev/docs'
    });
    await commands.invoke('add_history_entry', {
      title: 'Example Domain',
      url: 'https://example.com'
    });

    const titleMatches = await commands.invoke('search_history', { query: 'svelte' });
    assert.deepEqual(
      titleMatches.map((entry) => entry.url),
      ['https://svelte.dev/docs']
    );

    const urlMatches = await commands.invoke('search_history', { query: 'EXAMPLE' });
    assert.deepEqual(
      urlMatches.map((entry) => entry.title),
      ['Example Domain']
    );
  });

  it('preserves every concurrent history write', async () => {
    const dataDir = await tempDataDir();
    const commands = createCommandHandlers({ dataDir });
    const visits = Array.from({ length: 24 }, (_, index) => ({
      title: `Visit ${index}`,
      url: `https://example.com/${index}`
    }));

    await Promise.all(
      visits.map((visit) =>
        commands.invoke('add_history_entry', {
          title: visit.title,
          url: visit.url
        })
      )
    );

    const history = await commands.invoke('get_history');
    assert.equal(history.length, visits.length);
    assert.deepEqual(
      new Set(history.map((entry) => entry.url)),
      new Set(visits.map((visit) => visit.url))
    );
  });
});
