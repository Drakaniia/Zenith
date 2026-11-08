<script lang="ts" module>
  /** The picture choice in the form: the initial, the chosen image, or `emoji:<name>`. */
  export const LETTER = 'letter';
</script>

<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { profileInitial, textOn } from './profiles';
  import { EMOJI_CHOICES, pictureFromFile } from './profile-picture';

  // The picture choice of the profile form: the initial, the user's own picture (uploaded here) or an emoji.
  let {
    pick = $bindable(),
    image = $bindable(),
    error = $bindable(),
    color,
    cleaned,
  }: {
    pick: string;
    /** The picture the user chose (or the profile already had), a 256 px PNG data URL. */
    image: string | null;
    error: string;
    color: string;
    cleaned: string;
  } = $props();

  let fileInput = $state<HTMLInputElement | null>(null);

  async function choosePicture() {
    const file = fileInput?.files?.[0];
    if (fileInput) fileInput.value = '';
    if (!file) return;
    error = '';
    try {
      image = await pictureFromFile(file);
      pick = 'image';
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<fieldset>
  <legend>Picture</legend>
  <div class="pictures">
    <label class="pic" class:on={pick === LETTER} title="Initial">
      <input type="radio" name="profile-picture" value={LETTER} bind:group={pick} aria-label="Initial" />
      <span class="face" style:background={color} style:color={textOn(color)}>{profileInitial(cleaned || '?')}</span>
    </label>
    {#if image}
      <label class="pic" class:on={pick === 'image'} title="Your picture">
        <input type="radio" name="profile-picture" value="image" bind:group={pick} aria-label="Your picture" />
        <img class="face photo" src={image} alt="" draggable="false" />
      </label>
    {/if}
    <button type="button" class="pic upload" title={image ? 'Choose another picture…' : 'Choose a picture…'} aria-label={image ? 'Choose another picture' : 'Choose a picture from your files'} onclick={() => fileInput?.click()}>
      <Icon name="upload" size={14} />
    </button>
    {#each EMOJI_CHOICES as e (e.id)}
      <label class="pic" class:on={pick === e.avatar} title={e.label}>
        <input type="radio" name="profile-picture" value={e.avatar} bind:group={pick} aria-label={e.label} />
        <span class="face" style:background={color}><img src={e.src} alt="" draggable="false" /></span>
      </label>
    {/each}
  </div>
  <input bind:this={fileInput} type="file" accept="image/png,image/jpeg,image/webp,image/gif,image/avif,image/bmp" hidden onchange={choosePicture} />
</fieldset>

<style>
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: 8px;
    padding: 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .pictures {
    display: grid;
    grid-template-columns: repeat(auto-fill, 32px);
    gap: 8px;
    max-height: 164px;
    overflow-y: auto;
    /* Room for the selection ring, which sits outside each cell. */
    padding: 4px;
    margin: -4px;
    scrollbar-width: thin;
  }
  .pic {
    position: relative;
    width: 32px;
    height: 32px;
    padding: 0;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--text-2);
    cursor: pointer;
    transition: scale var(--fast, 120ms) ease;
  }
  .pic:hover {
    scale: 1.08;
  }
  .pic:active {
    scale: 0.94;
  }
  .pic input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .pic.on {
    box-shadow:
      0 0 0 2px var(--dialog-bg),
      0 0 0 4px var(--accent);
  }
  .pic:has(input:focus-visible),
  .pic.upload:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  .pic.upload {
    border: 1.5px dashed color-mix(in srgb, var(--text-2) 55%, transparent);
  }
  .pic.upload:hover {
    color: var(--text);
    border-color: var(--text-2);
  }
  .face {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 50%;
    font-size: 14px;
    font-weight: 700;
    line-height: 1;
  }
  .face img {
    width: 62%;
    height: 62%;
    display: block;
    pointer-events: none;
  }
  img.face.photo {
    object-fit: cover;
    pointer-events: none;
  }
</style>
