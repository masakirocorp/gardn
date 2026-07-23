import { expect, test } from "bun:test";
import { createServer, type Server, type Socket } from "node:net";
import { rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const originalEnvironment = {
  OMH_ENV: process.env.OMH_ENV,
  OMH_PANE_ID: process.env.OMH_PANE_ID,
  OMH_SOCKET_PATH: process.env.OMH_SOCKET_PATH,
};
let importCounter = 0;

type HookHandler = (event: unknown, context?: unknown) => unknown;
type RequestRecord = Record<string, unknown>;

type RecordingSocket = {
  path: string;
  server: Server;
  requests: RequestRecord[];
  getConnections: () => number;
};

function isRecord(value: unknown): value is RequestRecord {
  return typeof value === "object" && value !== null;
}

function freshImport(path: string) {
  // Fresh imports intentionally exercise the extension's reload lifecycle.
  importCounter += 1;
  return import(`${path}?test=${importCounter}`);
}

async function recordingSocket(
  handle: (request: RequestRecord, connection: number, socket: Socket) => void,
): Promise<RecordingSocket> {
  const path = join(tmpdir(), `omh-agent-state-${process.pid}-${Date.now()}-${Math.random()}.sock`);
  await rm(path, { force: true });
  const requests: RequestRecord[] = [];
  let connections = 0;
  const server = createServer((socket) => {
    connections += 1;
    const connection = connections;
    let input = "";
    socket.setEncoding("utf8");
    socket.on("data", (chunk: string) => {
      input += chunk;
      const newline = input.indexOf("\n");
      if (newline === -1) return;
      const parsed: unknown = JSON.parse(input.slice(0, newline));
      if (isRecord(parsed)) {
        requests.push(parsed);
        handle(parsed, connection, socket);
      }
    });
  });
  const listening = Promise.withResolvers<void>();
  server.once("error", listening.reject);
  server.listen(path, listening.resolve);
  await listening.promise;
  return { path, server, requests, getConnections: () => connections };
}

async function closeRecordingSocket(recording: RecordingSocket): Promise<void> {
  const closed = Promise.withResolvers<void>();
  recording.server.close((error) => (error ? closed.reject(error) : closed.resolve()));
  await closed.promise;
  await rm(recording.path, { force: true });
  for (const [name, value] of Object.entries(originalEnvironment)) {
    if (value === undefined) delete process.env[name];
    else process.env[name] = value;
  }
}

function createPiHarness() {
  const handlers = new Map<string, HookHandler>();
  const eventHandlers = new Map<string, HookHandler>();
  return {
    handlers,
    eventHandlers,
    pi: {
      on(event: string, handler: HookHandler) {
        handlers.set(event, handler);
      },
      events: {
        on(event: string, handler: HookHandler) {
          eventHandlers.set(event, handler);
        },
      },
    },
  };
}

function stateRequests(requests: RequestRecord[], state: string): RequestRecord[] {
  return requests.filter((request) => {
    if (request.method !== "pane.report_agent" || !isRecord(request.params)) return false;
    return request.params.state === state;
  });
}

async function waitForState(requests: RequestRecord[], state: string): Promise<void> {
  const deadline = Date.now() + 2_000;
  while (Date.now() < deadline && stateRequests(requests, state).length === 0) {
    await Bun.sleep(5);
  }
  expect(stateRequests(requests, state).length).toBeGreaterThan(0);
}
async function waitForNewState(
  requests: RequestRecord[],
  state: string,
  previousCount: number,
): Promise<void> {
  const deadline = Date.now() + 2_000;
  while (Date.now() < deadline && stateRequests(requests, state).length <= previousCount) {
    await Bun.sleep(5);
  }
  expect(stateRequests(requests, state).length).toBeGreaterThan(previousCount);
}

test("Pi and OMP reloads preserve working status", async () => {
  for (const integration of [
    ["Pi", "./pi/omh-agent-state.ts"],
    ["OMP", "./omp/omh-agent-state.ts"],
  ] as const) {
    let recording: RecordingSocket | undefined;
    try {
      recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
      process.env.OMH_ENV = "1";
      process.env.OMH_SOCKET_PATH = recording.path;
      process.env.OMH_PANE_ID = "test:p1";
      const harness = createPiHarness();
      const { default: install } = await freshImport(integration[1]);
      install(harness.pi);
      const sessionStart = harness.handlers.get("session_start");
      expect(sessionStart).toBeDefined();
      await sessionStart?.(
        { reason: "reload" },
        {
          hasUI: true,
          isIdle: () => false,
          sessionManager: {
            getSessionFile: () => undefined,
            getSessionId: () => undefined,
          },
        },
      );
      await waitForState(recording.requests, "working");
    } finally {
      if (recording) await closeRecordingSocket(recording);
    }
  }
});

test("Pi and OMP ignore non-UI runtimes and release on shutdown", async () => {
  for (const integration of ["pi", "omp"] as const) {
    let recording: RecordingSocket | undefined;
    try {
      recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
      process.env.OMH_ENV = "1";
      process.env.OMH_SOCKET_PATH = recording.path;
      process.env.OMH_PANE_ID = `test:${integration}`;
      const harness = createPiHarness();
      const { default: install } = await freshImport(`./${integration}/omh-agent-state.ts`);
      install(harness.pi);
      const sessionStart = harness.handlers.get("session_start");
      const agentStart = harness.handlers.get("agent_start");
      const shutdown = harness.handlers.get("session_shutdown");
      expect(sessionStart).toBeDefined();
      expect(agentStart).toBeDefined();
      expect(shutdown).toBeDefined();
      await sessionStart?.({}, { hasUI: false, isIdle: () => false });
      await agentStart?.({}, { hasUI: false });
      if (integration === "pi") {
        const agentSettled = harness.handlers.get("agent_settled");
        expect(agentSettled).toBeDefined();
        await agentSettled?.({}, { hasUI: false, isIdle: () => true });
      }
      expect(recording.requests).toHaveLength(0);

      await sessionStart?.({}, { hasUI: true, isIdle: () => false });
      await shutdown?.({ type: "session_shutdown" });
      expect(recording.requests.some((request) => request.method === "pane.release_agent")).toBe(true);
    } finally {
      if (recording) await closeRecordingSocket(recording);
    }
  }
});

test("Pi reports idle only after the agent settles", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.OMH_ENV = "1";
    process.env.OMH_SOCKET_PATH = recording.path;
    process.env.OMH_PANE_ID = "test:pi-settled";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/omh-agent-state.ts");
    install(harness.pi);

    let idle = true;
    const context = {
      hasUI: true,
      isIdle: () => idle,
      sessionManager: {
        getSessionFile: () => undefined,
        getSessionId: () => undefined,
      },
    };
    const sessionStart = harness.handlers.get("session_start");
    const agentStart = harness.handlers.get("agent_start");
    const settled = harness.handlers.get("agent_settled");
    expect(sessionStart).toBeDefined();
    expect(agentStart).toBeDefined();
    expect(settled).toBeDefined();
    expect(harness.handlers.get("agent_end")).toBeUndefined();

    await sessionStart?.({}, context);
    await waitForState(recording.requests, "idle");

    idle = false;
    await agentStart?.({}, context);
    await waitForState(recording.requests, "working");

    const idleReportsBeforeStaleSettlement = stateRequests(recording.requests, "idle").length;
    await settled?.({}, context);
    expect(stateRequests(recording.requests, "idle")).toHaveLength(idleReportsBeforeStaleSettlement);

    idle = true;
    await settled?.({}, context);
    await waitForNewState(recording.requests, "idle", idleReportsBeforeStaleSettlement);
  } finally {
    if (recording) await closeRecordingSocket(recording);
  }
});

