// Opening the notes editor.
import { registerActions } from "../../platform/actions";
import { webviewApi } from "../../platform/webview-api";
import { ui } from "../../shared/ui/ui.svelte";
import { openInternal } from "../browser/target";
import { notes } from "./notes.svelte";

/** Opens the notes editor (or brings its tab forward) with the keyboard in the open note;
 *  `create` starts a new note. */
export function openNotes(opts: { create?: boolean } = {}) {
  openInternal("notes");
  ui.closeMenu();
  if (ui.palette.open) ui.palette.open = false;
  // The notes page is part of the chrome UI: take the keyboard from the page webviews.
  void webviewApi.focusView(null);
  void notes.load().then(() => {
    if (opts.create || !notes.items.length) notes.create();
    else notes.focusEditor++;
  });
}

registerActions({
  openNotes: () => openNotes(),
  newNote: () => openNotes({ create: true }),
});
