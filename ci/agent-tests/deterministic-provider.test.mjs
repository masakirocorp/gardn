#!/usr/bin/env node
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const tempDirectory = await mkdtemp(join(tmpdir(), "gardn-provider-test-"));
const logFile = join(tempDirectory, "requests.ndjson");
const child = spawn(process.execPath, [fileURLToPath(new URL("./deterministic-provider.mjs", import.meta.url))], {
  env: { ...process.env, GARDN_PROVIDER_PORT: "0", GARDN_PROVIDER_LOG: logFile },
  stdio: ["ignore", "pipe", "inherit"],
});
const [firstChunk] = await once(child.stdout, "data");
const ready = JSON.parse(firstChunk.toString("utf8").trim());
const baseUrl = `http://${ready.host}:${ready.port}`;

async function post(path, body, headers = {}) {
  return fetch(`${baseUrl}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json", ...headers },
    body: JSON.stringify(body),
  });
}

async function sseData(response) {
  const text = await response.text();
  return text.split(/\r?\n/).filter((line) => line.startsWith("data: ")).map((line) => line.slice(6));
}

test("serves health and deterministic OpenAI completion", async () => {
  assert.deepEqual(await (await fetch(`${baseUrl}/health`)).json(), { ok: true });
  const response = await post("/v1/chat/completions", {
    model: "gardn-text",
    messages: [{ role: "user", content: "hello" }],
  });
  assert.equal(response.status, 200);
  const body = await response.json();
  assert.equal(body.choices[0].message.content, "GARDN_PROVIDER_OK");
  assert.equal(body.choices[0].finish_reason, "stop");
});

test("streams OpenAI chunks and terminal sentinel", async () => {
  const response = await post("/v1/chat/completions", {
    model: "gardn-text",
    stream: true,
    messages: [{ role: "user", content: "hello" }],
  });
  assert.match(response.headers.get("content-type"), /^text\/event-stream/);
  const events = await sseData(response);
  assert.equal(events.at(-1), "[DONE]");
  assert.equal(JSON.parse(events[1]).choices[0].delta.content, "GARDN_PROVIDER_OK");
  assert.equal(JSON.parse(events[2]).choices[0].finish_reason, "stop");
});

test("drives an OpenAI tool call followed by deterministic completion", async () => {
  const request = {
    model: "gardn-tool",
    messages: [{ role: "user", content: "use the tool" }],
    tools: [{ type: "function", function: { name: "lookup", parameters: { type: "object" } } }],
  };
  const firstBody = await (await post("/v1/chat/completions", request)).json();
  assert.equal(firstBody.choices[0].finish_reason, "tool_calls");
  assert.equal(firstBody.choices[0].message.tool_calls[0].function.name, "lookup");
  request.messages.push(firstBody.choices[0].message, {
    role: "tool",
    tool_call_id: "call_gardn_001",
    content: "result",
  });
  const secondBody = await (await post("/v1/chat/completions", request)).json();
  assert.equal(secondBody.choices[0].message.content, "GARDN_TOOL_COMPLETE");
});

test("provides retryable and permanent OpenAI errors", async () => {
  const retryBody = { model: "gardn-retry-429", messages: [{ role: "user", content: "retry me" }] };
  const first = await post("/v1/chat/completions", retryBody);
  assert.equal(first.status, 429);
  assert.equal((await first.json()).error.type, "rate_limit_exceeded");
  assert.equal((await post("/v1/chat/completions", retryBody)).status, 200);
  const error = await post("/v1/chat/completions", {
    model: "gardn-error-500",
    messages: [{ role: "user", content: "fail" }],
  });
  assert.equal(error.status, 500);
  assert.equal((await error.json()).error.code, "server_error");
});

test("serves Gemini completion, stream, and tool sequence", async () => {
  const completion = await post("/v1beta/models/gardn-text:generateContent?key=secret", {
    contents: [{ role: "user", parts: [{ text: "hello" }] }],
  });
  assert.equal((await completion.json()).candidates[0].content.parts[0].text, "GARDN_PROVIDER_OK");
  const stream = await post("/v1beta/models/gardn-text:streamGenerateContent?alt=sse&key=secret", {
    contents: [{ role: "user", parts: [{ text: "hello" }] }],
  });
  assert.equal(JSON.parse((await sseData(stream))[0]).candidates[0].content.parts[0].text, "GARDN_PROVIDER_OK");
  const request = {
    contents: [{ role: "user", parts: [{ text: "use the tool" }] }],
    tools: [{ functionDeclarations: [{ name: "lookup", parameters: { type: "object" } }] }],
  };
  const firstBody = await (await post("/v1beta/models/gardn-tool:generateContent", request)).json();
  assert.equal(firstBody.candidates[0].content.parts[0].functionCall.name, "lookup");
  request.contents.push(firstBody.candidates[0].content, {
    role: "user",
    parts: [{ functionResponse: { name: "lookup", response: { value: "result" } } }],
  });
  const secondBody = await (await post("/v1beta/models/gardn-tool:generateContent", request)).json();
  assert.equal(secondBody.candidates[0].content.parts[0].text, "GARDN_TOOL_COMPLETE");
});

test("logs requests without credentials", async () => {
  await post(
    "/v1/chat/completions?trace=one",
    { model: "gardn-text", messages: [{ role: "user", content: "logged" }] },
    { authorization: "Bearer must-not-log" },
  );
  const entries = (await (await fetch(`${baseUrl}/__gardn/requests`)).json()).requests;
  assert.equal(entries.at(-1).query.trace, "one");
  assert.equal(entries.at(-1).headers.authorization, undefined);
  const persisted = await readFile(logFile, "utf8");
  assert.doesNotMatch(persisted, /must-not-log/);
  assert.match(persisted, /"content":"logged"/);
});

test.after(async () => {
  child.kill("SIGTERM");
  await once(child, "exit");
  await rm(tempDirectory, { recursive: true, force: true });
});
