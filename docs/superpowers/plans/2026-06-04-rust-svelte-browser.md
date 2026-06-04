# Rust Svelte Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a functional desktop browser shell at the repository root (`C:\Users\Qwenzy\Desktop\ampui`) using Rust, Tauri v2, Svelte 5 runes, persistent bookmarks/history/settings, downloads, tabs, browser chrome, keyboard shortcuts, and documentation.

**Architecture:** The Svelte app renders only trusted browser chrome and internal pages. Arbitrary web pages run in separate Tauri child webviews controlled from Rust commands, so remote pages do not receive broad Tauri IPC permissions. Rust owns persistence, metadata fetching, downloads, and native webview operations; Svelte owns tab state, panels, settings UI, and command wrappers.

**Tech Stack:** Tauri v2.11.x, Rust stable, Svelte 5.55.x, TypeScript strict mode, Vite, pnpm, reqwest 0.13, tokio 1, scraper, vanilla CSS, lucide-svelte.

---

## Reality Checks

- The original prompt asks for one HTML `<webview>` element per tab. That is an Electron pattern, not the right Tauri v2 integration point. Tauri v2 exposes native `Webview` and `WebviewWindow` APIs; the official JS Webview API can create/show/hide/resize child webviews, while Rust exposes `navigate`, `reload`, `eval`, and `url`.
- The plan adds Rust-side webview commands because the Svelte side needs operations that are present in Tauri Rust docs but not fully exposed as direct JS methods.
- The original prompt asks to test bundles on macOS, Windows, and Linux. From this Windows workspace, local verification can only prove Windows. Cross-platform proof requires CI runners or physical machines for macOS and Linux.
- The original file tree omits files that Tauri/Vite require, including `index.html` and Tauri v2 capability files. The plan adds those files.
- The original file tree omits `Toast.svelte`, but the error-handling section requires it. The plan adds `src/lib/components/Toast.svelte` and `src/lib/state/toasts.svelte.ts`.
- Hovered-link status and full find-in-page are limited by safe Tauri webview APIs. Implement them as best-effort features through Rust `eval` only after confirming the child webview can safely receive injected scripts without exposing Tauri IPC to remote content.

References checked on 2026-06-04:

- Tauri JS Webview API: https://v2.tauri.app/reference/javascript/api/namespacewebview/
- Tauri JS core invoke API: https://v2.tauri.app/reference/javascript/api/namespacecore/
- Tauri event API: https://v2.tauri.app/reference/javascript/api/namespaceevent/
- Tauri Rust Webview API: https://docs.rs/tauri/latest/tauri/webview/struct.Webview.html
- Tauri Rust WebviewWindow API: https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html
- Svelte runes docs: https://svelte.dev/docs/svelte/what-are-runes
- Svelte `$state` docs: https://svelte.dev/docs/svelte/%24state
- Svelte `$props` docs: https://svelte.dev/docs/svelte/%24props
- reqwest 0.13 docs: https://docs.rs/crate/reqwest/latest

---

## File Map

Use the repository root as the project root:

- `C:\Users\Qwenzy\Desktop\ampui`

Backend files:

- `src-tauri/Cargo.toml` - Rust package, Tauri dependencies, runtime dependencies, test-only dependencies.
- `src-tauri/build.rs` - Tauri build hook.
- `src-tauri/tauri.conf.json` - Tauri v2 app, window, bundle, security, and dev/build configuration.
- `src-tauri/capabilities/default.json` - Tauri v2 permissions for the trusted Svelte chrome only.
- `src-tauri/src/main.rs` - Binary entry point calling `rust_browser_lib::run()`.
- `src-tauri/src/lib.rs` - Tauri builder, plugins, setup, and command registration.
- `src-tauri/src/models.rs` - Shared serializable structs from the prompt plus command payload structs.
- `src-tauri/src/commands/mod.rs` - Command module exports.
- `src-tauri/src/commands/navigation.rs` - URL normalization, search-engine routing, metadata fetching, and navigation tests.
- `src-tauri/src/commands/bookmarks.rs` - Bookmark JSON persistence commands and tests.
- `src-tauri/src/commands/history.rs` - History JSON persistence commands and tests.
- `src-tauri/src/commands/downloads.rs` - Download queue commands, streaming downloads, progress events, cancellation, and tests.
- `src-tauri/src/commands/settings.rs` - Settings JSON persistence, defaults, and tests.
- `src-tauri/src/commands/webviews.rs` - Rust-side child webview creation, navigation, reload, stop/close, bounds, eval, and find-page helpers.

Frontend and app files:

