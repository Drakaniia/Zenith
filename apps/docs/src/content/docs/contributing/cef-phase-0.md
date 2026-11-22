---
title: CEF Phase 0 baseline and gates
description: Frozen WebView2 feature, API, test and performance baseline, migration traceability, feasibility findings and unresolved CEF Phase 0 gates.
---

Status: audit completed 2026-11-21 at Zenith 0.1.13 (`8bb4690`), updated 2026-11-22. **Phase 0 conditionally exited for Phase 1 spike.** On 2026-11-22 the stakeholder approved folding the v0.1.13 `memory.csv` baseline into this record, deferring codec/Widevine/Safe Browsing legal and security approvals to the Phase 4 release gate, and starting Phase 1 in an isolated spike crate using a pinned prebuilt CEF binary distribution. Source paths below are relative to `apps/desktop` unless otherwise stated. The companion [CEF extension contract](../cef-extension-contract/) freezes extension semantics and its test manifest.

## Exit status and owners

| Phase 0 requirement | Result | Remaining owner/action |
| --- | --- | --- |
| Inventory WebView2/wry APIs, callbacks, commands, events, paths, scripts and E2E assumptions | Complete for current source; keep this inventory updated when host code changes | Engine adapter owner reviews each replacement before deleting old code. |
| Freeze current feature/browser compatibility matrix | Source/documented baseline frozen; no committed v0.1.13 full E2E result | Deferred to Phase 3 parity gate: run all 34 scenarios against v0.1.13 baseline and CEF adapter. |
| Freeze performance baselines | Historical v0.1.0 and 2026-11-21 v0.1.13 `memory.csv` recorded | Deferred to Phase 2/4 budgets: add Chrome/Firefox/Zen comparators, GPU/idle/network and package metadata. |
| Define extension API semantics | Complete as the intended contract; implementation and CEF results pending | Browser + extensions owners implement/test the companion contract in Phase 2/4. |
| Confirm codec, Widevine, Safe Browsing and redistribution feasibility | Constraints identified; legal/security approvals explicitly deferred to Phase 4 release gate | Phase 1 uses pinned official prebuilt CEF; Phase 4 blocks stable release on codec/CDM/Safe Browsing approvals. |
| Exit: traceability and explicit unsupported-item list | Traceability and list below complete; conditional exit granted 2026-11-22 for Phase 1 spike | No blanket waiver for stable release: deferred items remain hard gates in Phases 2–4. |

## WebView2/wry integration inventory

Direct engine use is concentrated in `src-tauri/src/host/`, trusted-window setup, extensions and media sniffing. The Svelte feature model, application-owned persistence and most domain logic remain engine-independent.

