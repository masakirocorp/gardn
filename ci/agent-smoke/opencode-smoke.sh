#!/usr/bin/env bash
set -euo pipefail

model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"
workdir="${HAKO_OPENCODE_SMOKE_DIR:-$(mktemp -d)}"
output="${HAKO_OPENCODE_SMOKE_OUTPUT:-$workdir/opencode-smoke.jsonl}"
mkdir -p "$workdir"

cat > "$workdir/opencode.json" <<EOF_CONFIG
{
  "\$schema": "https://opencode.ai/config.json",
  "model": "openrouter/$model",
  "small_model": "openrouter/$model"
}
EOF_CONFIG

prompt='Run the shell command: printf HAKO_OPENCODE_TOOL_OK. Then reply with exactly HAKO_OPENCODE_SMOKE_OK.'

timeout_seconds="${HAKO_OPENCODE_SMOKE_TIMEOUT:-120}"
set +e
timeout "$timeout_seconds" opencode run \
  --pure \
  --dangerously-skip-permissions \
  --dir "$workdir" \
  --model "openrouter/$model" \
  --format json \
  --title hako-opencode-smoke \
  "$prompt" >"$output" 2>&1
status=$?
set -e

if (( status != 0 )); then
  if grep -q 'Missing Authentication header' "$output"; then
    echo "opencode did not send OpenRouter credentials" >&2
  fi
  echo "opencode smoke failed with exit code $status" >&2
  sed -n '1,80p' "$output" >&2
  exit "$status"
fi

if ! grep -q 'HAKO_OPENCODE_TOOL_OK' "$output"; then
  echo "opencode smoke did not observe expected tool output marker" >&2
  sed -n '1,120p' "$output" >&2
  exit 1
fi

if ! grep -q 'HAKO_OPENCODE_SMOKE_OK' "$output"; then
  echo "opencode smoke did not observe expected response marker" >&2
  sed -n '1,120p' "$output" >&2
  exit 1
fi

printf 'opencode smoke ok: %s\n' "$model"
