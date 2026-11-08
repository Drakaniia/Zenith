---
title: Ad and tracker blocking
description: How Zenith's built-in blocker works, how to turn it off for a site, and how to add filter lists.
---

Zenith blocks ads and trackers in every tab, with no extension needed. It's on by default.

## How it works

Zenith checks every request a page makes against filter lists. A request to a blocked domain never reaches the network. Requests to the site you're visiting are never blocked.

Blocking works on domains. Zenith doesn't hide page elements with cosmetic filters, so you may see empty spaces where ads would have been.

On six news and media sites, blocking cut the requests that reached the network by about 75 % (see [Measurements](/contributing/measurements/)).

## Per-site control

The shield in the address pill shows how many requests were blocked on the current page. Click it to turn blocking off for that site, for example if the site doesn't work properly. Click again to turn it back on.

Sites where you turned blocking off are listed in **Settings › Privacy & blocking › Blocking turned off on**. Remove a site there to turn blocking back on.

## Filter lists

**Settings › Privacy & blocking** lists the filter lists:

| List | Default |
| --- | --- |
| Zenith built-in list | Always on |
| EasyList (ads) | On |
| EasyPrivacy (trackers) | On |
| Peter Lowe's ad and tracking servers | Off |

Zenith downloads the lists from their sites and refreshes them weekly. **Update now** fetches them again right away. Each list shows how many rules it contributes, or an error if it couldn't be loaded.

To add your own list, paste its URL (hosts-file or Adblock Plus format, starting with `https://`) into **Add a list URL** and choose **Add**. Custom lists can be removed with the trash button.

Turning **Block ads and trackers** off disables blocking everywhere. The same section shows the total number of rules and how many requests were blocked this session.

## Pop-ups

**Block pop-ups** (on by default) blocks windows that pages try to open without you clicking anything. When Zenith blocks one, it shows a message that lets you open it anyway. Links you click that open in a new window still open, as a tab next to the page.
