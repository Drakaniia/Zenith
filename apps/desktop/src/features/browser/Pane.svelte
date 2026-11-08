<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Favicon from '$shared/ui/Favicon.svelte';
  import NewTab from '$features/startpage/NewTab.svelte';
  import SettingsPage from '$features/settings/SettingsPage.svelte';
  import HistoryPage from '$features/history/HistoryPage.svelte';
  import BookmarksPage from '$features/bookmarks/BookmarksPage.svelte';
  import DownloadsPage from '$features/downloads/DownloadsPage.svelte';
  import ExtensionsPage from '$features/extensions/ExtensionsPage.svelte';
  import NotesPage from '$features/notes/NotesPage.svelte';
  import PasswordsPage from '$features/passwords/PasswordsPage.svelte';
  import { browser } from './browser.svelte';
  import { pane } from '$platform/layout.svelte';
  import { displayHost, internalPage, isNewTab, titleFor } from '$shared/url';
  import type { Tab } from './types';

  let { tab }: { tab: Tab } = $props();
  const page = $derived(internalPage(tab.url));
  const section = $derived(tab.url.split('/')[3] ?? '');
</script>

{#if isNewTab(tab.url) || page === 'newtab'}
  <div class="internal"><NewTab {tab} /></div>
{:else if page}
  <div class="internal">
    {#if page === 'settings'}
      <SettingsPage {section} />
    {:else if page === 'history'}
      <HistoryPage />
    {:else if page === 'bookmarks'}
      <BookmarksPage />
    {:else if page === 'downloads'}
      <DownloadsPage />
    {:else if page === 'extensions'}
      <ExtensionsPage />
    {:else if page === 'notes'}
      <NotesPage />
    {:else if page === 'passwords'}
      <PasswordsPage {section} />
    {/if}
  </div>
{:else if tab.crashed}
  <div class="internal state">
    <div class="state-card">
      <div class="state-icon warn"><Icon name="alert" size={28} /></div>
      <h2>{tab.crashed === 'unresponsive' ? 'This page is not responding' : 'This tab crashed'}</h2>
      <p class="muted">{displayHost(tab.url)} stopped working. Your other tabs are fine.</p>
      <div class="row">
        <button class="btn primary" onclick={() => browser.reviveCrashed(tab.id)}><Icon name="reload" size={15} /> Reload</button>
        {#if tab.crashed === 'unresponsive'}
          <button class="btn" onclick={() => (tab.crashed = '')}>Wait</button>
        {/if}
      </div>
    </div>
  </div>
{:else}
  <!-- The page webview is positioned over this element. It stays transparent while the page is
       live so the page shows through when floating UI raises the chrome above it. -->
  <div class="web" class:waiting={!tab.live} class:creating={tab.live && tab.creating} use:pane={tab.id}>
    {#if tab.live && tab.creating}
      <!-- A blank page while the webview starts (the switch itself is instant). -->
      <div class="top-bar"><span></span></div>
    {:else if !tab.live}
      <div class="state-card waking">
        {#if tab.error}
          <div class="state-icon warn"><Icon name="alert" size={26} /></div>
          <h2>Couldn't open this page</h2>
          <p class="muted ellipsis">{tab.error}</p>
          <button class="btn primary" onclick={() => browser.wake(tab)}><Icon name="reload" size={15} /> Try again</button>
        {:else}
          <div class="big-fav"><Favicon url={tab.url} favicon={tab.favicon} size={28} /></div>
          <p class="wake-title ellipsis">{titleFor(tab.url, tab.title)}</p>
          <div class="bar"><span></span></div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .web {
    flex: 1;
    min-width: 0;
    min-height: 0;
    position: relative;
  }
  .web.waiting {
    background: var(--page-bg);
    display: grid;
    place-items: center;
  }
  .web.creating {
    background: var(--page-bg);
  }
  .top-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    overflow: hidden;
  }
  .top-bar span {
    display: block;
    width: 30%;
    height: 100%;
    background: var(--accent);
    animation: sweep 1.1s ease-in-out infinite;
  }
  @keyframes sweep {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .internal {
    flex: 1;
    min-width: 0;
    min-height: 0;
    background: var(--page-bg);
    overflow: auto;
    user-select: text;
  }
  .state {
    display: grid;
    place-items: center;
  }
  .state-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    max-width: 420px;
    padding: 24px;
    text-align: center;
  }
  .state-card h2 {
    margin: 4px 0 0;
    font: 600 18px/1.3 var(--font-display);
  }
  .state-card p {
    margin: 0;
    max-width: 380px;
  }
  .state-icon {
    width: 56px;
    height: 56px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    background: var(--field);
  }
  .warn {
    color: var(--warn);
  }
  .row {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .waking {
    animation: appear 0.5s var(--ease-out) 0.15s both;
  }
  .big-fav {
    width: 56px;
    height: 56px;
    border-radius: 16px;
    display: grid;
    place-items: center;
    background: var(--field);
  }
  .wake-title {
    color: var(--text-2);
    max-width: 320px;
  }
  .bar {
    width: 120px;
    height: 3px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }
  .bar span {
    display: block;
    width: 40%;
    height: 100%;
    border-radius: 3px;
    background: var(--accent);
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(260%);
    }
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
</style>
