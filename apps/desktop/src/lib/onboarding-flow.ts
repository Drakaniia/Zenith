// The setup assistant's steps and rules, kept free of Svelte state so they can be unit-tested.
// The UI (components/onboarding) and its store (onboarding.svelte.ts) are built on these.
//
// Adding a step: add it to STEPS, list the settings it owns in STEP_FIELDS (restored when the
// step is skipped), and register its component in Onboarding.svelte.

export interface StepDef {
  id: string;
  /** Short name, for the progress arc and screen readers. */
  label: string;
  /** Can be skipped; skipping leaves its settings as they were when the step opened. */
  optional: boolean;
}

export const STEPS = [
  { id: "welcome", label: "Welcome", optional: false },
  { id: "appearance", label: "Appearance", optional: true },
  { id: "search", label: "Search", optional: false },
  { id: "start", label: "Start page", optional: true },
  { id: "import", label: "Import", optional: true },
  { id: "privacy", label: "Privacy", optional: true },
  { id: "ready", label: "Ready", optional: false },
] as const satisfies readonly StepDef[];

export type StepId = (typeof STEPS)[number]["id"];

/** "accent" is the colour of the first space rather than a setting. */
export const STEP_FIELDS: Record<StepId, readonly string[]> = {
  welcome: [],
  appearance: ["theme", "toolbarMode", "accent"],
  search: ["searchEngine", "searchSuggestions"],
  start: ["restoreSession", "homepage"],
  import: [],
  privacy: ["blockerEnabled", "blockPopups", "recordHistory", "historyDays", "autoUpdate"],
  ready: [],
};

/** Names of the space colours (browser.svelte.ts SPACE_COLORS), for labels and the summary. */
export const ACCENT_NAMES: Record<string, string> = {
  "#8b7cf6": "Violet",
  "#5b8def": "Blue",
  "#2bb5a8": "Teal",
  "#4fb86a": "Green",
  "#e0a83a": "Amber",
  "#ec7f43": "Orange",
  "#e8607a": "Rose",
  "#d36ad6": "Orchid",
  "#8f9bb3": "Slate",
};

export function stepIndex(id: string | undefined | null): number {
  return STEPS.findIndex((s) => s.id === id);
}

/** Clamps a step index into range. */
export function clampStep(i: number): number {
  return Math.max(0, Math.min(STEPS.length - 1, Math.round(i)));
}

/** The sun's altitude on the progress arc: 0° at the first step, 90° (the zenith) at the last. */
export function altitude(index: number): number {
  return (clampStep(index) / (STEPS.length - 1)) * 90;
}

/** Point on the arc for an altitude, in the arc's own box: the horizon runs along y = 1 from
 *  x = 0 to x = 1 and the zenith is at (0.5, 0). The sun rises from the left edge. */
export function arcPoint(deg: number): { x: number; y: number } {
  const a = ((180 - deg) * Math.PI) / 180;
  return { x: 0.5 + 0.5 * Math.cos(a), y: 1 - Math.sin(a) };
}

/** Whether the setup assistant opens when a window starts. Incognito windows never show it:
 *  nothing they change is saved. */
export function shouldOnboard(s: { onboarded?: boolean }, incognito: boolean): boolean {
  return !incognito && !s.onboarded;
}

export type HomepageResult = { ok: true; url: string } | { ok: false; error: string };

/** Validates a home page typed by the user. Empty means Zenith's own start page. A bare host
 *  gets https://; only http and https pages are accepted. */
export function normalizeHomepage(raw: string): HomepageResult {
  const input = raw.trim();
  if (!input) return { ok: true, url: "" };
  const withScheme = /^[a-z][a-z0-9+.-]*:\/\//i.test(input) ? input : `https://${input}`;
  try {
    const u = new URL(withScheme);
    const host = u.hostname;
    const hostOk =
      host === "localhost" || /^\d{1,3}(\.\d{1,3}){3}$/.test(host) || /^[^.\s]+(\.[^.\s]+)+$/.test(host);
    if ((u.protocol === "https:" || u.protocol === "http:") && hostOk) return { ok: true, url: u.href };
  } catch {
    /* invalid below */
  }
  return { ok: false, error: "Enter a web address, like example.com" };
}

/** A count with its noun: "1 bookmark", "1,204 bookmarks". */
export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString("en-US")} ${n === 1 ? one : many}`;
}
