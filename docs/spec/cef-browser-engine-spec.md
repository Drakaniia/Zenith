# Zenith CEF Browser Engine Specification

**Status:** Proposed; requirements gathered through stakeholder interview  
**Date:** 2026-11-21  
**Scope:** Replace Microsoft Edge WebView2 with a bundled, controllable Chromium Embedded Framework (CEF) runtime while retaining Zenith’s identity, interface, feature model, and lightweight behavior.  
**Implementation authorization:** This document is a specification only. No implementation is included or implied by its creation.

## 1. Executive summary

Zenith currently combines a Svelte 5 browser interface, a Rust/Tauri application layer, and raw wry/WebView2 child views for web content. WebView2 keeps installation small and delegates Chromium security updates to Microsoft, but it limits Zenith’s control over extensions, network handling, profiles, browser services, media components, privacy defaults, and automation.

The selected direction is a **custom CEF-based browser shell**:

- Keep Tauri as the application shell and retain the Rust/Svelte feature architecture and typed command/event boundary.
- Remove WebView2 completely, including as the host for the trusted Svelte interface. CEF must host both trusted application UI and untrusted web content.
- Bundle one pinned CEF runtime with each Zenith release; do not depend on a system browser engine or download the engine on first run.
- Permit a small, documented, auditable CEF/Chromium patch set when stock CEF cannot meet codec, DRM, extension, privacy, integration, or resource requirements.
- Preserve all current user-facing behavior and persistent Zenith-owned data for stable release. Preview builds may temporarily lack advanced media features, provided those gaps are explicit.
- Start website and extension storage fresh during migration, but preserve Zenith settings, sessions, profiles, history, bookmarks, notes, passwords, wallet data, download records, extension registry preferences, and other application-owned state.
- Treat “lightweight” as measured runtime efficiency plus explicit package/installed-size budgets, not the current sub-10 MB installer goal.
- Deliver through feasibility spikes, an opt-in staged preview, and a gated stable release. Do not maintain two equally supported production engines indefinitely.

CEF is not a drop-in full Chrome browser. In particular, extension support, browser UI, tab-management APIs, and Chrome-style services must not be assumed. The project must prove the highest-risk integrations before committing to full migration.

## 2. Problem statement

“Make Zenith a real browser” means giving the application direct, supportable control over the engine and browser services rather than merely displaying webpages successfully.

The requested control areas are:

1. **Extensions:** action badges, Zenith-aware tab APIs, extension lifecycle, native messaging, popups/options, and fewer undocumented API gaps.
2. **Network and content:** reliable request/response access, service/shared-worker coverage, advanced filtering, cosmetic hiding, and processing away from the UI thread.
3. **Browser services:** profile isolation, cookies, cache, permissions, certificates, downloads, site isolation, and storage management.
4. **Media and compatibility:** H.264/AAC, Widevine playback, PDFs, WebAuthn/passkeys, certificate UX, and mainstream website behavior.
5. **Engine/privacy customization:** controlled flags and services, no unsolicited vendor traffic, and documented security/privacy exceptions.
6. **Debugging and automation:** stable DevTools/CDP integration, extension debugging, crash diagnostics, and reproducible end-to-end testing.

The current implementation also has structural constraints: native page windows require explicit placement and z-order management; request interception touches the UI thread; extension-created pages are adopted indirectly through DevTools target discovery; and WebView2’s extension tab model does not match Zenith’s Spaces, split view, and Glance model.

## 3. Goals

### 3.1 Product goals

- Deliver a browser controlled by Zenith rather than by WebView2’s exposed API subset.
- Preserve the current minimalist Zen-style experience and all documented stable features.
- Match Chromium website behavior and browser controls, not Google account/service bundling.
- Offer broader, explicitly documented extension support with a maintained compatibility matrix.
- Support common proprietary codecs and DRM playback without allowing DRM circumvention or protected-media downloading.
- Retain Zenith’s no-telemetry/no-account posture and publish every application-initiated network destination and purpose.
- Remain lighter than mainstream full browsers under reproducible, matched workloads.

### 3.2 Engineering goals

- Isolate CEF-specific code behind a narrow engine adapter instead of spreading engine calls throughout feature domains.
- Preserve typed frontend calls and events wherever practical; contract changes require explicit versioning and migration.
- Keep untrusted pages unable to invoke application commands or access trusted UI capabilities.
- Make CEF and Chromium versions, build arguments, patches, licenses, symbols, and security provenance reproducible.
- Automate engine security monitoring and release an emergency update within the target defined in Section 15.
- Preserve existing test scenarios and extend them for CEF-specific lifecycle, rendering, extension, media, and recovery cases.

## 4. Non-goals and scope boundaries

The first stable CEF release does **not** aim to:

- Support macOS, Linux, Windows ARM64, or Windows 10. The release target is Windows 11 x64 only.
- Reproduce Google Chrome’s account sign-in, cross-device sync, Google integrations, promotions, telemetry, or bundled background services.
- Guarantee every Chrome extension API or every Chromium enterprise feature. Unsupported behavior must be documented rather than hidden.
- Build a new web engine from scratch or adopt Servo for production browsing.
- Recreate Zen Browser’s Firefox/Gecko implementation or replace Zenith’s UI with Zen’s interface.
- Maintain unrestricted Manifest V2 support as a stable-release requirement.
- Circumvent DRM, download protected streams, weaken certificate validation silently, or allow remote pages to access privileged IPC.
- Retain the old `<10 MB` installer target or the old `284 MB` total one-tab measurement as an absolute ceiling. The new budgets are in Section 12.
- Introduce per-profile or per-tab user-selectable rendering engines.
- Maintain WebView2 as a permanent fallback production backend.

