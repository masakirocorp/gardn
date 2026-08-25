#!/usr/bin/env bash

GARDN_TEST_DEFAULT_MODEL="openrouter/free"
GARDN_TEST_DEFAULT_FALLBACK_MODELS="nvidia/nemotron-3-super-120b-a12b:free"

gardn_test_unique_candidates() {
  local primary="$1"
  local fallbacks="${2:-${GARDN_TEST_FALLBACK_MODELS:-}}"
  python3 - "$primary" "$fallbacks" <<'PY'
import re
import sys

pattern = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*/[A-Za-z0-9][A-Za-z0-9._:+-]*")
seen = set()
for raw in [sys.argv[1], *sys.argv[2].split(",")]:
    model = raw.strip()
    if not model:
        continue
    if not pattern.fullmatch(model) or model.startswith("openrouter/openrouter/"):
        print(f"invalid canonical OpenRouter model id: {model!r}", file=sys.stderr)
        raise SystemExit(64)
    if model in seen:
        continue
    seen.add(model)
    print(model)
PY
}

gardn_test_available_candidates() {
  local candidates=()
  local model
  while IFS= read -r model; do
    [[ -n "$model" ]] && candidates+=("$model")
  done
  if [[ "${GARDN_TEST_SKIP_MODEL_PREFLIGHT:-0}" == "1" ]]; then
    printf '%s\n' "${candidates[@]}"
    return 0
  fi
  local response
  response="$(mktemp)"
  local models_url="${OPENROUTER_MODELS_URL:-${OPENROUTER_BASE_URL:-https://openrouter.ai/api/v1}/models}"
  local curl_args=(-fsSL --connect-timeout 15 --max-time 60)
  if [[ -n "${OPENROUTER_API_KEY:-}" ]]; then
    curl_args+=(-H "Authorization: Bearer ${OPENROUTER_API_KEY}")
  fi
  if ! curl "${curl_args[@]}" "$models_url" >"$response"; then
    rm -f "$response"
    echo "failed to load the OpenRouter model catalog" >&2
    return 69
  fi
  python3 - "$response" "${candidates[@]}" <<'PY'
import json
import sys
from pathlib import Path

try:
    payload = json.loads(Path(sys.argv[1]).read_text())
    available = {item["id"] for item in payload["data"]}
except (KeyError, TypeError, ValueError) as error:
    print(f"invalid OpenRouter model catalog: {error}", file=sys.stderr)
    raise SystemExit(69)
for model in sys.argv[2:]:
    if model in available:
        print(model)
    else:
        print(f"skip model {model}: absent from OpenRouter catalog", file=sys.stderr)
PY
  local status=$?
  rm -f "$response"
  return "$status"
}

gardn_test_provider_model() {
  printf 'openrouter/%s\n' "$1"
}

