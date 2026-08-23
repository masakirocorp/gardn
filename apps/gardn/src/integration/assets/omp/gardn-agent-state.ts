// installed by Gardn
// managed by Gardn; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// GARDN_INTEGRATION_ID=omp
// GARDN_INTEGRATION_VERSION=9
// @ts-nocheck

import net from "node:net";
import path from "node:path";

const GARDN_ENV = process.env.GARDN_ENV;
const socketPath = process.env.GARDN_SOCKET_PATH;
const socketEndpoint =
  process.platform === "win32" && socketPath ? `\\\\.\\pipe\\${socketPath}` : socketPath;
const paneId = process.env.GARDN_PANE_ID;
const source = "gardn:omp";

function enabled() {
  return GARDN_ENV === "1" && !!socketPath && !!paneId;
}

let requestQueue = Promise.resolve();

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

async function sendRequestNow(request: unknown): Promise<void> {
  if (await sendRequestAttempt(request, 500)) {
    return;
  }
  await sendRequestAttempt(request, 1500);
}

function sendRequest(request: unknown): Promise<void> {
  requestQueue = requestQueue.then(
    () => sendRequestNow(request),
    () => sendRequestNow(request),
  );
  return requestQueue;
}

type AgentState = "working" | "blocked" | "idle";

type QueuedState = {
  state: AgentState;
  message?: string;
  seq: number;
};

const idleDebounceMs = parseDurationEnv("GARDN_OMP_IDLE_DEBOUNCE_MS", 250);
const retryGraceMs = parseDurationEnv("GARDN_OMP_RETRY_GRACE_MS", 2500);
const retryableErrorPattern =
  /overloaded|provider.?returned.?error|rate.?limit|too many requests|429|500|502|503|504|service.?unavailable|server.?error|internal.?error|network.?error|connection.?error|connection.?refused|connection.?lost|websocket.?closed|websocket.?error|other side closed|fetch failed|upstream.?connect|reset before headers|socket hang up|ended without|http2 request did not get a response|timed? out|timeout|terminated|retry delay/i;
let reportSeq = Date.now() * 1000;
let currentAgentSessionId: string | undefined;
let currentAgentSessionPath: string | undefined;
const activeAgents = new Set<symbol>();


function nextReportSeq(): number {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
}
export function isAbsoluteSessionPath(file: unknown): file is string {
  return (
    typeof file === "string" &&
    (path.posix.isAbsolute(file) || path.win32.isAbsolute(file))
  );
}


function parseDurationEnv(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) {
    return fallback;
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return fallback;
  }
  return parsed;
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

  if (isAbsoluteSessionPath(file)) {
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
  return sessionParams;
}

function sendState(state: AgentState, message?: string, seq = nextReportSeq()): Promise<void> {
  return sendRequest({
    id: `${source}:${Date.now()}:${Math.random().toString(36).slice(2)}`,
    method: "pane.report_agent",
    params: withSessionRef({
      pane_id: paneId,
      source,
      agent: "omp",
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

function lastAssistantMessage(messages: unknown[]): any | undefined {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    const message = messages[i] as any;
    if (message?.role === "assistant") {
      return message;
    }
  }
  return undefined;
}

function retryableErrorMessage(event: any): string | undefined {
  const messages = Array.isArray(event?.messages) ? event.messages : [];
  const assistant = lastAssistantMessage(messages);
  if (assistant?.stopReason !== "error") {
    return undefined;
  }

  const errorMessage = String(assistant.errorMessage ?? "");
  if (!retryableErrorPattern.test(errorMessage)) {
    return undefined;
  }
  return errorMessage || "retryable provider error";
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
      agent: "omp",
      seq: nextReportSeq(),
      session_start_source: sessionStartSource,
      launch_env: launchEnv(),
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
      agent: "omp",
      seq: nextReportSeq(),
    }),
  });
}

