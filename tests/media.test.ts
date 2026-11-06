import { test } from "node:test";
import assert from "node:assert/strict";
import {
  downloadStatus,
  formatDuration,
  kindLabel,
  mediaMeta,
  mediaTitle,
} from "../src/lib/media.ts";
import type { DownloadItem, MediaItem } from "../src/lib/types.ts";

const item = (over: Partial<MediaItem> = {}): MediaItem => ({
  id: 1,
  kind: "file",
  url: "https://www.cdn.example.com/media/My%20Clip.mp4?sig=1",
  title: "",
  ext: "mp4",
  audioOnly: false,
  live: false,
  protected: false,
  foundBy: "network",
  detected: 0,
  variants: [],
  audioTracks: [],
  note: "",
  ...over,
});

test("durations read like a player clock", () => {
  assert.equal(formatDuration(187), "3:07");
  assert.equal(formatDuration(3725), "1:02:05");
  assert.equal(formatDuration(0), "");
  assert.equal(formatDuration(Number.NaN), "");
});

test("kinds are described plainly", () => {
  assert.equal(kindLabel(item()), "MP4 video");
  assert.equal(kindLabel(item({ ext: "mp3", audioOnly: true })), "MP3 audio");
  assert.equal(kindLabel(item({ kind: "hls" })), "Video stream (HLS)");
  assert.equal(
    kindLabel(item({ kind: "dash", audioOnly: true })),
    "Audio stream (DASH)",
  );
  assert.equal(kindLabel(item({ kind: "hls", live: true })), "Live stream");
});

test("item facts and titles", () => {
  const meta = mediaMeta(
    item({ height: 720, duration: 95, size: 5 * 1024 * 1024 }),
  );
  assert.equal(meta, "MP4 video · 720p · 1:35 · 5.0 MB · cdn.example.com");
  assert.equal(mediaTitle(item()), "My Clip.mp4");
  assert.equal(
    mediaTitle(item({ title: "  Launch trailer " })),
    "Launch trailer",
  );
  assert.equal(
    mediaTitle(item({ url: "https://a.example/", kind: "hls" })),
    "Video stream (HLS)",
  );
});

test("download rows describe media jobs", () => {
  const base: DownloadItem = {
    id: 1,
    url: "",
    path: "",
    mime: "",
    total: 0,
    received: 0,
    state: "progress",
    started: 0,
  };
  const ago = () => "just now";
  assert.equal(
    downloadStatus(
      {
        ...base,
        received: 1024 * 1024,
        total: 4 * 1024 * 1024,
        kind: "media",
        detail: "Segment 3 of 12",
      },
      ago,
    ),
    "1.0 MB of 4.0 MB · Segment 3 of 12",
  );
  assert.equal(
    downloadStatus(
      { ...base, received: 2048, live: true, detail: "Recording · 0:42" },
      ago,
    ),
    "Recording · 0:42 · 2.0 KB",
  );
  assert.equal(
    downloadStatus({ ...base, received: 10, total: 100, paused: true }, ago),
    "Paused · 10 B of 100 B",
  );
  assert.equal(
    downloadStatus({ ...base, state: "completed", total: 1024 }, ago),
    "1.0 KB · just now",
  );
  assert.equal(
    downloadStatus(
      {
        ...base,
        state: "completed",
        total: 1024,
        detail: "Video and audio were saved as separate files",
      },
      ago,
    ),
    "1.0 KB · Video and audio were saved as separate files",
  );
  assert.equal(
    downloadStatus(
      { ...base, state: "interrupted", reason: "Network error" },
      ago,
    ),
    "Network error",
  );
});