Cross-platform portability may influence interface design, but no untested platform claim is permitted.

## 5. Decisions from stakeholder interview

| Area | Decision |
| --- | --- |
| Primary engine | Embedded Chromium through CEF |
| Browser shell | Build and own the native CEF integration and extension bridge |
| Application framework | Retain Tauri, Rust domains, Svelte UI, and typed application contracts |
| WebView2 | Remove completely; CEF hosts trusted UI and untrusted pages |
| Current behavior | Preserve all current behavior for stable release |
| Preview exceptions | Advanced media features may be incomplete and clearly labelled |
| Distribution | One bundled, pinned CEF runtime per application release |
| CEF customization | Custom build allowed; maintain the smallest auditable patch set |
| Extension target | Current working behavior plus badges, tab APIs, native messaging, and robust popups/options |
| Parity fallback | Publish a capability matrix; do not block stable release on impossible full Chrome parity |
| Rendering | Prototype native windowed and off-screen/shared-texture approaches |
| Website migration | Fresh cookies/cache/site storage/permissions; warn before upgrade |
| Extension migration | Reinstall packages and retain registry preferences; fresh extension storage |
| Legacy engine data | Retain old WebView2 data until explicit user removal |
| Privacy | No unsolicited vendor traffic; documented user/security exceptions only |
| Blocking | Full filter syntax, cosmetic filtering, and worker-aware interception required |
| Fingerprinting | Experimental optional feature with per-site compatibility controls |
| Safe browsing | Privacy-preserving Safe Browsing integration |
| Update model | Atomic, signed, full app-plus-engine updates |
| Emergency maintenance | Target release within 72 hours of a usable upstream security fix |
| Rollout | Feasibility spike, then staged opt-in preview, then stable |
| Scheduling | Dependency-based phases and gates, no calendar commitments |

## 6. Current architecture and migration impact

### 6.1 Existing architecture

The current application has:

- A transparent Tauri/wry WebView2 instance for trusted Svelte UI.
- One raw wry child WebView2 per live tab, split pane, or Glance view.
- A shared WebView2 environment/browser process per profile, with per-site renderer processes.
- No Tauri IPC injected into page views; only narrowly scoped initialization scripts and reserved `window.open` names.
- Main-thread-only WebView2 objects and a posted window-message queue to avoid re-entrancy.
- Placeholder rectangles and `ResizeObserver`-driven layout, native resize handling, DPI conversion, clipping during animations, and fullscreen overrides.
- An overlay lease that raises transparent chrome above native page windows while menus, palette, prompts, and compact UI are visible.
- Rust-owned SQLite/JSON persistence and separate per-profile directories/processes.
- In-memory InPrivate contexts shared across incognito windows and discarded after the last incognito tab closes.

Primary implementation references:

- `apps/desktop/src-tauri/src/host/`
- `apps/desktop/src-tauri/src/host/tab/`
- `apps/desktop/src-tauri/src/extensions/`
- `apps/desktop/src-tauri/src/media/`
- `apps/desktop/src-tauri/src/profiles/`
- `apps/desktop/src-tauri/src/incognito/`
- `apps/desktop/src/platform/layout.svelte.ts`
- `apps/desktop/src/platform/webview-api.ts`
- `apps/desktop/src/features/browser/`
- `apps/docs/src/content/docs/contributing/architecture.md`

### 6.2 Components expected to remain engine-independent

Preserve unless a prototype demonstrates a concrete incompatibility:

- Svelte components and feature stores for tabs, Spaces, Essentials, pins, split view, Glance, bookmarks, history, settings, profiles, palette, sidebar, toolbar, shortcuts, downloads, notes, passwords, onboarding, and updates.
- Browser/session metadata and command/action registries.
- SQLite worker, JSON atomic snapshots, backup/migration rules, and domain query ownership.
- Vault encryption and application-level password/card data.
- Filter-list storage and preference model.
- Media parsing, HLS/DASH jobs, MP4 merge logic, torrent engine, and download record model, subject to new engine adapters.
- Windows default-browser registration, single-instance routing, profile identities, shortcuts, and updater policy.
- Existing Tauri command names, event names, JSON shapes, and storage paths by default, because installed versions depend on them.

### 6.3 Components requiring replacement or adapters

- Trusted UI hosting currently supplied by wry/WebView2.
- Page browser creation, navigation, lifecycle, focus, zoom, audio, fullscreen, favicons, find, print/save, source view, context menus, and permissions.
- Native placement/z-order or replacement compositor.
- Accelerator interception before page delivery.
- Cookie/cache/storage/profile operations.
- Request/response interception and media sniffing.
- Download-start/save lifecycle and page-download fast-mode handoff.
- Extension enumeration, loading, lifecycle, native messaging, badge state, popups, options, and browser API integration.
- CDP sessions, target tracking, automation, crash reporting, and DevTools opening.
- Engine subprocess launch/shutdown and installer runtime payload.

## 7. Target architecture

### 7.1 Process and trust model

The target contains these logical roles, whether implemented in one executable with CEF subprocess launches or separate helper executables:

