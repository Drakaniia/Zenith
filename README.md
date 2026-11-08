# Zenith

A lightweight, Zen-style web browser for Windows 11, built with Tauri 2 (Rust) and a Svelte 5 interface on top of the system's Microsoft Edge WebView2 engine.

Documentation: https://drakaniia.github.io/Zenith/

- **Vertical tabs** with favicons, audio indicators, drag to reorder, middle-click to close, sleeping and crashed states.
- **Essentials and pinned tabs**: an icon grid shared across spaces, plus per-space pinned tabs that remember their original page.
- **Spaces**: separate tab sets with their own name, emoji and accent colour.
- **Command palette address bar** (Ctrl+L, Ctrl+T; Ctrl+K searches the web, Ctrl+Alt+P opens the command palette): URL or search (Google or Yandex), open tabs, bookmarks, history, optional search suggestions, and `>` commands. It floats over the live page.
- **Compact mode** (Ctrl+Alt+C): the page fills the window, and the sidebar and toolbar float in from the edges.
- **Bookmarks** as in Zen: a toolbar (also at the top edge in the single-toolbar layout), a sidebar, the Library, an Edit bookmark panel with per-workspace bookmarks, tags and keywords, import from other browsers and HTML files, and daily JSON backups. See [Zen Browser UI in Zenith](apps/docs/src/content/docs/contributing/zen-ui.md).
- **Split view**: two to four tabs side by side, stacked or in a grid, created from the toolbar, a tab menu, by dragging a tab onto the page, or with Ctrl+Alt+V (side by side), Ctrl+Alt+H (stacked) or Ctrl+Alt+G (grid).
- **Glance**: Alt+click a link to preview it in a floating card, then open it as a tab, in split view, or close it.
- **Tab sleeping**: background tabs are suspended, then unloaded after a timeout and woken at their scroll position. Tabs playing audio, downloading or in split view are exempt.
- **Ad and tracker blocking** built in (EasyList, EasyPrivacy, custom lists), with a per-site toggle and a blocked-request counter.
- **Video downloader** built in: saves the videos and audio that pages play, whether plain files or HLS and DASH streams, with a choice of quality and audio language and recording of live HLS streams. Start it from the button over a video, the download button in the address pill, the page menu or by right-clicking a video. Separate audio and video tracks are merged into one .mp4. DRM-protected video is never downloaded.
- **Extensions** from the Chrome Web Store, `.crx` files or unpacked folders, where WebView2 supports them, with a toolbar menu, pinned extension icons and update checks for Web Store extensions (see [extension support](apps/docs/src/content/docs/contributing/extensions.md)).
- **Password manager and wallet**: passwords and payment cards in an encrypted vault on your PC, unlocked with a master password (with a recovery key). Zenith offers to save logins and cards, fills them in on sign-in and checkout forms, and imports and exports passwords as CSV. See [passwords and payment cards](apps/docs/src/content/docs/guides/passwords.md).
- **Profiles**: separate sets of settings, tabs, history, bookmarks, passwords, cookies and extensions, which can run side by side ([profiles](apps/docs/src/content/docs/guides/profiles.md)).
- **Incognito windows** (Ctrl+Shift+P): no history, downloads or session are saved, and cookies and site data stay in memory until the last incognito tab closes.
- **Notes** (Ctrl+Alt+N): a simple notes editor, with a shortcut that can bring Zenith forward from any app.
- History, bookmarks (HTML import and export), a downloads manager, session restore, permission prompts, find in page, zoom per site, printing, developer tools.
- **Customisable keyboard shortcuts** that work while a page has focus, with conflict detection.
- **Default browser** registration: links from other apps open as new tabs in the running window.
- Light, dark or follow-Windows theme.

## Numbers

|                                                                   | Measured on Zenith 0.1.0             | Target   |
| ----------------------------------------------------------------- | ------------------------------------ | -------- |
| Installer                                                         | 1.78 MB                              | < 10 MB  |
| Startup to visible UI                                             | 650–900 ms                           | < 1 s    |
| App overhead, 1 tab (Rust + chrome UI + WebView2 browser process) | 93 MB                                | < 120 MB |
| Everything, 1 tab (including GPU and renderers)                   | 284 MB (Edge, fresh profile: 548 MB) |          |
| Requests reaching the network on 6 news sites, blocking on        | −75 %                                |          |

Details and method: [measurements](apps/docs/src/content/docs/contributing/measurements.md).

## Requirements

Windows 11, or Windows 10 22H2, plus the Microsoft Edge WebView2 runtime. It ships with Windows 11; the installer downloads it if it is missing.

## Install

