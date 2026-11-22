---
title: CEF extension contract
description: Frozen Phase 0 semantics for extension tabs, windows, Spaces, split view, Glance and profiles, plus the compatibility test manifest.
---

Status: Phase 0 contract, 2026-11-21, Zenith 0.1.13. This defines the intended CEF bridge; it is not a claim that CEF already implements it. Current WebView2 evidence and unsupported items are recorded separately. Owners: browser-domain for tab/window mutations, extensions-domain for the bridge and fixtures.

## Identity, scope and permissions

- A Chrome extension `windowId` maps to one Zenith top-level window (`main`, `window-N`, `incognito-N`). A Space, split group, Glance card or extension popup is **not** a window. No operation may cross a Zenith profile: extensions in one process cannot enumerate another process or its tabs. Extra-window numbers are reused for the current live model and are not durable extension identities across restarts; window restoration emits normal creation/focus events with the newly allocated mapping.
- Zenith's stable string tab id is the extension-facing id, exposed through the bridge as the engine's integer id. Persisted state never uses CEF browser/frame ids. A browser generation prevents callbacks from a closed, navigated or recreated browser changing the wrong tab. Stability lasts for that tab's lifetime in its window: copying a session to another window deliberately allocates fresh ids, emits `onCreated` for those tabs and leaves the source tabs unchanged. Extensions must not treat ids as durable cross-session references.
- `currentWindow` and `lastFocusedWindow` mean the calling extension page's top-level window. An action popup/options page inherits its opener's window. `tabs.getCurrent()` returns a tab only for an extension page hosted as an ordinary tab; action popups return `undefined`. Queries without a calling page use explicit process-level last-focused-window ordering, which the adapter must add; per-window chrome/tab focus alone is insufficient.
- `incognito` reflects the actual private request context. Private tabs exist only in private windows. Normal and private windows have separate tab ids, extension storage and lifecycle. An extension runs privately only when both the global and per-extension switches allow it; `split` spanning is unsupported.
- URL, title and favicon are omitted without the required `tabs` or matching host permission. `activeTab` grants temporary access only to the genuinely active/focused pane after an eligible user gesture. Manifest warnings are not authorization. Mutations use existing validated browser-domain operations, not arbitrary backend commands. Unsupported properties and APIs return a defined Chrome-style error and appear in the compatibility matrix; no silent cross-profile or cross-Space fallback.

## Frozen tab semantics

| Chrome concept | Zenith behavior |
| --- | --- |
| `active: true` | The most recently focused pane of the current split group; otherwise the Space's selected tab. Exactly one per window. If no pane has focused yet, use `Space.activeTabId`. |
| `highlighted` | All visible panes in the active split group, in pane order; otherwise the selected tab. Extension selection/highlight changes are unsupported in the first bridge, rather than inventing a second selection model. |
| `windowId` + tab order | Essentials once in global order, then each Space in Space order with folder-aware pinned tabs followed by regular tabs. `Tab.index` is the zero-based position in this flattened projection; hidden Spaces keep their ordering. |
| `hidden` | False for every visible active-split pane and the ordinary active tab; true for background tabs and tabs in non-active Spaces. |
| Space visibility | Queries without filters return tabs from all Spaces in the window, including hidden Spaces. `activeSpaceId` is window state, not a Chrome window or tab-group substitute. A tab remains attached to its Space when another Space is selected. |
| Essentials | `pinned: true`, first in every query for their window. They belong to the window rather than a Space. Activation focuses their existing tab without creating a duplicate. |
| Pinned tabs/folders | `pinned: true`; pinned reset URLs are Zenith metadata, not Chrome tab URLs. Folders are presentation only and are not exposed as Chrome tab groups in the first bridge. |
| Split view | One Chrome window with one active tab and two to four highlighted tabs. `tabs.move` removes the moved tab from its split and inserts it at the requested flattened index; it never moves a whole split group or changes the window count. |
| Glance | Ephemeral and excluded from queries, events and session restore. `tabs.create` creates an ordinary tab, not Glance. Promoting Glance to a tab/split emits the normal created/updated events at promotion. |
| Extension popup | Not a tab or window; excluded from queries and `tabs.getCurrent()`. Its `tabs.query({active:true,currentWindow:true})` resolves to the underlying focused pane. The current `EXT_POPUP_SCRIPT` URL heuristic is replaced by explicit opener identity. |
| Page popup (`window.open`) | A sized popup remains an ephemeral Glance-style surface associated with its opener and excluded from ordinary queries. An unsized new target becomes an ordinary tab next to its opener. `window.opener` is preserved in both cases. |
| Sleeping/discarded | A suspended tab is live but not discarded. A sleeping/unloaded tab remains queryable with `discarded: true`; `tabs.reload`, activation or `tabs.update` wakes it. Its saved URL/title are available subject to permissions. Unsaved form data is not promised. |
| `tabs.remove` | Actually removes ordinary, pinned and Essential tabs, unlike Zenith's unload-on-`Ctrl+W` behavior. Validate every id before a multi-id removal; recompute split/active state and record closed-tab history where applicable. |
| `tabs.create` | Creates in the caller's window and Space, adjacent to the supplied opener when present, otherwise at the configured new-tab position. `active` selects the tab; inactive web content still loads. `index` addresses the flattened regular/pinned order; unsupported combinations fail instead of silently moving Spaces. |
| `tabs.update` | `active:true` selects the Space and focused pane, waking if needed. URL navigation uses existing navigation validation. `pinned` toggles Zenith pinned state and records the pinned URL. Essential state is not writable through this API. |
| `tabs.move` | Same-list reorder is supported; `-1` means the end. Reject an index that implicitly changes Essential/pinned/regular zone or Space. Same-context cross-window moves attach/detach safely into the destination active Space; cross-profile, normal/private and Essential cross-window moves are rejected. |
| `tabs.reload` | Reloads a live page or wakes a discarded one; `bypassCache` maps to the engine's cache-bypass reload. |
| `tabs.query` | Supports `active`, `hidden`, `highlighted`, `currentWindow`, `lastFocusedWindow`, `windowId`, `windowType`, `pinned`, `url`, `title`, `status`, `discarded`, `audible`, `muted` and `index` only to the extent compatibility tests prove them. Unknown filters reject rather than broaden results; URL/title filters cannot leak redacted matches. |
| `windows.*` | Get/list/populate/update/remove map to actual same-context top-level windows in this profile. Window focus activates the focused split pane. `windows.create` supports ordinary windows and same-policy incognito; `popup`, `panel` and `detached_panel` reject until a real native model exists. No cross-profile enumeration. |
| Events | `onCreated`, `onUpdated`, `onRemoved`, `onActivated`, `onHighlighted`, `onMoved`, `onAttached`/`onDetached` and window focus/removed events follow the same operations. No event is emitted for hidden duplicate targets, and stale engine callbacks are dropped. |

