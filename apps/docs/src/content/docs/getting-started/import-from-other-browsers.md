---
title: Import from other browsers
description: Bring bookmarks and history from Chrome, Edge, Firefox and other browsers, or from an exported bookmarks file.
sidebar:
  order: 3
---

Zenith can copy your bookmarks and history from another browser installed for your Windows account. The other browser is never changed, and it can stay open while you import.

## Supported browsers

Zenith looks for these in their standard profile folders:

- Google Chrome, Microsoft Edge, Brave, Vivaldi, Yandex Browser and Chromium
- Opera and Opera GX
- Firefox and Zen

If a browser has more than one profile, each profile is listed separately.

## Import

You can import during the [setup assistant](../first-run/), or at any time from **Settings › Bookmarks › Manage › Import › From a browser…**.

1. Choose the browser (and profile) to import from.
2. Choose what to bring:
   - **Bookmarks** go into a folder named after the browser, on the bookmarks toolbar.
   - **History** covers the period set in **Settings › Privacy & blocking › Keep history for** (3 months by default). It's unavailable if you've turned off **Save browsing history**.
   - **Addresses** (Chromium-based browsers only) go into Zenith's [password manager](../../guides/passwords/#addresses), which needs a master password set up first.
   - **Open tabs** (Chromium-based browsers only) from the browser's last session open in a new space named after it, at most 100. They load when you first visit them.
   - **Start page tiles** (Chromium-based browsers, including Yandex Browser's Tableau) are added after your own sites on the start page.
3. Choose **Import**. Zenith tells you how many new bookmarks and history entries it added.

You can import from several browsers, one after another.

## From a bookmarks file

Most browsers can export bookmarks as an HTML file. To import one, use **Choose an .html file…** in the setup assistant, or **Settings › Bookmarks › Manage › Import › From a file…**.

To restore a Zenith bookmarks backup instead (which also keeps tags, keywords and workspaces), see [Bookmarks](../../guides/bookmarks/).

## What isn't imported

Cookies aren't copied from other browsers. Saved passwords and payment cards of Chrome, Edge, Brave, Vivaldi, Chromium, Opera and Opera GX can be imported from the password manager's **Import from a browser…**; for Yandex Browser, Firefox and Zen, export passwords as a CSV file and import that. See [Passwords and payment cards](../../guides/passwords/).
