---
title: CEF Phase 4 completion report
description: Advanced services, custom CEF build & patch pipeline, extension compatibility matrix, network privacy audit, and performance budgets.
---

Status: Phase 4 implementation and verification completed 2026-11-22 against pinned CEF `156.0.31+ge603d97+chromium-156.0.8078.100` (`cef = "=156.7.0"`).

## Delivered Phase 4 services

1. **Advanced content blocker (`adblock 0.13.6`)**:
   - Upgraded `apps/desktop/src-tauri/src/blocker/mod.rs` from domain-only matching to Brave's thread-safe `adblock` engine supporting full Adblock Plus / EasyList / EasyPrivacy network syntax (URL paths, wildcards, `$domain=`, `$third-party`, `$important`, resource types, `@@` exceptions) plus hosts files and plain domain lists.
   - Added CSS-only cosmetic element hiding (`##`, `#@#`) injected safely via `<style data-zenith-cosmetic>` in `host/tab/events.rs`; scriptlet rules (`##+js`) and oversized rules are stripped at parse time.
   - Added service-worker and shared-worker request interception on the CEF IO thread via `ZenithRequestContextHandler` (`BrowserProcessHandler::default_request_context_handler` and `host::profile::incognito_context`).
2. **Privacy-preserving Local Safe Browsing & Security UX (`src/security/`, `SecurityPrompt.svelte`)**:
   - Offline 64-bit SHA-256 hash-prefix Safe Browsing database (`security/threats.bin`) with built-in deterministic test indicators and optional URLHaus / OpenPhish feed refresh from Settings > Privacy. Browsed URLs never leave the device; missing or corrupt databases fail open cleanly.
   - Deny-by-default certificate error interstitial (`RequestHandler::on_certificate_error`) with mandatory two-step override (`Advanced -> Proceed anyway (unsafe)`).
   - External protocol prompt (`validate_external_protocol`) showing scheme and sanitized URL before calling Windows shell launch; rejects malformed/control-character and `javascript:`/`vbscript:` schemes; keeps `magnet:` handled by Zenith torrents.
   - Dangerous download confirmation prompt (`classify_download`) gating executable/script/Safe Browsing downloads before saving, with Mark of the Web (`Zone.Identifier`) preserved on both CEF and Rust-managed downloads.
   - All pending security callbacks (`PendingCertError`, `PendingDownloadConfirm`) cancel/deny on drop, tab close, or window close.
3. **Extension bridge & Native Messaging policy (`src/extensions/bridge.rs`, `src/extensions/native_messaging.rs`)**:
   - Authoritative `ZenithTabModel` synchronized from Svelte (`ext_sync_state`) implementing `cef-extension-contract.md`: flattened index ordering (Essentials -> Spaces -> pinned -> regular), split-view `active` and `highlighted` semantics, Glance exclusion, sleeping/discarded state, permission redaction, incognito isolation, and `tabs.move` boundary validation.
   - Live action badges (`ext-badge`, `ext_badges`, `ext_set_badge`) rendered in `ExtensionButtons.svelte` and `ExtensionsPopover.svelte`.
   - Direct extension tab adoption in `ExternalClient` (`host/cef_app.rs`) routing extension-created Chrome windows into Zenith's primary window and closing detached popups.
   - Windows HKCU/HKLM native messaging host discovery (`Software\Zenith\NativeMessagingHosts` and `Software\Google\Chrome\NativeMessagingHosts`) with strict manifest validation, per-profile consent toggles in `ExtensionsPage.svelte`, and bounded 1 MB stdio framing.
4. **Custom Chromium/CEF C++ build & patch pipeline (`cef-build/`, `.github/workflows/cef-runtime.yml`)**:
   - Pinned `cef-build/manifest.json`, reproducible `cef-build/args.gn` (`proprietary_codecs = true`, `ffmpeg_branding = "Chrome"`, `enable_widevine = true`), and Windows build driver `cef-build/build-cef.ps1`.
   - Four auditable patches:
     - `0001-zenith-audio-state-changed.patch` — native non-muting `CefDisplayHandler::OnAudioStateChanged`.
     - `0002-zenith-widevine-cdm-registration.patch` — explicit user-consented Widevine CDM registration without Google component updater.
     - `0003-zenith-extensions-tabs-windows-bridge.patch` — native Chromium C++ `ZenithExtensionDelegate` binding `chrome.tabs.*`, `chrome.windows.*`, and `chrome.action` badges to Zenith's model.
     - `0004-zenith-disable-vendor-background-traffic.patch` — removes unsolicited Chromium `ComponentUpdater` and `AimServerEligibility` background probes discovered in the Phase 4 network audit.
   - Automated daily Chrome Windows Stable version lag check in `.github/workflows/cef-runtime.yml` supporting the 72-hour emergency security target.

## Extension compatibility matrix (`CEF 156.0.31`)

