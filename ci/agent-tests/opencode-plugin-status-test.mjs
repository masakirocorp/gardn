#!/usr/bin/env node
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const pluginPath = process.argv[2];
if (!pluginPath) {
  console.error("usage: opencode-plugin-status-test.mjs <omh-agent-state.js>");
  process.exit(1);
}

async function runScenario(name, events, options = {}) {
  const tmp = await import("node:fs/promises").then((fs) =>
    fs.mkdtemp(path.join(os.tmpdir(), `omh-opencode-${name}-`)),
  );
  const socketPath = path.join(tmp, "omh.sock");
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
      if (!options.noReply) {
        client.write(`${JSON.stringify({ id: request.id, result: { type: "ok" } })}\n`);
      }
    });
  });
  await new Promise((resolve) => server.listen(socketPath, resolve));

  process.env.OMH_ENV = "1";
  process.env.OMH_SOCKET_PATH = socketPath;
  process.env.OMH_PANE_ID = `pane-${name}`;
  process.env.OMH_OPENCODE_IDLE_REPORT_DELAY_MS = "5";
  process.env.OPENCODE_CONFIG = path.join(tmp, "opencode.json");

  const { OmhAgentStatePlugin } = await import(
    `${pathToFileURL(pluginPath).href}?${name}-${Date.now()}-${Math.random()}`,
  );
  const hooks = await OmhAgentStatePlugin();

  async function emit(item) {
    if (item.hook === "experimental.session.compacting") {
      await hooks["experimental.session.compacting"](item.input, item.output ?? {});
      return;
    }
    await hooks.event({ event: { type: item.type, properties: item.properties } });
  }


  for (const event of events) {
    await emit(event);
  }
  await new Promise((resolve) => setTimeout(resolve, 20));


  server.close();

  const reports = requests.filter((request) => request.method === "pane.report_agent");
  const sessions = requests.filter((request) => request.method === "pane.report_agent_session");
  const states = reports.map((request) => request.params.state);
  const sessionIDs = new Set(
    [...reports, ...sessions]
      .map((request) => request.params.agent_session_id)
      .filter(Boolean),
  );

  return { name, requests, reports, sessions, states, sessionIDs };
}

function fail(scenario, message) {
  console.error(`${scenario.name}: ${message}`);
  console.error(JSON.stringify(scenario.requests, null, 2));
  process.exit(1);
}

function assertStates(scenario, expected) {
  const actual = scenario.states.join(",");
  if (actual !== expected.join(",")) {
    fail(scenario, `expected states ${JSON.stringify(expected)}, observed ${JSON.stringify(scenario.states)}`);
  }
}

function assertOnlySession(scenario, expected) {
  if (scenario.sessionIDs.size !== 1 || !scenario.sessionIDs.has(expected)) {
    fail(scenario, `expected only session ${expected}, observed ${JSON.stringify([...scenario.sessionIDs])}`);
  }
}

function assertCommon(scenario) {
  if (!scenario.sessions.length) {
    fail(scenario, "missing session reports");
  }
  for (const request of scenario.reports) {
    const params = request.params;
    if (params.pane_id !== `pane-${scenario.name}`) fail(scenario, "wrong pane id");
    if (params.source !== "omh:opencode") fail(scenario, "wrong source");
    if (params.agent !== "opencode") fail(scenario, "wrong agent");
    if (!Number.isInteger(params.seq)) fail(scenario, "missing integer seq");
    if (params.launch_env.OPENCODE_CONFIG !== process.env.OPENCODE_CONFIG) {
      fail(scenario, "missing launch env");
    }
  }
}

const parent = "ses_parent";
const child = "ses_child";

