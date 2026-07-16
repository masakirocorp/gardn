#!/usr/bin/env bash

omh_test_unique_candidates() {
  local primary="$1"
  local fallbacks="${2:-${OMH_TEST_FALLBACK_MODELS:-}}"
  python3 - "$primary" "$fallbacks" <<'PY'
import sys
seen = set()
for raw in [sys.argv[1], *sys.argv[2].split(',')]:
    model = raw.strip()
    if not model or model in seen:
        continue
    seen.add(model)
    print(model)
PY
}

omh_test_openrouter_prefixed_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openrouter/*) printf '%s\n' "$model" ;;
      *) printf 'openrouter/%s\n' "$model" ;;
    esac
  done
}

omh_test_opencode_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openrouter/free|openrouter/auto|openrouter/fusion|openrouter/bodybuilder|openrouter/pareto-code|openrouter/owl-alpha)
        printf 'openrouter/%s\n' "$model"
        ;;
      openrouter/*)
        printf '%s\n' "$model"
        ;;
      *)
        printf 'openrouter/%s\n' "$model"
        ;;
    esac
  done
}

omh_test_openrouter_bare_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openrouter/*) printf '%s\n' "${model#openrouter/}" ;;
      *) printf '%s\n' "$model" ;;
    esac
  done
}

omh_test_openrouter_api_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openrouter/openai/*|openrouter/anthropic/*|openrouter/google/*|openrouter/meta-llama/*|openrouter/mistralai/*|openrouter/nvidia/*|openrouter/deepseek/*|openrouter/qwen/*|openrouter/x-ai/*|openrouter/poolside/*)
        printf '%s\n' "${model#openrouter/}"
        ;;
      *)
        printf '%s\n' "$model"
        ;;
    esac
  done
}


omh_test_non_openai_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openrouter/openai/*|openai/*|gpt-*)
        printf 'skip model %s: OpenAI-family test is intentionally excluded here\n' "$model" >&2
        ;;
      *) printf '%s\n' "$model" ;;
    esac
  done
}

omh_test_non_anthropic_candidates() {
  while IFS= read -r model; do
    case "$model" in
      openrouter/anthropic/*|anthropic/*|claude*)
        printf 'skip model %s: Anthropic-family test is intentionally excluded here\n' "$model" >&2
        ;;
      *) printf '%s\n' "$model" ;;
    esac
  done
}

omh_test_retryable_output() {
  local output="$1"
  [[ -f "$output" ]] || return 1
  grep -Eiq '(^|[^0-9])(408|429|500|502|503|504|529)([^0-9]|$)|rate.?limit|temporar(il)?y unavailable|overload|capacity|upstream|no route|no endpoint|provider.*(unavailable|error)|model.*(not found|unavailable|unsupported)|fetch failed|socket hang up|ECONNRESET|ETIMEDOUT|timed out|timeout|stream.*reset|ProviderModelNotFoundError' "$output"
}

omh_test_retryable_status_or_output() {
  local status="$1"
  local output="$2"
  [[ "$status" -eq 124 ]] || omh_test_retryable_output "$output"
}

omh_test_run_with_fallbacks() {
  local script="$1"
  local env_name="$2"
  shift 2
  local attempts=()
  local model status
  local candidates=()
  if [[ -n "${OMH_TEST_ACTIVE_MODEL:-}" ]]; then
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
    OMH_TEST_ACTIVE_MODEL="$model" "$script" "$@" >"$attempt_output" 2>&1
    status=$?
    set -e
    cat "$attempt_output" >&2
    if [[ "$status" -eq 0 ]]; then
      rm -f "$attempt_output"
      return 0
    fi
    if [[ "$status" -eq 75 ]] || omh_test_retryable_status_or_output "$status" "$attempt_output"; then
      attempts+=("$model: retryable provider/model failure")
      printf 'retrying after provider/model failure: %s\n' "$model" >&2
      rm -f "$attempt_output"
      continue
    fi
    rm -f "$attempt_output"
    return "$status"
  done
  printf 'all test model candidates failed before Oh My Herdr assertions:\n' >&2
  printf '  %s\n' "${attempts[@]}" >&2
  return 75
}