- `package.json` - pnpm scripts and frontend dependencies.
- `index.html` - Vite entry HTML required by Tauri.
- `vite.config.ts` - Vite/Svelte config for Tauri.
- `tsconfig.json` - Strict TypeScript config.
- `src/main.ts` - Svelte mount entry.
- `src/App.svelte` - Browser shell, global shortcuts, panel orchestration, internal settings tab rendering.
- `src/lib/utils/url.ts` - Pure URL helpers used by UI.
- `src/lib/utils/tauri.ts` - Fully typed wrappers around every Tauri command; no raw `invoke` outside this file.
- `src/lib/state/tabs.svelte.ts` - Runes class singleton for tab state.
- `src/lib/state/bookmarks.svelte.ts` - Runes class singleton for bookmark state.
- `src/lib/state/history.svelte.ts` - Runes class singleton for history state.
- `src/lib/state/downloads.svelte.ts` - Runes class singleton for download state.
- `src/lib/state/settings.svelte.ts` - Runes class singleton for settings state.
- `src/lib/state/toasts.svelte.ts` - Runes class singleton for toast queue.
- `src/lib/components/TitleBar.svelte` - Custom draggable title bar and window buttons.
- `src/lib/components/TabBar.svelte` - Tab strip, tab context menu, new/close/duplicate actions.
- `src/lib/components/NavigationBar.svelte` - Back/forward/reload/address/search/bookmark/menu UI.
- `src/lib/components/WebViewContainer.svelte` - Child webview lifecycle and geometry bridge.
- `src/lib/components/BookmarksPanel.svelte` - Bookmark panel.
- `src/lib/components/HistoryPanel.svelte` - History panel.
- `src/lib/components/DownloadManager.svelte` - Download progress panel.
- `src/lib/components/FindBar.svelte` - Find UI wired to best-effort Rust webview commands.
- `src/lib/components/StatusBar.svelte` - Loading/security/status bar.
- `src/lib/components/SettingsPage.svelte` - Internal settings UI.
- `src/lib/components/Toast.svelte` - Non-intrusive error notification queue.
- `README.md` - Setup, development, build, architecture, limitations.

---

## Task 1: Project Scaffold And Config

**Files:**

- Create: `package.json`
- Create: `index.html`
- Create: `vite.config.ts`
- Create: `tsconfig.json`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/src/main.rs` - minimal Cargo target needed for dependency resolution verification

- [x] **Step 1: Create the project directories**

Run:

```powershell
New-Item -ItemType Directory -Force src-tauri/src/commands
New-Item -ItemType Directory -Force src-tauri/capabilities
New-Item -ItemType Directory -Force src/lib/components
New-Item -ItemType Directory -Force src/lib/state
New-Item -ItemType Directory -Force src/lib/utils
```

Expected: directories exist under the repository root.

- [x] **Step 2: Add frontend package metadata**

Implement `package.json` with scripts:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "check": "svelte-check --tsconfig ./tsconfig.json",
    "tauri": "tauri",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  }
}
```

Use dependencies: `@tauri-apps/api`, `@tauri-apps/plugin-shell`, `lucide-svelte`, `svelte`. Use dev dependencies: `@sveltejs/vite-plugin-svelte`, `@tauri-apps/cli`, `svelte-check`, `typescript`, `vite`.

- [x] **Step 3: Add Vite and TypeScript config**

Set `tsconfig.json` to strict mode with `noImplicitAny`, `strictNullChecks`, and Svelte type support. Set `vite.config.ts` to use `svelte()` and Tauri-friendly server options.

- [x] **Step 4: Add Tauri Rust dependencies**

Implement `src-tauri/Cargo.toml` with the prompt dependencies and the minimal extra dependencies needed for correctness:

```toml
[dependencies]
tauri = { version = "2.11", features = ["devtools"] }
tauri-plugin-fs = "2"
tauri-plugin-dialog = "2"
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.13", features = ["json", "stream"] }
scraper = "0.22"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"
urlencoding = "2"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dev-dependencies]
tempfile = "3"
```

- [x] **Step 5: Add Tauri v2 app config and capabilities**

Configure one main window for the trusted Svelte chrome with hidden native title bar, minimum size 800x600, dev URL `http://localhost:1420`, frontend dist `../dist`, and permissions for core event/listen, window operations, shell open, dialog, and child webview management.

- [x] **Step 6: Verify dependency resolution**

Run:

```powershell
# from repository root
pnpm install
cd src-tauri
cargo metadata --format-version 1
```

Expected: pnpm installs dependencies; Cargo resolves all dependencies without version errors.

Task 1 execution notes:

- `@tauri-apps/plugin-shell` was set to `~2.3.5` because npm reported no matching `~2.4.0` release.
- `src-tauri/src/main.rs` was added during Task 1 because `cargo metadata` fails when a manifest has no Rust target.

---

## Task 2: Rust Models

**Files:**

- Create: `src-tauri/src/models.rs`

- [x] **Step 1: Add all prompt models exactly**

Define `Tab`, `Bookmark`, `HistoryEntry`, `DownloadItem`, and `AppSettings`, each deriving `serde::Serialize`, `serde::Deserialize`, `Clone`, and `Debug`.

- [x] **Step 2: Add command payload models**

Add `PageMetadata`, `DownloadProgress`, `WebviewBounds`, `WebviewCreateRequest`, `WebviewFindRequest`, and `FindResult`. Derive `Serialize`, `Deserialize`, `Clone`, and `Debug` where applicable.

- [x] **Step 3: Add settings default constructor**

Implement `impl Default for AppSettings` returning:

```rust
AppSettings {
    default_search_engine: "yandex".to_string(),
    homepage: "https://yandex.com".to_string(),
    theme: "dark".to_string(),
    javascript_enabled: true,
    show_status_bar: true,
}
```

- [x] **Step 4: Test model serialization**

Add inline Rust tests that serialize and deserialize one `Bookmark`, one `HistoryEntry`, one `DownloadItem`, and default `AppSettings`.

Run:

```powershell
cd src-tauri
cargo test models -- --nocapture
```

Expected: all model serialization tests pass.

Task 2 execution notes:
- `src-tauri/src/main.rs` now declares `mod models;` so inline model tests are discoverable before the later `lib.rs` task exists.
- `src-tauri/icons/icon.ico` was added because `cargo test models -- --nocapture` failed in Tauri's build script without the Windows resource icon.

