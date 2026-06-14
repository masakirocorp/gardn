#!/usr/bin/env bash
set -euo pipefail

if [[ "${OPENROUTER_API_KEY:-}" != "sk-hako-smoke-test" ]]; then
  echo "OPENROUTER_API_KEY was not propagated into the smoke environment" >&2
  exit 1
fi

required_env=(
  OPENAI_API_KEY
  ANTHROPIC_AUTH_TOKEN
  ANTHROPIC_BASE_URL
  ANTHROPIC_MODEL
  COPILOT_PROVIDER_API_KEY
  COPILOT_PROVIDER_BASE_URL
  COPILOT_MODEL
  CODEX_HOME
  HAKO_SMOKE_MODEL
  HAKO_SMOKE_FALLBACK_MODELS
  OPENCODE_AUTH_CONTENT
)

for key in "${required_env[@]}"; do
  if [[ -z "${!key:-}" ]]; then
    echo "missing smoke environment variable: $key" >&2
    exit 1
  fi
done

test -f "$CODEX_HOME/config.toml"
test -f "$HOME/.factory/settings.json"
test -f "$HOME/.hermes/config.yaml"
test -f "$HOME/.qoder/settings.json"

grep -q 'poolside/laguna-m.1:free' "$CODEX_HOME/config.toml"
grep -q 'model_provider = "openrouter"' "$CODEX_HOME/config.toml"
grep -q 'env_key = "OPENROUTER_API_KEY"' "$CODEX_HOME/config.toml"
grep -q 'https://openrouter.ai/api/v1' "$CODEX_HOME/config.toml"
grep -q 'poolside/laguna-m.1:free' "$HOME/.factory/settings.json"
grep -q 'generic-chat-completion-api' "$HOME/.factory/settings.json"
grep -q 'provider: openrouter' "$HOME/.hermes/config.yaml"
grep -q "default: \"poolside/laguna-m.1:free\"" "$HOME/.hermes/config.yaml"
grep -q 'openrouter' <<<"$OPENCODE_AUTH_CONTENT"

if [[ "$(id -un)" != "smoke" ]]; then
  echo "smoke checks must run as the non-root smoke user" >&2
  exit 1
fi

printf 'agent smoke environment ok\n'