1. **Application/browser process:** Rust/Tauri owns windows, settings, sessions, storage, profiles, engine lifecycle, privileged commands, and update policy.
2. **Trusted UI renderer:** loads only packaged Svelte assets or the controlled development URL. It exposes a minimal authenticated bridge to existing typed commands/events.
3. **Untrusted page renderers:** created by CEF/Chromium with site isolation and no Zenith privileged bridge.
4. **CEF subprocesses:** renderer, GPU, utility, network, and other processes as selected by supported CEF configuration. Zenith must not promise fewer processes than security isolation requires.
5. **Native engine adapter:** owns CEF callbacks, thread dispatch, browser-to-tab identity mapping, and translation to stable application events.

Remote pages, frames, workers, extension content scripts, navigation errors, downloaded files, manifests, and extension packages remain untrusted input. No page-origin string, tab id supplied by a page, or DevTools target metadata alone may authorize privileged work.

### 7.2 Engine adapter boundary

Define an internal engine contract covering:

- Browser/context creation and destruction.
- Navigation and history operations.
- Visibility, bounds, focus, fullscreen, zoom, suspend/resume, and discard.
- Load/title/URL/favicon/audio/error/crash events.
- Permission requests and persisted decisions.
- Cookie/cache/site-data operations.
- Request/response observation and filtering.
- Download callbacks and safe handoff to Rust download workers.
- Extension lifecycle and browser integration events.
- DevTools command/event channels.
- Trusted UI command transport and application event emission.

The adapter must preserve window-scoped and tab-scoped event routing. CEF integer browser/frame identifiers may change across navigations and must not replace Zenith’s stable tab ids in persisted state. Maintain an explicit mapping with generation identifiers to reject stale callbacks after close, crash, or recreation.

### 7.3 Tauri integration

Retain Tauri’s application lifecycle, plugins, updater, dialogs, Windows integration, and command registry. Replace the wry-provided trusted webview with a CEF-hosted trusted browser surface. The exact process-entry initialization order and window embedding mechanism are spike deliverables.

The trusted bridge must:

- Expose only explicitly registered application commands.
- Use structured, validated payloads compatible with current TypeScript declarations where feasible.
- Never be installed into ordinary page browsers or inherited through popup/opener relationships.
- Fail closed if a trusted document navigates to an untrusted origin.
- Prevent pages from reaching the bridge through iframes, `window.open`, `postMessage`, custom schemes, redirects, drag/drop, or DevTools targets.
- Provide typed backend-to-UI events without allowing arbitrary listeners to become a privileged page API.
- Preserve development mock and isolated test-instance workflows.

### 7.4 Profile contexts

Each Zenith profile retains its own process, application identifier, single-instance key, data directories, and CEF request/cache context. Different profiles may run side by side without shared cookies, cache, site storage, extensions, or settings.

The CEF profile directory must be distinct from legacy `WebView2` data. Add a versioned, engine-specific directory through `storage::Paths`; do not reinterpret or mutate the legacy folder as a CEF profile.

Incognito requirements:

- Use an isolated non-persistent CEF request context.
- Share incognito state across incognito windows only to the extent current behavior promises.
- Write no history, download history, session, window geometry, or changed settings.
- Prevent disk cache and persistent site/extension storage.
- Destroy all incognito contexts and in-memory state when the final incognito tab/window closes.
- Keep explicit per-extension incognito permission and global incognito-extension policy.
- Ensure torrent/media/download workers cannot accidentally persist private metadata or reuse private cookies after context destruction.

## 8. Rendering, layout, input, and accessibility

### 8.1 Required prototype comparison

Prototype both:

- **Native windowed CEF browsers:** external-parent child windows integrated with current pane placement and overlay leases.
- **Off-screen/shared-texture CEF browsers:** page frames composited with the trusted UI into a single surface or controlled composition tree.

Select using measurements and behavior, not aesthetics. Compare memory, GPU copies, frame latency, video playback, resize smoothness, DPI changes, input latency, accessibility, DevTools, context menus, fullscreen, crash recovery, and extension compatibility.

### 8.2 Behavior that either approach must preserve

- One to four live panes in split layouts, stacked/grid arrangements, and animated transitions.
- Glance above the chrome card while ordinary overlay UI remains correctly ordered.
- Live page/video rendering under transparent floating UI.
- No unpainted strips, one-frame offsets, focus stealing, or visible resize lag.
- Native window-edge resize behavior independent of a busy JavaScript UI thread.
- Correct physical/logical pixel conversion at mixed monitor DPI and when moving between monitors.
- HTML fullscreen, window fullscreen, exit paths, and recovery after renderer crash.
- Keyboard/mouse/touch/pen input, text selection, IME, clipboard, drag/drop, scroll chaining, pointer capture, and browser gestures.
- Existing customizable accelerators intercepted before pages without breaking reserved editing chords or AltGr.
- Browser accessibility tree, screen-reader operation, high contrast, keyboard-only navigation, and accessible permission/download/security prompts.

Off-screen rendering must not ship if it silently regresses accessibility, protected video, GPU interoperability, input routing, printing, or extension windows.

## 9. Extension system

### 9.1 CEF feasibility constraint

CEF’s own extension documentation/issue history distinguishes Alloy-style and Chrome-style browsers. Current CEF guidance states that the Alloy extension API has been removed and Chrome extension APIs are supported with Chrome-style browsers/windows, while external-native-parent and off-screen scenarios commonly use Alloy style. Therefore:

- A mandatory spike must determine whether the required APIs can run with the chosen embedded rendering approach.
- Stock CEF must not be represented as providing full Chrome extension support automatically.
- If Chrome-style windows and Zenith’s custom layout are incompatible, evaluate a minimal CEF/Chromium patch, an alternate composition architecture, or a documented API subset.
- Stable release follows the stakeholder decision to publish a capability matrix rather than claim or require total Chrome parity.