---

## Task 3: Settings Persistence Commands

**Files:**

- Create: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: Write settings tests first**

Add tests for:

- missing `settings.json` returns `AppSettings::default()`
- saving settings writes valid JSON
- invalid settings file falls back to default and does not panic

- [x] **Step 2: Implement file-path helper**

Inside `settings.rs`, implement a private `settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String>` using `app.path().app_data_dir()` through `tauri::Manager`.

- [x] **Step 3: Implement commands**

Implement:

```rust
#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String>

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String>
```

Create the app data directory before writes. Write JSON atomically enough for desktop use by writing to a temp file in the same directory and renaming it.

- [x] **Step 4: Run tests**

Run:

```powershell
cd src-tauri
cargo test settings -- --nocapture
```

Expected: settings tests pass.

Task 3 execution notes:

- `src-tauri/src/lib.rs` and `src-tauri/src/commands/mod.rs` did not exist yet, so task 3 created them while registering the settings commands.
- `src-tauri/icons/icon.ico` was replaced with a valid generated ICO because Tauri's compile-time context macro failed to decode the previous malformed icon before settings tests could run.
- `cargo test settings -- --nocapture` passed with 4 tests.

---

## Task 4: Navigation And Metadata Commands

**Files:**

- Create: `src-tauri/src/commands/navigation.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: Write URL normalization tests**

Cover these cases:

- `example.com` returns `https://example.com`
- `https://example.com/path` stays unchanged
- `localhost:3000` stays navigable as `http://localhost:3000` or `https://localhost:3000` only after an explicit decision in code comments
- `rust tauri docs` returns a search URL
- `no-dot-word` returns a search URL
- default search engine switches between Yandex and Google based on settings

- [x] **Step 2: Implement pure normalization helper**

Implement `normalize_url_with_settings(input: &str, settings: &AppSettings) -> Result<String, String>`. Use `urlencoding::encode` for query encoding. Search engines are `yandex` using `https://yandex.com/search/?text={query}` and `google` using `https://www.google.com/search?q={query}`. Reject empty input with a descriptive string error.

- [x] **Step 3: Implement Tauri command**

Implement:

```rust
#[tauri::command]
pub fn normalize_url(app: tauri::AppHandle, url: String) -> Result<String, String>
```

The frontend wrapper still calls it with only `{ url }`; Tauri injects `AppHandle`.

- [x] **Step 4: Write metadata extraction tests**

Add pure tests for an HTML string with `<title>` and `<link rel="icon" href="/favicon.ico">`. Expected metadata title is trimmed and favicon resolves relative to the page URL.

- [x] **Step 5: Implement `fetch_page_metadata`**

Use `reqwest::Client` with timeout and user agent. Fetch HTML, parse title and icon with `scraper`, resolve favicon against the final URL, and return `PageMetadata`. Return `Ok(PageMetadata { title: url.clone(), favicon_url: None })` for pages that fetch but have no title.

- [x] **Step 6: Run tests**

Run:

```powershell
cd src-tauri
cargo test navigation -- --nocapture
```

Expected: all navigation tests pass without real network dependency except the command itself.

---

## Task 5: Bookmark Commands

**Files:**

- Create: `src-tauri/src/commands/bookmarks.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: Write bookmark persistence tests**

Cover:

- missing file returns empty list
- save creates a bookmark with UUID and ISO timestamp
- duplicate URL updates title instead of creating duplicate entries
- delete removes only matching ID
- `is_bookmarked` matches normalized URL string exactly

- [x] **Step 2: Implement JSON helpers**

Private helpers:

```rust
fn read_bookmarks(path: &Path) -> Result<Vec<Bookmark>, String>
fn write_bookmarks(path: &Path, bookmarks: &[Bookmark]) -> Result<(), String>
```

Return `Vec::new()` when the file does not exist.

- [x] **Step 3: Implement commands**

Implement the exact prompt commands:

```rust
#[tauri::command]
pub fn get_bookmarks(app: tauri::AppHandle) -> Result<Vec<Bookmark>, String>
#[tauri::command]
pub fn save_bookmark(app: tauri::AppHandle, title: String, url: String) -> Result<Bookmark, String>
#[tauri::command]
pub fn delete_bookmark(app: tauri::AppHandle, id: String) -> Result<(), String>
#[tauri::command]
pub fn is_bookmarked(app: tauri::AppHandle, url: String) -> Result<bool, String>
```

- [x] **Step 4: Run tests**

Run:

```powershell
cd src-tauri
cargo test bookmarks -- --nocapture
```

Expected: all bookmark tests pass.

Task 5 execution notes:

- `save_bookmark` treats duplicate URLs as exact string matches and updates the existing title while preserving the original bookmark ID and `created_at`.
- Bookmark writes use the same temp-file pattern as settings; on Windows, the existing file is first moved to a backup and restored if the temp rename fails.
- `cargo test bookmarks -- --nocapture` passed with 6 tests.
- `cargo test` passed with 22 tests after the Task 5 changes.

---

## Task 6: History Commands

**Files:**

- Create: `src-tauri/src/commands/history.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: Write history tests**

Cover:

- `get_history` sorts most recent first
- `add_history_entry` records title, URL, UUID, timestamp
- adding the same URL twice creates separate visit entries
- `search_history` matches case-insensitive title and URL
- `delete_history_entry` removes only the selected ID
- `clear_history` writes an empty list

