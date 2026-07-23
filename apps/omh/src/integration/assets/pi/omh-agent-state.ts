// installed by Oh My Herdr
// managed by Oh My Herdr; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// OMH_INTEGRATION_ID=pi
// OMH_INTEGRATION_VERSION=6
// @ts-nocheck

import net from "node:net";

const OMH_ENV = process.env.OMH_ENV;
const socketPath = process.env.OMH_SOCKET_PATH;
const socketEndpoint =
  process.platform === "win32" && socketPath ? `\\\\.\\pipe\\${socketPath}` : socketPath;
const paneId = process.env.OMH_PANE_ID;
const source = "omh:pi";

function enabled() {
  return OMH_ENV === "1" && !!socketPath && !!paneId;
}

function sendRequestAttempt(request: unknown, timeoutMs: number): Promise<boolean> {
  if (!enabled()) {
    return Promise.resolve(true);
  }

  const { promise, resolve } = Promise.withResolvers<boolean>();
  let done = false;
  let timeout;
  const socket = net.createConnection(socketEndpoint!);
  const finish = (delivered: boolean) => {
    if (done) return;
    done = true;
    clearTimeout(timeout);
    socket.destroy();
    resolve(delivered);
  };

  socket.on("error", () => finish(false));
  socket.on("connect", () => socket.write(`${JSON.stringify(request)}\n`));
  socket.on("data", () => finish(true));
  socket.on("end", () => finish(false));
  timeout = setTimeout(() => finish(false), timeoutMs);
  timeout.unref?.();
  return promise;
}

async function sendRequest(request: unknown): Promise<void> {
  if (await sendRequestAttempt(request, 500)) {
    return;
  }
  await sendRequestAttempt(request, 1500);
}

type AgentState = "working" | "blocked" | "idle";

type QueuedState = {
  state: AgentState;
  message?: string;
  seq: number;
};

let reportSeq = Date.now() * 1000;
let currentAgentSessionId: string | undefined;
let currentAgentSessionPath: string | undefined;
const activeAgents = new Set<symbol>();

function nextReportSeq(): number {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
}

function sessionManagerStringMethod(ctx: unknown, method: string): string | undefined {
  let manager: unknown;
  if (ctx && typeof ctx === "object" && "sessionManager" in ctx) {
    manager = ctx.sessionManager;
  }
  if (!manager || typeof manager !== "object" || !(method in manager)) {
    return undefined;
  }

  const candidate = manager[method as keyof typeof manager];
  if (typeof candidate !== "function") {
    return undefined;
  }

  try {
    const result = candidate.call(manager);
    return typeof result === "string" ? result : undefined;
  } catch {
    return undefined;
  }
}

function updateSessionRef(ctx: unknown): void {
  const file = sessionManagerStringMethod(ctx, "getSessionFile");
  const id = sessionManagerStringMethod(ctx, "getSessionId");

  if (file && file.startsWith("/")) {
    currentAgentSessionPath = file;
    currentAgentSessionId = undefined;
    return;
  }

  if (id && id.length > 0) {
    currentAgentSessionPath = undefined;
    currentAgentSessionId = id;
  }
}

function launchEnv(): Record<string, string> {
  const env: Record<string, string> = {};
  for (const key of ["PI_CONFIG_DIR", "PI_CODING_AGENT_DIR"]) {
    const value = process.env[key];
    if (typeof value === "string" && value.length > 0) {
      env[key] = value;
    }
  }
  return env;
}

function withSessionRef(params: Record<string, unknown>): Record<string, unknown> {
  const sessionParams = currentAgentSessionPath
    ? { ...params, agent_session_path: currentAgentSessionPath }
    : currentAgentSessionId
      ? { ...params, agent_session_id: currentAgentSessionId }
      : params;
  return { ...sessionParams, launch_env: launchEnv() };
}

function sendState(state: AgentState, message?: string, seq = nextReportSeq()): Promise<void> {
  return sendRequest({
    id: `${source}:${Date.now()}:${Math.random().toString(36).slice(2)}`,
    method: "pane.report_agent",
    params: withSessionRef({
      pane_id: paneId,
      source,
      agent: "pi",
      state,
      message,
      seq,
    }),
  });
}

let sendInFlight = false;
let queuedState: QueuedState | undefined;

function queueState(state: AgentState, message?: string): void {
  queuedState = { state, message, seq: nextReportSeq() };
  if (!sendInFlight) {
    void drainStateQueue();
  }
}

async function drainStateQueue(): Promise<void> {
  if (sendInFlight) {
    return;
  }

  sendInFlight = true;
  try {
    while (queuedState) {
      const next = queuedState;
      queuedState = undefined;
      await sendState(next.state, next.message, next.seq);
    }
  } finally {
    sendInFlight = false;
    if (queuedState) {
      void drainStateQueue();
    }
  }
}

function reportSession(sessionStartSource = "startup"): Promise<void> {
  if (!currentAgentSessionPath && !currentAgentSessionId) {
    return Promise.resolve();
  }
  return sendRequest({
    id: `${source}:session:${Date.now()}:${Math.random().toString(36).slice(2)}`,
    method: "pane.report_agent_session",
    params: withSessionRef({
      pane_id: paneId,
      source,
      agent: "pi",
      seq: nextReportSeq(),
      session_start_source: sessionStartSource,
    }),
  });
}


function releaseAgent(): Promise<void> {
  return sendRequest({
    id: `${source}:release:${Date.now()}:${Math.random().toString(36).slice(2)}`,
    method: "pane.release_agent",
    params: withSessionRef({
      pane_id: paneId,
      source,
      agent: "pi",
      seq: nextReportSeq(),
    }),
  });
}

