// Isolated-world content script probe at document_start.
document.documentElement.dataset.zenithExt = "injected";
window.__EXT_ISOLATED_SECRET = "isolated-only";

if (location.pathname === "/page" && location.port) {
  chrome.runtime.sendMessage({ type: "run_checks", port: location.port }).catch(() => {});
}
