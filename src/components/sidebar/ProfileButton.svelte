<script lang="ts">
  import { appInfo } from '../../lib/app.svelte';
  import { api } from '../../lib/ipc';
  import { ui, type MenuItem } from '../../lib/ui.svelte';
  import { profileInitial, textOn } from '../../lib/profiles';

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
          swatch: { color: p.color, fg: textOn(p.color), text: profileInitial(p.name) },
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
    <span class="avatar" style:background={profile.color} style:color={textOn(profile.color)}>{profileInitial(profile.name)}</span>
  </button>
{/if}

<style>
  .avatar {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1;
    box-shadow: 0 0 0 1px color-mix(in srgb, currentColor 18%, transparent);
  }
</style>
