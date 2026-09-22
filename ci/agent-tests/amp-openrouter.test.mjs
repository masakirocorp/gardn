#!/usr/bin/env node
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { completeOpenRouterTurn } from "./amp-openrouter.mjs";

const messages = [
  { role: "system", content: "Answer with the requested marker." },
  { role: "user", content: "Return the success marker." },
];

async function startServer(handler) {
  const server = createServer((request, response) => {
    void handler(request, response);
  });
  const sockets = new Set();
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  return {
    baseUrl: `http://127.0.0.1:${address.port}/v1/`,
    close: () =>
      new Promise((resolve, reject) => {
        for (const socket of sockets) socket.destroy();
        server.close((error) => {
          if (error && error.code !== "ERR_SERVER_NOT_RUNNING") reject(error);
          else resolve();
        });
      }),
  };
}

function sendJson(response, payload, status = 200) {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(typeof payload === "string" ? payload : JSON.stringify(payload));
}


function completionPayload(content = "GARDN_OK", finishReason = "stop") {
  return {
    id: "completion-test",
    choices: [{ index: 0, message: { role: "assistant", content }, finish_reason: finishReason }],
  };
}

test("returns completed assistant text without altering its content", async () => {
  const server = await startServer((_request, response) =>
    sendJson(response, completionPayload(" GARDN_𐐀_OK\n")),
  );
  try {
    assert.equal(
      await completeOpenRouterTurn({
        messages,
        model: "openrouter/test-model",
        apiKey: "secret-test-key",
        baseUrl: server.baseUrl,
      }),
      " GARDN_𐐀_OK\n",
    );
  } finally {
    await server.close();
  }
});

test("rejects non-successful responses without exposing provider data or credentials", async () => {
  const server = await startServer((_request, response) =>
    sendJson(response, { error: { message: "private provider payload" } }, 429),
  );

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/test-model", apiKey: "secret-test-key", baseUrl: server.baseUrl }),
      (error) => {
        assert.match(error.message, /OpenRouter request failed \(HTTP 429\)/);
        assert.doesNotMatch(error.message, /private provider payload|secret-test-key/);
        return true;
      },
    );
  } finally {
    await server.close();
  }
});

test("rejects provider errors in an HTTP-successful JSON response", async () => {
  const server = await startServer((_request, response) =>
    sendJson(response, { error: { message: "private provider payload" } }),
  );

  try {
    await assert.rejects(
      completeOpenRouterTurn({ messages, model: "openrouter/test-model", apiKey: "secret-test-key", baseUrl: server.baseUrl }),
      (error) => {
        assert.match(error.message, /provider error/);
        assert.doesNotMatch(error.message, /private provider payload|secret-test-key/);
        return true;
      },
    );
  } finally {
    await server.close();
  }
});

for (const [name, payload, pattern] of [
  ["malformed JSON", "{invalid", /malformed JSON/],
  ["missing a single choice", { choices: [] }, /invalid response/],
  ["missing an assistant message", { choices: [{ finish_reason: "stop" }] }, /invalid response/],
]) {
  test(`rejects ${name}`, async () => {
    const server = await startServer((_request, response) => sendJson(response, payload));
    try {
      await assert.rejects(
        completeOpenRouterTurn({ messages, model: "openrouter/test-model", apiKey: "secret-test-key", baseUrl: server.baseUrl }),
        pattern,
      );
    } finally {
      await server.close();
    }
  });
}

for (const [name, payload, pattern] of [
  ["empty content", completionPayload("   "), /assistant completion is empty/],
  ["wrong role", { choices: [{ message: { role: "user", content: "GARDN" }, finish_reason: "stop" }] }, /invalid assistant message/],
  ["non-text content", { choices: [{ message: { role: "assistant", content: [{ type: "text", text: "GARDN" }] }, finish_reason: "stop" }] }, /invalid assistant message/],
]) {
  test(`rejects ${name} assistant completions`, async () => {
    const server = await startServer((_request, response) => sendJson(response, payload));
    try {
      await assert.rejects(
        completeOpenRouterTurn({ messages, model: "openrouter/test-model", apiKey: "secret-test-key", baseUrl: server.baseUrl }),
        pattern,
      );
    } finally {
      await server.close();
    }
  });
}

for (const [name, payload] of [
  ["tool calls", { choices: [{ message: { role: "assistant", content: null, tool_calls: [{ id: "call" }] }, finish_reason: "tool_calls" }] }],
  ["function calls", { choices: [{ message: { role: "assistant", content: null, function_call: { name: "lookup" } }, finish_reason: "function_call" }] }],
]) {
  test(`rejects ${name} instead of pretending a text turn completed`, async () => {
    const server = await startServer((_request, response) => sendJson(response, payload));
    try {
      await assert.rejects(
        completeOpenRouterTurn({ messages, model: "openrouter/test-model", apiKey: "secret-test-key", baseUrl: server.baseUrl }),
        /tool-call completion is unsupported/,
      );
    } finally {
      await server.close();
    }
  });
}

for (const finishReason of ["length", "content_filter", null]) {
  test(`rejects ${finishReason ?? "missing"} finish-reason completions`, async () => {
    const server = await startServer((_request, response) => sendJson(response, completionPayload("GARDN_PARTIAL", finishReason)));
    try {
      await assert.rejects(
        completeOpenRouterTurn({ messages, model: "openrouter/test-model", apiKey: "secret-test-key", baseUrl: server.baseUrl }),
        /did not finish with a complete assistant response/,
      );
    } finally {
      await server.close();
    }
  });
}

test("cancels response-body consumption when the caller aborts", { timeout: 5_000 }, async () => {
  const responseStarted = Promise.withResolvers();
  const server = await startServer((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.write('{"choices":[');
    responseStarted.resolve();
  });
  const controller = new AbortController();

  try {
    const completion = completeOpenRouterTurn({
      messages,
      model: "openrouter/test-model",
      apiKey: "secret-test-key",
      baseUrl: server.baseUrl,
      signal: controller.signal,
    });
    await responseStarted.promise;
    controller.abort();
    await assert.rejects(completion, /request was cancelled/);
  } finally {
    controller.abort();
    await server.close();
  }
});

test("times out response-body consumption when the provider never completes", { timeout: 5_000 }, async () => {
  const server = await startServer((_request, response) => {
    response.writeHead(200, { "content-type": "application/json" });
    response.write('{"choices":[');
  });

  try {
    await assert.rejects(
      completeOpenRouterTurn({
        messages,
        model: "openrouter/test-model",
        apiKey: "secret-test-key",
        baseUrl: server.baseUrl,
        timeoutMs: 100,
      }),
      /request timed out/,
    );
  } finally {
    await server.close();
  }
});
