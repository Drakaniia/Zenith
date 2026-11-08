<script lang="ts">
  import { fly } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { permissions } from '$shared/ui/ui.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import type { IconName } from '$shared/ui/icons';

  const LABELS: Record<string, { text: string; icon: IconName }> = {
    camera: { text: 'use your camera', icon: 'camera' },
    microphone: { text: 'use your microphone', icon: 'mic' },
    location: { text: 'know your location', icon: 'location' },
    notifications: { text: 'show notifications', icon: 'bell' },
    clipboard: { text: 'see text and images copied to the clipboard', icon: 'clipboard' },
    midi: { text: 'control MIDI devices', icon: 'keyboard' },
    windowManagement: { text: 'manage windows on all your displays', icon: 'expand' },
    localFonts: { text: 'use the fonts installed on this computer', icon: 'file' },
    files: { text: 'edit files on this computer', icon: 'folder' },
    downloads: { text: 'download multiple files', icon: 'download' },
    sensors: { text: 'use motion sensors', icon: 'zap' },
  };

  // Show the oldest request whose tab is on screen.
  const request = $derived(permissions.queue.find((p) => browser.isVisible(p.tabId)));
  let remember = $state(true);
  const host = $derived(request ? request.origin.replace(/^https?:\/\//, '') : '');
  const label = $derived(request ? (LABELS[request.kind] ?? { text: `use ${request.kind}`, icon: 'info' as IconName }) : null);
</script>

{#if request && label}
  <div class="prompt glass" use:overlayLease transition:fly={{ y: -10, duration: 160 }} role="alertdialog" aria-label="Permission request">
    <div class="icon"><Icon name={label.icon} size={20} /></div>
    <div class="body">
      <p><strong>{host}</strong> wants to {label.text}</p>
      <label class="remember"><input type="checkbox" bind:checked={remember} /> Remember this decision</label>
    </div>
    <div class="actions">
      <button class="btn" onclick={() => permissions.answer(request, false, remember)}>Block</button>
      <button class="btn primary" onclick={() => permissions.answer(request, true, remember)}>Allow</button>
    </div>
  </div>
{/if}

<style>
  .prompt {
    position: fixed;
    top: calc(var(--topbar) + 14px);
    left: calc(var(--sidebar-w) + 16px);
    z-index: 75;
    width: 380px;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 12px 14px;
    padding: 16px;
    border-radius: var(--radius-xl);
  }
  /* Sidebar on the right or a tab strip: the page starts near the window's left edge. */
  :global(.app.side-right) .prompt,
  :global(.app.side-top) .prompt,
  :global(.app.side-bottom) .prompt {
    left: 24px;
  }
  :global(.app.side-top) .prompt {
    top: calc(var(--strip-h) + 14px);
  }
  :global(.app.compact) ~ .prompt,
  :global(.app.compact) .prompt {
    left: 24px;
    top: 24px;
  }
  .icon {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .body p {
    margin: 2px 0 8px;
    line-height: 1.45;
  }
  .remember {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
