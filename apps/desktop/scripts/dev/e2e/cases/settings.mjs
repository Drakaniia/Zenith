// Scenarios: settings and tools (the shortcut editor, search suggestions, memory stats).
import {
  ui,
  invoke,
  waitFor,
  waitUi,
  openSettings,
} from "../harness.mjs";

export const scenarios = [];
const list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("shortcut-editor", async () => {
  await openSettings();
  const row = `[...document.querySelectorAll('#settings-shortcuts .row')].find((r) => r.querySelector('.label')?.textContent === 'Duplicate tab')`;
  const key = (code, ctrl = false, shift = false) =>
    ui(
      `window.dispatchEvent(new KeyboardEvent('keydown', { code: '${code}', key: '${code === "Escape" ? "Escape" : "x"}', ctrlKey: ${ctrl}, shiftKey: ${shift}, bubbles: true, cancelable: true }))`,
    );
  const message = () =>
    ui(
      `document.querySelector('#settings-shortcuts .message')?.textContent ?? ''`,
    );
  // Record a new chord.
  await ui(`${row}.querySelector('.add').click()`);
  await waitUi(`!!${row}.querySelector('.listening')`, "recording");
  await key("KeyY", true, true);
  await waitUi(
    `!${row}.querySelector('.listening') && [...${row}.querySelectorAll('.kbd')].some((k) => k.textContent === 'Y')`,
    "Ctrl+Shift+Y listed",
  );
  // A chord owned by another action needs confirmation.
  await ui(`${row}.querySelector('.add').click()`);
  await key("KeyT", true);
  const conflict = await waitFor(
    async () => ((await message()).includes("New tab") ? message() : null),
    "conflict warning",
  );
  // Editing keys are refused.
  await key("KeyC", true);
  const reserved = await waitFor(
    async () => ((await message()).includes("Reserved") ? message() : null),
    "reserved-key warning",
  );
  await key("Escape");
  await waitUi(`!${row}.querySelector('.listening')`, "recording cancelled");
  // Settings are saved with a short debounce.
  await waitFor(
    async () =>
      (
        JSON.parse(await invoke("settings_load")).shortcuts?.duplicateTab ?? []
      ).includes("Ctrl+Shift+Y"),
    "binding saved",
    5000,
  );
  return `recorded Ctrl+Shift+Y and saved it; conflict: "${conflict.trim()}"; reserved: "${reserved.trim()}"`;
});

test("search-suggestions", async () => {
  const g = await invoke(
    "search_suggest",
    { engine: "google", query: "rust lang" },
    20000,
  );
  const y = await invoke(
    "search_suggest",
    { engine: "yandex", query: "rust lang" },
    20000,
  );
  if (!g.length) throw new Error("no Google suggestions");
  return `Google: ${g.slice(0, 3).join(" / ")}; Yandex: ${y.length} suggestions`;
});

test("memory-stats", async () => {
  const m = await invoke("memory_stats");
  return `${m.processes.length} processes, ${Math.round(m.workingSet / 1048576)} MB working set, ${Math.round(m.privateBytes / 1048576)} MB private`;
});