| Current API / source | Disposition and required CEF work |
| --- | --- |
| Trusted UI: `WebviewWindowBuilder`, packaged URL, transparency, shared data directory, extensions, drag/drop and debug flags (`src-tauri/src/app/window.rs:100-169`; `app/commands.rs:25-105`) | Replace trusted hosting and DevTools/version calls; keep window geometry/labels/lifecycle. Recreate authenticated invoke/events, fail closed on untrusted navigation and never enable extensions/bridge inheritance in ordinary pages. |
| Host state/threading: environment, controllers/HWNDs, main-thread `HOST`, `Zenith::Exec`, download zombies (`host/mod.rs:69-190`; `host/chrome.rs:59-226`) | Replace COM state with CEF browser/request-context/frame ownership and generation mapping. Adapt dispatch to CEF UI/IO threads; preserve stable window/tab ids and serialized mutation. |
| Page creation/commands: `WebViewBuilder`, `Navigate`, history, reload/stop, print/save, mute/zoom, suspend/resume, source and DevTools (`host/tab/mod.rs:194-450`) | Replace engine calls behind adapter; keep `tab_create`, `tab_close`, `tab_navigate`, `tab_command`, `tab_sleep`, `tab_content_size`, `tab_set_glance` contracts. |
| Navigation/state callbacks (`host/tab/events.rs:22-349`) | Replace `NavigationStarting/Completed`, URL/history/title, favicon bytes, audio/mute, fullscreen, close and renderer failure callbacks. Keep `TabPatch`, history filtering and window-scoped events. |
| Popups: `NewWindowRequested`, gestures/features/deferral/`SetNewWindow` (`host/tab/new_window.rs:15-145`) | Replace popup lifecycle; preserve opener, popup blocking and foreground/background/split/Glance semantics. Route any replacement page channel through unprivileged isolated messaging. |
| Injected scripts (`app/window.rs:155-157`; `host/tab/scripts.rs:1-171`; `host/tab/events.rs:191-202`; `host/tab/sleep.rs:44-94`; `host/autofill/scripts.rs`; `host/page_tools.rs:20-93,258-478`; `media/detect.rs`) | Adapt the trusted incognito marker, all-frame modified-click, Web Store, autofill, scroll capture/restore, reader, text direction, picture-in-picture, DevTools-panel and media scripts to CEF/isolated worlds. Replace popup API shims with authoritative extension semantics. Main-frame/user-gesture/origin checks, CSP compatibility and stale-script lifetime remain mandatory. |
| Permissions (`host/tab/permissions.rs:14-134`) | Replace permission callback/deferral/profile persistence mapping; keep prompt payload and drop-denies behavior for microphone, camera, location, notifications, clipboard, MIDI, window management, fonts, files, downloads and sensors. |
| Requests/blocking (`host/tab/requests.rs:1-129`) | Replace request filters and synthetic responses; retain matcher/settings/counters. Add service/shared-worker, response and cosmetic coverage; matching must not block the UI thread. |
| Context menu/find/sleep (`host/tab/context_menu.rs:13-125`; `find.rs:17-82`; `sleep.rs:13-97`) | Replace menu construction, find/match APIs, scroll capture, suspend/discard and popup measurement. Preserve commands/events, timeout and recovery behavior. |
| Native layout/focus/input (`host/placement.rs:14-279`; `focus.rs`; `window_proc.rs`; `accelerators.rs:16-121`; `src/platform/layout.svelte.ts`) | Prototype both native child CEF and off-screen/shared-texture. Keep `LayoutSpec`/overlay leases if native wins; otherwise adapt geometry/focus compositing. Prove DPI, animation, IME, accessibility, AltGr and editing shortcuts. |
| Profiles/data clearing (`host/profile.rs:10-114`; `storage/mod.rs:16-49`; `profiles/mod.rs:1-18`) | Replace request/cache context and data masks; preserve process identity and application paths. Add a versioned CEF profile path; leave legacy `WebView2` unchanged. Incognito needs explicit shared in-memory ownership. |
| Downloads/fast handoff (`host/downloads.rs:19-472`; `host/fast_download.rs:1-160`) | Replace download start/progress/pause/resume/cancel/interrupt mapping; preserve records/path policy/CRX routing. Decouple downloads from closed tabs. Keep conservative range-probe/fallback and context-correct cookies. |
| Media observation (`media/sniff.rs:20-477`) | Replace response/header/body, script, cookie and UA adapters; retain bounded HLS/DASH parsing, detection, DRM refusal and jobs. Private-context cookie/job lifetime must be explicit, not taken from the normal chrome profile. |
| DevTools/page tools (`host/devtools.rs:1-133`; `host/page_tools.rs:20-263`) | Replace CDP/target/debug adapter, screenshots/device emulation, task manager and DevTools panels. Remove 1.5-second stray-target adoption in favor of direct extension-tab creation. |
| Extensions (`extensions/mod.rs:189-317`; `install.rs`; `private_profile.rs`; `crx.rs`; `updates.rs`) | Replace profile extension enumeration/load/enable/remove and private synchronization. Preserve registry/consent/package/update policy, with fresh extension storage. Implement the companion API contract and badges/native messaging. |
| Typed frontend bridge (`src/platform/ipc.ts`; `src-tauri/src/app/handlers.rs:1-152`) | Preserve command names, event names and JSON shapes where practical. Replace Tauri/wry transport beneath trusted UI only; no privileged API in remote pages, workers, iframes or extension content scripts. |

### Commands, events and paths that form the compatibility boundary

Engine-facing commands are `ui_ready`, `chrome_devtools`, `tab_create`, `tab_close`, `tab_navigate`, `tab_command`, `tab_sleep`, `tab_content_size`, `tab_set_glance`, `set_layout`, `focus_view`, `find_in_page`, `permission_respond`, `clear_browsing_data`, `apply_settings`, `set_shortcuts`, `debug_cdp` and `debug_layout`. Download/media/extension commands in `app/handlers.rs` remain application contracts but their implementations also cross the adapter.

Engine-originated events are `tab-update`, `tab-focused`, `tab-created`, `tab-open-request`, `tab-close-request`, `popup-blocked`, `permission-request`, `find-result`, `media-changed`, `media-request`, `download`, `download-exists`, `torrent-error`, `shortcut`, `password-suggest`, `password-suggest-close`, `password-offer`, `card-offer` and `ext-install`. Preserve `emit`, `emit_to_window` and `emit_tab` routing. `app-close-requested` remains application lifecycle rather than page-engine IPC.

