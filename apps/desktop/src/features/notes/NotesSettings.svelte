<script lang="ts">
  import Icon from '$shared/ui/Icon.svelte';
  import Toggle from '$shared/ui/Toggle.svelte';
  import { settings, type Settings } from '$features/settings/settings.svelte';
  import { openNotes } from './actions';
  import { displayChord } from '$features/shortcuts/shortcuts';

  /** Scrolls the settings page to another section (the shortcut editor). */
  let { jump }: { jump: (id: string) => void } = $props();

  const s = $derived(settings.value);

  function set<K extends keyof Settings>(key: K, value: Settings[K]) {
    settings.update({ [key]: value } as Partial<Settings>);
  }

  const notesKeys = $derived(s.shortcuts.openNotes ?? []);
  const notesConflicts = $derived(notesKeys.filter((k) => settings.globalShortcutConflicts.includes(k)));

  function setNotesGlobal(v: boolean) {
    set('notesGlobalShortcut', v);
    void settings.pushShortcuts();
  }
</script>

<!-- Notes -->
<section id="settings-notes">
  <h2>Notes</h2>
  <div class="group">
    <div class="row">
      <div class="text">
        <b>Notes shortcut</b>
        <span>
          {#if notesKeys.length}
            {#each notesKeys as chord, ci (chord)}{#if ci > 0}{' or '}{/if}{#each displayChord(chord) as part, i (i)}<span class="kbd">{part}</span>{/each}{/each}
            opens your notes.
          {:else}
            No shortcut is set.
          {/if}
        </span>
      </div>
      <div class="inline">
        <button class="btn ghost" onclick={() => jump('shortcuts')}>Change</button>
        <button class="btn" onclick={() => openNotes()}><Icon name="note" size={15} />Open notes</button>
      </div>
    </div>
    <div class="row">
      <div class="text">
        <b>Use the shortcut in every app</b>
        <span class:err={s.notesGlobalShortcut && notesConflicts.length > 0}>
          {#if s.notesGlobalShortcut && notesConflicts.length}
            {notesConflicts.join(', ')} is already taken by another app, so it only works inside Zenith. Choose another shortcut below.
          {:else}
            Brings Zenith to the front with your notes, even while you're in another app. While Zenith is running, other apps don't receive this key combination.
          {/if}
        </span>
      </div>
      <Toggle checked={s.notesGlobalShortcut} onchange={setNotesGlobal} label="Use the notes shortcut in every app" />
    </div>
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
  .err {
    color: var(--danger);
  }
  .inline {
    display: flex;
    gap: 6px;
  }
</style>
