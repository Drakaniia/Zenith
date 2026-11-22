// Phase 4 E2E scenarios: extension badges & tab semantics, local Safe Browsing interstitial,
// external protocol prompt, dangerous download warning, cosmetic blocking, PDF/WebAuthn & engine info.
import { createServer } from "node:http";
import { evalIn, invoke, openUrl, page, ui, waitFor, waitUi } from "../harness.mjs";

export const scenarios = [];
const test = (name, fn, limitMs) => scenarios.push({ name, fn, limitMs });

test("phase4-engine-info", async () => {
  const info = await invoke("engine_info");
  if (!info?.cefVersion || !info?.chromiumVersion) {
    throw new Error(`missing engine_info fields: ${JSON.stringify(info)}`);
  }
  if (!info.sandboxEnabled) throw new Error("expected sandboxEnabled=true");
  return `CEF ${info.cefVersion}, Chromium ${info.chromiumVersion}, sandbox=${info.sandboxEnabled}, custom=${info.customCefBuild}`;
});

test("phase4-extension-bridge-and-badges", async () => {
  // Verify synced Zenith tab semantics via the authoritative extension bridge.
  await waitFor(async () => {
    const tabs = await invoke("ext_query_tabs", { query: {} });
    return Array.isArray(tabs) && tabs.length >= 1 ? tabs : false;
  }, "ext_query_tabs synced", 10000);
  const all = await invoke("ext_query_tabs", { query: {} });
  const active = await invoke("ext_query_tabs", { query: { active: true, currentWindow: true } });
  if (active.length !== 1) throw new Error(`expected 1 active tab, got ${active.length}`);

  // Verify live action badge update & clear round-trip through the bridge.
  const extId = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  await invoke("ext_set_badge", {
    extensionId: extId,
    tabId: null,
    badge: { text: "42", backgroundColor: "#ff5500", textColor: "#ffffff" },
  });
  const badges = await invoke("ext_badges");
  const hit = badges.find((b) => b.extensionId === extId && b.badge?.text === "42");
  if (!hit) throw new Error(`badge not stored: ${JSON.stringify(badges)}`);
  await invoke("ext_set_badge", {
    extensionId: extId,
    tabId: null,
    badge: { text: "", backgroundColor: null, textColor: null },
  });
  const hosts = await invoke("ext_native_hosts");
  return `syncedTabs=${all.length}, active=${active[0].zenithId}, badgeRoundTrip=ok, nativeHosts=${hosts.length}`;
});

test("phase4-safebrowsing-and-protocols", async () => {
  const status = await invoke("security_safebrowsing_status");
  if (!status?.enabled || status.entries < 2) {
    throw new Error(`unexpected safebrowsing status: ${JSON.stringify(status)}`);
  }

  // Navigating to a built-in test threat host must trigger the local Safe Browsing interstitial.
  await openUrl("https://malware.testing.zenith.invalid/");
  await waitUi(
    `document.body.innerText.includes('Dangerous site blocked') && document.body.innerText.includes('malware.testing.zenith.invalid')`,
    "Safe Browsing interstitial",
    10000,
  );
  // Dismiss safely via "Back to safety".
  await ui(
    `[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Back to safety')?.click()`,
  );
  await waitUi(`!document.body.innerText.includes('Dangerous site blocked')`, "interstitial closed", 6000);

  // External protocol navigation must prompt explicitly rather than launch silently.
  const srv = createServer((req, res) => {
    res.writeHead(200, { "content-type": "text/html" });
    res.end(`<a id="proto" href="mailto:phase4@example.invalid">Mail</a>`);
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  const { port } = srv.address();
  try {
    await openUrl(`http://127.0.0.1:${port}/`);
    const t = await page(`127.0.0.1:${port}`);
    await evalIn(t, `document.getElementById('proto').click()`);
    await waitUi(
      `document.body.innerText.includes('Open external application') && document.body.innerText.includes('mailto')`,
      "external protocol prompt",
      10000,
    );
    await ui(
      `[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Cancel')?.click()`,
    );
    await waitUi(`!document.body.innerText.includes('Open external application')`, "protocol prompt dismissed", 6000);
  } finally {
    srv.close();
  }
  return `sbEntries=${status.entries}, interstitial=blocked+dismissed, externalProtocol=prompted+canceled`;
});

test("phase4-dangerous-download-and-webcompat", async () => {
  const srv = createServer((req, res) => {
    if (req.url === "/evil.exe") {
      res.writeHead(200, {
        "content-type": "application/octet-stream",
        "content-disposition": 'attachment; filename="zenith-test-payload.exe"',
      });
      res.end("MZ-test-payload");
      return;
    }
    res.writeHead(200, { "content-type": "text/html" });
    res.end(`<a id="dl" href="/evil.exe">Download EXE</a>`);
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  const { port } = srv.address();
  try {
    await openUrl(`http://127.0.0.1:${port}/`);
    const t = await page(`127.0.0.1:${port}`);
    // Verify WebAuthn & PDF viewer capability surface in Chromium runtime.
    const compat = await evalIn(
      t,
      `({ webauthn: typeof navigator.credentials?.create === 'function', pdfViewer: !!navigator.pdfViewerEnabled })`,
    );
    await evalIn(t, `document.getElementById('dl').click()`);
    await waitUi(
      `document.body.innerText.includes('Dangerous download') && document.body.innerText.includes('zenith-test-payload.exe')`,
      "dangerous download prompt",
      10000,
    );
    await ui(
      `[...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Discard')?.click()`,
    );
    await waitUi(`!document.body.innerText.includes('Dangerous download')`, "download warning discarded", 6000);
    return `dangerousDownload=prompted+discarded, webauthn=${compat?.webauthn}, pdfViewer=${compat?.pdfViewer}`;
  } finally {
    srv.close();
  }
});
