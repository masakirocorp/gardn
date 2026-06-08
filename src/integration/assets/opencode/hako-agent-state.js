// installed by hako
// managed by hako; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// HAKO_INTEGRATION_ID=opencode
// HAKO_INTEGRATION_VERSION=1

import net from "node:net";

const SOURCE = "hako:opencode";
let reportSeq = Date.now() * 1000;

function nextReportSeq() {
  reportSeq = Math.max(reportSeq + 1, Date.now() * 1000);
  return reportSeq;
}

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

function reportSession(sessionID) {
  if (!sessionID) {
    return Promise.resolve();
  }

  const paneId = process.env.HAKO_PANE_ID;
  const socketPath = process.env.HAKO_SOCKET_PATH;

  if (!paneId || !socketPath) {
    return Promise.resolve();
  }

  const requestId = `${SOURCE}:${Date.now()}:${Math.floor(Math.random() * 1_000_000)
    .toString()
    .padStart(6, "0")}`;
  const request = {
    id: requestId,
    method: "pane.report_agent_session",
    params: {
      pane_id: paneId,
      source: SOURCE,
      agent: "opencode",
      seq: nextReportSeq(),
      agent_session_id: sessionID,
      launch_env: launchEnv(),
    },
  };

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

      switch (type) {
        case "session.created":
        case "session.updated":
        case "session.status":
          await reportSession(sessionID);
          break;
        default:
          break;
      }
    },
  };
};
