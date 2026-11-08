//! Commands for the notes editor. Notes are shared by every window, incognito ones included
//! (notes are the user's own documents, not browsing data). Other windows hear about changes
//! through `notes-changed`.

use serde::Serialize;
use tauri::WebviewWindow;

use super::Note;
use crate::app::command::CmdResult;
use crate::host::{self, bookmarks_db};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NotesChanged {
    /// Window that made the change; it already has the new state.
    origin: String,
    id: String,
    deleted: bool,
}

#[tauri::command]
pub async fn notes_list() -> CmdResult<Vec<Note>> {
    bookmarks_db().call(super::notes_list).await
}

#[tauri::command]
pub async fn note_save(window: WebviewWindow, note: Note) -> CmdResult<bool> {
    if note.id.is_empty() || note.id.len() > 64 {
        return Err("invalid note id".into());
    }
    let id = note.id.clone();
    let written = bookmarks_db().call(move |c| super::note_save(c, &note)).await?;
    if written {
        host::emit("notes-changed", NotesChanged { origin: window.label().to_string(), id, deleted: false });
    }
    Ok(written)
}

#[tauri::command]
pub async fn note_delete(window: WebviewWindow, id: String) -> CmdResult<()> {
    let key = id.clone();
    bookmarks_db().call(move |c| super::note_delete(c, &key)).await?;
    host::emit("notes-changed", NotesChanged { origin: window.label().to_string(), id, deleted: true });
    Ok(())
}
