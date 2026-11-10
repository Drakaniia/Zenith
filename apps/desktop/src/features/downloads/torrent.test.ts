import { test } from "node:test";
import assert from "node:assert/strict";
import {
  findMagnet,
  formatEta,
  formatRatio,
  formatSpeed,
  isPlayable,
  isTorrentName,
  selectionSummary,
  torrentStatus,
} from "./torrent.ts";
import { downloadStatus } from "./media.ts";
import type { DownloadItem, TorrentInfo } from "./types.ts";

const info = (over: Partial<TorrentInfo> = {}): TorrentInfo => ({
  stage: "downloading",
  peers: 8,
  seeders: 3,
  downSpeed: 2 * 1024 * 1024,
  upSpeed: 300 * 1024,
  eta: 150,
  uploaded: 0,
  ratio: 0,
  ratioLimit: 1,
  files: 1,
  selected: 1,
  streamable: true,
  ...over,
});

const torrent = (over: Partial<DownloadItem> = {}, t: Partial<TorrentInfo> = {}): DownloadItem => ({
  id: 1,
  url: "magnet:?xt=urn:btih:abc",
  path: "C:\\Downloads\\Show",
  mime: "application/x-bittorrent",
  total: 1024 * 1024 * 100,
  received: 1024 * 1024 * 25,
  state: "progress",
  started: 0,
  kind: "torrent",
  torrent: info(t),
  ...over,
});

const ago = () => "just now";

test("speeds, times and ratios", () => {
  assert.equal(formatSpeed(1536), "1.5 KB/s");
  assert.equal(formatEta(undefined), "");
  assert.equal(formatEta(0), "");
  assert.equal(formatEta(42), "42 s left");
  assert.equal(formatEta(150), "3 min left");
  assert.equal(formatEta(3 * 3600 + 5 * 60), "3 h 5 min left");
  assert.equal(formatEta(7200), "2 h left");
  assert.equal(formatEta(86400), "1 day left");
  assert.equal(formatEta(3 * 86400), "3 days left");
  assert.equal(formatRatio(0.4249), "0.42");
  assert.equal(formatRatio(1), "1");
  assert.equal(formatRatio(Number.NaN), "0");
});

test("status lines follow the torrent's stage", () => {
  assert.equal(torrentStatus(torrent(), ago), "25.0 MB of 100 MB · ↓ 2.0 MB/s · 8 peers · 3 min left");
  assert.equal(torrentStatus(torrent({}, { peers: 0 }), ago), "25.0 MB of 100 MB · Looking for peers…");
  assert.equal(torrentStatus(torrent({}, { peers: 1, eta: undefined }), ago), "25.0 MB of 100 MB · ↓ 2.0 MB/s · 1 peer");
  assert.equal(torrentStatus(torrent({ detail: "Getting torrent info from peers…" }, { stage: "metadata" }), ago), "Getting torrent info from peers…");
  assert.equal(torrentStatus(torrent({}, { stage: "choosing" }), ago), "Choose the files to download");
  assert.equal(torrentStatus(torrent({}, { stage: "paused" }), ago), "Paused · 25.0 MB of 100 MB");
  assert.equal(torrentStatus(torrent({}, { stage: "checking" }), ago), "Checking files… · 25.0 MB of 100 MB");
  const seeding = torrent({ state: "completed" }, { stage: "seeding", ratio: 0.4249, peers: 2 });
  assert.equal(torrentStatus(seeding, ago), "Seeding · ↑ 300 KB/s · ratio 0.42 of 1 · 2 peers");
  assert.equal(torrentStatus(torrent({ state: "completed", received: 1024 * 1024 * 100 }, { stage: "done" }), ago), "100 MB · just now");
  assert.equal(torrentStatus(torrent({ state: "interrupted", reason: "Disk full" }, { stage: "error" }), ago), "Disk full");
  assert.equal(torrentStatus(torrent({ state: "canceled" }), ago), "Canceled");
  // The downloads list uses it for torrents.
  assert.equal(downloadStatus(torrent({}, { stage: "paused" }), ago), "Paused · 25.0 MB of 100 MB");
});

test("magnet links and torrent files", () => {
  assert.equal(findMagnet("  see magnet:?xt=urn:btih:ABC&dn=x  more"), "magnet:?xt=urn:btih:ABC&dn=x");
  assert.equal(findMagnet('<a href="MAGNET:?xt=urn:btih:abc">'), "MAGNET:?xt=urn:btih:abc");
  assert.equal(findMagnet("https://example.com"), null);
  assert.ok(isTorrentName("Show.S01.TORRENT"));
  assert.ok(!isTorrentName("show.mkv"));
  assert.ok(isPlayable("Season 1/Episode 1.mkv"));
  assert.ok(isPlayable("song.FLAC"));
  assert.ok(!isPlayable("readme.nfo"));
});

test("the picker sums the chosen files", () => {
  const files = [
    { index: 0, name: "a.mkv", size: 1024 * 1024, done: 0, selected: true },
    { index: 1, name: "b.mkv", size: 2 * 1024 * 1024, done: 0, selected: true },
    { index: 2, name: "c.nfo", size: 1024, done: 0, selected: true },
  ];
  assert.equal(selectionSummary(files, new Set([0, 1])), "2 of 3 files · 3.0 MB");
  assert.equal(selectionSummary(files.slice(0, 1), new Set()), "0 of 1 file · 0 B");
});
