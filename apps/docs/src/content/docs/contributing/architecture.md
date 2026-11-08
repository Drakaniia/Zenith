---
title: Architecture
description: How Zenith is put together - the chrome webview, tab webviews, floating UI, shortcuts, blocking, storage and updates.
---

Source paths on this page are relative to `apps/desktop`.

Zenith is a Tauri 2 application. Rust owns the window, every page webview and all storage; a Svelte 5 UI (the "chrome") draws the sidebar, palette, menus and internal pages. Pages are rendered by the system WebView2 (Chromium), so there is no bundled engine.

```
Native window (Mica/Acrylic, no title bar)
├─ chrome webview  — Tauri webview "main", Svelte UI, transparent, the only view with IPC
└─ tab webviews    — raw wry child webviews (one per live tab / split pane / Glance), no IPC
          all share one WebView2 environment → one browser process, per-site renderers
```

## Source map

The layout and its rules are described in [Project structure](../project-structure/).

| Area | Files |
| --- | --- |
| Startup, windows, single instance, maintenance flags | `src-tauri/src/main.rs`, `src-tauri/src/app/` (`mod.rs` the Tauri builder, `startup.rs` maintenance flags and second launches, `window.rs` windows and geometry); UI entry `src/main.ts`, `src/app/startup.ts` |
| Tab host: windows, layout, z-order, focus, shortcuts, stray targets | `src-tauri/src/host/`: `mod.rs` (host state, `post`), `chrome.rs`, `window_proc.rs`, `placement.rs`, `focus.rs`, `accelerators.rs`, `devtools.rs` |
| Page webviews and their WebView2 events | `src-tauri/src/host/tab/` (`mod.rs`, `events.rs`, `requests.rs`, `new_window.rs`, `permissions.rs`, `find.rs`, `context_menu.rs`, `scripts.rs`) |
| Downloads | `src-tauri/src/host/downloads.rs` (WebView2 side), `src-tauri/src/downloads/` (records); UI in `src/features/downloads/` |
| Extensions | `src-tauri/src/extensions/` (`install.rs`, `crx.rs`, `updates.rs`); UI in `src/features/extensions/` |
| Video downloader | `src-tauri/src/media/`: `sniff.rs` (WebView2 side), `detect.rs`, `hls.rs`, `dash/`, `job/`, `http.rs`, `mp4/`; UI in `src/features/downloads/MediaPanel.svelte` |
| Commands for the UI | `commands.rs` in each domain folder (`src-tauri/src/<domain>/commands.rs`), all registered in `src-tauri/src/app/handlers.rs`, helpers in `src-tauri/src/app/command.rs`; each feature's `src/features/<domain>/api.ts` wraps its commands |
| Ad/tracker blocker | `src-tauri/src/blocker/`, `src-tauri/resources/blocklist.txt` |
| Storage (JSON snapshots, SQLite) | `src-tauri/src/storage/mod.rs` (each domain keeps its own queries) |
| Shortcut parsing, reserved keys, global hotkeys | `src-tauri/src/shortcuts/keys.rs`, `src-tauri/src/shortcuts/hotkey.rs` |
| Windows integration (default browser, shell, memory) | `src-tauri/src/platform/win/`; UI side `src/platform/shell.ts` |
| Browser model (spaces, tabs, split, Glance, session) | `src/features/browser/browser.svelte.ts`, with `tabs.ts`, `spaces.ts`, `split.ts`, `glance.ts` and `session.ts` beside it |
| Webview placement and overlay leases | `src/platform/layout.svelte.ts`, `src/platform/webview-api.ts` |
| Tab sleeping | `src/features/browser/sleep.ts`, `src-tauri/src/host/tab/sleep.rs` |
| Actions, shortcuts, settings | `src/platform/actions.ts` (registry), each feature's `actions.ts` (loaded by `src/app/actions.ts`), `src/features/shortcuts/shortcuts.ts`, `src/features/settings/settings.svelte.ts`, `src-tauri/src/settings/` |
| First-run setup assistant | `src/features/onboarding/`: `onboarding-flow.ts` (steps, rules), `onboarding.svelte.ts` (state), `OnboardingDialog.svelte`, `steps/` |
| Import from other browsers | `src-tauri/src/importer/` |
| Password manager and wallet | `src-tauri/src/vault/`, `src-tauri/src/host/autofill/`; UI in `src/features/passwords/` |

## Tab webviews

