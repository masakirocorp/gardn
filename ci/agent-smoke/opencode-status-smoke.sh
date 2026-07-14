#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_OPENCODE_SMOKE_MODEL:-openrouter/openrouter/free}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_opencode_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_OPENCODE_SMOKE_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
repo_dir="${HAKO_REPO_DIR:-/repo}"
plugin_path="$repo_dir/apps/hako/src/integration/assets/opencode/hako-agent-state.js"
workdir="${HAKO_OPENCODE_STATUS_SMOKE_DIR:-$(mktemp -d)}"
socket_path="$workdir/hako.sock"
request_log="$workdir/hako-requests.jsonl"


if [[ ! -f "$plugin_path" ]]; then
  echo "opencode status test needs hako repo mounted at $repo_dir" >&2
  exit 1
fi

hako-agent-opencode-plugin-status-test "$plugin_path"

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

run_opencode() {
  local pane_id="$1"
  local dir="$2"
  local title="$3"
  local bash_permission="$4"
  local prompt="$5"

  mkdir -p "$dir"
  cat > "$dir/opencode.json" <<EOF_CONFIG
{
  "\$schema": "https://opencode.ai/config.json",
  "model": "$model",
  "small_model": "$model",
  "plugin": ["file://$plugin_path"],
  "permission": { "bash": "$bash_permission" },
  "agent": {
    "general": {
      "mode": "subagent",
      "model": "$model",
      "tools": { "bash": true, "task": true }
    }
  }
}
EOF_CONFIG

  set +e
  HAKO_ENV=1 \
  HAKO_SOCKET_PATH="$socket_path" \
  HAKO_PANE_ID="$pane_id" \
  timeout "${HAKO_OPENCODE_STATUS_SMOKE_TIMEOUT:-180}" opencode run \
    --dir "$dir" \
    --model "$model" \
    --format json \
    --title "$title" \
    "$prompt" >"$dir/output.jsonl" 2>&1
  local status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    if hako_smoke_retryable_status_or_output "$status" "$dir/output.jsonl"; then
      echo "$pane_id: retryable OpenCode provider/model failure with $model" >&2
      exit 75
    fi
    return "$status"
  fi
}

run_opencode \
  pane-opencode-allowed \
  "$workdir/allowed" \
  hako-opencode-status-working-idle \
  allow \
  'Run the shell command: printf HAKO_OPENCODE_STATUS_WORKING. Then reply with exactly HAKO_OPENCODE_STATUS_IDLE.'

run_opencode \
  pane-opencode-blocked \
  "$workdir/blocked" \
  hako-opencode-status-blocked \
  ask \
  'Run the shell command: printf HAKO_OPENCODE_STATUS_BLOCKED. Do not reply until it runs.'

run_opencode \
  pane-opencode-subagent \
  "$workdir/subagent" \
  hako-opencode-status-subagent \
  allow \
  'Use the task tool to launch one general subagent. The subagent must run shell command: printf HAKO_OPENCODE_SUBAGENT_OK. After the subagent finishes, reply exactly HAKO_OPENCODE_SUBAGENT_DONE.'

python3 - "$request_log" "$workdir" <<'PY'
import json
import sys
from pathlib import Path

request_log = Path(sys.argv[1])
workdir = Path(sys.argv[2])
requests = [json.loads(line) for line in request_log.read_text().splitlines() if line.strip()]
reports = [req for req in requests if req.get("method") == "pane.report_agent"]
sessions = [req for req in requests if req.get("method") == "pane.report_agent_session"]


def reports_for(pane_id):
    return [req for req in reports if req.get("params", {}).get("pane_id") == pane_id]


def session_reports_for(pane_id):
    return [req for req in sessions if req.get("params", {}).get("pane_id") == pane_id]


def states_for(pane_id):
    return [req["params"].get("state") for req in reports_for(pane_id)]


def assert_common(pane_id):
    pane_reports = reports_for(pane_id)
    if not pane_reports:
        raise SystemExit(f"{pane_id}: no status reports")
    for req in pane_reports:
        params = req.get("params", {})
        assert params.get("pane_id") == pane_id, req
        assert params.get("source") == "hako:opencode", req
        assert params.get("agent") == "opencode", req
        assert isinstance(params.get("seq"), int), req
    pane_sessions = session_reports_for(pane_id)
    if not pane_sessions:
        raise SystemExit(f"{pane_id}: no session reports")
    if not any(req.get("params", {}).get("agent_session_id") for req in pane_sessions):
        raise SystemExit(f"{pane_id}: no session id")


def assert_contains_in_order(pane_id, expected):
    states = states_for(pane_id)
    start = 0
    for state in expected:
        try:
            found = states.index(state, start)
        except ValueError as exc:
            raise SystemExit(f"{pane_id}: missing {state} after {start}; observed {states}") from exc
        start = found + 1


def assert_eventually_idle(pane_id):
    states = states_for(pane_id)
    if states[-1] != "idle":
        raise SystemExit(f"{pane_id}: final report should be idle; observed {states}")


def assert_no_reactivation_after_idle(pane_id):
    states = states_for(pane_id)
    if "idle" not in states:
        raise SystemExit(f"{pane_id}: no idle report; observed {states}")
    first_idle = states.index("idle")
    later_active = [state for state in states[first_idle + 1 :] if state != "idle"]
    if later_active:
        raise SystemExit(f"{pane_id}: active reports after idle {later_active}; observed {states}")


def assert_single_session_identity(pane_id):
    seen = {
        req.get("params", {}).get("agent_session_id")
        for req in reports_for(pane_id) + session_reports_for(pane_id)
        if req.get("params", {}).get("agent_session_id")
    }
    if len(seen) != 1:
        raise SystemExit(f"{pane_id}: expected one parent session id, observed {sorted(seen)}")


def assert_output_contains(run_dir, marker):
    output = (workdir / run_dir / "output.jsonl").read_text(errors="replace")
    if marker not in output:
        print(f"{run_dir}: missing output marker {marker}", file=sys.stderr)
        raise SystemExit(75)


for pane in ("pane-opencode-allowed", "pane-opencode-blocked", "pane-opencode-subagent"):
    assert_common(pane)
    assert_single_session_identity(pane)

assert_output_contains("allowed", "HAKO_OPENCODE_STATUS_IDLE")
assert_contains_in_order("pane-opencode-allowed", ["working"])

assert_contains_in_order("pane-opencode-blocked", ["working", "blocked"])
if "idle" in states_for("pane-opencode-blocked"):
    assert_eventually_idle("pane-opencode-blocked")

assert_output_contains("subagent", "HAKO_OPENCODE_SUBAGENT_OK")
assert_output_contains("subagent", "HAKO_OPENCODE_SUBAGENT_DONE")
assert_contains_in_order("pane-opencode-subagent", ["working"])
if "idle" in states_for("pane-opencode-subagent"):
    assert_eventually_idle("pane-opencode-subagent")
    assert_no_reactivation_after_idle("pane-opencode-subagent")

print("opencode status test ok: real cli reports working/blocked/subagent; plugin harness covers compacting, idle, and parent-session authority")
PY
