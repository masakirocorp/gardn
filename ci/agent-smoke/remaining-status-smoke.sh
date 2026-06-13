#!/usr/bin/env bash
set -euo pipefail

model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"
repo_dir="${HAKO_REPO_DIR:-/repo}"
workdir="${HAKO_REMAINING_STATUS_SMOKE_DIR:-$(mktemp -d)}"
socket_path="$workdir/hako.sock"
request_log="$workdir/hako-requests.jsonl"

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "remaining status test needs OPENROUTER_API_KEY" >&2
  exit 1
fi
if [[ "$model" == openai/* ]] || [[ "$model" == gpt-* ]]; then
  echo "remaining status test must use a non-OpenAI OpenRouter model, got: $model" >&2
  exit 1
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
  echo "fake hako socket did not start" >&2
  exit 1
fi

run_copilot_cli() {
  local dir="$workdir/copilot-real"
  mkdir -p "$dir/run"
  (
    cd "$dir/run"
    COPILOT_PROVIDER_BASE_URL="https://openrouter.ai/api/v1" \
    COPILOT_PROVIDER_API_KEY="$OPENROUTER_API_KEY" \
    COPILOT_MODEL="$model" \
    COPILOT_PROVIDER_WIRE_API="responses" \
    COPILOT_HOME="$dir/home" \
    timeout "${HAKO_REMAINING_STATUS_SMOKE_TIMEOUT:-180}" copilot \
      -p "Reply exactly HAKO_COPILOT_STATUS_OK" \
      --output-format json \
      --allow-all \
      --model "$model" \
      --no-auto-update >"$dir/output.jsonl" 2>&1
  )
  python3 - "$dir/output.jsonl" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "HAKO_COPILOT_STATUS_OK" not in output:
    raise SystemExit("copilot real cli did not produce expected marker")
if "api.githubcopilot.com" in output or "api.openai.com" in output:
    raise SystemExit("copilot smoke used hosted Copilot/OpenAI routing")
PY
}

send_shell_hook() {
  local agent="$1"
  local pane_id="$2"
  local action="$3"
  local payload="$4"
  local hook="$repo_dir/src/integration/assets/$agent/hako-agent-state.sh"
  HAKO_ENV=1 \
  HAKO_SOCKET_PATH="$socket_path" \
  HAKO_PANE_ID="$pane_id" \
  COPILOT_HOME="$workdir/copilot-home" \
  QODER_CONFIG_DIR="$workdir/qoder-home" \
  KIMI_CODE_HOME="$workdir/kimi-home" \
  DROID_HOME="$workdir/droid-home" \
  CURSOR_CONFIG_DIR="$workdir/cursor-home" \
  bash "$hook" "$action" <<<"$payload"
}

send_hermes_hook() {
  local pane_id="$1"
  local fn_name="$2"
  local session_id="$3"
  HAKO_ENV=1 \
  HAKO_SOCKET_PATH="$socket_path" \
  HAKO_PANE_ID="$pane_id" \
  HERMES_HOME="$workdir/hermes-home" \
  HERMES_PLUGIN_PATH="$repo_dir/src/integration/assets/hermes/__init__.py" \
  HERMES_FN="$fn_name" \
  HERMES_SESSION_ID="$session_id" \
  python3 - <<'PY'
import importlib.util
import os
spec = importlib.util.spec_from_file_location("hako_hermes", os.environ["HERMES_PLUGIN_PATH"])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
getattr(mod, os.environ["HERMES_FN"])(session_id=os.environ["HERMES_SESSION_ID"])
PY
}

run_copilot_cli

send_shell_hook copilot pane-copilot-allowed '' '{"hook_event_name":"SessionStart","session_id":"copilot-session","initial_prompt":"hello"}'
send_shell_hook copilot pane-copilot-allowed '' '{"hook_event_name":"Stop","session_id":"copilot-session","stop_reason":"end_turn"}'
send_shell_hook copilot pane-copilot-allowed '' '{"hook_event_name":"SessionEnd","session_id":"copilot-session","reason":"user_exit"}'
send_shell_hook copilot pane-copilot-blocked '' '{"hook_event_name":"UserPromptSubmit","session_id":"blocked-copilot","prompt":"hello"}'
send_shell_hook copilot pane-copilot-blocked '' '{"hook_event_name":"PreToolUse","session_id":"blocked-copilot","tool_name":"ask_user"}'
send_shell_hook copilot pane-copilot-subagent '' '{"hook_event_name":"UserPromptSubmit","session_id":"copilot-parent","prompt":"hello"}'
send_shell_hook copilot pane-copilot-subagent '' '{"hook_event_name":"agentStop","session_id":"copilot-parent","stop_reason":"end_turn"}'

send_shell_hook qodercli pane-qoder-allowed idle '{"hook_event_name":"SessionStart","session_id":"qoder-session"}'
send_shell_hook qodercli pane-qoder-allowed working '{"hook_event_name":"UserPromptSubmit","session_id":"qoder-session"}'
send_shell_hook qodercli pane-qoder-allowed idle '{"hook_event_name":"Stop","session_id":"qoder-session"}'
send_shell_hook qodercli pane-qoder-allowed release '{"hook_event_name":"SessionEnd","session_id":"qoder-session"}'
send_shell_hook qodercli pane-qoder-blocked working '{"hook_event_name":"UserPromptSubmit","session_id":"qoder-blocked"}'
send_shell_hook qodercli pane-qoder-blocked blocked '{"hook_event_name":"PermissionRequest","session_id":"qoder-blocked"}'
send_shell_hook qodercli pane-qoder-subagent working '{"hook_event_name":"UserPromptSubmit","session_id":"qoder-parent"}'
send_shell_hook qodercli pane-qoder-subagent idle '{"hook_event_name":"SubagentStop","session_id":"qoder-parent","agent_id":"child"}'
send_shell_hook qodercli pane-qoder-subagent idle '{"hook_event_name":"Stop","session_id":"qoder-parent"}'

send_shell_hook cursor pane-cursor-allowed idle '{"hook_event_name":"sessionStart","session_id":"cursor-session"}'
send_shell_hook cursor pane-cursor-allowed working '{"hook_event_name":"beforeSubmitPrompt","session_id":"cursor-session"}'
send_shell_hook cursor pane-cursor-allowed idle '{"hook_event_name":"stop","session_id":"cursor-session"}'
send_shell_hook cursor pane-cursor-allowed release '{"hook_event_name":"sessionEnd","session_id":"cursor-session"}'
send_shell_hook cursor pane-cursor-subagent working '{"hook_event_name":"beforeSubmitPrompt","session_id":"cursor-parent"}'
send_shell_hook cursor pane-cursor-subagent working '{"hook_event_name":"beforeShellExecution","session_id":"cursor-parent","agent_id":"child"}'
send_shell_hook cursor pane-cursor-subagent idle '{"hook_event_name":"stop","session_id":"cursor-parent"}'
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

send_hermes_hook pane-hermes-allowed _working hermes-session
send_hermes_hook pane-hermes-allowed _idle hermes-session
send_hermes_hook pane-hermes-allowed _finalize hermes-session
send_hermes_hook pane-hermes-blocked _working hermes-blocked
send_hermes_hook pane-hermes-blocked _blocked hermes-blocked
send_hermes_hook pane-hermes-compact _working hermes-compact

python3 - "$request_log" <<'PY'
import json
import sys
from pathlib import Path

requests = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
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
        raise SystemExit(f"{pane}: no Hako reports")
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

for pane, agent, source in [
    ("pane-copilot-allowed", "copilot", "hako:copilot"),
    ("pane-copilot-blocked", "copilot", "hako:copilot"),
    ("pane-copilot-subagent", "copilot", "hako:copilot"),
    ("pane-qoder-allowed", "qodercli", "hako:qodercli"),
    ("pane-qoder-blocked", "qodercli", "hako:qodercli"),
    ("pane-qoder-subagent", "qodercli", "hako:qodercli"),
    ("pane-cursor-allowed", "cursor", "hako:cursor"),
    ("pane-cursor-subagent", "cursor", "hako:cursor"),
    ("pane-droid-allowed", "droid", "hako:droid"),
    ("pane-droid-blocked", "droid", "hako:droid"),
    ("pane-droid-subagent", "droid", "hako:droid"),
    ("pane-droid-compact", "droid", "hako:droid"),
    ("pane-kimi-allowed", "kimi", "hako:kimi"),
    ("pane-kimi-blocked", "kimi", "hako:kimi"),
    ("pane-kimi-subagent", "kimi", "hako:kimi"),
    ("pane-kimi-compact", "kimi", "hako:kimi"),
    ("pane-hermes-allowed", "hermes", "hako:hermes"),
    ("pane-hermes-blocked", "hermes", "hako:hermes"),
    ("pane-hermes-compact", "hermes", "hako:hermes"),
]:
    assert_agent(pane, agent, source)
    assert_single_identity(pane)

assert_in_order("pane-copilot-allowed", ["working", "idle"])
if not by_pane(releases, "pane-copilot-allowed"):
    raise SystemExit("pane-copilot-allowed: missing release")
assert_in_order("pane-copilot-blocked", ["working", "blocked"])
assert_in_order("pane-copilot-subagent", ["working", "idle"])

assert_in_order("pane-qoder-allowed", ["idle", "working", "idle"])
if not by_pane(releases, "pane-qoder-allowed"):
    raise SystemExit("pane-qoder-allowed: missing release")
assert_in_order("pane-qoder-blocked", ["working", "blocked"])
assert_in_order("pane-qoder-subagent", ["working", "idle"])
if states("pane-qoder-subagent").count("idle") != 1:
    raise SystemExit(f"pane-qoder-subagent: child stop should not idle parent; observed {states('pane-qoder-subagent')}")

assert_in_order("pane-cursor-allowed", ["idle", "working", "idle"])
if not by_pane(releases, "pane-cursor-allowed"):
    raise SystemExit("pane-cursor-allowed: missing release")
assert_in_order("pane-cursor-subagent", ["working", "idle"])

assert_in_order("pane-droid-allowed", ["idle", "working", "idle"])
if not by_pane(releases, "pane-droid-allowed"):
    raise SystemExit("pane-droid-allowed: missing release")
assert_in_order("pane-droid-blocked", ["working", "blocked"])
assert_in_order("pane-droid-subagent", ["working", "idle"])
if states("pane-droid-subagent").count("idle") != 1:
    raise SystemExit(f"pane-droid-subagent: child stop should not idle parent; observed {states('pane-droid-subagent')}")
assert_in_order("pane-droid-compact", ["working"])

assert_in_order("pane-kimi-allowed", ["idle", "working", "idle"])
if not by_pane(releases, "pane-kimi-allowed"):
    raise SystemExit("pane-kimi-allowed: missing release")
assert_in_order("pane-kimi-blocked", ["working", "blocked"])
assert_in_order("pane-kimi-subagent", ["working", "idle"])
if states("pane-kimi-subagent").count("idle") != 1:
    raise SystemExit(f"pane-kimi-subagent: child stop should not idle parent; observed {states('pane-kimi-subagent')}")
assert_in_order("pane-kimi-compact", ["working"])

assert_in_order("pane-hermes-allowed", ["working", "idle"])
if not by_pane(releases, "pane-hermes-allowed"):
    raise SystemExit("pane-hermes-allowed: missing release")
assert_in_order("pane-hermes-blocked", ["working", "blocked"])
assert_in_order("pane-hermes-compact", ["working"])

print("remaining status test ok: Copilot OpenRouter real cli works; Copilot, Cursor, qodercli, droid, kimi, and Hermes state hooks align")
PY