- [x] **Step 2: Implement JSON helpers**

Private helpers:

```rust
fn read_history(path: &Path) -> Result<Vec<HistoryEntry>, String>
fn write_history(path: &Path, entries: &[HistoryEntry]) -> Result<(), String>
```

- [x] **Step 3: Implement commands**

Implement the exact prompt commands:

```rust
#[tauri::command]
pub fn get_history(app: tauri::AppHandle) -> Result<Vec<HistoryEntry>, String>
#[tauri::command]
pub fn add_history_entry(app: tauri::AppHandle, title: String, url: String) -> Result<(), String>
#[tauri::command]
pub fn search_history(app: tauri::AppHandle, query: String) -> Result<Vec<HistoryEntry>, String>
#[tauri::command]
pub fn clear_history(app: tauri::AppHandle) -> Result<(), String>
#[tauri::command]
pub fn delete_history_entry(app: tauri::AppHandle, id: String) -> Result<(), String>
```

- [x] **Step 4: Run tests**

Run:

```powershell
cd src-tauri
cargo test history -- --nocapture
```

Expected: all history tests pass.

Task 6 execution notes:

- Added `src-tauri/src/commands/history.rs` with JSON persistence helpers, Tauri commands, tests, and module/handler registration.
- History writes are guarded by an in-process mutex to avoid lost read-modify-write updates.
- If `history.json` is missing but `history.json.bak` exists after an interrupted Windows replacement, `read_history` recovers from the backup.
- `cargo test history -- --nocapture` passed with 11 tests after review fixes.

---

## Task 7: Download Commands

**Files:**

- Create: `src-tauri/src/commands/downloads.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: Write queue tests**

Cover:

- `get_downloads` returns current in-memory queue snapshot
- `clear_completed_downloads` removes completed downloads and keeps active/failed/paused
- `cancel_download` marks an existing download as failed or paused consistently and sets a cancellation flag
- cancelling a missing ID returns a clear error

- [x] **Step 2: Implement in-memory queue**

Use:

```rust
static DOWNLOADS: OnceLock<Mutex<Vec<DownloadRecord>>> = OnceLock::new();
```

Where `DownloadRecord` stores a public `DownloadItem` plus `Arc<AtomicBool>` cancellation state and optional local path.

- [x] **Step 3: Implement progress event payload**

Emit `download-progress` with `DownloadProgress` after every chunk and at completion/failure. Use `tauri::Emitter` for `app.emit("download-progress", payload)`.

- [x] **Step 4: Implement streaming download**

`start_download` should:

- create a UUID
- derive Downloads folder with `dirs::download_dir()`
- reject path separators in `filename`
- create the queue item with `downloading` status
- spawn an async task
- use `reqwest::Client`
- stream chunks with `response.chunk().await`
- update `downloaded_bytes`
- write to disk with `tokio::fs::File`
- set `completed` on success or `failed` on error/cancel

- [x] **Step 5: Implement commands**

Implement:

```rust
#[tauri::command]
pub fn get_downloads() -> Result<Vec<DownloadItem>, String>
#[tauri::command]
pub async fn start_download(app: tauri::AppHandle, url: String, filename: String) -> Result<String, String>
#[tauri::command]
pub fn cancel_download(id: String) -> Result<(), String>
#[tauri::command]
pub fn clear_completed_downloads() -> Result<(), String>
```

- [x] **Step 6: Run tests**

Run:

```powershell
cd src-tauri
cargo test downloads -- --nocapture
```

Expected: queue tests pass. Network download behavior is verified later with a manual smoke test.

Task 7 execution notes:

- Added `src-tauri/src/commands/downloads.rs` with the in-memory queue, streaming download task, cancellation state, progress events, and command registration.
- `cancel_download` now returns and emits a failed `DownloadProgress` payload immediately when an `AppHandle` has been captured by `start_download`.
- The download loop races chunk reads against cancellation polling so cancellation is not limited to arriving only after the next chunk.
- `local_path` is intentionally retained for the later download-manager/open-file task and annotated as reserved.
- `cargo test downloads -- --nocapture` passed with 4 tests after review fixes.

---

## Task 8: Rust Webview Commands

**Files:**

- Create: `src-tauri/src/commands/webviews.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: Define the command surface**

Add commands:

```rust
create_browser_webview(app, label, url, bounds, javascript_enabled) -> Result<(), String>
navigate_browser_webview(app, label, url) -> Result<(), String>
reload_browser_webview(app, label) -> Result<(), String>
close_browser_webview(app, label) -> Result<(), String>
show_browser_webview(app, label) -> Result<(), String>
hide_browser_webview(app, label) -> Result<(), String>
set_browser_webview_bounds(app, label, bounds) -> Result<(), String>
get_browser_webview_url(app, label) -> Result<String, String>
eval_browser_webview(app, label, script) -> Result<(), String>
find_in_browser_webview(app, request) -> Result<FindResult, String>
clear_browser_webview_find(app, label) -> Result<(), String>
```

- [x] **Step 2: Implement `create_browser_webview`**

Use Tauri Rust child webview APIs where possible. Configure each child webview with:

- unique label `tab_<uuid without hyphens>`
- initial remote URL
- exact bounds from the Svelte container
- JavaScript disabled when settings say false
- no permission grants for remote content
- navigation handler that emits a safe event to the main chrome when URL changes

- [x] **Step 3: Implement navigation and visibility**

Use Tauri Rust `navigate`, `reload`, `show`, `hide`, `close`, `set_bounds`/position/size APIs. Convert all Tauri errors to strings with command and label context.

