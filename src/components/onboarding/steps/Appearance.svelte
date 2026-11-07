<script lang="ts">
  import Choice from '../Choice.svelte';
  import { onboarding } from '../../../lib/onboarding.svelte';
  import { settings, type Settings } from '../../../lib/settings.svelte';
  import { SPACE_COLORS } from '../../../lib/browser.svelte';
  import { ACCENT_NAMES as COLOR_NAMES } from '../../../lib/onboarding-flow';

  const s = $derived(settings.value);

  const THEMES: { value: Settings['theme']; title: string; desc: string }[] = [
    { value: 'system', title: 'Match Windows', desc: 'Follows light or dark mode' },
    { value: 'light', title: 'Light', desc: 'Always light' },
    { value: 'dark', title: 'Dark', desc: 'Always dark' },
  ];

  const LAYOUTS: { value: Settings['toolbarMode']; title: string; desc: string }[] = [
    { value: 'single', title: 'Single toolbar', desc: 'Address bar in the sidebar. The most room for pages.' },
    { value: 'multiple', title: 'Multiple toolbars', desc: 'A toolbar with the address bar above the page.' },
    { value: 'collapsed', title: 'Collapsed', desc: 'Icon-only tabs, toolbar across the top.' },
  ];

</script>

<fieldset>
  <legend>Theme</legend>
  <div class="grid three">
    {#each THEMES as t (t.value)}
      <Choice name="fre-theme" value={t.value} selected={s.theme} title={t.title} desc={t.desc} onselect={(v) => onboarding.set('theme', v)}>
        {#snippet art()}
          <span class="theme-art {t.value}"><span></span></span>
        {/snippet}
      </Choice>
    {/each}
  </div>
</fieldset>

<fieldset>
  <legend>Accent colour <span class="hint">Tints the window and highlights. Each space can have its own.</span></legend>
  <div class="swatches">
    {#each SPACE_COLORS as c (c)}
      <label class="swatch" style:--c={c} title={COLOR_NAMES[c] ?? c}>
        <input
          type="radio"
          name="fre-accent"
          value={c}
          checked={onboarding.accent === c}
          aria-label={COLOR_NAMES[c] ?? c}
          onchange={() => onboarding.setAccent(c)}
        />
      </label>
    {/each}
  </div>
</fieldset>

<fieldset>
  <legend>Layout</legend>
  <div class="grid three">
    {#each LAYOUTS as l (l.value)}
      <Choice
        name="fre-layout"
        value={l.value}
        selected={s.toolbarMode}
        title={l.title}
        desc={l.desc}
        onselect={(v) => onboarding.set('toolbarMode', v)}
      >
        {#snippet art()}
          <!-- A small window: sidebar, toolbar and page in their places for this layout. -->
          <svg class="layout-art" viewBox="0 0 120 64" aria-hidden="true">
            <rect class="win" x="0.5" y="0.5" width="119" height="63" rx="7" />
            {#if l.value === 'single'}
              <rect class="side" x="6" y="6" width="30" height="52" rx="3" />
              <rect class="pill" x="9" y="9" width="24" height="6" rx="3" />
              <rect class="tab" x="9" y="20" width="24" height="4" rx="2" />
              <rect class="tab" x="9" y="27" width="18" height="4" rx="2" />
              <rect class="page" x="40" y="6" width="74" height="52" rx="4" />
            {:else if l.value === 'multiple'}
              <rect class="side" x="6" y="6" width="30" height="52" rx="3" />
              <rect class="tab" x="9" y="9" width="24" height="4" rx="2" />
              <rect class="tab" x="9" y="16" width="18" height="4" rx="2" />
              <rect class="pill" x="40" y="6" width="74" height="8" rx="4" />
              <rect class="page" x="40" y="18" width="74" height="40" rx="4" />
            {:else}
              <rect class="pill" x="6" y="6" width="108" height="8" rx="4" />
              <rect class="side" x="6" y="18" width="10" height="40" rx="3" />
              <rect class="tab" x="8" y="21" width="6" height="6" rx="2" />
              <rect class="tab" x="8" y="30" width="6" height="6" rx="2" />
              <rect class="page" x="20" y="18" width="94" height="40" rx="4" />
            {/if}
          </svg>
        {/snippet}
      </Choice>
    {/each}
  </div>
</fieldset>

<style>
  .theme-art {
    display: block;
    height: 46px;
    border-radius: 8px;
    corner-shape: var(--sq);
    overflow: hidden;
    position: relative;
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .theme-art span {
    position: absolute;
    inset: 9px 9px 0 30%;
    border-radius: 5px 5px 0 0;
  }
  .theme-art.light {
    background: #e9e9ec;
  }
  .theme-art.light span {
    background: #fff;
  }
  .theme-art.dark {
    background: #18181b;
  }
  .theme-art.dark span {
    background: #26262b;
  }
  .theme-art.system {
    background: linear-gradient(105deg, #e9e9ec 50%, #18181b 50%);
  }
  .theme-art.system span {
    background: linear-gradient(105deg, #fff 42%, #26262b 42%);
  }
  .theme-art span::before {
    content: '';
    position: absolute;
    left: 8px;
    top: 8px;
    width: 40%;
    height: 4px;
    border-radius: 2px;
    background: var(--accent);
    opacity: 0.8;
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 2px;
  }
  .swatch {
    position: relative;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    background: var(--c);
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
    transition:
      scale 160ms ease-out,
      box-shadow 150ms ease;
  }
  .swatch:active {
    scale: 0.92;
  }
  .swatch input {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
  }
  .swatch:has(input:checked) {
    box-shadow:
      0 0 0 2px var(--dialog-bg),
      0 0 0 4px var(--c);
  }
  .swatch:has(input:focus-visible) {
    outline: 2px solid var(--text);
    outline-offset: 5px;
  }

  .layout-art {
    display: block;
    width: 100%;
    height: auto;
    max-height: 64px;
  }
  .layout-art .win {
    fill: color-mix(in srgb, var(--text) 4%, transparent);
    stroke: var(--line-strong);
  }
  .layout-art .side {
    fill: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .layout-art .pill {
    fill: color-mix(in srgb, var(--text) 16%, transparent);
  }
  .layout-art .tab {
    fill: color-mix(in srgb, var(--text) 22%, transparent);
  }
  .layout-art .page {
    fill: color-mix(in srgb, var(--text) 9%, transparent);
  }
</style>
