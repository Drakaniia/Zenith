// Browser actions and their default keyboard shortcuts. The backend receives the same
// strings ("Ctrl+Shift+T") and matches them in WebView2's AcceleratorKeyPressed event, so
// shortcuts work while a web page has focus.
//
// The defaults follow Zen's keyboard shortcuts (docs.zen-browser.app/user-manual/shortcuts),
// with Firefox's alternatives where Zen keeps them.

/** Sections of the shortcut editor, in the order they are shown. */
export const GROUPS = [
  "Tabs",
  "Windows",
  "Navigation",
  "Search",
  "Page",
  "Zoom & media",
  "View",
  "Compact mode",
  "Split view",
  "Spaces",
  "Bookmarks & history",
  "Developer tools",
  "Browser",
] as const;
export type ActionGroup = (typeof GROUPS)[number];

export interface ActionDef {
  id: string;
  label: string;
  group: ActionGroup;
  keys: string[];
}

const tabNumbers: ActionDef[] = Array.from({ length: 8 }, (_, i) => ({
  id: `selectTab${i + 1}`,
  label: `Go to tab ${i + 1}`,
  group: "Tabs" as const,
  keys: [`Ctrl+${i + 1}`, `Alt+${i + 1}`],
}));

/** Zen: "Switch to Workspace (1-10)", not set by default. */
const spaceNumbers: ActionDef[] = Array.from({ length: 10 }, (_, i) => ({
  id: `switchSpace${i + 1}`,
  label: `Switch to space ${i + 1}`,
  group: "Spaces" as const,
  keys: [],
}));

const a = (id: string, label: string, group: ActionGroup, keys: string[] = []): ActionDef => ({
  id,
  label,
  group,
  keys,
});

