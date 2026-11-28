// Pure layout math and persistence parsing for docked per-tab DevTools.

export type DockSide = "right" | "bottom";

export interface DevToolsLayout {
  dock: DockSide;
  width: number;
  height: number;
}

export const MIN_DEVTOOLS_SIZE = 220;
export const MIN_CONTENT_SIZE = 240;

export const DEFAULT_DEVTOOLS_LAYOUT: DevToolsLayout = {
  dock: "right",
  width: 420,
  height: 300,
};

/** Clamps a docked DevTools width or height so both the page and DevTools stay usable. */
export function clampDevToolsSize(desired: number, containerSize: number): number {
  const safeDesired = Number.isFinite(desired) ? desired : MIN_DEVTOOLS_SIZE;
  if (!Number.isFinite(containerSize) || containerSize <= 0) {
    return Math.max(MIN_DEVTOOLS_SIZE, Math.round(safeDesired));
  }
  const max = Math.max(MIN_DEVTOOLS_SIZE, Math.floor(containerSize - MIN_CONTENT_SIZE));
  const min = Math.min(MIN_DEVTOOLS_SIZE, Math.max(120, Math.floor(containerSize * 0.5)));
  return Math.round(Math.min(max, Math.max(min, safeDesired)));
}

/** Parses stored DevTools layout JSON, falling back to defaults for missing or invalid fields. */
export function parseDevToolsLayout(raw: string | null | undefined): DevToolsLayout {
  if (!raw) return { ...DEFAULT_DEVTOOLS_LAYOUT };
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return { ...DEFAULT_DEVTOOLS_LAYOUT };
    const obj = parsed as Record<string, unknown>;
    const dock: DockSide = obj.dock === "bottom" ? "bottom" : "right";
    const width =
      typeof obj.width === "number" && Number.isFinite(obj.width)
        ? Math.max(MIN_DEVTOOLS_SIZE, Math.round(obj.width))
        : DEFAULT_DEVTOOLS_LAYOUT.width;
    const height =
      typeof obj.height === "number" && Number.isFinite(obj.height)
        ? Math.max(MIN_DEVTOOLS_SIZE, Math.round(obj.height))
        : DEFAULT_DEVTOOLS_LAYOUT.height;
    return { dock, width, height };
  } catch {
    return { ...DEFAULT_DEVTOOLS_LAYOUT };
  }
}
