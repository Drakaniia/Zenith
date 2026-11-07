<script lang="ts">
  import { scale } from 'svelte/transition';
  import { tick, untrack } from 'svelte';
  import Favicon from './Favicon.svelte';
  import FolderPicker from './bookmarks/FolderPicker.svelte';
  import SpacePicker from './bookmarks/SpacePicker.svelte';
  import { bookmarks } from '../lib/bookmarks.svelte';
  import { browser } from '../lib/browser.svelte';
  import { ui, type AnchorRect } from '../lib/ui.svelte';
  import { api } from '../lib/ipc';
  import { focusAfterLayout, overlayLease } from '../lib/layout.svelte';
  import { deleteNodes } from '../lib/bookmark-actions';
  import { faviconFor } from '../lib/favicons';
  import type { BookmarkPatch } from '../lib/types';

  // Zen's "Edit bookmark" panel under the address bar's bookmark button: name, location and
  // workspaces, "Remove bookmark" and "Save". The star and Ctrl+D have already bookmarked the page
  // (in the folder used last) when it opens, so leaving it any way keeps the bookmark; Escape
  // only drops the edits made in the panel.
  const panel = $derived(ui.bookmarkPopup);
  const node = $derived(panel ? bookmarks.node(panel.id) : undefined);

  let name = $state('');
  let parent = $state(1);
  let spaces = $state<string[]>([]);
  let foldersOpen = $state(false);
  let input: HTMLInputElement | undefined = $state();
  let busy = false;

  // A fresh draft each time the panel opens.
  $effect(() => {
    const p = panel;
    if (!p) return;
    untrack(() => {
      const n = bookmarks.node(p.id);
      if (!n) return;
      name = n.title;
      parent = n.parent ?? 1;
      spaces = [...n.spaces];
      foldersOpen = false;
      busy = false;
      // The keyboard moves from the page to the panel.
      void api.focusView(null);
      void tick().then(() => {
        input?.focus();
        input?.select();
      });
    });
  });

  // The bookmark went away (deleted in another window): nothing is left to edit.
  $effect(() => {
    if (panel && bookmarks.loaded && !node) ui.bookmarkPopup = null;
  });

  function close(restoreFocus = true) {
    ui.bookmarkPopup = null;
    if (restoreFocus) focusAfterLayout(browser.activeTab?.live ? browser.activeTab.id : null);
  }

  const sameSet = (a: string[], b: string[]) => a.length === b.length && a.every((x) => b.includes(x));

  /** Applies what was edited. */
  async function commit() {
    const id = panel?.id;
    const n = node;
    if (id === undefined || !n || busy) return;
    busy = true;
    const patch: BookmarkPatch = {};
    if (name.trim() !== n.title) patch.title = name.trim();
    if (!sameSet(spaces, n.spaces)) patch.spaces = spaces;
    try {
      if (patch.title !== undefined || patch.spaces !== undefined) await bookmarks.update(id, patch);
      if (parent !== n.parent) await bookmarks.move(id, parent, bookmarks.kids(parent).length);
      bookmarks.rememberFolder(parent);
    } catch (e) {
      ui.toast(`Couldn't save the bookmark: ${e}`, { icon: 'alert' });
    }
  }

  async function save() {
    await commit();
    close();
  }

  async function remove() {
    const id = panel?.id;
    close();
    if (id !== undefined) await deleteNodes([id]);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  /** Hangs the panel under its anchor (right edges aligned, or left edges when the anchor is near
   *  the left side of the window), kept inside the window while it grows with the folder list. */
  function place(el: HTMLElement, anchor: AnchorRect) {
    let a = anchor;
    const apply = () => {
      const w = el.offsetWidth;
      const h = el.offsetHeight;
      let left = a.right - w;
      if (left < 8) left = Math.max(8, a.left);
      left = Math.min(left, window.innerWidth - w - 8);
      let top = a.bottom + 8;
      if (top + h > window.innerHeight - 8) top = Math.max(8, window.innerHeight - h - 8);
      el.style.left = `${Math.max(8, left)}px`;
      el.style.top = `${top}px`;
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return {
      update(next: AnchorRect) {
        a = next;
        apply();
      },
      destroy: () => ro.disconnect(),
    };
  }
</script>

{#if panel && node}
  <div class="root" role="presentation" use:overlayLease>
    <div class="catcher" role="presentation" onpointerdown={() => void save()}></div>
    <div
      class="popup glass"
      role="dialog"
      aria-label="Edit bookmark"
      tabindex="-1"
      use:place={panel.anchor}
      {onkeydown}
      transition:scale={{ start: 0.97, duration: 120, opacity: 0 }}
    >
      <div class="thumb" aria-hidden="true">
        <Favicon url={node.url} favicon={faviconFor(node.url)} size={40} />
      </div>
      <form
        class="form"
        onsubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <h2>Edit bookmark</h2>
        <label class="row">
          <span class="label">Name</span>
          <input class="field" bind:this={input} bind:value={name} spellcheck="false" autocomplete="off" />
        </label>
        <div class="row top">
          <span class="label">Location</span>
          <FolderPicker bind:value={parent} bind:expanded={foldersOpen} />
        </div>
        <div class="row top">
          <span class="label">Workspaces</span>
          <SpacePicker bind:value={spaces} />
        </div>
        <div class="actions">
          <button type="button" class="btn ghost" onclick={() => void remove()}>Remove bookmark</button>
          <button type="submit" class="btn primary">Save</button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 85;
  }
  .catcher {
    position: absolute;
    inset: 0;
  }
  .popup {
    position: absolute;
    width: min(470px, calc(100vw - 16px));
    display: grid;
    grid-template-columns: 100px minmax(0, 1fr);
    gap: 16px;
    padding: 16px;
    border-radius: 14px;
    corner-shape: var(--sq);
    outline: none;
    transform-origin: top right;
  }
  .thumb {
    width: 100px;
    height: 100px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    corner-shape: var(--sq);
    background: color-mix(in srgb, var(--accent) 12%, var(--field));
    border: 1px solid var(--line);
  }
  .thumb :global(.fav) {
    border-radius: 9px;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  h2 {
    margin: 0 0 2px;
    font: 600 13px/1.3 var(--font);
  }
  .row {
    display: grid;
    grid-template-columns: 78px minmax(0, 1fr);
    align-items: center;
    gap: 8px;
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
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
</style>
