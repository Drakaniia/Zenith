<!-- The sidebar's downloads button. While files download it shows their progress; for a few
     minutes after one finishes it shows that file's type, and the file can be dragged from it
     onto a page (an upload area, a chat) like a file from Explorer. A click opens Downloads. -->
<script lang="ts">
  import { scale } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { withHint } from '$features/shortcuts/hints';
  import { downloads } from './downloads.svelte';
  import { downloadsApi } from './api';
  import { fileType } from './file-type';
  import { fileNameOf, latestFinished } from './recent';

  let now = $state(Date.now());
  const latest = $derived(downloads.active.length ? null : latestFinished(downloads.items, now));
  const recent = $derived(latest?.item ?? null);
  const type = $derived(recent ? fileType(fileNameOf(recent), recent.mime) : null);
  const title = $derived(
    recent
      ? `${fileNameOf(recent)}\nDrag onto a page to attach it · Click for Downloads`
      : withHint('Downloads', 'downloads'),
  );

  // The file goes back to the download icon when its few minutes are up.
  $effect(() => {
    const until = latest?.until;
    if (!until) return;
    const t = setTimeout(() => (now = Date.now()), Math.max(0, until - Date.now()) + 50);
    return () => clearTimeout(t);
  });

  let dragging = $state(false);
  let swallowClick = false;

  /** A press that moves a few pixels starts a real file drag (the shell's, run by the backend
   *  while the button is still held), so pages and apps get the file itself. */
  function onPointerDown(e: PointerEvent) {
    const file = recent;
    if (e.button !== 0 || !file || dragging) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const stop = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', stop);
      window.removeEventListener('pointercancel', stop);
    };
    const move = (ev: PointerEvent) => {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      stop();
      dragging = true;
      swallowClick = true;
      setTimeout(() => (swallowClick = false), 400);
      downloadsApi
        .downloadDrag(file.id)
        .catch((err) => ui.toast(String(err), { icon: 'alert', timeout: 6000 }))
        .finally(() => (dragging = false));
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', stop);
    window.addEventListener('pointercancel', stop);
  }

  function onClick() {
    if (swallowClick) return;
    ui.downloadsOpen = !ui.downloadsOpen;
  }
</script>

<button
  class="icon-btn dl"
  class:file={!!type}
  class:dragging
  {title}
  aria-label={recent ? `Downloads, latest: ${fileNameOf(recent)}` : 'Downloads'}
  onclick={onClick}
  onpointerdown={onPointerDown}
  ondragstart={(e) => e.preventDefault()}
  data-downloads-anchor
>
  <span class="motion-icon" data-sidebar-icon="downloads">
    {#if type}
      {#key recent?.id}
        <span class="glyph" style:color={type.color} in:scale={{ start: 0.6, duration: 220 }}>
          <Icon name={type.icon} />
          {#if type.label}
            <span class="ext" style:background={type.color ?? 'var(--accent-button)'}>{type.label}</span>
          {/if}
        </span>
      {/key}
    {:else}
      <Icon name="download" />
    {/if}
  </span>
  {#if downloads.progress >= 0}
    <svg class="ring" viewBox="0 0 36 36" aria-hidden="true">
      <circle cx="18" cy="18" r="15" pathLength="100" stroke-dasharray="{Math.max(3, downloads.progress * 100)} 100" />
    </svg>
  {:else if downloads.active.length}
    <span class="dot"></span>
  {/if}
</button>

<style>
  .motion-icon {
    display: grid;
  }
  .dl {
    position: relative;
  }
  .dl.file {
    cursor: grab;
  }
  .dl.dragging {
    cursor: grabbing;
    opacity: 0.6;
  }
  .glyph {
    position: relative;
    display: grid;
  }
  /* The extension on a small tag over the sheet's lower edge. */
  .ext {
    position: absolute;
    left: 50%;
    bottom: -4px;
    transform: translateX(-50%);
    padding: 0 2px;
    border-radius: 3px;
    color: #fff;
    font: 700 7px/9px var(--font);
    letter-spacing: 0.02em;
    white-space: nowrap;
    pointer-events: none;
  }
  .ring {
    position: absolute;
    inset: 1px;
    transform: rotate(-90deg);
    pointer-events: none;
  }
  .ring circle {
    fill: none;
    stroke: var(--accent-button);
    stroke-width: 2.4;
    stroke-linecap: round;
    transition: stroke-dasharray 0.3s;
  }
  .dot {
    position: absolute;
    right: 4px;
    top: 4px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent-button);
  }
</style>
