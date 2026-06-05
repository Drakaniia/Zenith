# Rust Browser

Rust Browser is a Tauri 2 desktop browser shell. The package and Rust crate are named
`rust-browser`; the product name in Tauri config is `Rust Browser`.

The frontend uses Svelte 5, TypeScript, and Vite. The trusted browser chrome runs in the main Tauri
webview labeled `main`; remote pages are isolated in child webviews managed by Rust.

## Prerequisites

- Node.js 20 or newer.
- pnpm. This repo declares `packageManager: pnpm@10.0.0`.
- Rust stable compatible with the crate's `rust-version = "1.77.2"` in
  `src-tauri/Cargo.toml`.
- Windows: Microsoft Edge WebView2 Runtime.
- Linux: WebKitGTK and the other system libraries required by Tauri 2 for Linux builds. Follow
  Tauri's distro-specific Linux prerequisites; on Debian/Ubuntu this includes
  `libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`, `libssl-dev`,
  `libayatana-appindicator3-dev`, and `librsvg2-dev`.
- macOS: Tauri's default bundle baseline is macOS 10.13 or newer unless
  `bundle.macOS.minimumSystemVersion` is configured. This project does not currently configure a
  stricter minimum. Building on macOS also requires Xcode command line tools.

Build and runtime behavior can differ by operating system because Tauri uses the platform webview.

## Setup and Development

Commands below are shown for PowerShell because this workspace is Windows. The same project scripts
are used on macOS and Linux shells.

Install dependencies:

```powershell
pnpm install
```

Run Tauri development mode:

```powershell
pnpm tauri:dev
```

`pnpm tauri:dev` starts Tauri dev mode. Tauri starts `pnpm dev` first and expects Vite at
`http://localhost:1420`.

Cargo-only checks:

```powershell
cd src-tauri
cargo test
cargo check
```

Available root scripts are `pnpm dev`, `pnpm build`, `pnpm preview`, `pnpm check`, `pnpm tauri`,
`pnpm tauri:dev`, `pnpm tauri:build`, and `pnpm format`. There is no `pnpm test` or `pnpm lint`
script.

## Builds

Create a Tauri bundle:

```powershell
pnpm tauri:build
```

Platform bundles must be built and tested on their target operating system or on CI runners for that
operating system. Do not treat a bundle produced on one OS as verification for another OS.

## Architecture

- Trusted Svelte chrome: the application UI runs in the main Tauri webview labeled `main`. The
  default capability file scopes Tauri permissions to that trusted webview.
- Untrusted remote pages: browser tabs are child webviews created by Rust commands in
  `src-tauri/src/commands/webviews.rs`. Browser child webview labels use `tab_` followed by 32
  lowercase hex characters, and remote page URLs are limited to `http` and `https`.
- Rust command layer: shared serializable payloads live in `src-tauri/src/models.rs`. Command modules
  are exported from `src-tauri/src/commands/mod.rs`, and command registration is centralized in
  `src-tauri/src/lib.rs` inside `tauri::generate_handler!`.
- JSON persistence: settings, bookmarks, and history are stored as `settings.json`,
  `bookmarks.json`, and `history.json` under Tauri `app_data_dir()`. URL normalization defaults
  search queries to Yandex; setting `default_search_engine` to `google` switches search URLs to
  Google.
- Download event flow: downloads are in-memory process state in
  `src-tauri/src/commands/downloads.rs`. Files are written to the system Downloads directory, and
  progress updates are emitted to the frontend as `download-progress`.

The main Tauri window has a minimum size of 800x600.

## Adding a Command

When adding a Tauri command, update all layers that participate in the command path:

1. Add or update the Rust command implementation in the relevant module under
   `src-tauri/src/commands/`.
2. Export a new command module from `src-tauri/src/commands/mod.rs` if the command is in a new
   module.
3. Register the command in `src-tauri/src/lib.rs` inside `tauri::generate_handler!`.
4. Add or update shared request and response payloads in `src-tauri/src/models.rs` when the command
   needs structured data.
5. Add a TypeScript wrapper in `src/lib/utils/tauri.ts`.
6. Call the wrapper from the relevant Svelte state or component code.
7. Add focused Rust tests and frontend checks appropriate to the command's behavior.

## Known Limitations

- No extension support.
- No browser sync.
- No password manager.
- Hovered-link detection is limited and should be treated as a best-effort safety signal.
- Find-in-page is best effort and depends on script evaluation support in the platform webview.
- Platform webview behavior differs across Windows WebView2, Linux WebKitGTK, and macOS WKWebView.
