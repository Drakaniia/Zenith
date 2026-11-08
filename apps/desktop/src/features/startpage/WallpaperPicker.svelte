<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { startPage } from './startpage.svelte';
  import { GALLERY, GALLERY_IDS, galleryById, galleryUrl } from './wallpapers';
  import {
    BLUR_MAX,
    DIM_MAX,
    MAX_CUSTOM_IMAGES,
    backgroundChoices,
    localDay,
    parseBackground,
    stepChoice,
    textTone,
    type StartPageSettings,
  } from './startpage';

  // The Background tab of the start page's customization panel (StartPagePanel): a preview to step
  // through backgrounds, the user's images, the gallery, darken/blur and the daily background.
  // The panel owns the background being looked at and the upload, so they survive switching tabs.
  let {
    candidate = $bindable(),
    fileInput = $bindable(),
    uploading,
    error,
    onfile: onFile,
    onremove: remove,
  }: {
    /** The background being looked at. It is only applied with "Use background". */
    candidate: string;
    fileInput: HTMLInputElement | undefined;
    uploading: boolean;
    error: string;
    onfile: () => Promise<void>;
    onremove: (id: string) => Promise<void>;
  } = $props();

  const p = $derived(startPage.prefs);
  const choices = $derived(backgroundChoices(GALLERY_IDS, p.customImages));
  const position = $derived(Math.max(0, choices.indexOf(candidate)));
  const preview = $derived(startPage.resolve(candidate));
  const previewText = $derived(preview ? textTone(preview.tone, p.dim) : null);
  const applied = $derived(candidate === p.background);

  function set<K extends keyof StartPageSettings>(key: K, value: StartPageSettings[K]) {
    startPage.set({ [key]: value } as Partial<StartPageSettings>);
  }

  function nameOf(ref: string): string {
    const b = parseBackground(ref);
    if (b.kind === 'gallery') return galleryById(b.id)?.name ?? '';
    if (b.kind === 'custom') return `Your image ${p.customImages.findIndex((c) => c.id === b.id) + 1}`;
    return 'No background';
  }

  function setDaily(on: boolean) {
    // Today keeps the current background; the first new tab of each later day changes it.
    startPage.set({ dailyBackground: on, dailyDate: localDay(new Date()) });
  }
</script>

