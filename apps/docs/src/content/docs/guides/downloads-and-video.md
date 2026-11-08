---
title: Downloads and video
description: Manage downloads and save videos and audio that pages play, including HLS and DASH streams.
---

## Downloads

Files download to your Windows **Downloads** folder. Choose a different folder in **Settings › Downloads › Save files to**.

The downloads button in the toolbar shows active and recent downloads. `Ctrl+Shift+Y` opens the full **Downloads** page, which is also in the app menu. Download history can be cleared in **Settings › Privacy & blocking › Clear browsing data**.

A tab that is downloading never goes to sleep. Incognito windows don't record downloads.

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
- Video downloads don't survive a restart of Zenith.

The downloader only contacts the site you're on, sending that site's cookies as the page would. See [Privacy](/reference/privacy/).
