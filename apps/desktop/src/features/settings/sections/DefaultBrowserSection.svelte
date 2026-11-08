<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { defaultBrowserApi } from '$features/default-browser/api';
  import { makeDefaultBrowser } from '$features/default-browser/default-browser';

  let defaultStatus = $state<{ registered: boolean; isDefault: boolean } | null>(null);

  async function makeDefault() {
    try {
      await makeDefaultBrowser();
      setTimeout(async () => (defaultStatus = await defaultBrowserApi.defaultBrowserStatus()), 1500);
    } catch {
      /* makeDefaultBrowser already said why */
    }
  }

  onMount(() => {
    void defaultBrowserApi.defaultBrowserStatus().then((d) => (defaultStatus = d)).catch(() => {});
  });
</script>

<!-- Default browser -->
<section id="settings-default">
  <h2>Default browser</h2>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>{defaultStatus?.isDefault ? 'Zenith is your default browser' : 'Make Zenith your default browser'}</b>
        <span>Windows asks you to confirm the choice in Settings › Apps › Default apps. Links from other apps then open as new tabs here.</span>
      </div>
      {#if defaultStatus?.isDefault}
        <span class="ok"><Icon name="check" size={16} /> Default</span>
      {:else}
        <button class="btn primary" onclick={makeDefault}>Make default</button>
      {/if}
    </div>
    {#if defaultStatus?.registered}
      <div class="row">
        <div class="text"><b>Remove Zenith from the Default apps list</b></div>
        <button
          class="btn ghost"
          onclick={async () => {
            await defaultBrowserApi.defaultBrowserUnregister();
            defaultStatus = await defaultBrowserApi.defaultBrowserStatus();
          }}>Unregister</button
        >
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
  .ok {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ok);
    font-weight: 600;
  }
</style>
