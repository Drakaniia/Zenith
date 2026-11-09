// Fly-to-downloads: a download's file lifts off where it started and arcs into the downloads
// button. Pure geometry and the file's badge; flights.svelte.ts and DownloadFlights.svelte play it.

import type { IconName } from "../../shared/ui/icons";
import { fileType } from "./file-type.ts";

export interface Point {
  x: number;
  y: number;
}

/** A rectangle in window CSS px. */
export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One sampled moment of a flight: where the file's centre is and how it looks. */
export interface FlightFrame {
  offset: number;
  x: number;
  y: number;
  scale: number;
  rotate: number;
  opacity: number;
  /** 0 on the ground, 1 fully lifted (drives the shadow). */
  lift: number;
}

/** The flying file's own size (CSS px) at scale 1. */
export const CHIP = { w: 44, h: 52 };

/** Share of the flight spent lifting the file before it travels. */
export const LIFT = 0.2;
/** How far the file rises, and how much it grows, as it lifts. */
const RISE_PX = 10;
const LIFT_SCALE = 1.12;

export function centre(b: Box): Point {
  return { x: b.x + b.w / 2, y: b.y + b.h / 2 };
}

/** A box of the chip's own size centred on `p` (a click on a page). */
export function boxAt(p: Point): Box {
  return { x: p.x - CHIP.w / 2, y: p.y - CHIP.h / 2, w: CHIP.w, h: CHIP.h };
}

/** Control point of the quadratic arc from `a` to `b`: above both ends and nearer the start, so
 *  the file rises first, then drops into the button. Never above the window's top `margin`. */
export function arcControl(a: Point, b: Point, margin = 12): Point {
  const dist = Math.hypot(b.x - a.x, b.y - a.y);
  const rise = Math.min(220, 40 + dist * 0.3);
  return { x: a.x + (b.x - a.x) * 0.3, y: Math.max(margin, Math.min(a.y, b.y) - rise) };
}

/** The point at `t` on the quadratic Bézier a → c → b. */
export function quad(a: Point, c: Point, b: Point, t: number): Point {
  const u = 1 - t;
  return {
    x: u * u * a.x + 2 * u * t * c.x + t * t * b.x,
    y: u * u * a.y + 2 * u * t * c.y + t * t * b.y,
  };
}

const easeOut = (t: number) => 1 - (1 - t) ** 3;
const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);

/** The scale at which the chip fits inside `b`. */
function fit(b: Box): number {
  return Math.min(b.w / CHIP.w, b.h / CHIP.h);
}

/** Flight time: longer for longer trips, within a range that still reads as one gesture. */
export function flightMs(from: Box, to: Box): number {
  const a = centre(from);
  const b = centre(to);
  return Math.round(560 + Math.min(1200, Math.hypot(b.x - a.x, b.y - a.y)) * 0.25);
}

/** Samples the flight from `from` (the file where it was) to `to` (the downloads button): it
 *  fades in and lifts in place, then follows the arc with an ease-in-out, tilting toward its
 *  heading, shrinking to fit the button and fading as it lands. */
export function flightFrames(from: Box, to: Box, steps = 32): FlightFrame[] {
  const start = centre(from);
  const end = centre(to);
  const s0 = fit(from);
  const s1 = s0 * LIFT_SCALE;
  const s2 = fit(to) * 0.55;
  const lifted = { x: start.x, y: start.y - RISE_PX };
  const control = arcControl(lifted, end);
  const tilt = end.x < start.x ? -1 : 1;
  const frames: FlightFrame[] = [];

  const liftSteps = Math.max(2, Math.round(steps * LIFT));
  for (let i = 0; i < liftSteps; i++) {
    const u = i / liftSteps;
    const e = easeOut(u);
    frames.push({
      offset: LIFT * u,
      x: start.x,
      y: start.y - RISE_PX * e,
      scale: s0 + (s1 - s0) * e,
      rotate: 0,
      opacity: Math.min(1, u * 2.5),
      lift: e,
    });
  }
  const travelSteps = Math.max(2, steps - liftSteps);
  for (let i = 0; i <= travelSteps; i++) {
    const q = i / travelSteps;
    const e = easeInOut(q);
    const p = quad(lifted, control, end, e);
    frames.push({
      offset: LIFT + (1 - LIFT) * q,
      x: p.x,
      y: p.y,
      scale: s1 + (s2 - s1) * e,
      rotate: tilt * 9 * Math.sin(Math.PI * q),
      opacity: q > 0.82 ? Math.max(0, 1 - (q - 0.82) / 0.18) : 1,
      lift: 1 - 0.6 * e,
    });
  }
  return frames;
}

export interface FileBadge {
  icon: IconName;
  label: string;
  /** The icon's colour for the file's kind; `null` keeps the text colour. */
  color: string | null;
}

/** What the flying file shows: its kind's icon and colour, and its extension. */
export function fileBadge(fileName: string, mime = ""): FileBadge {
  const { icon, label, color } = fileType(fileName, mime);
  return { icon, label, color };
}
