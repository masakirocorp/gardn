// installed by hako
// managed by hako; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// HAKO_INTEGRATION_ID=opencode
// HAKO_INTEGRATION_VERSION=2

import net from "node:net";

const SOURCE = "hako:opencode";
let reportSeq = Date.now() * 1000;

function nextReportSeq() {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
}

let primarySessionID;

function sessionIDFromProperties(properties) {
  return typeof properties?.sessionID === "string" && properties.sessionID
    ? properties.sessionID
    : undefined;
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

function taskToolIsActive(properties) {
  const part = properties?.part;
  return (
    part?.type === "tool" &&
    part?.tool === "task" &&
    ["pending", "running", "completed"].includes(part?.state?.status)
  );
}

function stopStepFinished(properties) {
  const part = properties?.part;
  return part?.type === "step-finish" && part?.reason === "stop";
}

function shouldHandlePrimarySession(sessionID) {
  if (!sessionID) {
    return true;
  }
  if (!primarySessionID) {
    primarySessionID = sessionID;
  }
  return sessionID === primarySessionID;
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
    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties ?? {};
      const sessionID = sessionIDFromProperties(properties);
      const primarySession = shouldHandlePrimarySession(sessionID);

      switch (type) {
        case "session.created":
        case "session.updated":
          if (primarySession) {
            await reportSession(sessionID);
          }
          break;
        case "session.status": {
          if (!primarySession) {
            break;
          }
          await reportSession(sessionID);
          const state = stateFromSessionStatus(properties.status);
          if (state) {
            await reportAgent(state, sessionID);
          }
          break;
        }
        case "message.part.updated":
          if (primarySession && taskToolIsActive(properties)) {
            await reportAgent("working", sessionID);
          }
          if (primarySession && stopStepFinished(properties)) {
            await reportAgent("idle", sessionID);
          }
          break;
        case "message.updated":
          if (primarySession && idleAssistantMessage(properties)) {
            await reportAgent("idle", sessionID);
          }
          break;
        case "session.idle":
          if (primarySession) {
            await reportAgent("idle", sessionID);
          }
          break;
        case "permission.asked":
          if (primarySession) {
            await reportAgent("blocked", sessionID);
          }
          break;
        case "permission.replied":
          if (primarySession) {
            await reportAgent(properties.reply === "reject" ? "idle" : "working", sessionID);
          }
          break;
        default:
          break;
      }
    },
  };
};
