---
title: Project structure
description: Where code lives in the Zenith repository, the rules that keep it organized, and how to add a feature.
---

Zenith is a pnpm monorepo. Each app and package is its own workspace; the root only holds workspace scripts, CI and the release script.

```
apps/
  desktop/              the browser (Tauri 2 + Svelte 5)
    src/                the chrome UI (TypeScript, Svelte)
    src-tauri/          the backend (Rust)
    scripts/            dev scripts: dev/ (build, run, e2e), measure*, gen-profile-avatars.mjs
    measurements/       generated performance data (see Measurements)
  docs/                 this website (Astro Starlight)
packages/
  brand/                the logo, shared by the app and the website
scripts/                release.mjs (version bump + tag), check-structure.mjs
```

Run everything from the repository root: `pnpm app:dev`, `pnpm app:build`, `pnpm check`, `pnpm test`, `pnpm test:rust`, `pnpm docs:dev`, `pnpm check:structure`.

## The UI: `apps/desktop/src/`

The UI is organized by feature. Dependencies point one way:

```
main.ts → app/ → features/<domain>/ → shared/, platform/
```

| Folder | What goes there |
| --- | --- |
| `app/` | The composition root: `App.svelte` and the pieces it is split into, the root overlays, `Menu`/`Toasts`, `actions.ts` (loads every feature's actions) and the dev-only `preview/` backend mock. Only `main.ts` imports it. |
| `features/<domain>/` | Everything one user-facing domain needs: its components, its store (`<domain>.svelte.ts`), pure logic (`<topic>.ts`) with colocated tests (`<topic>.test.ts`), backend calls (`api.ts`), types (`types.ts`), actions (`actions.ts`) and menus (`menus.ts`). |
| `shared/` | Domain-free building blocks used by three or more features: UI primitives (`shared/ui/`: `Icon`, `Toggle`, `Segmented`, `Favicon`, `WindowControls`, the menu model, motion), URL helpers, formatting, the clipboard. No business logic, and no imports from features. |
| `platform/` | The bridge to the backend: typed backend events (`ipc.ts`), page-webview placement and overlay leases (`layout.svelte.ts`, `webview-api.ts`), Windows shell services (`shell.ts`) and the action registry (`actions.ts`). No domain logic, no imports from features or shared. |
| `styles/` | Global CSS (`app.css`: tokens and base styles). |

Features today: `blocker`, `bookmarks`, `browser` (tabs, spaces, split view, Glance, session), `chatbot`, `default-browser`, `downloads` (with the video downloader), `extensions`, `history`, `notes`, `onboarding`, `page` (find in page, permissions, page tools), `palette` (address bar and command palette), `passwords` (with `cards/` for the wallet), `profiles`, `settings`, `shortcuts`, `sidebar`, `startpage`, `toolbar`, `updates`, `window` (this window, incognito, new windows, quit).

### Conventions

- **Names:** components `PascalCase.svelte`; modules `kebab-case.ts`; runes stores `kebab-case.svelte.ts`; tests `<module>.test.ts` next to the module. A component and a store in the same folder must not differ only in case (Windows file systems are case-insensitive): that is why the setup assistant is `OnboardingDialog.svelte` beside `onboarding.svelte.ts`.
- **Imports:** `.svelte` and `.svelte.ts` files use the aliases `$features/…`, `$shared/…`, `$platform/…` across folders and relative paths within a feature. Plain `.ts` modules always use relative imports, because the Node test runner loads them and resolves no aliases.
- **Folders:** a feature folder stays flat. Use a subfolder only for a cohesive sub-area (`onboarding/steps/`, `settings/sections/`, `passwords/cards/`). At most three levels below `src/`. No `index.ts` barrels: import the file you need.
- **Backend calls:** each feature's `api.ts` exports an object (`bookmarksApi`, `tabsApi`, …) wrapping `invoke` for its commands, and declares the backend events it receives by merging into `BackendEvents`:

  ```ts
  declare module "../../platform/ipc" {
    interface BackendEvents {
      "bookmarks-changed": { origin: string };
    }
  }
  ```

  `on("bookmarks-changed", …)` is then typed everywhere.
- **Actions:** keyboard shortcuts, menus and palette commands run actions by id. A feature registers the actions it owns in its `actions.ts` with `registerActions({ id: (source) => … })` from `platform/actions.ts`; `app/actions.ts` imports every feature's `actions.ts` once. Action ids and their default keys are listed in `features/shortcuts/shortcuts.ts`.

## The backend: `apps/desktop/src-tauri/src/`

One folder per domain, in the `mod.rs` style. Each domain keeps its Tauri commands in its own `commands.rs`; `app/handlers.rs` lists them all for `generate_handler!`.

| Module | What it holds |
| --- | --- |
| `main.rs` | Module declarations and `app::run()`. |
| `app/` | Startup, the builder and plugins, window creation and geometry, maintenance flags, the single-instance handler, `CmdResult` and command helpers (`command.rs`), the handler list, app-level commands. |
| `host/` | The tab host on WebView2: windows, page webviews and their events, layout and z-order, focus, accelerators, DevTools targets, page tools, downloads, autofill. |
| `session/`, `incognito/`, `profiles/` | Windows' sessions, incognito windows, browser profiles. |
| `settings/`, `shortcuts/` | Runtime settings and wallpapers; shortcut parsing, reserved keys and global hotkeys. |
| `storage/` | The SQLite worker, JSON snapshots, paths and atomic writes. |
| `history/`, `bookmarks/`, `notes/`, `downloads/`, `importer/` | Their data and commands. |
| `media/` | The video downloader: detection, HLS, DASH, MP4 merging, jobs. |
| `blocker/`, `extensions/`, `vault/`, `updates/`, `search/` | Ad blocking, extensions, the password manager and wallet, app updates, search suggestions. |
| `platform/` | WinHTTP (`net.rs`), URL validation (`urls.rs`), Windows integration (`win/`). |

Unit tests stay inline in the module they test (`#[cfg(test)] mod tests`); when a file gets long, the test module moves to `<module>/tests.rs`. No module may be named `windows` (it would clash with the `windows` crate).

## Rules the check enforces

`pnpm check:structure` (also run in CI) fails when:

- a file has more than **600 lines** (for Rust, not counting the test module). Split it along real seams. A file that genuinely can't be split may opt out with `structure: allow-large — <reason>` in its first ten lines;
- a folder holds more than **30 files**;
- an import breaks the direction above: anything outside `app/` importing `app/`, `shared/` or `platform/` importing a feature, or `platform/` importing `shared/`;
- modules import each other in a **runtime import cycle** (`import type` doesn't count). There are none today; keep it that way.

## Adding a feature

1. Create `apps/desktop/src/features/<name>/`. Put its store, components, logic and tests there.
2. Backend calls go in `features/<name>/api.ts`; on the Rust side create `src-tauri/src/<name>/mod.rs` and `commands.rs`, declare the module in `main.rs` and add the commands to `app/handlers.rs`.
3. New actions: add the id, label and default keys to `features/shortcuts/shortcuts.ts`, register the handler in `features/<name>/actions.ts`, and import that file from `app/actions.ts`.
4. If it has settings, add `<Name>Settings.svelte` in the feature and mount it in the settings page.
5. Document it: a guide in `apps/docs/src/content/docs/guides/`, and the shortcut and settings references if they changed.
