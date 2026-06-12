#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required for real CLI smoke runs" >&2
  exit 64
fi

model="${HAKO_SMOKE_MODEL:-openai/gpt-4o-mini}"
openrouter_base="${OPENROUTER_BASE_URL:-https://openrouter.ai/api/v1}"

export OPENROUTER_API_KEY
export OPENAI_API_KEY="$OPENROUTER_API_KEY"
export ANTHROPIC_AUTH_TOKEN="$OPENROUTER_API_KEY"
export ANTHROPIC_BASE_URL="$openrouter_base"
export ANTHROPIC_MODEL="${HAKO_SMOKE_ANTHROPIC_MODEL:-anthropic/claude-3.5-haiku}"
export COPILOT_PROVIDER_API_KEY="$OPENROUTER_API_KEY"
export COPILOT_PROVIDER_BASE_URL="$openrouter_base"
export COPILOT_MODEL="$model"

export CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
mkdir -p "$CODEX_HOME"
cat > "$CODEX_HOME/config.toml" <<EOF_CONFIG
model = "$model"
model_provider = "openrouter"
[model_providers.openrouter]
name = "OpenRouter"
base_url = "$openrouter_base"
env_key = "OPENROUTER_API_KEY"
wire_api = "chat"
EOF_CONFIG

mkdir -p "$HOME/.factory"
cat > "$HOME/.factory/settings.json" <<EOF_FACTORY
{
  "customModels": [
    {
      "model": "$model",
      "displayName": "Hako Smoke OpenRouter",
      "baseUrl": "$openrouter_base",
      "apiKey": "\${OPENROUTER_API_KEY}",
      "provider": "generic-chat-completion-api",
      "maxOutputTokens": 4096
    }
  ]
}
EOF_FACTORY

mkdir -p "$HOME/.config/hermes-agent"
cat > "$HOME/.config/hermes-agent/config.json" <<EOF_HERMES
{
  "provider": "openrouter",
  "model": "$model",
  "api_key_env": "OPENROUTER_API_KEY",
  "base_url": "$openrouter_base"
}
EOF_HERMES

mkdir -p "$HOME/.qoder"
cat > "$HOME/.qoder/settings.json" <<EOF_QODER
{
  "general": {
    "enableAutoUpdate": false
  }
}
EOF_QODER

exec "$@"