export const ACTIONS: ActionDef[] = [
  a("newTab", "New tab", "Tabs", ["Ctrl+T"]),
  a("closeTab", "Close tab", "Tabs", ["Ctrl+W", "Ctrl+F4"]),
  a("reopenClosed", "Reopen closed tab", "Tabs", ["Ctrl+Shift+T"]),
  a("nextTab", "Next tab", "Tabs", ["Ctrl+Tab", "Ctrl+PageDown"]),
  a("prevTab", "Previous tab", "Tabs", ["Ctrl+Shift+Tab", "Ctrl+PageUp"]),
  ...tabNumbers,
  a("lastTab", "Go to last tab", "Tabs", ["Ctrl+9", "Alt+9"]),
  a("showAllTabs", "Show all tabs", "Tabs"),
  a("duplicateTab", "Duplicate tab", "Tabs"),
  a("pinTab", "Pin or unpin tab", "Tabs"),
  a("resetPinned", "Reset pinned tab to its pinned address", "Tabs"),
  a("sleepTab", "Put tab to sleep", "Tabs"),

  a("newWindow", "New window", "Windows", ["Ctrl+N"]),
  a("newIncognitoWindow", "New incognito window", "Windows", ["Ctrl+Shift+P"]),
  a("closeWindow", "Close window", "Windows", ["Ctrl+Shift+W"]),
  a("undoCloseWindow", "Reopen closed window", "Windows", ["Ctrl+Shift+N"]),
  a("quit", "Quit Zenith", "Windows", ["Ctrl+Q", "Ctrl+Shift+Q"]),

  a("openPalette", "Open address bar", "Navigation", ["Ctrl+L", "Alt+D", "F6"]),
  a("back", "Back", "Navigation", ["Alt+Left", "Ctrl+BracketLeft"]),
  a("forward", "Forward", "Navigation", ["Alt+Right", "Ctrl+BracketRight"]),
  a("goHome", "Go to the home page", "Navigation", ["Alt+Home"]),
  a("reload", "Reload", "Navigation", ["F5", "Ctrl+R"]),
  a("hardReload", "Reload without cache", "Navigation", ["Ctrl+F5", "Ctrl+Shift+R"]),
  // Esc also stops a page that is still loading (built in, like Firefox).
  a("stop", "Stop loading", "Navigation"),
  a("copyUrl", "Copy page address", "Navigation", ["Ctrl+Shift+C"]),
  a("copyUrlMarkdown", "Copy page address as Markdown", "Navigation", ["Ctrl+Alt+Shift+C"]),

  a("focusSearch", "Search the web", "Search", ["Ctrl+K", "Ctrl+J"]),
  a("commandPalette", "Command palette", "Search", ["Ctrl+Alt+P"]),
  a("find", "Find in page", "Search", ["Ctrl+F"]),
  a("findNext", "Find next", "Search", ["F3", "Ctrl+G"]),
  a("findPrev", "Find previous", "Search", ["Shift+F3", "Ctrl+Shift+G"]),

  a("savePage", "Save page as", "Page", ["Ctrl+S"]),
  a("print", "Print", "Page", ["Ctrl+P"]),
  a("readerMode", "Reader view", "Page", ["Ctrl+Alt+R"]),
  a("pictureInPicture", "Picture-in-picture", "Page", ["Ctrl+Shift+BracketRight"]),
  a("viewSource", "View page source", "Page", ["Ctrl+U"]),
  a("pageInfo", "Page info", "Page", ["Ctrl+I"]),
  a("screenshot", "Take a screenshot", "Page", ["Ctrl+Shift+S"]),
  a("switchTextDirection", "Switch text direction", "Page", ["Ctrl+Shift+X"]),
  a("openFile", "Open file", "Page", ["Ctrl+O"]),
  a("downloadMedia", "Download video on this page", "Page"),

  a("muteTab", "Mute or unmute tab", "Zoom & media", ["Ctrl+M"]),
  a("zoomIn", "Zoom in", "Zoom & media", ["Ctrl+Plus", "Ctrl+NumpadAdd"]),
  a("zoomOut", "Zoom out", "Zoom & media", ["Ctrl+Minus", "Ctrl+NumpadSubtract"]),
  a("zoomReset", "Reset zoom", "Zoom & media", ["Ctrl+0", "Ctrl+Numpad0"]),

  a("toggleSidebar", "Expand or collapse the sidebar", "View", ["Alt+B"]),
  a("toggleBookmarksSidebar", "Show bookmarks sidebar", "View", ["Ctrl+B"]),
  a("toggleSidebarPanel", "Show or hide the sidebar panel", "View", ["Ctrl+Alt+Z"]),
  a("toggleBookmarksBar", "Show bookmarks toolbar", "View", ["Ctrl+Shift+B"]),
  a("toggleChatbot", "AI chatbot", "View", ["Ctrl+Alt+X"]),
  a("fullscreen", "Full screen", "View", ["F11"]),

  a("toggleCompact", "Toggle compact mode", "Compact mode", ["Ctrl+Alt+C"]),
  a("toggleFloatingSidebar", "Show or hide the floating sidebar", "Compact mode", ["Ctrl+Alt+S"]),
  a("toggleFloatingToolbar", "Show or hide the floating toolbar", "Compact mode", ["Ctrl+Alt+T"]),

  a("splitVertical", "Split view side by side", "Split view", ["Ctrl+Alt+V"]),
  a("splitHorizontal", "Split view stacked", "Split view", ["Ctrl+Alt+H"]),
  a("splitGrid", "Split view grid", "Split view", ["Ctrl+Alt+G"]),
  a("unsplit", "Close split view", "Split view", ["Ctrl+Alt+U"]),
  a("toggleSplit", "Split view with next tab", "Split view"),

  a("nextSpace", "Next space", "Spaces", ["Ctrl+Alt+E", "Ctrl+Alt+Right"]),
  a("prevSpace", "Previous space", "Spaces", ["Ctrl+Alt+Q", "Ctrl+Alt+Left"]),
  ...spaceNumbers,
  a("newSpace", "New space", "Spaces"),

  a("bookmark", "Bookmark this page", "Bookmarks & history", ["Ctrl+D", "Ctrl+Shift+D"]),
  a("bookmarkAllTabs", "Bookmark all tabs", "Bookmarks & history"),
  a("bookmarks", "Show bookmarks library", "Bookmarks & history", ["Ctrl+Shift+O"]),
  a("historySidebar", "History sidebar", "Bookmarks & history", ["Ctrl+H"]),
  a("history", "Show all history", "Bookmarks & history", ["Ctrl+Shift+H"]),

  a("devtools", "Developer tools", "Developer tools", ["F12", "Ctrl+Shift+I"]),
  a("inspector", "Inspector", "Developer tools", ["Ctrl+Shift+L"]),
  a("webConsole", "Web console", "Developer tools", ["Ctrl+Shift+K"]),
  // Firefox's Ctrl+Shift+Z is redo in pages, and Zen's Ctrl+Shift+W closes the window.
  a("jsDebugger", "JavaScript debugger", "Developer tools"),
  a("networkMonitor", "Network monitor", "Developer tools", ["Ctrl+Shift+E"]),
  a("styleEditor", "Style editor", "Developer tools", ["Shift+F7"]),
  a("performance", "Performance", "Developer tools", ["Shift+F5"]),
  a("storage", "Storage", "Developer tools", ["Shift+F9"]),
  a("domPanel", "DOM", "Developer tools"),
  a("accessibility", "Accessibility", "Developer tools", ["Shift+F12"]),
  a("responsiveDesign", "Responsive design mode", "Developer tools", ["Ctrl+Shift+M"]),
  a("browserToolbox", "Browser toolbox", "Developer tools", ["Ctrl+Alt+Shift+I"]),
  a("browserConsole", "Browser console", "Developer tools", ["Ctrl+Shift+J"]),
  a("taskManager", "Task manager", "Developer tools", ["Shift+Escape"]),

  a("downloads", "Downloads", "Browser", ["Ctrl+Shift+Y"]),
  a("extensions", "Extensions", "Browser", ["Ctrl+Shift+A"]),
  a("passwords", "Passwords", "Browser"),
  a("openNotes", "Open notes", "Browser", ["Ctrl+Alt+N"]),
  a("settings", "Settings", "Browser", ["Ctrl+Comma"]),
  a("clearData", "Clear browsing data", "Browser", ["Ctrl+Shift+Delete"]),
  a("runSetup", "Run setup again", "Browser"),
];

