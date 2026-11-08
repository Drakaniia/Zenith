---
title: Extension support
description: How Zenith runs Chromium extensions through WebView2, which extensions were tested, and what is known not to work.
---

This page is for contributors. For installing and managing extensions as a user, see [Extensions](../../guides/extensions/). Paths are relative to `apps/desktop`.

Zenith runs Chromium extensions through WebView2's browser-extension support (`AreBrowserExtensionsEnabled`). WebView2 implements a subset of the Chrome extension platform, so many extensions work and some do not. This page records what was tested and what is known not to work.

## Installing

Open **Extensions** (menu › Extensions, Ctrl+Shift+A, the command palette, or **Manage** in the toolbar's extensions menu) and use one of:

- **Chrome Web Store**: paste a store link (`https://chromewebstore.google.com/detail/…/<id>`) or the 32-letter extension id. Zenith downloads the `.crx` from Google's update service and installs it.
- **.crx file**: a packaged extension (CRX2 or CRX3).
- **Unpacked folder**: a folder with a `manifest.json` (for development).

Packages are unpacked with Windows' built-in `tar.exe` and copied into `%LOCALAPPDATA%\dev.zenith.browser\extensions`, because WebView2 removes an extension when its source folder changes. Installing an extension that is already installed replaces the old copy. Packages keep their Chrome id: the developer key from the `.crx` header is written into the unpacked manifest (`key`), as Chrome derives the id, so extensions that rely on their own id keep working and the Web Store can be asked for updates.

Extensions found in a page (a `.crx` download, or the Web Store's "Add to Chrome" button) are not installed straight away: they are staged under `extensions\pending` and offered with their permissions, then installed or discarded (`src-tauri/src/extensions/install.rs`).

Each extension can be enabled, disabled, removed or pinned to the toolbar. Extensions with an options page get an **Options** button, and **Open popup** opens an extension's popup in Glance.

## Toolbar

The **Extensions** button (puzzle piece, `src/features/extensions/ExtensionButtons.svelte`) sits in the top bar, the tab strip and the sidebar's top row, depending on the layout. It opens the extensions menu (`ExtensionsPopover.svelte`, like Chrome's puzzle menu): clicking an extension opens its popup as a small card anchored where the menu was, or its options page when it has no popup. Pinned extensions (`settings.pinnedExtensions`) show their icon next to the button and open their popup under that icon; the sidebar's top row shows at most two of them.

## Updates

Extensions installed from the Chrome Web Store are checked for updates (`src-tauri/src/extensions/updates.rs`, commands `ext_check_updates` and `ext_update`): the installed ids and versions are sent to Google's update service, and newer versions are listed on the Extensions page with **Update to …** and **Update all**. Zenith checks at startup at most once a day (setting `extensionUpdates`, on by default; never in incognito windows) and shows a toast when updates are found, or on demand with **Check for updates**. Updates install only when the user clicks: `ext_update` downloads the store's current version and reinstalls it, keeping the id, so pins stay. Extensions installed from a `.crx` file or an unpacked folder are not checked; install them again to update.

## Tested

Tested on WebView2 156 (Windows 11) with the end-to-end suite (`node scripts/dev/e2e.mjs --only store-extensions,extension-loading`) and by hand.

| Extension                                                               | Version        | Result                                                                                                                                                                                                                                              |
| ----------------------------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dark Reader (Web Store)                                                 | 4.9.134        | Works. Its content script runs in its own isolated world, restyles light pages (pages that are already dark are skipped by Dark Reader's own setting), and its background service worker runs. Its welcome and uninstall pages open as Zenith tabs. |
| uBlock Origin Lite (Web Store)                                          | 2026.1103.1743 | Works. Its service worker runs and its `declarativeNetRequest` rules block ad and analytics requests (verified with Zenith's own blocker switched off for the test site).                                                                           |
| Manifest V3 content-script extension (unpacked, used by the test suite) | 1.0            | Works: `content_scripts` with `<all_urls>` at `document_start`.                                                                                                                                                                                     |

WebView2's built-in components (Microsoft Edge PDF Viewer, Microsoft Clipboard Extension) are listed under "Built into WebView2" and cannot be disabled or removed, so PDF viewing cannot be broken by accident.

## Known limitations

- **No badges.** Pinned extensions show their icon, but action badges are not shown.
- **Pages an extension opens itself** (`chrome.tabs.create`, `runtime.openOptionsPage`, welcome and uninstall pages) are created by WebView2 outside any Zenith view. Zenith detects them and reopens them as normal tabs (the hidden copy is closed), so they are usable, but the extension cannot keep a reference to the original tab.
- **Unpacked extensions get their own id.** Web Store and `.crx` installs keep their Chrome id, but an unpacked extension gets an id derived from its folder. Extensions that rely on a fixed id (for example websites that talk to an extension through `externally_connectable`, or native messaging hosts) may not work when loaded unpacked.
- **Tab APIs are limited.** WebView2 maps `chrome.tabs` to its own WebView2 instances; extensions that manage the browser's tabs (tab managers, session savers) do not see Zenith's tab model.
- **Updates need a click,** and only Web Store extensions are checked; reinstall `.crx` and unpacked extensions to update them.
- Which other APIs work is decided by WebView2. Microsoft documents the supported set in the WebView2 browser-extension docs; when an extension fails, check its service worker's console (Developer tools on the extension's page).
