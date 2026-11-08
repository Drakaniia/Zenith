<script lang="ts">
  import { appInfo } from '$platform/app.svelte';
  import { api } from '$platform/ipc';
  import { ui, type MenuItem } from '$shared/ui/ui.svelte';
  import { textOn } from './profiles';
  import { avatarFace } from './profile-picture';
  import type { ProfileInfo } from '$shared/types';
  import ProfileAvatar from './ProfileAvatar.svelte';

  /** A menu row's badge for a profile: its picture, emoji or initial. */
  function swatchOf(p: ProfileInfo): NonNullable<MenuItem['swatch']> {
    const face = avatarFace(p);
    const base = { color: p.color, fg: textOn(p.color) };
    if (face.kind === 'letter') return { ...base, text: face.text };
    return { ...base, text: '', img: face.src, photo: face.kind === 'image' };
  }

  // The profile avatar at the top of the sidebar: which profile this window belongs to, and a
  // menu to open another profile (each one runs in its own window) or manage them.
  const profile = $derived(appInfo.info?.profile);
  let busy = false;

  async function openMenu(e: MouseEvent) {
    if (busy) return;
    busy = true;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    try {
      const view = await api.profilesList();
      const current = view.profiles.find((p) => p.id === view.active);
      const items: MenuItem[] = [
        { label: 'Profiles', heading: true },
        ...view.profiles.map<MenuItem>((p) => ({
          label: p.name,
          swatch: swatchOf(p),
          hint: p.id === view.active ? 'This window' : p.running ? 'Open' : undefined,
          run: () => void open(p.id),
        })),
        { separator: true },
        { label: 'New profile…', icon: 'plus', run: () => (ui.profileDialog = { mode: 'edit', id: null }) },
      ];
      if (current) {
        items.push({ label: `Edit “${current.name}”…`, icon: 'edit', run: () => (ui.profileDialog = { mode: 'edit', id: current.id }) });
      }
      items.push({ label: 'Manage profiles…', icon: 'user', run: () => (ui.profileDialog = { mode: 'manage' }) });
      ui.openMenu(r.left, r.bottom + 4, items);
    } catch (err) {
      ui.toast(`Couldn't load the profiles: ${err}`, { icon: 'alert' });
    } finally {
      busy = false;
    }
  }

  async function open(id: string) {
    try {
      await api.profileOpen(id);
    } catch (err) {
      ui.toast(String(err), { icon: 'alert' });
    }
  }
</script>

{#if profile}
  <button
    class="icon-btn avatar-btn"
    title={`Profile: ${profile.name}`}
    aria-label={`Profile: ${profile.name}. Switch or manage profiles`}
    data-action="profiles"
    onclick={openMenu}
  >
    <ProfileAvatar name={profile.name} color={profile.color} avatar={profile.avatar} image={profile.image} size={18} ring />
  </button>
{/if}
