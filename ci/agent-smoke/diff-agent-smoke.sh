#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
agent="${1:-}"
if [[ -z "$agent" ]]; then
  echo "usage: hako-agent-smoke-diff-agent <agent>" >&2
  exit 2
fi
case "$agent" in
  opencode) primary_model="${HAKO_OPENCODE_SMOKE_MODEL:-openrouter/openrouter/free}"; candidate_filter=hako_smoke_opencode_candidates; model_env=HAKO_OPENCODE_SMOKE_MODEL ;;
  claude) primary_model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"; candidate_filter=hako_smoke_openrouter_bare_candidates; model_env=HAKO_SMOKE_MODEL ;;
  codex|copilot|droid|kimi|hermes|pi|omp) primary_model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"; candidate_filter=hako_smoke_openrouter_bare_candidates; model_env=HAKO_SMOKE_MODEL ;;
  kiro) primary_model="kiro"; candidate_filter=cat; model_env=KIRO_API_KEY ;;
  *) echo "unsupported diff-agent smoke: $agent" >&2; exit 2 ;;
esac
if [[ "$agent" == "kiro" ]]; then
  if [[ -z "${KIRO_API_KEY:-}" ]]; then
    echo "kiro diff-agent smoke needs KIRO_API_KEY; Kiro does not support OpenRouter BYOK for CI" >&2
    exit 64
  fi
  export HAKO_SMOKE_ACTIVE_MODEL=kiro
fi
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  case "$agent" in
    opencode)
      hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
        | hako_smoke_opencode_candidates \
        | hako_smoke_run_with_fallbacks "$0" "$model_env" "$agent"
      ;;
    claude)
      hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
        | hako_smoke_openrouter_bare_candidates \
        | hako_smoke_non_anthropic_candidates \
        | hako_smoke_run_with_fallbacks "$0" "$model_env" "$agent"
      ;;
    codex|copilot|droid)
      hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
        | hako_smoke_openrouter_bare_candidates \
        | hako_smoke_non_openai_candidates \
        | hako_smoke_run_with_fallbacks "$0" "$model_env" "$agent"
      ;;
    *)
      hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
        | hako_smoke_openrouter_bare_candidates \
        | hako_smoke_run_with_fallbacks "$0" "$model_env" "$agent"
      ;;
  esac
  exit $?
fi
model="$HAKO_SMOKE_ACTIVE_MODEL"
repo_dir="${HAKO_REPO_DIR:-/repo}"
workdir="${HAKO_DIFF_AGENT_SMOKE_DIR:-$(mktemp -d)}"
mkdir -p "$workdir/$agent/run"
prompt_file="$workdir/$agent/prompt.txt"
output_file="$workdir/$agent/output.txt"
cat > "$prompt_file" <<'EOF_PROMPT'
You are receiving a Hako native diff payload that was sent to an agent pane.
If the selected hunk changes the Rust function return value from "before" to "after", reply exactly HAKO_DIFF_AGENT_PAYLOAD_OK and nothing else.

Repo: /tmp/hako-diff-agent-smoke
Branch: main
Scope: all
File: src/lib.rs
Bucket: unstaged
Status: modified
Shape: selected hunk

```diff
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn label() -> &'static str {
-    "before"
+    "after"
 }
```
EOF_PROMPT
prompt="$(<"$prompt_file")"

run_opencode() {
  cat > "$workdir/$agent/run/opencode.json" <<EOF_CONFIG
{"\$schema":"https://opencode.ai/config.json","model":"$model","small_model":"$model","permission":{"bash":{"*":"deny"}}}
EOF_CONFIG
  timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" opencode run --dir "$workdir/$agent/run" --model "$model" --format default --title hako-diff-agent-payload "$prompt" >"$output_file" 2>&1
}

