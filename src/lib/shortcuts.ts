// Browser actions and their default keyboard shortcuts. The backend receives the same
// strings ("Ctrl+Shift+T") and matches them in WebView2's AcceleratorKeyPressed event, so
// shortcuts work while a web page has focus.

export interface ActionDef {
  id: string;
  label: string;
  group: "Tabs" | "Navigation" | "Page" | "View" | "Spaces" | "Browser";
  keys: string[];
}

const tabNumbers: ActionDef[] = Array.from({ length: 8 }, (_, i) => ({
  id: `selectTab${i + 1}`,
  label: `Go to tab ${i + 1}`,
  group: "Tabs" as const,
  keys: [`Ctrl+${i + 1}`],
}));

export const ACTIONS: ActionDef[] = [
  { id: "newTab", label: "New tab", group: "Tabs", keys: ["Ctrl+T"] },
  {
    id: "closeTab",
    label: "Close tab",
    group: "Tabs",
    keys: ["Ctrl+W", "Ctrl+F4"],
  },
  {
    id: "reopenClosed",
    label: "Reopen closed tab",
    group: "Tabs",
    keys: ["Ctrl+Shift+T"],
  },
  {
    id: "nextTab",
    label: "Next tab",
    group: "Tabs",
    keys: ["Ctrl+Tab", "Ctrl+PageDown"],
  },
  {
    id: "prevTab",
    label: "Previous tab",
    group: "Tabs",
    keys: ["Ctrl+Shift+Tab", "Ctrl+PageUp"],
  },
  ...tabNumbers,
  { id: "lastTab", label: "Go to last tab", group: "Tabs", keys: ["Ctrl+9"] },
  { id: "duplicateTab", label: "Duplicate tab", group: "Tabs", keys: [] },
  { id: "pinTab", label: "Pin or unpin tab", group: "Tabs", keys: [] },
  {
    id: "muteTab",
    label: "Mute or unmute tab",
    group: "Tabs",
    keys: ["Ctrl+M"],
  },
  { id: "sleepTab", label: "Put tab to sleep", group: "Tabs", keys: [] },

  {
    id: "openPalette",
    label: "Open address bar",
    group: "Navigation",
    keys: ["Ctrl+L", "Alt+D", "F6"],
  },
  {
    id: "commandPalette",
    label: "Command palette",
    group: "Navigation",
    keys: ["Ctrl+K", "Ctrl+Shift+P"],
  },
  { id: "back", label: "Back", group: "Navigation", keys: ["Alt+Left"] },
  { id: "forward", label: "Forward", group: "Navigation", keys: ["Alt+Right"] },
  {
    id: "reload",
    label: "Reload",
    group: "Navigation",
    keys: ["F5", "Ctrl+R"],
  },
  {
    id: "hardReload",
    label: "Reload without cache",
    group: "Navigation",
    keys: ["Ctrl+F5", "Ctrl+Shift+R"],
  },
  {
    id: "copyUrl",
    label: "Copy page address",
    group: "Navigation",
    keys: ["Ctrl+Shift+C"],
  },

  { id: "find", label: "Find in page", group: "Page", keys: ["Ctrl+F"] },
  { id: "findNext", label: "Find next", group: "Page", keys: ["F3", "Ctrl+G"] },
  {
    id: "findPrev",
    label: "Find previous",
    group: "Page",
    keys: ["Shift+F3", "Ctrl+Shift+G"],
  },
  {
    id: "zoomIn",
    label: "Zoom in",
    group: "Page",
    keys: ["Ctrl+Plus", "Ctrl+NumpadAdd"],
  },
  {
    id: "zoomOut",
    label: "Zoom out",
    group: "Page",
    keys: ["Ctrl+Minus", "Ctrl+NumpadSubtract"],
  },
  {
    id: "zoomReset",
    label: "Reset zoom",
    group: "Page",
    keys: ["Ctrl+0", "Ctrl+Numpad0"],
  },
  { id: "print", label: "Print", group: "Page", keys: ["Ctrl+P"] },
  { id: "savePage", label: "Save page as", group: "Page", keys: ["Ctrl+S"] },
  {
    id: "downloadMedia",
    label: "Download video on this page",
    group: "Page",
    keys: [],
  },
  {
    id: "viewSource",
    label: "View page source",
    group: "Page",
    keys: ["Ctrl+U"],
  },
  {
    id: "devtools",
    label: "Developer tools",
    group: "Page",
    keys: ["F12", "Ctrl+Shift+I"],
  },

  {
    id: "toggleCompact",
    label: "Toggle compact mode",
    group: "View",
    keys: ["Ctrl+B"],
  },
  {
    id: "toggleSidebar",
    label: "Expand or collapse the sidebar",
    group: "View",
    keys: ["Ctrl+Shift+B"],
  },
  {
    id: "toggleSplit",
    label: "Split view with next tab",
    group: "View",
    keys: ["Ctrl+Alt+S"],
  },
  { id: "fullscreen", label: "Full screen", group: "View", keys: ["F11"] },

  {
    id: "nextSpace",
    label: "Next space",
    group: "Spaces",
    keys: ["Ctrl+Alt+Right"],
  },
  {
    id: "prevSpace",
    label: "Previous space",
    group: "Spaces",
    keys: ["Ctrl+Alt+Left"],
  },
  { id: "newSpace", label: "New space", group: "Spaces", keys: [] },

  {
    id: "newIncognitoWindow",
    label: "New incognito window",
    group: "Browser",
    keys: ["Ctrl+Shift+N"],
  },
  {
    id: "bookmark",
    label: "Bookmark this page",
    group: "Browser",
    keys: ["Ctrl+D"],
  },
  { id: "history", label: "History", group: "Browser", keys: ["Ctrl+H"] },
  {
    id: "bookmarks",
    label: "Bookmarks",
    group: "Browser",
    keys: ["Ctrl+Shift+O"],
  },
  { id: "downloads", label: "Downloads", group: "Browser", keys: ["Ctrl+J"] },
  { id: "extensions", label: "Extensions", group: "Browser", keys: [] },
  { id: "settings", label: "Settings", group: "Browser", keys: ["Ctrl+Comma"] },
  {
    id: "quit",
    label: "Quit Zenith",
    group: "Browser",
    keys: ["Ctrl+Shift+Q"],
  },
];

