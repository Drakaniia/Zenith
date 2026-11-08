import { test } from "node:test";
import assert from "node:assert/strict";
import { connect } from "../apps/desktop/scripts/dev/cdp-session.mjs";

test("probe CDP sessions correlate responses, evaluate values and close", async (t) => {
  class Socket {
    static current: Socket;
    sent: { id: number; method: string; params: object }[] = [];
    onopen?: () => void;
    onmessage?: (event: { data: string }) => void;
    closed = false;
    url: string;
    constructor(url: string) {
      this.url = url;
      Socket.current = this;
      queueMicrotask(() => this.onopen?.());
    }
    send(data: string) { this.sent.push(JSON.parse(data)); }
    reply(data: object) { this.onmessage?.({ data: JSON.stringify(data) }); }
    close() { this.closed = true; }
  }
  const original = globalThis.WebSocket;
  globalThis.WebSocket = Socket as unknown as typeof WebSocket;
  t.after(() => { globalThis.WebSocket = original; });
  const session = await connect({ webSocketDebuggerUrl: "ws://probe" });
  const socket = Socket.current;
  assert.equal(socket.url, "ws://probe");
  const first = session.send("Runtime.enable");
  const second = session.send("Input.dispatchMouseEvent", { x: 1 });
  assert.deepEqual(socket.sent, [
    { id: 1, method: "Runtime.enable", params: {} },
    { id: 2, method: "Input.dispatchMouseEvent", params: { x: 1 } },
  ]);
  socket.reply({ method: "Runtime.event" });
  socket.reply({ id: 2, result: {} });
  socket.reply({ id: 1, result: {} });
  assert.equal((await first).id, 1);
  assert.equal((await second).id, 2);
  const value = session.ev("6 * 7");
  assert.deepEqual(socket.sent[2], {
    id: 3, method: "Runtime.evaluate",
    params: { expression: "6 * 7", awaitPromise: true, returnByValue: true },
  });
  socket.reply({ id: 3, result: { result: { value: 42 } } });
  assert.equal(await value, 42);
  session.close();
  assert.equal(socket.closed, true);
});
