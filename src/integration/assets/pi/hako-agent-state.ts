// installed by hako
// managed by hako; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// HAKO_INTEGRATION_ID=pi
// HAKO_INTEGRATION_VERSION=2
// @ts-nocheck

import { createConnection } from "node:net";

const HAKO_ENV = process.env.HAKO_ENV;
const socketPath = process.env.HAKO_SOCKET_PATH;
const paneId = process.env.HAKO_PANE_ID;
const source = "hako:pi";

function enabled() {
  return HAKO_ENV === "1" && !!socketPath && !!paneId;
}

function sendRequest(request: unknown): Promise<void> {
  if (!enabled()) {
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    let done = false;
    const finish = () => {
      if (done) return;
      done = true;
      socket.destroy();
      resolve();
    };

    const socket = createConnection(socketPath!);
    socket.on("error", finish);
    socket.on("connect", () => socket.write(`${JSON.stringify(request)}\n`));
    socket.on("data", finish);
    socket.on("end", finish);
    const timeout = setTimeout(finish, 500);
    timeout.unref?.();
  });
}

type AgentState = "working" | "blocked" | "idle";

type QueuedState = {
  state: AgentState;
  message?: string;
  seq: number;
};

const idleDebounceMs = parseDurationEnv("HAKO_PI_IDLE_DEBOUNCE_MS", 250);
const retryGraceMs = parseDurationEnv("HAKO_PI_RETRY_GRACE_MS", 2500);
const retryableErrorPattern =
  /overloaded|provider.?returned.?error|rate.?limit|too many requests|429|500|502|503|504|service.?unavailable|server.?error|internal.?error|network.?error|connection.?error|connection.?refused|connection.?lost|websocket.?closed|websocket.?error|other side closed|fetch failed|upstream.?connect|reset before headers|socket hang up|ended without|http2 request did not get a response|timed? out|timeout|terminated|retry delay/i;
let reportSeq = Date.now() * 1000;
let currentAgentSessionId: string | undefined;
let currentAgentSessionPath: string | undefined;

function nextReportSeq(): number {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
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

function updateSessionRef(ctx: any): void {
  try {
    const file = ctx?.sessionManager?.getSessionFile?.();
    currentAgentSessionPath =
      typeof file === "string" && file.startsWith("/") ? file : undefined;
  } catch {
    currentAgentSessionPath = undefined;
  }

  try {
    const id = ctx?.sessionManager?.getSessionId?.();
    currentAgentSessionId = typeof id === "string" && id.length > 0 ? id : undefined;
  } catch {
    currentAgentSessionId = undefined;
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

  let agentActive = false;
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
    if (agentActive || retryHoldActive) {
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

  pi.on("session_start", (_event, ctx) => {
    updateSessionRef(ctx);
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

  pi.events.on("hako:blocked", (data) => {
    if (!data?.active) {
      blockedCount = Math.max(0, blockedCount - 1);
      if (blockedCount === 0) {
        blockedMessage = undefined;
      }
      publishState();
      return;
    }

    clearPendingTimers();
    blockedCount += 1;
    blockedMessage = data.label;
    publishState();
  });

  function isBlockingTool(event: any): boolean {
    return event?.toolName === "ask";
  }

  function clearBlockingTool(toolCallId: unknown): boolean {
    if (typeof toolCallId !== "string" || !blockingToolCalls.delete(toolCallId)) {
      return false;
    }

    blockedCount = Math.max(0, blockedCount - 1);
    if (blockedCount === 0) {
      blockedMessage = undefined;
    }
    publishState();
    return true;
  }

  pi.on("tool_execution_start", (event) => {
    if (!isBlockingTool(event) || typeof event?.toolCallId !== "string") {
      return;
    }
    if (blockingToolCalls.has(event.toolCallId)) {
      return;
    }

    clearPendingTimers();
    blockingToolCalls.add(event.toolCallId);
    blockedCount += 1;
    blockedMessage = typeof event.intent === "string" && event.intent.length > 0
      ? event.intent
      : "waiting for user";
    publishState();
  });

  pi.on("tool_execution_end", (event) => {
    clearBlockingTool(event?.toolCallId);
  });

  function markWorking() {
    clearPendingTimers();
    clearFailureState();
    agentActive = true;
    publishState();
  }

  function markIdle(event?: any) {
    if (!agentActive) {
      // Pi can emit duplicate/late end events while auto-retry is already
      // holding the pane in Working. Do not let an unqualified duplicate end
      // cancel the retry hold and publish a false Idle.
      return;
    }

    agentActive = false;

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
  pi.on("session_compact", markIdle);
  pi.on("auto_compaction_end", markIdle);

  pi.on("session_shutdown", async () => {
    clearPendingTimers();
    await releaseAgent();
  });
}
