import assert from "node:assert/strict";
import { chmod, mkdtemp, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const generator = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "generate-api-reference.mjs",
);

/** @returns {Record<string, any>} */
function validSchema() {
  return {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    product_version: "1.2.3",
    protocol: 7,
    schema_version: 1,
    schemas: {
      error_response: {
        title: "ErrorResponse",
        type: "object",
        properties: {
          id: { type: "string" },
          error: { $ref: "#/$defs/ErrorBody" },
        },
        required: ["id", "error"],
        $defs: {
          ErrorBody: {
            type: "object",
            properties: { code: { type: "string" }, message: { type: "string" } },
            required: ["code", "message"],
          },
        },
      },
      event: {
        title: "EventEnvelope",
        $defs: {
          EventData: {
            oneOf: [
              { $ref: "#/$defs/WorkspaceCreated" },
              {
                type: "object",
                properties: {
                  type: { const: "workspace_closed" },
                  workspace_id: { type: "string" },
                },
                required: ["type", "workspace_id"],
              },
            ],
          },
          WorkspaceCreated: {
            type: "object",
            properties: { type: { const: "workspace_created" }, workspace_id: { type: "string" } },
            required: ["type", "workspace_id"],
          },
        },
      },
      request: {
        title: "Request",
        oneOf: [
          {
            type: "object",
            properties: {
              id: { type: "string" },
              method: { const: "ping", type: "string" },
              params: { $ref: "#/$defs/PingParams" },
            },
            required: ["id", "method", "params"],
          },
        ],
        $defs: { PingParams: { type: "object", properties: {} } },
      },
      response: {
        title: "SuccessResponse",
        $defs: {
          ResponseResult: {
            oneOf: [
              { $ref: "#/$defs/Pong" },
              {
                type: "object",
                properties: {
                  type: { const: "ok" },
                  changed: { type: "boolean" },
                },
                required: ["type", "changed"],
              },
            ],
          },
          Pong: {
            type: "object",
            properties: {
              type: { const: "pong" },
              version: { type: "string" },
              capabilities: { $ref: "#/$defs/ServerCapabilities" },
            },
            required: ["type", "version", "capabilities"],
          },
          ServerCapabilities: {
            type: "object",
            properties: { live_handoff: { type: "boolean" } },
            required: ["live_handoff"],
          },
        },
      },
      subscription_event: {
        title: "SubscriptionEventEnvelope",
        $defs: {
          SubscriptionEventData: { anyOf: [{ $ref: "#/$defs/PaneOutputMatched" }] },
          PaneOutputMatched: {
            type: "object",
            properties: { pane_id: { type: "string" } },
            required: ["pane_id"],
          },
        },
      },
    },
  };
}

/**
 * @param {string} directory
 * @param {Record<string, any>} schema
 */
async function fakeBinary(directory, schema) {
  const schemaPath = path.join(directory, "binary-schema.json");
  const binaryPath = path.join(directory, "omh-fixture");
  await writeFile(schemaPath, `${JSON.stringify(schema)}\n`);
  await writeFile(binaryPath, `#!/bin/sh\ncat ${JSON.stringify(schemaPath)}\n`);
  await chmod(binaryPath, 0o755);
  return binaryPath;
}

/**
 * @param {string[]} args
 * @param {NodeJS.ProcessEnv} env
 */
function run(args, env = {}) {
  return spawnSync(process.execPath, [generator, ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });
}

test("binary-driven generation writes deterministic versioned reference", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "omh-api-reference-"));
  t.after(() =>
    import("node:fs/promises").then(({ rm }) => rm(root, { recursive: true, force: true })),
  );
  const binary = await fakeBinary(root, validSchema());

  const generated = run(["--binary", binary, "--root", root]);
  assert.equal(generated.status, 0, generated.stderr);
  assert.match(generated.stdout, /generated Local API reference for Oh My Herdr 1\.2\.3/);

  const immutableSchema = await readFile(
    path.join(root, "public", "api", "1.2.3", "schema.json"),
    "utf8",
  );
  assert.equal(
    immutableSchema,
    await readFile(path.join(root, ".generated", "api", "schema.json"), "utf8"),
  );
  assert.match(
    await readFile(
      path.join(root, "content", "docs", "api", "reference", "generated", "requests.mdx"),
      "utf8",
    ),
    /`ping`/,
  );
  assert.match(
    await readFile(
      path.join(root, "content", "docs", "api", "reference", "generated", "responses.mdx"),
      "utf8",
    ),
    /## `ok`[\s\S]*`changed`[\s\S]*### `ServerCapabilities`[\s\S]*`live_handoff`/,
  );
  assert.match(
    await readFile(
      path.join(root, "content", "docs", "api", "reference", "generated", "events.mdx"),
      "utf8",
    ),
    /## `workspace_closed`[\s\S]*`workspace_id`/,
  );

  const checked = run(["--binary", binary, "--root", root, "--check"]);
  assert.equal(checked.status, 0, checked.stderr);
  assert.equal(
    await readFile(path.join(root, "public", "api", "1.2.3", "schema.json"), "utf8"),
    immutableSchema,
  );
});

test("generation refuses to replace an existing product version", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "omh-api-reference-immutable-"));
  t.after(() =>
    import("node:fs/promises").then(({ rm }) => rm(root, { recursive: true, force: true })),
  );
  const initialBinary = await fakeBinary(root, validSchema());
  const initial = run(["--binary", initialBinary, "--root", root]);
  assert.equal(initial.status, 0, initial.stderr);

  const changedSchema = validSchema();
  changedSchema.protocol = 8;
  const changedBinary = await fakeBinary(root, changedSchema);
  const changed = run(["--binary", changedBinary, "--root", root]);

  assert.equal(changed.status, 1);
  assert.match(changed.stderr, /bump the Oh My Herdr product version/);
});

test("generation rejects request schemas that omit correlation ids", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "omh-api-reference-invalid-"));
  t.after(() =>
    import("node:fs/promises").then(({ rm }) => rm(root, { recursive: true, force: true })),
  );
  const schema = validSchema();
  delete schema.schemas.request.oneOf[0].properties.id;
  const binary = await fakeBinary(root, schema);

  const generated = run(["--binary", binary, "--root", root]);

  assert.equal(generated.status, 1);
  assert.match(generated.stderr, /request method ping does not require a string id/);
});

test("latest alias requires the release deployment gate", async (t) => {
  const root = await mkdtemp(path.join(os.tmpdir(), "omh-api-reference-latest-"));
  t.after(() =>
    import("node:fs/promises").then(({ rm }) => rm(root, { recursive: true, force: true })),
  );
  const binary = await fakeBinary(root, validSchema());

  const rejected = run(["--binary", binary, "--root", root, "--publish-latest"]);
  assert.equal(rejected.status, 1);
  assert.match(rejected.stderr, /requires OMH_API_RELEASE=1/);

  const released = run(["--binary", binary, "--root", root, "--publish-latest"], {
    OMH_API_RELEASE: "1",
    GITHUB_REF_NAME: "v1.2.3",
  });
  assert.equal(released.status, 0, released.stderr);
  assert.equal(
    await readFile(path.join(root, "public", "api", "latest", "schema.json"), "utf8"),
    await readFile(path.join(root, "public", "api", "1.2.3", "schema.json"), "utf8"),
  );
});
