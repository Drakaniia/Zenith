---
title: Install Zenith
description: System requirements, installing Zenith on Windows, automatic updates and uninstalling.
sidebar:
  order: 1
---

## Requirements

- Windows 11, or Windows 10 22H2. On Windows 10 the sidebar is solid: the translucent Mica and Acrylic materials need Windows 11.
- The Microsoft Edge WebView2 runtime. It ships with Windows 11. If it's missing, the installer downloads it for you.

Zenith runs on 64-bit Windows only. There are no macOS or Linux versions.

## Install

1. Download `Zenith_<version>_x64-setup.exe` from the [latest GitHub release](https://github.com/Drakaniia/Zenith/releases/latest).
2. Run it. Zenith installs for your Windows account only, so you don't need administrator rights.

The installer is under 2 MB because it doesn't bundle a browser engine: Zenith uses the WebView2 runtime that Windows already keeps up to date.

:::note
The installer isn't code-signed yet, so Windows SmartScreen may show "Windows protected your PC". Choose **More info › Run anyway**. Updates aren't affected; Zenith verifies each update with its own signing key.
:::

When Zenith opens for the first time, the [setup assistant](/getting-started/first-run/) walks you through a few choices.

## Make Zenith your default browser

The installer registers Zenith with Windows as a browser. To use it for links from other apps:

1. In Zenith, open **Settings › Default browser** and choose **Make default**. The setup assistant offers the same button on its last step.
2. Windows opens **Settings › Apps › Default apps**. Choose Zenith there. Windows doesn't let apps change the default browser on their own.

Links you open from other apps then appear as new tabs in the Zenith window that's already open.

## Updates

Zenith updates itself. A minute after it starts, and then every six hours, it checks for a new version. A new version downloads in the background and its signature is checked before anything is installed.

When an update is ready, a **Restart to update** button appears at the bottom of the sidebar. Either click it, or keep browsing: the update installs silently the next time you close Zenith. Your tabs come back after the restart.

To check by hand, or to turn automatic updates off, go to **Settings › About**.

## Uninstall

Uninstall Zenith from **Windows Settings › Apps › Installed apps**. The uninstaller also removes Zenith from the Default apps list.

Your browsing data stays in `%APPDATA%\dev.zenith.browser` and `%LOCALAPPDATA%\dev.zenith.browser`. Delete those folders if you want it gone too.
