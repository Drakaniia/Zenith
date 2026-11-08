// The notes editor's state (zenith://notes). It lives outside the page component, so the open
// note, the search and unsaved edits survive tab switches and the page being closed and
// reopened. Edits are saved shortly after typing stops, and before switching notes or closing
// the window. Other windows' changes arrive through `notes-changed`.
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api, on } from "$platform/ipc";
import { filterNotes, isEmptyNote, newNoteId, type Note } from "./notes";

const SAVE_DELAY = 500;

class NotesStore {
  items = $state<Note[]>([]);
  loaded = $state(false);
  activeId = $state<string | null>(null);
  query = $state("");
  /** Unsaved notes (edits waiting for the save timer, or a save that failed). */
  unsaved = $state(0);
  saveError = $state("");
  /** Bumped to ask the page to focus the editor or the search field. */
  focusEditor = $state(0);
  focusSearch = $state(0);

  active = $derived(this.items.find((n) => n.id === this.activeId) ?? null);
  visible = $derived(filterNotes(this.items, this.query));

  #dirty = new Set<string>();
  #timer: ReturnType<typeof setTimeout> | undefined;
  #loading: Promise<void> | null = null;
  #saving: Promise<void> = Promise.resolve();

  /** Loads the notes once per window (later calls wait for the same load). */
  load(): Promise<void> {
    this.#loading ??= this.#init();
    return this.#loading;
  }

  async #init() {
    const self = getCurrentWebviewWindow().label;
    void on("notes-changed", (e) => {
      if (e.origin === self) return;
      if (e.deleted && !this.#dirty.has(e.id)) {
        this.items = this.items.filter((n) => n.id !== e.id);
        if (this.activeId === e.id) this.activeId = this.visible[0]?.id ?? null;
      } else void this.#refresh();
    });
    await this.#refresh();
    this.loaded = true;
    if (!this.activeId) this.activeId = this.visible[0]?.id ?? null;
  }

  async #refresh() {
    let list: Note[];
    try {
      list = await api.notesList();
    } catch (e) {
      this.saveError = `Notes could not be loaded: ${e}`;
      return;
    }
    // Local edits not saved yet win over the stored copies.
    const local = new Map(this.items.map((n) => [n.id, n]));
    const merged = list.map((n) => (this.#dirty.has(n.id) ? local.get(n.id)! : n));
    // So do notes never stored: unsaved edits, and the new (still empty) note being written.
    const keep = new Set(this.#dirty);
    const open = this.activeId ? local.get(this.activeId) : undefined;
    if (open && isEmptyNote(open)) keep.add(open.id);
    for (const id of keep) {
      if (!list.some((n) => n.id === id) && local.has(id)) merged.push(local.get(id)!);
    }
    this.items = merged;
    if (this.activeId && !merged.some((n) => n.id === this.activeId)) {
      this.activeId = this.visible[0]?.id ?? null;
    }
  }

  /** Creates an empty note and opens it. */
  create(): string {
    this.#dropIfEmpty(this.activeId);
    const now = Date.now();
    const note: Note = { id: newNoteId(), title: "", body: "", created: now, updated: now };
    this.items = [note, ...this.items];
    this.activeId = note.id;
    this.query = "";
    this.focusEditor++;
    return note.id;
  }

  /** Opens another note, saving the current one first. */
  select(id: string) {
    if (id === this.activeId) return;
    this.#dropIfEmpty(this.activeId);
    void this.flush();
    this.activeId = id;
  }

  edit(id: string, patch: Partial<Pick<Note, "title" | "body">>) {
    const note = this.items.find((n) => n.id === id);
    if (!note) return;
    if ((patch.title ?? note.title) === note.title && (patch.body ?? note.body) === note.body) return;
    Object.assign(note, patch);
    note.updated = Date.now();
    this.#markDirty(id);
  }

  rename(id: string, title: string) {
    this.edit(id, { title: title.trim() });
  }

  /** Deletes a note and returns it (for "Undo"). */
  async remove(id: string): Promise<Note | null> {
    const i = this.items.findIndex((n) => n.id === id);
    if (i < 0) return null;
    const note = $state.snapshot(this.items[i]) as Note;
    if (this.activeId === id) {
      const order = this.visible;
      const at = order.findIndex((n) => n.id === id);
      this.activeId = (order[at + 1] ?? order[at - 1])?.id ?? null;
    }
    this.items = this.items.filter((n) => n.id !== id);
    this.#dirty.delete(id);
    this.unsaved = this.#dirty.size;
    try {
      await api.noteDelete(id);
    } catch (e) {
      this.saveError = `The note could not be deleted: ${e}`;
    }
    return note;
  }

  /** Brings back a deleted note. */
  restore(note: Note) {
    if (this.items.some((n) => n.id === note.id)) return;
    this.items = [note, ...this.items];
    this.activeId = note.id;
    this.#markDirty(note.id, true);
  }

  /** Saves every unsaved note now. */
  flush(): Promise<void> {
    clearTimeout(this.#timer);
    const ids = [...this.#dirty];
    this.#dirty.clear();
    // Saves run one after another, so an older copy never lands after a newer one.
    this.#saving = this.#saving.then(async () => {
      for (const id of ids) {
        const note = this.items.find((n) => n.id === id);
        if (!note) continue;
        try {
          await api.noteSave($state.snapshot(note) as Note);
          this.saveError = "";
        } catch (e) {
          this.#dirty.add(id);
          this.saveError = `Not saved: ${e}`;
        }
      }
      this.unsaved = this.#dirty.size;
    });
    return this.#saving;
  }

  #markDirty(id: string, now = false) {
    this.#dirty.add(id);
    this.unsaved = this.#dirty.size;
    clearTimeout(this.#timer);
    if (now) void this.flush();
    else this.#timer = setTimeout(() => void this.flush(), SAVE_DELAY);
  }

  /** A note left empty is not worth keeping: leaving it discards it. */
  #dropIfEmpty(id: string | null) {
    const note = id ? this.items.find((n) => n.id === id) : undefined;
    if (note && isEmptyNote(note)) void this.remove(note.id);
  }
}

export const notes = new NotesStore();
