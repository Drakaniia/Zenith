# Tauri Zen-Style Browser: Specification

Status: Draft v0.2 (requirements gathered via interview; open questions partly resolved; no code written)
Working name: **Zenith** (placeholder)
Date: 2026-11-04

## 1. Summary

A lightweight, real web browser for **Windows**, built with **Tauri 2** (Rust + system WebView2) and a **Svelte** chrome UI, with a modern UI inspired by Zen Browser: vertical tabs, compact auto-hiding chrome, Spaces, split view, Glance, and pinned/essential tabs. The overriding design goal is **minimum resource use** (RAM first, then installer size, then startup time, then animation smoothness).

v1 is a **prototype**: tabs + address bar + the Zen-style UI shell. However, the owner has also named ad/tracker blocking, extension support, default-browser registration, and custom shortcuts as **must-haves** (see Section 9 on how this conflicts with "prototype" and how to resolve it).

## 2. Goals and Non-Goals

### Goals

- Real browsing: each tab is an actual WebView2 web view rendering real websites.
- Zen-inspired UX: vertical sidebar, compact mode, Spaces, split view, Glance, pinned tabs, floating command-palette address bar.
- Resource targets (Section 7), prioritised in this order: **low RAM > small installer > fast cold start > smooth animation**.
- Windows 11 Mica/Acrylic translucent look.

### Non-Goals (v1)

- macOS and Linux support (Linux "can come later"; stock Tauri cannot lay out child webviews there).
- Own rendering engine (the system WebView2/Chromium is used).
- Sync, accounts, password manager/autofill (explicitly out; the "daily-driver" definition was **not** selected).
- Developer tools beyond what WebView2 provides by default.

## 3. Platform and Technology

| Area                 | Decision                                                                                                                                            |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| OS                   | **Windows 10 22H2+ and Windows 11** (decided). Mica/Acrylic on Windows 11; Windows 10 uses the solid-theme fallback                                 |
| Shell                | Tauri 2.x (latest stable at time of research: 2.12.2, wry 0.57.0). Pin exact minor versions because the `unstable` feature has no semver guarantees |
| Tabs                 | One wry **child webview** per tab via `Window::add_child` / `WebviewBuilder` (requires Cargo feature `unstable`)                                    |
| Chrome UI            | Svelte (compiled, no virtual DOM; chosen for small bundle and low RAM)                                                                              |
| Engine               | WebView2 (Evergreen runtime; not bundled to keep installer small, see Section 7)                                                                    |
| Language for backend | Rust                                                                                                                                                |

### Known platform constraints (from research, to be verified in Phase 0)

Sources: Tauri docs/PRs and a third-party research wiki (Syrinx) checked 2026-10-09; treat as medium confidence until reproduced.

1. **Multiwebview is unstable.** Maintainers have said they lack capacity for multiwebview bugs. Expect breakage on upgrades.
2. **No z-order API.** A child webview always draws above the main (chrome) webview. Chrome UI cannot natively draw over a tab. This directly affects compact-mode flyouts, the floating URL bar, and Glance.
3. **Keyboard shortcuts do not reach the app while a page has focus** (Windows and macOS). Tauri issue #9257 (unstable flag breaks Cmd+A/C/V) is open. In-app shortcuts such as Ctrl+T need custom handling; the OS-wide `global-shortcut` plugin is unsuitable.
4. **Windows builder deadlock** can occur when creating webviews from synchronous commands; use async commands.
5. **Process model:** WebView2 webviews sharing a user data folder share one browser process; renderer processes are still per-site by Chromium defaults. Per-tab RAM is therefore dominated by renderers, not by Tauri.
6. **Tauri injects its IPC scripts into every webview**, including remote pages. Remote pages must be given **no IPC capability** (capabilities/remote-URL scoping), or the IPC bridge must be disabled for tab webviews. This is a security requirement.
7. Browser features Tauri does not provide and the app must build itself: keyboard shortcuts, z-order handling, screenshots/thumbnails, find-in-page, favicons, crash recovery (WebView2 process-failed events), downloads UI, default-browser registration.

## 4. Feature Specification

### 4.1 Window and chrome layout

