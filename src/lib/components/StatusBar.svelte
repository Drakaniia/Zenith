<script lang="ts">
  import { AlertTriangle, LoaderCircle, Lock, PanelTop } from 'lucide-svelte';
  import { tabsState } from '../state/tabs.svelte';

  type SecurityKind = 'secure' | 'insecure' | 'internal' | 'empty';

  interface SecurityState {
    kind: SecurityKind;
    label: string;
    detail: string;
  }

  let activeTab = $derived(tabsState.activeTab);
  let activeUrl = $derived(activeTab?.url ?? '');
  let security = $derived(securityForUrl(activeUrl));
  let loadingText = $derived(activeTab?.is_loading ? `Loading ${targetLabel(activeUrl)}` : 'Ready');

  function securityForUrl(url: string): SecurityState {
    if (!url) {
      return {
        kind: 'empty',
        label: 'No page',
        detail: 'No active page'
      };
    }

    if (url.startsWith('browser://')) {
      return {
        kind: 'internal',
        label: 'Internal',
        detail: url
      };
    }

    try {
      const parsed = new URL(url);

      if (parsed.protocol === 'https:') {
        return {
          kind: 'secure',
          label: 'Secure',
          detail: parsed.href
        };
      }

      if (parsed.protocol === 'http:') {
        return {
          kind: 'insecure',
          label: 'Not Secure',
          detail: parsed.href
        };
      }
    } catch {
      return {
        kind: 'empty',
        label: 'Unknown',
        detail: url
      };
    }

    return {
      kind: 'empty',
      label: 'Unknown',
      detail: url
    };
  }

  function targetLabel(url: string): string {
    if (!url) {
      return 'page';
    }

    if (url.startsWith('browser://')) {
      return url;
    }

    try {
      return new URL(url).host || 'page';
    } catch {
      return 'page';
    }
  }
</script>

<footer class="status-bar" aria-label="Page status">
  <div class={`security security--${security.kind}`} title={security.detail}>
    {#if security.kind === 'secure'}
      <Lock size={13} strokeWidth={2.3} />
    {:else if security.kind === 'insecure'}
      <AlertTriangle size={13} strokeWidth={2.35} />
    {:else}
      <PanelTop size={13} strokeWidth={2.25} />
    {/if}
    <span>{security.label}</span>
  </div>

  <div class={`load-state ${activeTab?.is_loading ? 'load-state--active' : ''}`} aria-live="polite">
    {#if activeTab?.is_loading}
      <LoaderCircle size={13} strokeWidth={2.35} />
    {/if}
    <span>{loadingText}</span>
  </div>

  <div class="status-url" title={activeUrl || 'No active URL'}>
    {activeUrl || 'No active URL'}
  </div>
</footer>

<style>
  .status-bar {
    display: grid;
    grid-template-columns: minmax(92px, auto) minmax(96px, auto) minmax(0, 1fr);
    gap: 10px;
    align-items: center;
    width: 100%;
    height: var(--chrome-statusbar-height);
    min-height: var(--chrome-statusbar-height);
    padding: 0 10px;
    overflow: hidden;
    color: var(--color-text-secondary);
    background: color-mix(in srgb, var(--color-surface) 86%, var(--color-background));
    border-top: 1px solid var(--color-border);
    user-select: none;
  }

  .security,
  .load-state {
    display: flex;
    align-items: center;
    min-width: 0;
    gap: 5px;
    overflow: hidden;
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0;
    line-height: 1;
    white-space: nowrap;
  }

  .security span,
  .load-state span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .security--secure {
    color: var(--color-success);
  }

  .security--insecure {
    color: var(--color-warning);
  }

  .security--internal,
  .security--empty {
    color: var(--color-text-secondary);
  }

  .load-state--active {
    color: var(--color-accent);
  }

  .load-state--active :global(svg) {
    animation: status-spin 820ms linear infinite;
  }

  .status-url {
    min-width: 0;
    overflow: hidden;
    color: color-mix(in srgb, var(--color-text-secondary) 82%, transparent);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0;
    line-height: 1;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.app-shell[data-theme='light']) .status-bar {
    background: color-mix(in srgb, var(--color-surface) 90%, var(--color-background));
  }

  @media (max-width: 560px) {
    .status-bar {
      grid-template-columns: minmax(76px, auto) minmax(0, 1fr);
      gap: 6px;
      padding-inline: 7px;
    }

    .status-url {
      display: none;
    }
  }

  @keyframes status-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