<section>
  <div class="preview" data-text={previewText}>
    {#if preview}
      <div class="pv-img" style:background-image={`url("${preview.url}")`} style:filter={p.blur ? `blur(${p.blur / 4}px)` : null}></div>
      <div class="pv-scrim" style:opacity={p.dim / 100}></div>
    {:else}
      <div class="pv-plain"></div>
    {/if}
    <div class="pv-mock" aria-hidden="true">
      <span class="pv-search"></span>
      <span class="pv-tiles"><i></i><i></i><i></i><i></i><i></i></span>
    </div>
    <button type="button" class="pv-nav prev" aria-label="Previous background" onclick={() => (candidate = stepChoice(choices, candidate, -1))}>
      <Icon name="chevronLeft" size={16} />
    </button>
    <button type="button" class="pv-nav next" aria-label="Next background" onclick={() => (candidate = stepChoice(choices, candidate, 1))}>
      <Icon name="chevronRight" size={16} />
    </button>
  </div>
  <div class="caption">
    <span class="text"><b>{nameOf(candidate)}</b><small>{position + 1} of {choices.length}</small></span>
    {#if applied}
      <span class="in-use"><Icon name="check" size={13} /> In use</span>
      {#if p.background}
        <button type="button" class="btn ghost sm" onclick={() => { startPage.apply(''); candidate = ''; }}>Remove</button>
      {/if}
    {:else}
      <button type="button" class="btn primary sm" onclick={() => startPage.apply(candidate)}>
        {candidate ? 'Use background' : 'Remove background'}
      </button>
    {/if}
  </div>
</section>

<section>
  <h3>Your images</h3>
  <div class="thumbs">
    {#each p.customImages as c, i (c.id)}
      {@const ref = `custom:${c.id}`}
      <div class="thumb-wrap">
        <button
          type="button"
          class="thumb"
          class:sel={candidate === ref}
          aria-pressed={candidate === ref}
          aria-label="Your image {i + 1}"
          style:background-image={startPage.images[c.id] ? `url("${startPage.images[c.id]}")` : null}
          onclick={() => (candidate = ref)}
        >
          {#if p.background === ref}<span class="badge"><Icon name="check" size={10} stroke={3} /></span>{/if}
        </button>
        <button type="button" class="thumb-x" aria-label="Delete your image {i + 1}" title="Delete" onclick={() => remove(c.id)}>
          <Icon name="close" size={10} stroke={2.6} />
        </button>
      </div>
    {/each}
    {#if p.customImages.length < MAX_CUSTOM_IMAGES}
      <button type="button" class="thumb add" disabled={uploading} onclick={() => fileInput?.click()}>
        <Icon name={uploading ? 'reload' : 'upload'} size={15} class={uploading ? 'spin' : ''} />
        <span>{uploading ? 'Adding…' : 'Add image'}</span>
      </button>
    {/if}
  </div>
  <input bind:this={fileInput} type="file" accept="image/jpeg,image/png,image/webp,image/gif,image/avif,image/bmp" hidden onchange={onFile} />
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<section>
  <h3>Gallery</h3>
  <div class="thumbs">
    <button type="button" class="thumb none" class:sel={candidate === ''} aria-pressed={candidate === ''} onclick={() => (candidate = '')}>
      None
      {#if p.background === ''}<span class="badge"><Icon name="check" size={10} stroke={3} /></span>{/if}
    </button>
    {#each GALLERY as w (w.id)}
      {@const ref = `gallery:${w.id}`}
      <button
        type="button"
        class="thumb"
        class:sel={candidate === ref}
        aria-pressed={candidate === ref}
        aria-label={w.name}
        title={w.name}
        style:background-image={`url("${galleryUrl(w.id)}")`}
        onclick={() => (candidate = ref)}
      >
        {#if p.background === ref}<span class="badge"><Icon name="check" size={10} stroke={3} /></span>{/if}
      </button>
    {/each}
  </div>
</section>

<section class:off={!p.background}>
  <h3>Background &amp; appearance</h3>
  <label class="slider">
    <span>Darken</span>
    <input type="range" min="0" max={DIM_MAX} step="1" value={p.dim} oninput={(e) => set('dim', Number(e.currentTarget.value))} />
    <output>{p.dim}%</output>
  </label>
  <label class="slider">
    <span>Blur</span>
    <input type="range" min="0" max={BLUR_MAX} step="1" value={p.blur} oninput={(e) => set('blur', Number(e.currentTarget.value))} />
    <output>{p.blur}px</output>
  </label>
</section>

<section>
  <div class="row">
    <span class="text"><b>New background every day</b><small>Moves through the gallery and your images on the first new tab of each day.</small></span>
    <Toggle checked={p.dailyBackground} label="New background every day" onchange={setDaily} />
  </div>
</section>

<style>
  section {
    padding: 10px 0 4px;
    transition: opacity 150ms ease;
  }
  section + section {
    border-top: 1px solid var(--line);
  }
  section.off {
    opacity: 0.5;
  }
  h3 {
    margin: 2px 0 4px;
    font: 600 12px/1.3 var(--font);
    color: var(--text-3);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    min-height: 36px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .text b {
    font-weight: 500;
  }
  .text small {
    font-size: 11.5px;
    line-height: 1.35;
    color: var(--text-3);
  }
  .btn.sm {
    height: 28px;
    padding: 0 11px;
    font-size: 12.5px;
  }

  /* Background preview: the picture with a sketch of the start page over it. */
  .preview {
    position: relative;
    aspect-ratio: 16 / 10;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid var(--line);
    background: var(--page-bg);
  }
  .pv-img,
  .pv-scrim,
  .pv-plain {
    position: absolute;
    inset: 0;
  }
  .pv-img {
    inset: -4px;
    background: center / cover no-repeat;
    transition: filter 150ms ease;
  }
  .pv-scrim {
    background: #000;
  }
  .pv-plain {
    background:
      radial-gradient(120% 70% at 50% -10%, color-mix(in srgb, var(--accent) 18%, transparent), transparent 70%),
      var(--page-bg);
  }
  .pv-mock {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 9px;
  }
  .pv-search {
    width: 56%;
    height: 14px;
    border-radius: 7px;
    background: color-mix(in srgb, var(--dialog-bg) 82%, transparent);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
  }
  .pv-tiles {
    display: flex;
    gap: 6px;
  }
  .pv-tiles i {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--dialog-bg) 82%, transparent);
  }
  .preview[data-text='light'] .pv-tiles i,
  .preview[data-text='light'] .pv-search {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
  }
  .pv-nav {
    position: absolute;
    top: 50%;
    width: 28px;
    height: 28px;
    margin-top: -14px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text);
    background: color-mix(in srgb, var(--dialog-bg) 84%, transparent);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.22);
    transition: transform 120ms var(--ease-out);
  }
  .pv-nav:active {
    transform: scale(0.92);
  }
  .pv-nav:focus-visible {
    outline: 2px solid var(--accent-button);
  }
  .prev {
    left: 8px;
  }
  .next {
    right: 8px;
  }
  .caption {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding-top: 6px;
  }
  .caption .text {
    flex: 1;
  }
  .in-use {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }

  .thumbs {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    padding: 4px 0 6px;
  }
  .thumb-wrap {
    position: relative;
  }
  .thumb {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 10;
    border-radius: 7px;
    background: var(--field) center / cover no-repeat;
    outline: 2px solid transparent;
    outline-offset: 1px;
    transition:
      outline-color 120ms ease,
      transform 120ms var(--ease-out);
  }
  .thumb:active {
    transform: scale(0.96);
  }
  .thumb.sel {
    outline-color: var(--accent-button);
  }
  .thumb:focus-visible {
    outline-color: var(--accent-button);
  }
  .thumb.none,
  .thumb.add {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-2);
    border: 1px dashed var(--line-strong);
  }
  .thumb.none {
    border-style: solid;
    border-color: var(--line);
    background: var(--page-bg);
  }
  .thumb.add:hover {
    background-color: var(--hover);
    color: var(--text);
  }
  .thumb.add:disabled {
    opacity: 0.6;
  }
  .badge {
    position: absolute;
    left: 4px;
    bottom: 4px;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: #fff;
    background: var(--accent-button);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.3);
  }
  .thumb-x {
    position: absolute;
    top: -5px;
    right: -5px;
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    color: var(--text);
    background: var(--dialog-bg);
    border: 1px solid var(--line-strong);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .thumb-wrap:hover .thumb-x,
  .thumb-x:focus-visible {
    opacity: 1;
  }
  .error {
    margin: 2px 0 6px;
    font-size: 12px;
    color: var(--danger);
  }
  :global(.spin) {
    animation: sp-spin 0.8s linear infinite;
  }
  @keyframes sp-spin {
    to {
      rotate: 360deg;
    }
  }

  .slider {
    display: grid;
    grid-template-columns: 60px 1fr 42px;
    align-items: center;
    gap: 10px;
    min-height: 34px;
  }
  .slider output {
    text-align: right;
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    color: var(--text-2);
  }
  input[type='range'] {
    width: 100%;
    height: 18px;
    margin: 0;
    background: transparent;
    appearance: none;
    accent-color: var(--accent-button);
  }
  input[type='range']::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: var(--field);
  }
  input[type='range']::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    margin-top: -5px;
    border-radius: 50%;
    background: var(--accent-button);
    border: 2px solid var(--dialog-bg);
    box-shadow: 0 0 0 1px var(--line-strong);
  }
  input[type='range']:focus-visible::-webkit-slider-thumb {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-button) 45%, transparent);
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.spin) {
      animation: none;
    }
  }
</style>