### 9.2 Stable-release requirements

Preserve current install sources and behavior:

- Chrome Web Store id/link install through the existing consent flow.
- CRX2/CRX3 packages and unpacked folders.
- Stable Chrome ids for keyed packages; documented behavior for unpacked development installs.
- Enable/disable, remove, pin, options, popup, update check, user-initiated update, and permission display.
- Extension pages opened through extension APIs must map directly to Zenith tabs without hidden duplicate pages or delayed target adoption.
- Per-profile extension isolation and per-extension incognito control.
- Content scripts/isolated worlds, background service workers, `storage`, `runtime`, `webNavigation`, `declarativeNetRequest`, and other APIs already required by tested extensions.

Add these key gaps:

- Action icon badge text/background/color updates in toolbar/sidebar surfaces.
- `tabs` and `windows` APIs mapped intentionally to Zenith’s stable tabs/windows.
- Native messaging with explicit installation/permission policy and safe host process lifecycle.
- Reliable action popups, options pages, extension-created tabs, focus/activation, and close/update events.
- Extension service-worker and content-script debugging.

### 9.3 Zenith tab semantics

Define and test how extension operations behave for:

- Active tab in a split group versus most recently focused pane.
- Background tabs and sleeping/discarded tabs.
- Glance pages and extension popups, which must not appear as ordinary restorable session tabs.
- Essentials, pinned tabs, Spaces, and hidden tabs in non-active Spaces.
- Incognito split contexts and extension spanning restrictions.
- Drag-reordered tabs and windows with separate profiles.
- `tabs.move`, `tabs.remove`, `tabs.reload`, `tabs.update`, `tabs.query`, and `windows.*` operations.
- Tab URLs/titles withheld from extensions without required permissions.

No extension may silently enumerate another profile or treat a split group as a separate browser window unless the documented mapping explicitly says so.

### 9.4 Compatibility matrix and test set

Publish per-CEF-version results for supported APIs and tested extensions, including at minimum:

- Dark Reader.
- uBlock Origin Lite.
- The existing Manifest V3 content-script fixture.
- One password manager using native messaging, if licensing/test accounts permit.
- One tab/session manager exercising `tabs`/`windows`.
- One extension with action badges and popup resizing.

Record version, manifest version, required permissions, working APIs, known failures, workaround, and test date. Unsupported APIs must fail predictably and be visible to users/developers; do not claim “full extension platform” based only on installation success.

## 10. Content blocking, privacy, and network control

### 10.1 Blocking requirements

The stable CEF release must preserve current built-in lists, per-site disable state, counters, list updates, and first-party document protections while adding:

- Common Adblock Plus/EasyList/EasyPrivacy network-filter syntax, including resource types, domain/third-party constraints, important rules, and exceptions.
- Cosmetic element hiding and CSS/injection rules through isolated, least-privilege mechanisms.
- Service-worker and shared-worker request coverage where supported by Chromium’s architecture.
- Response access needed by media detection and filtering without unbounded buffering.
- Asynchronous/worker-thread parsing and matching so page loads never wait on the Rust/Tauri UI thread.
- Defined interaction order among Zenith blocking, Safe Browsing, extension `declarativeNetRequest`, redirects, and page CSP.

A malicious filter list or page must not cause unbounded memory, regex denial of service, arbitrary script execution in privileged contexts, or indefinite request deferral. Set parsing/count/size/time limits and fail open/closed according to a documented per-rule security policy.

### 10.2 Anti-fingerprinting

Anti-fingerprinting is experimental and optional, not a stable parity blocker. It must be disabled by default until compatibility is measured, have per-site controls, and document breakage/fingerprinting trade-offs. Do not promise anonymity or Tor-equivalent protection.

### 10.3 Network privacy inventory

Default builds must disable unsolicited Google/vendor telemetry, metrics, variations, crash uploads, promotional fetches, background component activity, and other undocumented home-phoning. Permitted exceptions require user action or a documented security function:

- Safe Browsing using the privacy-preserving mode.
- Widevine component acquisition/update when required and consented.
- User-requested Chrome Web Store install/update checks.
- Zenith filter-list and application update checks under existing settings.
- Search suggestions only when enabled.
- Web requests made by pages or explicitly enabled extensions.

Test a fresh profile with no page navigation and record process-initiated network traffic. Any newly discovered endpoint must be removed, justified, documented, and covered by a setting where appropriate.

## 11. Media, downloads, and web compatibility

### 11.1 Required compatibility

- HTML5 audio/video, WebRTC, WebGL/WebGPU as supported by selected CEF build/hardware, and mainstream site behavior.
- H.264 and AAC through a legally reviewed codec-enabled distribution.
- Widevine CDM playback for DRM services, with installation/update/licensing/region behavior documented.
- PDF viewing, printing/save as, find in page, zoom, page source, and DevTools.
- WebAuthn/passkeys and Windows platform authenticator integration.
- Clipboard, notifications, camera, microphone, geolocation, MIDI, local fonts, file access, and window-management permission flows where supported.
- OAuth/payment popup flows with working `window.opener` semantics and no hidden stray browser targets.

Supporting DRM playback does not change Zenith’s prohibition on downloading protected media.

### 11.2 Download behavior

Port current download records, safe filenames, resume/pause/retry/cancel behavior, progress events, and Mark of the Web. Preserve media/torrent functionality for stable, including:

- HLS/DASH/plain-file detection and authenticated downloads using the correct profile cookies.
- Referer/Origin/User-Agent propagation and HTTPS downgrade prevention.
- Private/local-network destination policy and bounded manifest parsing.
- Byte-range resume, segmented downloads, native MP4 merge, optional FFmpeg conversion, and full-disk failures.
- Magnet and `.torrent` routing, file selection, seeding, persistence, and token-protected loopback streaming.
- Closed-tab/zombie behavior where engine downloads require a live owner; preferably decouple downloads from tabs without changing semantics.
- Fast-download takeover only when the same request semantics and byte count are proven; otherwise leave the engine download untouched.

Preview may temporarily omit advanced media features, but the UI must label them unavailable and stable must restore the documented behavior.

## 12. Lightweight requirements and measurement gates

### 12.1 Provisional stable budgets

These are release gates unless a spike produces measured evidence and an explicitly approved spec revision:

- Self-contained NSIS installer: **≤150 MB**.
- Installed application and runtime files: **≤400 MB**, excluding user data, cache, symbols, and optional FFmpeg.
- Process start to visible trusted UI: **<1 second** under the documented reference workload.
- App-owned overhead with one ordinary tab: **≤120 MB**, defined as Rust/browser host + trusted UI renderer + CEF browser process private bytes.
- Total one-tab memory: lower than current stable Chrome, Edge, Firefox, and Zen under matched fresh-profile conditions on the reference machine.
- Five-tab and ten-tab memory, sleeping-tab recovery, CPU at idle, and GPU memory must be reported; no hidden preloading or disabling security features may be used to win comparisons.
- Background idle: no recurring avoidable CPU activity or unexpected network traffic; extensions and user media are excluded and separately labelled.

The old total one-tab figure (`284 MB`) and app-overhead figure (`93 MB`) remain baseline observations, not guaranteed outcomes after replacing the engine and compositor.

### 12.2 Measurement method

Extend `apps/desktop/measurements/` and measurement scripts to include:

- CEF/Chromium/Zenith versions, build flags, patch revision, codec/DRM state, OS build, hardware, GPU/driver, power mode, and memory pressure.
- Cold and warm startup, visible UI, first contentful browser page, restored session, and new-profile onboarding separately.
- Private bytes per process role plus shared/GPU memory with a documented aggregation method.
- Idle/new-tab, 1/5/10 light pages, representative heavy sites, video, split view, Glance, sleeping, and memory-pressure scenarios.
- Installer size, installed size, update download size, and optional rollback disk cost.
- Native-window versus off-screen prototype frame times, input latency, and GPU copies.
- Fresh comparison-browser profiles and identical tab/sleep/extension conditions.

Do not remove site isolation, Safe Browsing, or other security controls solely to satisfy resource budgets.

## 13. Data migration and upgrade experience

### 13.1 Preserve application-owned data

For every profile, retain without silent schema resets:

- Profile list, last-used profile, names, colors, avatars, shortcuts, and process identity.
- Settings, keyboard bindings, Spaces, Essentials, pins, split/session metadata, open tab URLs, scroll offsets where stored, and onboarding completion.
- History, bookmarks/tree/tags/keywords/workspaces/backups, notes, and download records.
- Password vault, payment cards, recovery metadata, and user wallpapers.
- Filter lists, custom rules, per-site blocker state, and extension registry preferences.
- Torrent application state where compatible and files already downloaded.

Existing Tauri command names, events, settings/session JSON shapes, and paths remain compatibility contracts. Version new engine-specific fields rather than casually renaming existing values.

### 13.2 Intentionally fresh browsing state

Do not promise migration of:

- Cookies or website sign-ins.
- HTTP cache, localStorage, IndexedDB, Cache Storage, service/shared-worker storage, or other site data.
- Site permission decisions stored in WebView2.
- Extension local/sync/session storage and extension sign-ins.

Restored tab URLs may therefore open signed out. The migration notice must state this before installation and identify that bookmarks, passwords, settings, history, and tabs are retained.

### 13.3 Extension restoration

Use the preserved extension registry to reinstall previously installed packages where possible. Preserve extension id, enabled state, pin state, incognito choice, store source, and a compatible version. Extension data starts fresh and extensions may require setup/sign-in.

If a package cannot be restored, show a post-migration report with name/id, reason, and retry/install action. Never install new unrequested extensions or bypass permission consent during migration.

### 13.4 Notice, retention, and recovery

- Before a user upgrades an existing WebView2 installation, show a clear one-time migration notice and require acknowledgement for the intentional sign-out/site-storage reset.
- Preview migration must be opt-in and use a copy or isolated profile unless an explicitly tested reversible path exists.
- Preserve legacy WebView2 directories unchanged until the user explicitly deletes them through a documented cleanup action.
- Retain pre-migration backups of application-owned JSON/SQLite data and verify their integrity before first write.
- Startup migration must be idempotent, resumable after forced termination, and transactional where possible.
- Never delete old data merely because a later CEF session starts successfully.
- Provide a supported recovery route for application data if first CEF startup fails. This may mean restoring a prior signed application package plus untouched legacy data; it does not imply a permanent dual-engine runtime.
- Define rollback compatibility and expiry before preview. New CEF-era data must not be silently readable as older formats unless backward compatibility is tested.

## 14. Security and privacy behavior

### 14.1 Safe Browsing and downloads

