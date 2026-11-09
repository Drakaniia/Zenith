---
title: Spaces and tabs
description: Vertical tabs, Essentials, pinned tabs, folders, spaces and tab sleeping in Zenith.
sidebar:
  order: 1
---

Zenith keeps your tabs in a vertical sidebar. From top to bottom it shows **Essentials**, the current space's **pinned tabs**, and then its regular tabs.

## Tabs

- **New tab**: `Ctrl+T`, or the **New Tab** row in the sidebar. New tabs go to the top of the list unless you turn off **Settings › General › Open new tabs at the top**.
- **Close**: `Ctrl+W`, the × on the tab, or middle-click it. `Ctrl+Shift+T` reopens the last closed tab.
- **Switch**: click, `Ctrl+Tab` / `Ctrl+Shift+Tab`, or `Ctrl+1` to `Ctrl+8` for the first eight tabs and `Ctrl+9` for the last.
- **Reorder**: drag a tab up or down.
- **Rename**: right-click a tab and choose **Rename tab**.

Tabs show their favicon and an audio indicator. Right-click a tab for more: **Duplicate**, **Copy link**, **Open in incognito window**, **Move to folder**, **Move to space**, **Put to sleep**, **Close other tabs** and **Close tabs below**.

The **Clear** button between pinned and regular tabs closes the space's regular tabs. You can hide it in **Settings › Appearance › Clear button**.

To find a tab among many, type `%` followed by a space in the address bar to search only open tabs.

## Essentials and pinned tabs

**Essentials** are an icon grid at the top of the sidebar, shared by all spaces. Drag a tab into the grid to make it an Essential.

**Pinned tabs** belong to one space and remember the page you pinned. If you navigate away, the tab shows a button to go back to the pinned page. Right-click a pinned tab for **Reset to pinned page** or **Replace pinned page with current**.

Pinned tabs and Essentials are always restored when Zenith starts, even if you turned off **Restore tabs from last time**.

## Folders

Group tabs into folders with **New folder** in a tab's or space's right-click menu, or from the **+** menu at the bottom of the sidebar. A collapsed folder still shows its active tab.

## Spaces

A space is a separate set of tabs with its own name, emoji and accent colour, for example Work and Personal. The space switcher sits at the bottom of the sidebar.

- **Create**: the **+** menu at the bottom of the sidebar, or **New space** in the app menu.
- **Switch**: click a space in the switcher, or use `Ctrl+Alt+E` / `Ctrl+Alt+Right` (next) and `Ctrl+Alt+Q` / `Ctrl+Alt+Left` (previous). **Switch to space 1–10** have no default keys; you can set them in **Settings › Keyboard shortcuts**.
- **Reorder**: drag spaces in the switcher.
- **Edit**: right-click a space for **Edit space…**, **Unload all tabs**, **Close unpinned tabs** and **Delete space**. You can't delete the last space.

The accent colour you choose in **Settings › Appearance** applies to the current space.

## Sleeping tabs

To save memory, Zenith puts background tabs to rest in two stages:

1. After a minute in the background, a tab is **suspended**: its scripts pause, but the page stays loaded.
2. After 5 minutes (change this in **Settings › Tabs & memory**), the tab **sleeps**: the page is unloaded. Click it and it reloads at the same scroll position.

Tabs that are visible, in split view or Glance, downloading, or playing audio stay awake. With **Unload tabs when Windows is low on memory** turned on (it is off by default), Zenith puts the least recently used tabs to sleep early when Windows runs low on memory. You can also put a tab to sleep from its right-click menu.

:::caution
A sleeping tab loses anything you typed into it that the page hadn't saved.
:::

## Sidebar layout

Right-click an empty part of the sidebar to move it to the **Left**, **Right**, **Top** or **Bottom** of the window. Top and bottom turn it into a horizontal tab strip. Drag the sidebar's edge to resize it (double-click the edge to reset), and press `Alt+B` to expand or collapse it. More options are in **Settings › Appearance**.
