---
title: Downloads and video
description: Manage downloads, download torrents and magnet links, and save videos and audio that pages play, including HLS and DASH streams.
---

## Downloads

Files download to your Windows **Downloads** folder. Choose a different folder in **Settings › Downloads › Save files to**.

The downloads button in the toolbar shows active and recent downloads. `Ctrl+Shift+Y` opens the full **Downloads** page, which is also in the app menu. Download history can be cleared in **Settings › Privacy & blocking › Clear browsing data**.

For ten minutes after a file finishes downloading, the downloads button shows that file's type (a red sheet for a PDF, blue for a Word document, an archive for a ZIP, and so on) with its extension. Drag the button onto a page, such as a Google Classroom assignment or a Messenger chat, to attach the file as if you had picked it from your Downloads folder. Clicking the button still opens the downloads list.

A tab that is downloading never goes to sleep. Incognito windows don't record downloads.

## Torrents and magnet links

Zenith downloads torrents itself; you don't need a separate torrent app.

### Start a torrent

- Click a magnet link on a page.
- Download a `.torrent` file from a page. It starts as soon as the file is saved.
- On the **Downloads** page, click **Add torrent** and paste a magnet link, or choose **Open .torrent file…**.
- Drop `.torrent` files or magnet links onto the **Downloads** page.
- Open magnet links and `.torrent` files from other apps. Choose Zenith for **magnet** and **.torrent** in Windows **Settings › Apps › Default apps** (the installer registers Zenith for both).

Torrents appear in the regular downloads list. A magnet link first gets the torrent's file list from other peers, which can take a moment.

### Choose files

When a torrent has several files, Zenith asks which ones to download before it starts. Click **Choose files** in the list (or **Choose** in the notification), tick the files you want, and click **Download**. Click the **Files** button on a downloading torrent to change the choice later. Turn **Settings › Downloads › Ask which files to download** off to download every file straight away.

### Watch while it downloads

Video and audio files can play before the torrent finishes. Open **Files** and click the play button next to a file: it opens in a new tab, and Zenith fetches the parts the player needs first. Each file downloads roughly from start to end, so it plays smoothly from the beginning.

### Progress, pausing and seeding

A torrent's row shows its download speed, the number of peers and the time left. Pause and resume it like any download. Paused and unfinished torrents continue when you restart Zenith, and data already on disk is checked rather than downloaded again.

When a torrent finishes, it keeps **seeding** (sharing with others) until it has uploaded as much as its size, then stops on its own. The row shows the upload speed and the ratio reached. Click **Stop seeding** to stop sooner; the files stay. **Settings › Downloads › Seed after downloading** sets how much to share, or turns seeding off.

**Cancel** on a torrent that is still downloading deletes what it downloaded so far.

### Torrent settings

In **Settings › Downloads › Torrents** you can turn torrent support off (magnet links then open in another app, as before), and set download and upload speed limits for all torrents together.

Zenith finds peers through trackers, the DHT network, peer exchange and local discovery, and opens its listening port on your router (UPnP) so others can connect. Other peers see your IP address, as with any torrent app. Only download what you have the right to share.

## The video downloader

Zenith can save videos and audio that pages play (progressive files, HLS and DASH streams) as well as links you paste directly into **Downloads**.

### Start a download

- Point at a video on the page and click the floating **Download** button to open the inline quality and audio picker right over the video.
- Click the download button in the address pill, choose the video download item in the page menu, or right-click a video.
- On the **Downloads** page (or in the downloads popover), click **Download video** and paste a direct stream link or a video page URL.

The **Download video on this page** action has no default shortcut; you can set one in **Settings › Keyboard shortcuts**.

### Choose what to save

The picker lists what the page has played (or what a pasted link resolves to). For streams you can pick the **quality**, the **audio language**, and the output format (**Video**, **Audio · M4A**, **Audio · MP3**, or **Audio · Opus**). Live HLS streams are recorded from close to the live edge until you stop the recording or the stream ends; the recording is kept either way.

Media downloads appear in the regular downloads list. Finished files are named after the page title and quality. A failed native download waits in the list so you can retry it.

### FFmpeg and yt-dlp

Zenith runs its native Rust extractor and fMP4 merger first, with no external tools required. In **Settings › Downloads › Video downloader** you can install or point Zenith at two optional tools with one click:

- **[FFmpeg](https://ffmpeg.org/)**: adds MP3/Opus/M4A audio extraction from videos, `.ts` to `.mp4` conversion, and merging of audio and video tracks Zenith can't merge itself (such as WebM).
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: acts as a fallback when a page doesn't expose a direct stream (such as YouTube, TikTok, X, or Twitch) and powers pasted URLs from those sites.

### Settings

**Settings › Downloads › Show a download button on videos** toggles the floating button globally. You can also hide it on individual sites from the **Hide on …** button in the video dropdown, and re-enable hidden sites in Settings.

## Limitations

- DRM-protected video (Netflix, Disney+ and similar, or anything using encrypted streams) is never downloaded.
- Sites that use proprietary streaming protocols require optional **yt-dlp** to be installed in Settings.
- Live DASH streams can't be recorded.
- Video downloads don't survive a restart of Zenith. Torrents do.
- Torrents saved into a folder that already holds files with the same names continue from those files.
- Torrents started in an incognito window are kept out of the download history, but they continue after a restart like any torrent until you remove them.

The downloader only contacts the site you're on, sending that site's cookies as the page would. See [Privacy](../../reference/privacy/).