- Single native window hosting: one **chrome webview** (Svelte UI) and N **tab webviews** (children), positioned by Rust to fit the content area.
- Layout: vertical **sidebar** on the left (width resizable, default ~240 px), content area to the right.
- Resize behaviour: tab webview bounds recomputed on window resize, sidebar resize, split-view changes, and compact-mode transitions. Use `set_auto_resize` only where it matches the layout; otherwise explicit bounds.
- Material: Windows 11 **Mica/Acrylic** applied to the window (the sidebar/chrome area shows the translucency; page content is opaque as child webviews are opaque). Setting for "Solid theme" as a fallback for Windows 10 and for lowest GPU/RAM use. Verify transparent window plus child webviews behaves correctly (open question OQ-3).

### 4.2 Vertical tabs

- Tabs listed vertically in the sidebar with favicon, title, close button, audible/muted indicator.
- Drag to reorder; drag between pinned and normal zones.
- New tab button; middle-click to close; Ctrl+T / Ctrl+W / Ctrl+Shift+T (reopen closed) / Ctrl+Tab / Ctrl+1..9.
- Tab states: **active** (live webview), **background-live**, **sleeping** (webview destroyed, only metadata kept), **loading**, **crashed**.

### 4.3 Pinned / Essentials tabs

- **Essentials**: a compact grid (icon-only tiles) at the top of the sidebar, shared across all Spaces (as in Zen).
- **Pinned tabs**: per-Space pinned tabs that keep their original URL (reset-to-pinned-URL action).
- Pinned tabs are never closed by the sleep timer but **may be put to sleep** (state retained, webview freed). Decision (OQ-5): pinned and Essentials tabs **may sleep** (state kept); only audio, download and split/Glance tabs are exempt.

### 4.4 Spaces (Workspaces)

- Multiple Spaces, each with its own tab list, pinned tabs, name, icon/emoji and accent colour.
- Switch via sidebar footer icons and keyboard shortcut (e.g. Ctrl+Alt+Left/Right; configurable).
- Switching a Space sleeps or hides the previous Space's webviews per the sleep policy (Section 5) to save RAM.
- Spaces share one WebView2 profile (cookies/logins shared) in v1. Per-Space isolated profiles are a possible later feature (extra browser process = more RAM; contradicts the top priority).

### 4.5 Compact mode

- Option to auto-hide the sidebar (and toolbar) so the page fills the window.
- Sidebar revealed by mouse hover at the left edge (with a small delay/hysteresis) or a shortcut (e.g. Ctrl+B).
- **Requirement (owner-decided): the revealed sidebar must float over the page, not push or resize it.**
- Implementation strategy is an open design decision because of the z-order constraint (Section 6).

### 4.6 Floating address bar / command palette

- Ctrl+L (and Ctrl+K optional) opens a centred floating palette over the page.
- Accepts URL or search text; shows suggestions from: open tabs, history, bookmarks, and the default search engine's suggestions endpoint (opt-in, privacy toggle).
- Also acts as a command palette (e.g. "New Space", "Toggle compact mode"), prefix-based (`>` for commands).
- Must **float over the page** (same z-order constraint as 4.5).
- A minimal URL display (domain pill) is shown for the active tab in the sidebar; clicking it opens the palette.
- Search engines: **Google (default)** and **Yandex** selectable in settings (decided, OQ-7). Remote search suggestions are **opt-in**, off by default, so the new-tab page makes no network calls. Other engines can be added later.

### 4.7 Split view

- Show **2 tabs** side by side in v1 (decided, OQ-10); the layout model is designed to extend to 3-4 panes later. Draggable divider.
- Each pane is its own child webview with explicit bounds. Focus follows the clicked pane; the sidebar highlights both tabs.
- Create via: context menu on tab ("Split with..."), drag a tab onto the content area edge, or shortcut.
- Exit via close button on a pane or shortcut; panes revert to normal tabs.

### 4.8 Glance

