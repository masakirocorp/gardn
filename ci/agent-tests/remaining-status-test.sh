#!/usr/bin/env bash
set -euo pipefail
target="${OMH_REMAINING_STATUS_TARGET:-all}"
case "$target" in
  all|copilot|qoder|cursor|devin|droid|kimi|hermes) ;;
  *)
    echo "unknown remaining status test target: $target" >&2
    exit 2
    ;;
esac

target_selected() {
  [[ "$target" == "all" || "$target" == "$1" ]]
}

test_model_lib="${OMH_AGENT_TEST_MODELS_LIB:-/usr/local/lib/omh-agent-test-models.sh}"
seam_only="${OMH_REMAINING_STATUS_SEAM_ONLY:-0}"
needs_model=1
[[ "$target" == "devin" ]] && needs_model=0
if [[ -f "$test_model_lib" ]]; then
  source "$test_model_lib"
elif [[ "$seam_only" != "1" && "$needs_model" == "1" ]]; then
  echo "remaining status test needs $test_model_lib" >&2
  exit 1
fi
primary_model="${OMH_TEST_MODEL:-poolside/laguna-m.1:free}"
if [[ -z "${OMH_TEST_ACTIVE_MODEL:-}" && "$seam_only" != "1" && "$needs_model" == "1" ]]; then
  omh_test_unique_candidates "$primary_model" "${OMH_TEST_FALLBACK_MODELS:-}" \
    | omh_test_openrouter_api_candidates \
    | omh_test_non_openai_candidates \
    | omh_test_run_with_fallbacks "$0" OMH_TEST_MODEL "$@"
  exit $?
fi

model="${OMH_TEST_ACTIVE_MODEL:-$primary_model}"
repo_dir="${OMH_REPO_DIR:-/repo}"
workdir="${OMH_REMAINING_STATUS_TEST_DIR:-$(mktemp -d)}"
socket_path="$workdir/omh.sock"
request_log="$workdir/omh-requests.jsonl"


