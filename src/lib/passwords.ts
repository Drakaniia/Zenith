// Password manager: types shared with the backend (vault/, password_commands.rs) and pure
// helpers for the UI. Kept free of Svelte and Tauri imports so it can be unit tested.

export interface VaultStatus {
  /** A master password has been set up. */
  exists: boolean;
  unlocked: boolean;
  hasRecovery: boolean;
  count: number;
  /** The password file exists but can't be used. */
  damaged: string | null;
  /** Seconds until another master password attempt is accepted. */
  retryAfter: number;
}

export interface PasswordEntry {
  id: string;
  origin: string;
  /** Host for display, without "www.". */
  site: string;
  username: string;
  label: string;
  created: number;
  updated: number;
  lastUsed: number;
  timesUsed: number;
  hasNotes: boolean;
}

export interface EntryInput {
  url: string;
  username: string;
  /** null keeps the saved password when editing. */
  password: string | null;
  label: string;
  /** null keeps the saved notes when editing. */
  notes: string | null;
}

export interface SavedAccount {
  id: string;
  username: string;
  label: string;
  site: string;
  /** Saved for exactly this origin (not just the same site). */
  exact: boolean;
}

export interface PasswordSuggest {
  tabId: string;
  site: string;
  field: "user" | "pass";
  newPassword: boolean;
  /** The field in the page's CSS pixels. */
  rect: { x: number; y: number; w: number; h: number };
  /** The page's devicePixelRatio (includes its zoom). */
  dpr: number;
  accounts: SavedAccount[];
  generated: string | null;
  /** The list already on screen, narrowed by what was typed in the field: update it in place. */
  refresh?: boolean;
}

export interface PasswordSaveOffer {
  offerId: number;
  tabId: string;
  site: string;
  username: string;
  /** Would replace the password of an existing entry. */
  update: boolean;
}

export type ImportStatus = "new" | "duplicate" | "conflict" | "invalid";

export interface ImportPreviewRow {
  index: number;
  site: string;
  origin: string;
  username: string;
  label: string;
  status: ImportStatus;
  reason: string | null;
  existingId: string | null;
}

export interface ImportOpened {
  token: number;
  fileName: string;
  needsPassword: boolean;
  rows: ImportPreviewRow[];
}

export interface ImportSummary {
  added: number;
  replaced: number;
  skipped: number;
}

/** Backend errors that mean "ask for the master password and try again". */
export const NEED_MASTER = "Enter your master password to continue.";
export const LOCKED = "The password manager is locked.";
export const MIN_MASTER_LEN = 8;

export function needsMaster(e: unknown): boolean {
  const s = String(e);
  return s === NEED_MASTER || s === LOCKED;
}

/** Entries grouped by site (several accounts per site), sites in alphabetical order. */
export function groupBySite(entries: PasswordEntry[]): { site: string; origin: string; entries: PasswordEntry[] }[] {
  const groups = new Map<string, { site: string; origin: string; entries: PasswordEntry[] }>();
  for (const e of entries) {
    const key = e.site.toLowerCase();
    let g = groups.get(key);
    if (!g) {
      g = { site: e.site, origin: e.origin, entries: [] };
      groups.set(key, g);
    }
    g.entries.push(e);
  }
  const list = [...groups.values()];
  for (const g of list) g.entries.sort((a, b) => a.username.localeCompare(b.username));
  return list.sort((a, b) => a.site.localeCompare(b.site));
}

/** Search across site, address, username and label. Every word must match. */
export function filterEntries(entries: PasswordEntry[], query: string): PasswordEntry[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return entries;
  return entries.filter((e) => {
    const hay = `${e.site} ${e.origin} ${e.username} ${e.label}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

export type Strength = { score: 0 | 1 | 2 | 3 | 4; label: string };

/** A rough strength estimate (length and character variety, common patterns penalised). */
export function strength(pw: string): Strength {
  if (!pw) return { score: 0, label: "" };
  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/\d/.test(pw)) pool += 10;
  if (/[^a-zA-Z\d]/.test(pw)) pool += 33;
  let bits = pw.length * Math.log2(Math.max(pool, 2));
  if (/^(.)\1+$/.test(pw)) bits = Math.min(bits, 10);
  if (/(password|qwerty|123456|letmein|admin|welcome|iloveyou)/i.test(pw)) bits -= 20;
  if (/^\d+$/.test(pw)) bits = Math.min(bits, pw.length * 2.5);
  const score = bits < 28 ? 0 : bits < 40 ? 1 : bits < 60 ? 2 : bits < 80 ? 3 : 4;
  const label = ["Very weak", "Weak", "Fair", "Strong", "Very strong"][score];
  return { score: score as Strength["score"], label };
}

/** Problems with a new master password, or "" when it is acceptable. */
export function masterProblem(pw: string, confirm: string): string {
  if (pw.length < MIN_MASTER_LEN) return `Use at least ${MIN_MASTER_LEN} characters.`;
  if (strength(pw).score < 2) return "Choose something harder to guess: longer, or with more kinds of characters.";
  if (pw !== confirm) return "The two passwords don't match.";
  return "";
}

/**
 * Where the account list goes, in window CSS pixels: under the login field (above it when there
 * is no room below), inside the page's pane. `pane` is the page webview's rectangle; the field
 * rectangle is in the page's CSS pixels, scaled by the page zoom (page dpr / window dpr).
 */
export function placeSuggestions(
  field: { x: number; y: number; w: number; h: number },
  pageDpr: number,
  windowDpr: number,
  pane: { x: number; y: number; w: number; h: number },
  size: { w: number; h: number },
): { x: number; y: number; w: number; above: boolean } {
  const zoom = pageDpr > 0 && windowDpr > 0 ? pageDpr / windowDpr : 1;
  const left = pane.x + field.x * zoom;
  const top = pane.y + field.y * zoom;
  const bottom = top + field.h * zoom;
  const w = Math.min(Math.max(field.w * zoom, size.w), pane.w - 16);
  const x = Math.min(Math.max(left, pane.x + 8), pane.x + pane.w - w - 8);
  const roomBelow = pane.y + pane.h - bottom;
  const above = roomBelow < size.h + 12 && top - pane.y > roomBelow;
  const y = above ? Math.max(pane.y + 8, top - size.h - 6) : Math.min(bottom + 6, pane.y + pane.h - 8);
  return { x: Math.round(x), y: Math.round(y), w: Math.round(w), above };
}

/** Recovery keys as shown: groups of four, upper case. */
export function formatRecoveryKey(input: string): string {
  const clean = input.toUpperCase().replace(/[^0-9A-Z]/g, "");
  return clean.match(/.{1,4}/g)?.join("-") ?? "";
}
