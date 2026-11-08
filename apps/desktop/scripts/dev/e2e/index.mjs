// End-to-end checks against an isolated Zenith test instance (debug build, own profile and
// remote-debugging port; see run.ps1). Never drives any other Zenith window.
//   powershell -File scripts/dev/run.ps1 -Fresh -SkipSetup
//   node scripts/dev/e2e.mjs [--only name,name] [--skip-restart]
// Drives the chrome UI through the DOM and pages through CDP Input events (trusted input).
// This is the runner: flags, the scenario order and the result summary. The helpers are in
// harness.mjs, the scenarios in cases/ (run from apps/desktop).
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { sessions, ui, waitUi } from "./harness.mjs";
import * as basics from "./cases/basics.mjs";
import * as tabs from "./cases/tabs.mjs";
import * as pages from "./cases/pages.mjs";
import * as downloads from "./cases/downloads.mjs";
import * as extensions from "./cases/extensions.mjs";
import * as windows from "./cases/windows.mjs";
import * as settings from "./cases/settings.mjs";
import * as restart from "./cases/restart.mjs";

const args = process.argv.slice(2);
const only = args.includes("--only")
  ? args[args.indexOf("--only") + 1].split(",")
  : null;
const skipRestart = args.includes("--skip-restart");

// --------------------------------------------------------------------------------------------
const results = [];
const resultsFile = join(tmpdir(), "zenith-e2e-results.json");
writeFileSync(resultsFile, "[]");
async function test(name, fn, limitMs = 60000) {
  if (only && !only.includes(name)) return;
  const start = Date.now();
  const deadline = new Promise((_, rej) =>
    setTimeout(
      () => rej(new Error(`test timed out after ${limitMs / 1000}s`)),
      limitMs,
    ),
  );
  try {
    const detail = await Promise.race([fn(), deadline]);
    results.push({ name, ok: true, detail, ms: Date.now() - start });
    console.log(`PASS ${name}${detail ? ` — ${detail}` : ""}`);
  } catch (e) {
    results.push({
      name,
      ok: false,
      detail: String(e?.message ?? e),
      ms: Date.now() - start,
    });
    console.log(`FAIL ${name} — ${e?.message ?? e}`);
  }
  writeFileSync(resultsFile, JSON.stringify(results, null, 2));
}

await waitUi(`!!document.querySelector('.app')`, "chrome UI", 20000);
// Start from a known layout: leave compact mode if it was turned on.
if (await ui(`document.querySelector('.app').classList.contains('compact')`)) {
  await ui(
    `document.querySelector('.hot-left').dispatchEvent(new MouseEvent('mouseenter'))`,
  );
  await waitUi(
    `!!document.querySelector('.flyout [data-action=compact]')`,
    "floating sidebar",
  );
  await ui(`document.querySelector('.flyout [data-action=compact]').click()`);
  await waitUi(
    `!document.querySelector('.app').classList.contains('compact')`,
    "compact mode off",
  );
}
await waitUi(
  `!!document.querySelector('.sidebar .new-tab')`,
  "chrome UI with restored session",
  20000,
);

// The scenarios run in this order: later ones start from the state earlier ones leave behind.
const scenarios = [
  ...basics.scenarios,
  ...tabs.scenarios,
  ...pages.scenarios,
  ...downloads.scenarios,
  ...extensions.loading,
  ...windows.scenarios,
  ...pages.zoom,
  ...downloads.exempt,
  ...settings.scenarios,
  ...extensions.store,
  ...tabs.sleepTimeout,
];
for (const s of scenarios) await test(s.name, s.fn, s.limitMs);
if (!skipRestart) {
  for (const s of restart.scenarios) await test(s.name, s.fn, s.limitMs);
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} passed`);
writeFileSync(resultsFile, JSON.stringify(results, null, 2));
for (const s of sessions.values()) s.close();
process.exit(failed.length ? 1 : 0);