Integrate Safe Browsing using local/hash-based lookups and proxy/privacy mechanisms supported by the chosen implementation. Document lookup/update endpoints, data sent, failure behavior, settings, and coverage limits.

Default policy is deny-by-default with explicit user override:

- Invalid certificates produce a clear interstitial; no silent bypass and no one-click accidental proceed.
- Camera, microphone, geolocation, clipboard, local fonts, files, MIDI, sensors, and similar sensitive permissions require informed prompts and safe defaults.
- Dangerous downloads receive reputation/signature/type warnings where available, user confirmation, safe filenames, and Mark of the Web.
- External protocols require an explicit prompt, show the target application/scheme, and reject malformed or silently repeated launches. Magnet remains a deliberate Zenith-handled exception.
- Pending permission/download/navigation requests are denied or cancelled safely if their tab, window, or renderer closes.

### 14.2 Chromium hardening

- Keep Chromium sandbox, site isolation, process separation, and exploit mitigations enabled.
- Do not expose remote debugging on a public interface. Require opt-in, loopback-only binding and access control appropriate to the debugging channel.
- Do not pass untrusted page arguments into engine command-line switches.
- Validate custom schemes and internal pages; trusted internal origins cannot be navigated to arbitrary remote content while retaining privileges.
- Treat native messaging hosts and extension packages as privileged code with explicit consent and auditable process permissions.
- Avoid persistent sensitive logging of cookies, passwords, tokens, full media URLs, or private browsing data.
- Package debug symbols separately; retain them securely for crash diagnosis without enabling telemetry uploads by default.

## 15. Build, release, and engine maintenance

### 15.1 Reproducible runtime supply chain

Each release must record:

- Exact Zenith, CEF, and Chromium versions and commit identifiers.
- CEF branch/support status and Chrome security baseline.
- Complete GN/CMake/build configuration and toolchain requirements.
- Every local patch with purpose, upstream issue, security impact, owner, and removal/review condition.
- Codec and Widevine packaging choice, license notices, third-party attributions, and redistribution review.
- Runtime file manifest, hashes, signatures, symbols, and SBOM/provenance artifacts where supported.
- Automated license scanning and no undocumented copied proprietary browser components.

Prefer upstream CEF APIs/build options over patches. Patches must be small, individually buildable/testable where feasible, and reviewed at every engine upgrade.

### 15.2 Atomic updates

The application and exact CEF runtime update as one signed full package:

- Reuse the existing signed updater trust model, per-user installation, quiet installation on exit, explicit restart action, and session restoration.
- Download to a staging area, verify signature/hash and runtime manifest before replacing active files, and never run a mixture of old/new engine components.
- Do not hold an entire large update in memory without a measured resource reason; streaming verification/staging is preferred if compatible with the updater’s signature guarantees.
- Preserve the previous verified installation for rollback, with disk cleanup only after successful launch and within the installed-size accounting policy.
- Coordinate updates across simultaneously running profiles and flush application stores before installation.
- Interrupted updates must boot the previous complete package or complete recovery, never a partially updated browser.
- Release metadata must expose engine version/security baseline in Settings/About and release notes.

### 15.3 Security cadence

- Subscribe to Chrome stable security releases and CEF build/branch announcements.
- Automate comparison of the bundled Chromium baseline against newly announced security fixes.
- Track upstream CEF availability lag and distinguish “Chrome fix announced” from “usable CEF fix available.”
- Target a signed Zenith emergency release within **72 hours after a usable upstream fix is available** for critical/high exploitable issues affecting Zenith.
- Rebuild/test custom patches and high-risk integration areas before release; emergency releases may be narrowly scoped but cannot skip updater/signature and smoke/security checks.
- Define end-of-support for old CEF branches and do not ship an unsupported engine silently.
- Maintain a vulnerability disclosure and emergency rollback process.

## 16. Delivery phases and decision gates

No calendar estimates are attached. A phase begins only after its dependencies and preceding exit gate are satisfied.

### Phase 0 — Baseline and requirement freeze

- Inventory every WebView2/wry API use, callback, command, event, profile path, injected script, and E2E assumption.
- Freeze the current feature/browser compatibility matrix and performance baselines.
- Define supported extension API semantics for Spaces, split view, Glance, sleeping, and profiles.
- Confirm codec, Widevine, Safe Browsing, and redistribution/licensing feasibility.
- **Exit:** traceability from every current feature to keep/replace/adapt work and an explicit unsupported-item list.

### Phase 1 — CEF runtime and Tauri integration spike

- Build a pinned CEF runtime with required codecs and a minimal reproducible patch pipeline.
- Launch a CEF-hosted trusted Svelte UI inside the retained Tauri application lifecycle.
- Prove typed invoke/event bridging with remote pages unable to access it.
- Prove CEF subprocess launch, single-instance/profile arguments, shutdown, crash recovery, packaging, and signed update of a trivial app-plus-runtime package.
- **Exit:** no WebView2 dependency in the spike, working trusted UI, isolated untrusted page, reproducible package, and no unresolved architectural blocker.

### Phase 2 — Rendering and extension go/no-go prototypes

- Compare native child windows against off-screen/shared-texture rendering using Section 8 criteria.
- Test CEF runtime/style combinations against required extension APIs and embedded layout.
- Prove native messaging, badges, direct extension-tab creation, popup/options, MV3 service worker, and content-script isolation.
- Measure prototype startup, memory, installed size, and rendering overhead against budgets.
- **Exit:** chosen rendering/runtime style with documented limitations; key extension flows work or have an approved capability-matrix exception; no evidence that stable budgets are impossible.

