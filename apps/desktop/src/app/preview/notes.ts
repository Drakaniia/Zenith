import { UNHANDLED } from "./handler";

// Notes for the notes editor (zenith://notes), kept in memory.
const HOUR = 3_600_000;
const previewNotes = [
  {
    id: "n1",
    title: "Shell redesign",
    body: "Sidebar\n- Essentials grid: 4 per row\n- Space switcher in the footer\n\nToolbar\n- Single toolbar by default\n- Compact mode on Ctrl+Alt+C",
    created: Date.now() - 50 * HOUR,
    updated: Date.now() - 0.2 * HOUR,
  },
  {
    id: "n2",
    title: "",
    body: "Groceries\nmilk, eggs, coffee beans\nbasil for Friday",
    created: Date.now() - 30 * HOUR,
    updated: Date.now() - 26 * HOUR,
  },
  {
    id: "n3",
    title: "Reading list",
    body: "On squircles — corner-shape in CSS\nThe Zen Browser source tour",
    created: Date.now() - 400 * HOUR,
    updated: Date.now() - 120 * HOUR,
  },
];

/** Notes editor commands. */
export function handleNotes(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "notes_list":
      return previewNotes.map((n) => ({ ...n }));
    case "note_save": {
      const note = a.note as (typeof previewNotes)[number];
      const i = previewNotes.findIndex((n) => n.id === note.id);
      if (i >= 0) previewNotes[i] = { ...note };
      else previewNotes.push({ ...note });
      return true;
    }
    case "note_delete": {
      const i = previewNotes.findIndex((n) => n.id === a.id);
      if (i >= 0) previewNotes.splice(i, 1);
      return null;
    }
    default:
      return UNHANDLED;
  }
}