if [[ "$seam_only" != "1" && "$needs_model" == "1" ]]; then
  if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
    echo "remaining status test needs OPENROUTER_API_KEY" >&2
    exit 1
  fi
  if [[ "$model" == openai/* ]] || [[ "$model" == gpt-* ]]; then
    echo "remaining status test must use a non-OpenAI OpenRouter model, got: $model" >&2
    exit 1
  fi
fi

mkdir -p "$workdir"

python3 - "$socket_path" "$request_log" <<'PY' &
import json
import os
import socket
import sys
import time

socket_path, request_log = sys.argv[1:3]
try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass
server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(16)
server.settimeout(0.2)
deadline = time.time() + 300
with open(request_log, "a", encoding="utf-8") as out:
    while time.time() < deadline:
        try:
            conn, _ = server.accept()
        except TimeoutError:
            continue
        except OSError:
            break
        with conn:
            conn.settimeout(1)
            data = b""
            while not data.endswith(b"\n"):
                try:
                    chunk = conn.recv(4096)
                except TimeoutError:
                    break
                if not chunk:
                    break
                data += chunk
            if not data:
                continue
            out.write(data.decode("utf-8", "replace"))
            out.flush()
            try:
                request = json.loads(data)
                conn.sendall((json.dumps({"id": request.get("id"), "result": {"type": "ok"}}) + "\n").encode())
            except Exception:
                pass
PY
server_pid=$!
trap 'kill "$server_pid" >/dev/null 2>&1 || true' EXIT
for _ in $(seq 1 50); do
  [[ -S "$socket_path" ]] && break
  sleep 0.1
done
if [[ ! -S "$socket_path" ]]; then
  echo "fake omh socket did not start" >&2
  exit 1
fi


return_real_cli_failure() {
  local output="$1"
  local status="$2"
  cat "$output" >&2 || true
  if omh_test_retryable_status_or_output "$status" "$output"; then
    return 75
  fi
  return "$status"
}

install_copilot_real_hooks() {
  local home="$workdir/copilot-real/home"
  local hook="$home/hooks/omh-agent-state.sh"
  mkdir -p "$home/hooks"
  cp "$repo_dir/apps/omh/src/integration/assets/copilot/omh-agent-state.sh" "$hook"
  chmod +x "$hook"
  cat > "$home/settings.json" <<EOF_COPILOT
{
  "hooks": {
    "SessionStart": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "UserPromptSubmit": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "PreToolUse": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "PostToolUse": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "PostToolUseFailure": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "Stop": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "agentStop": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "SessionEnd": [{"type": "command", "command": "bash $hook", "timeout": 10}],
    "notification": [{"matcher": "permission_prompt|elicitation_dialog|agent_idle", "type": "command", "command": "bash $hook", "timeout": 10}]
  }
}
EOF_COPILOT
}

install_droid_real_hooks() {
  local hook="$repo_dir/apps/omh/src/integration/assets/droid/omh-agent-state.sh"
  local settings="${FACTORY_HOME:-$HOME/.factory}/settings.json"
  python3 - "$settings" "$hook" <<'PY'
import json, sys
from pathlib import Path
settings_path = Path(sys.argv[1])
hook = sys.argv[2]
settings = json.loads(settings_path.read_text())
hooks = settings.setdefault("hooks", {})
for event, action in [
    ("SessionStart", "idle"),
    ("UserPromptSubmit", "working"),
    ("PreToolUse", "working"),
    ("PermissionRequest", "blocked"),
    ("PostToolUse", "working"),
    ("PostToolUseFailure", "working"),
    ("PreCompact", "working"),
    ("PostCompact", "working"),
    ("Stop", "idle"),
    ("SessionEnd", "release"),
]:
    hooks[event] = [{"matcher": "*", "hooks": [{"type": "command", "command": f"bash {hook} {action}", "timeout": 10}]}]
settings_path.write_text(json.dumps(settings, indent=2))
PY
}

install_kimi_real_hooks() {
  local hook="$repo_dir/apps/omh/src/integration/assets/kimi/omh-agent-state.sh"
  local config="${KIMI_CODE_HOME:-$HOME/.kimi-code}/config.toml"
  cat >> "$config" <<EOF_KIMI_HOOKS

[[hooks]]
event = "SessionStart"
command = "bash $hook idle"
timeout = 10

[[hooks]]
event = "UserPromptSubmit"
command = "bash $hook working"
timeout = 10

[[hooks]]
event = "PreToolUse"
command = "bash $hook working"
timeout = 10

[[hooks]]
event = "PermissionRequest"
command = "bash $hook blocked"
timeout = 10

[[hooks]]
event = "Stop"
command = "bash $hook idle"
timeout = 10

[[hooks]]
event = "SessionEnd"
command = "bash $hook release"
timeout = 10
EOF_KIMI_HOOKS
}
run_copilot_cli() {
  local dir="$workdir/copilot-real"
  mkdir -p "$dir/run"
  set +e
  (
    cd "$dir/run"
    OMH_ENV=1 \
    OMH_SOCKET_PATH="$socket_path" \
    OMH_PANE_ID="pane-copilot-real" \
    COPILOT_PROVIDER_BASE_URL="https://openrouter.ai/api/v1" \
    COPILOT_PROVIDER_API_KEY="$OPENROUTER_API_KEY" \
    COPILOT_MODEL="$model" \
    COPILOT_PROVIDER_WIRE_API="responses" \
    COPILOT_HOME="$dir/home" \
    timeout "${OMH_REMAINING_STATUS_TEST_TIMEOUT:-180}" copilot \
      -p "Reply exactly OMH_COPILOT_STATUS_OK" \
      --output-format json \
      --allow-all \
      --model "$model" \
      --no-auto-update >"$dir/output.jsonl" 2>&1
  )
  local code=$?
  set -e
  if [[ "$code" -ne 0 ]]; then
    return_real_cli_failure "$dir/output.jsonl" "$code"
    return $?
  fi
  python3 - "$dir/output.jsonl" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "OMH_COPILOT_STATUS_OK" not in output:
    raise SystemExit("copilot real cli did not produce expected marker")
if "api.githubcopilot.com" in output or "api.openai.com" in output:
    raise SystemExit("copilot test used hosted Copilot/OpenAI routing")
PY
}

run_cursor_cli_or_auth_contract() {
  local dir="$workdir/cursor-real"
  mkdir -p "$dir/run"
  if [[ -n "${CURSOR_API_KEY:-}" ]]; then
    (
      cd "$dir/run"
      timeout "${OMH_REMAINING_STATUS_TEST_TIMEOUT:-180}" cursor-agent \
        --print \
        --output-format text \
        --trust \
        --api-key "$CURSOR_API_KEY" \
        --model "${OMH_TEST_CURSOR_MODEL:-$model}" \
        "Reply exactly OMH_CURSOR_STATUS_OK" >"$dir/output.txt" 2>&1
    )
    python3 - "$dir/output.txt" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "OMH_CURSOR_STATUS_OK" not in output:
    raise SystemExit(f"cursor real cli did not produce expected marker: {output[-1000:]}")
PY
    return
  fi

  set +e
  (
    cd "$dir/run"
    timeout 60 cursor-agent \
      --print \
      --output-format text \
      --trust \
      --api-key "$OPENROUTER_API_KEY" \
      --model "$model" \
      "Reply exactly OMH_CURSOR_STATUS_OK" >"$dir/output.txt" 2>&1
  )
  local code=$?
  set -e
  if [[ "$code" -eq 0 ]]; then
    echo "cursor accepted OPENROUTER_API_KEY as --api-key; add real test coverage instead of auth-contract coverage" >&2
    exit 1
  fi
  python3 - "$dir/output.txt" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "invalid" not in output.lower() and "api key" not in output.lower():
    raise SystemExit(f"cursor OpenRouter auth contract changed; observed: {output[-1000:]}")
PY
}

run_qoder_cli_or_auth_contract() {
  local dir="$workdir/qoder-real"
  mkdir -p "$dir/run"
  if [[ -n "${QODER_PERSONAL_ACCESS_TOKEN:-}" ]]; then
    (
      cd "$dir/run"
      timeout "${OMH_REMAINING_STATUS_TEST_TIMEOUT:-180}" qodercli \
        -p \
        --output-format json \
        --permission-mode dont_ask \
        --model "${OMH_TEST_QODER_MODEL:-$model}" \
        "Reply exactly OMH_QODER_STATUS_OK" >"$dir/output.jsonl" 2>&1
    )
    python3 - "$dir/output.jsonl" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "OMH_QODER_STATUS_OK" not in output:
    raise SystemExit(f"qodercli real cli did not produce expected marker: {output[-1000:]}")
PY
    return
  fi

  set +e
  (
    cd "$dir/run"
    timeout 60 qodercli \
      -p \
      --output-format json \
      --permission-mode dont_ask \
      --model "$model" \
      "Reply exactly OMH_QODER_STATUS_OK" >"$dir/output.jsonl" 2>&1
  )
  local code=$?
  set -e
  if [[ "$code" -eq 0 ]]; then
    echo "qodercli ran without QODER_PERSONAL_ACCESS_TOKEN; add real test coverage instead of auth-contract coverage" >&2
    exit 1
  fi
  python3 - "$dir/output.jsonl" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "not logged in" not in output.lower() and "login" not in output.lower():
    raise SystemExit(f"qodercli auth contract changed; observed: {output[-1000:]}")
PY
}

run_droid_cli() {
  local dir="$workdir/droid-real"
  mkdir -p "$dir/run"
  set +e
  (
    cd "$dir/run"
    OMH_ENV=1 \
    OMH_SOCKET_PATH="$socket_path" \
    OMH_PANE_ID="pane-droid-real" \
    DROID_HOME="${DROID_HOME:-$HOME/.factory}" \
    FACTORY_HOME="${FACTORY_HOME:-$HOME/.factory}" \
    timeout "${OMH_REMAINING_STATUS_TEST_TIMEOUT:-180}" droid exec \
      --model "$model" \
      --output-format json \
      --cwd "$dir/run" \
      "Reply exactly OMH_DROID_STATUS_OK" >"$dir/output.jsonl" 2>&1
  )
  local code=$?
  set -e
  if [[ "$code" -ne 0 ]]; then
    return_real_cli_failure "$dir/output.jsonl" "$code"
    return $?
  fi
  python3 - "$dir/output.jsonl" <<'PY'
import json
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "api.openai.com" in output:
    raise SystemExit("droid test used OpenAI routing")
for line in output.splitlines():
    try:
        item = json.loads(line)
    except json.JSONDecodeError:
        continue
    if item.get("type") == "result" and "OMH_DROID_STATUS_OK" in str(item.get("result", "")):
        break
else:
    raise SystemExit(f"droid real cli did not produce expected marker: {output[-1000:]}")
PY
}

run_kimi_cli() {
  local dir="$workdir/kimi-real"
  mkdir -p "$dir/run"
  set +e
  (
    cd "$dir/run"
    OMH_ENV=1 \
    OMH_SOCKET_PATH="$socket_path" \
    OMH_PANE_ID="pane-kimi-real" \
    KIMI_CODE_HOME="${KIMI_CODE_HOME:-$HOME/.kimi-code}" \
    timeout "${OMH_REMAINING_STATUS_TEST_TIMEOUT:-180}" kimi \
      -p "Reply exactly OMH_KIMI_STATUS_OK" \
      --output-format text >"$dir/output.txt" 2>&1
  )
  local code=$?
  set -e
  if [[ "$code" -ne 0 ]]; then
    return_real_cli_failure "$dir/output.txt" "$code"
    return $?
  fi
  python3 - "$dir/output.txt" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "OMH_KIMI_STATUS_OK" not in output:
    raise SystemExit(f"kimi real cli did not produce expected marker: {output[-1000:]}")
if "api.openai.com" in output:
    raise SystemExit("kimi test used OpenAI routing")
PY
}

install_hermes_real_plugin() {
  local dir="$HOME/.hermes"
  local plugin_dir="$dir/plugins/omh-agent-state"
  mkdir -p "$plugin_dir"
  cp "$repo_dir/apps/omh/src/integration/assets/hermes/__init__.py" "$plugin_dir/__init__.py"
  cp "$repo_dir/apps/omh/src/integration/assets/hermes/plugin.yaml" "$plugin_dir/plugin.yaml"
  python3 - "$dir/config.yaml" <<'PY'
import sys
from pathlib import Path

config_path = Path(sys.argv[1])
content = config_path.read_text() if config_path.exists() else ""
if "plugins:" not in content:
    if content and not content.endswith("\n"):
        content += "\n"
    content += "\nplugins:\n  enabled:\n    - omh-agent-state\n"
elif "omh-agent-state" not in content:
    if content and not content.endswith("\n"):
        content += "\n"
    content += "  enabled:\n    - omh-agent-state\n"
config_path.write_text(content)
PY
}

run_hermes_cli() {
  local dir="$workdir/hermes-real"
  mkdir -p "$dir/run"
  set +e
  (
    cd "$dir/run"
    OMH_ENV=1 \
    OMH_SOCKET_PATH="$socket_path" \
    OMH_PANE_ID="pane-hermes-real" \
    HERMES_ACCEPT_HOOKS=1 \
    timeout "${OMH_REMAINING_STATUS_TEST_TIMEOUT:-180}" hermes \
      -z "Reply exactly OMH_HERMES_STATUS_OK" \
      --provider openrouter \
      --model "$model" \
      --ignore-rules >"$dir/output.txt" 2>&1
  )
  local code=$?
  set -e
  if [[ "$code" -ne 0 ]]; then
    return_real_cli_failure "$dir/output.txt" "$code"
    return $?
  fi
  python3 - "$dir/output.txt" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "OMH_HERMES_STATUS_OK" not in output:
    raise SystemExit(f"hermes real cli did not produce expected marker: {output[-1000:]}")
if "api.openai.com" in output:
    raise SystemExit("hermes test used OpenAI routing")
PY
}

send_shell_hook() {
  local agent="$1"
  local pane_id="$2"
  local action="$3"
  local payload="$4"
  local hook="$repo_dir/apps/omh/src/integration/assets/$agent/omh-agent-state.sh"
  OMH_ENV=1 \
  OMH_SOCKET_PATH="$socket_path" \
  OMH_PANE_ID="$pane_id" \
  COPILOT_HOME="$workdir/copilot-home" \
  QODER_CONFIG_DIR="$workdir/qoder-home" \
  KIMI_CODE_HOME="$workdir/kimi-home" \
  DROID_HOME="$workdir/droid-home" \
  CURSOR_CONFIG_DIR="$workdir/cursor-home" \
  bash "$hook" "$action" <<<"$payload"
}

send_devin_hook() {
  local pane_id="$1"
  local project_dir="$2"
  local list_json="$3"
  local payload="$4"
  local hook="$repo_dir/apps/omh/src/integration/assets/devin/omh-agent-state.sh"
  OMH_ENV=1 \
  OMH_SOCKET_PATH="$socket_path" \
  OMH_PANE_ID="$pane_id" \
  DEVIN_PROJECT_DIR="$project_dir" \
  OMH_DEVIN_LIST_JSON="$list_json" \
  bash "$hook" session <<<"$payload"
}

send_hermes_hook() {
  local pane_id="$1"
  local fn_name="$2"
  local session_id="$3"
  OMH_ENV=1 \
  OMH_SOCKET_PATH="$socket_path" \
  OMH_PANE_ID="$pane_id" \
  HERMES_HOME="$workdir/hermes-home" \
  HERMES_PLUGIN_PATH="$repo_dir/apps/omh/src/integration/assets/hermes/__init__.py" \
  HERMES_FN="$fn_name" \
  HERMES_SESSION_ID="$session_id" \
  python3 - <<'PY'
import importlib.util
import os
spec = importlib.util.spec_from_file_location("omh_hermes", os.environ["HERMES_PLUGIN_PATH"])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
getattr(mod, os.environ["HERMES_FN"])(session_id=os.environ["HERMES_SESSION_ID"])
PY
}

if [[ "$seam_only" != "1" ]]; then
  if target_selected copilot; then
    install_copilot_real_hooks
    run_copilot_cli
  fi
  if target_selected cursor; then
    run_cursor_cli_or_auth_contract
  fi
  if target_selected qoder; then
    run_qoder_cli_or_auth_contract
  fi
  if target_selected droid; then
    install_droid_real_hooks
    run_droid_cli
  fi
  if target_selected kimi; then
    install_kimi_real_hooks
    run_kimi_cli
  fi
  if target_selected hermes; then
    install_hermes_real_plugin
    run_hermes_cli
  fi
fi

if target_selected copilot; then
  send_shell_hook copilot pane-copilot-allowed '' '{"hook_event_name":"SessionStart","session_id":"copilot-session","initial_prompt":"hello"}'
  send_shell_hook copilot pane-copilot-allowed '' '{"hook_event_name":"Stop","session_id":"copilot-session","stop_reason":"end_turn"}'
  send_shell_hook copilot pane-copilot-allowed '' '{"hook_event_name":"SessionEnd","session_id":"copilot-session","reason":"user_exit"}'
  send_shell_hook copilot pane-copilot-blocked '' '{"hook_event_name":"UserPromptSubmit","session_id":"blocked-copilot","prompt":"hello"}'
  send_shell_hook copilot pane-copilot-blocked '' '{"hook_event_name":"PreToolUse","session_id":"blocked-copilot","tool_name":"ask_user"}'
  send_shell_hook copilot pane-copilot-subagent '' '{"hook_event_name":"UserPromptSubmit","session_id":"copilot-parent","prompt":"hello"}'
  send_shell_hook copilot pane-copilot-subagent '' '{"hook_event_name":"agentStop","session_id":"copilot-parent","stop_reason":"end_turn"}'
fi

if target_selected qoder; then
  send_shell_hook qodercli pane-qoder-allowed idle '{"hook_event_name":"SessionStart","session_id":"qoder-session"}'
  send_shell_hook qodercli pane-qoder-allowed working '{"hook_event_name":"UserPromptSubmit","session_id":"qoder-session"}'
  send_shell_hook qodercli pane-qoder-allowed idle '{"hook_event_name":"Stop","session_id":"qoder-session"}'
  send_shell_hook qodercli pane-qoder-allowed release '{"hook_event_name":"SessionEnd","session_id":"qoder-session"}'
  send_shell_hook qodercli pane-qoder-blocked working '{"hook_event_name":"UserPromptSubmit","session_id":"qoder-blocked"}'
  send_shell_hook qodercli pane-qoder-blocked blocked '{"hook_event_name":"PermissionRequest","session_id":"qoder-blocked"}'
  send_shell_hook qodercli pane-qoder-subagent working '{"hook_event_name":"UserPromptSubmit","session_id":"qoder-parent"}'
  send_shell_hook qodercli pane-qoder-subagent idle '{"hook_event_name":"SubagentStop","session_id":"qoder-parent","agent_id":"child"}'
  send_shell_hook qodercli pane-qoder-subagent idle '{"hook_event_name":"Stop","session_id":"qoder-parent"}'
fi

if target_selected cursor; then
  send_shell_hook cursor pane-cursor-allowed idle '{"hook_event_name":"sessionStart","session_id":"cursor-session"}'
  send_shell_hook cursor pane-cursor-allowed working '{"hook_event_name":"beforeSubmitPrompt","session_id":"cursor-session"}'
  send_shell_hook cursor pane-cursor-allowed idle '{"hook_event_name":"stop","session_id":"cursor-session"}'
  send_shell_hook cursor pane-cursor-allowed release '{"hook_event_name":"sessionEnd","session_id":"cursor-session"}'
  send_shell_hook cursor pane-cursor-subagent working '{"hook_event_name":"beforeSubmitPrompt","session_id":"cursor-parent"}'
  send_shell_hook cursor pane-cursor-subagent working '{"hook_event_name":"beforeShellExecution","session_id":"cursor-parent","agent_id":"child"}'
  send_shell_hook cursor pane-cursor-subagent idle '{"hook_event_name":"stop","session_id":"cursor-parent"}'
fi

if target_selected devin; then
  send_devin_hook pane-devin-direct /tmp/omh-devin-project '[{"id":"stale-devin-direct","working_directory":"/tmp/omh-devin-project"}]' '{"hook_event_name":"SessionStart","session_id":"devin-direct","source":"startup"}'
  send_devin_hook pane-devin-list /tmp/omh-devin-project '[{"id":"other-devin","working_directory":"/tmp/omh-other-project"},{"id":"devin-list","working_directory":"/tmp/omh-devin-project"}]' '{"hook_event_name":"PreToolUse","tool_name":"exec"}'
  send_devin_hook pane-devin-prompt-stale /tmp/omh-devin-project '[{"id":"stale-prompt","working_directory":"/tmp/omh-devin-project"}]' '{"hook_event_name":"UserPromptSubmit","prompt":"run tests"}'
  send_devin_hook pane-devin-startup-stale /tmp/omh-devin-project '[{"id":"stale-startup","working_directory":"/tmp/omh-devin-project"}]' '{"hook_event_name":"SessionStart","source":"startup"}'
fi

if target_selected droid; then
  send_shell_hook droid pane-droid-allowed idle '{"hook_event_name":"SessionStart","session_id":"droid-session"}'
  send_shell_hook droid pane-droid-allowed working '{"hook_event_name":"UserPromptSubmit","session_id":"droid-session"}'
  send_shell_hook droid pane-droid-allowed idle '{"hook_event_name":"Stop","session_id":"droid-session"}'
  send_shell_hook droid pane-droid-allowed release '{"hook_event_name":"SessionEnd","session_id":"droid-session"}'
  send_shell_hook droid pane-droid-blocked working '{"hook_event_name":"UserPromptSubmit","session_id":"droid-blocked"}'
  send_shell_hook droid pane-droid-blocked blocked '{"hook_event_name":"PermissionRequest","session_id":"droid-blocked"}'
  send_shell_hook droid pane-droid-subagent working '{"hook_event_name":"UserPromptSubmit","session_id":"droid-parent"}'
  send_shell_hook droid pane-droid-subagent idle '{"hook_event_name":"SubagentStop","session_id":"droid-parent","agent_id":"child"}'
  send_shell_hook droid pane-droid-subagent idle '{"hook_event_name":"Stop","session_id":"droid-parent"}'
  send_shell_hook droid pane-droid-compact working '{"hook_event_name":"PreCompact","session_id":"droid-compact"}'
fi

if target_selected kimi; then
  send_shell_hook kimi pane-kimi-allowed idle '{"hook_event_name":"SessionStart","session_id":"kimi-session"}'
  send_shell_hook kimi pane-kimi-allowed working '{"hook_event_name":"UserPromptSubmit","session_id":"kimi-session"}'
  send_shell_hook kimi pane-kimi-allowed idle '{"hook_event_name":"Stop","session_id":"kimi-session"}'
  send_shell_hook kimi pane-kimi-allowed release '{"hook_event_name":"SessionEnd","session_id":"kimi-session"}'
  send_shell_hook kimi pane-kimi-blocked working '{"hook_event_name":"UserPromptSubmit","session_id":"kimi-blocked"}'
  send_shell_hook kimi pane-kimi-blocked blocked '{"hook_event_name":"PermissionRequest","session_id":"kimi-blocked"}'
  send_shell_hook kimi pane-kimi-subagent working '{"hook_event_name":"UserPromptSubmit","session_id":"kimi-parent"}'
  send_shell_hook kimi pane-kimi-subagent idle '{"hook_event_name":"SubagentStop","session_id":"kimi-parent","agent_id":"child"}'
  send_shell_hook kimi pane-kimi-subagent idle '{"hook_event_name":"Stop","session_id":"kimi-parent"}'
  send_shell_hook kimi pane-kimi-compact working '{"hook_event_name":"PreCompact","session_id":"kimi-compact"}'
fi

if target_selected hermes; then
  send_hermes_hook pane-hermes-allowed _working hermes-session
  send_hermes_hook pane-hermes-allowed _idle hermes-session
  send_hermes_hook pane-hermes-allowed _finalize hermes-session
  send_hermes_hook pane-hermes-blocked _working hermes-blocked
  send_hermes_hook pane-hermes-blocked _blocked hermes-blocked
  send_hermes_hook pane-hermes-compact _working hermes-compact
fi

python3 - "$request_log" "$seam_only" "$target" <<'PY'
import json
import sys
from pathlib import Path

requests = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
seam_only = sys.argv[2] == "1"
target = sys.argv[3]
reports = [req for req in requests if req.get("method") == "pane.report_agent"]
sessions = [req for req in requests if req.get("method") == "pane.report_agent_session"]
releases = [req for req in requests if req.get("method") == "pane.release_agent"]

def by_pane(items, pane):
    return [req for req in items if req.get("params", {}).get("pane_id") == pane]

def states(pane):
    return [req["params"].get("state") for req in by_pane(reports, pane)]

def assert_agent(pane, agent, source):
    items = by_pane(reports, pane) + by_pane(sessions, pane) + by_pane(releases, pane)
    if not items:
        raise SystemExit(f"{pane}: no Oh My Herdr reports")
    for req in items:
        params = req.get("params", {})
        assert params.get("pane_id") == pane, req
        assert params.get("agent") == agent, req
        assert params.get("source") == source, req
        assert isinstance(params.get("seq"), int), req

def assert_in_order(pane, expected):
    seen = states(pane)
    start = 0
    for state in expected:
        try:
            index = seen.index(state, start)
        except ValueError as exc:
            raise SystemExit(f"{pane}: missing {state}; observed {seen}") from exc
        start = index + 1

def assert_single_identity(pane):
    seen = {
        req.get("params", {}).get("agent_session_id")
        for req in by_pane(reports, pane) + by_pane(sessions, pane) + by_pane(releases, pane)
        if req.get("params", {}).get("agent_session_id")
    }
    if len(seen) != 1:
        raise SystemExit(f"{pane}: expected one session id, observed {sorted(seen)}")

def assert_devin_identity_only(pane, expected_session_id):
    pane_reports = by_pane(reports, pane)
    pane_sessions = by_pane(sessions, pane)
    pane_releases = by_pane(releases, pane)
    if pane_reports:
        raise SystemExit(f"{pane}: Devin hook must not emit lifecycle state reports; observed {pane_reports}")
    if pane_releases:
        raise SystemExit(f"{pane}: Devin hook must not release panes; observed {pane_releases}")
    if len(pane_sessions) != 1:
        raise SystemExit(f"{pane}: expected one Devin session identity report, observed {pane_sessions}")
    params = pane_sessions[0].get("params", {})
    if params.get("agent_session_id") != expected_session_id:
        raise SystemExit(f"{pane}: expected session {expected_session_id}, observed {params.get('agent_session_id')}")

def assert_no_pane_reports(pane):
    items = by_pane(reports, pane) + by_pane(sessions, pane) + by_pane(releases, pane)
    if items:
        raise SystemExit(f"{pane}: expected no Oh My Herdr reports, observed {items}")

expected_by_target = {
    "copilot": [
        ("pane-copilot-allowed", "copilot", "omh:copilot"),
        ("pane-copilot-blocked", "copilot", "omh:copilot"),
        ("pane-copilot-subagent", "copilot", "omh:copilot"),
    ],
    "qoder": [
        ("pane-qoder-allowed", "qodercli", "omh:qodercli"),
        ("pane-qoder-blocked", "qodercli", "omh:qodercli"),
        ("pane-qoder-subagent", "qodercli", "omh:qodercli"),
    ],
    "cursor": [
        ("pane-cursor-allowed", "cursor", "omh:cursor"),
        ("pane-cursor-subagent", "cursor", "omh:cursor"),
    ],
    "devin": [
        ("pane-devin-direct", "devin", "omh:devin"),
        ("pane-devin-list", "devin", "omh:devin"),
    ],
    "droid": [
        ("pane-droid-allowed", "droid", "omh:droid"),
        ("pane-droid-blocked", "droid", "omh:droid"),
        ("pane-droid-subagent", "droid", "omh:droid"),
        ("pane-droid-compact", "droid", "omh:droid"),
    ],
    "kimi": [
        ("pane-kimi-allowed", "kimi", "omh:kimi"),
        ("pane-kimi-blocked", "kimi", "omh:kimi"),
        ("pane-kimi-subagent", "kimi", "omh:kimi"),
        ("pane-kimi-compact", "kimi", "omh:kimi"),
    ],
    "hermes": [
        ("pane-hermes-allowed", "hermes", "omh:hermes"),
        ("pane-hermes-blocked", "hermes", "omh:hermes"),
        ("pane-hermes-compact", "hermes", "omh:hermes"),
    ],
}
selected_targets = list(expected_by_target) if target == "all" else [target]
expected_agents = [
    expected
    for selected in selected_targets
    for expected in expected_by_target[selected]
]
if not seam_only:
    real_by_target = {
        "copilot": ("pane-copilot-real", "copilot", "omh:copilot"),
        "droid": ("pane-droid-real", "droid", "omh:droid"),
        "kimi": ("pane-kimi-real", "kimi", "omh:kimi"),
        "hermes": ("pane-hermes-real", "hermes", "omh:hermes"),
    }
    expected_agents.extend(
        real_by_target[selected]
        for selected in selected_targets
        if selected in real_by_target
    )

for pane, agent, source in expected_agents:
    assert_agent(pane, agent, source)
    assert_single_identity(pane)

if "copilot" in selected_targets:
    if not seam_only:
        assert_in_order("pane-copilot-real", ["working", "idle"])
    assert_in_order("pane-copilot-allowed", ["working", "idle"])
    if not by_pane(releases, "pane-copilot-allowed"):
        raise SystemExit("pane-copilot-allowed: missing release")
    assert_in_order("pane-copilot-blocked", ["working", "blocked"])
    assert_in_order("pane-copilot-subagent", ["working", "idle"])

if "qoder" in selected_targets:
    assert_in_order("pane-qoder-allowed", ["idle", "working", "idle"])
    if not by_pane(releases, "pane-qoder-allowed"):
        raise SystemExit("pane-qoder-allowed: missing release")
    assert_in_order("pane-qoder-blocked", ["working", "blocked"])
    assert_in_order("pane-qoder-subagent", ["working", "idle"])
    if states("pane-qoder-subagent").count("idle") != 1:
        raise SystemExit(f"pane-qoder-subagent: child stop should not idle parent; observed {states('pane-qoder-subagent')}")

if "cursor" in selected_targets:
    assert_in_order("pane-cursor-allowed", ["idle", "working", "idle"])
    if not by_pane(releases, "pane-cursor-allowed"):
        raise SystemExit("pane-cursor-allowed: missing release")
    assert_in_order("pane-cursor-subagent", ["working", "idle"])

if "devin" in selected_targets:
    assert_devin_identity_only("pane-devin-direct", "devin-direct")
    assert_devin_identity_only("pane-devin-list", "devin-list")
    assert_no_pane_reports("pane-devin-prompt-stale")
    assert_no_pane_reports("pane-devin-startup-stale")

if "droid" in selected_targets:
    if not seam_only:
        assert_in_order("pane-droid-real", ["idle"])
        if not by_pane(releases, "pane-droid-real"):
            raise SystemExit("pane-droid-real: missing release")
    assert_in_order("pane-droid-allowed", ["idle", "working", "idle"])
    if not by_pane(releases, "pane-droid-allowed"):
        raise SystemExit("pane-droid-allowed: missing release")
    assert_in_order("pane-droid-blocked", ["working", "blocked"])
    assert_in_order("pane-droid-subagent", ["working", "idle"])
    if states("pane-droid-subagent").count("idle") != 1:
        raise SystemExit(f"pane-droid-subagent: child stop should not idle parent; observed {states('pane-droid-subagent')}")
    assert_in_order("pane-droid-compact", ["working"])

if "kimi" in selected_targets:
    if not seam_only:
        assert_in_order("pane-kimi-real", ["idle", "working", "idle"])
        if not by_pane(releases, "pane-kimi-real"):
            raise SystemExit("pane-kimi-real: missing release")
    assert_in_order("pane-kimi-allowed", ["idle", "working", "idle"])
    if not by_pane(releases, "pane-kimi-allowed"):
        raise SystemExit("pane-kimi-allowed: missing release")
    assert_in_order("pane-kimi-blocked", ["working", "blocked"])
    assert_in_order("pane-kimi-subagent", ["working", "idle"])
    if states("pane-kimi-subagent").count("idle") != 1:
        raise SystemExit(f"pane-kimi-subagent: child stop should not idle parent; observed {states('pane-kimi-subagent')}")
    assert_in_order("pane-kimi-compact", ["working"])

if "hermes" in selected_targets:
    if not seam_only:
        assert_in_order("pane-hermes-real", ["idle", "working", "idle"])
    assert_in_order("pane-hermes-allowed", ["working", "idle"])
    if not by_pane(releases, "pane-hermes-allowed"):
        raise SystemExit("pane-hermes-allowed: missing release")
    assert_in_order("pane-hermes-blocked", ["working", "blocked"])
    assert_in_order("pane-hermes-compact", ["working"])

scope = "all grouped agents" if target == "all" else target
mode = "seam" if seam_only else ("hook" if target == "devin" else "real")
print(f"remaining status test ok: target={scope}; mode={mode}")
PY