test("Pi settlement preserves blocked-state precedence", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.OMH_ENV = "1";
    process.env.OMH_SOCKET_PATH = recording.path;
    process.env.OMH_PANE_ID = "test:pi-settled-blocked";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/omh-agent-state.ts");
    install(harness.pi);

    let idle = true;
    const context = { hasUI: true, isIdle: () => idle };
    const sessionStart = harness.handlers.get("session_start");
    const agentStart = harness.handlers.get("agent_start");
    const settled = harness.handlers.get("agent_settled");
    const blocked = harness.eventHandlers.get("omh:blocked");
    expect(sessionStart).toBeDefined();
    expect(agentStart).toBeDefined();
    expect(settled).toBeDefined();
    expect(blocked).toBeDefined();

    await sessionStart?.({}, context);
    await waitForState(recording.requests, "idle");
    idle = false;
    await agentStart?.({}, context);
    await waitForState(recording.requests, "working");
    await blocked?.({ active: true, label: "approval" }, context);
    await waitForState(recording.requests, "blocked");

    idle = true;
    await settled?.({}, context);
    expect(stateRequests(recording.requests, "idle")).toHaveLength(1);
    expect(stateRequests(recording.requests, "blocked")).toHaveLength(1);
    await blocked?.({ active: false }, context);
    await waitForNewState(recording.requests, "idle", 1);
  } finally {
    if (recording) await closeRecordingSocket(recording);
  }
});