- Peek a link in a floating overlay card without opening a tab (Zen: Alt+click or a modifier + click).
- The overlay hosts a **temporary tab webview** sized to the card (e.g. ~80% of the window) with buttons: **Open as tab**, **Open in split**, **Close**.
- Must float over the page: requires the overlay approach in Section 6. The Glance webview counts toward RAM only while open; destroy on close.
- Link interception: detect modifier+click inside the tab page. This needs a script injected into the tab webview (an initialisation script with **no IPC exposure**) that reports events to the backend over a restricted channel (open question OQ-4).

### 4.9 Persistence (v1)

- **Session restore:** tabs (URL, title, favicon, scroll position optional), Spaces, pins, essentials, split state, window size/position, active tab. Written atomically on change (debounced) so crashes restore cleanly.
- **History:** URL, title, visit time, visit count; searchable via palette; clear-history UI; size cap/retention setting.
- **Bookmarks:** flat list plus optional folders (v1: folders optional); star toggle; import/export HTML (export first, import later).
- **Downloads manager:** list of downloads with progress, pause/cancel, open file, open folder; download location setting; handled through WebView2 download events.
- Storage: a single embedded store (SQLite via `rusqlite` with minimal features, or flat JSON for session state). Decision (OQ-6): **SQLite for history/bookmarks, JSON snapshot for session state.** Use `rusqlite` with minimal features and measure the binary-size impact in Phase 0.
- Passwords/autofill/sync: **not** in v1.

### 4.10 Ad and tracker blocking (must-have)

