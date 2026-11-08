// Backend storage for notes.
import { invoke } from "@tauri-apps/api/core";
import type { Note } from "./notes";

export const notesApi = {
  notesList: () => invoke<Note[]>("notes_list"),
  /** Saves a note; false when a newer copy (from another window) is already stored. */
  noteSave: (note: Note) => invoke<boolean>("note_save", { note }),
  noteDelete: (id: string) => invoke<void>("note_delete", { id }),
};

declare module "../../platform/ipc" {
  interface BackendEvents {
    "notes-changed": { origin: string; id: string; deleted: boolean };
  }
}
