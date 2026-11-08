<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import { MAX_PROFILE_NAME } from './profiles';
  import type { ProfileEntry, ProfilesView } from './types';
  import ProfileAvatar from './ProfileAvatar.svelte';
  import AvatarPicker from './AvatarPicker.svelte';

  // The form for one profile (name, picture, colour, desktop shortcut, delete). The dialog owns the
  // state and the actions; this is the edit mode's markup.
  let {
    name = $bindable(),
    color = $bindable(),
    pick = $bindable(),
    image = $bindable(),
    shortcut = $bindable(),
    confirmDelete = $bindable(),
    error = $bindable(),
    nameInput = $bindable(),
    view,
    editing,
    isNew,
    blocker,
    cleaned,
    avatar,
    busy,
    note,
    fromList,
    onsave,
    onshortcut,
    onremove,
    onback,
  }: {
    name: string;
    color: string;
    pick: string;
    image: string | null;
    shortcut: boolean;
    confirmDelete: boolean;
    error: string;
    nameInput: HTMLInputElement | null;
    view: ProfilesView | null;
    editing: ProfileEntry | null;
    isNew: boolean;
    blocker: string | null;
    cleaned: string;
    avatar: string | null;
    busy: boolean;
    note: string;
    fromList: boolean;
    onsave: () => Promise<void>;
    onshortcut: () => Promise<void>;
    onremove: () => Promise<void>;
    onback: () => void;
  } = $props();
</script>

<form
  class="body"
  onsubmit={(e) => {
    e.preventDefault();
    void onsave();
  }}
>
  <div class="preview">
    <ProfileAvatar name={cleaned || '?'} {color} {avatar} {image} size={44} />
    <label class="name-field">
      <span>Name</span>
      <input bind:this={nameInput} bind:value={name} maxlength={MAX_PROFILE_NAME + 20} placeholder="Work, Personal, School…" spellcheck="false" autocomplete="off" />
    </label>
  </div>

  <AvatarPicker bind:pick bind:image bind:error {color} {cleaned} />

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
      <button type="button" class="btn" disabled={busy} onclick={onshortcut}>{editing.shortcut ? 'Recreate' : 'Create'}</button>
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
            <button type="button" class="btn danger" disabled={busy} onclick={onremove}><Icon name="trash" size={15} />Delete profile</button>
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
    <button type="button" class="btn" onclick={onback}>{fromList ? 'Back' : 'Cancel'}</button>
    <button type="submit" class="btn primary" disabled={busy || !cleaned || (!isNew && !editing)} aria-busy={busy}>
      {isNew ? 'Create and open' : 'Save'}
    </button>
  </footer>
</form>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 10px 20px 4px;
    overflow-y: auto;
    margin: 0;
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
</style>
