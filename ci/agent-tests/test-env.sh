#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required for real CLI test runs" >&2
  exit 64
fi

test_model_lib="${GARDN_AGENT_TEST_MODELS_LIB:-/usr/local/lib/gardn-agent-test-models.sh}"
if [[ ! -f "$test_model_lib" ]]; then
  echo "agent test environment needs $test_model_lib" >&2
  exit 1
fi
source "$test_model_lib"

model="${GARDN_TEST_MODEL:-$GARDN_TEST_DEFAULT_MODEL}"
fallback_models="${GARDN_TEST_FALLBACK_MODELS:-$GARDN_TEST_DEFAULT_FALLBACK_MODELS}"
gardn_test_unique_candidates "$model" "$fallback_models" >/dev/null
export GARDN_TEST_MODEL="$model"
export GARDN_TEST_FALLBACK_MODELS="$fallback_models"
export OPENROUTER_API_KEY
gardn_test_configure_model "$model"

mkdir -p "$HOME/.qoder"
cat > "$HOME/.qoder/settings.json" <<EOF_QODER
{
  "general": {
    "enableAutoUpdate": false
  }
}
EOF_QODER

exec "$@"