Download `Zenith_<version>_x64-setup.exe` from [GitHub Releases](https://github.com/Drakaniia/Zenith/releases/latest). After that, Zenith updates itself: new versions download in the background and install silently when you close it. To publish a release, see [releasing](apps/docs/src/content/docs/contributing/releasing.md).

To install a local build, run `apps\desktop\src-tauri\target\release\bundle\nsis\Zenith_<version>_x64-setup.exe` from `pnpm app:build` (per-user install, no administrator rights). It registers Zenith as a browser in **Settings › Apps › Default apps**. To make it the default, use **Settings › Default browser › Make default** in Zenith, which opens that Windows page (Windows does not let apps change the default silently).

## Documentation

The user guide and contributor docs are published at https://drakaniia.github.io/Zenith/. The website is built with Astro Starlight in `apps/docs` (pages in `apps/docs/src/content/docs`). Run `pnpm docs:dev` to preview it locally, or `pnpm docs:build` to build it.

## Build from source

Prerequisites: Rust 1.85+ with the MSVC toolchain, Node.js 22+, pnpm. Zenith is a pnpm monorepo (the browser in `apps/desktop`, the website in `apps/docs`); run these from the repository root. See [project structure](apps/docs/src/content/docs/contributing/project-structure.md) for where code lives.

```powershell
pnpm install
pnpm app:dev      # development build with hot reload
pnpm app:build    # release build + NSIS installer in apps\desktop\src-tauri\target\release\bundle\nsis
```

## Tests

```powershell
# From the repository root
pnpm check            # svelte-check / TypeScript
pnpm test             # UI unit tests (URL handling, fuzzy search, shortcut rules, ...) and release-script tests
pnpm test:rust       # Rust unit tests (blocker, URL validation, storage, shortcuts, extensions,
                      #  video downloader: HLS/DASH parsing, MP4 merging, downloads against a local server)
pnpm check:structure  # file and folder size limits, import direction, no import cycles

# End-to-end, against an isolated test instance (own profile, never touches your windows), from apps/desktop
cd apps/desktop
powershell -ExecutionPolicy Bypass -File scripts/dev/build.ps1
powershell -ExecutionPolicy Bypass -File scripts/dev/run.ps1 -Fresh -SkipSetup
node scripts/dev/e2e.mjs
```

The e2e suite (34 scenarios) drives the real app over the DevTools protocol: navigation, IPC isolation, webview placement, floating UI over pages, ad blocking, history, bookmarks, Glance, split view and closing a pane, drag-to-pin, Essentials across spaces, zoom, sleeping with scroll restore and the audio and download exemptions, find, permissions, popups, crash recovery, downloads, the video downloader (detection from page traffic, the button over a video, a stream saved byte-exact), extensions (including Chrome Web Store installs), compact mode, spaces, the shortcut editor (recording, conflicts, reserved keys), search suggestions, and session restore after a normal exit and after a forced kill.

## Keyboard shortcuts (defaults)

| Keys                                 | Action                                           |
| ------------------------------------ | ------------------------------------------------ |
| Ctrl+T / Ctrl+W / Ctrl+Shift+T       | New tab / close tab / reopen closed tab          |
| Ctrl+L, Alt+D, F6                    | Address bar                                      |
| Ctrl+K, Ctrl+J                       | Search the web                                   |
| Ctrl+Alt+P                           | Command palette                                  |
| Ctrl+Tab, Ctrl+1…8, Ctrl+9           | Switch tabs                                      |
| Ctrl+N / Ctrl+Shift+P                | New window / new incognito window                |
| Ctrl+Alt+C / Ctrl+Alt+S              | Compact mode / floating sidebar in compact mode  |
| Ctrl+Alt+V / Ctrl+Alt+H / Ctrl+Alt+G | Split view side by side / stacked / grid         |
| Ctrl+Alt+E, Ctrl+Alt+Q               | Next / previous space (also Ctrl+Alt+Right/Left) |
| Ctrl+D, Ctrl+Shift+D                 | Bookmark this page                               |
| Ctrl+B / Ctrl+Shift+B / Ctrl+Shift+O | Bookmarks sidebar / bookmarks toolbar / library  |
| Ctrl+H / Ctrl+Shift+H                | History sidebar / all history                    |
| Ctrl+Shift+Y                         | Downloads                                        |
| Ctrl+F, F3                           | Find in page                                     |
| Ctrl+Plus / Ctrl+Minus / Ctrl+0      | Zoom in / out / reset                            |
| Alt+click a link                     | Glance                                           |

All of them can be changed in **Settings › Keyboard shortcuts**. The full list is in the [keyboard shortcuts reference](apps/docs/src/content/docs/reference/keyboard-shortcuts.md).

## Privacy

No telemetry and no accounts. Zenith itself only goes online to refresh filter lists (weekly, easylist.to), for search suggestions if you turn them on (off by default), to download an extension when you install one from the Chrome Web Store, and to check for updates (Zenith's own on GitHub Releases, Web Store extensions' on Google's update service; both can be turned off). The new-tab page makes no network requests. The video downloader contacts the site you are on, and only that site: it downloads the media you choose, sending the site's cookies for that address as the page would. If WebView2 cannot hand over a streaming playlist the page has just loaded, it fetches that same playlist again to list the stream. Browsing data lives in `%LOCALAPPDATA%\dev.zenith.browser` and `%APPDATA%\dev.zenith.browser`, and can be cleared in **Settings › Privacy**.

## How it works

See [architecture](apps/docs/src/content/docs/contributing/architecture.md): one WebView2 process tree shared by all tabs, tab webviews without any app IPC, floating UI by raising a transparent chrome window over the pages, shortcuts through WebView2's accelerator events, and blocking in `WebResourceRequested`.

## Known limitations

- Windows only (Linux/macOS would need a different webview layout).
- Extension support is limited to what WebView2 implements: no action badges, limited tab APIs, and only Web Store extensions are checked for updates ([details](apps/docs/src/content/docs/contributing/extensions.md)).
- Ad blocking works on domains (no cosmetic element hiding yet).
- The video downloader saves what a page streams over plain HTTP: files, HLS and DASH. DRM-protected video (Encrypted Media Extensions, encrypted HLS, DASH ContentProtection) is refused by design. Sites that use their own streaming protocols (YouTube, for example) can't be saved. Live DASH can't be recorded. Without FFmpeg, MPEG-TS streams stay .ts, and WebM audio and video that need merging are saved as two files. Download jobs don't survive a restart of the browser.
- No sync between PCs. Form autofill covers logins and payment cards only; names, addresses and phone numbers can't be saved yet.
