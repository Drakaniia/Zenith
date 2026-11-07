<script lang="ts">
  import Icon from '../Icon.svelte';
  import { settings } from '../../lib/settings.svelte';
  import { ui } from '../../lib/ui.svelte';
  import { api } from '../../lib/ipc';
  import { ACTIONS, ACTION_LABELS, GROUPS, chordFromEvent, defaultShortcuts, displayChord, isReserved } from '../../lib/shortcuts';

  const groups = GROUPS;
  let filter = $state('');
  let recording = $state<string | null>(null);
  let message = $state('');
  let pendingConflict = $state<{ chord: string; owner: string } | null>(null);

  const shortcuts = $derived(settings.value.shortcuts);

  function owner(chord: string, except: string): string | null {
    for (const [action, keys] of Object.entries(shortcuts)) {
      if (action !== except && keys.includes(chord)) return action;
    }
    return null;
  }

  async function startRecording(action: string) {
    recording = action;
    message = 'Press the new shortcut… (Esc to cancel)';
    pendingConflict = null;
    ui.shortcutRecording = true;
    window.addEventListener('keydown', onKey, true);
    // Let the chrome see the raw keys while recording.
    await api.setShortcuts([]);
  }

  async function stopRecording() {
    window.removeEventListener('keydown', onKey, true);
    recording = null;
    pendingConflict = null;
    ui.shortcutRecording = false;
    await settings.pushShortcuts();
  }

  function assign(action: string, chord: string) {
    const keys = settings.value.shortcuts[action] ?? [];
    if (!keys.includes(chord)) settings.value.shortcuts[action] = [...keys, chord];
    settings.save(false);
  }

  function onKey(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      message = '';
      void stopRecording();
      return;
    }
    const chord = chordFromEvent(e);
    if (!chord) return;
    const reserved = isReserved(chord);
    if (reserved) {
      message = `${displayChord(chord).join(' ')} — ${reserved}`;
      return;
    }
    const taken = owner(chord, recording);
    if (taken && pendingConflict?.chord !== chord) {
      pendingConflict = { chord, owner: taken };
      message = `${displayChord(chord).join(' ')} is used by “${ACTION_LABELS[taken] ?? taken}”. Press it again to reassign.`;
      return;
    }
    if (taken) {
      settings.value.shortcuts[taken] = settings.value.shortcuts[taken].filter((k) => k !== chord);
    }
    assign(recording, chord);
    message = '';
    void stopRecording().then(() => {
      // System-wide shortcuts (the notes shortcut) can be owned by another app.
      if (settings.globalShortcutConflicts.includes(chord))
        message = `${displayChord(chord).join(' ')} is taken by another app, so it only works inside Zenith.`;
    });
  }

  function remove(action: string, chord: string) {
    settings.value.shortcuts[action] = settings.value.shortcuts[action].filter((k) => k !== chord);
    settings.save(false);
    void settings.pushShortcuts();
  }

  function resetAll() {
    settings.value.shortcuts = defaultShortcuts();
    settings.save(false);
    void settings.pushShortcuts();
  }

  const visible = $derived(
    ACTIONS.filter((a) => !filter || a.label.toLowerCase().includes(filter.toLowerCase()) || (shortcuts[a.id] ?? []).some((k) => k.toLowerCase().includes(filter.toLowerCase()))),
  );
</script>

<div class="editor">
  <div class="tools">
    <input class="field grow" placeholder="Search shortcuts" bind:value={filter} />
    <button class="btn ghost" onclick={resetAll}>Reset all</button>
  </div>
  {#if message}<div class="message">{message}</div>{/if}
  {#each groups as group (group)}
    {@const items = visible.filter((a) => a.group === group)}
    {#if items.length}
      <div class="group-title">{group}</div>
      <div class="group">
        {#each items as a (a.id)}
          <div class="row" class:recording={recording === a.id}>
            <span class="label">{a.label}</span>
            <div class="keys">
              {#each shortcuts[a.id] ?? [] as chord (chord)}
                <span class="chord">
                  {#each displayChord(chord) as part, i (i)}<span class="kbd">{part}</span>{/each}
                  <button class="x" title="Remove" onclick={() => remove(a.id, chord)}><Icon name="close" size={11} /></button>
                </span>
              {/each}
              {#if recording === a.id}
                <span class="listening">Listening…</span>
              {:else}
                <button class="add" title="Add shortcut" onclick={() => startRecording(a.id)}><Icon name="plus" size={13} /></button>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/each}
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .tools {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .message {
    padding: 8px 12px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    font-size: 12.5px;
  }
  .group-title {
    margin-top: 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
  }
  .group {
    border-radius: var(--radius-lg);
    background: var(--field);
    border: 1px solid var(--line);
    padding: 2px 12px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: 42px;
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row.recording {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    margin: 0 -12px;
    padding: 0 12px;
  }
  .label {
    font-size: 13px;
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
  }
  .chord {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .x {
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    border-radius: 4px;
    color: var(--text-3);
    opacity: 0;
  }
  .chord:hover .x {
    opacity: 1;
  }
  .x:hover {
    background: var(--hover);
    color: var(--text);
  }
  .add {
    width: 24px;
    height: 22px;
    display: grid;
    place-items: center;
    border-radius: 5px;
    border: 1px dashed var(--line-strong);
    color: var(--text-3);
  }
  .add:hover {
    color: var(--text);
    border-color: var(--text-3);
  }
  .listening {
    font-size: 12px;
    color: var(--accent);
    font-weight: 600;
  }
</style>