export default function (pi) {
  if (!enabled()) {
    return;
  }

  const instanceId = Symbol("omh-pi-agent");
  let blockedCount = 0;
  let blockedMessage: string | undefined;
  let lastState: AgentState | undefined;
  let lastMessage: string | undefined;
  const blockingToolCalls = new Set<string>();
  let rootSession = false;
  const permissionGateToolCalls = new Set<string>();


  function desiredState() {
    if (blockedCount > 0) {
      return { state: "blocked" as const, message: blockedMessage };
    }
    if (activeAgents.size > 0) {
      return { state: "working" as const, message: undefined };
    }
    return { state: "idle" as const, message: undefined };
  }

  function publishState(force = false) {
    const next = desiredState();
    if (!force && next.state === lastState && next.message === lastMessage) {
      return;
    }
    lastState = next.state;
    lastMessage = next.message;
    queueState(next.state, next.message);
  }

  function sessionIsActive(ctx: unknown): boolean {
    if (!ctx || typeof ctx !== "object" || !("isIdle" in ctx)) {
      return false;
    }
    const candidate = ctx.isIdle;
    if (typeof candidate !== "function") {
      return false;
    }
    try {
      return candidate.call(ctx) === false;
    } catch {
      return false;
    }
  }

  function sessionIsIdle(ctx: unknown): boolean {
    if (!ctx || typeof ctx !== "object" || !("isIdle" in ctx)) {
      return false;
    }
    const candidate = ctx.isIdle;
    if (typeof candidate !== "function") {
      return false;
    }
    try {
      return candidate.call(ctx) === true;
    } catch {
      return false;
    }
  }

  function activateRootSession(ctx: unknown, sessionStartSource = "startup"): boolean {
    if (!ctx || typeof ctx !== "object" || !("hasUI" in ctx) || ctx.hasUI !== true) {
      return false;
    }
    rootSession = true;
    updateSessionRef(ctx);
    void reportSession(sessionStartSource);
    return true;
  }

  pi.on("session_start", (_event, ctx) => {
    if (!ctx || typeof ctx !== "object" || !("hasUI" in ctx) || ctx.hasUI !== true) {
      rootSession = false;
      return;
    }
    if (!activateRootSession(ctx)) {
      return;
    }
    if (sessionIsActive(ctx)) {
      activeAgents.add(instanceId);
    } else {
      activeAgents.delete(instanceId);
    }
    publishState(true);
  });
  function rootSessionActive(ctx?: unknown): boolean {
    return rootSession || activateRootSession(ctx);
  }

  function enterBlocked(message: string | undefined) {
    blockedCount += 1;
    blockedMessage = message;
    publishState();
  }

  function leaveBlocked() {
    blockedCount = Math.max(0, blockedCount - 1);
    if (blockedCount === 0) {
      blockedMessage = undefined;
    }
    publishState();
  }

  pi.events.on("omh:blocked", (data) => {
    if (!rootSession) {
      return;
    }
    if (!data?.active) {
      leaveBlocked();
      return;
    }

    enterBlocked(data.label);
  });

  pi.events.on("masakiro:permission_gate", (data) => {
    if (!rootSession) {
      return;
    }
    const toolCallId = typeof data?.toolCallId === "string" ? data.toolCallId : undefined;
    if (!toolCallId) {
      return;
    }

    if (!data?.active) {
      if (!permissionGateToolCalls.delete(toolCallId)) {
        return;
      }
      leaveBlocked();
      return;
    }

    if (permissionGateToolCalls.has(toolCallId)) {
      return;
    }

    permissionGateToolCalls.add(toolCallId);
    const reason = typeof data.reason === "string" && data.reason.length > 0
      ? data.reason
      : "waiting for permission";
    enterBlocked(reason);
  });

  function isBlockingTool(event: any): boolean {
    return event?.toolName === "ask";
  }

  function clearBlockingTool(toolCallId: unknown): boolean {
    if (typeof toolCallId !== "string" || !blockingToolCalls.delete(toolCallId)) {
      return false;
    }

    leaveBlocked();
    return true;
  }

  pi.on("tool_execution_start", (event, ctx) => {
    if (!rootSessionActive(ctx) || !isBlockingTool(event) || typeof event?.toolCallId !== "string") {
      return;
    }
    if (blockingToolCalls.has(event.toolCallId)) {
      return;
    }

    blockingToolCalls.add(event.toolCallId);
    blockedCount += 1;
    blockedMessage = typeof event.intent === "string" && event.intent.length > 0
      ? event.intent
      : "waiting for user";
    publishState();
  });

  pi.on("tool_execution_end", (event, ctx) => {
    if (!rootSessionActive(ctx)) {
      return;
    }
    clearBlockingTool(event?.toolCallId);
  });

  function markWorking(_event?: unknown, ctx?: unknown) {
    if (!rootSessionActive(ctx)) {
      return;
    }
    activeAgents.add(instanceId);
    publishState();
  }

  function markSettled(_event?: unknown, ctx?: unknown) {
    if (!rootSessionActive(ctx) || !sessionIsIdle(ctx)) {
      return;
    }
    if (!activeAgents.delete(instanceId)) {
      return;
    }
    publishState();
  }

  pi.on("agent_start", markWorking);
  pi.on("session_before_compact", markWorking);
  pi.on("session.compacting", markWorking);
  pi.on("auto_compaction_start", markWorking);
  pi.on("agent_settled", markSettled);
  pi.on("session_compact", markWorking);
  pi.on("auto_compaction_end", markWorking);

  pi.on("session_shutdown", async () => {
    if (!rootSession) {
      return;
    }
    activeAgents.delete(instanceId);
    if (activeAgents.size === 0) {
      await releaseAgent();
    }
  });
}
