// Lists the JavaScript execution contexts of a page (main world plus extension content-script
// worlds), to see which extensions injected scripts into it.
//   node scripts/dev/contexts.mjs <url-substring> [--port 9444]
const args = process.argv.slice(2);
const port = args.includes("--port")
  ? Number(args[args.indexOf("--port") + 1])
  : Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
const match = args.find((a) => !a.startsWith("--") && !/^\d+$/.test(a)) ?? "";
const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const target = list.find(
  (t) =>
    t.type === "page" &&
    !t.url.includes("tauri.localhost") &&
    t.url.includes(match),
);
if (!target) {
  console.error(
    `no page matching "${match}"; have: ${list.map((t) => t.url).join(", ")}`,
  );
  process.exit(1);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = rej;
});
const contexts = [];
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.method === "Runtime.executionContextCreated") {
    const c = msg.params.context;
    contexts.push({
      name: c.name || "(main)",
      origin: c.origin,
      type: c.auxData?.type ?? "",
    });
  }
};
ws.send(JSON.stringify({ id: 1, method: "Runtime.enable" }));
await new Promise((r) => setTimeout(r, 1200));
ws.close();
console.log(JSON.stringify({ url: target.url, contexts }, null, 2));
