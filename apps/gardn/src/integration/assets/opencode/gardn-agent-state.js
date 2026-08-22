// installed by Gardn
// managed by Gardn; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// GARDN_INTEGRATION_ID=opencode
// GARDN_INTEGRATION_VERSION=8

import net from "node:net";

const SOURCE = "gardn:opencode";
let reportSeq = Date.now() * 1000;

function nextReportSeq() {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
}
const IDLE_REPORT_DELAY_MS = Number.parseInt(
  process.env.GARDN_OPENCODE_IDLE_REPORT_DELAY_MS ?? "750",
  10,
);


const sessions = new Map();
const busyChildren = new Set();
const backgroundChildren = new Set();
const activeTasks = new Set();
const pendingPermissionIDs = new Set();
const anonymousPermissions = new Map();
let anonymousActiveTasks = 0;
let lastReportedKey;
let pendingIdleTimer;
let pendingIdleKey;
let primarySessionID;
let primaryBusy = false;
function clearPrimarySessionRuntime() {
  primaryBusy = false;
  busyChildren.clear();
  backgroundChildren.clear();
  activeTasks.clear();
  pendingPermissionIDs.clear();
  anonymousPermissions.clear();
  anonymousActiveTasks = 0;
  cancelPendingIdle();
}

function setPrimarySession(sessionID) {
  if (!sessionID || primarySessionID === sessionID) {
    return;
  }

  primarySessionID = sessionID;
  clearPrimarySessionRuntime();
}


function sessionIDFromProperties(properties) {
  return typeof properties?.sessionID === "string" && properties.sessionID
    ? properties.sessionID
    : undefined;
}

function parentIDFromProperties(properties) {
  const parentID = properties?.info?.parentID ?? properties?.info?.parentId;
  return typeof parentID === "string" && parentID ? parentID : undefined;
}

function rememberSession(sessionID, properties, focusPrimary = false) {
  if (!sessionID) {
    return;
  }

  const previous = sessions.get(sessionID) ?? {};
  const parentID = parentIDFromProperties(properties) ?? previous.parentID;
  const status = properties?.status?.type ?? previous.status;
  sessions.set(sessionID, { parentID, status });

  if (!parentID && (!primarySessionID || focusPrimary)) {
    setPrimarySession(sessionID);
  }
}

function isPrimarySession(sessionID) {
  return Boolean(sessionID && primarySessionID && sessionID === primarySessionID);
}
function stateFromStoredStatus(status) {
  return stateFromSessionStatus({ type: status });
}

function reconcileKnownSessionState() {
  if (!primarySessionID) {
    return;
  }

  const primaryState = stateFromStoredStatus(sessions.get(primarySessionID)?.status);
  if (primaryState) {
    primaryBusy = primaryState === "working";
  }

  for (const [sessionID, info] of sessions) {
    if (!isChildOfPrimary(sessionID)) {
      continue;
    }
    const state = stateFromStoredStatus(info.status);
    if (state === "working") {
      busyChildren.add(sessionID);
    } else if (state === "idle") {
      busyChildren.delete(sessionID);
      backgroundChildren.delete(sessionID);
    }
  }
}


function isChildOfPrimary(sessionID) {
  if (!sessionID || !primarySessionID) {
    return false;
  }

  let currentID = sessionID;
  const seen = new Set();
  while (currentID && !seen.has(currentID)) {
    seen.add(currentID);
    const parentID = sessions.get(currentID)?.parentID;
    if (!parentID) {
      return false;
    }
    if (parentID === primarySessionID) {
      return true;
    }
    currentID = parentID;
  }

  return false;
}

function visibleSessionID() {
  return primarySessionID;
}

function visibleState() {
  if (pendingPermissionIDs.size > 0 || anonymousPermissions.size > 0) {
    return "blocked";
  }
  if (
    primaryBusy ||
    activeTasks.size > 0 ||
    anonymousActiveTasks > 0 ||
    busyChildren.size > 0 ||
    backgroundChildren.size > 0
  ) {
    return "working";
  }
  return "idle";
}

function taskKey(properties) {
  const part = properties?.part;
  const id = part?.id ?? part?.callID ?? part?.callId ?? part?.toolCallID ?? part?.toolCallId;
  return typeof id === "string" && id ? id : undefined;
}

function permissionID(properties) {
  const id = properties?.id ?? properties?.permissionID ?? properties?.permissionId;
  return typeof id === "string" && id ? id : undefined;
}

function anonymousPermissionKey(sessionID) {
  return `${sessionID ?? "unknown"}\0anon`;
}

function identifiedPermissionKey(sessionID, id) {
  return `${sessionID ?? "unknown"}\0id\0${id}`;
}

function incrementPermission(sessionID, properties) {
  const id = permissionID(properties);
  if (id) {
    pendingPermissionIDs.add(identifiedPermissionKey(sessionID, id));
    return;
  }

  const key = anonymousPermissionKey(sessionID);
  anonymousPermissions.set(key, (anonymousPermissions.get(key) ?? 0) + 1);
}

