// The start page's built-in background gallery. Every picture is drawn here as an SVG (seeded,
// so it is the same on every machine): nothing is downloaded and nothing ships as a file.
//
// `tone` is how bright the middle of the picture is, where the greeting, search bar and site
// tiles sit. The start page uses it to pick light or dark text over the picture.

import type { Tone } from "./startpage";

export interface Wallpaper {
  id: string;
  name: string;
  tone: Tone;
  svg: () => string;
}

const W = 1600;
const H = 1000;

/** Small seeded PRNG (mulberry32). */
function rng(seed: number): () => number {
  let s = seed | 0;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const n = (v: number) => Math.round(v * 10) / 10;

function svgDoc(defs: string, body: string): string {
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid slice">` +
    `<defs>${defs}</defs>${body}</svg>`
  );
}

/** A vertical gradient: stops as [offset 0..1, colour]. */
function vgrad(id: string, stops: [number, string][]): string {
  return (
    `<linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1">` +
    stops.map(([o, c]) => `<stop offset="${o}" stop-color="${c}"/>`).join("") +
    `</linearGradient>`
  );
}

function blur(id: string, sd: number): string {
  return `<filter id="${id}" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="${sd}"/></filter>`;
}

/** A ridge line across the width (smoothed random heights around `y`), filled to the bottom. */
function ridge(seed: number, y: number, amp: number, steps: number, fill: string, extra = ""): string {
  const r = rng(seed);
  const pts: [number, number][] = [];
  for (let i = 0; i <= steps; i++) pts.push([(i / steps) * W, y + (r() - 0.5) * 2 * amp]);
  let d = `M0 ${H} L0 ${n(pts[0][1])}`;
  for (let i = 1; i < pts.length; i++) {
    const [x0, y0] = pts[i - 1];
    const [x1, y1] = pts[i];
    d += ` Q${n(x0)} ${n(y0)} ${n((x0 + x1) / 2)} ${n((y0 + y1) / 2)}`;
  }
  d += ` L${W} ${n(pts[pts.length - 1][1])} L${W} ${H} Z`;
  return `<path d="${d}" fill="${fill}"${extra}/>`;
}

/** A sine wave across the width, filled to the bottom. */
function wave(y: number, amp: number, cycles: number, phase: number, fill: string, extra = ""): string {
  let d = `M0 ${H}`;
  for (let i = 0; i <= 80; i++) {
    const x = (i / 80) * W;
    const a = (i / 80) * Math.PI * 2 * cycles + phase;
    d += ` L${n(x)} ${n(y + Math.sin(a) * amp + Math.sin(a * 2.3 + phase) * amp * 0.25)}`;
  }
  return `<path d="${d} L${W} ${H} Z" fill="${fill}"${extra}/>`;
}

function stars(seed: number, count: number, maxY: number, color: string): string {
  const r = rng(seed);
  let out = "";
  for (let i = 0; i < count; i++) {
    out += `<circle cx="${n(r() * W)}" cy="${n(r() * maxY)}" r="${n(0.5 + r() * 1.3)}" fill="${color}" opacity="${n(0.25 + r() * 0.7)}"/>`;
  }
  return out;
}

/** Topographic contour lines around a centre. */
function contours(seed: number, cx: number, cy: number, levels: number, gap: number, stroke: string, accent?: string): string {
  const r = rng(seed);
  const p1 = r() * 6;
  const p2 = r() * 6;
  const p3 = r() * 6;
  let out = "";
  for (let k = 1; k <= levels; k++) {
    const base = k * gap;
    let d = "";
    for (let i = 0; i <= 120; i++) {
      const t = (i / 120) * Math.PI * 2;
      const wob = 1 + 0.14 * Math.sin(3 * t + p1 + k * 0.08) + 0.08 * Math.sin(5 * t + p2) + 0.05 * Math.sin(2 * t + p3 + k * 0.2);
      d += `${i ? " L" : "M"}${n(cx + Math.cos(t) * base * wob * 1.25)} ${n(cy + Math.sin(t) * base * wob)}`;
    }
    const hot = accent && k % 6 === 0;
    out += `<path d="${d} Z" fill="none" stroke="${hot ? accent : stroke}" stroke-width="${hot ? 2.2 : 1.4}"/>`;
  }
  return out;
}

