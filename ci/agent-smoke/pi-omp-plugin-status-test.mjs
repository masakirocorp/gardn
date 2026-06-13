#!/usr/bin/env node
import assert from "node:assert/strict";
import { once } from "node:events";
import { mkdtempSync, rmSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const pluginPath = process.argv[2];
const expectedAgent = process.argv[3];
if (!pluginPath || !expectedAgent) {
  console.error("usage: pi-omp-plugin-status-test.mjs <plugin.ts> <pi|omp>");
  process.exit(64);
}

const expectedSource = `hako:${expectedAgent}`;
const pane = `pane-${expectedAgent}`;
const root = mkdtempSync(path.join(os.tmpdir(), `hako-${expectedAgent}-plugin-`));
const socketPath = path.join(root, "hako.sock");
const requests = [];

const server = net.createServer((conn) => {
  let buffer = "";
  conn.on("data", (chunk) => {
    buffer += chunk.toString("utf8");
    let index;
    while ((index = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (!line.trim()) continue;
      const request = JSON.parse(line);
      requests.push(request);
      conn.write(`${JSON.stringify({ id: request.id, result: { type: "ok" } })}\n`);
      conn.end();
    }
  });
});
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(socketPath, () => {
    server.off("error", reject);
    resolve();
  });
});

process.env.HAKO_ENV = "1";
process.env.HAKO_SOCKET_PATH = socketPath;
process.env.HAKO_PANE_ID = pane;
process.env.PI_CONFIG_DIR = path.join(root, "config");
process.env.PI_CODING_AGENT_DIR = path.join(root, "agent");
process.env.HAKO_PI_IDLE_DEBOUNCE_MS = "5";
process.env.HAKO_OMP_IDLE_DEBOUNCE_MS = "5";
process.env.HAKO_PI_RETRY_GRACE_MS = "10";
process.env.HAKO_OMP_RETRY_GRACE_MS = "10";

class Harness {
  constructor() {
    this.handlers = new Map();
    this.events = { on: (name, handler) => this.on(`event:${name}`, handler) };
  }

  on(name, handler) {
    const handlers = this.handlers.get(name) ?? [];
    handlers.push(handler);
    this.handlers.set(name, handlers);
  }

  emit(name, event = {}, ctx = undefined) {
    for (const handler of this.handlers.get(name) ?? []) {
      handler(event, ctx);
    }
  }
}

