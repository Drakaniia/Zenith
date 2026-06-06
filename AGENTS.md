# AGENTS.md

## Verified Commands

- Install dependencies with `pnpm install`; this repo declares `packageManager: pnpm@10.0.0`.
- Frontend/desktop scripts are: `pnpm dev`, `pnpm build`, `pnpm preview`, `pnpm check`, `pnpm electron:dev`, `pnpm electron:start`, `pnpm electron:test`, `pnpm tauri`, `pnpm tauri:dev`, `pnpm tauri:build`, `pnpm format`. There is no `pnpm test` or `pnpm lint`.
- `pnpm electron:dev` starts Vite at `http://localhost:1420` and then launches Electron through `electron/main.mjs` after `wait-on` confirms the dev server is reachable.
- `pnpm electron:start` builds the Vite renderer and launches Electron against `dist/index.html`; `pnpm electron:test` runs Node backend tests under `electron/backend/*.test.mjs`.
- `pnpm tauri:dev` runs Tauri dev mode; `src-tauri/tauri.conf.json` starts `pnpm dev` first and expects Vite at `http://localhost:1420`.
- Vite uses strict port `1420`; with `TAURI_DEV_HOST`, HMR uses port `1421`. Vite watch ignores `src-tauri/**`.
- Rust commands run from `src-tauri`: `cargo test`, focused tests like `cargo test navigation -- --nocapture`, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.
- As of 2026-06-06, `pnpm electron:test`, `pnpm check`, and `pnpm build` pass.

## Current Shape

- This is a Svelte 5 desktop browser shell that now has an Electron main/preload backend and still retains the Tauri 2 Rust backend.
- `src` contains the active Svelte browser-chrome frontend. Verify every file path against the working tree before following older plans.
- Electron entrypoints are `electron/main.mjs`, `electron/preload.cjs`, and backend modules under `electron/backend`.
- `docs/superpowers/plans/2026-06-04-rust-svelte-browser.md` is useful design history, but verify every file path and claim against the working tree; it references frontend files and `README.md` that are not currently present.
- Rust entrypoints are `src-tauri/src/main.rs` -> `rust_browser::run()` -> `src-tauri/src/lib.rs`.
- All Tauri command registration is centralized in `src-tauri/src/lib.rs` inside `tauri::generate_handler!`; adding a command also requires exporting its module in `src-tauri/src/commands/mod.rs`.

## Electron Conventions

- The renderer talks to Electron through `window.ampuiElectron`, exposed by `electron/preload.cjs`; do not expose Node primitives directly to Svelte code.
- Electron IPC command names intentionally mirror `src/bridge/invoke.ts` where practical so feature state modules can stay runtime-agnostic.
- Electron remote page content is rendered with `<webview>` elements owned by `src/features/tabs/components/WebviewHost.svelte`; keep untrusted page content out of the trusted Svelte shell.
- Electron persistence stores `settings.json`, `bookmarks.json`, and `history.json` under `app.getPath('userData')`; writers use temp files and backup/replace behavior.

## Tauri And Rust Conventions

- The trusted chrome webview is labeled `main`; `src-tauri/capabilities/default.json` grants permissions only to `webviews: ["main"]`. Keep remote page content in child webviews without broad Tauri IPC permissions.
- Browser child webviews are implemented in `src-tauri/src/commands/webviews.rs` using Tauri `WebviewBuilder`. Labels must be `tab_` plus 32 lowercase hex characters, URLs must be `http`/`https`, and bounds must be finite with positive width/height.
- Webview events emitted to the frontend are `browser-webview-navigation`, `browser-webview-title`, and `browser-webview-load`.
- Shared JSON payload structs live in `src-tauri/src/models.rs`; keep command payloads serializable with `serde`.
- Settings, bookmarks, and history persist as JSON under `app.path().app_data_dir()`. Their writers use temp files and Windows-specific replacement/backup behavior; preserve that when changing persistence.
- Downloads are in-memory process state in `src-tauri/src/commands/downloads.rs`; downloaded files go to `dirs::download_dir()` and progress is emitted as `download-progress`.
- URL normalization defaults search queries to Yandex; setting `default_search_engine` to `google` switches search URLs to Google. Bare localhost inputs normalize to `http://`.

## Formatting

- Root `pnpm format` runs Prettier over the repo; `.prettierignore` excludes `.agents/`, lockfiles, `node_modules`, and Tauri generated/target output.
- Prettier is configured for 2 spaces, single quotes, no trailing commas, and `printWidth: 100`; Svelte files use `prettier-plugin-svelte`.

## Important

- Always Use skills
