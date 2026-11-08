<script lang="ts">
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type Settings } from '../settings.svelte';
  import { SEARCH_ENGINES } from '$shared/url';
  import { incognito } from '$features/window/window.svelte';
  import { onboarding } from '$features/onboarding/onboarding.svelte';
  import { normalizeHomepage } from '$features/onboarding/onboarding-flow';

  const s = $derived(settings.value);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  let homepageText = $state(settings.value.homepage);
  let homepageError = $state('');
  function saveHomepage() {
    const r = normalizeHomepage(homepageText);
    if (!r.ok) {
      homepageError = r.error;
      return;
    }
    homepageError = '';
    homepageText = r.url;
    set('homepage', r.url);
  }
</script>

<!-- General -->
<section id="settings-general">
  <h2>General</h2>
  <div class="group">
    <div class="row">
      <div class="text"><b>Restore tabs from last time</b><span>Pinned tabs and Essentials are always kept.</span></div>
      <Toggle checked={s.restoreSession} onchange={(v) => set('restoreSession', v)} label="Restore tabs" />
    </div>
    <div class="row">
      <div class="text"><b>Open new tabs at the top</b><span>Otherwise new tabs go to the bottom of the list.</span></div>
      <Toggle checked={s.newTabsAtTop} onchange={(v) => set('newTabsAtTop', v)} label="New tabs at top" />
    </div>
    <div class="row">
      <div class="text">
        <b>Home page</b>
        <span class:err={!!homepageError}>{homepageError || 'Opens in a window with no tabs, and when you press Enter in an empty new tab. Leave empty for the Zenith start page.'}</span>
      </div>
      <input
        class="field homepage"
        type="url"
        spellcheck="false"
        placeholder="Zenith start page"
        aria-label="Home page"
        aria-invalid={!!homepageError}
        bind:value={homepageText}
        onchange={saveHomepage}
        onkeydown={(e) => e.key === 'Enter' && saveHomepage()}
      />
    </div>
    <div class="row">
      <div class="text"><b>Search engine</b><span>Used for searches from the address bar.</span></div>
      <select class="field select" aria-label="Search engine" value={s.searchEngine} onchange={(e) => set('searchEngine', (e.currentTarget as HTMLSelectElement).value)}>
        {#each Object.entries(SEARCH_ENGINES) as [id, e] (id)}<option value={id}>{e.name}</option>{/each}
      </select>
    </div>
    <div class="row">
      <div class="text">
        <b>Search suggestions</b>
        <span>Sends what you type in the address bar to {SEARCH_ENGINES[s.searchEngine]?.name}. Off by default.</span>
      </div>
      <Toggle checked={s.searchSuggestions} onchange={(v) => set('searchSuggestions', v)} label="Search suggestions" />
    </div>
    <div class="row">
      <div class="text"><b>Show most visited sites on new tabs</b><span>Taken from your local history only.</span></div>
      <Toggle checked={s.showTopSites} onchange={(v) => set('showTopSites', v)} label="Top sites" />
    </div>
    {#if !incognito}
      <div class="row">
        <div class="text">
          <b>Setup assistant</b>
          <span>{s.onboardedAt ? `Completed ${new Date(s.onboardedAt).toLocaleDateString()}. ` : ''}Walk through theme, search, home page, import and privacy again.</span>
        </div>
        <button class="btn" onclick={() => onboarding.restart()}>Run setup again</button>
      </div>
    {/if}
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
  .err {
    color: var(--danger);
  }
  .homepage {
    width: min(280px, 45%);
    flex: none;
  }
  .select {
    height: 32px;
    padding: 0 8px;
  }
</style>