export default function (pi) {
  if (!enabled()) {
    return;
  }

  const instanceId = Symbol("gardn-omp-agent");
  let rootSession = false;
  let retryHoldActive = false;
  let failureBlocked = false;
  let failureMessage: string | undefined;
  let blockedCount = 0;
  let blockedMessage: string | undefined;
  let lastState: AgentState | undefined;
  let lastMessage: string | undefined;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  const blockingToolCalls = new Set<string>();
  const permissionGateToolCalls = new Set<string>();

  function clearTimer(timer: ReturnType<typeof setTimeout> | undefined) {
    if (timer) {
      clearTimeout(timer);
    }
  }

  function clearPendingTimers() {
    clearTimer(idleTimer);
    clearTimer(retryTimer);
    idleTimer = undefined;
    retryTimer = undefined;
  }

  function clearFailureState() {
    retryHoldActive = false;
    failureBlocked = false;
    failureMessage = undefined;
  }

  function desiredState() {
    if (blockedCount > 0) {
      return { state: "blocked" as const, message: blockedMessage };
    }
    if (failureBlocked) {
      return { state: "blocked" as const, message: failureMessage };
    }
    if (activeAgents.size > 0 || retryHoldActive) {
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

  function scheduleIdle() {
    clearPendingTimers();
    clearFailureState();
    idleTimer = setTimeout(() => {
      idleTimer = undefined;
      publishState();
    }, idleDebounceMs);
    idleTimer.unref?.();
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

  function activateRootSession(ctx: unknown, sessionStartSource = "startup"): boolean {
    if (!ctx || typeof ctx !== "object" || !("hasUI" in ctx) || ctx.hasUI !== true) {
      return false;
    }
    rootSession = true;
    updateSessionRef(ctx);
    void reportSession(sessionStartSource);
    return true;
  }

  function resetSessionState() {
    clearPendingTimers();
    clearFailureState();
    activeAgents.delete(instanceId);
    blockedCount = 0;
    blockedMessage = undefined;
    blockingToolCalls.clear();
    permissionGateToolCalls.clear();
  }

  function rootSessionActive(ctx?: unknown): boolean {
    return rootSession || activateRootSession(ctx);
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

  pi.on("session_switch", (event, ctx) => {
    if (!activateRootSession(ctx, event?.reason || "resume")) {
      return;
    }
    resetSessionState();
    publishState(true);
  });

  function holdForRetry(message: string) {
    clearPendingTimers();
    retryHoldActive = true;
    failureBlocked = false;
    failureMessage = message;
    publishState();

    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      retryHoldActive = false;
      failureBlocked = true;
      publishState();
    }, retryGraceMs);
    retryTimer.unref?.();
  }

  function enterBlocked(message: string | undefined) {
    clearPendingTimers();
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

  pi.events.on("gardn:blocked", (data) => {
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
  function askBlockedMessage(args: unknown): string {
    if (!args || typeof args !== "object" || !("questions" in args) || !Array.isArray(args.questions)) {
      return "waiting for user input";
    }
    const firstQuestion = args.questions.find(
      (question: unknown) =>
        question && typeof question === "object" && "question" in question
        && typeof question.question === "string",
    );
    return firstQuestion?.question || "waiting for user input";
  }


  function clearBlockingTool(toolCallId: unknown): boolean {
    if (typeof toolCallId !== "string" || !blockingToolCalls.delete(toolCallId)) {
      return false;
    }

    leaveBlocked();
    return true;
  }

  pi.on("tool_approval_requested", (event, ctx) => {
    if (!rootSessionActive(ctx)) {
      return;
    }
    const label = typeof event?.reason === "string" && event.reason.length > 0
      ? event.reason
      : `${event?.toolName || "Tool"} approval`;
    enterBlocked(label);
  });

  pi.on("tool_approval_resolved", (_event, ctx) => {
    if (!rootSessionActive(ctx)) {
      return;
    }
    leaveBlocked();
  });

  pi.on("tool_execution_start", (event, ctx) => {
    if (!rootSessionActive(ctx) || !isBlockingTool(event) || typeof event?.toolCallId !== "string") {
      return;
    }
    if (blockingToolCalls.has(event.toolCallId)) {
      return;
    }

    clearPendingTimers();
    blockingToolCalls.add(event.toolCallId);
    blockedCount += 1;
    blockedMessage = askBlockedMessage(event.args);
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
    clearPendingTimers();
    clearFailureState();
    activeAgents.add(instanceId);
    publishState();
  }

function isNonterminalAgentEnd(event: unknown): boolean {
  return (
    !!event
    && typeof event === "object"
    && "isTerminal" in event
    && event.isTerminal === false
  );
}

  function markIdle(event?: unknown, ctx?: unknown) {
    if (!rootSessionActive(ctx)) {
      return;
    }
    if (isNonterminalAgentEnd(event)) {
      return;
    }
    if (!activeAgents.delete(instanceId)) {
      // OMP can emit duplicate/late end events while auto-retry is already
      // holding the pane in Working. Do not let an unqualified duplicate end
      // cancel the retry hold and publish a false Idle.
      return;
    }


    const retryableMessage = retryableErrorMessage(event);
    if (retryableMessage) {
      holdForRetry(retryableMessage);
      return;
    }

    scheduleIdle();
  }

  pi.on("agent_start", markWorking);
  pi.on("session_before_compact", markWorking);
  pi.on("session.compacting", markWorking);
  pi.on("auto_compaction_start", markWorking);
  pi.on("agent_end", markIdle);
  pi.on("session_compact", markWorking);
  pi.on("auto_compaction_end", markWorking);

  pi.on("session_shutdown", async () => {
    if (!rootSession) {
      return;
    }
    clearPendingTimers();
    activeAgents.delete(instanceId);
    if (activeAgents.size === 0) {
      await releaseAgent();
    }
  });

}
