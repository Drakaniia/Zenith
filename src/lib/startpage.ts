// Start page (new tab) preferences: what is shown, how big the site tiles are, and the background.
// Pure functions only, so they can be tested without the UI. The live store is startpage.svelte.ts.

export type Tone = "light" | "dark";
export type TileSize = "small" | "medium" | "large";
export type StartAlign = "center" | "top";
export type SearchWidth = "compact" | "wide";

export interface CustomWallpaper {
  id: string;
  /** Brightness of the middle of the picture (see wallpapers.ts). */
  tone: Tone;
}

export interface StartPageSettings {
  showGreeting: boolean;
  showSearch: boolean;
  /** The site tiles: Essentials and most visited sites. */
  showTiles: boolean;
  showEssentials: boolean;
  tileLabels: boolean;
  tileSize: TileSize;
  /** Most tiles shown at once. */
  tileCount: number;
  showHints: boolean;
  /** Holding the pointer over the settings button for 3 s clears the page. */
  hoverToHide: boolean;
  align: StartAlign;
  searchWidth: SearchWidth;
  /** "" (none), "gallery:<id>" or "custom:<id>". */
  background: string;
  customImages: CustomWallpaper[];
  /** Switch to another background once a day. */
  dailyBackground: boolean;
  /** Local day (YYYY-MM-DD) the background was last chosen. */
  dailyDate: string;
  /** Darkening over the background, 0-70 %. */
  dim: number;
  /** Background blur, 0-20 px. */
  blur: number;
}

export const TILE_COUNTS = [4, 8, 12, 16] as const;
export const MAX_CUSTOM_IMAGES = 12;
export const DIM_MAX = 70;
export const BLUR_MAX = 20;
/** Longest side of an uploaded background after downscaling. */
export const UPLOAD_MAX_SIDE = 2560;
export const HOVER_HIDE_MS = 3000;

/** Tile geometry per size, in px. */
export const TILE_METRICS: Record<TileSize, { tile: number; icon: number; favicon: number; radius: number; letter: number }> = {
  small: { tile: 78, icon: 40, favicon: 18, radius: 12, letter: 16 },
  medium: { tile: 92, icon: 48, favicon: 22, radius: 14, letter: 19 },
  large: { tile: 112, icon: 62, favicon: 28, radius: 18, letter: 24 },
};

export function defaultStartPage(): StartPageSettings {
  return {
    showGreeting: true,
    showSearch: true,
    showTiles: true,
    showEssentials: true,
    tileLabels: true,
    tileSize: "medium",
    tileCount: 12,
    showHints: true,
    hoverToHide: true,
    align: "center",
    searchWidth: "wide",
    background: "",
    customImages: [],
    dailyBackground: false,
    dailyDate: "",
    dim: 20,
    blur: 0,
  };
}

const CUSTOM_ID = /^[a-z0-9-]{1,40}$/;

const bool = (v: unknown, fallback: boolean) => (typeof v === "boolean" ? v : fallback);
const oneOf = <T extends string>(v: unknown, options: readonly T[], fallback: T): T =>
  options.includes(v as T) ? (v as T) : fallback;
const clampNum = (v: unknown, min: number, max: number, fallback: number) =>
  typeof v === "number" && Number.isFinite(v) ? Math.min(max, Math.max(min, Math.round(v))) : fallback;

/** Saved preferences (any shape, from settings.json) with every field checked. */
export function normalizeStartPage(raw: unknown, galleryIds: readonly string[]): StartPageSettings {
  const d = defaultStartPage();
  if (!raw || typeof raw !== "object") return d;
  const r = raw as Record<string, unknown>;
  const customImages: CustomWallpaper[] = [];
  if (Array.isArray(r.customImages)) {
    for (const c of r.customImages as unknown[]) {
      const img = c as Partial<CustomWallpaper> | null;
      if (!img || typeof img.id !== "string" || !CUSTOM_ID.test(img.id)) continue;
      if (customImages.some((x) => x.id === img.id)) continue;
      customImages.push({ id: img.id, tone: img.tone === "light" ? "light" : "dark" });
      if (customImages.length >= MAX_CUSTOM_IMAGES) break;
    }
  }
  const out: StartPageSettings = {
    showGreeting: bool(r.showGreeting, d.showGreeting),
    showSearch: bool(r.showSearch, d.showSearch),
    showTiles: bool(r.showTiles, d.showTiles),
    showEssentials: bool(r.showEssentials, d.showEssentials),
    tileLabels: bool(r.tileLabels, d.tileLabels),
    tileSize: oneOf(r.tileSize, ["small", "medium", "large"] as const, d.tileSize),
    tileCount: (TILE_COUNTS as readonly number[]).includes(r.tileCount as number) ? (r.tileCount as number) : d.tileCount,
    showHints: bool(r.showHints, d.showHints),
    hoverToHide: bool(r.hoverToHide, d.hoverToHide),
    align: oneOf(r.align, ["center", "top"] as const, d.align),
    searchWidth: oneOf(r.searchWidth, ["compact", "wide"] as const, d.searchWidth),
    background: "",
    customImages,
    dailyBackground: bool(r.dailyBackground, d.dailyBackground),
    dailyDate: typeof r.dailyDate === "string" && /^\d{4}-\d{2}-\d{2}$/.test(r.dailyDate) ? r.dailyDate : "",
    dim: clampNum(r.dim, 0, DIM_MAX, d.dim),
    blur: clampNum(r.blur, 0, BLUR_MAX, d.blur),
  };
  out.background = backgroundExists(r.background, galleryIds, customImages) ? (r.background as string) : "";
  return out;
}

