import { expect, test } from "bun:test";
import net from "node:net";
import gardnAmpAgentState from "./gardn-agent-state";
import type { PluginAPI } from "@ampcode/plugin";
import { createTestEndpoint } from "../test-endpoint";

type ThreadState = "idle" | "running" | "awaiting-approval" | "error";
type ThreadId = `T-${string}`;
type Report = {
  method: string;
  params: {
    agent_session_id: string;
    state?: string;
    message?: string;
    seq: number;
    launch_env?: Record<string, string>;
  };
};
type Observable<T> = {
  current: T;
  subscribe(callback: (value: T) => void): { unsubscribe(): void };
  emit(value: T): void;
};
type FakeThread = {
  id: ThreadId;
  state: Observable<ThreadState> & { get(): Promise<ThreadState> };
};
type FakeAmp = {
  activeThread: Observable<{ id: ThreadId } | null>;
  threads: { get(id: ThreadId): FakeThread };
  onDispose(callback: () => void | Promise<void>): { unsubscribe(): void };
  dispose(): void | Promise<void>;
};

function observable<T>(initial: T): Observable<T> {
  const listeners = new Set<(value: T) => void>();
  return {
    current: initial,
    subscribe(callback: (value: T) => void) {
      listeners.add(callback);
      return {
        unsubscribe: () => {
          listeners.delete(callback);
        },
      };
    },
    emit(value: T) {
      this.current = value;
      for (const callback of listeners) callback(value);
    },
  };
}

function thread(id: ThreadId, initial: ThreadState): FakeThread {
  const state = observable(initial);
  return { id, state: Object.assign(state, { get: () => Promise.resolve(state.current) }) };
}

function ampApi(selected: FakeThread, threads = [selected]): FakeAmp {
  let dispose: () => void | Promise<void> = () => {};
  return {
    activeThread: observable<{ id: ThreadId } | null>({ id: selected.id }),
    threads: {
      get(id: ThreadId) {
        const found = threads.find((candidate) => candidate.id === id);
        if (!found) throw new Error(`Unknown test thread ${id}`);
        return found;
      },
    },
    onDispose(callback: typeof dispose) {
      dispose = callback;
      return { unsubscribe() {} };
    },
    dispose: () => dispose(),
  };
}

function environment(values: Record<string, string>) {
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
  const endpoint = await createTestEndpoint("gardn-amp", "direct");
  const path = endpoint.value;
  const reports: Report[] = [];
  const listeners = new Set<() => void>();
  const server = net.createServer((socket) => {
    let buffered = "";
    socket.on("data", (chunk) => {
      buffered += chunk.toString();
      const lines = buffered.split("\n");
      buffered = lines.pop()!;
      for (const line of lines) {
        reports.push(JSON.parse(line));
        socket.write("{}\n");
        for (const notify of listeners) notify();
      }
    });
  });
  const listening = Promise.withResolvers<void>();
  server.once("error", listening.reject);
  server.listen(endpoint.listenEndpoint, listening.resolve);
  await listening.promise;
  return {
    path,
    reports,
    waitFor(predicate: (report: Report) => boolean): Promise<Report> {
      const { promise, resolve } = Promise.withResolvers<Report>();
      const check = () => {
        const found = reports.find(predicate);
        if (found) {
          listeners.delete(check);
          resolve(found);
        }
      };
      listeners.add(check);
      check();
      return promise;
    },
    async [Symbol.asyncDispose]() {
      const closing = Promise.withResolvers<void>();
      server.close(() => closing.resolve());
      await closing.promise;
      await endpoint.cleanup();
    },
  };
}

function install(amp: FakeAmp) {
  gardnAmpAgentState(amp as unknown as PluginAPI);
}

const status = (value: string) => (report: Report) =>
  report.method === "pane.report_agent" && report.params.state === value;