function context(sessionFile = `${root}/project/session.jsonl`, sessionId = "session-root") {
  return {
    sessionManager: {
      getSessionFile: () => sessionFile,
      getSessionId: () => sessionId,
    },
  };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
async function waitForNewRequests(previousCount, added = 1) {
  await waitForRequests(previousCount + added);
}


async function waitForRequests(count) {
  const deadline = Date.now() + 1000;
  while (requests.length < count && Date.now() < deadline) {
    await sleep(5);
  }
  assert.equal(requests.length >= count, true, `expected at least ${count} requests, got ${requests.length}`);
}

function reports() {
  return requests.filter((request) => request.method === "pane.report_agent");
}

function releases() {
  return requests.filter((request) => request.method === "pane.release_agent");
}

function states() {
  return reports().map((request) => request.params.state);
}

function assertCommon() {
  for (const request of [...reports(), ...releases()]) {
    const params = request.params;
    assert.equal(params.pane_id, pane, JSON.stringify(request));
    assert.equal(params.source, expectedSource, JSON.stringify(request));
    assert.equal(params.agent, expectedAgent, JSON.stringify(request));
    assert.equal(typeof params.seq, "number", JSON.stringify(request));
    assert.equal(params.agent_session_path, `${root}/project/session.jsonl`, JSON.stringify(request));
    assert.deepEqual(params.launch_env, {
      PI_CONFIG_DIR: path.join(root, "config"),
      PI_CODING_AGENT_DIR: path.join(root, "agent"),
    });
  }
}

function assertContainsInOrder(expected) {
  const observed = states();
  let start = 0;
  for (const state of expected) {
    const index = observed.indexOf(state, start);
    assert.notEqual(index, -1, `missing ${state} after ${start}; observed ${JSON.stringify(observed)}`);
    start = index + 1;
  }
}

const module = await import(`${pathToFileURL(pluginPath).href}?t=${Date.now()}`);
const plugin = module.default;
const pi = new Harness();
plugin(pi);

pi.emit("session_start", {}, context());
await waitForRequests(1);
assertContainsInOrder(["idle"]);

pi.emit("agent_start");
await waitForRequests(2);
assertContainsInOrder(["idle", "working"]);

pi.emit("tool_execution_start", {
  toolName: "ask",
  toolCallId: "ask-1",
  intent: "choose A/B",
});
await waitForRequests(3);
assertContainsInOrder(["idle", "working", "blocked"]);

pi.emit("tool_execution_start", {
  toolName: "ask",
  toolCallId: "ask-1",
  intent: "duplicate ask should not overcount",
});
await sleep(20);
assert.equal(states().filter((state) => state === "blocked").length, 1, JSON.stringify(states()));

pi.emit("tool_execution_end", { toolCallId: "ask-1" });
await waitForRequests(4);
assertContainsInOrder(["idle", "working", "blocked", "working"]);

pi.events.on("hako:blocked", () => {});
pi.emit("event:hako:blocked", { active: true, label: "external blocker" });
await waitForRequests(5);
pi.emit("event:hako:blocked", { active: false });
await waitForRequests(6);
assertContainsInOrder(["blocked", "working", "blocked", "working"]);

pi.emit("event:masakiro:permission_gate", {
  active: true,
  toolName: "bash",
  toolCallId: "perm-1",
  reason: "recursive delete",
  command: "rm -rf tmp",
});
await waitForRequests(7);
pi.emit("event:masakiro:permission_gate", {
  active: true,
  toolName: "bash",
  toolCallId: "perm-1",
  reason: "duplicate permission should not overcount",
});
await sleep(20);
assert.equal(states().filter((state) => state === "blocked").length, 3, JSON.stringify(states()));
pi.emit("event:masakiro:permission_gate", {
  active: false,
  approved: true,
  toolName: "bash",
  toolCallId: "perm-1",
  reason: "recursive delete",
});
await waitForRequests(8);
assertContainsInOrder(["blocked", "working", "blocked", "working", "blocked", "working"]);

pi.emit("agent_end", { messages: [] });
await sleep(20);
assert.equal(states().at(-1), "idle", JSON.stringify(states()));

const beforeManualCompact = requests.length;
pi.emit("session.compacting");
await waitForNewRequests(beforeManualCompact);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
pi.emit("session_compact");
await sleep(20);
assert.equal(states().at(-1), "idle", JSON.stringify(states()));

const beforeAutoCompact = requests.length;
pi.emit("auto_compaction_start");
await waitForNewRequests(beforeAutoCompact);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
pi.emit("auto_compaction_end");
await sleep(20);
assert.equal(states().at(-1), "idle", JSON.stringify(states()));

const beforeDuplicateEnd = states().length;
pi.emit("agent_end", { messages: [] });
await sleep(20);
assert.equal(states().length, beforeDuplicateEnd, "duplicate agent_end should not publish another state");

const beforeRetryStart = requests.length;
pi.emit("agent_start");
await waitForNewRequests(beforeRetryStart);
pi.emit("agent_end", {
  messages: [
    {
      role: "assistant",
      stopReason: "error",
      errorMessage: "provider returned error 503",
    },
  ],
});
assert.equal(states().at(-1), "working", JSON.stringify(states()));
await sleep(30);
assert.equal(states().at(-1), "blocked", JSON.stringify(states()));

const beforeBlockedRecovery = requests.length;
pi.emit("agent_start");
await waitForNewRequests(beforeBlockedRecovery);
assert.equal(states().at(-1), "working", JSON.stringify(states()));

const child = new Harness();
plugin(child);
child.emit("session_start", {}, context(`${root}/project/session/child.jsonl`, "child-session"));
const beforeChildStart = requests.length;
child.emit("agent_start");
await waitForNewRequests(beforeChildStart);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
child.emit("agent_end", { messages: [] });
await sleep(20);
assert.equal(states().at(-1), "working", "child end must not idle the parent while parent is active");
const releasesBeforeChildShutdown = releases().length;
child.emit("session_shutdown");
await sleep(20);
assert.equal(releases().length, releasesBeforeChildShutdown, "child shutdown must not release the parent pane");

pi.emit("agent_end", { messages: [] });
await sleep(20);
assert.equal(states().at(-1), "idle", JSON.stringify(states()));

const beforeShutdown = requests.length;
pi.emit("session_shutdown");
await waitForNewRequests(beforeShutdown);
assert.equal(releases().length, 1, "session_shutdown should release the pane agent");
assertCommon();

await new Promise((resolve) => server.close(resolve));
rmSync(root, { recursive: true, force: true });
console.log(`${expectedAgent} plugin status test ok: session refs, working, blocked, compaction, idle debounce, retry hold, release`);
