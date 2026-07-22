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

const expectedSource = `omh:${expectedAgent}`;
const pane = `pane-${expectedAgent}`;
const root = mkdtempSync(path.join(os.tmpdir(), `omh-${expectedAgent}-plugin-`));
const socketPath = path.join(root, "omh.sock");
const requests = [];
let dropNextLifecycleResponse = false;

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
      if (expectedAgent === "omp" && request.method === "pane.report_agent" && dropNextLifecycleResponse) {
        dropNextLifecycleResponse = false;
        continue;
      }
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

process.env.OMH_ENV = "1";
process.env.OMH_SOCKET_PATH = socketPath;
process.env.OMH_PANE_ID = pane;
process.env.PI_CONFIG_DIR = path.join(root, "config");
process.env.PI_CODING_AGENT_DIR = path.join(root, "agent");
process.env.OMH_PI_IDLE_DEBOUNCE_MS = "5";
process.env.OMH_OMP_IDLE_DEBOUNCE_MS = "5";
process.env.OMH_PI_RETRY_GRACE_MS = "10";
process.env.OMH_OMP_RETRY_GRACE_MS = "10";

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
    hasUI: true,
    isIdle: () => true,
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


function lifecycleRequests() {
  return requests.filter(
    (request) => request.method === "pane.report_agent" || request.method === "pane.release_agent",
  );
}

async function waitForRequests(count) {
  const deadline = Date.now() + 1000;
  while (lifecycleRequests().length < count && Date.now() < deadline) {
    await sleep(5);
  }
  assert.equal(
    lifecycleRequests().length >= count,
    true,
    `expected at least ${count} lifecycle requests, got ${lifecycleRequests().length}`,
  );
}

function reports() {
  return requests.filter((request) => request.method === "pane.report_agent");
}

function releases() {
  return requests.filter((request) => request.method === "pane.release_agent");
}

function sessionReports() {
  return requests.filter((request) => request.method === "pane.report_agent_session");
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
    assert.equal(typeof params.agent_session_path, "string", JSON.stringify(request));
    assert.ok(
      params.agent_session_path === `${root}/project/session.jsonl`
        || params.agent_session_path.startsWith(`${root}/project/session/`),
      JSON.stringify(request),
    );
    if (expectedAgent === "omp") {
      assert.equal("launch_env" in params, false, JSON.stringify(request));
    } else {
      assert.deepEqual(params.launch_env, {
        PI_CONFIG_DIR: path.join(root, "config"),
        PI_CODING_AGENT_DIR: path.join(root, "agent"),
      });
    }
  }
  if (expectedAgent === "omp") {
    assert.ok(sessionReports().length > 0, "OMP should report session launch context");
    for (const request of sessionReports()) {
      assert.deepEqual(request.params.launch_env, {
        PI_CONFIG_DIR: path.join(root, "config"),
        PI_CODING_AGENT_DIR: path.join(root, "agent"),
      });
    }
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

pi.events.on("omh:blocked", () => {});
pi.emit("event:omh:blocked", { active: true, label: "external blocker" });
await waitForRequests(5);
pi.emit("event:omh:blocked", { active: false });
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

const beforeManualCompact = lifecycleRequests().length;
pi.emit("session.compacting");
await waitForNewRequests(beforeManualCompact);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
const beforeManualCompactEnd = lifecycleRequests().length;
pi.emit("session_compact");
await sleep(100);
assert.equal(
  lifecycleRequests().length,
  beforeManualCompactEnd,
  "manual compaction completion must not publish idle while the agent continues",
);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
pi.emit("agent_end", { messages: [] });
await waitForNewRequests(beforeManualCompactEnd);
assert.equal(states().at(-1), "idle", JSON.stringify(states()));

const beforeAutoCompact = lifecycleRequests().length;
pi.emit("auto_compaction_start");
await waitForNewRequests(beforeAutoCompact);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
const beforeAutoCompactEnd = lifecycleRequests().length;
pi.emit("auto_compaction_end");
await sleep(100);
assert.equal(
  lifecycleRequests().length,
  beforeAutoCompactEnd,
  "automatic compaction completion must not publish idle while the agent continues",
);
assert.equal(states().at(-1), "working", JSON.stringify(states()));
pi.emit("agent_end", { messages: [] });
await waitForNewRequests(beforeAutoCompactEnd);
assert.equal(states().at(-1), "idle", JSON.stringify(states()));

const beforeDuplicateEnd = states().length;
pi.emit("agent_end", { messages: [] });
await sleep(20);
assert.equal(states().length, beforeDuplicateEnd, "duplicate agent_end should not publish another state");

if (expectedAgent === "omp") {
  const beforeDroppedLifecycle = lifecycleRequests().length;
  const beforeDroppedReports = reports().length;
  dropNextLifecycleResponse = true;
  pi.emit("agent_start");
  await waitForNewRequests(beforeDroppedLifecycle);
  const droppedRequest = reports()[beforeDroppedReports];
  pi.emit("agent_end", { messages: [] });
  await waitForNewRequests(beforeDroppedLifecycle + 1);
  assert.deepEqual(
    reports()[beforeDroppedReports + 1],
    droppedRequest,
    "dropped lifecycle report should be retried unchanged",
  );
  await waitForNewRequests(beforeDroppedLifecycle + 2);
  assert.equal(states().at(-1), "idle", JSON.stringify(states()));
  const afterRetryRequests = lifecycleRequests().length;
  await sleep(50);
  assert.equal(
    lifecycleRequests().length,
    afterRetryRequests,
    "successful lifecycle retry should not produce another report",
  );
}

const beforeRetryStart = lifecycleRequests().length;
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

const beforeBlockedRecovery = lifecycleRequests().length;
pi.emit("agent_start");
await waitForNewRequests(beforeBlockedRecovery);
assert.equal(states().at(-1), "working", JSON.stringify(states()));

const child = new Harness();
plugin(child);
child.emit("session_start", {}, context(`${root}/project/session/child.jsonl`, "child-session"));
const beforeChildStart = lifecycleRequests().length;
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

const beforeShutdown = lifecycleRequests().length;
pi.emit("session_shutdown", { reason: "quit" });
await waitForNewRequests(beforeShutdown);
assert.equal(releases().length, 1, "session_shutdown should release the pane agent");
assertCommon();

await new Promise((resolve) => server.close(resolve));
rmSync(root, { recursive: true, force: true });
console.log(`${expectedAgent} plugin status test ok: session refs, working, blocked, compaction, idle debounce, retry hold, release`);
