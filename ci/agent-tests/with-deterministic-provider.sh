#!/usr/bin/env bash
set -euo pipefail

provider_port="${GARDN_PROVIDER_PORT:-8765}"
provider_log="${GARDN_PROVIDER_LOG:-$(mktemp)}"
GARDN_PROVIDER_PORT="$provider_port" GARDN_PROVIDER_LOG="$provider_log" \
  node /usr/local/lib/gardn-deterministic-provider.mjs >"${provider_log}.server" 2>&1 &
provider_pid=$!
trap 'kill "$provider_pid" >/dev/null 2>&1 || true' EXIT

for _ in $(seq 1 100); do
  if curl -fsS "http://127.0.0.1:${provider_port}/health" >/dev/null; then
    break
  fi
  if ! kill -0 "$provider_pid" 2>/dev/null; then
    cat "${provider_log}.server" >&2
    exit 1
  fi
  sleep 0.1
done
curl -fsS "http://127.0.0.1:${provider_port}/health" >/dev/null

export GARDN_DETERMINISTIC_PROVIDER_URL="http://127.0.0.1:${provider_port}"
export GARDN_DETERMINISTIC_PROVIDER_LOG="$provider_log"
export OPENAI_API_KEY="gardn-deterministic-key"
export GEMINI_API_KEY="gardn-deterministic-key"
export GOOGLE_GEMINI_BASE_URL="$GARDN_DETERMINISTIC_PROVIDER_URL"

"$@"