- [x] **Step 4: Implement best-effort find**

Use `eval` or `eval_with_callback` to inject a page-local text-search script. Return `FindResult { active_match, total_matches }`. If callback support is unavailable on the current platform, return an explicit error string so the UI can toast "Find in page is unavailable on this platform".

- [ ] **Step 5: Manual webview smoke test**

After frontend integration, verify that:

- a child webview appears below the chrome
- navigating updates URL/title metadata
- reload works
- closing the tab closes the child webview
- remote pages cannot invoke privileged Tauri commands

Task 8 execution notes:

- Added `src-tauri/src/commands/webviews.rs` and registered all 11 Rust webview commands.
- Enabled Tauri's `unstable` feature because local Tauri 2.11.2 gates `Window::add_child` and `Manager::get_webview` behind that feature.
- Remote webview labels are validated as `tab_` plus 32 lowercase hex characters, command/navigation URLs are restricted to HTTP(S), remote-initiated navigation rejects unsupported schemes, and bounds reject non-finite or non-positive dimensions.
- Added unit tests for URL policy, label validation, bounds validation, and find-script JSON escaping.
- `cargo test webviews -- --nocapture` passed with 4 tests after review fixes.
- Manual webview smoke testing remains unchecked because the frontend integration that renders/manages child webviews is scheduled later in Task 16 and Task 23.

---

## Task 9: Tauri Builder And Command Registration

**Files:**

- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/commands/mod.rs`
- Modify: all command modules

- [x] **Step 1: Export command modules**

`commands/mod.rs` should export:

```rust
pub mod bookmarks;
pub mod downloads;
pub mod history;
pub mod navigation;
pub mod settings;
pub mod webviews;
```

- [x] **Step 2: Register plugins**

In `lib.rs`, use `tauri::Builder::default()` and register:

- `tauri_plugin_fs::init()`
- `tauri_plugin_dialog::init()`
- `tauri_plugin_shell::init()`

- [x] **Step 3: Register invoke handler**

Register every command from navigation, bookmarks, history, downloads, settings, and webviews in one `tauri::generate_handler![...]` block.

- [x] **Step 4: Verify Rust build**

Run:

```powershell
cd src-tauri
cargo fmt
cargo test
cargo check
```

Expected: formatting applies cleanly; tests pass; `cargo check` passes.

Task 9 execution notes:

- `src-tauri/src/commands/mod.rs` exports bookmarks, downloads, history, navigation, settings, and webviews.
- `src-tauri/src/lib.rs` registers the fs, dialog, and shell plugins, and registers all commands from the six command modules in one `tauri::generate_handler![...]` block.
- Added `#[cfg_attr(mobile, tauri::mobile_entry_point)]` to `run()` to keep the Tauri v2 `lib.rs` entry point mobile-compatible.
- `cargo fmt` exited 0.
- `cargo test` passed with 40 tests.
- `cargo check` exited 0.

---

## Task 10: Typed Frontend Tauri Wrappers And URL Utilities

**Files:**

- Create: `src/lib/utils/tauri.ts`
- Create: `src/lib/utils/url.ts`

- [ ] **Step 1: Define TypeScript types matching Rust**

In `tauri.ts`, define interfaces for:

- `Tab`
- `Bookmark`
- `HistoryEntry`
- `DownloadItem`
- `AppSettings`
- `PageMetadata`
- `DownloadProgress`
- `WebviewBounds`
- `FindResult`

Use `unknown` only where external API payload narrowing is required. Do not use `any`.

- [ ] **Step 2: Wrap every invoke call**

Use:

```ts
import { invoke } from '@tauri-apps/api/core';
```

Expose one typed function per Rust command. Every function catches errors at the state/component layer, not inside the wrapper, so callers can show contextual toasts.

- [ ] **Step 3: Add pure URL helpers**

In `url.ts`, implement:

- `isInternalUrl(url: string): boolean`
- `isSecureUrl(url: string): boolean`
- `displayUrl(url: string): string`
- `domainFromUrl(url: string): string`
- `suggestFilenameFromUrl(url: string): string`

- [ ] **Step 4: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected initially may fail until components exist. Type errors in `utils` must be fixed before continuing.

---

## Task 11: Runes State Modules

**Files:**

- Create: `src/lib/state/tabs.svelte.ts`
- Create: `src/lib/state/bookmarks.svelte.ts`
- Create: `src/lib/state/history.svelte.ts`
- Create: `src/lib/state/downloads.svelte.ts`
- Create: `src/lib/state/settings.svelte.ts`
- Create: `src/lib/state/toasts.svelte.ts`

- [ ] **Step 1: Use class-based runes state**

Each state file should export a singleton class instance. Use class fields like:

```ts
class TabsState {
  tabs = $state<BrowserTab[]>([]);
  activeTabId = $state<string | null>(null);
  activeTab = $derived(this.tabs.find((tab) => tab.id === this.activeTabId) ?? null);
}

export const tabsState = new TabsState();
```

Do not use Svelte stores unless a specific Tauri listener needs an unsubscribe bridge.

- [ ] **Step 2: Implement tab state**

`tabs.svelte.ts` should include:

