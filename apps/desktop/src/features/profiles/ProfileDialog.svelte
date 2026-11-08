<script lang="ts">
  import { fade, scale } from 'svelte/transition';
  import { tick } from 'svelte';
  import Icon from '$shared/ui/Icon.svelte';
  import { appInfo } from '$features/window/window.svelte';
  import { browser } from '$features/browser/browser.svelte';
  import { profilesApi } from './api';
  import { webviewApi } from '$platform/webview-api';
  import { ui } from '$shared/ui/ui.svelte';
  import { focusAfterLayout, overlayLease } from '$platform/layout.svelte';
  import { MAX_PROFILE_NAME, cleanProfileName, deleteBlocker, nextProfileColor, profileInitial, textOn } from './profiles';
  import { EMOJI_CHOICES, avatarEmoji, pictureFromFile, renderEmojiIcon } from './profile-picture';
  import type { ProfileEntry, ProfilesView } from './types';
  import ProfileAvatar from './ProfileAvatar.svelte';

  // Profiles: the list (open, edit, new) and the form for one profile (name, picture, colour,
  // desktop shortcut, delete). Every profile is a separate set of data running in its own window.
  /** The picture choice in the form: the initial, the chosen image, or `emoji:<name>`. */
  const LETTER = 'letter';
  const spec = $derived(ui.profileDialog);
  let view = $state<ProfilesView | null>(null);
  /** The edit form came from the list: going back returns there. */
  let fromList = $state(false);
  let name = $state('');
  let color = $state('');
  let pick = $state(LETTER);
  /** The picture the user chose (or the profile already had), a 256 px PNG data URL. */
  let image = $state<string | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);
  let shortcut = $state(false);
  let confirmDelete = $state(false);
  let busy = $state(false);
  let error = $state('');
  let note = $state('');
  let nameInput = $state<HTMLInputElement | null>(null);

  const editing = $derived(spec?.mode === 'edit' && spec.id ? (view?.profiles.find((p) => p.id === spec.id) ?? null) : null);
  const isNew = $derived(spec?.mode === 'edit' && !spec.id);
  const blocker = $derived(editing && view ? deleteBlocker(editing, view.active) : null);
  const cleaned = $derived(cleanProfileName(name));
  const avatar = $derived(pick === LETTER ? null : pick);

  async function refresh() {
    try {
      view = await profilesApi.profilesList();
    } catch (e) {
      error = String(e);
    }
  }

  // Reload the list and reset the form whenever the dialog opens or switches view.
  $effect(() => {
    const s = spec;
    if (!s) return;
    error = '';
    note = '';
    confirmDelete = false;
    void webviewApi.focusView(null);
    void refresh().then(async () => {
      if (s.mode !== 'edit' || !view) return;
      const p = s.id ? view.profiles.find((x) => x.id === s.id) : null;
      name = p?.name ?? '';
      color = p?.color ?? nextProfileColor(view.colors, view.profiles);
      image = p?.image ?? null;
      pick = p?.avatar === 'image' ? (image ? 'image' : LETTER) : p?.avatar && avatarEmoji(p.avatar) ? p.avatar : LETTER;
      shortcut = false;
      await tick();
      nameInput?.focus();
      nameInput?.select();
    });
  });

  function close() {
    ui.profileDialog = null;
    fromList = false;
    focusAfterLayout(browser.activeTab?.live ? browser.activeTab.id : null);
  }

  function back() {
    if (fromList) {
      fromList = false;
      ui.profileDialog = { mode: 'manage' };
    } else close();
  }

  function edit(id: string | null) {
    fromList = spec?.mode === 'manage';
    ui.profileDialog = { mode: 'edit', id };
  }

  async function open(p: ProfileEntry) {
    try {
      await profilesApi.profileOpen(p.id);
      close();
    } catch (e) {
      error = String(e);
    }
  }

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

  /** The picture as the backend keeps it (a 256 px PNG) for the window and shortcut icons. */
  async function renderedIcon(): Promise<string | null> {
    if (pick === 'image') {
      if (!image) throw new Error('Choose the picture again.');
      return image;
    }
    const emoji = avatarEmoji(avatar);
    return emoji ? renderEmojiIcon(emoji, color) : null;
  }

  async function save() {
    if (busy || !cleaned) return;
    busy = true;
    error = '';
    try {
      const icon = await renderedIcon();
      if (isNew) {
        const p = await profilesApi.profileCreate(cleaned, color, avatar, icon, shortcut);
        // Like other browsers: a new profile opens right away, in its own window.
        await profilesApi.profileOpen(p.id);
        ui.toast(`Opening “${p.name}” in a new window`, { icon: 'user' });
        close();
      } else if (editing) {
        const p = await profilesApi.profileUpdate(editing.id, cleaned, color, avatar, icon);
        if (appInfo.info && appInfo.info.profile.id === p.id) appInfo.info.profile = p;
        back();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function makeShortcut() {
    if (!editing || busy) return;
    busy = true;
    error = '';
    try {
      await profilesApi.profileShortcut(editing.id);
      note = `Added “Zenith - ${editing.name}” to the desktop.`;
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!editing || busy) return;
    busy = true;
    error = '';
    try {
      await profilesApi.profileDelete(editing.id);
      ui.toast(`Deleted the profile “${editing.name}”`, { icon: 'trash' });
      fromList = true;
      back();
    } catch (e) {
      error = String(e);
      confirmDelete = false;
    } finally {
      busy = false;
    }
  }
</script>

{#if spec}
  <div class="root" role="presentation" use:overlayLease>
    <div class="scrim" role="presentation" transition:fade={{ duration: 120 }} onpointerdown={close}></div>
    <div
      class="dialog glass"
      role="dialog"
      aria-modal="true"
      aria-label={spec.mode === 'manage' ? 'Profiles' : isNew ? 'New profile' : 'Edit profile'}
      tabindex="-1"
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          if (confirmDelete) confirmDelete = false;
          else back();
        }
      }}
      transition:scale={{ start: 0.97, duration: 140 }}
    >
      <header>
        <h2>{spec.mode === 'manage' ? 'Profiles' : isNew ? 'New profile' : 'Edit profile'}</h2>
        <button class="icon-btn sm" title="Close" onclick={close}><Icon name="close" size={14} /></button>
      </header>

      {#if spec.mode === 'manage'}
        <div class="body">
          <p class="lead">Each profile keeps its own history, bookmarks, passwords, cookies, extensions and settings, and opens in its own window.</p>
          {#if view}
            <ul class="list">
              {#each view.profiles as p (p.id)}
                <li>
                  <ProfileAvatar name={p.name} color={p.color} avatar={p.avatar} image={p.image} size={26} />
                  <span class="name">
                    <span class="ellipsis">{p.name}</span>
                    <small>{p.id === view.active ? 'This window' : p.running ? 'Open' : p.isDefault ? 'Default profile' : ''}</small>
                  </span>
                  {#if p.id !== view.active}
                    <button class="btn sm" onclick={() => open(p)}>{p.running ? 'Switch to' : 'Open'}</button>
                  {/if}
                  <button class="icon-btn sm" title={`Edit “${p.name}”`} onclick={() => edit(p.id)}><Icon name="edit" size={14} /></button>
                </li>
              {/each}
            </ul>
          {:else if !error}
            <p class="note">Loading…</p>
          {/if}
          {#if error}<p class="error" role="alert">{error}</p>{/if}
        </div>
        <footer>
          <button class="btn" onclick={() => edit(null)}><Icon name="plus" size={15} />New profile</button>
          <span class="grow"></span>
          <button class="btn primary" onclick={close}>Done</button>
        </footer>
      {:else}
        <form
          class="body"
          onsubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <div class="preview">
            <ProfileAvatar name={cleaned || '?'} {color} {avatar} {image} size={44} />
            <label class="name-field">
              <span>Name</span>
              <input bind:this={nameInput} bind:value={name} maxlength={MAX_PROFILE_NAME + 20} placeholder="Work, Personal, School…" spellcheck="false" autocomplete="off" />
            </label>
          </div>

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

          {#if view}
            <fieldset>
              <legend>Colour</legend>
              <div class="colors">
                {#each view.colors as c (c)}
                  <label class="color" class:on={color === c} style:--c={c} title={c}>
                    <input type="radio" name="profile-color" value={c} bind:group={color} />
                    {#if color === c}<Icon name="check" size={13} />{/if}
                  </label>
                {/each}
              </div>
            </fieldset>
          {/if}

          {#if isNew}
            <label class="check">
              <input type="checkbox" bind:checked={shortcut} />
              <span><b>Add a desktop shortcut</b><small>Opens Zenith straight into this profile.</small></span>
            </label>
          {:else if editing}
            <div class="row">
              <div>
                <b>Desktop shortcut</b>
                <small>{editing.shortcut ? 'On your desktop. Create it again after renaming the profile.' : 'Opens Zenith straight into this profile.'}</small>
              </div>
              <button type="button" class="btn" disabled={busy} onclick={makeShortcut}>{editing.shortcut ? 'Recreate' : 'Create'}</button>
            </div>

            {#if !editing.isDefault}
              <div class="danger-zone">
                {#if confirmDelete}
                  <p>
                    Delete “{editing.name}” and everything in it? Its history, bookmarks, saved passwords, cookies, extensions and
                    settings are removed from this PC. This can't be undone.
                  </p>
                  <div class="actions">
                    <button type="button" class="btn" onclick={() => (confirmDelete = false)}>Cancel</button>
                    <button type="button" class="btn danger" disabled={busy} onclick={remove}><Icon name="trash" size={15} />Delete profile</button>
                  </div>
                {:else}
                  <div class="row">
                    <div>
                      <b>Delete profile</b>
                      <small>{blocker ?? 'Removes the profile and all of its data.'}</small>
                    </div>
                    <button type="button" class="btn danger" disabled={!!blocker || busy} onclick={() => (confirmDelete = true)}>Delete…</button>
                  </div>
                {/if}
              </div>
            {/if}
          {/if}

          {#if note}<p class="ok" role="status">{note}</p>{/if}
          {#if error}<p class="error" role="alert">{error}</p>{/if}

          <footer class="form-footer">
            <button type="button" class="btn" onclick={back}>{fromList ? 'Back' : 'Cancel'}</button>
            <button type="submit" class="btn primary" disabled={busy || !cleaned || (!isNew && !editing)} aria-busy={busy}>
              {isNew ? 'Create and open' : 'Save'}
            </button>
          </footer>
        </form>
      {/if}
    </div>
  </div>
{/if}

<style>
  .root {
    position: fixed;
    inset: 0;
    z-index: 88;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--backdrop);
  }
  .dialog {
    position: relative;
    width: min(460px, 100%);
    max-height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: 16px;
    corner-shape: var(--sq);
    background: var(--dialog-bg);
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 16px 4px 20px;
  }
  h2 {
    margin: 0;
    font: 600 16px/1.3 var(--font-display);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 10px 20px 4px;
    overflow-y: auto;
    margin: 0;
  }
  .lead,
  .note {
    margin: 0;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 6px 0 10px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    background: var(--field);
    border: 1px solid var(--line);
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .preview {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .name-field {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
  }
  .name-field input {
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r-btn);
    corner-shape: var(--sq);
    border: 1px solid var(--line);
    background: var(--field);
    color: var(--text-1, inherit);
    font: inherit;
    font-weight: 400;
    font-size: 13.5px;
    outline: none;
  }
  .name-field input:focus {
    background: var(--field-focus);
    border-color: color-mix(in srgb, var(--accent) 70%, transparent);
  }
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
  .colors {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
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
  .color {
    position: relative;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--c);
    color: #fff;
    cursor: pointer;
    box-shadow: 0 0 0 1px color-mix(in srgb, currentColor 15%, transparent);
  }
  .color.on {
    box-shadow:
      0 0 0 2px var(--dialog-bg),
      0 0 0 4px var(--c);
  }
  .color input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .color:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .check input {
    margin-top: 3px;
    accent-color: var(--accent);
  }
  .check span,
  .row > div {
    display: flex;
    flex-direction: column;
    min-width: 0;
    font-size: 13px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  small {
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .danger-zone {
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .danger-zone p {
    margin: 0 0 10px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--danger);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .ok {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-btn);
    font-size: 12.5px;
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 12%, transparent);
  }
  .error {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--r-btn);
    font-size: 12.5px;
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px 18px;
  }
  .form-footer {
    padding: 4px 0 14px;
  }
  .grow {
    flex: 1;
  }
</style>