Current storage is `%APPDATA%\dev.zenith.browser` (or `Profiles\<id>`) for `session.json`, `windows\window-N.json`, `settings.json`, `window.json`, `zenith.db`, `bookmarkbackups`, `profile.lock` and vault data. The roaming base also holds `profiles.json` and `Avatars`; `%LOCALAPPDATA%\dev.zenith.browser` (or profile equivalent) contains `WebView2`, `extensions` (including `registry.json`), `filters`, `wallpapers`, `media-tmp` and `torrents`. `ZENITH_PROFILE_DIR` supplies isolated `roaming`/`local` test roots. Application-owned data and extension registry preferences, sources and incognito policy are preserved; the runtime `profile.lock` is recreated, not migrated. Cookies/site storage/extension storage intentionally start fresh.

### Identity and lifetime defects the adapter must fix

- Callbacks capture only a stable tab id; there is no browser/frame/navigation generation. Late navigation, block flush, sleep, scan, autofill, CRX readback, DevTools retry or popup work can arrive after close/recreation.
- Unknown tab-to-window mappings can fall back to `main`; CEF must tombstone closed generations and reject stale/foreign events instead of rerouting them.
- Extension target discovery uses URL/type/target sets and a delay, then closes/reopens pages into the primary window, losing opener and extension references.
- WebView2 download ownership forces hidden zombie tabs. CEF transfer lifetime must not depend on a live page browser.

## Feature compatibility and traceability

| Feature | Keep | Replace/adapt | Stable disposition |
| --- | --- | --- | --- |
| Browser: tabs, Spaces, Essentials, pins/folders, split, Glance, sessions, sleeping | Svelte model, session metadata, stable ids and user semantics | Browser lifecycle/navigation/events, rendering/focus/zoom/audio/crash, generation-safe sleep/restore | Required parity; no ephemeral surfaces in restored sessions. |
| Sidebar, toolbar, start page, compact/layout | Components, menus, tiles/wallpapers, animation | Placeholder geometry/overlay/focus integration as selected renderer requires | Required parity including mixed DPI/input/accessibility. |
| Blocker | Lists/preferences/site switches/counters/store | Request interception; add full filter/cosmetic/worker/async support | Current subset is not sufficient for stable CEF. |
| Extensions | Install/manage/pin/options/update UI, registry and consent | Entire engine bridge, lifecycle, direct tabs, badges, native messaging, popup/options and private contexts | Required contract + matrix; no full-Chrome claim. |
| Downloads | Records/UI/path collision/resume/actions | Engine download adapter and safe closed-tab ownership | Required parity; security/reputation UX needs proof. |
| Media downloads | Detection policy, HLS/DASH, jobs, merge/FFmpeg, DRM refusal | Response observation, page scripts, cookies/UA and fast-download handoff | Stable parity required; preview gaps must be labelled. |
| Torrents | librqbit jobs, persistence, file choice, seeding/token streaming | Magnet navigation/external scheme and private-context routing | Required parity; no protected-media capability added. |
| Passwords/wallet | Vault crypto, storage, import/export, origin policy and UI | Main-frame autofill detection/fill transport | Required parity without exposing privileged page IPC. |
| History/bookmarks/notes | SQLite/JSON data, queries, backups and UI | History event source; bookmark favicon behavior where engine-dependent | Preserve application-owned data and behavior. |
| Page tools/find/permissions | UI and reader/PiP/source/screenshot/text-direction behavior | Find, print/save, PDF, DevTools panels, task manager, permission persistence and certificate UX | Unsupported equivalents must be explicit before stable. |
| Palette/settings/shortcuts | Actions, settings schema, suggestions policy, binding parser | Trusted invoke/events and pre-page accelerator/input routing | Required parity; search suggestions remain optional. |
| Profiles/incognito/windows | Profile identities/processes, shortcuts, shared-private-session model | CEF contexts/cache/site storage and explicit private cleanup | Required isolation; fresh website/extension storage. |
| Onboarding/importer | Setup flow, browser import and application data formats | New migration notice/backup/recovery path | Current setup retained; migration UX still to design/test. |
| Default browser/shell | Registration, single instance, external URL routing | Engine-specific launch/subprocess argument handling | Windows 11 x64 CEF target only. |
| Updates | Signed Tauri updater policy and session restore | Atomic app+engine payload, manifest verification, rollback and multi-profile coordination | Current separate Evergreen model is replaced. |
| Chatbot | UI/action integration | Any page-context extraction/injection must use unprivileged adapter | Feature behavior needs an explicit E2E case; no current comprehensive engine coverage. |

