// Named actions: keyboard shortcuts, menus and palette commands all run an action by its id
// (the ids of shortcuts.ts). Each feature registers the actions it owns in its own actions.ts;
// app/actions.ts loads them all at startup.

/** Runs one action. `source` is the tab a shortcut came from (null: the window's UI). */
export type ActionHandler = (source: string | null | undefined, action: string) => unknown;

const handlers = new Map<string, ActionHandler>();
const patterns: { re: RegExp; run: (match: RegExpExecArray, source?: string | null) => unknown }[] = [];
/** Actions that keep firing while their key is held down. */
const repeatable = new Set<string>();
let blocked: () => boolean = () => false;

export function registerActions(
  map: Record<string, ActionHandler>,
  opts: { repeatable?: string[] } = {},
) {
  for (const [id, run] of Object.entries(map)) {
    if (handlers.has(id)) console.warn(`action ${id} registered twice`);
    handlers.set(id, run);
  }
  for (const id of opts.repeatable ?? []) repeatable.add(id);
}

/** Families of actions such as `selectTab1`…`selectTab9`. */
export function registerActionPattern(
  re: RegExp,
  run: (match: RegExpExecArray, source?: string | null) => unknown,
) {
  patterns.push({ re, run });
}

/** While `when` is true no action runs (the shortcut editor is recording a key). */
export function blockActionsWhile(when: () => boolean) {
  blocked = when;
}

export async function runAction(
  action: string,
  source?: string | null,
  repeat = false,
) {
  if (repeat && !repeatable.has(action)) return;
  if (blocked()) return;
  const run = handlers.get(action);
  if (run) return run(source, action);
  for (const p of patterns) {
    const m = p.re.exec(action);
    if (m) return p.run(m, source);
  }
}
