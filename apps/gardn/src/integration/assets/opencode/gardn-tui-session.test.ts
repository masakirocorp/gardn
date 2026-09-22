import { expect, test } from "bun:test";
import net from "node:net";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import plugin from "./gardn-tui-session.js";

type Request = {
  method: string;
  params: {
    pane_id: string;
    source: string;
    agent: string;
    agent_session_id: string;
    session_start_source: string;
    seq?: number;
  };
};

function integrationEnvironment(socketPath: string) {
  const values = {
    GARDN_ENV: "1",
    GARDN_SOCKET_PATH: socketPath,
    GARDN_PANE_ID: "test:p1",
  };
  const saved = Object.fromEntries(Object.keys(values).map((key) => [key, process.env[key]]));
  Object.assign(process.env, values);
  return {
    [Symbol.dispose]() {
      for (const [key, value] of Object.entries(saved)) {
        if (value === undefined) delete process.env[key];
        else process.env[key] = value;
      }
    },
  };
}

async function recordingSocket() {
  const directory = await mkdtemp(join(tmpdir(), "gardn-tui-"));
  const path = join(directory, "gardn.sock");
  const requests: Request[] = [];
  const listeners = new Set<() => void>();
  let readIndex = 0;
  const server = net.createServer((socket) => {
    let buffered = "";
    socket.on("data", (chunk) => {
      buffered += chunk.toString();
      const lines = buffered.split("\n");
      buffered = lines.pop()!;
      for (const line of lines) {
        requests.push(JSON.parse(line));
        socket.end("{}\n");
        for (const notify of listeners) notify();
      }
    });
  });
  const listening = Promise.withResolvers<void>();
  server.once("error", listening.reject);
  server.listen(path, listening.resolve);
  await listening.promise;
  return {
    path,
    requests,
    nextRequest(): Promise<Request> {
      const index = readIndex++;
      const { promise, resolve } = Promise.withResolvers<Request>();
      const check = () => {
        const request = requests[index];
        if (request) {
          listeners.delete(check);
          resolve(request);
        }
      };
      listeners.add(check);
      check();
      return promise;
    },
    async [Symbol.asyncDispose]() {
      const closing = Promise.withResolvers<void>();
      server.close((error) => (error ? closing.reject(error) : closing.resolve()));
      await closing.promise;
      await rm(directory, { recursive: true, force: true });
    },
  };
}

function fakeApi() {
  const sessions = new Map<string, { id: string; parentID?: string }>();
  let current: { name: string; params?: { sessionID: string } } = { name: "home" };
  let dispose: (() => void) | undefined;

  return {
    api: {
      route: {
        get current() {
          return current;
        },
      },
      state: {
        session: {
          get(sessionID: string) {
            return sessions.get(sessionID);
          },
        },
      },
      lifecycle: {
        onDispose(handler: () => void) {
          dispose = handler;
          return () => {};
        },
      },
    },
    addSession(session: { id: string; parentID?: string }) {
      sessions.set(session.id, session);
    },
    select(sessionID: string) {
      current = { name: "session", params: { sessionID } };
    },
    dispose() {
      dispose?.();
    },
    [Symbol.dispose]() {
      dispose?.();
    },
  };
}

test.serial("reports a root session when only the local route changes", async () => {
  await using socket = await recordingSocket();
  using env = integrationEnvironment(socket.path);
  using tui = fakeApi();
  tui.addSession({ id: "session-a" });
  await plugin.tui(tui.api);

  tui.select("session-a");
  const request = await socket.nextRequest();

  expect(request).toMatchObject({
    method: "pane.report_agent_session",
    params: {
      pane_id: "test:p1",
      source: "gardn:opencode",
      agent: "opencode",
      agent_session_id: "session-a",
      session_start_source: "select",
    },
  });
  expect(request.params.seq).toBeUndefined();
});

test.serial("retries an initial selection while Gardn detects the process", async () => {
  await using socket = await recordingSocket();
  using env = integrationEnvironment(socket.path);
  using tui = fakeApi();
  tui.addSession({ id: "session-a" });
  tui.select("session-a");

  await plugin.tui(tui.api);
  expect((await socket.nextRequest()).params.agent_session_id).toBe("session-a");
  expect((await socket.nextRequest()).params.agent_session_id).toBe("session-a");
});

test.serial("reports only the root session selected by this TUI", async () => {
  await using socket = await recordingSocket();
  using env = integrationEnvironment(socket.path);
  using tui = fakeApi();
  tui.addSession({ id: "session-a" });
  tui.addSession({ id: "session-b" });
  tui.addSession({ id: "unselected-session" });
  tui.select("session-a");
  await plugin.tui(tui.api);
  expect((await socket.nextRequest()).params.agent_session_id).toBe("session-a");

  tui.select("session-b");
  expect((await socket.nextRequest()).params.agent_session_id).toBe("session-b");
  expect(socket.requests.map((request) => request.params.agent_session_id)).toEqual([
    "session-a",
    "session-b",
  ]);
});

test.serial("does not replace the root session with a selected child session", async () => {
  await using socket = await recordingSocket();
  using env = integrationEnvironment(socket.path);
  using tui = fakeApi();
  tui.addSession({ id: "root-session" });
  tui.addSession({ id: "child-session", parentID: "root-session" });
  tui.select("root-session");
  await plugin.tui(tui.api);
  await socket.nextRequest();

  tui.select("child-session");
  await Bun.sleep(250);

  expect(socket.requests.map((request) => request.params.agent_session_id)).toEqual(["root-session"]);
});

test.serial("stops route polling when the TUI plugin is disposed", async () => {
  await using socket = await recordingSocket();
  using env = integrationEnvironment(socket.path);
  using tui = fakeApi();
  tui.addSession({ id: "session-a" });
  await plugin.tui(tui.api);
  tui.dispose();
  tui.select("session-a");

  await Bun.sleep(250);

  expect(socket.requests).toEqual([]);
});
