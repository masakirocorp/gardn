#!/usr/bin/env node
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { completeOpenRouterTurn } from "./amp-openrouter.mjs";

const messages = [
  { role: "system", content: "Answer with the requested marker." },
  { role: "user", content: "Return the success marker." },
];

function wait(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function startServer(handler) {
  const server = createServer((request, response) => {
    void handler(request, response);
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  return {
    baseUrl: `http://127.0.0.1:${address.port}/v1`,
    close: () =>
      new Promise((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      }),
  };
}

async function writeChunks(response, chunks) {
  for (const chunk of chunks) {
    if (response.destroyed) return;
    response.write(chunk);
    await wait(1);
  }
  response.end();
}

function sendSse(response, payload) {
  response.writeHead(200, {
    "cache-control": "no-cache",
    "content-type": "text/event-stream; charset=utf-8",
  });
  return writeChunks(response, payload);
}

function event(payload, lineEnding = "\r\n") {
  return `data: ${JSON.stringify(payload)}${lineEnding}${lineEnding}`;
}

test("returns assistant text from fragmented UTF-8 SSE events", async () => {
  const server = await startServer(async (_request, response) => {
    const start = event({ choices: [{ delta: { role: "assistant", content: null }, finish_reason: null }] });
    const first = event({ choices: [{ delta: { content: "GARDN_" }, finish_reason: null }] });
    const second = event({ choices: [{ delta: { content: "𐐀_OK" }, finish_reason: null }] });
    const finish = event({ choices: [{ delta: {}, finish_reason: "stop" }] });
    const usage = event({ choices: [], usage: { completion_tokens: 2 } });
    const complete = `${start}${first}: keep-alive\r\n\r\n${second}${finish}${usage}data: [DONE]\r\n\r\n`;
    const bytes = Buffer.from(complete, "utf8");
    const symbolStart = bytes.indexOf(Buffer.from("𐐀", "utf8"));
    await sendSse(response, [bytes.subarray(0, symbolStart + 1), bytes.subarray(symbolStart + 1, symbolStart + 2), bytes.subarray(symbolStart + 2)]);
  });

  try {
    const result = await completeOpenRouterTurn({
      messages,
      model: "openrouter/free",
      apiKey: "test-key",
      baseUrl: server.baseUrl,
    });
    assert.equal(result, "GARDN_𐐀_OK");
  } finally {
    await server.close();
  }
});

test("rejects non-successful provider responses without exposing response data", async () => {
  const server = await startServer((_request, response) => {
    response.writeHead(429, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: { message: "private provider payload" } }));
  });

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      (error) => {
        assert.match(error.message, /OpenRouter request failed \(HTTP 429\)/);
        assert.doesNotMatch(error.message, /private provider payload|test-key/);
        return true;
      },
    );
  } finally {
    await server.close();
  }
});

test("rejects provider errors delivered in an HTTP-successful stream", async () => {
  const server = await startServer((_request, response) =>
    sendSse(response, ['data: {"error":{"message":"private stream payload"}}\n\n']),
  );

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      (error) => {
        assert.match(error.message, /provider error/);
        assert.doesNotMatch(error.message, /private stream payload|test-key/);
        return true;
      },
    );
  } finally {
    await server.close();
  }
});

test("rejects a stream that ends before the completion marker", async () => {
  const server = await startServer((_request, response) =>
    sendSse(response, [event({ choices: [{ delta: { content: "GARDN_PARTIAL" }, finish_reason: "stop" }] }, "\n")]),
  );

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      /OpenRouter stream ended before completion marker|OpenRouter stream was truncated/,
    );
  } finally {
    await server.close();
  }
});

test("rejects an empty assistant completion", async () => {
  const server = await startServer((_request, response) =>
    sendSse(response, [event({ choices: [{ delta: {}, finish_reason: "stop" }] }), "data: [DONE]\n\n"]),
  );

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      /assistant completion is empty/,
    );
  } finally {
    await server.close();
  }
});

test("rejects tool calls instead of pretending a text-only turn completed", async () => {
  const server = await startServer((_request, response) =>
    sendSse(response, [
      event({
        choices: [
          {
            delta: { tool_calls: [{ index: 0, function: { name: "lookup", arguments: "{}" } }] },
            finish_reason: null,
          },
        ],
      }),
      "data: [DONE]\n\n",
    ]),
  );

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      /tool-call completion is unsupported/,
    );
  } finally {
    await server.close();
  }
});

test("rejects token-limited text instead of accepting a partial answer", async () => {
  const server = await startServer((_request, response) =>
    sendSse(response, [
      event({ choices: [{ delta: { content: "GARDN_PARTIAL" }, finish_reason: "length" }] }),
      "data: [DONE]\n\n",
    ]),
  );
  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      /did not finish with a complete assistant response/,
    );
  } finally {
    await server.close();
  }
});

test("rejects malformed provider events", async () => {
  const server = await startServer((_request, response) =>
    sendSse(response, ["data: {invalid\n\n", "data: [DONE]\n\n"]),
  );
  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl }),
      /malformed JSON/,
    );
  } finally {
    await server.close();
  }
});

test("cancels an unfinished provider response", { timeout: 5_000 }, async () => {
  const streaming = Promise.withResolvers();
  const server = await startServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.write(event({ choices: [{ delta: { content: "partial" }, finish_reason: null }] }));
    streaming.resolve();
  });
  const controller = new AbortController();
  try {
    const completion = completeOpenRouterTurn({
      messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl,
      signal: controller.signal,
    });
    await streaming.promise;
    controller.abort();
    await assert.rejects(completion, /request was cancelled/);
  } finally {
    controller.abort();
    await server.close();
  }
});

test("times out a provider that never completes its response", { timeout: 5_000 }, async () => {
  const server = await startServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.flushHeaders();
  });
  try {
    await assert.rejects(
      completeOpenRouterTurn({
        messages, model: "openrouter/free", apiKey: "test-key", baseUrl: server.baseUrl, timeoutMs: 100,
      }),
      /request timed out/,
    );
  } finally {
    await server.close();
  }
});
