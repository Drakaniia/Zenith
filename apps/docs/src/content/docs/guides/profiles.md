---
title: Profiles
description: Keep separate sets of history, bookmarks, passwords, cookies, extensions and settings, and run them side by side.
---

A profile is a completely separate Zenith: its own settings, tabs and spaces, history, bookmarks, saved passwords and cards, cookies and site data, extensions, filter lists and start-page backgrounds. Use profiles to keep work and personal browsing apart, or to stay signed in to two accounts of the same site.

The data Zenith had before you created any profiles is the **Default** profile.

## Create a profile

1. Click the profile button in the sidebar.
2. Choose **New profile…**.
3. Enter a name (for example Work, Personal or School), pick a colour, and choose a picture: the profile's initial, an emoji, or your own image.
4. Leave **Add a desktop shortcut** ticked if you want a shortcut that opens Zenith straight into this profile.

## Switch profiles

Click the profile button and pick a profile from the list. Each profile runs in its own Zenith window and process, so you can have several open at once. Their windows group separately on the taskbar, and the window title shows the profile name, for example "Zenith — Work".

When you start Zenith without choosing a profile, the one you used last opens. Links from other apps also open there.

You can also start a profile from the command line: `zenith.exe --profile <id>`. See [Command line](../../reference/command-line/).

## Edit or delete a profile

Click the profile button and choose **Manage profiles…**, then pick a profile to rename it or change its colour and picture. **Desktop shortcut › Create** (or **Recreate** after a rename) makes its desktop shortcut.

**Delete profile** removes the profile and everything in it from your PC: history, bookmarks, saved passwords, cookies, extensions and settings. This can't be undone. You can't delete the Default profile, or a profile that is currently open.

## Where profile data lives

The Default profile uses `%APPDATA%\dev.zenith.browser` and `%LOCALAPPDATA%\dev.zenith.browser`. Other profiles live in a `Profiles\<id>` folder inside each of these.