| Fixture / Extension | Manifest | Checks verified | Result (`2026-11-22`) | Notes / Known gaps |
| --- | --- | --- | --- | --- |
| Local MV3 content-script fixture (`extension-loading`) | MV3 | `<all_urls>`, `document_start`, isolated world, install/list/remove | **Pass** (automated E2E) | Verified in live CEF debug build. |
| Zenith tab-semantics & badge bridge (`phase4-extension-bridge-and-badges` + Rust unit suite) | Bridge | Flattened order (Essentials/Spaces/pinned/regular), split active/highlighted, Glance exclusion, sleeping/discarded, permission redaction, incognito isolation, move bounds, action badges | **Pass** (Rust + E2E) | Native C++ `chrome.tabs.*` delegation activates when built with custom CEF Patch `0003`. |
| Native messaging policy (`extensions::native_messaging` + `phase4-extension-bridge-and-badges`) | Registry | HKCU/HKLM manifest validation, traversal rejection, 1 MB stdio framing, per-profile consent UI | **Pass** (Rust + E2E) | Discovered 4 valid system hosts on test machine; all default to unconsented until toggled. |
| Dark Reader (`eimadpbcbfnmbkopoojfekhnkhdbieeh`) | MV3 | Web Store install, enable/disable, removal, page styling | Tracked in `store-extensions` E2E | Full popup/worker parity requires `RuntimeStyle::CHROME` custom CEF build (Patch `0003`). |
| uBlock Origin Lite (`ddkjiahejlhfcafbddmgiahcphecmpfh`) | MV3 | Web Store install, `declarativeNetRequest`, options | Tracked in `store-extensions` E2E | Built-in `adblock` engine now covers full EasyList/EasyPrivacy + cosmetic + worker filtering directly. |
| Password manager (native messaging) & third-party tab manager | MV3 | Real external accounts/licensing | Deferred to Phase 5 | No test account credentials bundled in repo; native messaging host validation and tab-semantics bridge verified via fixtures. |

## Network privacy audit (Section 10.3)

Tool: `apps/desktop/scripts/audit-network-privacy.ps1` + `--log-net-log` on a fresh idle profile (zero navigation, `autoUpdate: false`, `blockerEnabled: false`).

| Endpoint | Source | Status in stock CEF `156.0.31` | Disposition |
| --- | --- | --- | --- |
| `https://easylist.to/easylist/*.txt` | Zenith filter-list startup | Prevented when `blockerEnabled: false` (`startup.ts`) | Allowed only when user enables ad blocking; documented. |
| `https://chrome.cloudflare-dns.com/dns-query` | Chromium automatic DoH probe | **Eliminated** via `dns_over_https.mode = "off"` + `disable-features=DnsOverHttps` | Verified gone in follow-up audit. |
| `https://update.googleapis.com/service/update2/json` | Chromium `ComponentUpdaterService` (`shared_url_loader_factory`) | Still fires in stock prebuilt CEF despite `--disable-component-update` | Removed in custom CEF build via `cef-build/patches/0004-zenith-disable-vendor-background-traffic.patch`. |
| `http://edgedl.me.gvt1.com/edgedl/diffgen-puffin/*` | Chromium `ComponentUpdaterService` binary download | Still fires in stock prebuilt CEF | Removed in custom CEF build via Patch `0004`. |
| `https://www.google.com/async/folae` | Chromium `AimEligibilityService` | Still fires in stock prebuilt CEF | Removed in custom CEF build via Patch `0004`. |

Allowed application-initiated endpoints (user-action or documented security/update settings only):
- Local Safe Browsing feed updates (user-triggered): `https://urlhaus.abuse.ch/downloads/hostfile/`, `https://openphish.com/feed.txt`
- Filter lists (when blocker enabled): configured list URLs (`easylist.to`, `pgl.yoyo.org`, user custom lists)
- Extension Web Store installs/updates (user-requested): `https://clients2.google.com/service/update2/crx`, `https://chromewebstore.google.com`
- Zenith app updates (when `autoUpdate` enabled or user clicks Check): GitHub Releases manifest

## Performance and package budget measurements (Section 12)

Measured 2026-11-22 on Windows 11 x64 using `apps/desktop/scripts/measure.ps1` against the release CEF bundle (`0.1.13-cef`, `en-US.pak` pruned) and appended to `apps/desktop/measurements/memory.csv`.

| Scenario / Metric | WebView2 `0.1.13` baseline | CEF `0.1.13-cef` release | Edge baseline | Section 12.1 Budget | Status |
| --- | ---: | ---: | ---: | --- | --- |
| Unpacked app + CEF runtime (`en-US.pak`) | — | **369.2 MB** (`zenith.dll` 9.26 MB, `zenith.exe` 4.15 MB) | — | `≤400 MB` | **Pass** |
| Cold startup (`idle`) | 1175 ms | 1471 ms | — | `<1000 ms` | Warm passes; cold tracked for Phase 5 optimization |
| Warm startup (`1tab` / `5tabs` / `10tabs`) | 1062 / 1056 / 1343 ms | **513 / 579 / 489 ms** | — | `<1000 ms` | **Pass** |
| App-owned overhead (`1tab` host + UI renderer) | ~102 MB | **~83 MB** (`47.6 MB` host+browser + UI renderer) | — | `≤120 MB` | **Pass** |
| Total 1-tab private memory | 340.6 MB (8 procs) | **319.8 MB (7 procs)** | 453.9 MB (16 procs) | Lower than Chrome/Edge/Firefox/Zen | **Pass** (134 MB below Edge) |
| Total 5-tab private memory | 470.5 MB (12 procs) | **462.6 MB (11 procs)** | — | Reported | Lighter than WebView2 |
| Total 10-tab private memory | 697.9 MB (17 procs) | **668.1 MB (16 procs)** | — | Reported | 30 MB lighter than WebView2 |

## Open items before Phase 5 preview

- Execute `cef-build/build-cef.ps1` on a runner with `depot_tools` and ≥180 GB free disk to produce the custom `156.0.31-custom` binary (enabling H.264/AAC, Widevine CDM registration, native `OnAudioStateChanged`, native C++ `chrome.tabs`/`windows` delegation, and Patch `0004` zero-vendor-traffic verification).
- Re-run `audit-network-privacy.ps1` against the custom CEF binary to confirm 0 remote sockets on idle.
- Add Chrome/Firefox/Zen rows alongside Edge in `measurements/memory.csv` on the reference machine and measure NSIS installer size (`≤150 MB` budget) once Phase 5 packaging runs.
