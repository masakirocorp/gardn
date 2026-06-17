#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required for real CLI smoke runs" >&2
  exit 64
fi

model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"
fallback_models="${HAKO_SMOKE_FALLBACK_MODELS:-openrouter/free,openai/gpt-oss-120b:free,nvidia/nemotron-3-super-120b-a12b:free,openrouter/owl-alpha}"
openrouter_base="${OPENROUTER_BASE_URL:-https://openrouter.ai/api/v1}"

export HAKO_SMOKE_MODEL="$model"
export HAKO_OPENCODE_SMOKE_MODEL="${HAKO_OPENCODE_SMOKE_MODEL:-openrouter/openrouter/free}"
export HAKO_SMOKE_FALLBACK_MODELS="$fallback_models"
export OPENROUTER_API_KEY
export OPENAI_API_KEY="$OPENROUTER_API_KEY"
export ANTHROPIC_AUTH_TOKEN="$OPENROUTER_API_KEY"
export ANTHROPIC_BASE_URL="$openrouter_base"
export ANTHROPIC_MODEL="${HAKO_SMOKE_ANTHROPIC_MODEL:-$model}"
export COPILOT_PROVIDER_API_KEY="$OPENROUTER_API_KEY"
export COPILOT_PROVIDER_BASE_URL="$openrouter_base"
export COPILOT_MODEL="$model"
export OPENCODE_AUTH_CONTENT="{\"openrouter\":{\"type\":\"api\",\"key\":\"$OPENROUTER_API_KEY\"}}"

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
  ],
  "model": "$model",
  "cloudSessionSync": false
}
EOF_FACTORY

export KIMI_CODE_HOME="${KIMI_CODE_HOME:-$HOME/.kimi-code}"
mkdir -p "$KIMI_CODE_HOME"
cat > "$KIMI_CODE_HOME/config.toml" <<EOF_KIMI
default_model = "openrouter"

[providers.openrouter]
type = "openai"
base_url = "$openrouter_base"
api_key = "$OPENROUTER_API_KEY"

[models.openrouter]
provider = "openrouter"
model = "$model"
max_context_size = 128000
max_output_size = 4096
capabilities = ["tool_use"]
EOF_KIMI
export KIMI_DISABLE_TELEMETRY=1
export KIMI_CODE_NO_AUTO_UPDATE=1

mkdir -p "$HOME/.hermes"
cat > "$HOME/.hermes/.env" <<EOF_HERMES_ENV
OPENROUTER_API_KEY=$OPENROUTER_API_KEY
EOF_HERMES_ENV
cat > "$HOME/.hermes/config.yaml" <<EOF_HERMES
model:
  provider: openrouter
  default: "$model"
  base_url: "$openrouter_base"
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
