const DEFAULT_BASE_URL = "https://openrouter.ai/api/v1";
const DEFAULT_TIMEOUT_MS = 120_000;
const MAX_OUTPUT_TOKENS = 256;

function providerError(message) {
  return new Error(`OpenRouter ${message}`);
}

function endpointFor(baseUrl) {
  if (typeof baseUrl !== "string" || baseUrl.trim() === "") {
    throw providerError("base URL is invalid");
  }

  let url;
  try {
    url = new URL(baseUrl);
  } catch {
    throw providerError("base URL is invalid");
  }

  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw providerError("base URL is invalid");
  }
  url.pathname = `${url.pathname.replace(/\/+$/, "")}/chat/completions`;
  url.search = "";
  url.hash = "";
  return url;
}

function parseSseEvent(dataLines, state) {
  const data = dataLines.join("\n");
  if (data === "[DONE]") {
    if (state.seenDone) throw providerError("stream sent duplicate completion marker");
    if (!state.finished) throw providerError("stream ended before a completed assistant response");
    state.seenDone = true;
    return;
  }
  if (state.seenDone) throw providerError("stream sent data after completion marker");
  if (data === "") throw providerError("stream contained an empty event");

  let payload;
  try {
    payload = JSON.parse(data);
  } catch {
    throw providerError("stream contained malformed JSON");
  }
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) {
    throw providerError("stream contained an invalid response");
  }
  if (payload.error != null) throw providerError("stream reported a provider error");
  if (!Array.isArray(payload.choices)) throw providerError("stream contained an invalid response");
  if (payload.choices.length === 0) {
    if (!state.finished) throw providerError("stream contained an incomplete response");
    return;
  }
  if (state.finished) throw providerError("stream sent data after the completed response");

  for (const choice of payload.choices) {
    if (choice === null || typeof choice !== "object" || Array.isArray(choice)) {
      throw providerError("stream contained an invalid response");
    }
    const delta = choice.delta;
    if (delta === null || typeof delta !== "object" || Array.isArray(delta)) {
      throw providerError("stream contained an invalid response");
    }
    if (delta.tool_calls != null || delta.function_call != null) {
      throw providerError("tool-call completion is unsupported");
    }
    if (delta.content != null) {
      if (typeof delta.content !== "string") throw providerError("stream contained invalid assistant text");
      state.text += delta.content;
    }
    if (choice.finish_reason !== null && choice.finish_reason !== undefined) {
      if (choice.finish_reason !== "stop") {
        throw providerError("stream did not finish with a complete assistant response");
      }
      state.finished = true;
    }
  }
}

function consumeSseText(text, state, pending, dataLines, final = false) {
  pending += text;
  while (pending.length > 0) {
    let delimiterIndex = -1;
    for (let index = 0; index < pending.length; index += 1) {
      if (pending[index] === "\n" || pending[index] === "\r") {
        delimiterIndex = index;
        break;
      }
    }
    if (delimiterIndex === -1) break;

    const delimiter = pending[delimiterIndex];
    if (delimiter === "\r" && delimiterIndex === pending.length - 1 && !final) break;
    let delimiterLength = 1;
    if (delimiter === "\r" && pending[delimiterIndex + 1] === "\n") delimiterLength = 2;
    const line = pending.slice(0, delimiterIndex);
    pending = pending.slice(delimiterIndex + delimiterLength);
    if (line === "") {
      if (dataLines.length > 0) {
        parseSseEvent(dataLines, state);
        dataLines.length = 0;
      }
    } else if (!line.startsWith(":")) {
      const separator = line.indexOf(":");
      const field = separator === -1 ? line : line.slice(0, separator);
      let value = separator === -1 ? "" : line.slice(separator + 1);
      if (value.startsWith(" ")) value = value.slice(1);
      if (field === "data") dataLines.push(value);
    }
  }
  return pending;
}

async function readCompletion(response) {
  if (!response.body) throw providerError("response had no stream body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const state = { text: "", finished: false, seenDone: false };
  const dataLines = [];
  let pending = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      let text;
      try {
        text = decoder.decode(value, { stream: true });
      } catch {
        throw providerError("stream contained invalid UTF-8");
      }
      pending = consumeSseText(text, state, pending, dataLines);
    }
    try {
      pending = consumeSseText(decoder.decode(), state, pending, dataLines, true);
    } catch (error) {
      if (error instanceof Error && error.message.startsWith("OpenRouter ")) throw error;
      throw providerError("stream contained invalid UTF-8");
    }
  } catch (error) {
    if (error instanceof Error && error.message.startsWith("OpenRouter ")) throw error;
    throw providerError("stream could not be read");
  } finally {
    reader.releaseLock();
  }

  if (pending.length > 0 || dataLines.length > 0) throw providerError("stream was truncated");
  if (!state.seenDone) throw providerError("stream ended before completion marker");
  if (!state.finished) throw providerError("stream ended before a completed assistant response");
  if (state.text.trim() === "") throw providerError("assistant completion is empty");
  return state.text;
}

export async function completeOpenRouterTurn({
  messages,
  model,
  apiKey,
  baseUrl = DEFAULT_BASE_URL,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  signal,
}) {
  if (typeof model !== "string" || model.trim() === "") throw providerError("model is required");
  if (typeof apiKey !== "string" || apiKey.trim() === "") throw providerError("API key is required");
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw providerError("timeout must be positive and finite");

  const endpoint = endpointFor(baseUrl);
  const body = JSON.stringify({
    model,
    messages,
    stream: true,
    temperature: 0,
    max_tokens: MAX_OUTPUT_TOKENS,
  });

  if (signal?.aborted) throw providerError("request was cancelled");
  const requestController = new AbortController();
  let timedOut = false;
  const onCallerAbort = () => requestController.abort();
  signal?.addEventListener("abort", onCallerAbort, { once: true });
  const timeout = setTimeout(() => {
    timedOut = true;
    requestController.abort();
  }, timeoutMs);

  try {
    let response;
    try {
      response = await fetch(endpoint, {
        method: "POST",
        headers: {
          accept: "text/event-stream",
          authorization: `Bearer ${apiKey}`,
          "content-type": "application/json",
        },
        body,
        signal: requestController.signal,
      });
    } catch {
      if (timedOut) throw providerError("request timed out");
      if (signal?.aborted) throw providerError("request was cancelled");
      throw providerError("request failed");
    }

    if (!response.ok) {
      throw providerError(`request failed (HTTP ${response.status})`);
    }
    try {
      return await readCompletion(response);
    } catch (error) {
      if (timedOut) throw providerError("request timed out");
      if (signal?.aborted) throw providerError("request was cancelled");
      throw error;
    }
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", onCallerAbort);
    requestController.abort();
  }
}