If this gate fails, revisit CEF patches or Chromium shell architecture before migrating all features. Do not conceal the failure behind a broad “Chrome compatible” claim.

### Phase 3 — Core engine adapter and browser parity

- Port tab lifecycle, navigation, state events, placement/focus/shortcuts, profiles/incognito, permissions, downloads, storage clearing, DevTools, find/print/zoom/fullscreen, crash recovery, and sleep/discard.
- Port application-owned storage unchanged with migration backups.
- Re-run all existing unit, Rust, structure, build, and E2E scenarios against CEF.
- **Exit:** core current E2E suite passes, no privileged IPC in pages, profiles/incognito isolation verified, and stable startup/session recovery.

### Phase 4 — Advanced services and stable feature completion

- Complete extension bridge and compatibility matrix.
- Complete advanced network/cosmetic/worker-aware blocker and Safe Browsing.
- Complete Widevine, WebAuthn/passkeys, PDF, certificate/security UX, and network-privacy audit.
- Restore advanced media, fast downloads, and torrents omitted from early preview.
- Meet performance/package budgets or obtain a documented requirement revision.
- **Exit:** stable feature checklist and security/performance gates pass.

### Phase 5 — Opt-in staged preview

- Offer a separately identifiable preview with explicit fresh-site-storage notice and backups.
- Start with internal/test profiles, then invited users, then broader opt-in preview.
- Publish known extension/API/media gaps and collect feedback without enabling telemetry by default.
- Test upgrade, interrupted migration, forced kill, downgrade/recovery, failed engine update, and multi-profile updates.
- **Exit:** no unresolved data-loss/security critical issues; agreed compatibility and reliability thresholds met; support/rollback documentation ready.

### Phase 6 — Stable migration and legacy retirement

- Show the pre-upgrade notice and preserve old WebView2 data.
- Release signed atomic app-plus-CEF updates through staged rings.
- Monitor signed crash-free/user-feedback metrics available without mandatory telemetry; provide manual diagnostic export.
- Remove WebView2 code/build dependency only after the stable CEF path is available and recovery artifacts are published.
- Retain migration/legacy cleanup code for a defined support window; do not keep a second selectable engine indefinitely.
- **Exit:** supported installation is CEF-only, documentation matches behavior, and legacy data cleanup remains explicit and reversible until confirmed.

## 17. Verification and acceptance criteria

### 17.1 Required repository checks

Implementation work must ultimately pass, from repository root:

- `pnpm check`
- `pnpm test`
- `pnpm check:structure`
- `pnpm --filter @zenith/desktop build`
- `pnpm test:rust`

The CEF build/package/update pipeline and E2E suite must also run in a clean Windows 11 x64 environment. New C++ or binding code needs equivalent automated build, test, warning, and sanitizer/static-analysis policy appropriate to its toolchain.

### 17.2 Functional acceptance

Stable release is accepted only when:

- Every documented current feature works or has an explicitly approved, documented extension/API capability exception; no general feature waiver is allowed.
- Advanced media features omitted from preview are restored.
- Current tested extensions plus required key-gap scenarios pass.
- Navigation, popup/opener, crash, sleep, restore, split, Glance, profiles, incognito, downloads, permissions, and shortcuts preserve current semantics.
- No website or extension content script can invoke trusted Zenith commands.
- Fresh and migrated installations have correct profile isolation and no unexpected network calls.
- Package/startup/memory budgets and comparison tests pass.
- Signed atomic update, interrupted update, previous-version recovery, and multi-profile update tests pass.

### 17.3 New CEF-specific tests

Add coverage for:

- CEF subprocess unexpected exit/hang and browser-process recovery.
- Stale browser/frame callback after tab close or navigation.
- Trusted UI navigation/redirect attempting to retain privileged bridge.
- Popup, iframe, worker, custom-scheme, drag/drop, and extension attempts to cross trust boundaries.
- Incognito context destruction, disk artifacts, extension access, and worker/download cleanup.
- Mixed-DPI/monitor movement, IME, accessibility, GPU reset, protected video, and off-screen/windowed input parity.
- Malformed manifests, CRX packages, filter rules, native-messaging messages, responses, and download metadata.
- Safe Browsing unavailable/stale/corrupt database behavior and certificate/external-protocol denial paths.
- Corrupt runtime files, signature mismatch, partial package, rollback, and incompatible engine/app version.
- Migration interrupted at every persistence step and repeated startup without duplicate/lost records.

## 18. Risks and mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| CEF extension support conflicts with custom embedded views | Required APIs cannot coexist with layout | Early runtime/style spike; minimal patches; capability matrix; reconsider shell before full migration |
| Removing wry also removes trusted UI transport | Large Tauri integration rewrite | Prove trusted CEF bridge and lifecycle in Phase 1 before feature porting |
| Off-screen rendering increases GPU/memory/input complexity | Misses lightweight goal or accessibility | Benchmark native windows first-class; shared textures; reject regressions |
| Custom Chromium patch set becomes a fork | Unsustainable upgrades/security lag | Small patch budget, upstream issues, per-patch ownership/review, automated rebases |
| CEF lags Chromium security fixes | Users exposed after public CVEs | Monitor both projects, emergency 72-hour target after usable fix, disclose support status |
| Bundled runtime breaks size/startup/memory budgets | No longer lightweight | Measure in Phase 2, avoid duplicate engines/services, preserve sleeping, revise only explicitly |
| Codec/Widevine licensing or distribution fails | Media compatibility unavailable | Legal/build review in Phase 0; user-visible limitation if unresolved |
| Fresh site storage surprises users | Lost sign-ins/extension state perceived as data loss | Pre-upgrade acknowledgement, explicit report, preserve app data and legacy folders |
| Extension tab semantics corrupt Zenith state | Spaces/split/session inconsistencies | Stable id mapping and API contract tests; route all mutations through browser domain |
| Request filtering stalls or weakens security | Slow pages/privacy failures | Async worker matching, bounded inputs, defined fail policy, adversarial tests |
| Large atomic updates consume disk/memory | Failed updates and resource regressions | Stream/stage safely, retain one rollback, test interruption and low disk |
| Chromium services create undocumented traffic | Violates privacy promise | Custom build configuration, fresh-profile traffic audit, endpoint inventory |