## Frozen automated compatibility baseline

The suite has **34 registered scenarios**, run in order against a debug isolated profile with `run.ps1 -Fresh -SkipSetup`: `palette-navigation`, `tab-has-no-ipc`, `webview-matches-pane`, `palette-raises-overlay`, `ad-blocking`, `history`, `bookmark`, `glance`, `second-tab-and-split`, `split-pane-close`, `drag-to-pin`, `sleep-and-wake`, `sleep-timeout-scroll-audio`, `find-in-page`, `permission-prompt`, `popup-blocking-and-user-popups`, `crash-recovery`, `zoom`, `download`, `media-downloader`, `download-exempt`, `extension-loading`, `store-extensions`, `compact-mode`, `layout-animation`, `narrow-multiple-toolbars`, `incognito-window`, `spaces`, `essentials`, `shortcut-editor`, `search-suggestions`, `memory-stats`, `session-restore`, `abrupt-restore`.

Harness assumptions that must be ported rather than accidentally treated as feature contracts:

- CDP HTTP/WebSocket on `ZENITH_REMOTE_DEBUG_PORT` (default 9444), `/json/list`, target page URLs and `webSocketDebuggerUrl`.
- Trusted UI recognized at `tauri.localhost` or Vite 1420; commands invoked through `window.__TAURI_INTERNALS__`; incognito marker `window.__ZENITH_INCOGNITO__`.
- CDP `Runtime.evaluate`, trusted `Input.dispatchMouseEvent`, `Network.getCookies` and `Page.crash`; `debug_layout` joins native bounds/target ids to tab ids.
- Real example/IANA/RFC sites, Web Store downloads, search providers and a 100 MB download; local HTTP media fixtures. Tests are stateful and ordered, with restart last.
- Native pane bounds/z-order, live targets disappearing on sleep/close, same-process incognito targets and WebView2 built-in extension enumeration need CEF-specific equivalents.

The current run does not comprehensively cover profiles, vault/cards, notes, bookmark tree/import/export, torrents, DASH, onboarding, extension popup/options/update/badges/native messaging, printing/PDF/codecs/DRM/WebAuthn, accessibility, mixed DPI or network privacy. Those are explicit test gaps, not passing compatibility. No committed v0.1.13 E2E result was found; generated results live in the OS temp directory.

## Frozen performance and package baseline

Evidence: `measurements/memory.csv` (method in [Measurements](../measurements/) and `scripts/measure.ps1`). Private bytes include Zenith and its WebView2 process family.

### Current v0.1.13 baseline (measured 2026-11-21)

| Scenario | Processes | Private MB | Working-set MB | Startup ms |
| --- | ---: | ---: | ---: | ---: |
| Idle, Mica | 7 | 261.7 | 477.2 | 1175 |
| One tab | 8 | 340.6 | 591.9 | 1062 |
| Five tabs | 12 | 470.5 | 854.0 | 1056 |
| Five tabs, four asleep | 10 | 400.6 | 481.8 | 1358 |
| Ten tabs | 17 | 697.9 | 1204.7 | 1343 |
| Edge one tab, fresh profile | 17 | 449.0 | 739.3 | — |

v0.1.13 one-tab process split: Zenith 17.1 MB, browser 45.1 MB, GPU 186.0 MB, utility 21.2 MB, two renderers 68.5 MB (idle trusted UI renderer: 40.3 MB). Derived app-owned overhead is **102.5 MB** (`17.1 + 45.1 + 40.3`), under the `≤120 MB` provisional CEF budget; total one-tab private memory is 24.1% below Edge (`340.6` vs `449.0 MB`). Note that v0.1.13 startup measured 1,056–1,358 ms on this run, exceeding the `<1 second` target under load.

### Historical v0.1.0 baseline (measured 2026-11-05, Windows 11 build 26200, WebView2 156.0.4314.58)

Other applications consumed 75–95% RAM during this run, so values are approximate.