The current source of the model is `src/features/browser/types.ts`, `browser.svelte.ts`, `tabs.ts`, `tab-move.ts`, `split.ts`, `glance.ts` and `session.ts`. In particular, `visibleIds` is the focused split group and `order()` is Essentials → pinned/folders → regular. `EXT_POPUP_SCRIPT` in `src-tauri/src/host/tab/scripts.rs` is temporary WebView2 behavior, not this contract's implementation.

## Required API slices

1. **Parity:** install sources/consent, CRX2/CRX3 and unpacked packages, stable keyed ids, documented unpacked-id behavior, enable/disable/remove/pin, options/popup, update check and user-initiated update, content scripts/isolated worlds, MV3 workers, `storage`, `runtime`, `webNavigation`, `declarativeNetRequest`.
2. **New stable requirements:** action badge text/background/color; the tab/window semantics above; direct extension-tab creation with a retained reference; native messaging with explicit host consent and bounded lifecycle; popup/options focus and close/update events; worker/content-script debugging.
3. **Explicitly not promised:** all Chrome APIs, Chrome sync/accounts, unrestricted Manifest V2, extension-created Chrome tab groups matching Zenith folders, `incognito: "split"`, or an extension changing another profile. Every other API starts as unsupported until entered in the tested matrix.

## Curated compatibility manifest

Each row records extension id, version, manifest version, install source, required permissions, test steps, result and known failures for a specific CEF/Chromium build. Installation alone is never a pass.

| Fixture | Required checks | Baseline / CEF result |
| --- | --- | --- |
| Local MV3 content-script fixture (`scripts/dev/e2e/cases/extensions.mjs`) | `<all_urls>`, `document_start`, isolated world, removal | WebView2 156: content marker passed automated E2E; isolated-world verification was manual. CEF: pending. |
| Dark Reader (`eimadpbcbfnmbkopoojfekhnkhdbieeh`) | Content-script isolation, page styling, MV3 worker, welcome/uninstall pages, popup | WebView2 156, 4.9.134: install and restyle recorded by E2E; worker/welcome/uninstall checks recorded manually in [Extension support](../extensions/). Popup still requires a recorded result. CEF: pending. |
| uBlock Origin Lite (`ddkjiahejlhfcafbddmgiahcphecmpfh`) | MV3 worker, `declarativeNetRequest`, options; test with Zenith blocker disabled | WebView2 156, 2026.1103.1743: install recorded by E2E; worker/network blocking were manual. Options still requires a recorded result. CEF: pending. |
| Zenith tab-semantics fixture (new, unpacked) | Every table row above; query permission redaction; Spaces, split focus, Glance/popups, Essentials, folders, sleeping, ordering, multiple windows | New; no implementation or result yet. |
| Badge/popup fixture (new, unpacked) | Badge text/background/color per tab/window; popup sizing; focus, close and update events | New; WebView2 badges unsupported. |
| Native-messaging fixture (new, local test host) | Consent, host manifest validation, bounded messages/processes, disconnect/crash and profile isolation | New; licensing/test-host details pending. |
| Password manager using native messaging | Real host install/consent, locked/unlocked lifecycle, content-script integration | Candidate not selected; requires licensing and test accounts. |
| Tab/session manager | `tabs`/`windows` enumeration, move/remove/reload/update, restore, sleeping and Space semantics | Candidate/version not selected. |

Existing tests do not prove general WebView2 or Chrome compatibility. The two store tests catch installation failures, check built-in disable protection and removal, and observe Dark Reader styling without failing if it is absent; they do not assert all required behaviors. Manual observations are not a repeatable CEF acceptance result. All CEF rows remain pending until executed.

## Regression cases to add before Phase 3 completion

- Split focus changes active/highlighted correctly; background and hidden-Space tabs are queryable without activation.
- Extension popups see the opener, never themselves; direct `tabs.create` preserves the returned tab reference and produces no hidden duplicate.
- Sleeping tabs report discarded and wake on reload/activation; close/crash/recreation rejects stale events.
- Reordered Essentials/pinned/regular tabs return stable indices; cross-Space/profile/private operations fail without side effects.
- URLs/titles are redacted without permission, pages and content scripts cannot reach privileged IPC, and native-messaging hosts terminate cleanly.
