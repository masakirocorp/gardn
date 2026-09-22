import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

export type TestEndpointMode = "marker" | "direct";

export type TestEndpoint = {
  value: string;
  listenEndpoint: string;
  cleanup(): Promise<void>;
};

export async function createTestEndpoint(
  prefix: string,
  mode: TestEndpointMode = "marker",
): Promise<TestEndpoint> {
  if (process.platform === "win32") {
    const marker = `${prefix}-${process.pid}-${crypto.randomUUID()}`;
    const listenEndpoint = `\\\\.\\pipe\\${marker}`;
    return {
      value: mode === "marker" ? marker : listenEndpoint,
      listenEndpoint,
      cleanup: () => Promise.resolve(),
    };
  }

  const directory = await mkdtemp(join(tmpdir(), `${prefix}-`));
  const endpoint = join(directory, "gardn.sock");
  return {
    value: endpoint,
    listenEndpoint: endpoint,
    async cleanup() {
      await rm(directory, { recursive: true, force: true });
    },
  };
}
