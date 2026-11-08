---
title: Privacy
description: What Zenith sends over the network on its own, where your data is stored, and how to clear it.
---

Zenith has no telemetry and no accounts. **Settings › About** says so too: "None. Zenith sends no usage data."

## When Zenith itself goes online

Apart from the pages you visit, Zenith only makes network requests for these:

- **Filter lists** for [ad and tracker blocking](../../guides/ad-blocking/), refreshed weekly from easylist.to (and from any other list you add or turn on).
- **Search suggestions**, if you turn them on in **Settings › General**. They are off by default. When on, what you type in the address bar is sent to your search engine.
- **Chrome Web Store installs**: when you install an extension from the store, Zenith downloads it from Google's update service.
- **Updates**: a minute after startup and then every six hours, Zenith checks GitHub Releases for a newer version. You can turn this off with **Settings › About › Update automatically**.

The new-tab page makes no network requests.

## The video downloader

The [video downloader](../../guides/downloads-and-video/) contacts the site you are on, and only that site. It downloads the media you choose, sending the site's cookies for that address as the page would. If WebView2 can't hand over a streaming playlist the page has just loaded, Zenith fetches that same playlist again to list the stream.

## Where your data lives

Browsing data is stored on your PC in two folders:

- `%LOCALAPPDATA%\dev.zenith.browser`
- `%APPDATA%\dev.zenith.browser`

**Settings › About** shows the profile folder in use. Additional [profiles](../../guides/profiles/) keep their data in a `Profiles\<id>` folder inside each of these.

Saved passwords and payment cards are encrypted with your master password. See [Passwords and payment cards](../../guides/passwords/).

## Clearing data

**Settings › Privacy & blocking › Clear browsing data** (`Ctrl+Shift+Del`) removes browsing history, cookies and site data, cached files, download history or site permissions, for a time range you choose. In the same section you can stop saving history or limit how long it's kept.

For browsing that leaves nothing behind, use an [incognito window](../../guides/incognito/).
