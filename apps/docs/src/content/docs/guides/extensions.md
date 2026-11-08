---
title: Extensions
description: Install Chrome extensions from the Chrome Web Store or from files, and manage them in Zenith.
---

Zenith runs Chrome extensions through the WebView2 engine. WebView2 supports a subset of what Chrome does, so many extensions work and some don't. Dark Reader and uBlock Origin Lite, for example, were tested and work.

## Install an extension

Open the **Extensions** page with `Ctrl+Shift+A`, from the app menu, or with **Manage** in the extensions menu. Then use one of:

- **Chrome Web Store**: paste a store link (`https://chromewebstore.google.com/detail/…`) or the 32-letter extension ID. Zenith downloads the extension from Google and installs it.
- **A `.crx` file**: a packaged extension.
- **An unpacked folder**: a folder with a `manifest.json`, for extension development.

**Extension catalogue** in the extensions menu opens the Chrome Web Store so you can find extensions.

Installing isn't available in an [incognito window](/guides/incognito/).

## Use extensions

The **Extensions** button (puzzle piece) in the toolbar lists your extensions. Click one to open its popup, or its options page if it has no popup. Pin an extension to show its icon next to the button.

Pages an extension opens on its own, such as a welcome page, open as normal tabs.

## Manage extensions

On the Extensions page you can enable, disable or remove each extension, and open its **Options** if it has them.

Zenith checks the Chrome Web Store for updates to store extensions when it starts. Turn that off with **Check the Chrome Web Store for extension updates when Zenith starts**, check by hand with **Check for updates**, and install them with **Update all** or the button on each extension.

**Built into WebView2** lists the components that come with the engine, such as the PDF viewer. They can't be disabled or removed.

## Known limitations

- Extension IDs differ from the Web Store's for unpacked extensions. Websites or native apps that talk to an extension by its fixed ID may not work with it.
- Extensions that manage tabs (tab managers, session savers) don't see Zenith's tabs, spaces or folders.
- Which other extension APIs work is decided by WebView2. If an extension misbehaves, open developer tools on its page and check its console.

Technical details and test results are in [Extension support](/contributing/extensions/).
