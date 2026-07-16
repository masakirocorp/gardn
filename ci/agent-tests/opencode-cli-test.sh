#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/omh-agent-test-models.sh
primary_model="${OMH_OPENCODE_TEST_MODEL:-openrouter/openrouter/free}"
if [[ -z "${OMH_TEST_ACTIVE_MODEL:-}" ]]; then
  omh_test_unique_candidates "$primary_model" "${OMH_TEST_FALLBACK_MODELS:-}" \
    | omh_test_opencode_candidates \
    | omh_test_run_with_fallbacks "$0" OMH_OPENCODE_TEST_MODEL "$@"
  exit $?
fi

model="$OMH_TEST_ACTIVE_MODEL"
workdir="${OMH_OPENCODE_TEST_DIR:-$(mktemp -d)}"
output="${OMH_OPENCODE_TEST_OUTPUT:-$workdir/opencode-test.jsonl}"
mkdir -p "$workdir"


cat > "$workdir/opencode.json" <<EOF_CONFIG
{
  "\$schema": "https://opencode.ai/config.json",
  "model": "$model",
  "small_model": "$model"
}
EOF_CONFIG

prompt='Run the shell command: printf OMH_OPENCODE_TOOL_OK. Then reply with exactly OMH_OPENCODE_TEST_OK.'

timeout_seconds="${OMH_OPENCODE_TEST_TIMEOUT:-120}"
set +e
timeout "$timeout_seconds" opencode run \
  --pure \
  --dangerously-skip-permissions \
  --dir "$workdir" \
  --model "$model" \
  --format json \
  --title omh-opencode-test \
  "$prompt" >"$output" 2>&1
status=$?
set -e

if (( status != 0 )); then
  if grep -q 'Missing Authentication header' "$output"; then
    echo "opencode did not send OpenRouter credentials" >&2
  elif omh_test_retryable_status_or_output "$status" "$output"; then
    echo "opencode test retryable provider/model failure with $model" >&2
    sed -n '1,80p' "$output" >&2
    exit 75
  fi
  echo "opencode test failed with exit code $status" >&2
  sed -n '1,80p' "$output" >&2
  exit "$status"
fi

if ! grep -q 'OMH_OPENCODE_TOOL_OK' "$output"; then
  echo "opencode test did not observe expected tool output marker" >&2
  sed -n '1,120p' "$output" >&2
  exit 1
fi

if ! grep -q 'OMH_OPENCODE_TEST_OK' "$output"; then
  echo "opencode test did not observe expected response marker" >&2
  sed -n '1,120p' "$output" >&2
  exit 1
fi

printf 'opencode test ok: %s\n' "$model"
