# Electron Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the existing Svelte browser shell in Electron with an Electron main/preload IPC backend and Electron webview rendering.

**Architecture:** Keep the Svelte 5 UI and shared payload types. Add an Electron main process that owns app windows, persistence, URL normalization, metadata fetching, and download bookkeeping. Expose a narrow preload API that preserves the current `invokeCommand(command, args)` and event contracts so most feature state modules remain unchanged.

**Tech Stack:** Svelte 5, TypeScript, Vite, Electron, Node.js ESM, `node:test` for Electron backend tests.

---

## File Structure

- Create: `electron/main.mjs` for Electron app/window lifecycle and IPC registration.
- Create: `electron/preload.cjs` for `contextBridge` APIs exposed to the renderer.
- Create: `electron/backend/commands.mjs` for command handler registration and implementations.
- Create: `electron/backend/store.mjs` for atomic JSON persistence helpers.
- Create: `electron/backend/navigation.mjs` for URL normalization and page metadata extraction.
- Create: `electron/backend/downloads.mjs` for in-memory download state and progress emission.
- Create: `electron/backend/*.test.mjs` for backend unit tests.
- Create: `src/bridge/electron-api.d.ts` for typed `window.ampuiElectron`.
- Modify: `package.json` and `pnpm-lock.yaml` to add Electron dependencies and scripts.
- Modify: `vite.config.ts` to support Electron builds without Tauri-only env assumptions.
- Modify: `tsconfig.json` to include Electron bridge declarations.
- Modify: `src/bridge/runtime.ts`, `src/bridge/invoke.ts`, `src/bridge/events/*.ts`, and `src/bridge/commands/*.ts` to prefer Electron when available and retain Tauri fallback.
- Modify: `src/features/tabs/components/WebviewHost.svelte` to render and control Electron `<webview>` elements when running under Electron.
- Modify: `src/app/App.svelte`, `src/shell/components/TitleBar.svelte`, `src/shell/utils/focus.ts`, and Settings/Downloads copy to use desktop-runtime wording instead of Tauri-only wording.
- Modify: `README.md` and `AGENTS.md` to document verified Electron commands and current source shape.

---

## Task 1: Electron Backend Tests

**Files:**

- Create: `electron/backend/navigation.test.mjs`
- Create: `electron/backend/store.test.mjs`
- Create: `electron/backend/commands.test.mjs`

- [ ] **Step 1: Write failing tests for URL normalization**

Test the same externally visible cases already covered by Rust: bare domains become `https://`, explicit `https://` stays unchanged, bare localhost becomes `http://`, empty input rejects, Yandex is default, Google setting switches search URLs.

Run: `node --test electron/backend/navigation.test.mjs`
Expected: FAIL because `electron/backend/navigation.mjs` does not exist.

- [ ] **Step 2: Write failing tests for JSON persistence**

Test missing settings returns defaults, invalid settings falls back to defaults, bookmarks de-duplicate by URL while preserving id/timestamp, and history search is case-insensitive.

Run: `node --test electron/backend/store.test.mjs electron/backend/commands.test.mjs`
Expected: FAIL because persistence and command modules do not exist.

---

## Task 2: Electron Main, Preload, And Backend

**Files:**

- Create: `electron/main.mjs`
- Create: `electron/preload.cjs`
- Create: `electron/backend/commands.mjs`
- Create: `electron/backend/store.mjs`
- Create: `electron/backend/navigation.mjs`
- Create: `electron/backend/downloads.mjs`
- Modify: `package.json`

- [ ] **Step 1: Implement `navigation.mjs` minimally**

Provide `normalizeUrlWithSettings(input, settings)` and `fetchPageMetadata(url)` with the current command payload shape: `{ title, favicon_url }`.

- [ ] **Step 2: Implement `store.mjs` minimally**

Provide atomic JSON read/write helpers and store classes/functions for `settings.json`, `bookmarks.json`, and `history.json` under an injected data directory.

- [ ] **Step 3: Run backend tests**

Run: `node --test electron/backend/*.test.mjs`
Expected: PASS.

- [ ] **Step 4: Implement `commands.mjs`**

