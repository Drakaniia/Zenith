<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { tick, untrack } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import FolderPicker from './FolderPicker.svelte';
  import SpacePicker from './SpacePicker.svelte';
  import { bookmarks } from './bookmarks.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { webviewApi } from '$platform/webview-api';
  import { focusAfterLayout, overlayLease } from '$platform/layout.svelte';
  import { faviconFor } from '$shared/favicons';
  import { bookmarkTitle, formatTags, parseTags, tabsFolderName } from './bookmarks';
  import { isWebUrl } from '$shared/url';

  // The detailed bookmark dialogs (Zen: "Bookmark Tab…" with tags and keyword, "Bookmark Tabs…"
  // into a new folder, and new or renamed folders).
  const spec = $derived(ui.bookmarkDialog);

  let title = $state('');
  let url = $state('');
  let parent = $state(1);
  let tags = $state('');
  let keyword = $state('');
  let spaces = $state<string[]>([]);
  let foldersOpen = $state(true);
  let error = $state('');
  let busy = $state(false);
  let first: HTMLInputElement | undefined = $state();

  $effect(() => {
    const s = spec;
    if (!s) return;
    untrack(() => {
      error = '';
      busy = false;
      foldersOpen = true;
      const n = s.kind !== 'tabs' && s.id !== undefined ? bookmarks.node(s.id) : undefined;
      if (s.kind === 'bookmark') {
        title = n ? n.title : s.title;
        url = n ? n.url : s.url;
        parent = n?.parent ?? s.parent ?? bookmarks.defaultParent();
        tags = formatTags(n?.tags ?? []);
        keyword = n?.keyword ?? '';
        spaces = [...(n?.spaces ?? [])];
      } else if (s.kind === 'folder') {
        title = n?.title ?? '';
        url = '';
        parent = n?.parent ?? s.parent ?? bookmarks.defaultParent();
        spaces = [...(n?.spaces ?? [])];
      } else {
        title = tabsFolderName();
        url = '';
        parent = bookmarks.defaultParent();
        spaces = [];
      }
      void webviewApi.focusView(null);
      void tick().then(() => {
        first?.focus();
        first?.select();
      });
    });
  });

  const heading = $derived(
    !spec
      ? ''
      : spec.kind === 'tabs'
        ? `Bookmark ${spec.tabs.length} ${spec.tabs.length === 1 ? 'tab' : 'tabs'}`
        : spec.kind === 'folder'
          ? spec.id === undefined
            ? 'New folder'
            : 'Edit folder'
          : spec.id === undefined
            ? 'Add bookmark'
            : 'Edit bookmark',
  );

  const typed = $derived(parseTags(tags));
  const suggestions = $derived(
    bookmarks.tags.filter((t) => !typed.some((x) => x.toLowerCase() === t.tag.toLowerCase())).slice(0, 12),
  );

  function close() {
    ui.bookmarkDialog = null;
    focusAfterLayout(browser.activeTab?.live ? browser.activeTab.id : null);
  }

  async function moveIfNeeded(id: number, from: number | null) {
    if (parent !== from) await bookmarks.move(id, parent, bookmarks.kids(parent).length);
  }

  async function saveBookmark(s: Extract<NonNullable<typeof spec>, { kind: 'bookmark' }>) {
    const address = url.trim();
    if (!isWebUrl(address)) throw new Error('Bookmarks need an http(s) address.');
    if (/\s/.test(keyword.trim())) throw new Error("A keyword can't contain spaces.");
    const word = keyword.trim().toLowerCase();
    if (s.id !== undefined) {
      const n = bookmarks.node(s.id);
      if (!n) throw new Error('That bookmark no longer exists.');
      await bookmarks.update(s.id, { title: title.trim(), url: address, tags: typed, keyword: word, spaces });
      await moveIfNeeded(s.id, n.parent);
    } else {
      await bookmarks.addBookmark(address, title, { parent, tags: typed, keyword: word, spaces });
    }
    bookmarks.rememberFolder(parent);
  }

  async function saveFolder(s: Extract<NonNullable<typeof spec>, { kind: 'folder' }>) {
    const name = title.trim() || 'New folder';
    if (s.id !== undefined) {
      const n = bookmarks.node(s.id);
      if (!n) throw new Error('That folder no longer exists.');
      await bookmarks.update(s.id, { title: name, spaces });
      await moveIfNeeded(s.id, n.parent);
    } else {
      const id = await bookmarks.addFolder(parent, name);
      if (spaces.length) await bookmarks.update(id, { spaces });
    }
  }

  async function saveTabs(s: Extract<NonNullable<typeof spec>, { kind: 'tabs' }>) {
    const name = title.trim() || tabsFolderName();
    await bookmarks.add(parent, [
      {
        kind: 'folder',
        title: name,
        children: s.tabs.map((t) => ({ kind: 'bookmark' as const, url: t.url, title: bookmarkTitle(t.title, t.url) })),
      },
    ]);
    ui.clearTabSelection();
    ui.toast(`Bookmarked ${s.tabs.length} ${s.tabs.length === 1 ? 'tab' : 'tabs'}`, { icon: 'bookmark' });
  }

  async function submit() {
    const s = spec;
    if (!s || busy) return;
    busy = true;
    error = '';
    try {
      if (s.kind === 'bookmark') await saveBookmark(s);
      else if (s.kind === 'folder') await saveFolder(s);
      else await saveTabs(s);
      close();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }
</script>

{#if spec}
  <div class="root" role="presentation" use:overlayLease>
    <div class="scrim" role="presentation" transition:fade={{ duration: 120 }} onpointerdown={close}></div>
    <div
      class="dialog glass"
      role="dialog"
      aria-modal="true"
      aria-label={heading}
      tabindex="-1"
      {onkeydown}
      transition:scale={{ start: 0.97, duration: 140 }}
    >
    <form
      class="inner"
      onsubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <header>
        <h2>{heading}</h2>
        <button type="button" class="icon-btn sm" title="Close" onclick={close}><Icon name="close" size={14} /></button>
      </header>

      <div class="body">
        <label class="row">
          <span class="label">{spec.kind === 'tabs' ? 'Folder name' : 'Name'}</span>
          <input class="field" bind:this={first} bind:value={title} spellcheck="false" autocomplete="off" />
        </label>
        {#if spec.kind === 'bookmark'}
          <label class="row">
            <span class="label">Address</span>
            <input class="field" bind:value={url} spellcheck="false" autocomplete="off" />
          </label>
        {/if}

        <div class="row top">
          <span class="label">Location</span>
          <FolderPicker bind:value={parent} bind:expanded={foldersOpen} exclude={spec.kind === 'folder' ? spec.id : undefined} />
        </div>

        {#if spec.kind === 'bookmark'}
          <div class="row top">
            <span class="label">Tags</span>
            <div class="stack">
              <input class="field" bind:value={tags} placeholder="Separate tags with commas" spellcheck="false" autocomplete="off" />
              <p class="hint">Use tags to organize and search for bookmarks from the address bar.</p>
              {#if suggestions.length}
                <div class="chips">
                  {#each suggestions as t (t.tag)}
                    <button type="button" class="chip" onclick={() => (tags = formatTags([...typed, t.tag]))}>{t.tag}</button>
                  {/each}
                </div>
              {/if}
            </div>
          </div>
          <div class="row top">
            <span class="label">Keyword</span>
            <div class="stack">
              <input class="field" bind:value={keyword} spellcheck="false" autocomplete="off" />
              <p class="hint">Use a single keyword to open bookmarks directly from the address bar. A %s in the address is replaced by the words typed after it.</p>
            </div>
          </div>
        {/if}

        {#if spec.kind !== 'tabs'}
          <div class="row top">
            <span class="label">Workspaces</span>
            <SpacePicker bind:value={spaces} />
          </div>
        {:else}
          <div class="row top">
            <span class="label">Tabs</span>
            <ul class="tabs">
              {#each spec.tabs as t (t.url + t.title)}
                <li>
                  <Favicon url={t.url} favicon={t.favicon ?? faviconFor(t.url)} size={16} />
                  <span class="ellipsis">{bookmarkTitle(t.title, t.url)}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if error}<p class="error" role="alert">{error}</p>{/if}
      </div>

      <footer>
        <button type="button" class="btn" onclick={close}>Cancel</button>
        <button type="submit" class="btn primary" disabled={busy}>Save</button>
      </footer>
    </form>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 88;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--backdrop);
  }
  .dialog {
    position: relative;
    width: min(540px, 100%);
    max-height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: 16px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    outline: none;
  }
  .inner {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 16px 16px 4px 20px;
  }
  h2 {
    margin: 0;
    font: 600 16px/1.3 var(--font-display);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 10px 20px 4px;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
  }
  .row.top {
    align-items: start;
  }
  .row.top .label {
    padding-top: 7px;
  }
  .label {
    font-size: 13px;
    color: var(--text-2);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .hint {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--text-3);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .chip {
    height: 24px;
    padding: 0 10px;
    border-radius: 12px;
    font-size: 12px;
    background: var(--field);
    border: 1px solid var(--line);
    color: var(--text-2);
  }
  .chip:hover {
    color: var(--text);
    background: var(--field-focus);
  }
  .tabs {
    margin: 0;
    padding: 4px;
    list-style: none;
    max-height: 150px;
    overflow-y: auto;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .tabs li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 26px;
    padding: 0 6px;
    font-size: 12.5px;
  }
  .error {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-btn);
    font-size: 12.5px;
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px 18px;
  }
</style>
