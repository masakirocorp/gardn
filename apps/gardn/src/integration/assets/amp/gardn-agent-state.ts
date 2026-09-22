// installed by Gardn
// managed by Gardn; reinstalling or updating the integration overwrites this file.
// add custom hooks/plugins beside this file instead of editing it.
// GARDN_INTEGRATION_ID=amp
// GARDN_INTEGRATION_VERSION=1

import net from "node:net";
import type { Socket } from "node:net";
import type { PluginAPI, Subscription, ThreadID, ThreadState } from "@ampcode/plugin";

function send(endpoint: string, request: unknown, timeoutMs: number): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>();
  let socket: Socket | undefined;
  const timeout = setTimeout(() => finish(false), timeoutMs);
  const finish = (delivered: boolean) => {
    clearTimeout(timeout);
    socket?.destroy();
    resolve(delivered);
  };
  try {
    socket = net.createConnection(endpoint);
    socket.once("connect", () => socket!.write(`${JSON.stringify(request)}\n`));
    socket.once("data", () => finish(true));
    socket.once("error", () => finish(false));
    socket.once("close", () => finish(false));
  } catch {
    finish(false);
  }
  return promise;
}

const states = {
  running: { state: "working" },
  "awaiting-approval": { state: "blocked", message: "waiting for Amp tool approval" },
  error: { state: "blocked", message: "Amp thread reported an error" },
  idle: { state: "idle" },
} as const;

export default function gardnAmpAgentState(amp: PluginAPI): void {
  const endpoint = process.env.GARDN_SOCKET_PATH;
  const paneId = process.env.GARDN_PANE_ID;
  if (process.env.GARDN_ENV !== "1" || !endpoint || !paneId) return;

  const launchEnv: Record<string, string> = {};
  for (const key of ["AMP_SETTINGS_FILE", "AMP_URL", "XDG_CONFIG_HOME"]) {
    const value = process.env[key];
    if (value) launchEnv[key] = value;
  }

  let disposed = false;
  let generation = 0;
  let selectedId: ThreadID | undefined;
  let reportedId: ThreadID | undefined;
  let stateSubscription: Subscription | undefined;
  let queue = Promise.resolve();
  let sequence = Date.now() * 1_000;

  const report = async (method: string, threadId: ThreadID, params = {}, retry = true) => {
    sequence = Math.max(sequence + 1, Date.now() * 1_000);
    const request = {
      id: `gardn:amp:${sequence}`,
      method,
      params: {
        pane_id: paneId,
        source: "gardn:amp",
        agent: "amp",
        agent_session_id: threadId,
        seq: sequence,
        ...params,
      },
    };
    if (!(await send(endpoint, request, 500)) && retry) {
      await send(endpoint, request, 1_500);
    }
  };

  const release = () => {
    queue = queue.then(async () => {
      if (!reportedId) return;
      // Leave room for an in-flight report within Amp's three-second shutdown grace.
      await report("pane.release_agent", reportedId, {}, false);
      reportedId = undefined;
    });
    return queue;
  };

  const select = (threadId: ThreadID | undefined) => {
    if (disposed || threadId === selectedId) return;
    selectedId = threadId;
    const selection = ++generation;
    stateSubscription?.unsubscribe();
    stateSubscription = undefined;
    if (!threadId) {
      void release();
      return;
    }

    queue = queue.then(async () => {
      if (disposed || generation !== selection) return;
      // The server can accept a report even if its acknowledgment is lost.
      reportedId = threadId;
      await report("pane.report_agent_session", threadId, {
        session_start_source: "select",
        launch_env: launchEnv,
      });
    });

    let latestState: ThreadState | undefined;
    const publish = (state: ThreadState) => {
      if (disposed || generation !== selection || state === latestState) return;
      latestState = state;
      queue = queue.then(async () => {
        if (!disposed && generation === selection) {
          await report("pane.report_agent", threadId, states[state]);
        }
      });
    };

    const thread = amp.threads.get(threadId);
    let observedLiveState = false;
    stateSubscription = thread.state.subscribe((state) => {
      observedLiveState = true;
      publish(state);
    });
    void thread.state.get().then(
      (state) => {
        // A snapshot requested before a live event must not roll that event back.
        if (!observedLiveState) publish(state);
      },
      () => undefined,
    );
  };

  const activeSubscription = amp.activeThread.subscribe((thread) => select(thread?.id));
  select(amp.activeThread.current?.id);
  amp.onDispose(() => {
    if (disposed) return queue;
    disposed = true;
    generation++;
    activeSubscription.unsubscribe();
    stateSubscription?.unsubscribe();
    return release();
  });
}