export const ACTION_LABELS: Record<string, string> = Object.fromEntries(
  ACTIONS.map((a) => [a.id, a.label]),
);

export function defaultShortcuts(): Record<string, string[]> {
  return Object.fromEntries(ACTIONS.map((a) => [a.id, [...a.keys]]));
}

const CODE_NAMES: Record<string, string> = {
  Equal: "Plus",
  Minus: "Minus",
  Comma: "Comma",
  Period: "Period",
  Slash: "Slash",
  Backslash: "Backslash",
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
  Quote: "Quote",
  Semicolon: "Semicolon",
  Backquote: "Backquote",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  ArrowUp: "Up",
  ArrowDown: "Down",
  Space: "Space",
  Tab: "Tab",
  Enter: "Enter",
  Escape: "Escape",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Home: "Home",
  End: "End",
  Insert: "Insert",
  Delete: "Delete",
  Backspace: "Backspace",
  NumpadAdd: "NumpadAdd",
  NumpadSubtract: "NumpadSubtract",
  NumpadMultiply: "NumpadMultiply",
  NumpadDivide: "NumpadDivide",
  NumpadDecimal: "NumpadDecimal",
};

/** Converts a keydown event into the chord string format, or null for bare modifiers. */
export function chordFromEvent(e: KeyboardEvent): string | null {
  const code = e.code;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) key = code;
  else if (/^Numpad[0-9]$/.test(code)) key = code;
  else key = CODE_NAMES[code] ?? null;
  if (!key) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

/** Mirrors the backend's reserved-key rule: editing keys always go to the page. */
export function isReserved(chord: string): string | null {
  const parts = chord.split("+");
  const key = parts[parts.length - 1];
  const ctrl = parts.includes("Ctrl");
  const alt = parts.includes("Alt");
  const shift = parts.includes("Shift");
  if (ctrl && !alt && !shift && ["C", "V", "X", "A", "Z", "Y"].includes(key))
    return "Reserved for editing";
  if (ctrl && !alt && shift && ["Z", "V"].includes(key))
    return "Reserved for editing";
  if (
    ctrl &&
    !alt &&
    [
      "Backspace",
      "Delete",
      "Left",
      "Right",
      "Up",
      "Down",
      "Home",
      "End",
    ].includes(key)
  )
    return "Reserved for text navigation";
  const fn = /^F([1-9]|1[0-9]|2[0-4])$/.test(key);
  if (!ctrl && !alt && !fn) return "Add Ctrl or Alt (or use a function key)";
  return null;
}

const DISPLAY: Record<string, string> = {
  Plus: "+",
  Minus: "−",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Quote: "'",
  Semicolon: ";",
  Backquote: "`",
  Left: "←",
  Right: "→",
  Up: "↑",
  Down: "↓",
  PageUp: "PgUp",
  PageDown: "PgDn",
  NumpadAdd: "Num +",
  NumpadSubtract: "Num −",
  Escape: "Esc",
};

export function displayChord(chord: string): string[] {
  return chord
    .split("+")
    .map((p) => DISPLAY[p] ?? p.replace(/^Numpad/, "Num "));
}
