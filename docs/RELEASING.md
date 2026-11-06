# Releasing and updates

## How updates reach users

Installed copies of Zenith update themselves:

1. A minute after startup, then every six hours, Zenith reads `latest.json` from the latest GitHub Release (`https://github.com/Drakaniia/Zenith/releases/latest/download/latest.json`).
2. If the release is newer, its installer downloads in the background and its signature is checked against the public key in `src-tauri/tauri.conf.json`. A file that doesn't match is never installed.
3. A **Restart to update** button appears in the sidebar footer. Settings › About also shows the update.
4. The update installs silently when Zenith closes, or right away from **Restart to update**. The NSIS installer runs per-user in quiet mode with no prompts or administrator rights. After **Restart to update** it reopens Zenith, and session restore brings the tabs back.

Users can turn this off in **Settings › About › Update automatically**, and can check by hand with **Check for updates**. Pre-releases (tags like `v0.3.0-beta.1`) are published as GitHub pre-releases and are not offered to installed copies.

## Cutting a release

```powershell
pnpm release patch          # or minor, major, or an explicit version: pnpm release 0.2.0
git push origin HEAD
git push origin v0.2.0      # the tag starts the release build
```

`pnpm release` refuses to run with uncommitted changes. It bumps the version in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` and `src-tauri/tauri.conf.json`, then commits and tags. `--push` pushes both for you; `--dry-run` only shows what would change.

The tag starts `.github/workflows/release.yml` on GitHub Actions (Windows):

1. Checks that the tag matches the version in all four files.
2. Builds the NSIS installer, signs the update, and publishes the GitHub Release with the installer, its `.sig` and `latest.json`.

A separate CI workflow runs frontend checks and tests, builds the frontend, and runs Rust tests on pushes to `main` and pull requests.

A failed build can be re-run from the Actions tab (**Release › Run workflow** with the tag).

## One-time setup

The update signing key was generated with `pnpm tauri signer generate`:

- `C:\Users\Qwenzy\.tauri\zenith-updater.key` is the private key, and `zenith-updater.password` next to it is its password. Keep both secret and back them up. If either is lost, installed copies can never be updated again (they'd need a manual reinstall with a new key).
- `C:\Users\Qwenzy\.tauri\zenith-updater.key.pub` is the public key. It's already in `tauri.conf.json`.

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
