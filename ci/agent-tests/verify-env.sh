#!/usr/bin/env bash
set -euo pipefail

if [[ "${OPENROUTER_API_KEY:-}" != "sk-gardn-agent-test" ]]; then
  echo "OPENROUTER_API_KEY was not propagated into the test environment" >&2
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
  FACTORY_HOME
  KIMI_CODE_HOME
  GARDN_TEST_MODEL
  GARDN_TEST_FALLBACK_MODELS
  OPENCODE_AUTH_CONTENT
)

for key in "${required_env[@]}"; do
  if [[ -z "${!key:-}" ]]; then
    echo "missing test environment variable: $key" >&2
    exit 1
  fi
done

test -f "$CODEX_HOME/config.toml"
test -f "$FACTORY_HOME/settings.json"
test -f "$KIMI_CODE_HOME/config.toml"
test -f "$HOME/.hermes/config.yaml"
test -f "$HOME/.qoder/settings.json"

grep -Fq "$GARDN_TEST_MODEL" "$CODEX_HOME/config.toml"
grep -q 'model_provider = "openrouter"' "$CODEX_HOME/config.toml"
grep -q 'env_key = "OPENROUTER_API_KEY"' "$CODEX_HOME/config.toml"
grep -q 'https://openrouter.ai/api/v1' "$CODEX_HOME/config.toml"
grep -Fq "$GARDN_TEST_MODEL" "$FACTORY_HOME/settings.json"
grep -q 'generic-chat-completion-api' "$FACTORY_HOME/settings.json"
grep -q 'provider: openrouter' "$HOME/.hermes/config.yaml"
grep -Fq "$GARDN_TEST_MODEL" "$HOME/.hermes/config.yaml"
grep -Fq "$GARDN_TEST_MODEL" "$KIMI_CODE_HOME/config.toml"
grep -q 'openrouter' <<<"$OPENCODE_AUTH_CONTENT"

if [[ "$(id -un)" != "agenttest" ]]; then
  echo "agent checks must run as the non-root agent user" >&2
  exit 1
fi

printf 'agent test environment ok\n'