- `openTab(url?: string, title?: string): BrowserTab`
- `openInternalTab(url: 'browser://settings'): BrowserTab`
- `closeTab(id: string): BrowserTab | null`
- `setActiveTab(id: string): void`
- `updateTab(id: string, patch: Partial<BrowserTab>): void`
- `duplicateTab(id: string): BrowserTab | null`
- `closeOtherTabs(id: string): string[]`
- `nextTab(): void`
- `previousTab(): void`
- `tabByIndex(index: number): void`
- `lastTab(): void`

The close functions must return closed tab IDs so `WebViewContainer` can close native child webviews.

- [ ] **Step 3: Implement backend-backed states**

`bookmarks`, `history`, `downloads`, and `settings` state modules must call only typed wrappers from `utils/tauri.ts`, wrap each call in `try/catch`, update `$state` on success, and call `toastsState.error(...)` on failure.

- [ ] **Step 4: Implement toasts state**

`toasts.svelte.ts` should support:

- `success(message: string)`
- `error(message: string)`
- `info(message: string)`
- `remove(id: string)`
- automatic removal after 4000 ms

- [ ] **Step 5: Run typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: state files typecheck once placeholder component imports are not present.

---

## Task 12: App Entry, Global CSS, And Toasts

**Files:**

- Create: `src/main.ts`
- Create: `src/App.svelte`
- Create: `src/lib/components/Toast.svelte`

- [ ] **Step 1: Implement `main.ts`**

Mount `App` into `document.getElementById('app')`. Throw a clear error if the element is missing.

- [ ] **Step 2: Add global CSS in `App.svelte`**

Use CSS custom properties for dark and light themes. Include:

- background `#0f0f10` / `#f5f5f7`
- surface `#1a1a1c` / `#ffffff`
- border `#2a2a2e` / `#d1d1d6`
- accent `#3b82f6` / `#2563eb`
- text primary/secondary as prompt specifies
- focus-visible outlines
- styled scrollbars
- fixed chrome layout variables for 36px tab bar, 48px nav bar, 24px status bar

- [ ] **Step 3: Implement `Toast.svelte`**

Render toasts from `toastsState.items`. Use Svelte 5 event attributes (`onclick`) and `$props()` if props are needed. No `on:` syntax and no `export let`.

- [ ] **Step 4: Initial typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: failures only for components not yet created. Fix `App`, `main`, and `Toast` issues immediately.

---

## Task 13: Title Bar

**Files:**

- Create: `src/lib/components/TitleBar.svelte`
- Modify: `src/App.svelte`

- [ ] **Step 1: Implement props and active title**

Use `$props()` for:

```ts
interface Props {
  title: string;
}
```

Render current tab title or `Rust Browser`.

- [ ] **Step 2: Implement draggable region**

Add `data-tauri-drag-region` to the title region. Do not put it on interactive buttons.

- [ ] **Step 3: Implement window buttons**

Use:

```ts
import { getCurrentWindow } from '@tauri-apps/api/window';
```

Wire minimize, maximize/restore, and close. Catch errors and show a toast.

- [ ] **Step 4: Style platform positions**

Detect macOS with `navigator.userAgent` and place controls left on macOS, right elsewhere. Keep hit targets stable at 32px.

- [ ] **Step 5: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no errors from `TitleBar.svelte`.

---

## Task 14: Tab Bar

**Files:**

- Create: `src/lib/components/TabBar.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/state/tabs.svelte.ts`

- [ ] **Step 1: Render tabs**

Each tab shows favicon or globe icon, loading spinner, title with ellipsis, and close button. Active tab uses accent border/background.

- [ ] **Step 2: Implement tab actions**

Wire:

- click tab -> `tabsState.setActiveTab`
- close button -> `tabsState.closeTab`
- plus button -> `tabsState.openTab(settingsState.settings.homepage)`
- duplicate -> `tabsState.duplicateTab`
- close other tabs -> `tabsState.closeOtherTabs`

- [ ] **Step 3: Implement context menu**

Use component-local `$state` for menu position and selected tab ID. Provide Reload, Duplicate Tab, Close Tab, and Close Other Tabs.

- [ ] **Step 4: Keep layout stable**

Use fixed tab height 36px, horizontal overflow, and fixed-width close/icon buttons so text never shifts controls.

- [ ] **Step 5: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no errors from `TabBar.svelte`.

---

## Task 15: Navigation Bar

**Files:**

- Create: `src/lib/components/NavigationBar.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/state/history.svelte.ts`
- Modify: `src/lib/state/bookmarks.svelte.ts`

- [ ] **Step 1: Implement props**

Use `$props()` for callbacks:

```ts
interface Props {
  onNavigate: (url: string) => Promise<void>;
  onReload: () => Promise<void>;
  onStop: () => Promise<void>;
  onBack: () => Promise<void>;
  onForward: () => Promise<void>;
  onOpenPanel: (panel: 'bookmarks' | 'history' | 'downloads' | 'settings' | 'find') => void;
}
```

- [ ] **Step 2: Implement address input**

Use component `$state` for input text, focus state, validation error, and suggestion list. On focus, select all text. On Enter, call typed `normalizeUrl`, then `onNavigate`.

- [ ] **Step 3: Implement suggestions**

Call `historyState.search(input)` after a short debounce. Show recent history title/URL. Clicking a suggestion navigates and closes the dropdown.

- [ ] **Step 4: Implement bookmark star**

Use `bookmarksState.checkIsBookmarked(activeTab.url)` in `$effect`. Toggle by calling `bookmarksState.addBookmark` or `bookmarksState.removeBookmark`.

- [ ] **Step 5: Implement menu**

