import { afterEach, expect, test } from "bun:test";
import net, { createServer, type Server, type Socket } from "node:net";
import { EventEmitter } from "node:events";
import { rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const originalPlatform = process.platform;
const originalCreateConnection = net.createConnection;
const originalEnvironment = {
  GARDN_ENV: process.env.GARDN_ENV,
  GARDN_PANE_ID: process.env.GARDN_PANE_ID,
  GARDN_SOCKET_PATH: process.env.GARDN_SOCKET_PATH,
};
let importCounter = 0;

afterEach(() => {
  Object.defineProperty(process, "platform", { value: originalPlatform });
  net.createConnection = originalCreateConnection;
  for (const [name, value] of Object.entries(originalEnvironment)) {
    if (value === undefined) delete process.env[name];
    else process.env[name] = value;
  }
});

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

function configureIntegrationEnvironment(socketPath: string) {
  process.env.GARDN_ENV = "1";
  process.env.GARDN_SOCKET_PATH = socketPath;
  process.env.GARDN_PANE_ID = "test:p1";
}

type CapturedSocket = EventEmitter & {
  destroy: () => CapturedSocket;
  setTimeout: () => CapturedSocket;
  write: (...args: unknown[]) => boolean;
  end: (...args: unknown[]) => CapturedSocket;
};

function captureConnectionEndpoint() {
  let connectedEndpoint: unknown;
  net.createConnection = ((...args: unknown[]) => {
    connectedEndpoint = args[0];
    const socket = new EventEmitter() as CapturedSocket;
    socket.destroy = () => socket;
    socket.setTimeout = () => socket;
    socket.write = () => {
      queueMicrotask(() => socket.emit("data"));
      return true;
    };
    socket.end = (...endArgs: unknown[]) => {
      const callback = endArgs.find((value) => typeof value === "function");
      if (typeof callback === "function") callback();
      queueMicrotask(() => socket.emit("close"));
      return socket;
    };
    queueMicrotask(() => {
      const callback = args.find((value) => typeof value === "function");
      if (typeof callback === "function") callback();
      socket.emit("connect");
    });
    return socket as unknown as Socket;
  }) as typeof net.createConnection;
  return () => connectedEndpoint;
}

test.serial("Pi maps the Windows socket marker path to a named pipe endpoint", async () => {
  const markerPath = `gardn-pi-${process.pid}.sock`;
  configureIntegrationEnvironment(markerPath);
  Object.defineProperty(process, "platform", { value: "win32" });
  const connectedEndpoint = captureConnectionEndpoint();
  const harness = createPiHarness();

  const { default: install } = await freshImport("./pi/gardn-agent-state.ts");
  install(harness.pi);
  await harness.handlers.get("session_start")?.(
    { reason: "startup" },
    {
      hasUI: true,
      mode: "tui",
      isIdle: () => true,
      sessionManager: {
        getSessionFile: () => undefined,
        getSessionId: () => undefined,
      },
    },
  );

  expect(connectedEndpoint()).toBe(`\\\\.\\pipe\\${markerPath}`);
});

test.serial("OMP maps the Windows socket marker path to a named pipe endpoint", async () => {
  const markerPath = `gardn-omp-${process.pid}.sock`;
  configureIntegrationEnvironment(markerPath);
  Object.defineProperty(process, "platform", { value: "win32" });
  const connectedEndpoint = captureConnectionEndpoint();
  const harness = createPiHarness();

  const { default: install } = await freshImport("./omp/gardn-agent-state.ts");
  install(harness.pi);
  await harness.handlers.get("session_start")?.(
    { reason: "startup" },
    {
      hasUI: true,
      isIdle: () => true,
      sessionManager: {
        getSessionFile: () => undefined,
        getSessionId: () => undefined,
      },
    },
  );

  expect(connectedEndpoint()).toBe(`\\\\.\\pipe\\${markerPath}`);
});

test("OMP accepts POSIX and Windows session paths", async () => {
  const { isAbsoluteSessionPath } = await freshImport("./omp/gardn-agent-state.ts");

  expect(isAbsoluteSessionPath("/tmp/omp-session.jsonl")).toBe(true);
  expect(isAbsoluteSessionPath("C:\\Users\\User\\.omp\\agent\\sessions\\omp-session.jsonl")).toBe(
    true,
  );
  expect(isAbsoluteSessionPath("C:/Users/User/.omp/agent/sessions/omp-session.jsonl")).toBe(true);
  expect(isAbsoluteSessionPath("relative/omp-session.jsonl")).toBe(false);
});

test.serial("OpenCode maps the Windows socket marker path to a named pipe endpoint", async () => {
  const markerPath = `gardn-opencode-${process.pid}.sock`;
  configureIntegrationEnvironment(markerPath);
  Object.defineProperty(process, "platform", { value: "win32" });
  const connectedEndpoint = captureConnectionEndpoint();

  const { GardnAgentStatePlugin } = await freshImport("./opencode/gardn-agent-state.js");
  const plugin = await GardnAgentStatePlugin();
  await plugin.event?.({
    event: {
      type: "session.updated",
      properties: { sessionID: "opencode-session" },
    },
  });

  expect(connectedEndpoint()).toBe(`\\\\.\\pipe\\${markerPath}`);
});

test.serial("OpenCode keeps the Unix socket endpoint unchanged", async () => {
  const socketPath = `/tmp/gardn-opencode-${process.pid}.sock`;
  configureIntegrationEnvironment(socketPath);
  Object.defineProperty(process, "platform", { value: "darwin" });
  const connectedEndpoint = captureConnectionEndpoint();

  const { GardnAgentStatePlugin } = await freshImport("./opencode/gardn-agent-state.js");
  const plugin = await GardnAgentStatePlugin();
  await plugin.event?.({
    event: {
      type: "session.updated",
      properties: { sessionID: "opencode-session" },
    },
  });

  expect(connectedEndpoint()).toBe(socketPath);
});

async function recordingSocket(
  handle: (request: RequestRecord, connection: number, socket: Socket) => void,
): Promise<RecordingSocket> {
  const path = join(tmpdir(), `gardn-agent-state-${process.pid}-${Date.now()}-${Math.random()}.sock`);
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

test.serial("Pi and OMP reloads preserve working status", async () => {
  for (const integration of [
    ["Pi", "./pi/gardn-agent-state.ts"],
    ["OMP", "./omp/gardn-agent-state.ts"],
  ] as const) {
    let recording: RecordingSocket | undefined;
    try {
      recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
      process.env.GARDN_ENV = "1";
      process.env.GARDN_SOCKET_PATH = recording.path;
      process.env.GARDN_PANE_ID = "test:p1";
      const harness = createPiHarness();
      const { default: install } = await freshImport(integration[1]);
      install(harness.pi);
      const sessionStart = harness.handlers.get("session_start");
      expect(sessionStart).toBeDefined();
      await sessionStart?.(
        { reason: "reload" },
        {
          hasUI: true,
          mode: "tui",
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

test.serial("Pi and OMP ignore non-UI runtimes and release on shutdown", async () => {
  for (const integration of ["pi", "omp"] as const) {
    let recording: RecordingSocket | undefined;
    try {
      recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
      process.env.GARDN_ENV = "1";
      process.env.GARDN_SOCKET_PATH = recording.path;
      process.env.GARDN_PANE_ID = `test:${integration}`;
      const harness = createPiHarness();
      const { default: install } = await freshImport(`./${integration}/gardn-agent-state.ts`);
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

      await sessionStart?.({}, { hasUI: true, mode: "tui", isIdle: () => false });
      await shutdown?.({ type: "session_shutdown" });
      expect(recording.requests.some((request) => request.method === "pane.release_agent")).toBe(true);
    } finally {
      if (recording) await closeRecordingSocket(recording);
    }
  }
});

test.serial("Pi reports idle only after the agent settles", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.GARDN_ENV = "1";
    process.env.GARDN_SOCKET_PATH = recording.path;
    process.env.GARDN_PANE_ID = "test:pi-settled";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/gardn-agent-state.ts");
    install(harness.pi);

    let idle = true;
    const context = {
      hasUI: true,
      mode: "tui",
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

test.serial("Pi ignores RPC sessions even when UI APIs are available", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.GARDN_ENV = "1";
    process.env.GARDN_SOCKET_PATH = recording.path;
    process.env.GARDN_PANE_ID = "test:pi-rpc";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/gardn-agent-state.ts");
    install(harness.pi);

    const context = {
      hasUI: true,
      mode: "rpc",
      isIdle: () => true,
    };
    const sessionStart = harness.handlers.get("session_start");
    const agentStart = harness.handlers.get("agent_start");
    const settled = harness.handlers.get("agent_settled");
    expect(sessionStart).toBeDefined();
    expect(agentStart).toBeDefined();
    expect(settled).toBeDefined();

    await sessionStart?.({ reason: "startup" }, context);
    await agentStart?.({}, context);
    await settled?.({}, context);
    await Bun.sleep(25);

    expect(recording.requests).toEqual([]);
  } finally {
    if (recording) await closeRecordingSocket(recording);
  }
});

test.serial("Pi settlement preserves blocked-state precedence", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.GARDN_ENV = "1";
    process.env.GARDN_SOCKET_PATH = recording.path;
    process.env.GARDN_PANE_ID = "test:pi-settled-blocked";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/gardn-agent-state.ts");
    install(harness.pi);

    let idle = true;
    const context = { hasUI: true, mode: "tui", isIdle: () => idle };
    const sessionStart = harness.handlers.get("session_start");
    const agentStart = harness.handlers.get("agent_start");
    const settled = harness.handlers.get("agent_settled");
    const blocked = harness.eventHandlers.get("gardn:blocked");
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

test.serial("OMP session resume resets blocked state and reports its lifecycle source", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, _connection, socket) => socket.end("{}\n"));
    process.env.GARDN_ENV = "1";
    process.env.GARDN_SOCKET_PATH = recording.path;
    process.env.GARDN_PANE_ID = "test:p3";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./omp/gardn-agent-state.ts");
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

test.serial("Pi retries an unanswered socket report", async () => {
  let recording: RecordingSocket | undefined;
  try {
    recording = await recordingSocket((_request, connection, socket) => {
      if (connection > 1) socket.end("{}\n");
    });
    process.env.GARDN_ENV = "1";
    process.env.GARDN_SOCKET_PATH = recording.path;
    process.env.GARDN_PANE_ID = "test:p4";
    const harness = createPiHarness();
    const { default: install } = await freshImport("./pi/gardn-agent-state.ts");
    install(harness.pi);
    const sessionStart = harness.handlers.get("session_start");
    expect(sessionStart).toBeDefined();
    await sessionStart?.({}, {
      hasUI: true,
      mode: "tui",
      isIdle: () => false,
      sessionManager: {
        getSessionFile: () => undefined,
        getSessionId: () => undefined,
      },
    });
    const deadline = Date.now() + 3_500;
    while (Date.now() < deadline && recording.getConnections() < 2) await Bun.sleep(5);
    expect(recording.getConnections()).toBeGreaterThanOrEqual(2);
    expect(recording.requests.length).toBeGreaterThanOrEqual(2);
    expect(recording.requests[1]).toEqual(recording.requests[0]);
  } finally {
    if (recording) await closeRecordingSocket(recording);
  }
});
