#!/usr/bin/env node
// Checks the folder rules of the desktop app (apps/docs/src/content/docs/contributing/project-structure.md):
//   - files stay under 600 lines (Rust: not counting the `#[cfg(test)] mod tests` block);
//     a file may opt out with `structure: allow-large — <reason>` in its first 10 lines
//   - directories hold at most 30 files
//   - imports follow main.ts → app → features → shared / platform: nothing outside app imports
//     app/, shared/ and platform/ import no feature, platform imports nothing from shared
//   - no new runtime import cycles between modules (`import type` does not count); the cycles
//     that existed when the rule was introduced are listed in scripts/structure-baseline.json
// Usage: node scripts/check-structure.mjs [--report]   (--report: print, never fail)
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const app = join(root, "apps", "desktop");
const src = join(app, "src");
const LIMIT = 600;
const DIR_LIMIT = 30;
const report = process.argv.includes("--report");
const toPosix = (p) => p.split(sep).join("/");
const problems = [];

function walk(d, acc = []) {
  for (const e of readdirSync(d, { withFileTypes: true })) {
    if (e.name === "node_modules" || e.name === "target" || e.name === "dist" || e.name.startsWith(".")) continue;
    const p = join(d, e.name);
    if (e.isDirectory()) walk(p, acc);
    else acc.push(p);
  }
  return acc;
}

// ---- sizes
const CODE = /\.(ts|svelte|rs|mjs|js|css)$/;
const roots = [src, join(app, "src-tauri", "src"), join(app, "scripts"), join(root, "scripts")];
const files = roots.filter(existsSync).flatMap((r) => walk(r));
for (const f of files.filter((f) => CODE.test(f))) {
  const text = readFileSync(f, "utf8");
  if (/structure: allow-large/.test(text.split(/\r?\n/).slice(0, 10).join("\n"))) continue;
  let lines = text.split(/\r?\n/);
  if (f.endsWith(".rs")) {
    const i = lines.findIndex((l, k) => l.trim() === "#[cfg(test)]" && /^mod tests\b/.test(lines[k + 1] ?? ""));
    if (i >= 0) lines = lines.slice(0, i);
  }
  if (lines.length > LIMIT) problems.push(`size: ${toPosix(relative(root, f))} has ${lines.length} lines (limit ${LIMIT})`);
}

// ---- directory sizes
const dirs = new Map();
for (const f of files) dirs.set(dirname(f), (dirs.get(dirname(f)) ?? 0) + 1);
for (const [d, n] of dirs) if (n > DIR_LIMIT) problems.push(`dir: ${toPosix(relative(root, d))} holds ${n} files (limit ${DIR_LIMIT})`);

// ---- imports
const ALIAS = { $features: "features", $shared: "shared", $platform: "platform" };
const exts = ["", ".ts", ".svelte", "/index.ts"];
function resolveSpec(from, spec) {
  const head = spec.split("/")[0];
  let base;
  if (ALIAS[head]) base = join(src, ALIAS[head], spec.slice(head.length + 1));
  else if (spec.startsWith(".")) base = resolve(dirname(from), spec);
  else return null;
  for (const e of exts) if (existsSync(base + e) && statSync(base + e).isFile()) return base + e;
  return null;
}
const layer = (abs) => {
  const parts = toPosix(relative(src, abs)).split("/");
  if (parts.length === 1) return { top: "root", unit: "root" };
  return { top: parts[0], unit: parts[0] === "features" ? `features/${parts[1]}` : parts[0] };
};
const graph = new Map(); // module -> Set(module), runtime imports only
const IMPORT = /(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)(["'])([^"']+)\1/g;
const TYPE_ONLY = /^\s*(import|export)\s+type\s/;
for (const f of files.filter((f) => f.startsWith(src) && /\.(ts|svelte)$/.test(f))) {
  const text = readFileSync(f, "utf8");
  const from = layer(f);
  const self = toPosix(relative(src, f));
  if (!graph.has(self)) graph.set(self, new Set());
  for (const m of text.matchAll(IMPORT)) {
    // `import type` and `declare module` add no runtime edge but still count for direction.
    const target = resolveSpec(f, m[2].split("?")[0]);
    if (!target) continue;
    const to = layer(target);
    const where = `${toPosix(relative(src, f))} → ${toPosix(relative(src, target))}`;
    if (to.top === "app" && from.top !== "app" && from.top !== "root") problems.push(`layer: only main.ts and app/ may import app/: ${where}`);
    if ((from.top === "shared" || from.top === "platform") && to.top === "features") problems.push(`layer: ${from.top}/ must not import a feature: ${where}`);
    if (from.top === "platform" && to.top === "shared") problems.push(`layer: platform/ must not import shared/: ${where}`);
    const line = text.slice(text.lastIndexOf("\n", m.index) + 1, m.index);
    const typeOnly = TYPE_ONLY.test(line) || /^\s*\}\s*$/.test(line) && TYPE_ONLY.test(text.slice(text.lastIndexOf("import", m.index), m.index));
    if (!typeOnly && !/declare module/.test(line)) graph.get(self).add(toPosix(relative(src, target)));
  }
}

// ---- module cycles (each strongly connected group reported once)
let index = 0;
const idx = new Map(), low = new Map(), stack = [], on = new Set(), groups = [];
function strong(v) {
  idx.set(v, index); low.set(v, index); index++; stack.push(v); on.add(v);
  for (const w of graph.get(v)?.keys() ?? []) {
    if (!idx.has(w)) { strong(w); low.set(v, Math.min(low.get(v), low.get(w))); }
    else if (on.has(w)) low.set(v, Math.min(low.get(v), idx.get(w)));
  }
  if (low.get(v) === idx.get(v)) {
    const g = [];
    let w;
    do { w = stack.pop(); on.delete(w); g.push(w); } while (w !== v);
    if (g.length > 1) groups.push(g);
  }
}
for (const v of graph.keys()) if (!idx.has(v)) strong(v);
const cycles = groups.map((g) => g.sort());

const known = JSON.parse(readFileSync(join(root, "scripts", "structure-baseline.json"), "utf8"));
// A known cycle may shrink (that is progress) but not grow or appear anew.
const allowed = known.importCycles.map((c) => new Set(c));
for (const c of cycles) {
  if (!allowed.some((a) => c.every((m) => a.has(m)))) problems.push(`cycle: modules import each other in a cycle: ${c.join(" ↔ ")}`);
}

if (process.argv.includes("--cycles")) console.log(JSON.stringify(cycles));
if (problems.length) {
  console.log(problems.join("\n"));
  console.log(`\n${problems.length} structure problem(s). See apps/docs/src/content/docs/contributing/project-structure.md.`);
  if (!report) process.exit(1);
} else console.log("structure ok");
