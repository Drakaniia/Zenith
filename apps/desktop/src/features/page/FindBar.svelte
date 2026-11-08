<script lang="ts">
  import { slide } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { api } from '$platform/ipc';
  import { ui } from '$shared/ui/ui.svelte';
  import { focusAfterLayout } from '$platform/layout.svelte';

  let timer: ReturnType<typeof setTimeout> | undefined;

  function search() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      const f = ui.find;
      if (!f.query) {
        f.matches = 0;
        f.active = -1;
        void api.find(f.tabId, 'stop');
        return;
      }
      void api.find(f.tabId, 'start', f.query, f.caseSensitive);
    }, 90);
  }

  function step(dir: 'next' | 'prev') {
    if (!ui.find.query) return;
    void api.find(ui.find.tabId, dir);
  }

  function close() {
    const id = ui.find.tabId;
    void api.find(id, 'stop');
    ui.find.open = false;
    ui.find.matches = 0;
    ui.find.active = -1;
    focusAfterLayout(id);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      step(e.shiftKey ? 'prev' : 'next');
    } else if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }
</script>

<div class="find" transition:slide={{ duration: 140 }}>
  <Icon name="search" size={15} />
  <!-- svelte-ignore a11y_autofocus -->
  <input
    id="find-input"
    class="input"
    placeholder="Find in page"
    spellcheck="false"
    autocomplete="off"
    autofocus
    bind:value={ui.find.query}
    oninput={search}
    {onkeydown}
  />
  <span class="count" class:none={ui.find.query && ui.find.matches === 0}>
    {#if ui.find.query}
      <!-- WebView2 reports the active match 1-based. -->
      {ui.find.matches
        ? ui.find.active > 0
          ? `${ui.find.active} of ${ui.find.matches}`
          : `${ui.find.matches} matches`
        : 'No matches'}
    {/if}
  </span>
  <button
    class="icon-btn sm case"
    class:on={ui.find.caseSensitive}
    title="Match case"
    onclick={() => {
      ui.find.caseSensitive = !ui.find.caseSensitive;
      search();
    }}>Aa</button
  >
  <button class="icon-btn sm" title="Previous (Shift+Enter)" onclick={() => step('prev')}><Icon name="chevronUp" size={15} /></button>
  <button class="icon-btn sm" title="Next (Enter)" onclick={() => step('next')}><Icon name="chevronDown" size={15} /></button>
  <button class="icon-btn sm" title="Close (Esc)" onclick={close}><Icon name="close" size={15} /></button>
</div>

<style>
  .find {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 40px;
    padding: 0 8px 0 12px;
    border-radius: var(--radius-lg);
    background: var(--surface);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    color: var(--text-2);
  }
  .input {
    flex: 1;
    min-width: 80px;
    height: 28px;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
  }
  .count {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .count.none {
    color: var(--danger);
  }
  .case {
    font-size: 12px;
    font-weight: 600;
    width: 28px;
  }
  .case.on {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
</style>
