// Scenarios: downloads (plain downloads, the media downloader, the exempt list).
import { mkdirSync, existsSync, readdirSync, rmSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import {
  session,
  evalIn,
  ui,
  invoke,
  waitFor,
  waitUi,
  page,
  click,
  clickSelector,
  typeInPalette,
  openUrl,
  activeTabId,
  tabSel,
  pageOf,
  downloadDir,
  settleStartupSettings,
} from "../harness.mjs";

export const scenarios = [];
export const exempt = [];
let list = scenarios;
const test = (name, fn, limitMs) => list.push({ name, fn, limitMs });

test("download", async () => {
  rmSync(downloadDir, { recursive: true, force: true });
  mkdirSync(downloadDir, { recursive: true });
  await settleStartupSettings();
  await invoke("apply_settings", {
    config: {
      downloadDir,
      recordHistory: true,
      blockPopups: true,
      blockerEnabled: true,
    },
    appearance: {
      theme: "system",
      material: "mica",
      pageBackground: [32, 32, 36],
      contentScheme: "system",
    },
  });
  const t = await pageOf(await waitFor(activeTabId, "active tab"));
  await evalIn(
    t,
    `(() => { const a = document.createElement('a'); a.id = 'zd'; a.href = 'data:text/plain,zenith-download-test'; a.download = 'zenith-test.txt'; a.textContent = 'download'; a.style.cssText = 'position:fixed;left:10px;top:60px;font-size:30px'; document.body.append(a); })()`,
  );
  await clickSelector(t, "#zd");
  await waitFor(
    () => existsSync(join(downloadDir, "zenith-test.txt")),
    "file on disk",
    15000,
  );
  const active = await waitFor(
    async () => {
      const list = await invoke("downloads_history", { limit: 5 });
      return list.find((d) => d.state === "completed") ?? null;
    },
    "download record",
    10000,
  );
  return `saved ${readdirSync(downloadDir).join(", ")} (${active.state})`;
});

test("media-downloader", async () => {
  // A local site: a page with a <video> (a progressive file) whose player also loads an HLS stream.
  const segments = [0, 1, 2].map((i) =>
    Buffer.alloc(150_000 + i * 1000, 0x30 + i),
  );
  const routes = {
    "/watch": [
      "text/html",
      `<!doctype html><title>Zenith media test</title><body style="margin:40px"><video id="v" src="/clip.mp4" muted preload="auto" style="width:480px;height:270px;background:#000"></video><script>fetch('/hls/master.m3u8').then((r) => r.text()).then(() => fetch('/hls/v.m3u8'));</script>`,
    ],
    "/clip.mp4": ["video/mp4", Buffer.alloc(300_000, 7)],
    "/hls/master.m3u8": [
      "application/vnd.apple.mpegurl",
      "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=900000,RESOLUTION=640x360\nv.m3u8\n",
    ],
    "/hls/v.m3u8": [
      "application/vnd.apple.mpegurl",
      `#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXT-X-PLAYLIST-TYPE:VOD\n${segments.map((_, i) => `#EXTINF:4,\ns${i}.ts\n`).join("")}#EXT-X-ENDLIST\n`,
    ],
    ...Object.fromEntries(
      segments.map((b, i) => [`/hls/s${i}.ts`, ["video/mp2t", b]]),
    ),
  };
  const server = createServer((req, res) => {
    const route = routes[req.url.split("?")[0]];
    if (!route) return res.writeHead(404).end();
    const body = Buffer.isBuffer(route[1]) ? route[1] : Buffer.from(route[1]);
    const m = /bytes=(\d+)-(\d*)/.exec(req.headers.range ?? "");
    if (m) {
      const a = Number(m[1]);
      const b = m[2]
        ? Math.min(Number(m[2]), body.length - 1)
        : body.length - 1;
      res.writeHead(206, {
        "Content-Type": route[0],
        "Content-Range": `bytes ${a}-${b}/${body.length}`,
        "Content-Length": b - a + 1,
        "Accept-Ranges": "bytes",
      });
      return res.end(body.subarray(a, b + 1));
    }
    res.writeHead(200, {
      "Content-Type": route[0],
      "Content-Length": body.length,
      "Accept-Ranges": "bytes",
    });
    res.end(body);
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const base = `http://127.0.0.1:${server.address().port}`;
  let saved = "";
  try {
    rmSync(downloadDir, { recursive: true, force: true });
    mkdirSync(downloadDir, { recursive: true });
    await settleStartupSettings();
    await invoke("apply_settings", {
      config: {
        downloadDir,
        recordHistory: true,
        blockPopups: true,
        blockerEnabled: true,
        mediaButton: true,
        useFfmpeg: false,
      },
      appearance: {
        theme: "system",
        material: "mica",
        pageBackground: [32, 32, 36],
        contentScheme: "system",
      },
    });
    await openUrl(`${base}/watch`);
    const t = await page(`${base}/watch`);
    const id = await waitFor(activeTabId, "media tab id");
    // Found from network traffic alone: the <video> file and the stream (its variant folded in).
    const listed = await waitFor(
      async () => {
        const r = await invoke("media_list", { tabId: id, scan: true });
        return r.items
          .map((i) => i.kind)
          .sort()
          .join(",") === "file,hls"
          ? r
          : null;
      },
      "video file and HLS stream detected",
      15000,
    );
    const stream = listed.items.find((i) => i.kind === "hls");
    if (stream.duration !== 12)
      throw new Error(
        `stream duration ${stream.duration} (media playlist not folded into the master)`,
      );
    await waitUi(
      `!!document.querySelector('.pill .media')`,
      "download button in the address pill",
    );
    // The button over the video (trusted pointer input) opens the panel for that video.
    const v = await evalIn(
      t,
      `(() => { const b = document.getElementById('v').getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
    );
    await (
      await session(t)
    ).send("Input.dispatchMouseEvent", { type: "mouseMoved", x: v.x, y: v.y });
    const host = await waitFor(
      () =>
        evalIn(
          t,
          `(() => { const h = document.querySelector('zenith-media-button'); if (!h || getComputedStyle(h).display === 'none') return null; const b = h.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`,
        ),
      "download button over the video",
    );
    await click(t, host.x, host.y);
    await waitUi(
      `!!document.querySelector('.panel .item.hinted')`,
      "media panel with the clicked video highlighted",
      10000,
    );
    // Download the stream from the panel; segments must arrive complete and in order.
    await ui(
      `[...document.querySelectorAll('.panel .item')].find((i) => i.innerText.includes('HLS')).querySelector('.go').click()`,
    );
    const expected = Buffer.concat(segments);
    const record = await waitFor(
      async () =>
        (await invoke("downloads_history", { limit: 10 })).find(
          (d) => d.url === `${base}/hls/master.m3u8` && d.state !== "progress",
        ),
      "stream download finished",
      20000,
    );
    saved = record.path;
    if (record.state !== "completed")
      throw new Error(`stream download ${record.state}`);
    if (!saved.startsWith(downloadDir))
      throw new Error(`saved outside the configured folder: ${saved}`);
    if (!saved.endsWith(".ts"))
      throw new Error(`unexpected container: ${saved}`);
    if (!readFileSync(saved).equals(expected))
      throw new Error("the saved stream differs from its segments");
    await ui(
      `document.querySelector('.panel header .icon-btn:last-child').click()`,
    );
    return `listed ${listed.items.map((i) => i.kind).join(" + ")}; in-page button opened the panel; saved "${saved.split(/[\\/]/).pop()}" (${expected.length} bytes, exact)`;
  } finally {
    server.close();
    // Never leave test output behind (e.g. in the user's Downloads folder after a settings race).
    if (saved && existsSync(saved)) rmSync(saved, { force: true });
  }
});

// Run at a different point of the suite (see index.mjs).
list = exempt;

test("download-exempt", async () => {
  // A control tab that should go to sleep, and a tab with a (paused) download that must not.
  await openUrl("https://example.org/?sleep-control");
  await page("sleep-control");
  const controlId = await waitFor(activeTabId, "control tab id");
  await openUrl("https://example.com/?download-exempt");
  const t = await page("download-exempt");
  const id = await waitFor(activeTabId, "download tab id");
  await evalIn(
    t,
    `(() => { const a = document.createElement('a'); a.id = 'zbig'; a.href = 'https://proof.ovh.net/files/100Mb.dat'; a.textContent = 'big file'; a.style.cssText = 'position:fixed;left:10px;top:110px;font-size:30px'; document.body.append(a); })()`,
  );
  await clickSelector(t, "#zbig");
  const dl = await waitFor(
    async () =>
      (await invoke("downloads_active")).find(
        (d) => d.state === "progress" && d.tabId === id,
      ),
    "download started",
    20000,
  );
  await invoke("download_action", { id: dl.id, action: "pause" });
  await ui(`document.querySelector('.new-tab').click()`);
  await waitUi(`!!document.querySelector('.palette input')`, "palette");
  await typeInPalette("");
  const start = Date.now();
  await waitUi(
    `!!document.querySelector(${JSON.stringify(tabSel(controlId, ".sleeping"))})`,
    "control tab asleep",
    150000,
  );
  const slept = Math.round((Date.now() - start) / 1000);
  const downloadTabAsleep = await ui(
    `!!document.querySelector(${JSON.stringify(tabSel(id, ".sleeping"))})`,
  );
  await invoke("download_action", { id: dl.id, action: "cancel" });
  if (downloadTabAsleep)
    throw new Error("the tab with a download in progress was put to sleep");
  return `control tab slept after ~${slept}s; the tab with a download in progress stayed awake (download then canceled)`;
}, 200000);
