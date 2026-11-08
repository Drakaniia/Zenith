<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import Icon from '$shared/ui/Icon.svelte';
  import { extensions } from './extensions.svelte';
  import { overlayLease } from '$platform/layout.svelte';
  import { openInternal } from '../../app/actions';

  // The toolbar's extensions menu (Chrome's puzzle menu): open an extension, pin it next to
  // the button, or go to the extensions page or the Web Store.
  const pos = $derived(extensions.popover);
  // The last anchor stays in use while the menu plays its exit transition: reading the cleared
  // `extensions.popover` there threw, which left an invisible layer over the window that
  // swallowed every click.
  let last = { x: 0, y: 0, below: true };
  const at = $derived.by(() => (pos ? (last = pos) : last));
  const W = 320;
  const left = $derived(Math.max(8, Math.min(at.x, window.innerWidth - W - 8)));

  function close() {
    extensions.close();
  }
</script>

{#if pos}
  <div class="root" use:overlayLease>
    <div class="catcher" role="presentation" transition:fade={{ duration: 100 }} onpointerdown={close}></div>
    <div
      class="pop glass"
      style:left="{left}px"
      style:top={at.below ? `${at.y}px` : null}
      style:bottom={at.below ? null : `${window.innerHeight - at.y}px`}
      transition:fly={{ y: at.below ? -8 : 8, duration: 150 }}
      role="dialog"
      aria-label="Extensions"
    >
      <h3>Extensions</h3>
      {#if extensions.user.length}
        <div class="list">
          {#each extensions.user as ext (ext.id)}
            {@const pinned = extensions.isPinned(ext.id)}
            <div class="row" class:off={!ext.enabled}>
              <button class="open" disabled={!ext.enabled} title={ext.popupUrl ? 'Open' : ext.optionsUrl ? 'Options' : ''} onclick={() => extensions.open(ext, at)}>
                <span class="ico">{#if ext.icon}<img src={ext.icon} alt="" width="18" height="18" />{:else}<Icon name="puzzle" size={15} />{/if}</span>
                <span class="name ellipsis">{ext.name}</span>
                {#if !ext.enabled}<span class="tag">Off</span>{/if}
              </button>
              <button
                class="icon-btn pin"
                class:on={pinned}
                title={pinned ? 'Unpin from toolbar' : 'Pin to toolbar'}
                aria-pressed={pinned}
                onclick={() => extensions.togglePin(ext.id)}
              >
                <Icon name={pinned ? 'unpin' : 'pin'} size={14} />
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty">
          <p>No extensions installed yet.</p>
        </div>
      {/if}
      <div class="foot">
        <button class="btn ghost" onclick={() => extensions.openStore()}><Icon name="globe" size={14} />Extension catalogue</button>
        <button
          class="btn ghost"
          onclick={() => {
            close();
            openInternal('extensions');
          }}><Icon name="settings" size={14} />Manage</button
        >
      </div>
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
  .pop {
    position: absolute;
    width: 320px;
    max-height: min(480px, calc(100vh - 80px));
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-xl);
    padding: 10px;
  }
  h3 {
    margin: 2px 8px 8px;
    font: 600 14px/1.2 var(--font-display);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
    min-height: 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: var(--radius);
  }
  .row:hover {
    background: var(--element-bg);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 8px;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .open:disabled {
    cursor: default;
  }
  .row.off .ico,
  .row.off .name {
    opacity: 0.5;
  }
  .ico {
    width: 20px;
    height: 20px;
    flex: none;
    display: grid;
    place-items: center;
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .tag {
    font-size: 11px;
    color: var(--text-3);
  }
  .pin {
    flex: none;
    opacity: 0.55;
  }
  .row:hover .pin,
  .pin.on {
    opacity: 1;
  }
  .pin.on {
    color: var(--accent);
  }
  .empty {
    padding: 14px 8px 18px;
    color: var(--text-3);
    text-align: center;
  }
  .empty p {
    margin: 0;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
  }
  .foot .btn {
    gap: 6px;
  }
</style>
