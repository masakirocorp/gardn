#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_OPENCODE_SMOKE_MODEL:-openrouter/openrouter/free}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_opencode_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_OPENCODE_SMOKE_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
workdir="${HAKO_OPENCODE_SMOKE_DIR:-$(mktemp -d)}"
output="${HAKO_OPENCODE_SMOKE_OUTPUT:-$workdir/opencode-smoke.jsonl}"
mkdir -p "$workdir"


cat > "$workdir/opencode.json" <<EOF_CONFIG
{
  "\$schema": "https://opencode.ai/config.json",
  "model": "$model",
  "small_model": "$model"
}
EOF_CONFIG

prompt='Run the shell command: printf HAKO_OPENCODE_TOOL_OK. Then reply with exactly HAKO_OPENCODE_SMOKE_OK.'

timeout_seconds="${HAKO_OPENCODE_SMOKE_TIMEOUT:-120}"
set +e
timeout "$timeout_seconds" opencode run \
  --pure \
  --dangerously-skip-permissions \
  --dir "$workdir" \
  --model "$model" \
  --format json \
  --title hako-opencode-smoke \
  "$prompt" >"$output" 2>&1
status=$?
set -e

if (( status != 0 )); then
  if grep -q 'Missing Authentication header' "$output"; then
    echo "opencode did not send OpenRouter credentials" >&2
  elif hako_smoke_retryable_status_or_output "$status" "$output"; then
    echo "opencode smoke retryable provider/model failure with $model" >&2
    sed -n '1,80p' "$output" >&2
    exit 75
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
