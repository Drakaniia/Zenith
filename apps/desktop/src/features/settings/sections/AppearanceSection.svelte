<script lang="ts">
  import Toggle from '$shared/ui/Toggle.svelte';
  import Segmented from '$shared/ui/Segmented.svelte';
  import { settings, SIDEBAR_MAX, SIDEBAR_MIN, type Settings } from '../settings.svelte';
  import { ui } from '$shared/ui/ui.svelte';
  import { shortcutHint } from '$features/shortcuts/hints';
  import { SIDEBAR_POSITIONS, isStrip, positionHint } from '$features/sidebar/sidebar-position';
  import { onboarding } from '$features/onboarding/onboarding.svelte';
  import { ACCENT_NAMES } from '$features/onboarding/onboarding-flow';
  import { browser, SPACE_COLORS } from '$features/browser/browser.svelte';

  const s = $derived(settings.value);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }
</script>

<!-- Appearance -->
<section id="settings-appearance">
  <h2>Appearance</h2>
  <div class="group">
    <div class="row">
      <div class="text"><b>Theme</b></div>
      <Segmented
        value={s.theme}
        options={[
          { value: 'system', label: 'Windows' },
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark' },
        ]}
        onchange={(v) => set('theme', v)}
      />
    </div>
    {#if browser.activeSpace}
      <div class="row">
        <div class="text">
          <b>Accent colour</b>
          <span>Tints the window and highlights for the “{browser.activeSpace.name}” space. Each space can have its own.</span>
        </div>
        <div class="swatches" role="radiogroup" aria-label="Accent colour">
          {#each SPACE_COLORS as c (c)}
            <label class="swatch" style:--c={c} title={ACCENT_NAMES[c] ?? c}>
              <input
                type="radio"
                name="settings-accent"
                value={c}
                checked={onboarding.accent === c}
                aria-label={ACCENT_NAMES[c] ?? c}
                onchange={() => onboarding.setAccent(c)}
              />
            </label>
          {/each}
        </div>
      </div>
    {/if}
    <div class="row">
      <div class="text"><b>Website appearance</b><span>The color scheme websites are asked to use.</span></div>
      <Segmented
        value={s.contentScheme}
        options={[
          { value: 'system', label: 'Windows' },
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark' },
        ]}
        onchange={(v) => set('contentScheme', v)}
      />
    </div>
    <div class="row">
      <div class="text">
        <b>Sidebar position</b>
        <span>{positionHint(s.sidebarPosition)} You can also right-click the sidebar to move it.</span>
      </div>
      <Segmented
        value={s.sidebarPosition}
        options={SIDEBAR_POSITIONS}
        onchange={(v) => {
          ui.flyout = false;
          set('sidebarPosition', v);
        }}
      />
    </div>
    <div class="row">
      <div class="text">
        <b>Layout</b>
        <span>
          {isStrip(s.sidebarPosition) && s.toolbarMode === 'single'
            ? 'A tab strip keeps the toolbar above the page; the single toolbar applies with the sidebar on the left or right.'
            : isStrip(s.sidebarPosition) && s.toolbarMode === 'collapsed'
              ? 'Icon-only tabs in the strip, with a toolbar above the page.'
              : s.toolbarMode === 'single'
            ? 'Address bar in the sidebar; window controls appear at the top edge.'
            : s.toolbarMode === 'multiple'
              ? 'A toolbar with the address bar above the page.'
              : 'Icon-only sidebar with a toolbar above the page.'}
        </span>
      </div>
      <Segmented
        value={s.toolbarMode}
        options={[
          { value: 'single', label: 'Single toolbar' },
          { value: 'multiple', label: 'Multiple toolbars' },
          { value: 'collapsed', label: 'Collapsed' },
        ]}
        onchange={(v) => set('toolbarMode', v)}
      />
    </div>
    <div class="row">
      <div class="text"><b>Clear button</b><span>Show "Clear" between pinned and regular tabs.</span></div>
      <Toggle checked={s.showClearButton} onchange={(v) => set('showClearButton', v)} label="Clear button" />
    </div>
    <div class="row">
      <div class="text"><b>Compact mode</b><span>Hide the sidebar and toolbar; reveal them from the window edges.{shortcutHint('toggleCompact') ? ` ${shortcutHint('toggleCompact')}.` : ''}</span></div>
      <Toggle checked={s.compact} onchange={(v) => set('compact', v)} label="Compact mode" />
    </div>
    {#if !isStrip(s.sidebarPosition)}
    <div class="row">
      <div class="text"><b>Sidebar width</b><span>{s.sidebarWidth}px — you can also drag the sidebar edge (double-click it to reset).</span></div>
      <input
        type="range"
        min={SIDEBAR_MIN}
        max={SIDEBAR_MAX}
        step="1"
        value={s.sidebarWidth}
        oninput={(e) => (settings.value.sidebarWidth = Number((e.currentTarget as HTMLInputElement).value))}
        onchange={() => settings.save(false)}
      />
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
  .swatches {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    padding: 4px;
  }
  .swatch {
    position: relative;
    width: 22px;
    height: 22px;
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
    cursor: pointer;
  }
  .swatch:has(input:checked) {
    outline: 2px solid var(--c);
    outline-offset: 2px;
  }
  .swatch:has(input:focus-visible) {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  input[type='range'] {
    width: 180px;
    accent-color: var(--accent);
  }
</style>
