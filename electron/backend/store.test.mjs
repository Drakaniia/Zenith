import assert from 'node:assert/strict';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, it } from 'node:test';

import { createStore } from './store.mjs';

const tempDirs = [];

async function tempDataDir() {
  const dir = await mkdtemp(join(tmpdir(), 'ampui-electron-store-'));
  tempDirs.push(dir);
  return dir;
}

afterEach(async () => {
  await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })));
});

describe('createStore settings persistence', () => {
  it('returns default settings when settings.json is missing', async () => {
    const dataDir = await tempDataDir();
    const store = createStore({ dataDir });

    assert.deepEqual(await store.getSettings(), {
      default_search_engine: 'yandex',
      homepage: 'browser://home',
      theme: 'dark',
      javascript_enabled: true,
      show_status_bar: true
    });
  });

  it('falls back to default settings when settings.json is invalid', async () => {
    const dataDir = await tempDataDir();
    await writeFile(join(dataDir, 'settings.json'), '{ invalid json', 'utf8');

    const store = createStore({ dataDir });

    assert.deepEqual(await store.getSettings(), {
      default_search_engine: 'yandex',
      homepage: 'browser://home',
      theme: 'dark',
      javascript_enabled: true,
      show_status_bar: true
    });
  });
});
