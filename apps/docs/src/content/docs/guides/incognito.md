---
title: Incognito windows
description: Browse without saving history, cookies or downloads to your profile.
---

An incognito window doesn't keep a record of what you do in it. Its title bar says "Zenith Incognito".

## Open one

- Press `Ctrl+Shift+P`.
- Choose **New incognito window** in the app menu or the **+** menu at the bottom of the sidebar.
- Right-click a tab or a bookmark and choose **Open in incognito window**, or right-click a link on a page and choose **Open link in incognito window**.
- Start Zenith with `zenith.exe --incognito`. See [Command line](../../reference/command-line/).

Incognito windows open quickly because they run in the Zenith process that's already running.

## What incognito keeps and what it doesn't

Pages in incognito windows run in WebView2's InPrivate mode:

- Cookies, cache and site storage stay in memory. All open incognito windows share them, and they're discarded when the last incognito tab closes.
- History and downloads aren't recorded, and the session (open tabs) isn't saved.
- The window starts with your settings, but changes you make in it last only as long as the window.
- Zenith never offers to save passwords or cards in an incognito window.

Your bookmarks and ad-blocking filter lists are shared with your normal window.

Some things aren't available in incognito: installing extensions, importing data, restoring bookmark backups, installing updates, and running the setup assistant.

## Closing

Close an incognito window like any other. If you close your normal window while incognito windows are open, they stay open; starting Zenith again brings the normal window back.

:::note
Incognito hides your browsing from other people using this PC. It doesn't hide it from the sites you visit, your employer or your internet provider.
:::
