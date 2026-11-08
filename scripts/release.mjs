#!/usr/bin/env node
// Cuts a release: bumps the version everywhere it is written, commits, and tags. Pushing the tag
// makes .github/workflows/release.yml build, sign and publish it (see apps/docs/src/content/docs/contributing/releasing.md).
//
//   pnpm release 0.2.0            explicit version (or: patch | minor | major)
//   pnpm release patch --push     also push the commit and the tag
//   pnpm release minor --dry-run  show what would change, touch nothing
//   node scripts/release.mjs --check v0.2.0   (CI) fail unless the tag matches the app version
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

export const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

/** Negative, zero or positive, like a sort comparator. Pre-releases sort before their release. */
export function compareVersions(a, b) {
  const pa = SEMVER.exec(a);
  const pb = SEMVER.exec(b);
  if (!pa || !pb) throw new Error(`not a version: ${pa ? b : a}`);
  for (let i = 1; i <= 3; i++) {
    const d = Number(pa[i]) - Number(pb[i]);
    if (d) return d;
  }
  if (pa[4] === pb[4]) return 0;
  if (!pa[4]) return 1;
  if (!pb[4]) return -1;
  return pa[4] < pb[4] ? -1 : 1;
}

/** The version after `current` for `spec`: "patch", "minor", "major" or an explicit version. */
export function nextVersion(current, spec) {
  const m = SEMVER.exec(current);
  if (!m) throw new Error(`the current version "${current}" is not semver`);
  const [maj, min, pat] = [Number(m[1]), Number(m[2]), Number(m[3])];
  let next;
  if (spec === "major") next = `${maj + 1}.0.0`;
  else if (spec === "minor") next = `${maj}.${min + 1}.0`;
  else if (spec === "patch")
    next = m[4] ? `${maj}.${min}.${pat}` : `${maj}.${min}.${pat + 1}`;
  else next = String(spec ?? "").replace(/^v/, "");
  if (!SEMVER.test(next))
    throw new Error(
      `"${spec}" is not a version (use x.y.z, or patch / minor / major)`,
    );
  if (compareVersions(next, current) <= 0)
    throw new Error(`${next} is not newer than the current ${current}`);
  return next;
}

function replaceOnce(text, pattern, version, what) {
  if (!pattern.test(text))
    throw new Error(`could not find the version in ${what}`);
  return text.replace(
    pattern,
    (_, before, _old, after) => `${before}${version}${after}`,
  );
}

// The first `"version"` in package.json and tauri.conf.json is the top-level one.
const JSON_VERSION = /("version"\s*:\s*")([^"]*)(")/;
// The `version` line of the [package] table (the first table in Cargo.toml).
const CARGO_VERSION = /(\[package\][^[]*?\nversion\s*=\s*")([^"]*)(")/;
// The app's own entry in Cargo.lock (cargo would rewrite it on the next build anyway).
const LOCK_VERSION =
  /(\[\[package\]\]\r?\nname = "zenith"\r?\nversion = ")([^"]*)(")/;

export const FILES = [
  { path: "apps/desktop/package.json", pattern: JSON_VERSION },
  { path: "apps/desktop/src-tauri/Cargo.toml", pattern: CARGO_VERSION },
  { path: "apps/desktop/src-tauri/Cargo.lock", pattern: LOCK_VERSION },
  { path: "apps/desktop/src-tauri/tauri.conf.json", pattern: JSON_VERSION },
];

export function setVersion(text, pattern, version, what = "the file") {
  return replaceOnce(text, pattern, version, what);
}

export function readVersion(text, pattern, what = "the file") {
  const m = pattern.exec(text);
  if (!m) throw new Error(`could not find the version in ${what}`);
  return m[2];
}

function read(path) {
  return readFileSync(join(root, path), "utf8");
}

function git(...args) {
  return execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

/** Every place the version is written must agree. */
export function currentVersions() {
  return Object.fromEntries(
    FILES.map((f) => [f.path, readVersion(read(f.path), f.pattern, f.path)]),
  );
}

function check(tag) {
  const want = String(tag ?? "")
    .replace(/^refs\/tags\//, "")
    .replace(/^v/, "");
  const found = currentVersions();
  const wrong = Object.entries(found).filter(([, v]) => v !== want);
  if (!SEMVER.test(want) || wrong.length) {
    console.error(`Tag "${tag}" does not match the app version:`);
    for (const [path, v] of Object.entries(found))
      console.error(`  ${path}: ${v}`);
    console.error(
      "Cut releases with `pnpm release <version>` so the tag and the files agree.",
    );
    process.exit(1);
  }
  console.log(`Releasing Zenith ${want}`);
}

function release(spec, { push, dryRun }) {
  if (!spec) {
    console.error(
      "Usage: pnpm release <version | patch | minor | major> [--push] [--dry-run]",
    );
    process.exit(1);
  }
  const versions = currentVersions();
  const distinct = [...new Set(Object.values(versions))];
  if (distinct.length !== 1) {
    console.error("The version differs between files; fix them first:");
    for (const [path, v] of Object.entries(versions))
      console.error(`  ${path}: ${v}`);
    process.exit(1);
  }
  const current = distinct[0];
  const next = nextVersion(current, spec);
  const tag = `v${next}`;
  if (!dryRun) {
    const dirty = git("status", "--porcelain");
    if (dirty) {
      console.error(
        "Commit or stash your changes first; a release tags exactly what is committed:",
      );
      console.error(dirty);
      process.exit(1);
    }
    let exists = true;
    try {
      git("rev-parse", "-q", "--verify", `refs/tags/${tag}`);
    } catch {
      exists = false;
    }
    if (exists) {
      console.error(`Tag ${tag} already exists.`);
      process.exit(1);
    }
  }
  console.log(
    `${current} -> ${next}${dryRun ? " (dry run: nothing written)" : ""}`,
  );
  for (const f of FILES) {
    const updated = setVersion(read(f.path), f.pattern, next, f.path);
    console.log(`  ${f.path}`);
    if (!dryRun) writeFileSync(join(root, f.path), updated);
  }
  if (dryRun) return;
  git("add", ...FILES.map((f) => f.path));
  git("commit", "-m", `release: ${tag}`);
  git("tag", "-a", tag, "-m", `Zenith ${tag}`);
  console.log(`Committed and tagged ${tag}.`);
  if (push) {
    const branch = git("rev-parse", "--abbrev-ref", "HEAD");
    execFileSync("git", ["push", "origin", branch], {
      cwd: root,
      stdio: "inherit",
    });
    execFileSync("git", ["push", "origin", tag], {
      cwd: root,
      stdio: "inherit",
    });
    console.log(
      `Pushed. GitHub Actions is building ${tag}: https://github.com/Drakaniia/Zenith/actions`,
    );
  } else {
    console.log(
      `Publish it with:\n  git push origin HEAD && git push origin ${tag}`,
    );
  }
}

function main(argv) {
  const flags = new Set(argv.filter((a) => a.startsWith("--")));
  const args = argv.filter((a) => !a.startsWith("--"));
  try {
    if (flags.has("--check")) return check(args[0]);
    release(args[0], {
      push: flags.has("--push"),
      dryRun: flags.has("--dry-run"),
    });
  } catch (e) {
    console.error(e instanceof Error ? e.message : String(e));
    process.exit(1);
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  main(process.argv.slice(2));
}
