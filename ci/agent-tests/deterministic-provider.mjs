#!/usr/bin/env node
import { appendFileSync } from "node:fs";
import { createServer } from "node:http";
import process from "node:process";

const host = process.env.GARDN_PROVIDER_HOST || "127.0.0.1";
const port = Number.parseInt(process.env.GARDN_PROVIDER_PORT || process.env.PORT || "8765", 10);
const logFile = process.env.GARDN_PROVIDER_LOG || process.env.LOG_FILE || "";
const requests = [];
const attempts = new Map();

function sendJson(response, status, body) {
  const payload = `${JSON.stringify(body)}\n`;
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(payload),
  });
  response.end(payload);
}

function sendSse(response, events, done = false) {
  response.writeHead(200, {
    "content-type": "text/event-stream; charset=utf-8",
    "cache-control": "no-cache",
    connection: "keep-alive",
  });
  for (const event of events) {
    response.write(`data: ${JSON.stringify(event)}\n\n`);
  }
  if (done) response.write("data: [DONE]\n\n");
  response.end();
}

async function readJson(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > 1024 * 1024) throw new Error("request body exceeds 1 MiB");
    chunks.push(chunk);
  }
  const text = Buffer.concat(chunks).toString("utf8");
  return text ? JSON.parse(text) : {};
}

function recordRequest(request, url, body) {
  const entry = {
    sequence: requests.length + 1,
    method: request.method,
    path: url.pathname,
    query: Object.fromEntries(url.searchParams),
    headers: Object.fromEntries(
      Object.entries(request.headers).filter(([name]) => name !== "authorization" && name !== "x-goog-api-key"),
    ),
    body,
  };
  requests.push(entry);
  if (logFile) appendFileSync(logFile, `${JSON.stringify(entry)}\n`, { mode: 0o600 });
}

function requestText(body) {
  const openAiText = Array.isArray(body.messages)
    ? body.messages.map((message) => (typeof message.content === "string" ? message.content : "")).join("\n")
    : "";
  const geminiText = Array.isArray(body.contents)
    ? body.contents
        .flatMap((content) => (Array.isArray(content.parts) ? content.parts : []))
        .map((part) => (typeof part.text === "string" ? part.text : ""))
        .join("\n")
    : "";
  return `${openAiText}\n${geminiText}`;
}

function scenarioFor(request, body, model) {
  const explicit = request.headers["x-gardn-scenario"];
  if (typeof explicit === "string" && explicit) return explicit;
  const marker = requestText(body).match(/\[gardn:(text|tool|retry-429|error-400|error-500)\]/i);
  if (marker) return marker[1].toLowerCase();
  for (const scenario of ["retry-429", "error-400", "error-500", "tool"]) {
    if (model.includes(scenario)) return scenario;
  }
  return "text";
}

function attemptFor(protocol, model, body) {
  const key = JSON.stringify([protocol, model, body.messages || body.contents || []]);
  const attempt = (attempts.get(key) || 0) + 1;
  attempts.set(key, attempt);
  return attempt;
}

function openAiError(response, status) {
  const label = status === 429 ? "rate_limit_exceeded" : "server_error";
  sendJson(response, status, {
    error: {
      message: `deterministic provider error ${status}`,
      type: label,
      param: null,
      code: label,
    },
  });
}

function openAiTool(body) {
  const tools = body.tools?.filter((tool) => tool?.type === "function") || [];
  const selected =
    tools.find((tool) => /(?:bash|shell|terminal|exec)/i.test(tool.function?.name || "")) || tools[0];
  const name = selected?.function?.name || "gardn_echo";
  const properties = selected?.function?.parameters?.properties || {};
  const commandProperty =
    ["command", "cmd", "script"].find((property) => Object.hasOwn(properties, property)) || "value";
  const argumentsValue =
    commandProperty === "value"
      ? { value: "gardn" }
      : { [commandProperty]: "printf GARDN_PROVIDER_TOOL_OK" };
  return {
    id: "call_gardn_001",
    type: "function",
    function: {
      name,
      arguments: JSON.stringify(argumentsValue),
    },
  };
}

function hasOpenAiToolResult(body) {
  return body.messages?.some((message) => message?.role === "tool") === true;
}

function openAiResponse(model, text, finishReason, message) {
  return {
    id: "chatcmpl-gardn-001",
    object: "chat.completion",
    created: 1_700_000_000,
    model,
    choices: [{ index: 0, message, finish_reason: finishReason }],
    usage: { prompt_tokens: 8, completion_tokens: 4, total_tokens: 12 },
  };
}

