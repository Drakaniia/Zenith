---
title: Settings
description: What each section of Zenith's Settings page controls, with the default values.
---

Open Settings with `Ctrl+,`, from the menu, or by typing "settings" in the command palette. The search box at the top filters the page as you type. Settings save as soon as you change them.

Passwords, payment cards and the start page have their own settings, described in [Passwords and payment cards](../../guides/passwords/) and [Start page](../../guides/start-page/).

## General

| Setting | What it does | Default |
| --- | --- | --- |
| Restore tabs from last time | Reopens your tabs when Zenith starts. Pinned tabs and Essentials are always kept. | On |
| Open new tabs at the top | Otherwise new tabs go to the bottom of the list. | On |
| Home page | Opens in a window with no tabs, and when you press Enter in an empty new tab. Leave it empty for the Zenith start page. | Empty |
| Search engine | Used for searches from the address bar: Google, Bing, DuckDuckGo, Brave Search, Ecosia or Yandex. | Google |
| Search suggestions | Sends what you type in the address bar to your search engine to get suggestions. | Off |
| Show most visited sites on new tabs | Taken from your local history only. | On |
| Setup assistant | **Run setup again** reopens the [setup assistant](../../getting-started/first-run/). | |

## Appearance

| Setting | What it does | Default |
| --- | --- | --- |
| Theme | Windows (follows the system), Light or Dark. | Windows |
| Accent colour | Tints the window and highlights for the current space. Each space can have its own. | |
| Translucent sidebar | Your desktop shows through the sidebar. Off uses the least GPU and memory. Needs Windows 11. The sidebar is also solid while transparency effects are off in Windows or battery saver is on. | On |
| Sidebar material | Mica (tints with your wallpaper), Mica Alt, or Acrylic (blurs what's behind the window). Shown when the sidebar is translucent. | Mica |
| Website appearance | The colour scheme websites are asked to use: Windows, Light or Dark. | Windows |
| Sidebar position | Left, Right, Top or Bottom. Top and bottom turn the sidebar into a horizontal tab strip. You can also right-click the sidebar to move it. | Left |
| Layout | **Single toolbar** puts the address bar in the sidebar. **Multiple toolbars** adds a toolbar above the page. **Collapsed** is an icon-only sidebar with a toolbar above the page. | Single toolbar |
| Clear button | Shows "Clear" between pinned and regular tabs. | On |
| Compact mode | Hides the sidebar and toolbar; they slide in from the window edges. See [Compact mode](../../guides/compact-mode/). | Off |
| Sidebar width | 163–500 px. You can also drag the sidebar edge, and double-click it to reset. Not shown for a tab strip. | 220 px |

## Privacy & blocking

| Setting | What it does | Default |
| --- | --- | --- |
| Block ads and trackers | Network-level blocking in every tab. Shows the number of rules and of requests blocked this session. | On |
| Filter lists | The built-in Zenith list (always on), EasyList, EasyPrivacy and Peter Lowe's list, plus any list you add by URL. **Update now** downloads them again. | EasyList and EasyPrivacy on |
| Blocking turned off on | Sites where you switched blocking off. Remove a site to turn blocking back on there. | |
| Block pop-ups | Blocks windows a page opens without you clicking anything. | On |
| Save browsing history | | On |
| Keep history for | 1 week, 1 month, 3 months, 1 year or Forever. | 3 months |

**Clear browsing data** removes any of: browsing history, cookies and site data (this signs you out), cached images and files, download history, and site permissions, for the last hour, 24 hours, 7 days, 4 weeks or all time. `Ctrl+Shift+Del` opens this section.

See [Ad and tracker blocking](../../guides/ad-blocking/) and [Privacy](../privacy/).

## Tabs & memory

| Setting | What it does | Default |
| --- | --- | --- |
| Put background tabs to sleep after | Never, 1, 2, 5, 10, 15 or 30 minutes, or 1 hour. Sleeping tabs free their memory and reload when you open them. Tabs playing audio, downloading or in split view stay awake. | 5 minutes |
| Suspend background tabs after a minute | Pauses page scripts without unloading the page. | On |
| Sleep tabs when switching spaces | | Off |
| Unload tabs when Windows is low on memory | | On |

Unloading a tab discards anything you typed into it that the page hasn't saved. **Memory in use** shows the private memory of all Zenith processes and how much of the system's memory is in use.

## Bookmarks

| Setting | What it does | Default |
| --- | --- | --- |
| Bookmarks toolbar | Always, On a new tab, or Never. With the single toolbar layout, a hidden toolbar still appears while the pointer is at the top edge of the window. | On a new tab |
| Show Other Bookmarks | At the end of the toolbar, when something is in it. | On |
| Bookmarks sidebar | Opens or closes the bookmarks sidebar. | Closed |

Under **Manage** you can open the bookmarks library, import from another browser or an HTML file, export to HTML, and back up or restore everything. See [Bookmarks](../../guides/bookmarks/).

## Downloads

| Setting | What it does | Default |
| --- | --- | --- |
| Save files to | The download folder. **Reset** goes back to your Windows Downloads folder. | Downloads |
| Show a download button on videos | Appears when you point at a video. Applies to pages loaded after you change it. | On |
| Use FFmpeg when it is installed | Lets FFmpeg convert `.ts` recordings to `.mp4` and merge audio and video Zenith can't merge itself. | On |
| FFmpeg location | Found automatically on your `PATH`, or choose `ffmpeg.exe`. | Automatic |

See [Downloads and video](../../guides/downloads-and-video/).

## Notes

| Setting | What it does | Default |
| --- | --- | --- |
| Notes shortcut | Shows the shortcut that opens your notes (`Ctrl+Alt+N`). **Change** jumps to Keyboard shortcuts. | |
| Use the shortcut in every app | Brings Zenith to the front with your notes even while another app is in front. While Zenith runs, other apps don't receive that key combination. If another app already owns it, Zenith tells you and the shortcut only works inside Zenith. | On |

## Keyboard shortcuts

Every action and its keys, with a recorder and conflict detection. See [Keyboard shortcuts](../keyboard-shortcuts/).

## Default browser

**Make default** opens the Windows Default apps page, where you confirm the choice (Windows doesn't let apps change it silently). Once Zenith is the default, links from other apps open as new tabs. **Unregister** removes Zenith from the Default apps list.

## About

Shows the Zenith version, the WebView2 runtime version and your profile folder.

| Setting | What it does | Default |
| --- | --- | --- |
| Check for updates / Restart to update | Checks now, or installs a downloaded update and restarts with your tabs restored. | |
| Update automatically | Downloads new versions in the background and installs them silently when you close Zenith. | On |