export const ACTION_LABELS: Record<string, string> = Object.fromEntries(
  ACTIONS.map((a) => [a.id, a.label]),
);

export function defaultShortcuts(): Record<string, string[]> {
  return Object.fromEntries(ACTIONS.map((a) => [a.id, [...a.keys]]));
}

/** Earlier versions' defaults for actions whose defaults changed (mostly to Zen's keys). A
 *  saved binding that is still one of these follows to the new default; one the user changed
 *  stays as it is. */
const OLD_DEFAULTS: Record<string, readonly (readonly string[])[]> = {
  toggleCompact: [["Ctrl+B"]],
  toggleSidebar: [["Ctrl+Shift+B"]],
  toggleSplit: [["Ctrl+Alt+S"]],
  nextSpace: [["Ctrl+Alt+Right"]],
  prevSpace: [["Ctrl+Alt+Left"]],
  ...Object.fromEntries(
    Array.from({ length: 8 }, (_, i) => [`selectTab${i + 1}`, [[`Ctrl+${i + 1}`]]]),
  ),
  lastTab: [["Ctrl+9"]],
  back: [["Alt+Left"]],
  forward: [["Alt+Right"]],
  commandPalette: [["Ctrl+K", "Ctrl+Shift+P"]],
  newIncognitoWindow: [["Ctrl+Shift+N"]],
  bookmark: [["Ctrl+D"]],
  bookmarkAllTabs: [["Ctrl+Shift+D"]],
  history: [["Ctrl+H"]],
  downloads: [["Ctrl+J"]],
  extensions: [[]],
  quit: [["Ctrl+Shift+Q"]],
};

const sameKeys = (a: readonly string[], b: readonly string[]) =>
  a.length === b.length && a.every((k, i) => k === b[i]);

/** The bindings to use: the user's saved ones over the defaults. A default the user never had
 *  (a new action, or one that moved to another key) doesn't take a key that one of the user's
 *  own bindings already uses. Bindings for actions this version doesn't know are kept. */
export function mergeShortcuts(
  saved: Record<string, string[]> | undefined,
  defaults: Record<string, string[]> = defaultShortcuts(),
): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  const used = new Set<string>();
  const fresh: string[] = [];
  for (const action of Object.keys(defaults)) {
    const mine = saved?.[action];
    const old = OLD_DEFAULTS[action] ?? [];
    if (Array.isArray(mine) && !old.some((o) => sameKeys(mine, o))) {
      out[action] = [...mine];
      for (const k of mine) used.add(k);
    } else fresh.push(action);
  }
  for (const action of fresh) out[action] = defaults[action].filter((k) => !used.has(k));
  for (const [action, keys] of Object.entries(saved ?? {})) {
    if (!(action in out) && Array.isArray(keys)) out[action] = [...keys];
  }
  return out;
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

/** Mirrors the backend's reserved-key rule (keys.rs `is_reserved`): editing keys always go to
 *  the page. */
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
  // Ctrl+Shift+Delete is the browsers' "clear browsing data" key, not a text edit.
  const clearData = ctrl && !alt && shift && key === "Delete";
  if (
    ctrl &&
    !alt &&
    !clearData &&
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
  // Shift+Esc opens the task manager, as in Firefox and Chromium.
  const taskManager = !ctrl && !alt && shift && key === "Escape";
  if (!ctrl && !alt && !fn && !taskManager) return "Add Ctrl or Alt (or use a function key)";
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
  Delete: "Del",
};

/** Key names spelled as symbols in one-line labels (menus, tooltips): "Ctrl+Shift+]". Plus,
 *  Minus and the arrows keep their names, which read better joined with "+". */
const LABEL: Record<string, string> = {
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Quote: "'",
  Semicolon: ";",
  Backquote: "`",
  PageUp: "PgUp",
  PageDown: "PgDn",
  Escape: "Esc",
  Delete: "Del",
};

export function chordLabel(chord: string): string {
  return chord
    .split("+")
    .map((p) => LABEL[p] ?? p.replace(/^Numpad/, "Num "))
    .join("+");
}

export function displayChord(chord: string): string[] {
  return chord
    .split("+")
    .map((p) => DISPLAY[p] ?? p.replace(/^Numpad/, "Num "));
}
