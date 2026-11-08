---
title: Building from source
description: Build Zenith, run the checks and unit tests, and drive the end-to-end suite against an isolated test instance.
---

Zenith is a pnpm monorepo. The browser lives in `apps/desktop` (Svelte UI in `src`, Rust in `src-tauri`), this website in `apps/docs`, and shared brand assets in `packages/brand`.

## Prerequisites

- Windows 11, or Windows 10 22H2
- Rust 1.85 or newer with the MSVC toolchain
- Node.js 22 or newer and pnpm
- The Microsoft Edge WebView2 runtime (it ships with Windows 11)

## Build

Run these from the repository root:

```powershell
pnpm install
pnpm app:dev      # development build with hot reload
pnpm app:build    # release build + NSIS installer
```

The installer from `pnpm app:build` lands in `apps\desktop\src-tauri\target\release\bundle\nsis`, named `Zenith_<version>_x64-setup.exe`. It installs per user, without administrator rights.

## Checks and unit tests

Also from the repository root:

```powershell
pnpm check        # svelte-check / TypeScript
pnpm test         # UI unit tests (URL handling, fuzzy search, shortcut rules, ...) and release-script tests
pnpm test:rust    # Rust unit tests
```

The Rust tests cover the blocker, URL validation, storage, shortcuts, extensions and the video downloader (HLS and DASH parsing, MP4 merging, downloads against a local server).

## End-to-end tests

The e2e suite drives the real app over the DevTools protocol, against an isolated test instance with its own profile folder and remote-debugging port. It never touches your own Zenith windows. Run the scripts from `apps/desktop`:

```powershell
cd apps/desktop
powershell -ExecutionPolicy Bypass -File scripts/dev/build.ps1
powershell -ExecutionPolicy Bypass -File scripts/dev/run.ps1 -Fresh -SkipSetup
node scripts/dev/e2e.mjs
```

- `build.ps1` builds the frontend and the debug binary. Pass `-Release` for a release build. If a `zenith.exe` is running, it is renamed out of the way so the linker can replace it.
- `run.ps1` starts the test instance on port 9444 with a profile in `%TEMP%\zenith-test-profile`. `-Fresh` starts from an empty profile, which opens the setup assistant; `-SkipSetup` marks setup as done. `-Stop` stops the instance it started.
- `e2e.mjs` accepts `--only name,name` to run selected scenarios and `--skip-restart` to skip the session-restore restarts.

The scenarios cover navigation, IPC isolation, webview placement, floating UI over pages, ad blocking, history, bookmarks, Glance, split view, drag-to-pin, Essentials across spaces, zoom, tab sleeping with scroll restore, find, permissions, popups, crash recovery, downloads, the video downloader, extensions (including Chrome Web Store installs), compact mode, spaces, the shortcut editor, search suggestions, and session restore after a normal exit and after a forced kill.

## This website

```powershell
pnpm docs:dev     # local preview with hot reload
pnpm docs:build   # static build in apps/docs/dist
```

Pages are Markdown files under `apps/docs/src/content/docs`. The sidebar is generated from the folders, so a new file shows up in its section automatically.

## Next steps

- [Architecture](../architecture/) explains how the pieces fit together.
- [Releasing and updates](../releasing/) covers cutting a release.
- [Measurements](../measurements/) lists size, startup and memory numbers and how to re-run them.
