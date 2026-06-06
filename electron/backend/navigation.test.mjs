import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { normalizeUrlWithSettings } from './navigation.mjs';

const yandexSettings = {
  default_search_engine: 'yandex',
  homepage: 'browser://home',
  theme: 'dark',
  javascript_enabled: true,
  show_status_bar: true
};

function settingsWithSearchEngine(defaultSearchEngine) {
  return {
    ...yandexSettings,
    default_search_engine: defaultSearchEngine
  };
}

describe('normalizeUrlWithSettings', () => {
  it('normalizes a bare domain to https', () => {
    assert.equal(normalizeUrlWithSettings('example.com', yandexSettings), 'https://example.com');
  });

  it('leaves an absolute https URL unchanged', () => {
    assert.equal(
      normalizeUrlWithSettings('https://example.com/path', yandexSettings),
      'https://example.com/path'
    );
  });

  it('normalizes localhost with a port to http', () => {
    assert.equal(
      normalizeUrlWithSettings('localhost:3000', yandexSettings),
      'http://localhost:3000'
    );
  });

  it('uses Yandex search by default for search input', () => {
    assert.equal(
      normalizeUrlWithSettings('rust tauri docs', yandexSettings),
      'https://yandex.com/search/?text=rust%20tauri%20docs'
    );
  });

  it('uses Google search when settings select Google', () => {
    assert.equal(
      normalizeUrlWithSettings('rust tauri docs', settingsWithSearchEngine('google')),
      'https://www.google.com/search?q=rust%20tauri%20docs'
    );
  });

  it('rejects empty input', () => {
    assert.throws(
      () => normalizeUrlWithSettings('   ', yandexSettings),
      /URL or search query cannot be empty|empty/i
    );
  });
});
