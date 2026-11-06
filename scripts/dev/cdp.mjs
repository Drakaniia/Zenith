// Minimal Chrome DevTools Protocol client for driving a running Zenith instance.
// Requires the app to be started with ZENITH_REMOTE_DEBUG_PORT (see run.ps1; default test port 9444).
//
//   node scripts/dev/cdp.mjs list
//   node scripts/dev/cdp.mjs eval "<expression>"            (chrome UI page)
//   node scripts/dev/cdp.mjs eval --target example.com "<expression>"  (first page whose URL matches)
//   node scripts/dev/cdp.mjs wait "<expression>" [timeoutMs]  (poll until truthy)
//   node scripts/dev/cdp.mjs shot out.png [--target ...]      (capture one webview)
import { writeFileSync } from "node:fs";

const argv = process.argv.slice(2);
let port = Number(process.env.ZENITH_REMOTE_DEBUG_PORT || 9444);
let target = "chrome";
const rest = [];
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === "--port") port = Number(argv[++i]);
  else if (argv[i] === "--target") target = argv[++i];
  else rest.push(argv[i]);
}
const [cmd, ...rawParams] = rest;
// Some shells pass quoting characters through literally; strip them so expressions evaluate.
const params = rawParams.map((p) =>
  p.replace(/^\\?"/, "").replace(/\\?"$/, ""),
);

async function targets() {
  const res = await fetch(`http://127.0.0.1:${port}/json/list`);
  return res.json();
}

function isChrome(t) {
  return /^(https?:\/\/tauri\.localhost|tauri:\/\/localhost|http:\/\/127\.0\.0\.1:1420)/.test(
    t.url,
  );
}

async function pick() {
  const list = (await targets()).filter((t) => t.type === "page");
  const t =
    target === "chrome"
      ? list.find(isChrome)
      : list.find((x) => !isChrome(x) && x.url.includes(target));
  if (!t)
    throw new Error(
      `no target matching ${target}; have: ${list.map((x) => x.url).join(", ")}`,
    );
  return t;
}

async function session(t) {
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });
  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const mid = ++id;
      pending.set(mid, resolve);
      ws.send(JSON.stringify({ id: mid, method, params }));
    });
  return { send, close: () => ws.close() };
}

async function evaluate(expression) {
  const s = await session(await pick());
  const r = await s.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  s.close();
  if (r.result?.exceptionDetails) {
    throw new Error(
      JSON.stringify(
        r.result.exceptionDetails.exception?.description ??
          r.result.exceptionDetails,
      ),
    );
  }
  return r.result?.result?.value;
}

try {
  if (cmd === "list") {
    for (const t of await targets())
      console.log(`${t.type}\t${t.id}\t${t.url}`);
  } else if (cmd === "eval") {
    const v = await evaluate(params.join(" "));
    console.log(typeof v === "string" ? v : JSON.stringify(v, null, 2));
  } else if (cmd === "wait") {
    const [expr, timeout = "10000"] = params;
    const until = Date.now() + Number(timeout);
    let v;
    while (Date.now() < until) {
      try {
        v = await evaluate(expr);
        if (v) break;
      } catch {}
      await new Promise((r) => setTimeout(r, 250));
    }
    console.log(JSON.stringify(v));
    if (!v) process.exit(2);
  } else if (cmd === "shot") {
    const s = await session(await pick());
    const r = await s.send("Page.captureScreenshot", { format: "png" });
    s.close();
    writeFileSync(params[0], Buffer.from(r.result.data, "base64"));
    console.log(`saved ${params[0]}`);
  } else {
    console.log(
      "commands: list | eval <expr> | wait <expr> [ms] | shot <file>",
    );
    process.exit(1);
  }
} catch (e) {
  console.error(String(e?.stack || e));
  process.exit(1);
}
