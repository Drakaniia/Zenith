# Extensions

Zenith runs Chromium extensions through WebView2's browser-extension support (`AreBrowserExtensionsEnabled`). WebView2 implements a subset of the Chrome extension platform, so many extensions work and some do not. This page records what was tested and what is known not to work.

## Installing

Open **Extensions** (menu › Extensions, or the command palette) and use one of:

- **Chrome Web Store**: paste a store link (`https://chromewebstore.google.com/detail/…/<id>`) or the 32-letter extension id. Zenith downloads the `.crx` from Google's update service and installs it.
- **.crx file**: a packaged extension (CRX2 or CRX3).
- **Unpacked folder**: a folder with a `manifest.json` (for development).

Packages are unpacked with Windows' built-in `tar.exe` and copied into `%LOCALAPPDATA%\dev.zenith.browser\extensions`, because WebView2 removes an extension when its source folder changes. Installing an extension that is already installed replaces the old copy; this is also how you update an extension (Zenith does not auto-update extensions in v1).

Each extension can be enabled, disabled or removed. Extensions with an options page get an **Options** button, and toolbar popups open in Glance via **Open popup** (Zenith has no toolbar).

## Tested

Tested on WebView2 156 (Windows 11) with the end-to-end suite (`node scripts/dev/e2e.mjs --only store-extensions,extension-loading`) and by hand.

| Extension                                                               | Version        | Result                                                                                                                                                                                                                                              |
| ----------------------------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dark Reader (Web Store)                                                 | 4.9.134        | Works. Its content script runs in its own isolated world, restyles light pages (pages that are already dark are skipped by Dark Reader's own setting), and its background service worker runs. Its welcome and uninstall pages open as Zenith tabs. |
| uBlock Origin Lite (Web Store)                                          | 2026.1103.1743 | Works. Its service worker runs and its `declarativeNetRequest` rules block ad and analytics requests (verified with Zenith's own blocker switched off for the test site).                                                                           |
| Manifest V3 content-script extension (unpacked, used by the test suite) | 1.0            | Works: `content_scripts` with `<all_urls>` at `document_start`.                                                                                                                                                                                     |

WebView2's built-in components (Microsoft Edge PDF Viewer, Microsoft Clipboard Extension) are listed under "Built into WebView2" and cannot be disabled or removed, so PDF viewing cannot be broken by accident.

## Known limitations

- **No toolbar.** Extension action buttons and badges are not shown. Popups can be opened from the Extensions page (in Glance).
- **Pages an extension opens itself** (`chrome.tabs.create`, `runtime.openOptionsPage`, welcome and uninstall pages) are created by WebView2 outside any Zenith view. Zenith detects them and reopens them as normal tabs (the hidden copy is closed), so they are usable, but the extension cannot keep a reference to the original tab.
- **Extension ids differ from the Web Store ids.** Unpacked extensions get an id derived from their folder. Extensions that rely on a fixed id (for example websites that talk to an extension through `externally_connectable`, or native messaging hosts) may not work.
- **Tab APIs are limited.** WebView2 maps `chrome.tabs` to its own WebView2 instances; extensions that manage the browser's tabs (tab managers, session savers) do not see Zenith's tab model.
- **No automatic updates** for extensions in v1; reinstall to update.
- Which other APIs work is decided by WebView2. Microsoft documents the supported set in the WebView2 browser-extension docs; when an extension fails, check its service worker's console (Developer tools on the extension's page).
