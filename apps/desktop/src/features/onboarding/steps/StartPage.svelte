<script lang="ts">
  import { untrack } from 'svelte';
  import Choice from '../Choice.svelte';
  import { onboarding } from '../onboarding.svelte';
  import { settings } from '$features/settings/settings.svelte';
  import { normalizeHomepage } from '../onboarding-flow';

  const s = $derived(settings.value);
  let kind = $state<'zenith' | 'site'>(untrack(() => (settings.value.homepage ? 'site' : 'zenith')));
  let text = $state(untrack(() => settings.value.homepage.replace(/^https:\/\//, '').replace(/\/$/, '')));
  let touched = $state(false);
  let input: HTMLInputElement | undefined = $state();

  const result = $derived(normalizeHomepage(text));
  const error = $derived(kind === 'site' && touched && !result.ok ? result.error : '');

  // Continue waits for a valid address while "A website" is chosen.
  $effect(() => {
    if (kind === 'zenith') {
      onboarding.hold = '';
      if (untrack(() => settings.value.homepage)) onboarding.set('homepage', '');
      return;
    }
    if (result.ok && result.url) {
      onboarding.hold = '';
      if (untrack(() => settings.value.homepage) !== result.url) onboarding.set('homepage', result.url);
    } else {
      onboarding.hold = 'Enter a web address for your home page';
    }
  });

  function pickSite() {
    kind = 'site';
    requestAnimationFrame(() => input?.focus());
  }
</script>

<fieldset>
  <legend>When Zenith opens</legend>
  <div class="grid two">
    <Choice
      name="fre-restore"
      value={true}
      selected={s.restoreSession}
      title="Reopen my tabs"
      desc="Pick up where you left off last time."
      onselect={() => onboarding.set('restoreSession', true)}
    />
    <Choice
      name="fre-restore"
      value={false}
      selected={s.restoreSession}
      title="Start fresh"
      desc="Only pinned tabs and Essentials come back."
      onselect={() => onboarding.set('restoreSession', false)}
    />
  </div>
</fieldset>

<fieldset>
  <legend>Home page <span class="hint">Opens in a window with no tabs, and when you press Enter in an empty new tab.</span></legend>
  <div class="grid two">
    <Choice
      name="fre-home"
      value="zenith"
      selected={kind}
      title="Zenith start page"
      desc="A search box and your most visited sites."
      onselect={() => (kind = 'zenith')}
    />
    <Choice name="fre-home" value="site" selected={kind} title="A website" desc="Any page you like to start from." onselect={pickSite} />
  </div>
  {#if kind === 'site'}
    <div class="address">
      <label for="fre-homepage" class="sr">Home page address</label>
      <input
        id="fre-homepage"
        bind:this={input}
        class="field"
        type="url"
        inputmode="url"
        spellcheck="false"
        autocomplete="off"
        placeholder="example.com"
        aria-invalid={!!error}
        aria-describedby="fre-homepage-help"
        bind:value={text}
        onblur={() => (touched = true)}
        oninput={() => {
          if (text.includes('.')) touched = true;
        }}
      />
      <p id="fre-homepage-help" class="help" class:err={!!error}>
        {error || (result.ok && result.url ? `Opens ${result.url}` : 'Type an address, like example.com')}
      </p>
    </div>
  {/if}
</fieldset>

<style>
  .address {
    margin-top: 10px;
    animation: open 220ms cubic-bezier(0.23, 1, 0.32, 1) both;
  }
  @keyframes open {
    from {
      opacity: 0;
      translate: 0 -4px;
    }
  }
  .field {
    width: 100%;
    height: 38px;
    font-size: 14px;
  }
  .field[aria-invalid='true'] {
    border-color: color-mix(in srgb, var(--danger) 70%, transparent);
  }
  .help {
    margin: 6px 2px 0;
    font-size: 12px;
    color: var(--text-3);
    word-break: break-all;
  }
  .help.err {
    color: var(--danger);
  }
</style>