run_claude() {
  local config_dir="$workdir/$agent/config"
  mkdir -p "$config_dir"
  cat > "$workdir/$agent/settings.json" <<EOF_CONFIG
{"env":{"ANTHROPIC_BASE_URL":"https://openrouter.ai/api","ANTHROPIC_AUTH_TOKEN":"${OPENROUTER_API_KEY}","ANTHROPIC_API_KEY":"","ANTHROPIC_MODEL":"${model}","CLAUDE_CONFIG_DIR":"${config_dir}"}}
EOF_CONFIG
  (cd "$workdir/$agent/run" && timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" claude -p --settings "$workdir/$agent/settings.json" --model "$model" --output-format stream-json --verbose --name hako-diff-agent-payload "$prompt" >"$output_file" 2>&1)
}

run_codex() {
  local home="$workdir/$agent/codex"
  mkdir -p "$home"
  git -C "$workdir/$agent/run" init --quiet
  cat > "$home/config.toml" <<EOF_CONFIG
model = "${model}"
model_provider = "openrouter"
approval_policy = "never"
sandbox_mode = "workspace-write"
[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
EOF_CONFIG
  (cd "$workdir/$agent/run" && CODEX_HOME="$home" timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" codex exec --cd "$workdir/$agent/run" --model "$model" "$prompt" >"$output_file" 2>&1)
}

run_pi_omp() {
  local bin="$1"
  local extension="$repo_dir/src/integration/assets/$bin/hako-agent-state.ts"
  (cd "$workdir/$agent/run" && PI_CONFIG_DIR="$workdir/$agent/config" PI_CODING_AGENT_DIR="$workdir/$agent/agent" timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" "$bin" -p --model "openrouter/$model" --tools none --auto-approve -e "$extension" "$prompt" >"$output_file" 2>&1)
}

run_copilot() {
  (cd "$workdir/$agent/run" && COPILOT_PROVIDER_BASE_URL="https://openrouter.ai/api/v1" COPILOT_PROVIDER_API_KEY="$OPENROUTER_API_KEY" COPILOT_MODEL="$model" COPILOT_PROVIDER_WIRE_API="responses" COPILOT_HOME="$workdir/$agent/home" timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" copilot -p "$prompt" --output-format json --allow-all --model "$model" --no-auto-update >"$output_file" 2>&1)
}

run_droid() {
  (cd "$workdir/$agent/run" && DROID_HOME="${DROID_HOME:-$HOME/.factory}" FACTORY_HOME="${FACTORY_HOME:-$HOME/.factory}" timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" droid exec --model "$model" --output-format json --cwd "$workdir/$agent/run" "$prompt" >"$output_file" 2>&1)
}

run_kimi() {
  (cd "$workdir/$agent/run" && KIMI_CODE_HOME="${KIMI_CODE_HOME:-$HOME/.kimi-code}" timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" kimi -p "$prompt" --output-format text >"$output_file" 2>&1)
}

run_hermes() {
  (cd "$workdir/$agent/run" && HERMES_ACCEPT_HOOKS=1 timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" hermes -z "$prompt" --provider openrouter --model "$model" --ignore-rules >"$output_file" 2>&1)
}

run_kiro() {
  (cd "$workdir/$agent/run" && timeout "${HAKO_DIFF_AGENT_SMOKE_TIMEOUT:-180}" kiro-cli chat --no-interactive "$prompt" >"$output_file" 2>&1)
}

set +e
case "$agent" in
  opencode) run_opencode ;;
  claude) run_claude ;;
  codex) run_codex ;;
  pi) run_pi_omp pi ;;
  omp) run_pi_omp omp ;;
  copilot) run_copilot ;;
  droid) run_droid ;;
  kimi) run_kimi ;;
  hermes) run_hermes ;;
  kiro) run_kiro ;;
esac
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  if hako_smoke_retryable_status_or_output "$status" "$output_file"; then
    echo "$agent diff-agent smoke retryable provider/model failure with $model" >&2
    sed -n '1,160p' "$output_file" >&2
    exit 75
  fi
  echo "$agent diff-agent smoke failed with status $status" >&2
  sed -n '1,200p' "$output_file" >&2
  exit "$status"
fi
if ! grep -q 'HAKO_DIFF_AGENT_PAYLOAD_OK' "$output_file"; then
  echo "$agent diff-agent smoke missing expected marker" >&2
  sed -n '1,200p' "$output_file" >&2
  exit 1
fi
if grep -E 'api\.openai\.com|api\.anthropic\.com' "$output_file" >/dev/null; then
  echo "$agent diff-agent smoke used a first-party hosted model route" >&2
  sed -n '1,200p' "$output_file" >&2
  exit 1
fi
printf '%s diff-agent smoke ok: live agent understood hako selected-hunk payload\n' "$agent"