gardn_test_non_openai_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openai/*|gpt-*)
        printf 'skip model %s: OpenAI-family test is intentionally excluded here\n' "$model" >&2
        ;;
      *) printf '%s\n' "$model" ;;
    esac
  done
}

gardn_test_non_anthropic_candidates() {
  while IFS= read -r model; do
    case "$model" in
      anthropic/*|claude*)
        printf 'skip model %s: Anthropic-family test is intentionally excluded here\n' "$model" >&2
        ;;
      *) printf '%s\n' "$model" ;;
    esac
  done
}

gardn_test_configure_model() {
  local model="$1"
  local openrouter_base="${OPENROUTER_BASE_URL:-https://openrouter.ai/api/v1}"
  if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
    echo "OPENROUTER_API_KEY is required for model configuration" >&2
    return 64
  fi

  export GARDN_TEST_MODEL="$model"
  export OPENAI_API_KEY="$OPENROUTER_API_KEY"
  export OPENAI_BASE_URL="$openrouter_base"
  export ANTHROPIC_AUTH_TOKEN="$OPENROUTER_API_KEY"
  export ANTHROPIC_BASE_URL="$openrouter_base"
  export ANTHROPIC_MODEL="$model"
  export COPILOT_PROVIDER_API_KEY="$OPENROUTER_API_KEY"
  export COPILOT_PROVIDER_BASE_URL="$openrouter_base"
  export COPILOT_MODEL="$model"
  export OPENCODE_AUTH_CONTENT="{\"openrouter\":{\"type\":\"api\",\"key\":\"$OPENROUTER_API_KEY\"}}"
  export KILO_AUTH_CONTENT="$OPENCODE_AUTH_CONTENT"

  export CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
  local factory_home="${FACTORY_HOME:-${DROID_HOME:-$HOME/.factory}}"
  export FACTORY_HOME="$factory_home"
  export DROID_HOME="$factory_home"
  export KIMI_CODE_HOME="${KIMI_CODE_HOME:-$HOME/.kimi-code}"
  mkdir -p "$CODEX_HOME" "$factory_home" "$KIMI_CODE_HOME" "$HOME/.hermes"

  python3 - "$model" "$openrouter_base" "$OPENROUTER_API_KEY" "$CODEX_HOME/config.toml" "$factory_home/settings.json" "$KIMI_CODE_HOME/config.toml" "$HOME/.hermes/config.yaml" <<'PY'
import json
import sys
from pathlib import Path

model, base_url, api_key, codex_path, factory_path, kimi_path, hermes_path = sys.argv[1:]
Path(codex_path).write_text(
    f'model = "{model}"\n'
    'model_provider = "openrouter"\n'
    '[model_providers.openrouter]\n'
    'name = "OpenRouter"\n'
    f'base_url = "{base_url}"\n'
    'env_key = "OPENROUTER_API_KEY"\n'
    'wire_api = "chat"\n'
)
Path(factory_path).write_text(json.dumps({
    "customModels": [{
        "model": model,
        "displayName": "Gardn Test OpenRouter",
        "baseUrl": base_url,
        "apiKey": "${OPENROUTER_API_KEY}",
        "provider": "generic-chat-completion-api",
        "maxOutputTokens": 4096,
    }],
    "model": model,
    "cloudSessionSync": False,
}, indent=2) + "\n")
Path(kimi_path).write_text(
    'default_model = "openrouter"\n\n'
    '[providers.openrouter]\n'
    'type = "openai"\n'
    f'base_url = "{base_url}"\n'
    f'api_key = "{api_key}"\n\n'
    '[models.openrouter]\n'
    'provider = "openrouter"\n'
    f'model = "{model}"\n'
    'max_context_size = 128000\n'
    'max_output_size = 4096\n'
    'capabilities = ["tool_use"]\n'
)
Path(hermes_path).write_text(
    'model:\n'
    '  provider: openrouter\n'
    f'  default: "{model}"\n'
    f'  base_url: "{base_url}"\n'
)
PY
  export KIMI_DISABLE_TELEMETRY=1
  export KIMI_CODE_NO_AUTO_UPDATE=1
  cat > "$HOME/.hermes/.env" <<EOF_HERMES_ENV
OPENROUTER_API_KEY=$OPENROUTER_API_KEY
EOF_HERMES_ENV
}

gardn_test_retryable_output() {
  local output="$1"
  [[ -f "$output" ]] || return 1
  grep -Eiq '(^|[^0-9])(408|429|500|502|503|504|529)([^0-9]|$)|rate.?limit|temporar(il)?y unavailable|overload|capacity|no route|no endpoint|provider[^[:alnum:]]+(unavailable|error)|model[^[:alnum:]]+(not found|unavailable|unsupported)|ProviderModelNotFoundError' "$output"
}

gardn_test_retryable_status_or_output() {
  local status="$1"
  local output="$2"
  [[ "$status" -ne 0 ]] && gardn_test_retryable_output "$output"
}

gardn_test_run_with_fallbacks() {
  local script="$1"
  shift
  local attempts=()
  local model status
  local candidates=()
  if [[ -n "${GARDN_TEST_ACTIVE_MODEL:-}" ]]; then
    return 0
  fi
  while IFS= read -r model; do
    [[ -n "$model" ]] && candidates+=("$model")
  done
  for model in "${candidates[@]}"; do
    local attempt_output
    attempt_output="$(mktemp)"
    printf 'trying test model: %s\n' "$model" >&2
    set +e
    GARDN_TEST_ACTIVE_MODEL="$model" "$script" "$@" >"$attempt_output" 2>&1
    status=$?
    set -e
    cat "$attempt_output" >&2
    if [[ "$status" -eq 0 ]]; then
      rm -f "$attempt_output"
      return 0
    fi
    if [[ "$status" -eq 75 ]]; then
      attempts+=("$model: retryable provider/model failure")
      printf 'retrying after provider/model failure: %s\n' "$model" >&2
      rm -f "$attempt_output"
      continue
    fi
    rm -f "$attempt_output"
    return "$status"
  done
  printf 'all test model candidates failed before Gardn assertions:\n' >&2
  printf '  %s\n' "${attempts[@]}" >&2
  return 75
}