function decrementPermission(sessionID, properties) {
  const id = permissionID(properties);
  if (id) {
    pendingPermissionIDs.delete(identifiedPermissionKey(sessionID, id));
    return;
  }

  const key = anonymousPermissionKey(sessionID);
  const count = anonymousPermissions.get(key) ?? 0;
  if (count <= 1) {
    anonymousPermissions.delete(key);
  } else {
    anonymousPermissions.set(key, count - 1);
  }
}
function launchEnv() {
  const env = {};
  for (const key of ["OPENCODE_CONFIG", "XDG_DATA_HOME"]) {
    const value = process.env[key];
    if (typeof value === "string" && value.length > 0) {
      env[key] = value;
    }
  }
  return env;
}

function requestEnvelope(method, params) {
  const paneId = process.env.GARDN_PANE_ID;

  return {
    id: `${SOURCE}:${Date.now()}:${Math.floor(Math.random() * 1_000_000)
      .toString()
      .padStart(6, "0")}`,
    method,
    params: {
      pane_id: paneId,
      source: SOURCE,
      agent: "opencode",
      seq: nextReportSeq(),
      ...params,
    },
  };
}

function sendRequest(request) {
  const socketPath = process.env.GARDN_SOCKET_PATH;

  if (!process.env.GARDN_PANE_ID || !socketPath) {
    return Promise.resolve();
  }

  const socketEndpoint =
    process.platform === "win32" ? `\\\\.\\pipe\\${socketPath}` : socketPath;

  return new Promise((resolve) => {
    const client = net.createConnection(socketEndpoint, () => {
      client.end(`${JSON.stringify(request)}\n`, resolve);
    });

    const finish = () => {
      client.destroy();
      resolve();
    };

    client.setTimeout(100, finish);
    client.on("error", finish);
    client.on("close", resolve);
  });
}

function reportSession(sessionID) {
  if (!sessionID) {
    return Promise.resolve();
  }

  return sendRequest(
    requestEnvelope("pane.report_agent_session", {
      agent_session_id: sessionID,
      launch_env: launchEnv(),
    }),
  );
}

function reportAgent(state, sessionID) {
  return sendRequest(
    requestEnvelope("pane.report_agent", {
      state,
      agent_session_id: sessionID,
      launch_env: launchEnv(),
    }),
  );
}

function stateFromSessionStatus(status) {
  switch (status?.type) {
    case "busy":
    case "retry":
      return "working";
    case "idle":
      return "idle";
    default:
      return undefined;
  }
}
function taskIsBackground(properties) {
  const metadata = properties?.part?.state?.metadata;
  return metadata?.background === true || metadata?.state === "running";
}


function idleAssistantMessage(properties) {
  const info = properties?.info;
  return (
    info?.role === "assistant" &&
    info?.finish === "stop" &&
    typeof info?.time?.completed === "number"
  );
}

function taskToolStatus(properties) {
  const part = properties?.part;
  if (part?.type !== "tool" || part?.tool !== "task") {
    return undefined;
  }
  const status = part?.state?.status;
  return typeof status === "string" ? status : undefined;
}

function stopStepFinished(properties) {
  const part = properties?.part;
  return part?.type === "step-finish" && part?.reason === "stop";
}

function childSessionIDFromTask(properties) {
  const metadata = properties?.part?.state?.metadata;
  const sessionID =
    metadata?.sessionID ??
    metadata?.sessionId ??
    metadata?.session?.id ??
    metadata?.childSessionID ??
    metadata?.childSessionId;
  return typeof sessionID === "string" && sessionID ? sessionID : undefined;
}


async function emitAgentState(state, sessionID, reportKey) {
  lastReportedKey = reportKey;
  await reportAgent(state, sessionID);
}

function cancelPendingIdle() {
  if (!pendingIdleTimer) {
    return;
  }
  clearTimeout(pendingIdleTimer);
  pendingIdleTimer = undefined;
  pendingIdleKey = undefined;
}

async function recomputeAgentState() {
  const state = visibleState();
  const sessionID = visibleSessionID();
  const reportKey = `${state}\0${sessionID ?? ""}`;
  if (state !== "idle") {
    cancelPendingIdle();
  }
  if (reportKey === lastReportedKey || reportKey === pendingIdleKey) {
    return;
  }

  if (state === "idle") {
    cancelPendingIdle();
    pendingIdleKey = reportKey;
    pendingIdleTimer = setTimeout(() => {
      pendingIdleTimer = undefined;
      pendingIdleKey = undefined;
      if (visibleState() !== "idle" || visibleSessionID() !== sessionID) {
        return;
      }
      void emitAgentState(state, sessionID, reportKey);
    }, IDLE_REPORT_DELAY_MS);
    return;
  }

  await emitAgentState(state, sessionID, reportKey);
}
async function markSessionWorking(sessionID) {
  if (!sessionID) {
    return;
  }

  rememberSession(sessionID, { status: { type: "busy" } });
  reconcileKnownSessionState();

  if (isPrimarySession(sessionID)) {
    await reportSession(sessionID);
    primaryBusy = true;
  } else if (isChildOfPrimary(sessionID)) {
    busyChildren.add(sessionID);
  }

  await recomputeAgentState();
}


