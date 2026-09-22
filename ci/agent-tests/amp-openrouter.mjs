const DEFAULT_BASE_URL = "https://openrouter.ai/api/v1";
const DEFAULT_TIMEOUT_MS = 120_000;
// Free routing can select reasoning models, whose thinking shares the output budget.
const MAX_OUTPUT_TOKENS = 4096;

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

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function finishReason(reason) {
  return ["length", "tool_calls", "content_filter", "error"].includes(reason) ? reason : "unknown";
}

async function readCompletion(response) {
  if (!response.body) throw providerError("response had no body");

  let payload;
  try {
    payload = await response.json();
  } catch {
    throw providerError("response contained malformed JSON");
  }

  if (!isObject(payload)) throw providerError("response contained an invalid response");
  if (payload.error != null) throw providerError("response reported a provider error");
  if (!Array.isArray(payload.choices) || payload.choices.length !== 1) {
    throw providerError("response contained an invalid response");
  }

  const choice = payload.choices[0];
  if (!isObject(choice) || !isObject(choice.message)) {
    throw providerError("response contained an invalid response");
  }
  if (choice.message.tool_calls != null || choice.message.function_call != null) {
    throw providerError("tool-call completion is unsupported");
  }
  if (choice.message.role !== "assistant" || typeof choice.message.content !== "string") {
    throw providerError("response contained an invalid assistant message");
  }
  if (choice.finish_reason !== "stop") {
    throw providerError(
      `completion did not finish with a complete assistant response (${finishReason(choice.finish_reason)})`,
    );
  }
  if (choice.message.content.trim() === "") throw providerError("assistant completion is empty");
  return choice.message.content;
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
    stream: false,
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
          accept: "application/json",
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

    if (!response.ok) throw providerError(`request failed (HTTP ${response.status})`);

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
