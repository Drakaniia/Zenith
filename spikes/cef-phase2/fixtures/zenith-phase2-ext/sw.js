// MV3 service worker exercising action badges, direct tab creation, and native messaging.
let ran = false;

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg?.type === "run_checks" && msg.port && !ran) {
    ran = true;
    runAllChecks(String(msg.port)).finally(() => sendResponse({ ok: true }));
    return true;
  }
  return false;
});

async function runAllChecks(port) {
  const result = {
    extensionId: chrome.runtime.id,
    workerAlive: true,
    badgeText: null,
    badgeColor: null,
    badgeOk: false,
    directTabId: null,
    nativeOk: false,
    nativeError: null,
  };

  try {
    await chrome.action.setBadgeText({ text: "42" });
    await chrome.action.setBadgeBackgroundColor({ color: "#ff5500" });
    const text = await chrome.action.getBadgeText({});
    const color = await chrome.action.getBadgeBackgroundColor({});
    result.badgeText = text;
    result.badgeColor = Array.isArray(color) ? color : null;
    result.badgeOk =
      text === "42" &&
      Array.isArray(color) &&
      color[0] === 255 &&
      color[1] === 85 &&
      color[2] === 0;
  } catch (e) {
    result.badgeText = "ERR:" + String(e);
  }

  try {
    const resp = await chrome.runtime.sendNativeMessage("com.zenith.phase2.echo", {
      ping: "hello-cef",
    });
    result.nativeOk = Boolean(resp && resp.ok === true && resp.echo?.ping === "hello-cef");
    if (!result.nativeOk) {
      result.nativeError = "unexpected_response:" + JSON.stringify(resp);
    }
  } catch (e) {
    result.nativeError = String(e?.message || e);
  }

  try {
    const created = await chrome.tabs.create({
      url: `http://127.0.0.1:${port}/ext-created`,
    });
    result.directTabId = typeof created?.id === "number" ? created.id : null;
    if (typeof created?.id === "number") {
      // Close the probe tab after load so no untracked Chrome window blocks CEF shutdown.
      const tid = created.id;
      setTimeout(() => {
        chrome.tabs.remove(tid).catch(() => {});
      }, 1000);
    }
  } catch (e) {
    result.directTabId = -1;
  }

  await fetch(`http://127.0.0.1:${port}/report/sw`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(result),
  }).catch(() => {});
}