test("OMP session resume resets blocked state and reports its lifecycle source", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.OMH_ENV = "1";
    process.env.OMH_SOCKET_PATH = recording.path;
    process.env.OMH_PANE_ID = "test:p3";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./omp/omh-agent-state.ts");
    install(harness.pi);
    const sessionStart = harness.handlers.get("session_start");
    const sessionSwitch = harness.handlers.get("session_switch");
    const approval = harness.handlers.get("tool_approval_requested");
    const resolved = harness.handlers.get("tool_approval_resolved");
    expect(sessionStart).toBeDefined();
    expect(sessionSwitch).toBeDefined();
    expect(approval).toBeDefined();
    expect(resolved).toBeDefined();
    const context = {
      hasUI: true,
      isIdle: () => true,
      sessionManager: {
        getSessionFile: () => "/tmp/omp-resumed.jsonl",
        getSessionId: () => undefined,
      },
    };
    await sessionStart?.({}, context);
    await approval?.({ reason: "needs approval" }, context);
    await waitForState(recording.requests, "blocked");
    const idleReportsBeforeResume = stateRequests(recording.requests, "idle").length;
    await sessionSwitch?.({ reason: "resume" }, context);
    await waitForNewState(recording.requests, "idle", idleReportsBeforeResume);
    const sessionReports = recording.requests.filter((request) => request.method === "pane.report_agent_session");
    expect(sessionReports.some((request) => isRecord(request.params) && request.params.session_start_source === "resume")).toBe(true);
    await resolved?.({}, context);
  } finally {
    if (recording) await closeRecordingSocket(recording);
  }
});

test("Pi retries an unanswered socket report", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, connection, socket) => {
      if (connection > 1) socket.end("{}\n");
    });
    process.env.OMH_ENV = "1";
    process.env.OMH_SOCKET_PATH = recording.path;
    process.env.OMH_PANE_ID = "test:p4";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/omh-agent-state.ts");
    install(harness.pi);
    const sessionStart = harness.handlers.get("session_start");
    expect(sessionStart).toBeDefined();
    await sessionStart?.({}, {
      hasUI: true,
      isIdle: () => false,
      sessionManager: {
        getSessionFile: () => undefined,
        getSessionId: () => undefined,
      },
    });
    const deadline = Date.now() + 2_500;
    while (Date.now() < deadline && recording.getConnections() < 2) await Bun.sleep(5);
    expect(recording.getConnections()).toBeGreaterThanOrEqual(2);
    expect(recording.requests.length).toBeGreaterThanOrEqual(2);
    expect(recording.requests[1]).toEqual(recording.requests[0]);
  } finally {
    if (recording) await closeRecordingSocket(recording);
  }
});
