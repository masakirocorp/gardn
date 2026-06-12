// installed by hako
// managed by hako; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// HAKO_INTEGRATION_ID=opencode
// HAKO_INTEGRATION_VERSION=3

import net from "node:net";

const SOURCE = "hako:opencode";
let reportSeq = Date.now() * 1000;

function nextReportSeq() {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
}

const sessions = new Map();
const activeChildren = new Set();
const activeTasks = new Set();
const pendingPermissions = new Set();
let primarySessionID;
let primaryBusy = false;

function sessionIDFromProperties(properties) {
  return typeof properties?.sessionID === "string" && properties.sessionID
    ? properties.sessionID
    : undefined;
}

function parentIDFromProperties(properties) {
  const parentID = properties?.info?.parentID ?? properties?.info?.parentId;
  return typeof parentID === "string" && parentID ? parentID : undefined;
}

function rememberSession(sessionID, properties) {
  if (!sessionID) {
    return;
  }

  const current = sessions.get(sessionID) ?? {};
  const parentID = parentIDFromProperties(properties) ?? current.parentID;
  sessions.set(sessionID, { parentID });

  if (!primarySessionID && !parentID) {
    primarySessionID = sessionID;
  }
}

function isPrimarySession(sessionID) {
  return Boolean(sessionID && primarySessionID && sessionID === primarySessionID);
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
  if (pendingPermissions.size > 0) {
    return "blocked";
  }
  if (primaryBusy || activeTasks.size > 0 || activeChildren.size > 0) {
    return "working";
  }
  return "idle";
}

function taskKey(properties) {
  const part = properties?.part;
  const id = part?.id ?? part?.callID ?? part?.callId ?? part?.toolCallID ?? part?.toolCallId;
  if (typeof id === "string" && id) {
    return id;
  }
  return "task";
}

function permissionKey(sessionID, properties) {
  const id = properties?.id ?? properties?.permissionID ?? properties?.permissionId;
  const suffix = typeof id === "string" && id ? id : "permission";
  return `${sessionID ?? "unknown"}:${suffix}`;
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
  const paneId = process.env.HAKO_PANE_ID;

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
  const socketPath = process.env.HAKO_SOCKET_PATH;

  if (!process.env.HAKO_PANE_ID || !socketPath) {
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    const client = net.createConnection(socketPath, () => {
      client.write(`${JSON.stringify(request)}\n`);
    });

    const finish = () => {
      client.destroy();
      resolve();
    };

    client.setTimeout(500, finish);
    client.on("data", finish);
    client.on("error", finish);
    client.on("end", finish);
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

function taskStillBackgrounded(properties) {
  const metadata = properties?.part?.state?.metadata;
  return metadata?.background === true || metadata?.state === "running";
}

async function recomputeAgentState() {
  await reportAgent(visibleState(), visibleSessionID());
}
let eventQueue = Promise.resolve();

async function handleEvent(event) {
  const type = event?.type;
  const properties = event?.properties ?? {};
  const sessionID = sessionIDFromProperties(properties);

  if (type === "session.created" || type === "session.updated" || parentIDFromProperties(properties)) {
    rememberSession(sessionID, properties);
  }

  const primarySession = isPrimarySession(sessionID);
  const childSession = isChildOfPrimary(sessionID);

  switch (type) {
    case "session.created":
    case "session.updated":
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
          activeChildren.add(sessionID);
        } else {
          activeChildren.delete(sessionID);
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
          activeTasks.add(key);
          if (childSessionID) {
            activeChildren.add(childSessionID);
          }
        } else if (status === "completed") {
          activeTasks.delete(key);
          if (childSessionID && !taskStillBackgrounded(properties)) {
            activeChildren.delete(childSessionID);
          }
          if (childSessionID && taskStillBackgrounded(properties)) {
            activeChildren.add(childSessionID);
          }
        } else if (status === "error") {
          activeTasks.delete(key);
          if (childSessionID) {
            activeChildren.delete(childSessionID);
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
        activeChildren.delete(sessionID);
        await recomputeAgentState();
      }
      break;
    case "permission.asked":
      if (primarySession || childSession) {
        pendingPermissions.add(permissionKey(sessionID, properties));
        await recomputeAgentState();
      }
      break;
    case "permission.replied":
      if (primarySession || childSession) {
        pendingPermissions.delete(permissionKey(sessionID, properties));
        await recomputeAgentState();
      }
      break;
    default:
      break;
  }
}

function queueEvent(event) {
  eventQueue = eventQueue.then(
    () => handleEvent(event),
    () => handleEvent(event),
  );
  return eventQueue;
}

export const HakoAgentStatePlugin = async () => {
  if (
    process.env.HAKO_ENV !== "1" ||
    !process.env.HAKO_SOCKET_PATH ||
    !process.env.HAKO_PANE_ID
  ) {
    return {};
  }

  return {
    event: async ({ event }) => queueEvent(event),
  };
};