Each live tab is a `wry` child webview created with the chrome's `ICoreWebView2Environment`, so every view shares one WebView2 browser process and profile (`%LOCALAPPDATA%\dev.zenith.browser\WebView2`). Tab webviews are not Tauri webviews: they never get Tauri's IPC scripts, so remote pages cannot call any command (verified by the e2e test `tab-has-no-ipc`). Two scripts are injected into pages. The first is a 20-line click handler that turns Alt+click into a Glance request and Ctrl/middle-click into a background-tab request, by calling `window.open` with reserved names. The second, optional, is the video downloader's button (below), which uses the same mechanism. `NewWindowRequested` recognises the reserved names and cancels the window. Nothing in the page can reach the app.

All WebView2 objects live on the UI thread. Commands hop there with `run_on_main_thread`; WebView2 callbacks that would re-enter WebView2 (creating a popup's webview, focus changes during layout) are posted to a window-message queue (`host::post`) so they run after the callback returns.

## Layout and the z-order problem

Child webviews are native windows that always draw above the chrome webview, so floating UI (palette, compact sidebar, Glance, menus, prompts) cannot simply be drawn on top. Zenith solves it with **window order instead of an extra overlay webview**:

- Each visible page has an empty placeholder element (`use:pane`). A `ResizeObserver` reports the placeholders' rectangles to Rust, which positions the webviews exactly over them (window resizes are also handled natively in a window subclass, so pages follow the window edge without waiting for the UI).
- Floating UI takes an **overlay lease** (`use:overlayLease`) while it is mounted, including its exit animation. While any lease exists, Rust raises the chrome's window above the page windows. The chrome is transparent, so the live page stays visible wherever the UI does not draw, and the floating UI appears over it. When the last lease ends, the chrome goes back to the bottom.
- Glance is a real tab webview positioned over the Glance card, raised above the raised chrome.

This needs no extra webview or screenshots, so floating UI costs no memory, and pages keep rendering (videos keep playing) under the palette.

## Keyboard shortcuts

WebView2 raises `AcceleratorKeyPressed` on the controller before a page sees a key, for the chrome and for every tab. Rust matches the chord against the user's bindings (`shortcuts/keys.rs`) and, on a match, marks the key handled and emits a `shortcut` event to the UI. Editing keys (Ctrl+C/V/X/A/Z/Y, Ctrl+Shift+Z/V, word navigation) are reserved and can never be bound, so pages and text fields always receive them; AltGr chords are left alone.

## Ad and tracker blocking

Every tab registers a `WebResourceRequested` filter for all resource contexts and request sources. The engine (`src-tauri/src/blocker/`) holds two hash sets (block and allow) of domains built from a built-in list plus the domain-level subset of hosts/Adblock Plus syntax in EasyList and EasyPrivacy (≈93,000 rules); a lookup walks the request host's parent domains, a few hash probes per request. Requests to the page's own site are never blocked. Blocked requests get a synthetic 403 response; counts are coalesced and shown in the address pill, which also toggles blocking per site. Lists are refreshed weekly from easylist.to.

## Video downloader

Detection is passive and per tab (`TabShared.media`):

- `WebResourceRequested` (already hooked for blocking) remembers which requests came from `<video>`/`<audio>` elements. `WebResourceResponseReceived` classifies each response by content type, status and size (`detect::classify`). Media-element files become items. Byte-range reads of whole files by Media Source players also count. Stream segments (`.ts`, `.m4s`, ordinary script-loaded media) are ignored.
- HLS playlists and DASH manifests are read from the response WebView2 already received (`GetContent`); only if that fails are they fetched again. A master playlist absorbs its variant playlists, which supply the duration and the live and encrypted state, so the panel shows one item with its qualities and audio languages.
- Opening the panel also runs a scan script in the page (`<video>`/`<audio>` sources, `og:video`), which finds files not requested yet and counts players using `blob:` sources or DRM (`mediaKeys`).
- The optional button over videos is a closed shadow-DOM element styled through CSSOM, so page CSP and styles don't affect it. On a trusted click it calls `window.open(src, "_zenith_media")`, which only opens the panel; the page gets no other channel. The script is registered with `AddScriptToExecuteOnDocumentCreated` before a tab's first navigation, and is added to or removed from live tabs when the setting changes.

Downloading happens in `media/job/`, one thread per job, outside WebView2's download manager:

- WinHTTP (`media/http.rs`, HTTP/2, one session per job) sends the page's Referer, Origin and User-Agent, and the profile's cookies for each address (`ICoreWebView2CookieManager`, fetched on the UI thread). SameSite=Strict cookies go only to same-site media.
- Zenith follows redirects itself rather than leaving them to WinHTTP. Each hop gets the cookies of its own host, HTTPS never downgrades to HTTP, and every hop passes an address policy: media found on a public page may not point into the local network (loopback, private, link-local or `.local` addresses).
- Segments download on six parallel workers and are appended in order, so disk use stays near the output size. Plain files resume with `Range`. Failed requests retry with backoff. A failed job waits in the list for Retry.
- Live HLS is recorded from near the live edge until the user stops it, the playlist ends, or the stream stays unreachable; each case keeps the recording. A restarted sequence is noticed. A full disk fails the job visibly, and a failed recording can still be saved from the downloads list.
- Separate video and audio are merged natively when both are fragmented MP4 (`mp4::merge`). Each fragment is kept byte-for-byte; the merge only renumbers tracks and fragments, rescales edit lists into the new movie timescale, and interleaves fragments by decode time. Otherwise FFmpeg is used when installed (setting, or PATH). FFmpeg also converts MPEG-TS results to MP4. Without it, the parts are kept.
- Work files live in `%LOCALAPPDATA%\dev.zenith.browser\media-tmp` and are cleared at startup. Finished files move into the downloads folder under the page title and quality.
- Jobs appear in the regular downloads list (`kind: "media"`). They don't keep a tab awake, because they don't depend on its webview. Job state changes are emitted and written under one lock, so the list never shows stale states or brings back removed rows.
- Finished files get the Mark of the Web (`Zone.Identifier`, with the source and page addresses minus their query strings). Their extensions come from fixed lists (never from the page), and names are made safe for Windows, including reserved device names such as `nul.tar`.

Pages are untrusted input, so their manifests are bounded. Playlists and MPDs are parsed on worker threads. HLS is capped at 200,000 segments per playlist; DASH at 100 representations and 400,000 segments in total. A page gets at most 200 manifest reads, 8 at a time. FFmpeg reads only local files with a forced demuxer (`-protocol_whitelist file -f …`), so a hostile "segment" can't make it fetch anything. Cancel kills FFmpeg mid-run.

DRM is never circumvented. Items are refused when the player uses Encrypted Media Extensions, an HLS key method other than NONE is present, a DASH ContentProtection is declared, or an MP4 init segment carries `encv`/`enca` sample entries or `pssh`.

## Tab sleeping

`src/features/browser/sleep.ts` runs every 30 s. A background tab is first suspended (`TrySuspend`, renderer kept) after a minute, then put to sleep after the configured timeout (default 5 min): its scroll position is read, the webview is destroyed, and only metadata stays. Clicking it recreates the webview and restores the scroll position after load. Tabs that are visible, in split view or Glance, downloading, or playing audio (or did so within the last minute) are exempt. When Windows reports ≥ 88 % memory load, the least recently used idle tabs are slept early. Pinned and Essential tabs may sleep.

## Popups and extension pages

`window.open`/`target=_blank` from a user action become a tab next to the opener; WebView2 is handed a new tab webview through `SetNewWindow`, so `window.opener` keeps working (OAuth, payments). Script-initiated popups without a user gesture are blocked with a toast that can open them. Pages that WebView2 opens on its own for an extension are found through the chrome's DevTools session (`Target.targetCreated`): a page target that belongs to no Zenith webview is closed and reopened as a tab.

## Incognito windows

An incognito window is another window of the running Zenith process, labelled `incognito-N` (Ctrl+Shift+P, the app menu, a tab's menu, "Open link in incognito window" in a page's context menu, or `zenith --incognito`). See `src-tauri/src/incognito/`.

- All windows pass the same WebView2 options, so they share one WebView2 browser process. A new incognito window does not start a process or a profile; its UI is ready in well under a second.
- The host keeps per-window state (`host::Win`: chrome webview, layout, focus, fullscreen tab) and one tab map for all windows. Every tab records its window (`TabShared::window`), and tab events are emitted only to that window (`emit_tab`). The UI listens window-scoped (`getCurrentWebviewWindow().listen`), so it never sees another window's events. Commands that act on a window take the calling `WebviewWindow`.
- Incognito tabs use WebView2 InPrivate mode: cookies, cache and site storage stay in memory, are shared by all incognito windows (as in other browsers), and are discarded once the last incognito tab closes.
- An incognito window never records history or downloads, never loads or saves a session, window geometry or settings, and refuses to install extensions or update. It starts from the user's settings; changes made there last only as long as the window. Bookmarks and filter lists are shared with the user's profile.
- Closing a window first lets its UI save (`app-close-requested`, answered with `window_close`). Closing the last window quits Zenith. Closing the normal window while incognito windows are open keeps them, and a second launch then reopens the normal window.
- Earlier versions ran incognito as a separate process on a throwaway profile in `%TEMP%\zenith-incognito`. Leftover folders are still swept at startup, and `--purge-incognito <dir>` still works.

## Bookmarks

A tree as in Firefox's Places, which Zen builds on. `src-tauri/src/bookmarks/` owns it: the table `bookmark_nodes` (parent, kind `bookmark`/`folder`/`separator`, title, url, position, dates, tags, keyword, workspaces) and three fixed roots, 1 Bookmarks Toolbar, 2 Bookmarks Menu and 3 Other Bookmarks. Positions are kept contiguous in every folder. The old flat `bookmarks` table is converted once, under Other Bookmarks, and left in place as a backup; a `meta` flag and an immediate transaction make that safe when two windows start together.

- **Commands** (all emit `bookmarks-changed` with the window label, so other windows reload): `bookmarks_list`, `bookmarks_add` (nodes with children, at an index), `bookmark_update`, `bookmark_move` (the index counts the folder's children after the node has left), `bookmarks_reorder`, `bookmarks_remove`, `bookmarks_remove_url`. Mistakes the user can fix (a bad address, a folder moved into itself) come back as readable text.
- **Files:** Netscape HTML import and export (tree, separators, tags, keyword, toolbar and unfiled folders; same-named folders merge and known bookmarks are skipped), a JSON backup of the whole tree, and automatic daily backups in `bookmarkbackups` (newest ten kept, plus the copy made before any restore). A restore validates the whole file first, then replaces everything in one transaction. Restoring by name accepts only the file names the app writes, never a path.
- **Workspaces:** a bookmark with workspace ids shows on the toolbar and in menus only in those, as in Zen (hidden when it belongs to other workspaces and not the active one). Ids of deleted workspaces are ignored. The sidebar and Library show everything.
- **Favicons:** bookmarks without a remembered icon get their site's `/favicon.ico` through `favicon_fetch` (private hosts refused, 128 KB cap, raster formats only, returned as a data URL), because the page CSP allows no remote images.
- **UI** (`src/features/bookmarks/`): `bookmarks.ts` holds the tree logic (search, keywords, restriction tokens, drag-move planning) and is unit tested; `bookmarks.svelte.ts` is the store with undo and redo; `bookmark-actions.ts` the user actions. The toolbar, sidebar and Library, the Edit bookmark panel and the detailed dialog are components of the same names.

## First-run setup

The first time the normal window starts (`settings.onboarded` is false), a setup assistant opens over it before the window is shown, so a returning user never sees it flash by. Incognito windows never show it. It has seven steps: welcome, appearance (theme, accent colour, layout), search engine, start page (restore tabs, home page), import, privacy, and a summary. Every choice is applied and saved the moment it is made, so the window behind the card previews it, and closing Zenith mid-setup resumes at the same step (`onboardingStep`). Skipping an optional step restores the settings it owns to their values when the step opened (`STEP_FIELDS`). Only "Start browsing" (or "Skip setup and use the defaults") sets `onboarded`; after that the assistant opens again only from Settings › General › Run setup again or the "Run setup again" command.

While it is open the rest of the UI is `inert` and shortcuts other than Quit are ignored. Its progress indicator is the sun climbing an arc from the horizon (first step, 0°) to the zenith (last step, 90°); steps already reached can be revisited from their marker.

To add a step: add it to `STEPS` and `STEP_FIELDS` in `src/features/onboarding/onboarding-flow.ts`, write its component in `src/features/onboarding/steps/`, and register it in `VIEWS` in `src/features/onboarding/OnboardingDialog.svelte`.

Test instances: `run.ps1 -SkipSetup` writes `{"onboarded":true}` into a new profile so the e2e tests start at the browser.

### Importing from other browsers

`src-tauri/src/importer/` finds profiles of Chrome, Edge, Brave, Vivaldi, Chromium, Opera, Opera GX, Firefox and Zen in their standard folders for the current Windows user. The UI only passes back a source id from that scan, never a path. Chromium `Bookmarks` (JSON) is read directly; `History` and Firefox's `places.sqlite` are copied (with their WAL) to a temporary folder and read there, because the other browser may be running and its files must never be changed. Imported bookmarks go into a folder named after the browser, with nested folders kept as "Chrome / Dev"; a bookmark already in that folder is skipped, so importing twice adds nothing. History is limited to the retention period (and 20,000 entries) and merged: an existing page keeps the higher visit count and the later visit. Passwords, cookies and open tabs are not imported.

## Password manager

`src-tauri/src/vault/` (crypto, CSV, the vault; commands in `commands.rs` and `card_commands.rs`), `src-tauri/src/host/autofill/` (page scripts, login and payment filling); UI in `src/features/passwords/` (`PasswordsPage.svelte`, `PasswordSavePrompt.svelte`, `PasswordSuggest.svelte`, the wallet in `cards/`).

- **Keys.** A random 256-bit vault key encrypts each password and note with AES-256-GCM (fresh nonce; the entry's id, origin and username are authenticated with it, so ciphertexts can't be moved between entries). The vault key is stored only wrapped twice: by an Argon2id key from the master password (46 MiB, 2 passes) and by one from the recovery key shown once at setup. The master password is never stored; a wrong one simply fails to unwrap. Five wrong attempts start a doubling wait (up to a minute).
- **Metadata** (origins, usernames, labels, "never save" sites, 32-bit keyed password fingerprints) is encrypted with Windows DPAPI for the current user. It is readable while the vault is locked, so login pages can be offered accounts and repeated logins recognised without the master password.
- **File.** `%APPDATA%\dev.zenith.browser\vault.json`, written atomically. A file that can't be read (damaged, another Windows account) is never overwritten; "start over" renames it aside.
- **Locking.** The unlocked key lives only in memory: Zenith starts locked, locks after the configured idle time (default 15 min) and on Lock. "Ask every time" requires the master password for each fill, reveal and copy.
- **Pages.** A main-frame script reports login-field clicks, submitted logins and login forms through reserved `window.open` names (`_zenith_pw_*`), like the other page scripts. Only top-level-document messages count (`OriginalSourceFrameInfo`), and clicks and submissions need a user gesture. The origin is always taken from the tab's own address, never from the message. Filling runs `ExecuteScript` after the backend checks the tab's origin against the entry (same origin, or same registrable site with that setting; never HTTPS→HTTP, IPs, or shared hosting suffixes such as github.io), and the fill script re-checks `location.origin`. Passwords cross into the chrome UI only when revealed in the manager; copy uses a clipboard write excluded from history and cloud sync and cleared after 30 s.
- **Import/export.** CSV from Chrome, Edge, Firefox, Bitwarden, 1Password and others (column names detected), previewed with new/duplicate/conflict rows before saving. Export writes Chrome-format CSV after the master password and a plain-text warning; backups are the same entries encrypted with the master password (`.zenithpasswords`), restorable with Import.
- **Wallet.** Payment cards live in the same vault (`vault/cards.rs`) and are offered on checkout forms (`host/autofill/payment.rs`); saving is offered only on HTTPS pages, and revealing, copying and filling a card always needs the master password.
- Incognito windows never offer to save logins or cards. Personal data is a placeholder.

## Persistence

- `session.json`, `settings.json`, `window.json` (`%APPDATA%\dev.zenith.browser`): JSON written atomically (temp file, flush, rename) and debounced, so a crash or kill leaves the last complete snapshot.
- `zenith.db`: SQLite (WAL) for history, bookmarks, downloads and notes, owned by one worker thread so the UI thread never waits on disk.

## Crash handling

A crashed or hung page shows a "tab crashed / not responding" card with Reload. If the chrome UI's renderer dies it reloads; if the whole WebView2 browser process exits, the app restarts and restores the session.

## Security notes

- IPC is limited to the local `main` webview by the `chrome` capability; the chrome has a strict CSP and no remote code.
- URLs from the palette and from other apps are validated: `javascript:`, `data:` and other schemes are refused; external launches accept only http/https and existing local files.
- Remote debugging is compiled out of release builds unless built with the `remote-debug` feature.
- SmartScreen and Safe Browsing behave as WebView2's defaults (SmartScreen on).

## Updates

Zenith updates itself through Tauri's updater plugin (`src-tauri/src/updates/`). Release CI publishes a signed installer and `latest.json` to GitHub Releases for every version tag. The app checks that feed a minute after startup and then every six hours. A newer version downloads in the background and its signature is verified. The NSIS installer then runs in quiet per-user mode when Zenith quits, or right away from "Restart to update", which relaunches into the restored session. Database writes are flushed first, because the installer ends the process. The rendering engine updates separately through the WebView2 Evergreen runtime. See [Releasing and updates](../releasing/).
