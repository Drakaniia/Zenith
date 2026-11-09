import { test } from "node:test";
import assert from "node:assert/strict";
import { fileNameOf, latestFinished, RECENT_MS } from "./recent.ts";
import { extOf, fileType } from "./file-type.ts";
import type { DownloadItem } from "./types.ts";

const dl = (over: Partial<DownloadItem> = {}): DownloadItem => ({
  id: 1,
  url: "https://example.com/a.pdf",
  path: "C:\\Users\\me\\Downloads\\a.pdf",
  mime: "application/pdf",
  total: 10,
  received: 10,
  state: "completed",
  started: 0,
  ended: 1_000,
  ...over,
});

test("the latest finished file is the one that ended last, not started last", () => {
  const items = [dl({ id: 1, started: 900, ended: 1_000 }), dl({ id: 2, started: 100, ended: 2_000 })];
  assert.equal(latestFinished(items, 2_000)?.item.id, 2);
});

test("only completed downloads with a file count", () => {
  const items = [
    dl({ id: 1, state: "progress", ended: null }),
    dl({ id: 2, state: "interrupted", ended: 5_000 }),
    dl({ id: 3, state: "canceled", ended: 6_000 }),
    dl({ id: 4, path: "", ended: 7_000 }),
  ];
  assert.equal(latestFinished(items, 7_000), null);
});

test("a finished file stays recent for the window, then goes", () => {
  const items = [dl({ ended: 1_000 })];
  const r = latestFinished(items, 1_000);
  assert.equal(r?.until, 1_000 + RECENT_MS);
  assert.ok(latestFinished(items, 1_000 + RECENT_MS - 1));
  assert.equal(latestFinished(items, 1_000 + RECENT_MS), null);
});

test("an older recent file is not shown once a newer one has expired", () => {
  const items = [dl({ id: 1, ended: 1_000 }), dl({ id: 2, ended: 2_000 })];
  assert.equal(latestFinished(items, 2_000 + RECENT_MS), null);
});

test("file names fall back to the path", () => {
  assert.equal(fileNameOf(dl({ fileName: "b.zip" })), "b.zip");
  assert.equal(fileNameOf(dl()), "a.pdf");
});

test("extensions come from the last part of a path", () => {
  assert.equal(extOf("C:\\dir.v2\\report.PDF"), "pdf");
  assert.equal(extOf("C:\\dir.v2\\README"), "");
  assert.equal(extOf(".bashrc"), "");
  assert.equal(extOf("archive.tar.gz"), "gz");
});

test("file types by extension", () => {
  const kind = (n: string) => fileType(n).kind;
  assert.equal(kind("a.pdf"), "pdf");
  assert.equal(kind("a.docx"), "word");
  assert.equal(kind("a.xlsx"), "sheet");
  assert.equal(kind("a.csv"), "sheet");
  assert.equal(kind("a.pptx"), "slides");
  assert.equal(kind("a.jpeg"), "image");
  assert.equal(kind("a.mp4"), "video");
  assert.equal(kind("a.flac"), "audio");
  assert.equal(kind("a.zip"), "archive");
  assert.equal(kind("a.7z"), "archive");
  assert.equal(kind("setup.exe"), "installer");
  assert.equal(kind("a.json"), "code");
  assert.equal(kind("a.txt"), "text");
  assert.equal(kind("a.xyz"), "other");
});

test("file types fall back to the MIME type, and carry their look", () => {
  assert.equal(fileType("download", "application/pdf").kind, "pdf");
  assert.equal(fileType("doc", "application/vnd.openxmlformats-officedocument.wordprocessingml.document").kind, "word");
  assert.equal(fileType("pic", "image/png").kind, "image");
  assert.equal(fileType("x", "application/octet-stream").kind, "other");
  const pdf = fileType("Report.pdf");
  assert.deepEqual(pdf, { kind: "pdf", icon: "fileText", color: "#e5484d", label: "PDF" });
  assert.equal(fileType("data.jsonl").label, "JSON");
  assert.equal(fileType("plain").color, null);
});
