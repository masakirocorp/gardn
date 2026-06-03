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

function reportState(action, sessionID) {
  const paneId = process.env.HAKO_PANE_ID;
  const socketPath = process.env.HAKO_SOCKET_PATH;

  if (!paneId || !socketPath) {
    return Promise.resolve();
  }

  const requestId = `${SOURCE}:${Date.now()}:${Math.floor(Math.random() * 1_000_000)
    .toString()
    .padStart(6, "0")}`;
  const params =
    action === "release"
      ? {
          pane_id: paneId,
          source: SOURCE,
          agent: "opencode",
          seq: nextReportSeq(),
          ...(sessionID ? { agent_session_id: sessionID } : {}),
        }
      : {
          pane_id: paneId,
          source: SOURCE,
          agent: "opencode",
          state: action,
          seq: nextReportSeq(),
          ...(sessionID ? { agent_session_id: sessionID } : {}),
        };
  const request = {
    id: requestId,
    method: action === "release" ? "pane.release_agent" : "pane.report_agent",
    params,
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
    dispose: async () => {
      await reportState("release");
    },
    event: async ({ event }) => {
      const type = event?.type;
      const properties = event?.properties ?? {};
      const sessionID = sessionIDFromProperties(properties);

      switch (type) {
        case "permission.asked":
        case "question.asked":
          await reportState("blocked", sessionID);
          break;
        case "permission.replied": {
          const reply = properties.reply ?? properties.response;
          if (reply === "reject") {
            await reportState("idle", sessionID);
          } else if (reply === "once" || reply === "always") {
            await reportState("working", sessionID);
          }
          break;
        }
        case "question.replied":
          await reportState("working", sessionID);
          break;
        case "question.rejected":
          await reportState("idle", sessionID);
          break;
        case "session.created":
        case "session.updated":
          // Metadata events only; lifecycle state comes from session.status and
          // the deprecated session.idle event.
          break;
        case "session.status": {
          const status =
            typeof properties.status === "string"
              ? properties.status
              : properties.status?.type;
          if (status === "busy" || status === "retry") {
            await reportState("working", sessionID);
          } else if (status === "idle") {
            await reportState("idle", sessionID);
          }
          break;
        }
        case "session.idle":
          await reportState("idle", sessionID);
          break;
        default:
          break;
      }
    },
  };
};