| Scenario | Processes | Private MB | Working-set MB | Startup ms |
| --- | ---: | ---: | ---: | ---: |
| Idle, Mica | 7 | 230.7 | 428.0 | 729 |
| One tab | 8 | 283.8 | 515.5 | 694 |
| Five tabs | 12 | 425.1 | 791.1 | 647 |
| Five tabs, four asleep | 8 | 291.8 | 553.2 | 895 |
| Idle, Solid | 7 | 235.5 | 427.2 | 788 |
| Ten tabs | 17 | 576.7 | 1091.5 | 723 |
| Edge one tab, fresh profile | 18 | 548.4 | 1043.9 | — |

v0.1.0 one-tab process split: Zenith 16.3 MB, browser 45.3 MB, GPU 141.8 MB, utility 21.0 MB, two renderers 56.8 MB. Derived app-owned overhead is **93.7 MB**, not total memory. v0.1.0 NSIS was **1.78 MB** and `zenith.exe` **4.4 MB**. On 2026-11-06, six rotating live sites with 93,011 rules produced 1,093 requests unblocked and 270 reaching network with blocking (87 blocked, documented −75%).

These are baseline observations, not CEF budgets. Chrome/Firefox/Zen comparators, v0.1.13 package sizes, GPU/idle CPU/network traces, and full environment metadata remain open for the Phase 2/4 measurement gate.

## Feasibility and redistribution findings

Sources were accessed 2026-11-21. Technical capability does not imply legal permission.

- **CEF base:** BSD-licensed standalone binary distributions are documented; distributing applications must include CEF and applicable third-party notices. CEF security updates can lag Chromium announcements, so a supported branch, provenance and emergency-update owner are required. [CEF overview/security](https://github.com/chromiumembedded/cef), [distribution/licensing](https://chromiumembedded.github.io/cef/general_usage).
- **H.264/AAC:** stock CEF does not supply proprietary codecs for licensing reasons. A reproducible build using the proprietary-codec/Chrome FFmpeg configuration is technically available, but patent, FFmpeg configuration, notices and redistribution require legal review and exact-package playback tests. [CEF codec/build discussion](https://github.com/chromiumembedded/cef/issues/3559), [maintainer statement](https://github.com/chromiumembedded/cef/issues/3820).
- **Widevine:** Google lists CEF/Electron as supported but requires a license agreement. The CDM is not open source and may not be used/distributed without a separate Google license. Build flags do not grant redistribution or streaming-provider compatibility. **Unresolved legal blocker.** [Widevine overview](https://developers.google.com/widevine/drm/overview), [CDM license](https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/widevine/LICENSE).
- **Safe Browsing:** hash-prefix lookup and optional OHTTP offer privacy mechanisms, but raw-URL lookup is not the chosen design. Current API terms state non-commercial use only and direct commercial use to Web Risk; CEF inheritance of Chrome's service, credentials, OHTTP and UX is unproven. Security/privacy must verify endpoints, keys/quota, database freshness, failure behavior, incognito and interstitials. **Unresolved terms/integration blocker.** [Safe Browsing v5](https://developers.google.com/safe-browsing/reference), [Chrome API keys](https://www.chromium.org/developers/how-tos/api-keys/).
- **Runtime choice:** Chrome bootstrap, Chrome-style windows, Alloy-style embedded windows and off-screen rendering have different capabilities. The runtime/style and extension/rendering combination remains a Phase 2 go/no-go prototype, not a settled API assumption. [CEF architecture](https://chromiumembedded.github.io/cef/architecture).

## Explicit unsupported / not-yet-proven list

- No CEF runtime, adapter, trusted bridge, pinned build/patch pipeline or app+engine package exists yet.
- No current Zenith-aware `chrome.tabs`/`chrome.windows`; badges, native messaging and direct extension-tab identity are unsupported today.
- Current blocking lacks service/shared-worker and cosmetic coverage and intercepts on the UI thread.
- CEF equivalents are unproven for print/save, task manager, find, suspend/discard, context menus, download resume/error taxonomy, favicon bytes, permission persistence and specific DevTools panels.
- H.264/AAC, Widevine, privacy-preserving Safe Browsing, WebAuthn/passkeys, PDF and certificate UX are not accepted until exact-build/security/legal tests pass.
- Native versus off-screen rendering, accessibility/IME/input, protected video and extension compatibility are unselected and unmeasured.
- Fresh-profile network destinations, incognito disk artifacts, migration/rollback, corrupt/interrupted atomic updates and the CEF memory/package budgets have no passing evidence.
- Chrome accounts/sync, unrestricted MV2, cross-platform/ARM64/Windows 10 support and total Chrome API parity are explicitly out of scope, not deferred promises.