export type BackgroundRef = { kind: "none" } | { kind: "gallery"; id: string } | { kind: "custom"; id: string };

export function parseBackground(ref: string): BackgroundRef {
  const i = ref.indexOf(":");
  const kind = ref.slice(0, i);
  const id = ref.slice(i + 1);
  if (i > 0 && id && (kind === "gallery" || kind === "custom")) return { kind, id };
  return { kind: "none" };
}

function backgroundExists(ref: unknown, galleryIds: readonly string[], custom: CustomWallpaper[]): boolean {
  if (typeof ref !== "string") return false;
  const b = parseBackground(ref);
  if (b.kind === "gallery") return galleryIds.includes(b.id);
  if (b.kind === "custom") return custom.some((c) => c.id === b.id);
  return false;
}

/** Every background the user can pick, in browsing order: none, their own images, the gallery. */
export function backgroundChoices(galleryIds: readonly string[], custom: CustomWallpaper[]): string[] {
  return ["", ...custom.map((c) => `custom:${c.id}`), ...galleryIds.map((id) => `gallery:${id}`)];
}

/** The choice `step` places from `current`, wrapping around. */
export function stepChoice(choices: readonly string[], current: string, step: number): string {
  if (!choices.length) return "";
  const i = Math.max(0, choices.indexOf(current));
  return choices[(((i + step) % choices.length) + choices.length) % choices.length];
}

/** Local calendar day as YYYY-MM-DD. */
export function localDay(d: Date): string {
  const p = (v: number) => String(v).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Days since 1970-01-01 for a YYYY-MM-DD day (timezone-independent). */
export function dayNumber(day: string): number {
  const [y, m, d] = day.split("-").map(Number);
  return Math.floor(Date.UTC(y, (m || 1) - 1, d || 1) / 86_400_000);
}

/** The background for `day` when it changes daily: each day moves one step through the pictures
 *  (none excluded), so consecutive days never repeat while there are at least two. */
export function dailyChoice(galleryIds: readonly string[], custom: CustomWallpaper[], day: string): string {
  const pool = backgroundChoices(galleryIds, custom).slice(1);
  if (!pool.length) return "";
  return pool[dayNumber(day) % pool.length];
}

/** Text colour that stays readable over a background of `tone` with `dim` % darkening. */
export function textTone(tone: Tone, dim: number): Tone {
  return tone === "light" && dim < 35 ? "dark" : "light";
}

/** Brightness (0-1) of RGBA pixel data, by Rec. 709 relative luminance in gamma space. */
export function averageLuminance(px: ArrayLike<number>): number {
  let sum = 0;
  let count = 0;
  for (let i = 0; i + 3 < px.length; i += 4) {
    const a = px[i + 3] / 255;
    sum += ((0.2126 * px[i] + 0.7152 * px[i + 1] + 0.0722 * px[i + 2]) / 255) * a + (1 - a);
    count++;
  }
  return count ? sum / count : 0;
}

export function toneOf(luminance: number): Tone {
  return luminance > 0.58 ? "light" : "dark";
}

/** Size that fits `w`×`h` inside a `max`×`max` box, never enlarging. */
export function fitWithin(w: number, h: number, max: number): [number, number] {
  const scale = Math.min(1, max / Math.max(w, h, 1));
  return [Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale))];
}

export function newWallpaperId(now: number, random: number): string {
  return `w-${now.toString(36)}-${Math.floor(random * 36 ** 4)
    .toString(36)
    .padStart(4, "0")}`;
}

export function greetingFor(hour: number): string {
  return hour < 5 ? "Good night" : hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
}