async function handleEvent(event) {
  const type = event?.type;
  const properties = event?.properties ?? {};
  const sessionID = sessionIDFromProperties(properties);

  if (
    type === "session.created" ||
    type === "session.updated" ||
    type === "session.status" ||
    type === "session.idle" ||
    parentIDFromProperties(properties)
  ) {
    rememberSession(
      sessionID,
      type === "session.idle" ? { ...properties, status: { type: "idle" } } : properties,
      type === "session.created" || type === "session.updated",
    );
    reconcileKnownSessionState();
  }

  const primarySession = isPrimarySession(sessionID);
  const childSession = isChildOfPrimary(sessionID);

  switch (type) {
    case "gardn.session.compacting":
      await markSessionWorking(sessionID);
      break;
    case "session.created":
    case "session.updated":
      // Creation is server-global, so an attached client may own it. The
      // TUI plugin separately reports the root selected in this pane.
      if (primarySession) {
        await reportSession(sessionID);
      }
      break;
    case "session.status": {
      const state = stateFromSessionStatus(properties.status);
      if (!state) {
        break;
      }

      if (primarySession) {
        await reportSession(sessionID);
        primaryBusy = state === "working";
        await recomputeAgentState();
      } else if (childSession) {
        if (state === "working") {
          busyChildren.add(sessionID);
        } else {
          busyChildren.delete(sessionID);
          backgroundChildren.delete(sessionID);
        }
        await recomputeAgentState();
      }
      break;
    }
    case "message.part.updated": {
      if (!primarySession) {
        break;
      }

      const status = taskToolStatus(properties);
      if (status) {
        const key = taskKey(properties);
        const childSessionID = childSessionIDFromTask(properties);
        if (childSessionID) {
          rememberSession(childSessionID, { info: { parentID: sessionID } });
        }

        if (status === "pending" || status === "running") {
          if (key) {
            activeTasks.add(key);
          } else if (status === "pending") {
            anonymousActiveTasks += 1;
          } else if (anonymousActiveTasks === 0) {
            anonymousActiveTasks = 1;
          }
          if (childSessionID && taskIsBackground(properties)) {
            backgroundChildren.add(childSessionID);
          }
        } else if (status === "completed") {
          if (key) {
            activeTasks.delete(key);
          } else if (anonymousActiveTasks > 0) {
            anonymousActiveTasks -= 1;
          }
          if (childSessionID && taskIsBackground(properties)) {
            backgroundChildren.add(childSessionID);
          }
        } else if (status === "error") {
          if (key) {
            activeTasks.delete(key);
          } else if (anonymousActiveTasks > 0) {
            anonymousActiveTasks -= 1;
          }
          if (childSessionID) {
            backgroundChildren.delete(childSessionID);
          }
        }
        await recomputeAgentState();
      }

      if (stopStepFinished(properties)) {
        primaryBusy = false;
        await recomputeAgentState();
      }
      break;
    }
    case "message.updated":
      if (primarySession && idleAssistantMessage(properties)) {
        primaryBusy = false;
        await recomputeAgentState();
      }
      break;
    case "session.idle":
      if (primarySession) {
        primaryBusy = false;
        await recomputeAgentState();
      } else if (childSession) {
        busyChildren.delete(sessionID);
        backgroundChildren.delete(sessionID);
        await recomputeAgentState();
      }
      break;
    case "permission.asked":
      if (primarySession || childSession) {
        incrementPermission(sessionID, properties);
        await recomputeAgentState();
      }
      break;
    case "permission.replied":
      if (primarySession || childSession) {
        decrementPermission(sessionID, properties);
        await recomputeAgentState();
      }
      break;
    default:
      break;
  }
}

let eventQueue = Promise.resolve();

function queueEvent(event) {
  eventQueue = eventQueue.then(
    () => handleEvent(event),
    () => handleEvent(event),
  );
  return eventQueue;
}

export const GardnAgentStatePlugin = async () => {
  if (
    process.env.GARDN_ENV !== "1" ||
    !process.env.GARDN_SOCKET_PATH ||
    !process.env.GARDN_PANE_ID
  ) {
    return {};
  }

  return {
    event: async ({ event }) => queueEvent(event),
    "experimental.session.compacting": async (input) =>
      queueEvent({ type: "gardn.session.compacting", properties: { sessionID: input?.sessionID } }),
  };
};
