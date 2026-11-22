---
title: CEF Phase 5 & 6 completion report
description: Opt-in preview update channel, migration notice, direct NSIS packaging with runtime-manifest verification and rollback retention, and legacy WebView2/Tauri retirement.
---

Status: Phase 5 & 6 implementation completed 2026-11-22 against pinned CEF `156.0.31+ge603d97+chromium-156.0.8078.100`.

## Delivered

1. **Opt-in preview update channel (`updates/mod.rs`, `host/config.rs`, `AboutSection.svelte`)**:
   - Added `previewUpdates` setting (default `false`) in **Settings › About**.
   - When enabled, the updater checks both `preview.json` (on the rolling `preview` GitHub release) and `latest.json`, selecting whichever signed release has the highest semver above the running version so preview users automatically graduate to stable.
2. **Pre-upgrade NSIS warning & one-time in-app migration notice (`installer.nsi`, `storage/migration.rs`, `MigrationNotice.svelte`)**:
   - Interactive NSIS upgrades detect legacy `WebView2/` profiles without `CEF/v1/.migrated-v1` and prompt for acknowledgement before installing.
   - `storage/migration.rs` records `fromLegacy` and `noticeSeen` in `CEF/v1/.migrated-v1`; upgraded profiles see a one-time `MigrationNotice` overlay explaining preserved/backed-up data vs fresh site storage and untouched `WebView2/`.
3. **Direct CEF NSIS packaging, startup manifest check & rollback retention (`installer.nsi`, `package-nsis.ps1`, `release.yml`, `app/security.rs`, `updates/mod.rs`)**:
   - Replaced `tauri-apps/tauri-action@v0` in `.github/workflows/release.yml` with direct NSIS packaging (`package-nsis.ps1`), minisign signing (`TAURI_SIGNING_PRIVATE_KEY`), `runtime-manifest.json` generation, and ≤400 MB unpacked / ≤150 MB installer budget checks.
   - Startup `verify_runtime_manifest` validates every file listed in `runtime-manifest.json` via SHA-256 and fails closed before CEF init on tampering or missing files.
   - `updates::finalize_pending_install` verifies post-upgrade boot and rotates the previous verified installer into `%LOCALAPPDATA%\dev.zenith.browser\update-rollback\` (retaining at most one rollback installer) while handling interrupted upgrades safely.
4. **Legacy WebView2 / Tauri retirement**:
   - Removed `tauri.conf.json`, `capabilities/`, `gen/`, `windows/hooks.nsh`, `@tauri-apps/cli`, `@tauri-apps/plugin-dialog`, and stale WebView2 dev probes (`audio-probe.mjs`, `zoom-probe.mjs`, `focus-probe.ps1`, `build-release.ps1`).
   - Updated `scripts/release.mjs`, `scripts/release.test.ts`, `package.json`, and user/contributor docs.
   - Retained `@tauri-apps/api` in the Svelte frontend via Zenith's authenticated CEF bridge shim (`app/bridge.rs`).