Menu actions: New Tab, Bookmarks, History, Downloads, Settings, Find in Page. Use lucide icons and no text-only buttons where an icon is clearer.

- [ ] **Step 6: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no errors from `NavigationBar.svelte`.

---

## Task 16: WebView Container

**Files:**

- Create: `src/lib/components/WebViewContainer.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/state/tabs.svelte.ts`
- Modify: `src/lib/utils/tauri.ts`

- [ ] **Step 1: Implement child webview lifecycle**

Use a container `div` with `bind:this`. For each non-internal tab:

- create a native child webview through `createBrowserWebview`
- use the tab's stable webview label
- hide inactive tab webviews
- show active tab webview
- close webviews for closed tabs

- [ ] **Step 2: Implement geometry sync**

Use `ResizeObserver` and window resize handling inside `$effect`. Convert container bounds to Tauri logical coordinates. Call `setBrowserWebviewBounds` whenever the content area changes.

- [ ] **Step 3: Implement navigation methods**

Expose callbacks to `App.svelte`:

- `navigateActive(url)`
- `reloadActive()`
- `stopActive()`
- `backActive()`
- `forwardActive()`
- `findActive(request)`

Because native back/forward may not be directly exposed, implement back/forward initially with tab-local history stacks updated from app-driven navigations and webview URL events. Be explicit in code comments that this tracks app-visible navigations, not every in-page redirect.

- [ ] **Step 4: Update metadata**

After navigation starts, set `is_loading = true`. After command returns and metadata fetch resolves, update title/favicon, add history entry, and set `is_loading = false`. On error, update tab title to `Load failed` and show toast.

- [ ] **Step 5: Validate remote content isolation**

Do not grant broad Tauri invoke permissions to remote webviews. Verify a remote page cannot access trusted app command wrappers.

- [ ] **Step 6: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no TypeScript errors from webview orchestration.

---

## Task 17: Find Bar And Status Bar

**Files:**

- Create: `src/lib/components/FindBar.svelte`
- Create: `src/lib/components/StatusBar.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/utils/tauri.ts`

- [ ] **Step 1: Implement FindBar**

Use `$props()` for:

```ts
interface Props {
  open: boolean;
  onClose: () => void;
  onFind: (
    query: string,
    direction: 'next' | 'previous',
    caseSensitive: boolean
  ) => Promise<FindResult>;
  onClear: () => Promise<void>;
}
```

Render input, match count, previous/next buttons, case-sensitive toggle, and close button. Escape closes the bar.

- [ ] **Step 2: Implement find behavior**

Use `$effect` to run find when query/case state changes and open is true. Catch unavailable-platform errors and show a toast once per open session.

- [ ] **Step 3: Implement StatusBar**

Render security state from active URL:

- HTTPS: green lock and `Secure`
- HTTP: warning and `Not Secure`
- `browser://`: grey internal indicator

Show loading text from active tab state.

- [ ] **Step 4: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no errors from `FindBar` or `StatusBar`.

---

## Task 18: Bookmarks And History Panels

**Files:**

- Create: `src/lib/components/BookmarksPanel.svelte`
- Create: `src/lib/components/HistoryPanel.svelte`
- Modify: `src/App.svelte`

- [ ] **Step 1: Implement BookmarksPanel**

Props:

```ts
interface Props {
  open: boolean;
  onClose: () => void;
  onNavigate: (url: string) => Promise<void>;
}
```

Load bookmarks on open. Render add-current button when active URL is not bookmarked, list entries with favicon/title/URL, and delete buttons. Empty state must be simple and non-decorative.

- [ ] **Step 2: Implement HistoryPanel**

Props match `BookmarksPanel`. Load history on open. Search input calls `historyState.search`. Group entries into Today, Yesterday, This Week, and Earlier using local date comparisons.

- [ ] **Step 3: Implement clear confirmation**

Use `tauri-plugin-dialog` through a typed wrapper if available, or implement an in-app confirmation dialog. Clearing history must call `historyState.clear()`, refresh the list, and toast success/error.

- [ ] **Step 4: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no errors from both panels.

---

## Task 19: Download Manager

**Files:**

- Create: `src/lib/components/DownloadManager.svelte`
- Modify: `src/lib/state/downloads.svelte.ts`
- Modify: `src/App.svelte`
- Modify: `src/lib/utils/tauri.ts`

- [ ] **Step 1: Listen for progress events**

Use:

```ts
import { listen } from '@tauri-apps/api/event';
```

Listen for `download-progress`, narrow the payload to `DownloadProgress`, update state, and unsubscribe on teardown.

- [ ] **Step 2: Render download rows**

Show filename, progress bar, bytes downloaded / total, status badge, and action buttons. If total bytes are unknown, show an indeterminate bar.

- [ ] **Step 3: Wire actions**

Implement:

- cancel active download
- clear completed
- open file or show in folder using shell plugin when a local path is available

If the backend does not expose local path yet, add `local_path: Option<String>` to `DownloadItem` before implementing frontend actions.

- [ ] **Step 4: Manual download smoke test**

Run the app and download a small public file. Expected: progress event updates, file appears in system Downloads, final status is completed.

---

## Task 20: Settings Page

**Files:**

- Create: `src/lib/components/SettingsPage.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/state/settings.svelte.ts`

- [ ] **Step 1: Render internal tab only**

When active tab URL is `browser://settings`, hide child webviews and render `SettingsPage` inside the content area.

- [ ] **Step 2: Implement settings controls**

Render:

- default search engine selector with Yandex and Google options only
- homepage URL input
- theme segmented control
- JavaScript enabled toggle
- show status bar toggle
- clear history button
- clear browsing data button that calls `clearAllBrowsingData` for browser webviews where possible
- app version/about section

- [ ] **Step 3: Auto-save changes**

Use `$effect` with a debounce. Every settings change calls `settingsState.save()`. On error, show toast and keep local state visible.

- [ ] **Step 4: Typecheck**

Run:

```powershell
# from repository root
pnpm check
```

Expected: no errors from `SettingsPage`.

---

## Task 21: App Shell And Keyboard Shortcuts

**Files:**

- Modify: `src/App.svelte`
- Modify: state and component files as needed

- [ ] **Step 1: Compose the shell**

Render in order:

- `TitleBar`
- `TabBar`
- `NavigationBar`
- content area containing `WebViewContainer` or `SettingsPage`
- `FindBar`
- slide-over panels
- `DownloadManager`
- `StatusBar`
- `Toast`

- [ ] **Step 2: Implement startup load**

On first app load:

- load settings
- load bookmarks
- load history
- load downloads
- open homepage tab

Use `$effect` for lifecycle work and return cleanup functions where listeners are registered.

- [ ] **Step 3: Implement global shortcuts**

In one `$effect`, add a `keydown` listener for all prompt shortcuts:

- `Ctrl+T`
- `Ctrl+W`
- `Ctrl+Tab`
- `Ctrl+Shift+Tab`
- `Ctrl+L`
- `Ctrl+R` / `F5`
- `Ctrl+Shift+R`
- `Escape`
- `Alt+Left`
- `Alt+Right`
- `Ctrl+F`
- `Ctrl+H`
- `Ctrl+D`
- `Ctrl+,`
- `Ctrl+Shift+J`
- `Ctrl+1` through `Ctrl+8`
- `Ctrl+9`

Prevent default only when the browser shell handles the shortcut.

- [ ] **Step 4: Verify no Svelte 4 patterns**

Search:

```powershell
# from repository root
rg "export let|\\$:" src
rg "on:" src
```

Expected: no matches except inside explanatory README text if any.

- [ ] **Step 5: Full frontend typecheck and build**

Run:

```powershell
# from repository root
pnpm check
pnpm build
```

Expected: both pass.

---

## Task 22: README

**Files:**

- Create: `README.md`

- [ ] **Step 1: Document prerequisites**

Include Node.js 20+, pnpm, Rust stable matching the project, Windows WebView2, Linux WebKitGTK dependencies, and macOS baseline.

- [ ] **Step 2: Document setup and development**

Include:

```powershell
pnpm install
pnpm tauri:dev
```

And Cargo-only checks:

```powershell
cd src-tauri
cargo test
cargo check
```

- [ ] **Step 3: Document builds**

Include:

```powershell
pnpm tauri:build
```

State honestly that platform bundles must be built/tested on their target OS or CI runners.

- [ ] **Step 4: Document architecture**

Explain:

- trusted Svelte chrome
- untrusted remote pages in child webviews
- Rust command layer
- JSON persistence files
- download event flow

- [ ] **Step 5: Document adding a command**

List the exact required edits: Rust command module, `mod.rs`, `lib.rs` handler, `utils/tauri.ts` wrapper, state/component caller, tests.

- [ ] **Step 6: Document known limitations**

Include no extensions, no sync, no password manager, limited safe hovered-link detection, best-effort find-in-page, and platform-specific webview behavior differences.

---

## Task 23: Verification And Packaging

**Files:**

- All project files

- [ ] **Step 1: Run static checks**

Run:

```powershell
# from repository root
pnpm check
pnpm build
cd src-tauri
cargo fmt --check
cargo test
cargo check
```

Expected: all pass.

- [ ] **Step 2: Run Windows Tauri dev smoke test**

Run:

```powershell
# from repository root
pnpm tauri:dev
```

Manual checks:

- app opens at 800x600 or larger
- custom title bar controls work
- homepage tab loads
- address navigation works
- metadata updates title/favicon when available
- bookmarks persist after restart
- history persists after restart
- settings persist after restart
- download progress updates
- settings tab opens as `browser://settings`
- shortcuts work

- [ ] **Step 3: Build Windows bundle**

Run:

```powershell
# from repository root
pnpm tauri:build
```

Expected: Windows bundle completes. Record output artifact path in the final implementation summary.

- [ ] **Step 4: Cross-platform verification**

Use macOS and Linux machines or CI runners to run:

```bash
pnpm install
pnpm check
pnpm build
cd src-tauri && cargo test && cargo check
cd .. && pnpm tauri:build
```

Expected: all pass on each OS. If not available, final implementation summary must state that cross-platform build verification was not performed.

---

## Self-Review

- Spec coverage: The plan covers all requested Rust models, Tauri commands, settings, persistence, downloads, frontend state files, UI components, shortcuts, error toasts, README, and verification. It adds missing required support files and webview commands.
- Placeholder scan: No task is left as "TBD" or "implement later". Known platform/API limits are documented as explicit implementation constraints.
- Type consistency: Rust `AppSettings`, `PageMetadata`, `DownloadItem`, `DownloadProgress`, `WebviewBounds`, and TypeScript counterparts are named consistently across backend, wrappers, and components.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-06-04-rust-svelte-browser.md`. Two execution options:

**1. Subagent-Driven (recommended)** - Dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** - Execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

Choose one before implementation starts.
