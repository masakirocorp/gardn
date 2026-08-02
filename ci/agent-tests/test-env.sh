#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required for real CLI test runs" >&2
  exit 64
fi

test_model_lib="${OMH_AGENT_TEST_MODELS_LIB:-/usr/local/lib/omh-agent-test-models.sh}"
if [[ ! -f "$test_model_lib" ]]; then
  echo "agent test environment needs $test_model_lib" >&2
  exit 1
fi
source "$test_model_lib"

model="${OMH_TEST_MODEL:-$OMH_TEST_DEFAULT_MODEL}"
fallback_models="${OMH_TEST_FALLBACK_MODELS:-$OMH_TEST_DEFAULT_FALLBACK_MODELS}"
omh_test_unique_candidates "$model" "$fallback_models" >/dev/null
export OMH_TEST_MODEL="$model"
export OMH_TEST_FALLBACK_MODELS="$fallback_models"
export OPENROUTER_API_KEY
omh_test_configure_model "$model"

mkdir -p "$HOME/.qoder"
cat > "$HOME/.qoder/settings.json" <<EOF_QODER
{
  "general": {
    "enableAutoUpdate": false
  }
}
EOF_QODER

exec "$@"
