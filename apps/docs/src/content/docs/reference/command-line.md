---
title: Command line
description: Arguments zenith.exe accepts - web addresses, profiles, incognito windows and maintenance flags.
---

You rarely need these: Windows passes links to Zenith when it is your default browser, and the profile and incognito shortcuts Zenith creates use them for you. They are useful for your own shortcuts and scripts.

```powershell
zenith.exe [--profile <id>] [--incognito] [address ...]
```

## Web addresses

Any argument that isn't a flag is treated as something to open:

- `http://` and `https://` addresses open as they are.
- A bare domain such as `example.com` opens as `https://example.com/`.
- The full path of an existing local file, such as `C:\page.html`, opens as a `file:///` address.
- Anything else is ignored, including `file:`, `javascript:` and `data:` addresses passed from outside.

If Zenith is already running with the same profile, the addresses open as new tabs in its window and that window comes to the front. Otherwise Zenith starts and opens them.

## `--profile <id>`

Opens a [profile](../../guides/profiles/). `--profile=<id>` works too. The id is the short name Zenith gave the profile when you created it (for example `work`, or `work-2` if that was taken), not the display name. The desktop shortcut Zenith creates for a profile uses this flag.

- Without the flag, the profile you used last opens. That includes links opened from other apps.
- If no profile with that id exists any more (an old shortcut, for example), the Default profile opens.
- Each profile runs in its own Zenith process, so a second launch with the same id goes to that profile's running window.

## `--incognito`

Opens an [incognito window](../../guides/incognito/). Combine it with addresses to open them in that window; only `http://` and `https://` addresses are used. If Zenith is already running, the incognito window opens in that process next to your normal window.

## Maintenance flags

These run a task and exit without opening a window. They must be the first argument.

| Flag | What it does |
| --- | --- |
| `--register-browser` | Registers Zenith with Windows as a browser, so it appears in **Settings › Apps › Default apps**. The installer runs this. Exits with code 1 if registration fails. |
| `--unregister-browser` | Removes that registration. The uninstaller runs this, and **Settings › Default browser › Unregister** in Zenith does the same. |
| `--purge-incognito <folder>` | Deletes a leftover incognito profile folder from an older version of Zenith (which ran incognito windows on throwaway profiles in `%TEMP%\zenith-incognito`). Only folders directly inside that folder are touched. Retries for up to 30 seconds while files are in use; exits with 0 when the folder is gone, 1 on timeout and 2 if the path isn't such a folder. Current versions sweep these folders at startup on their own. |

## Environment variables

These are for development and testing, not everyday use.

| Variable | Effect |
| --- | --- |
| `ZENITH_PROFILE_DIR` | An absolute path. Zenith keeps all of its data inside that folder instead of `%APPDATA%` and `%LOCALAPPDATA%`, and the instance doesn't hand off to an already running Zenith. The test scripts use this. |
| `ZENITH_PERF` | Writes `perf.json` with the startup time. Used by the [measurement scripts](../../contributing/measurements/). |
| `ZENITH_REMOTE_DEBUG_PORT` | Debug builds only: opens a DevTools remote-debugging port, used by the end-to-end tests. |
| `ZENITH_UPDATE_ENDPOINT` | Debug builds only: reads update information from this address instead of GitHub. See [Releasing](../../contributing/releasing/). |