const childAggregation = await runScenario("child-aggregation", [
  { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
  { type: "session.status", properties: { sessionID: parent, status: { type: "busy" } } },
  {
    type: "message.part.updated",
    properties: {
      sessionID: parent,
      part: { id: "foreground-task", type: "tool", tool: "task", state: { status: "pending" } },
    },
  },
  { type: "session.created", properties: { sessionID: child, info: { id: child, parentID: parent } } },
  { type: "session.status", properties: { sessionID: child, status: { type: "idle" } } },
  { type: "session.idle", properties: { sessionID: child } },
  {
    type: "message.part.updated",
    properties: {
      sessionID: parent,
      part: { id: "foreground-task", type: "tool", tool: "task", state: { status: "completed" } },
    },
  },
  {
    type: "message.part.updated",
    properties: {
      sessionID: parent,
      part: {
        id: "background-task",
        type: "tool",
        tool: "task",
        state: { status: "completed", metadata: { sessionID: child, background: true } },
      },
    },
  },
  { type: "permission.asked", properties: { sessionID: child, id: "child-permission" } },
  { type: "permission.replied", properties: { sessionID: child, id: "child-permission", reply: "allow" } },
  { type: "message.part.updated", properties: { sessionID: parent, part: { type: "step-finish", reason: "stop" } } },
  { type: "session.idle", properties: { sessionID: parent } },
  { type: "session.status", properties: { sessionID: child, status: { type: "idle" } } },
]);
assertCommon(childAggregation);
assertOnlySession(childAggregation, parent);
assertStates(childAggregation, ["working", "blocked", "working", "idle"]);

const foregroundCompletedChild = await runScenario("foreground-completed-child", [
  { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
  { type: "session.status", properties: { sessionID: parent, status: { type: "busy" } } },
  { type: "session.created", properties: { sessionID: child, info: { id: child, parentID: parent } } },
  { type: "session.status", properties: { sessionID: child, status: { type: "idle" } } },
  {
    type: "message.part.updated",
    properties: {
      sessionID: parent,
      part: {
        id: "foreground-child-task",
        type: "tool",
        tool: "task",
        state: { status: "completed", metadata: { sessionID: child } },
      },
    },
  },
  { type: "session.idle", properties: { sessionID: parent } },
]);
assertCommon(foregroundCompletedChild);
assertOnlySession(foregroundCompletedChild, parent);
assertStates(foregroundCompletedChild, ["working", "idle"]);

const permissions = await runScenario("permissions", [
  { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
  { type: "session.status", properties: { sessionID: parent, status: { type: "idle" } } },
  { type: "permission.asked", properties: { sessionID: parent, id: "p1" } },
  { type: "permission.asked", properties: { sessionID: parent, id: "p1" } },
  { type: "permission.replied", properties: { sessionID: parent, id: "p1", reply: "allow" } },
  { type: "permission.asked", properties: { sessionID: parent, id: "p2" } },
  { type: "permission.asked", properties: { sessionID: parent, id: "p3" } },
  { type: "permission.replied", properties: { sessionID: parent, id: "p2", reply: "allow" } },
  { type: "permission.replied", properties: { sessionID: parent, id: "p3", reply: "allow" } },
  { type: "permission.asked", properties: { sessionID: parent } },
  { type: "permission.asked", properties: { sessionID: parent } },
  { type: "permission.replied", properties: { sessionID: parent, reply: "allow" } },
  { type: "permission.replied", properties: { sessionID: parent, reply: "allow" } },
]);
assertCommon(permissions);
assertOnlySession(permissions, parent);
assertStates(permissions, ["blocked", "idle"]);

const anonymousTasks = await runScenario("anonymous-tasks", [
  { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
  { type: "session.status", properties: { sessionID: parent, status: { type: "idle" } } },
  { type: "message.part.updated", properties: { sessionID: parent, part: { type: "tool", tool: "task", state: { status: "pending" } } } },
  { type: "message.part.updated", properties: { sessionID: parent, part: { type: "tool", tool: "task", state: { status: "pending" } } } },
  { type: "session.idle", properties: { sessionID: parent } },
  { type: "message.part.updated", properties: { sessionID: parent, part: { type: "tool", tool: "task", state: { status: "completed" } } } },
  { type: "message.part.updated", properties: { sessionID: parent, part: { type: "tool", tool: "task", state: { status: "completed" } } } },
]);
assertCommon(anonymousTasks);
assertOnlySession(anonymousTasks, parent);
assertStates(anonymousTasks, ["working", "idle"]);

const prePrimary = await runScenario("pre-primary", [
  { type: "session.created", properties: { sessionID: child, info: { id: child, parentID: parent } } },
  { type: "session.status", properties: { sessionID: child, status: { type: "busy" } } },
  { type: "session.status", properties: { sessionID: parent, status: { type: "busy" } } },
  { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
  { type: "session.status", properties: { sessionID: parent, status: { type: "idle" } } },
  { type: "session.status", properties: { sessionID: child, status: { type: "idle" } } },
]);
assertCommon(prePrimary);
assertOnlySession(prePrimary, parent);
assertStates(prePrimary, ["working", "idle"]);

const compacting = await runScenario("compacting", [
  { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
  { hook: "experimental.session.compacting", input: { sessionID: parent } },
  { type: "session.idle", properties: { sessionID: parent } },
]);
assertCommon(compacting);
assertOnlySession(compacting, parent);
assertStates(compacting, ["working", "idle"]);

const noReply = await runScenario(
  "no-reply",
  [
    { type: "session.created", properties: { sessionID: parent, info: { id: parent } } },
    { type: "session.status", properties: { sessionID: parent, status: { type: "busy" } } },
    { type: "session.idle", properties: { sessionID: parent } },
  ],
  { noReply: true },
);
assertCommon(noReply);
assertOnlySession(noReply, parent);
assertStates(noReply, ["working", "idle"]);

console.log("opencode plugin status test ok: parent/child aggregation, permissions, compacting, pre-primary replay, socket no-reply");