function handleOpenAi(request, response, body) {
  const model = typeof body.model === "string" ? body.model : "gardn-text";
  const scenario = scenarioFor(request, body, model);
  const attempt = attemptFor("openai", model, body);
  if (scenario === "retry-429" && attempt === 1) return openAiError(response, 429);
  if (scenario === "error-400") return openAiError(response, 400);
  if (scenario === "error-500") return openAiError(response, 500);

  const wantsTool = scenario === "tool" && !hasOpenAiToolResult(body);
  if (!body.stream) {
    if (wantsTool) {
      const toolCall = openAiTool(body);
      return sendJson(
        response,
        200,
        openAiResponse(model, "", "tool_calls", { role: "assistant", content: null, tool_calls: [toolCall] }),
      );
    }
    const text = hasOpenAiToolResult(body) ? "GARDN_TOOL_COMPLETE" : "GARDN_PROVIDER_OK";
    return sendJson(response, 200, openAiResponse(model, text, "stop", { role: "assistant", content: text }));
  }

  const base = {
    id: "chatcmpl-gardn-001",
    object: "chat.completion.chunk",
    created: 1_700_000_000,
    model,
  };
  if (wantsTool) {
    const toolCall = openAiTool(body);
    return sendSse(
      response,
      [
        { ...base, choices: [{ index: 0, delta: { role: "assistant", content: "" }, finish_reason: null }] },
        {
          ...base,
          choices: [
            {
              index: 0,
              delta: {
                tool_calls: [
                  {
                    index: 0,
                    id: toolCall.id,
                    type: "function",
                    function: { name: toolCall.function.name, arguments: toolCall.function.arguments },
                  },
                ],
              },
              finish_reason: null,
            },
          ],
        },
        { ...base, choices: [{ index: 0, delta: {}, finish_reason: "tool_calls" }] },
      ],
      true,
    );
  }

  const text = hasOpenAiToolResult(body) ? "GARDN_TOOL_COMPLETE" : "GARDN_PROVIDER_OK";
  return sendSse(
    response,
    [
      { ...base, choices: [{ index: 0, delta: { role: "assistant", content: "" }, finish_reason: null }] },
      { ...base, choices: [{ index: 0, delta: { content: text }, finish_reason: null }] },
      { ...base, choices: [{ index: 0, delta: {}, finish_reason: "stop" }] },
      { ...base, choices: [], usage: { prompt_tokens: 8, completion_tokens: 4, total_tokens: 12 } },
    ],
    true,
  );
}

function geminiError(response, status) {
  const statusName = status === 429 ? "RESOURCE_EXHAUSTED" : status === 400 ? "INVALID_ARGUMENT" : "INTERNAL";
  sendJson(response, status, {
    error: { code: status, message: `deterministic provider error ${status}`, status: statusName },
  });
}

function hasGeminiToolResult(body) {
  return body.contents?.some((content) => content?.parts?.some((part) => part?.functionResponse)) === true;
}

function geminiTool(body) {
  const declaration = body.tools
    ?.flatMap((tool) => tool?.functionDeclarations || tool?.function_declarations || [])
    .find((value) => value?.name);
  return { name: declaration?.name || "gardn_echo", args: { value: "gardn" } };
}

function geminiResponse(part, finishReason = "STOP") {
  return {
    candidates: [{ content: { role: "model", parts: [part] }, finishReason, index: 0 }],
    usageMetadata: { promptTokenCount: 8, candidatesTokenCount: 4, totalTokenCount: 12 },
    modelVersion: "gardn-deterministic-001",
  };
}

function handleGemini(request, response, url, body, model, streaming) {
  const scenario = scenarioFor(request, body, model);
  const attempt = attemptFor("gemini", model, body);
  if (scenario === "retry-429" && attempt === 1) return geminiError(response, 429);
  if (scenario === "error-400") return geminiError(response, 400);
  if (scenario === "error-500") return geminiError(response, 500);

  const wantsTool = scenario === "tool" && !hasGeminiToolResult(body);
  const part = wantsTool
    ? { functionCall: geminiTool(body) }
    : { text: hasGeminiToolResult(body) ? "GARDN_TOOL_COMPLETE" : "GARDN_PROVIDER_OK" };
  const payload = geminiResponse(part);
  if (streaming || url.searchParams.get("alt") === "sse") return sendSse(response, [payload]);
  return sendJson(response, 200, payload);
}

const server = createServer(async (request, response) => {
  const url = new URL(request.url || "/", `http://${request.headers.host || "localhost"}`);
  if (request.method === "GET" && url.pathname === "/health") {
    return sendJson(response, 200, { ok: true });
  }
  if (request.method === "GET" && url.pathname === "/__gardn/requests") {
    return sendJson(response, 200, { requests });
  }
  if (request.method !== "POST") return sendJson(response, 404, { error: "not found" });

  let body;
  try {
    body = await readJson(request);
  } catch (error) {
    return sendJson(response, 400, { error: { message: error.message } });
  }
  recordRequest(request, url, body);

  if (url.pathname === "/v1/chat/completions" || url.pathname === "/chat/completions") {
    return handleOpenAi(request, response, body);
  }
  const match = url.pathname.match(/^\/(?:v1|v1beta)\/models\/([^/:]+):(streamGenerateContent|generateContent)$/);
  if (match) {
    return handleGemini(request, response, url, body, decodeURIComponent(match[1]), match[2] === "streamGenerateContent");
  }
  return sendJson(response, 404, { error: "not found" });
});

server.listen(port, host, () => {
  const address = server.address();
  process.stdout.write(`${JSON.stringify({ ready: true, host, port: address.port })}\n`);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
