#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_openrouter_bare_candidates \
    | hako_smoke_non_anthropic_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_SMOKE_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
repo_dir="${HAKO_REPO_DIR:-/repo}"
hook_path="$repo_dir/src/integration/assets/claude/hako-agent-state.sh"
workdir="${HAKO_CLAUDE_STATUS_SMOKE_DIR:-$(mktemp -d)}"
socket_path="$workdir/hako.sock"
request_log="$workdir/hako-requests.jsonl"


if [[ ! -f "$hook_path" ]]; then
  echo "claude status test needs hako repo mounted at $repo_dir" >&2
  exit 1
fi
if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "claude status test needs OPENROUTER_API_KEY" >&2
  exit 1
fi
if [[ "$model" == anthropic/* ]] || [[ "$model" == claude* ]]; then
  echo "claude status test must use a non-Anthropic OpenRouter model, got: $model" >&2
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
                response = {"id": request.get("id"), "result": {"type": "ok"}}
                conn.sendall((json.dumps(response) + "\n").encode())
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

write_settings() {
  local path="$1"
  local config_dir="$2"
  cat > "$path" <<EOF_CONFIG
{
  "env": {
    "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
    "ANTHROPIC_AUTH_TOKEN": "${OPENROUTER_API_KEY}",
    "ANTHROPIC_API_KEY": "",
    "ANTHROPIC_MODEL": "${model}",
    "CLAUDE_CONFIG_DIR": "${config_dir}"
  },
  "hooks": {
    "SessionStart": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash ${hook_path} session"}]}],
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "SubagentStart": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "SubagentStop": [{"hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "PreCompact": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "PostCompact": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "PreToolUse": [{"hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "PostToolUse": [{"hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "PostToolUseFailure": [{"hooks": [{"type": "command", "command": "bash ${hook_path} working"}]}],
    "PermissionRequest": [{"hooks": [{"type": "command", "command": "bash ${hook_path} blocked"}]}],
    "Stop": [{"hooks": [{"type": "command", "command": "bash ${hook_path} idle"}]}],
    "SessionEnd": [{"hooks": [{"type": "command", "command": "bash ${hook_path} release"}]}],
    "Notification": [
      {"matcher": "permission_prompt|elicitation_dialog", "hooks": [{"type": "command", "command": "bash ${hook_path} blocked"}]},
      {"matcher": "idle_prompt", "hooks": [{"type": "command", "command": "bash ${hook_path} idle"}]}
    ]
  }
}
EOF_CONFIG
}

run_claude() {
  local pane_id="$1"
  local dir="$2"
  local title="$3"
  local tools="$4"
  local prompt="$5"

  mkdir -p "$dir/config" "$dir/run"
  write_settings "$dir/settings.json" "$dir/config"
  set +e
  (
    cd "$dir/run"
    HAKO_ENV=1 \
    HAKO_SOCKET_PATH="$socket_path" \
    HAKO_PANE_ID="$pane_id" \
    timeout "${HAKO_CLAUDE_STATUS_SMOKE_TIMEOUT:-180}" claude -p \
      --settings "$dir/settings.json" \
      --model "$model" \
      --output-format stream-json \
      --include-hook-events \
      --verbose \
      --tools "$tools" \
      --allowedTools "$tools" \
      --name "$title" \
      "$prompt" >"$dir/output.jsonl" 2>&1
  )
  local status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    if hako_smoke_retryable_status_or_output "$status" "$dir/output.jsonl"; then
      echo "$pane_id: retryable Claude/OpenRouter provider failure with $model" >&2
      exit 75
    fi
    return "$status"
  fi
  if grep -E 'api\.anthropic\.com|"model":"anthropic/|"model":"claude' "$dir/output.jsonl" >/dev/null; then
    echo "$pane_id: Claude smoke used Anthropic routing/model" >&2
    exit 1
  fi
}

run_claude \
  pane-claude-allowed \
  "$workdir/allowed" \
  hako-claude-status-working-idle \
  "" \
  'Reply exactly HAKO_CLAUDE_STATUS_IDLE.'

run_claude \
  pane-claude-subagent \
  "$workdir/subagent" \
  hako-claude-status-subagent \
  "Task" \
  'Use the Task tool to launch one subagent. The subagent must reply exactly CHILD_OK. After the subagent finishes, reply exactly HAKO_CLAUDE_SUBAGENT_DONE.'

# Permission and compaction are lifecycle hook seams. Claude print mode does not
# reliably block on interactive approval, so exercise the real installed hook with
# the same event payload shape the CLI sends.
HAKO_ENV=1 HAKO_SOCKET_PATH="$socket_path" HAKO_PANE_ID="pane-claude-blocked" bash "$hook_path" session <<'EOF_HOOK'
{"session_id":"blocked-session","hook_event_name":"SessionStart"}
EOF_HOOK
HAKO_ENV=1 HAKO_SOCKET_PATH="$socket_path" HAKO_PANE_ID="pane-claude-blocked" bash "$hook_path" working <<'EOF_HOOK'
{"session_id":"blocked-session","hook_event_name":"UserPromptSubmit"}
EOF_HOOK
HAKO_ENV=1 HAKO_SOCKET_PATH="$socket_path" HAKO_PANE_ID="pane-claude-blocked" bash "$hook_path" blocked <<'EOF_HOOK'
{"session_id":"blocked-session","hook_event_name":"PermissionRequest"}
EOF_HOOK
HAKO_ENV=1 HAKO_SOCKET_PATH="$socket_path" HAKO_PANE_ID="pane-claude-compact" bash "$hook_path" session <<'EOF_HOOK'
{"session_id":"compact-session","hook_event_name":"SessionStart"}
EOF_HOOK
HAKO_ENV=1 HAKO_SOCKET_PATH="$socket_path" HAKO_PANE_ID="pane-claude-compact" bash "$hook_path" working <<'EOF_HOOK'
{"session_id":"compact-session","hook_event_name":"PreCompact"}
EOF_HOOK

python3 - "$request_log" "$workdir" <<'PY'
import json
import sys
from pathlib import Path

request_log = Path(sys.argv[1])
workdir = Path(sys.argv[2])
requests = [json.loads(line) for line in request_log.read_text().splitlines() if line.strip()]
reports = [req for req in requests if req.get("method") == "pane.report_agent"]
sessions = [req for req in requests if req.get("method") == "pane.report_agent_session"]
releases = [req for req in requests if req.get("method") == "pane.release_agent"]


def reports_for(pane_id):
    return [req for req in reports if req.get("params", {}).get("pane_id") == pane_id]


def sessions_for(pane_id):
    return [req for req in sessions if req.get("params", {}).get("pane_id") == pane_id]


def releases_for(pane_id):
    return [req for req in releases if req.get("params", {}).get("pane_id") == pane_id]


def states_for(pane_id):
    return [req["params"].get("state") for req in reports_for(pane_id)]


def assert_common(pane_id):
    pane_reports = reports_for(pane_id)
    if not pane_reports:
        raise SystemExit(f"{pane_id}: no status reports")
    for req in pane_reports:
        params = req.get("params", {})
        assert params.get("pane_id") == pane_id, req
        assert params.get("source") == "hako:claude", req
        assert params.get("agent") == "claude", req
        assert isinstance(params.get("seq"), int), req
    if not sessions_for(pane_id):
        raise SystemExit(f"{pane_id}: no session reports")


def assert_contains_in_order(pane_id, expected):
    states = states_for(pane_id)
    start = 0
    for state in expected:
        try:
            found = states.index(state, start)
        except ValueError as exc:
            raise SystemExit(f"{pane_id}: missing {state} after {start}; observed {states}") from exc
        start = found + 1


def assert_single_session_identity(pane_id):
    seen = {
        req.get("params", {}).get("agent_session_id")
        for req in reports_for(pane_id) + sessions_for(pane_id) + releases_for(pane_id)
        if req.get("params", {}).get("agent_session_id")
    }
    if len(seen) != 1:
        raise SystemExit(f"{pane_id}: expected one session id, observed {sorted(seen)}")


def assert_output_contains(run_dir, marker):
    output = (workdir / run_dir / "output.jsonl").read_text(errors="replace")
    if marker not in output:
        raise SystemExit(f"{run_dir}: missing output marker {marker}")


for pane in (
    "pane-claude-allowed",
    "pane-claude-subagent",
    "pane-claude-blocked",
    "pane-claude-compact",
):
    assert_common(pane)
    assert_single_session_identity(pane)

assert_output_contains("allowed", "HAKO_CLAUDE_STATUS_IDLE")
assert_contains_in_order("pane-claude-allowed", ["working", "idle"])
if not releases_for("pane-claude-allowed"):
    raise SystemExit("pane-claude-allowed: missing release")

assert_output_contains("subagent", "CHILD_OK")
assert_output_contains("subagent", "HAKO_CLAUDE_SUBAGENT_DONE")
assert_contains_in_order("pane-claude-subagent", ["working", "idle"])
if not releases_for("pane-claude-subagent"):
    raise SystemExit("pane-claude-subagent: missing release")

assert_contains_in_order("pane-claude-blocked", ["working", "blocked"])
assert_contains_in_order("pane-claude-compact", ["working"])

print("claude status test ok: OpenRouter real cli reports working, idle, release, and subagent; hook seam covers blocked and compacting")
PY