export const GALLERY: readonly Wallpaper[] = [
  {
    id: "dawn-ridge",
    name: "Dawn ridge",
    tone: "dark",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#3e3566"],
          [0.45, "#9a6c9c"],
          [0.68, "#f0a58a"],
          [1, "#f6c6a0"],
        ]) + blur("g", 40),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          `<circle cx="1040" cy="640" r="150" fill="#ffd9b8" opacity="0.55" filter="url(#g)"/>` +
          `<circle cx="1040" cy="640" r="74" fill="#ffe7cf"/>` +
          ridge(11, 650, 60, 9, "#a8789f") +
          ridge(12, 720, 70, 11, "#7f5b8c") +
          ridge(13, 800, 55, 13, "#5a4174") +
          ridge(14, 890, 40, 15, "#35274f"),
      ),
  },
  {
    id: "night-ridge",
    name: "Night ridge",
    tone: "dark",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#070b1d"],
          [0.6, "#18204a"],
          [1, "#343b72"],
        ]) + blur("g", 30),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          stars(21, 170, 640, "#ffffff") +
          `<circle cx="1210" cy="230" r="90" fill="#f4efe0" opacity="0.18" filter="url(#g)"/>` +
          `<circle cx="1210" cy="230" r="40" fill="#f4efe0"/>` +
          ridge(22, 700, 90, 10, "#20264d") +
          ridge(23, 790, 70, 12, "#161a38") +
          ridge(24, 880, 45, 16, "#0c0f23"),
      ),
  },
  {
    id: "tide",
    name: "Tide",
    tone: "dark",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#062033"],
          [1, "#0d4f6c"],
        ]),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          wave(540, 26, 1.6, 0.4, "#11657f", ` opacity="0.7"`) +
          wave(610, 30, 1.9, 1.7, "#147a8f", ` opacity="0.8"`) +
          wave(690, 28, 1.4, 3.1, "#1a8f98") +
          wave(770, 32, 2.1, 4.4, "#167e88") +
          wave(850, 26, 1.7, 5.6, "#0f6372") +
          wave(930, 20, 2.4, 0.9, "#0a4655"),
      ),
  },
  {
    id: "ember",
    name: "Ember sea",
    tone: "dark",
    svg: () => {
      const r = rng(41);
      let glints = "";
      for (let i = 0; i < 26; i++) {
        const y = 620 + i * 14 + r() * 6;
        const w = (420 - i * 12) * (0.55 + r() * 0.5);
        glints += `<rect x="${n(800 - w / 2 + (r() - 0.5) * 60)}" y="${n(y)}" width="${n(w)}" height="${n(2 + r() * 3)}" rx="2" fill="#f7a65c" opacity="${n(0.75 - i * 0.025)}"/>`;
      }
      return svgDoc(
        vgrad("s", [
          [0, "#170c1d"],
          [0.32, "#4a1a2c"],
          [0.52, "#b03f30"],
          [0.62, "#f28c3f"],
        ]) +
          vgrad("sea", [
            [0, "#3a1624"],
            [1, "#0f070f"],
          ]) +
          blur("g", 50),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          `<circle cx="800" cy="618" r="190" fill="#ffb36b" opacity="0.5" filter="url(#g)"/>` +
          `<circle cx="800" cy="618" r="96" fill="#ffd38f"/>` +
          `<rect y="618" width="${W}" height="${H - 618}" fill="url(#sea)"/>` +
          glints,
      );
    },
  },
  {
    id: "aurora",
    name: "Aurora",
    tone: "dark",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#030a12"],
          [1, "#0b2430"],
        ]) + blur("g", 46),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          stars(51, 120, 700, "#dff7ff") +
          `<g filter="url(#g)" fill="none" stroke-linecap="round">` +
          `<path d="M-100 420 C 300 180, 700 520, 1000 300 S 1500 160, 1750 330" stroke="#3ee8a5" stroke-width="110" opacity="0.55"/>` +
          `<path d="M-100 520 C 380 330, 760 640, 1120 420 S 1520 330, 1750 480" stroke="#2bc4c9" stroke-width="80" opacity="0.4"/>` +
          `<path d="M200 300 C 520 120, 900 260, 1300 140" stroke="#9a7bff" stroke-width="70" opacity="0.35"/>` +
          `</g>` +
          ridge(52, 820, 70, 11, "#0a1a20") +
          ridge(53, 900, 40, 14, "#050e12"),
      ),
  },
  {
    id: "moss",
    name: "Moss",
    tone: "dark",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#14302a"],
          [1, "#3f6b4c"],
        ]) + blur("g", 30),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          ridge(61, 600, 70, 8, "#4a7a55", ` opacity="0.85"`) +
          `<rect y="600" width="${W}" height="90" fill="#9fc3a0" opacity="0.18" filter="url(#g)"/>` +
          ridge(62, 690, 65, 10, "#33603f") +
          `<rect y="700" width="${W}" height="70" fill="#9fc3a0" opacity="0.14" filter="url(#g)"/>` +
          ridge(63, 790, 55, 12, "#22452d") +
          ridge(64, 890, 35, 16, "#132a1c"),
      ),
  },
  {
    id: "contour",
    name: "Contour",
    tone: "dark",
    svg: () =>
      svgDoc(
        "",
        `<rect width="${W}" height="${H}" fill="#0f1418"/>` +
          contours(71, 1180, 380, 34, 26, "#25323c", "#4c6a80") +
          contours(72, 240, 900, 18, 30, "#202b33"),
      ),
  },
  {
    id: "dunes",
    name: "Dunes",
    tone: "light",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#f5e2c8"],
          [1, "#eab88b"],
        ]),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          wave(640, 40, 0.8, 0.3, "#e0a370") +
          wave(720, 50, 0.7, 2.2, "#cf8752") +
          wave(810, 45, 0.9, 4.1, "#b46a3b") +
          wave(900, 35, 0.6, 1.4, "#8e4f2a"),
      ),
  },
  {
    id: "fog",
    name: "Fog",
    tone: "light",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#e8edf1"],
          [1, "#c3cfda"],
        ]) + blur("g", 34),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          ridge(81, 620, 80, 9, "#aebccb", ` opacity="0.8"`) +
          `<rect y="620" width="${W}" height="110" fill="#f3f6f8" opacity="0.6" filter="url(#g)"/>` +
          ridge(82, 720, 70, 11, "#91a3b5") +
          `<rect y="730" width="${W}" height="90" fill="#eef2f5" opacity="0.45" filter="url(#g)"/>` +
          ridge(83, 820, 55, 13, "#73879c") +
          ridge(84, 910, 35, 16, "#56697e"),
      ),
  },
  {
    id: "glacier",
    name: "Glacier",
    tone: "light",
    svg: () =>
      svgDoc(
        vgrad("s", [
          [0, "#bcd9ec"],
          [0.7, "#e6f2f9"],
          [1, "#f4f9fc"],
        ]),
        `<rect width="${W}" height="${H}" fill="url(#s)"/>` +
          ridge(91, 640, 120, 7, "#a9cbe2") +
          ridge(92, 730, 95, 9, "#82adcf") +
          ridge(93, 820, 60, 12, "#5f8fb9") +
          ridge(94, 910, 30, 16, "#3f6f9c"),
      ),
  },
  {
    id: "chalk",
    name: "Chalk",
    tone: "light",
    svg: () =>
      svgDoc(
        "",
        `<rect width="${W}" height="${H}" fill="#eef0f2"/>` +
          contours(101, 380, 260, 30, 30, "#d3d9df", "#b4bfca") +
          contours(102, 1380, 860, 20, 32, "#d9dee3"),
      ),
  },
  {
    id: "bloom",
    name: "Bloom",
    tone: "light",
    svg: () =>
      svgDoc(
        blur("g", 120),
        `<rect width="${W}" height="${H}" fill="#f5d3d0"/>` +
          `<g filter="url(#g)">` +
          `<ellipse cx="300" cy="260" rx="420" ry="300" fill="#f7a9a2"/>` +
          `<ellipse cx="1250" cy="220" rx="380" ry="280" fill="#f9d6a8"/>` +
          `<ellipse cx="1150" cy="820" rx="460" ry="300" fill="#d6b7ea"/>` +
          `<ellipse cx="380" cy="860" rx="380" ry="260" fill="#aecbf0"/>` +
          `</g>`,
      ),
  },
];

export const GALLERY_IDS: readonly string[] = GALLERY.map((w) => w.id);

export function galleryById(id: string): Wallpaper | undefined {
  return GALLERY.find((w) => w.id === id);
}

const urls = new Map<string, string>();

/** The gallery picture as a data URL (drawn once, then cached). */
export function galleryUrl(id: string): string | null {
  const cached = urls.get(id);
  if (cached) return cached;
  const w = galleryById(id);
  if (!w) return null;
  const url = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(w.svg())}`;
  urls.set(id, url);
  return url;
}
