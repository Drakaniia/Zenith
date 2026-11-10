// The hidden fast-download switch: revealed by clicking the Downloads heading in Settings a few
// times in quick succession (and hidden again the same way).

/** Clicks needed, and the most time they may take together. */
export const SECRET_TAPS = 7;
export const SECRET_WINDOW_MS = 3000;

/** Connections per file in fast mode (FAST_CONNECTIONS in the backend's media job). */
export const FAST_CONNECTIONS_LABEL = 8;

/** Counts quick clicks; `tap` returns true on the one that completes the gesture. */
export function tapCounter(taps = SECRET_TAPS, windowMs = SECRET_WINDOW_MS) {
  let times: number[] = [];
  return {
    tap(now: number): boolean {
      times = [...times.filter((t) => now - t < windowMs), now];
      if (times.length < taps) return false;
      times = [];
      return true;
    },
  };
}
