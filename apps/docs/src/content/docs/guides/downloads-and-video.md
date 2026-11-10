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

Zenith can save the videos and audio a page plays: plain files as well as HLS and DASH streams. It notices media as the page loads it.

### Start a download

- Point at a video on the page and click the download button that appears over it.
- Click the download button in the address pill.
- Choose the video download item in the page menu.
- Right-click a video.

The **Download video on this page** action has no default shortcut; you can set one in **Settings › Keyboard shortcuts**.

### Choose what to save

A panel lists what the page has played. For streams you can pick the **quality** and the **audio language**. Live HLS streams are recorded from close to the live edge until you stop the recording or the stream ends; the recording is kept either way.

Media downloads appear in the regular downloads list. Finished files are named after the page title and quality. A failed download waits in the list so you can retry it.

### FFmpeg

Zenith merges separate video and audio tracks into one `.mp4` on its own when both are fragmented MP4. If [FFmpeg](https://ffmpeg.org/) is installed, Zenith also uses it to:

- convert MPEG-TS (`.ts`) recordings to `.mp4`, and
- merge audio and video it can't merge itself (such as WebM).

Without FFmpeg, `.ts` streams stay `.ts`, and WebM audio and video that need merging are saved as two files. Zenith finds FFmpeg on your `PATH`, or you can choose `ffmpeg.exe` in **Settings › Downloads › FFmpeg location**. Turn **Use FFmpeg when it is installed** off to never use it.

### Settings

**Settings › Downloads › Show a download button on videos** controls the button over videos. It applies to pages loaded after you change it.

## Limitations

- DRM-protected video (Netflix, Disney+ and similar, or anything using encrypted streams) is never downloaded.
- Sites that use their own streaming protocols, YouTube for example, can't be saved.
- Live DASH streams can't be recorded.
- Video downloads don't survive a restart of Zenith. Torrents do.
- Torrents saved into a folder that already holds files with the same names continue from those files.
- Torrents started in an incognito window are kept out of the download history, but they continue after a restart like any torrent until you remove them.

The downloader only contacts the site you're on, sending that site's cookies as the page would. See [Privacy](../../reference/privacy/).