Register `ipcMain.handle('ampui:invoke', ...)` and support the command names in `src/bridge/invoke.ts` for settings, bookmarks, history, navigation, downloads, and window operations.

- [ ] **Step 5: Implement `main.mjs` and `preload.cjs`**

Create a frameless `BrowserWindow` with `contextIsolation: true`, `nodeIntegration: false`, `webviewTag: true`, and the preload path. Load `http://localhost:1420` in dev and `dist/index.html` in production.

---

## Task 3: Renderer Bridge Adapter

**Files:**

- Create: `src/bridge/electron-api.d.ts`
- Modify: `src/bridge/runtime.ts`
- Modify: `src/bridge/invoke.ts`
- Modify: `src/bridge/events/browser-webviews.ts`
- Modify: `src/bridge/events/downloads.ts`
- Modify: `src/bridge/events/shortcuts.ts`
- Modify: `src/bridge/commands/tabs.ts`
- Modify: `src/bridge/commands/windows.ts`
- Modify: `src/bridge/commands/devtools.ts`
- Modify: `src/shell/utils/focus.ts`

- [ ] **Step 1: Add typed preload surface**

Declare `window.ampuiElectron` with `invoke`, `on`, `off`, `platform`, `appVersion`, `clearAllBrowsingData`, `focusShell`, and `toggleDevtools`.

- [ ] **Step 2: Update runtime detection**

Expose `isElectronRuntime()`, `isTauriRuntime()`, and `isDesktopRuntime()`. Keep existing imports working for Tauri fallback.

- [ ] **Step 3: Route invoke and events**

Make `invokeCommand` call Electron first when `window.ampuiElectron` exists; otherwise use Tauri invoke. Make event listeners use Electron `on/off` first; otherwise Tauri `listen`.

- [ ] **Step 4: Route window commands**

Make window controls and external URL opening use Electron command handlers when Electron is present, with Tauri fallback preserved.

Run: `pnpm check`
Expected: PASS.

---

## Task 4: Electron Webview Host

**Files:**

- Modify: `src/features/tabs/components/WebviewHost.svelte`
- Modify: `src/shared/types/events.ts` only if needed for Electron event typing.

- [ ] **Step 1: Add Electron webview element state**

Track `Map<label, Electron.WebviewTag>` in the component and render one `<webview>` per remote tab when Electron runtime is active.

- [ ] **Step 2: Wire Electron webview events**

On `did-start-loading`, `did-stop-loading`, `did-navigate`, `did-navigate-in-page`, and `page-title-updated`, call the existing `handleWebviewLoad`, `handleWebviewNavigation`, and `handleWebviewTitle` handlers with the current payload shapes.

- [ ] **Step 3: Wire controller methods**

For Electron webviews, implement navigate, reload, stop, focus, print, zoom, find, clear find, and app-managed back/forward with Electron webview APIs. Keep the Tauri command path unchanged for Tauri runtime.

- [ ] **Step 4: Verify Svelte diagnostics**

Run: `pnpm check`
Expected: PASS.

---

## Task 5: Scripts, Dependencies, And Docs

**Files:**

- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `vite.config.ts`
- Modify: `README.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: Add Electron scripts**

Add `electron:dev`, `electron:start`, and `electron:test`. Keep existing Tauri scripts unless the user explicitly asks to remove Tauri.

- [ ] **Step 2: Install dependencies**

Run: `pnpm add -D electron concurrently wait-on`
Expected: `package.json` and `pnpm-lock.yaml` update.

- [ ] **Step 3: Update docs**

Document that the verified frontend commands now include Electron scripts and that `src` contains the Svelte frontend.

---

## Task 6: Verification And Review

**Files:**

- No planned source edits unless verification exposes defects.

- [ ] **Step 1: Run automated checks**

Run:

```powershell
pnpm electron:test
pnpm check
pnpm build
```

Expected: all pass.

- [ ] **Step 2: Smoke Electron startup**

Run: `pnpm electron:start`
Expected: Electron starts from the production `dist/index.html` without renderer import errors.

- [ ] **Step 3: Subagent reviews**

Dispatch a spec-compliance review and code-quality review. Fix any blocking findings and re-run the relevant checks.