- Built-in blocking using request interception via WebView2 `WebResourceRequested` (accessed through wry's WebView2 access or `webview2-com`) matching against a compact filter list (EasyList/EasyPrivacy subset), implemented in Rust.
- Requirements: negligible memory overhead (compiled/optimised matcher such as a minimal Adblock-rust style engine, or a lightweight domain-based blocklist as phase 1); per-site toggle; blocked-count indicator in sidebar.
- Cosmetic filtering (element hiding) is a stretch goal.
- Feasibility must be proven in Phase 0 because it depends on what wry exposes (OQ-2).

### 4.11 Extension support (must-have, limited)

- WebView2 supports loading unpacked Chromium extensions only in a restricted form (browser-extension APIs behind an environment option; many APIs unsupported). Target: load unpacked or installed Chrome-compatible extensions where WebView2 allows it, and document which APIs work.
- Requirements: a simple Extensions page to add/remove/enable; no Chrome Web Store integration in v1 unless feasible.
- High risk item; Phase 0 spike required (OQ-2). Owner decision: **if unsupported or unclean, delay launch rather than drop it**.

### 4.12 Default browser registration

- Register as a browser candidate on Windows: URL protocols `http`/`https`, file types `.html`/`.htm`, with the Capabilities registry keys required for "Default apps".
- Tauri's `bundle.fileAssociations` covers file types only; protocol/Capabilities registry entries need NSIS installer hooks or WiX fragments.
- Windows 10/11 does not allow silent default changes; the app opens the Default Apps settings page.
- App must handle being launched with a URL argument (single-instance: forward the URL to the running instance, open it in a new tab).

### 4.13 Custom keyboard shortcuts

- All browser actions bound to configurable shortcuts; settings UI with conflict detection.
- Because focus inside a tab webview swallows app-level key events (constraint 3), shortcuts must work **while a page has focus**. Candidate mechanisms: (a) a script injected into every tab that captures keydown and reports to the backend, (b) native Win32 accelerator/keyboard hook scoped to the window, (c) WebView2 `AcceleratorKeyPressed` event via the controller. Option (c) is preferred if reachable; decide in Phase 0.
- Reserved page keys (e.g. Ctrl+C/V/A) must never be hijacked.

### 4.14 Popups and new windows

- `window.open` and `target=_blank` open in a **new tab** (adjacent to the opener, in the same Space). Use wry's new-window handler to deny the native window and create a tab.
- Known limitation: OAuth/payment flows that depend on `window.opener` can break when the popup becomes a tab. Mitigation: if the opener relationship is needed, allow wry's `NewWindowResponse::Create` with the provided webview features for those cases. Revisit when failures are observed.

### 4.15 Other baseline browser behaviour (needed for a "real" browser even in a prototype)

- Back/forward/reload/stop; loading indicator; favicon fetch.
- Find in page (Ctrl+F), zoom (Ctrl +/-/0).
- HTTPS indicator for the active tab; certificate error interstitial (defer to WebView2 default).
- Permission prompts (camera, mic, geolocation, notifications) via WebView2 permission requested events; simple prompt UI.
- Crash/hang recovery: handle `ProcessFailed`; show a "tab crashed, reload" state.
- New-tab page: minimal (search box plus essentials), no network calls.
- Context menu: default WebView2 menu in v1.

## 5. Tab Sleeping (RAM Strategy)

Owner decision: **aggressively sleep or unload background tabs after a timeout**.

- A background tab becomes **sleeping** after a configurable idle timeout (default: 5 minutes; setting range 1 min to never).
- Sleeping = destroy the child webview, keep metadata: URL, title, favicon, scroll position, optionally a small compressed thumbnail. Restoring = create the webview and navigate.
- Exemptions (never sleep), decided in OQ-5: tab currently playing audio/video, tab with an in-progress download, tab in an open split pane or Glance. Pinned and Essentials tabs are **not** exempt. Form-input and websocket heuristics are not in v1 (possible later).
- Wake on click. Show a dimmed state in the sidebar for sleeping tabs.
- Space switch: tabs in the previous Space follow the same timer, with an optional "sleep immediately on Space switch" setting.
- Memory-pressure trigger: additionally sleep least-recently-used tabs when system or app memory crosses a threshold.
- Data-loss caveat: unloading loses unsaved in-page state. Show a one-time note in settings. Define exemption heuristics carefully (OQ-5).
- Optionally use WebView2 `TrySuspend`/`Resume` (memory-saving suspend) as a lighter first stage before full destruction: suspend at T1, destroy at T2.

## 6. Architecture

```
┌─────────────────────────── Native Window (Mica/Acrylic) ───────────────────────────┐
│ Chrome webview (Svelte)        │  Tab webview(s) (child webviews, one per active   │
│  - sidebar, tabs, spaces       │   tab / split pane)                               │
│  - sits at the back            │  - Always draws above the chrome webview          │
└────────────────────────────────────────────────────────────────────────────────────┘
        ▲  IPC (chrome only)                      ▲ bounds, lifecycle, events
        └───────────── Rust core: TabManager, SleepScheduler, Store, Blocker ──────────┘
```

Components (Rust):

- **TabManager:** creates, destroys, shows, hides, and positions child webviews; maps tab IDs to webview labels; handles split layouts.
- **SleepScheduler:** timers, exemptions, memory-pressure checks.
- **Store:** session snapshot, history, bookmarks, downloads, settings.
- **Blocker:** request interception and filter matching.
- **ShortcutRouter:** collects key events from tab webviews and the chrome webview, resolves bindings.
- **Platform layer:** WebView2 specifics (extensions, downloads, permissions, accelerator keys, process-failed events, Mica via window effects).

### The z-order problem (critical design decision)

Because child webviews always draw above the chrome webview, floating UI needs a workaround. Candidate strategies:

| Strategy                                                 | How it works                                                                                                                                      | Pros                                         | Cons                                                                                                                             |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| A. Overlay webview created last                          | Create a transparent, borderless overlay (separate child webview or separate transparent window) above the tabs for flyouts/palette/Glance chrome | True float, real interactivity               | Extra webview = extra RAM; transparency support with WebView2 child webviews needs verification; focus handling                  |
| B. Freeze frame + chrome swap                            | Screenshot the active tab, hide its webview, show the screenshot in the chrome webview, draw UI over it                                           | Chrome UI stays in one Svelte surface; cheap | Page is frozen/not interactive while overlay is open; screenshot cost; fine for palette and flyout, poor for Glance live content |
| C. Shrink/clip the tab webview                           | Resize tab bounds so no overlap                                                                                                                   | Simple                                       | Violates "must float over the page" requirement                                                                                  |
| D. Separate always-on-top borderless window for overlays | Overlay in its own Tauri window                                                                                                                   | Works without z-order                        | Window management, focus and animation issues                                                                                    |

Working recommendation (to validate in Phase 0): **B for the address-bar palette and compact sidebar flyout** (short-lived, page need not stay interactive), and **A or D for Glance** (needs a live webview). A shared "overlay surface" that is created lazily and destroyed after use keeps steady-state RAM low. This follows the approach (occlusion leases and freeze frames) reportedly used by another open-source Tauri project (codeg).

## 7. Performance Targets (success criteria)

Owner-selected: installer under 10 MB, cold start under 1 second, "lowest MB as possible". Priority order for trade-offs: **RAM > installer size > startup > animation**.

| Metric                       | Target                                                                                          | Notes                                                                                                                                                                                                                                                                                             |
| ---------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Installer size               | < 10 MB                                                                                         | WebView2 Evergreen runtime **not** bundled (rely on the OS-provided runtime, present on Windows 11 and updated Windows 10). Use bootstrapper download only if missing. NSIS preferred over MSI for size. Release profile: `opt-level="z"`, LTO, `codegen-units=1`, `panic="abort"`, strip symbols |
| Cold start to interactive UI | < 1 s                                                                                           | Show the chrome UI first, restore tabs lazily (only the active tab live; others created as sleeping). Avoid loading history/bookmarks until needed                                                                                                                                                |
| RAM, app overhead            | **Provisional target: under 120 MB with 1 tab** (decided, OQ-9); final number set after Phase 0 | Measure the browser's own overhead excluding page content: Rust process + chrome webview + WebView2 browser process. Track and publish numbers every release                                                                                                                                      |
| RAM, tab content             | Controlled via sleeping and suspend                                                             | Per-tab cost is mostly site renderer memory                                                                                                                                                                                                                                                       |
| UI animation                 | 60 fps for sidebar, Spaces, palette                                                             | Lowest priority; disable or simplify animations before spending RAM                                                                                                                                                                                                                               |

Measurement plan: record private working set for all related processes (app, `msedgewebview2.exe` tree) with 1, 5, 10 tabs, idle vs sleeping, compared against Edge and Zen as baselines.

Guidelines to protect the budget: minimal Rust dependencies and features, no heavy UI libraries in Svelte, no image-heavy assets, avoid keeping thumbnails in memory (write to disk, small size), lazy-load settings/history UI, and no background polling.

## 8. Security and Privacy

- Remote pages get **no Tauri IPC**. Capabilities scoped to the chrome webview only; tab webviews created without invoke access.
- Injected scripts in tabs must be minimal and treated as untrusted-page-adjacent; communicate through a narrow, validated channel.
- URL handling: sanitise and validate navigations from the palette and external launches; block `javascript:` and `file:` from external launch unless explicitly allowed.
- Content Security Policy for the chrome UI; no remote code in the chrome webview.
- Privacy defaults: no telemetry, no search suggestions requests until the user opts in, no network calls on the new-tab page.
- Safe Browsing/SmartScreen: WebView2 defaults apply; document what is enabled.
- Cookies/data: stored in a dedicated WebView2 user data folder under `%LOCALAPPDATA%`; "clear browsing data" UI.

## 9. Scope, Risks and Conflicts

**Scope conflict to resolve:** the owner chose "Prototype: tabs + address bar + Zen-style UI shell" as the definition of v1, but also selected all six Zen features, four extra capabilities (ad blocking, extensions, default-browser registration, custom shortcuts), four persisted data types (session, history, bookmarks, downloads), and a hard "delay launch rather than drop" policy for ad blocking and extensions. Combined, this exceeds a prototype. Recommended resolution: **phase the work** (Section 10). "Prototype" = Milestone 1-2; the must-have list gates a "v1 release".

Top risks:

1. **Unstable multiwebview API** (breaking changes, unmaintained). Mitigation: pin versions, isolate all webview handling behind a `TabManager` abstraction.
2. **z-order / overlay UX** (compact mode, palette, Glance). Mitigation: Phase 0 spike on strategies A-D.
3. **Keyboard shortcuts while page focused.** Mitigation: Phase 0 spike on WebView2 accelerator events.
4. **Ad blocking and extension feasibility** through wry. Mitigation: Phase 0 spike; may require direct `webview2-com` calls; extension API coverage in WebView2 is limited.
5. **Mica/Acrylic plus child webviews** (transparent window composition, performance cost).
6. **RAM target vs features.** Overlay webviews, thumbnails and Mica all cost memory; features must be measured against the budget.
7. **Default-browser registration** requires installer customisation outside Tauri's defaults.
8. **Crash recovery and WebView2 deadlocks** (sync command builder deadlock).
9. Maintainer capacity for multiwebview: possible forced migration to direct WebView2 control if Tauri's layer becomes a blocker (fallback plan: use `windows`/`webview2-com` crates directly).

## 10. Milestones

**Phase 0: Feasibility spikes (gate everything else)**

- Create window with chrome webview + 2 child tab webviews on Windows; measure RAM, startup, installer size baseline.
- z-order strategies A-D; pick one for palette/flyout/Glance.
- Keyboard shortcuts while a tab has focus.
- Mica/Acrylic with child webviews.
- Request interception for ad blocking; extension loading in WebView2; popup handling and download events.
- Output: go/no-go notes and revised budget.

**Milestone 1: Shell**

- Window, sidebar, vertical tabs, new/close/switch, address bar palette, navigation controls, favicon and title updates, basic shortcuts.

**Milestone 2: Zen UI**

- Pinned/Essentials, Spaces, compact mode with floating sidebar, split view, Glance, tab sleeping, Mica theme.

**Milestone 3: Persistence**

- Session restore, history, bookmarks, downloads manager, settings.

**Milestone 4: Must-have capabilities**

- Ad/tracker blocking, extension support, default-browser registration/installer hooks, customisable shortcuts.

**Milestone 5: Hardening and release**

- Crash recovery, permissions UI, performance tuning against targets, installer under 10 MB, update mechanism decision, documentation.

## 11. Acceptance Criteria (high level)

- Opens and browses real HTTPS sites in tabs on Windows 10/11.
- All six Zen features function as described, with palette, compact flyout and Glance **floating over** page content.
- Background tabs sleep after the configured timeout and wake correctly with scroll position; exempt tabs (audio, downloads) are not slept.
- Session, history, bookmarks and downloads survive restart; closing the app abruptly restores the last good session.
- Ad blocking measurably reduces requests on a reference set of sites; extension loading works for at least a documented set of extensions.
- Installer < 10 MB, cold start to interactive < 1 s on a reference machine, RAM numbers recorded and tracked per release.
- Tab webviews have no access to Tauri IPC (verified by test page).
- Registers as a selectable default browser; external links open in a new tab in the running instance.

## 12. Decisions and Open Questions

### Resolved

| ID    | Decision                                                                                                                                                                               |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OQ-1  | Minimum Windows 10 22H2+; Windows 10 gets the solid-theme fallback, Windows 11 gets Mica/Acrylic                                                                                       |
| OQ-5  | Sleep exemptions: audio, downloads, split/Glance only. Pinned and Essentials can sleep. Default timeout 5 min                                                                          |
| OQ-6  | SQLite (history, bookmarks) + JSON snapshot (session)                                                                                                                                  |
| OQ-7  | Google default, Yandex selectable, search suggestions opt-in                                                                                                                           |
| OQ-8  | Working name **Zenith** (placeholder, final name before public release). Repo private; choose a license before the first public release (Apache-2.0 or MIT recommended if open source) |
| OQ-9  | Provisional RAM overhead budget < 120 MB with 1 tab; revisit after Phase 0                                                                                                             |
| OQ-10 | 2 split panes in v1, layout model ready for up to 4; no per-Space profile isolation in v1                                                                                              |

### Still open (resolved by the Phase 0 spike)

- OQ-2: How much of WebView2's request interception, extensions, accelerator keys and permission APIs does wry expose? Is `webview2-com` needed directly?
- OQ-3: Does a transparent/Mica window with opaque child webviews render correctly and perform acceptably?
- OQ-4: Mechanism for Glance link detection and shortcut capture from tab pages without exposing IPC.

## 13. Out of Scope for This Document

No code, scaffolding or dependency changes have been made. This file is the only artifact produced.