## 19. Alternatives considered

### Stay on WebView2

Best for tiny installation and delegated engine security maintenance, but rejected as the primary direction because the requested controls span every major area and remain constrained by Microsoft’s API/runtime decisions.

### Firefox/Gecko fork like Zen Browser

Offers a complete browser base and mature extension model, but rejected for this phase because the stakeholder chose to preserve Zenith’s Svelte/Tauri application and current behavior rather than port the product into Firefox’s architecture. Gecko also does not inherently guarantee lower memory or Chromium-extension compatibility.

### Full Chromium fork/shell

Provides maximum control and potentially closer extension behavior, but not selected initially due to build/maintenance burden. It remains a fallback if Phase 2 proves CEF incompatible with mandatory requirements.

### Servo

Attractive Rust/embeddable/lightweight goals, but its own project description says the short-term focus is transitioning from research toward production readiness. It is not a suitable near-term drop-in for Zenith’s required web, DRM, extension, and compatibility surface.

## 20. Research sources

These sources inform constraints; implementation must pin and revalidate versions during Phase 0/1 rather than treating this document as a permanent API reference.

- [Zen Browser desktop repository](https://github.com/zen-browser/desktop): describes Zen as Firefox-based and demonstrates the full-fork alternative.
- [CEF project repository and overview](https://github.com/chromiumembedded/cef): BSD-licensed Chromium embedding, stable APIs, binary distributions, branch model, and explicit instruction that applications distributing CEF keep it updated for Chromium CVEs.
- [CEF issue #1947 — Chrome extension support](https://github.com/chromiumembedded/cef/issues/1947): extension APIs/UI are application integration work; the discussion distinguishes Alloy limitations and the Chrome runtime direction.
- [CEF issue #3685 — Alloy bootstrap removal](https://github.com/chromiumembedded/cef/issues/3685): documents Chrome bootstrap/style combinations, external-parent/off-screen behavior, and Chrome extension API support limited to Chrome-style browsers/windows.
- [Servo project About page](https://servo.org/about/): lightweight embeddable goals and current transition from research toward production readiness.
- Repository baseline: `README.md`, `apps/docs/src/content/docs/contributing/architecture.md`, `extensions.md`, `measurements.md`, `reference/privacy.md`, and `getting-started/install.md`.

## 21. Specification completion checklist

Phase 0 baseline artifacts (2026-11-21, updated 2026-11-22): [WebView2 inventory, feature traceability, baselines and open gates](apps/docs/src/content/docs/contributing/cef-phase-0.md) and [extension API/browser semantics and compatibility manifest](apps/docs/src/content/docs/contributing/cef-extension-contract.md). Phase 0 **conditionally exited for the Phase 1 spike** on 2026-11-22 with v0.1.13 `memory.csv` recorded; codec/Widevine/Safe Browsing legal and security approvals are explicitly deferred to the Phase 4 release gate, and full v0.1.13 E2E/comparator baselines remain open before Phase 3/4 exits.

Phase 1 core spike artifact (2026-11-22): [CEF Phase 1 spike report](apps/docs/src/content/docs/contributing/cef-phase-1.md) and `spikes/cef-phase1`. Trusted UI bridge, untrusted page/iframe isolation, fail-closed navigation, single-instance lock, and crash recovery passed on pinned prebuilt CEF `156.0.31`; NSIS/signed-update packaging and custom codec/patch builds remain deferred.

Phase 4–6 artifacts (2026-11-22): [CEF Phase 4 report](apps/docs/src/content/docs/contributing/cef-phase-4.md) and [CEF Phase 5 & 6 report](apps/docs/src/content/docs/contributing/cef-phase-5-6.md). Opt-in preview feed, pre-upgrade NSIS warning, one-time migration notice, direct NSIS packaging + SHA-256 runtime-manifest verification + single-installer rollback retention, and legacy WebView2/Tauri bundler retirement are implemented.

Before implementation planning is considered complete, attach or link:

- A WebView2 API-to-CEF adapter inventory with feature owners (linked above; role owners assigned, named accountable owners still required).
- The exact extension API/browser semantics matrix and curated compatibility test manifest (linked above; CEF results pending).
- Prototype benchmark results and the chosen rendering/runtime-style rationale.
- Final CEF version/build flags/patch list and licensing review for H.264/AAC, Widevine, Safe Browsing, and redistribution.
- Detailed threat model for trusted UI IPC, extensions, native messaging, custom schemes, downloads, and updater.
- Migration state machine, backup/rollback compatibility table, and legacy-data retention period.
- Network endpoint/data-flow inventory and incognito persistence audit checklist.
- Release-support policy, emergency-update runbook, and runtime SBOM/provenance template.
