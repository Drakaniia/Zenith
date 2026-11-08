<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import Segmented from '$shared/ui/Segmented.svelte';
  import { settings, type Settings } from '$features/settings/settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { openInternal } from '$features/browser/target';
  import { shortcutHint } from '$features/shortcuts/hints';
  import { bookmarks } from './bookmarks.svelte';
  import { backupBookmarksFile, exportBookmarksFile, importBookmarksFile, restoreBookmarks } from './bookmark-actions';

  const s = $derived(settings.value);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }
</script>

<!-- Bookmarks -->
<section id="settings-bookmarks">
  <h2>Bookmarks</h2>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Bookmarks toolbar</b>
        <span>{shortcutHint('toggleBookmarksBar') ? `${shortcutHint('toggleBookmarksBar')} shows or hides it. ` : ''}With the single toolbar layout, a toolbar that isn't showing still appears while the pointer is at the top edge of the window.</span>
      </div>
      <Segmented
        value={s.bookmarksBar}
        options={[
          { value: 'always', label: 'Always' },
          { value: 'newtab', label: 'On a new tab' },
          { value: 'never', label: 'Never' },
        ]}
        onchange={(v) => set('bookmarksBar', v)}
      />
    </div>
    <div class="row">
      <div class="text"><b>Show Other Bookmarks</b><span>At the end of the toolbar, when something is in it.</span></div>
      <Toggle checked={s.bookmarksOther} onchange={(v) => set('bookmarksOther', v)} label="Show Other Bookmarks" />
    </div>
    <div class="row">
      <div class="text">
        <b>Bookmarks sidebar</b>
        <span>{shortcutHint('toggleBookmarksSidebar') ? `${shortcutHint('toggleBookmarksSidebar')}. ` : ''}All of your bookmarks, with search, beside the page.</span>
      </div>
      <button class="btn" onclick={() => set('bookmarksPanel', !s.bookmarksPanel)}>{s.bookmarksPanel ? 'Close' : 'Open'}</button>
    </div>
  </div>
  <h3>Manage</h3>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Bookmarks library</b>
        <span>{bookmarks.count} {bookmarks.count === 1 ? 'bookmark' : 'bookmarks'}. Organize folders, tags and keywords.</span>
      </div>
      <button class="btn" onclick={() => openInternal('bookmarks')}><Icon name="library" size={15} />Open library</button>
    </div>
    <div class="row">
      <div class="text"><b>Import</b><span>From another browser on this PC, or from an exported bookmarks file (.html).</span></div>
      <div class="inline">
        <button class="btn" onclick={() => (ui.bookmarkImport = true)}><Icon name="importBox" size={15} />From a browser…</button>
        <button class="btn" onclick={() => void importBookmarksFile()}>From a file…</button>
      </div>
    </div>
    <div class="row">
      <div class="text">
        <b>Export and back up</b>
        <span>An HTML file other browsers can read, or a backup of everything: folders, tags, keywords and workspaces. Zenith also keeps a backup for each day you use it.</span>
      </div>
      <div class="inline">
        <button class="btn" onclick={() => void exportBookmarksFile()}>Export…</button>
        <button class="btn" onclick={() => void backupBookmarksFile()}>Back up…</button>
        <button class="btn" onclick={() => void restoreBookmarks()}>Restore…</button>
      </div>
    </div>
  </div>
</section>

<style>
  section {
    padding-top: 8px;
    margin-bottom: 28px;
  }
  h2 {
    margin: 0 0 12px;
    font: 600 18px/1.3 var(--font-display);
  }
  h3 {
    margin: 22px 0 10px;
    font: 600 14px/1.3 var(--font-display);
    color: var(--text-2);
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    padding: 4px 16px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 58px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: 0;
  }
  /* Controls keep their natural width; the description column gives way instead. */
  .row > :not(.text) {
    flex: none;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text span {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
  .inline {
    display: flex;
    gap: 6px;
  }
</style>
