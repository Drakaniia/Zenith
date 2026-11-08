<script lang="ts">
  import { slide } from 'svelte/transition';
  import Icon from '../Icon.svelte';
  import Favicon from '../Favicon.svelte';
  import { browser, tabTitle } from '../../lib/browser.svelte';
  import { displayHost } from '../../lib/url';

  // Zen's sidebar media controls (src/zen/media): shown while a tab that is not on screen plays
  // audio. WebView2 gives Zenith mute control and the audible state, not play/pause, so the bar
  // offers what the backend can do: jump to the tab, mute or unmute it, or dismiss the bar.
  let dismissed = $state<string | null>(null);

  const playing = $derived(
    Object.values(browser.tabs)
      .filter((t) => (t.audible || (t.muted && browser.recentlyAudible(t.id))) && !browser.isVisible(t.id))
      .sort((a, b) => b.lastActive - a.lastActive)[0],
  );
  const show = $derived(playing && playing.id !== dismissed ? playing : undefined);

  $effect(() => {
    if (dismissed && !browser.tabs[dismissed]?.audible) dismissed = null;
  });
</script>

{#if show}
  <div class="media" transition:slide={{ duration: 150 }}>
    <button class="info" title="Go to tab" onclick={() => browser.activate(show.id)}>
      <span class="fav"><Favicon url={show.url} favicon={show.favicon} size={16} /></span>
      <span class="text">
        <span class="title ellipsis">{tabTitle(show)}</span>
        <span class="host ellipsis">{displayHost(show.url)}</span>
      </span>
      {#if show.audible && !show.muted}
        <span class="bars" aria-hidden="true"><i></i><i></i><i></i></span>
      {/if}
    </button>
    <div class="controls">
      <button class="icon-btn sm" title={show.muted ? 'Unmute' : 'Mute'} onclick={() => browser.setMuted(show.id, !show.muted)}>
        <Icon name={show.muted ? 'volumeOff' : 'volume'} size={15} />
      </button>
      <button class="icon-btn sm" title="Hide" onclick={() => (dismissed = show.id)}>
        <Icon name="close" size={14} />
      </button>
    </div>
  </div>
{/if}

<style>
  .media {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    margin: 4px calc(var(--zen-pad) + 2px) 0;
    padding: 6px 6px 6px 4px;
    border-radius: var(--r-tab);
    corner-shape: var(--sq);
    background: var(--element-bg);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
    text-align: left;
    border-radius: 8px;
  }
  .fav {
    display: grid;
    flex: none;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .title {
    font-size: 12.5px;
    font-weight: 600;
  }
  .host {
    font-size: 11px;
    opacity: 0.6;
  }
  .controls {
    display: flex;
    gap: 2px;
  }
  .bars {
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
    flex: none;
  }
  .bars i {
    width: 2.5px;
    border-radius: 2px;
    background: var(--accent-fg);
    animation: bar 0.9s ease-in-out infinite;
  }
  .bars i:nth-child(2) {
    animation-delay: -0.3s;
  }
  .bars i:nth-child(3) {
    animation-delay: -0.6s;
  }
  @keyframes bar {
    0%,
    100% {
      height: 3px;
    }
    50% {
      height: 12px;
    }
  }
</style>
