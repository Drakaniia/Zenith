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
  import { cleanProfileName, deleteBlocker, nextProfileColor } from './profiles';
  import { avatarEmoji, renderEmojiIcon } from './profile-picture';
  import type { ProfileEntry, ProfilesView } from './types';
  import ProfileAvatar from './ProfileAvatar.svelte';
  import ProfileForm from './ProfileForm.svelte';
  import { LETTER } from './AvatarPicker.svelte';

  // Profiles: the list (open, edit, new) and the form for one profile (name, picture, colour,
  // desktop shortcut, delete). Every profile is a separate set of data running in its own window.
  const spec = $derived(ui.profileDialog);
  let view = $state<ProfilesView | null>(null);
  /** The edit form came from the list: going back returns there. */
  let fromList = $state(false);
  let name = $state('');
  let color = $state('');
  let pick = $state(LETTER);
  /** The picture the user chose (or the profile already had), a 256 px PNG data URL. */
  let image = $state<string | null>(null);
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
        <ProfileForm
          bind:name
          bind:color
          bind:pick
          bind:image
          bind:shortcut
          bind:confirmDelete
          bind:error
          bind:nameInput
          {view}
          {editing}
          {isNew}
          {blocker}
          {cleaned}
          {avatar}
          {busy}
          {note}
          {fromList}
          onsave={save}
          onshortcut={makeShortcut}
          onremove={remove}
          onback={back}
        />
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
  small {
    color: var(--text-3);
    font-size: 11.5px;
    line-height: 1.4;
  }
  .btn.sm {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
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
  .grow {
    flex: 1;
  }
</style>
