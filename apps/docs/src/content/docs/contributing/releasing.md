---
title: Releasing and updates
description: How installed copies update themselves, how to cut a release, and the one-time signing setup.
---

## How updates reach users

Installed copies of Zenith update themselves:

1. A minute after startup, then every six hours, Zenith reads `latest.json` from the latest GitHub Release (`https://github.com/Drakaniia/Zenith/releases/latest/download/latest.json`). If **Settings › About › Preview updates** is enabled, it also checks `preview.json` from the rolling `preview` release and picks the newest version.
2. If the release is newer, its installer streams to disk and its minisign signature is verified against Zenith's pinned public key in `apps/desktop/src-tauri/src/updates/mod.rs`. A file that doesn't match is never installed.
3. A **Restart to update** button appears in the sidebar footer. Settings › About also shows the update.
4. The update installs silently when Zenith closes, or right away from **Restart to update**. The NSIS installer runs per-user in quiet mode, replaces the CEF runtime atomically, and on next boot Zenith verifies `runtime-manifest.json` and retains the previous verified installer in `%LOCALAPPDATA%\dev.zenith.browser\update-rollback\` for rollback.

Users can turn automatic updates off or opt in to pre-release preview builds in **Settings › About**. Pre-release tags (such as `v0.2.0-preview.1`) are published as GitHub pre-releases and update the rolling `preview` release's `preview.json` without affecting stable users.

## Cutting a release

```powershell
pnpm release patch          # or minor, major, or an explicit version: pnpm release 0.2.0-preview.1
git push origin HEAD
git push origin v0.2.0      # the tag starts the release build
```

`pnpm release` refuses to run with uncommitted changes. It bumps the version in `apps/desktop/package.json`, `apps/desktop/src-tauri/Cargo.toml`, and `apps/desktop/src-tauri/Cargo.lock`, then commits and tags. `--push` pushes both for you; `--dry-run` only shows what would change.

The tag starts `.github/workflows/release.yml` on GitHub Actions (Windows):

1. Checks that the tag matches the version in all three files.
2. Exports the pinned CEF runtime, builds the release bundle + `runtime-manifest.json`, packages the NSIS installer (`package-nsis.ps1`, checking the ≤400 MB unpacked and ≤150 MB installer budgets), signs it with minisign, and publishes the GitHub Release with the installer, `.sig`, `runtime-manifest.json`, and `latest.json` (or `preview.json` for pre-releases).

A separate CI workflow runs frontend checks and tests, builds the frontend, and runs Rust tests on pushes to `main` and pull requests.

A failed build can be re-run from the Actions tab (**Release › Run workflow** with the tag).

## One-time setup

The update signing key is a minisign key pair:

- Private key and password are stored as `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` GitHub repository secrets. Keep both secret and back them up: if lost, installed copies can never auto-update again.
- The corresponding base64-encoded public key is pinned in `apps/desktop/src-tauri/src/updates/mod.rs` (`PUBKEY_B64`).

Add both to the repository as secrets once:

```powershell
Get-Content -Raw $env:USERPROFILE\.tauri\zenith-updater.key | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo Drakaniia/Zenith
Get-Content -Raw $env:USERPROFILE\.tauri\zenith-updater.password | gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo Drakaniia/Zenith
```

The key needs a password: with an empty one, Tauri waits for a password prompt and the build hangs.

## Code signing (not set up yet)

The installer is not Authenticode-signed, so a manually downloaded installer gets a SmartScreen warning ("Windows protected your PC" › More info › Run anyway). Automatic updates are unaffected: they're verified with the updater key, not Authenticode.

To sign later, add a certificate as secrets and set `bundle.windows.certificateThumbprint` (with the certificate imported in the workflow), or use `bundle.windows.signCommand` with Azure Trusted Signing.

## Testing an update locally

A debug build reads `ZENITH_UPDATE_ENDPOINT` instead of the GitHub feed, so a local `latest.json` can be served. Debug builds allow plain-HTTP feeds. Test instances (`ZENITH_PROFILE_DIR`) don't check for updates unless that variable is set.
