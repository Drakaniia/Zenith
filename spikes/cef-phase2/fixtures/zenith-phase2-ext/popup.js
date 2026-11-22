(async () => {
  const port = new URLSearchParams(location.search).get("port");
  if (!port) return;
  let tabsCount = 0;
  let ok = false;
  try {
    const tabs = await chrome.tabs.query({});
    tabsCount = Array.isArray(tabs) ? tabs.length : 0;
    ok = Boolean(chrome.runtime.id) && tabsCount > 0;
  } catch (_) {}
  await fetch(`http://127.0.0.1:${port}/report/popup`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ ok, tabsCount }),
  }).catch(() => {});
})();
