// Shortcut hints for menus and tooltips, from the user's current bindings.
import { settings } from "../settings/settings.svelte";
import { chordLabel } from "./shortcuts";

/** The first key bound to an action, for menu hints and tooltips. */
export function shortcutHint(action: string): string | undefined {
  const key = settings.value.shortcuts[action]?.[0];
  return key && chordLabel(key);
}

/** A tooltip such as "Compact mode (Ctrl+Alt+C)": the label with its shortcut, when it has one. */
export function withHint(label: string, action: string): string {
  const key = shortcutHint(action);
  return key ? `${label} (${key})` : label;
}
