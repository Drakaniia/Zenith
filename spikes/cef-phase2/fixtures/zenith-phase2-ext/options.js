(async () => {
  const port = new URLSearchParams(location.search).get("port");
  if (!port) return;
  const ok = Boolean(chrome.runtime.id && chrome.runtime.getManifest()?.manifest_version === 3);
  await fetch(`http://127.0.0.1:${port}/report/options`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ ok }),
  }).catch(() => {});
})();