test.serial("reports selected Amp lifecycle, safe launch context, and release", async () => {
  await using socket = await recordingSocket();
  using env = environment({
    GARDN_ENV: "1",
    GARDN_SOCKET_PATH: socket.path,
    GARDN_PANE_ID: "test:pane",
    AMP_URL: "https://amp.example",
    AMP_API_KEY: "must-not-leak",
  });
  const selected = thread("T-lifecycle", "running");
  const amp = ampApi(selected);
  install(amp);
  try {
    await socket.waitFor(status("working"));
    expect(socket.reports[0]).toMatchObject({
      method: "pane.report_agent_session",
      params: {
        pane_id: "test:pane",
        source: "gardn:amp",
        agent: "amp",
        agent_session_id: "T-lifecycle",
        session_start_source: "select",
        launch_env: { AMP_URL: "https://amp.example" },
      },
    });
    expect(socket.reports[0].params.launch_env).not.toHaveProperty("AMP_API_KEY");
    selected.state.emit("awaiting-approval");
    const approval = await socket.waitFor(status("blocked"));
    expect(approval.params.message).toContain("approval");
    selected.state.emit("error");
    const error = await socket.waitFor(
      (r) => status("blocked")(r) && r.params.message?.includes("error") === true,
    );
    expect(error.params.agent_session_id).toBe("T-lifecycle");
    selected.state.emit("idle");
    await socket.waitFor(status("idle"));
    amp.activeThread.emit(null);
    const release = await socket.waitFor((r) => r.method === "pane.release_agent");
    expect(release.params.agent_session_id).toBe("T-lifecycle");
    for (let index = 1; index < socket.reports.length; index++) {
      expect(socket.reports[index].params.seq).toBeGreaterThan(
        socket.reports[index - 1].params.seq,
      );
    }
  } finally {
    await amp.dispose();
  }
});

test.serial(
  "ignores background events and stale snapshots after selecting another Amp thread",
  async () => {
    await using socket = await recordingSocket();
    using env = environment({
      GARDN_ENV: "1",
      GARDN_SOCKET_PATH: socket.path,
      GARDN_PANE_ID: "test:pane",
    });
    const first = thread("T-first", "idle");
    const snapshot = Promise.withResolvers<ThreadState>();
    first.state.get = () => snapshot.promise;
    const second = thread("T-second", "idle");
    const amp = ampApi(first, [first, second]);
    install(amp);
    try {
      await socket.waitFor((r) => r.method === "pane.report_agent_session");
      amp.activeThread.emit({ id: second.id });
      await socket.waitFor(status("idle"));
      first.state.emit("awaiting-approval");
      snapshot.resolve("running");
      await snapshot.promise;
      second.state.emit("running");
      await socket.waitFor(status("working"));
      expect(
        socket.reports
          .filter((r) => r.method === "pane.report_agent")
          .map((r) => [r.params.agent_session_id, r.params.state]),
      ).toEqual([
        ["T-second", "idle"],
        ["T-second", "working"],
      ]);
    } finally {
      await amp.dispose();
    }
    expect(socket.reports.at(-1)).toMatchObject({
      method: "pane.release_agent",
      params: { agent_session_id: "T-second" },
    });
  },
);

test.serial("does not replace a live Amp state with an older initial snapshot", async () => {
  await using socket = await recordingSocket();
  using env = environment({
    GARDN_ENV: "1",
    GARDN_SOCKET_PATH: socket.path,
    GARDN_PANE_ID: "test:pane",
  });
  const selected = thread("T-current", "idle");
  const snapshot = Promise.withResolvers<ThreadState>();
  const reading = Promise.withResolvers<void>();
  selected.state.get = () => {
    reading.resolve();
    return snapshot.promise;
  };
  const amp = ampApi(selected);
  install(amp);
  try {
    await reading.promise;
    selected.state.emit("running");
    await socket.waitFor(status("working"));
    snapshot.resolve("idle");
    await snapshot.promise;
    selected.state.emit("error");
    await socket.waitFor(status("blocked"));
    expect(
      socket.reports.filter((r) => r.method === "pane.report_agent").map((r) => r.params.state),
    ).toEqual(["working", "blocked"]);
  } finally {
    await amp.dispose();
  }
});

test.serial("releases the reported thread when selection clears during shutdown", async () => {
  await using socket = await recordingSocket();
  using env = environment({
    GARDN_ENV: "1",
    GARDN_SOCKET_PATH: socket.path,
    GARDN_PANE_ID: "test:pane",
  });
  const first = thread("T-reported", "running");
  const second = thread("T-never-reported", "idle");
  const amp = ampApi(first, [first, second]);
  install(amp);
  await socket.waitFor(status("working"));
  amp.activeThread.emit({ id: second.id });
  amp.activeThread.emit(null);
  await amp.dispose();
  expect(socket.reports.at(-1)).toMatchObject({
    method: "pane.release_agent",
    params: { agent_session_id: "T-reported" },
  });
});
