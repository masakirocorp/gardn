#!/usr/bin/env node
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const pluginPath = process.argv[2];
if (!pluginPath) {
  console.error("usage: opencode-plugin-status-test.mjs <hako-agent-state.js>");
  process.exit(1);
}

const tmp = await import("node:fs/promises").then((fs) =>
  fs.mkdtemp(path.join(os.tmpdir(), "hako-opencode-plugin-test-")),
);
const socketPath = path.join(tmp, "hako.sock");
const requests = [];

const server = net.createServer((client) => {
  let data = "";
  client.on("data", (chunk) => {
    data += chunk.toString("utf8");
    if (!data.endsWith("\n")) {
      return;
    }
    const request = JSON.parse(data);
    requests.push(request);
    client.write(`${JSON.stringify({ id: request.id, result: { type: "ok" } })}\n`);
  });
});
await new Promise((resolve) => server.listen(socketPath, resolve));

process.env.HAKO_ENV = "1";
process.env.HAKO_SOCKET_PATH = socketPath;
process.env.HAKO_PANE_ID = "pane-opencode-plugin-unit";
process.env.OPENCODE_CONFIG = path.join(tmp, "opencode.json");

const { HakoAgentStatePlugin } = await import(`${pathToFileURL(pluginPath).href}?${Date.now()}`);
const hooks = await HakoAgentStatePlugin();

async function emit(type, properties) {
  await hooks.event({ event: { type, properties } });
}

const parent = "ses_parent";
const child = "ses_child";
const childFirst = "ses_child_first";
const parentAfterChild = "ses_parent_after_child";

await emit("session.created", {
  sessionID: childFirst,
  info: { id: childFirst, parentID: parentAfterChild },
});
await emit("session.status", { sessionID: childFirst, status: { type: "busy" } });
await emit("session.created", { sessionID: parent, info: { id: parent } });
await emit("session.status", { sessionID: parent, status: { type: "busy" } });
await emit("message.part.updated", {
  sessionID: parent,
  part: { id: "foreground-task", type: "tool", tool: "task", state: { status: "pending" } },
});
await emit("session.created", {
  sessionID: child,
  info: { id: child, parentID: parent },
});
await emit("session.status", { sessionID: child, status: { type: "idle" } });
await emit("session.idle", { sessionID: child });
await emit("message.part.updated", {
  sessionID: parent,
  part: { id: "foreground-task", type: "tool", tool: "task", state: { status: "completed" } },
});
await emit("message.part.updated", {
  sessionID: parent,
  part: {
    id: "background-task",
    type: "tool",
    tool: "task",
    state: { status: "completed", metadata: { sessionID: child, background: true } },
  },
});
await emit("permission.asked", { sessionID: child, id: "child-permission" });
await emit("permission.replied", { sessionID: child, id: "child-permission", reply: "allow" });
await emit("message.part.updated", {
  sessionID: parent,
  part: { type: "step-finish", reason: "stop" },
});
await emit("session.idle", { sessionID: parent });
await emit("session.status", { sessionID: child, status: { type: "idle" } });
await emit("permission.asked", { sessionID: parent, id: "primary-permission" });
await emit("permission.replied", { sessionID: parent, id: "primary-permission", reply: "reject" });
await emit("permission.asked", { sessionID: parent });
await emit("permission.asked", { sessionID: parent });
await emit("permission.replied", { sessionID: parent, reply: "allow" });
await emit("permission.replied", { sessionID: parent, reply: "allow" });
await emit("message.part.updated", {
  sessionID: parent,
  part: { type: "tool", tool: "task", state: { status: "pending" } },
});
await emit("message.part.updated", {
  sessionID: parent,
  part: { type: "tool", tool: "task", state: { status: "pending" } },
});
await emit("session.idle", { sessionID: parent });
await emit("message.part.updated", {
  sessionID: parent,
  part: { type: "tool", tool: "task", state: { status: "completed" } },
});
await emit("message.part.updated", {
  sessionID: parent,
  part: { type: "tool", tool: "task", state: { status: "completed" } },
});
server.close();

const reports = requests.filter((request) => request.method === "pane.report_agent");
const sessions = requests.filter((request) => request.method === "pane.report_agent_session");
const states = reports.map((request) => request.params.state);
const sessionIDs = new Set(
  [...reports, ...sessions]
    .map((request) => request.params.agent_session_id)
    .filter(Boolean),
);

function fail(message) {
  console.error(message);
  console.error(JSON.stringify(requests, null, 2));
  process.exit(1);
}

if (!sessions.length) {
  fail("missing session reports");
}
if (sessionIDs.size !== 1 || !sessionIDs.has(parent)) {
  fail(`expected only parent session id, observed ${JSON.stringify([...sessionIDs])}`);
}
if (states.join(",") !== "working,blocked,working,idle,blocked,idle,blocked,idle,working,idle") {
  fail(`unexpected state sequence ${JSON.stringify(states)}`);
}

for (const request of reports) {
  const params = request.params;
  if (params.pane_id !== "pane-opencode-plugin-unit") fail("wrong pane id");
  if (params.source !== "hako:opencode") fail("wrong source");
  if (params.agent !== "opencode") fail("wrong agent");
  if (!Number.isInteger(params.seq)) fail("missing integer seq");
  if (params.launch_env.OPENCODE_CONFIG !== process.env.OPENCODE_CONFIG) fail("missing launch env");
}

console.log("opencode plugin status test ok: parent session authority, task subagent, blocked, idle");
