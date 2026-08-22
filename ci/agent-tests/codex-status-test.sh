#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/gardn-agent-test-models.sh
primary_model="${GARDN_TEST_MODEL:-$GARDN_TEST_DEFAULT_MODEL}"
if [[ -z "${GARDN_TEST_ACTIVE_MODEL:-}" ]]; then
  gardn_test_unique_candidates "$primary_model" "${GARDN_TEST_FALLBACK_MODELS:-}" \
    | gardn_test_available_candidates \
    | gardn_test_non_openai_candidates \
    | gardn_test_run_with_fallbacks "$0" "$@"
  exit $?
fi

model="$GARDN_TEST_ACTIVE_MODEL"
gardn_test_configure_model "$model"
repo_dir="${GARDN_REPO_DIR:-/repo}"
hook_path="$repo_dir/apps/gardn/src/integration/assets/codex/gardn-agent-state.sh"
workdir="${GARDN_CODEX_STATUS_TEST_DIR:-$(mktemp -d)}"
socket_path="$workdir/gardn.sock"
request_log="$workdir/gardn-requests.jsonl"


if [[ ! -f "$hook_path" ]]; then
  echo "codex status test needs gardn repo mounted at $repo_dir" >&2
  exit 1
fi
if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "codex status test needs OPENROUTER_API_KEY" >&2
  exit 1
fi
if [[ "$model" == openai/* ]] || [[ "$model" == gpt-* ]]; then
  echo "codex status test must use a non-OpenAI OpenRouter model, got: $model" >&2
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
  echo "fake gardn socket did not start" >&2
  exit 1
fi

# TODO: Replace the seam-driven status assertions below with real `codex exec`
# hook assertions once upstream dispatches hooks in exec mode. Codex currently
# documents hooks for config layers, but openai/codex#26452 and #26383 track
# that `codex exec` does not dispatch valid hooks.json/config.toml hooks. Until
# that is fixed upstream, this test can only prove real Codex OpenRouter
# transport plus Gardn's hook-script behavior through direct invocation.

run_codex_cli() {
  local dir="$workdir/real-cli"
  mkdir -p "$dir/codex" "$dir/run"
  git -C "$dir/run" init --quiet
  cat > "$dir/codex/config.toml" <<EOF_CONFIG
model = "${model}"
model_provider = "openrouter"
approval_policy = "never"
sandbox_mode = "workspace-write"

[features]
multi_agent = false

[model_providers.openrouter]
name = "OpenRouter"
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
wire_api = "responses"
EOF_CONFIG
  (
    cd "$dir/run"
    set +e
    CODEX_HOME="$dir/codex" \
    timeout "${GARDN_CODEX_STATUS_TEST_TIMEOUT:-180}" codex exec \
      --cd "$dir/run" \
      --model "$model" \
      'Reply exactly GARDN_CODEX_STATUS_OK.' >"$dir/output.txt" 2>&1
    local status=$?
    set -e
    if [[ "$status" -ne 0 ]]; then
      cat "$dir/output.txt" >&2
      if gardn_test_retryable_status_or_output "$status" "$dir/output.txt"; then
        echo "retryable Codex/OpenRouter provider failure with $model" >&2
        exit 75
      fi
      return "$status"
    fi
  )
  python3 - "$dir/output.txt" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors="replace")
if "GARDN_CODEX_STATUS_OK" not in output:
    raise SystemExit("codex real cli did not produce expected marker")
if "api.openai.com" in output or '"provider":"openai"' in output:
    raise SystemExit("codex test used OpenAI routing")
if "provider: openrouter" not in output:
    raise SystemExit("codex test did not report OpenRouter provider")
PY
}

send_hook() {
  local pane_id="$1"
  local action="$2"
  local payload="$3"
  GARDN_ENV=1 \
  GARDN_SOCKET_PATH="$socket_path" \
  GARDN_PANE_ID="$pane_id" \
  CODEX_HOME="$workdir/codex-profile" \
  bash "$hook_path" "$action" <<<"$payload"
}

run_codex_cli

send_hook pane-codex-allowed session '{"session_id":"codex-session","transcript_path":"/tmp/codex-session.jsonl","hook_event_name":"SessionStart"}'
send_hook pane-codex-allowed working '{"session_id":"codex-session","hook_event_name":"UserPromptSubmit"}'
send_hook pane-codex-allowed idle '{"session_id":"codex-session","hook_event_name":"Stop"}'
send_hook pane-codex-allowed release '{"session_id":"codex-session","hook_event_name":"SessionEnd"}'

send_hook pane-codex-blocked session '{"session_id":"blocked-session","transcript_path":"/tmp/blocked-session.jsonl","hook_event_name":"SessionStart"}'
send_hook pane-codex-blocked working '{"session_id":"blocked-session","hook_event_name":"UserPromptSubmit"}'
send_hook pane-codex-blocked blocked '{"session_id":"blocked-session","hook_event_name":"PermissionRequest"}'

send_hook pane-codex-compact session '{"session_id":"compact-session","transcript_path":"/tmp/compact-session.jsonl","hook_event_name":"SessionStart"}'
send_hook pane-codex-compact working '{"session_id":"compact-session","hook_event_name":"PreCompact"}'

send_hook pane-codex-subagent session '{"session_id":"parent-session","transcript_path":"/tmp/parent-session.jsonl","hook_event_name":"SessionStart"}'
send_hook pane-codex-subagent working '{"session_id":"parent-session","hook_event_name":"UserPromptSubmit"}'
send_hook pane-codex-subagent working '{"session_id":"parent-session","hook_event_name":"SubagentStart","agent_id":"child-1"}'
send_hook pane-codex-subagent idle '{"hook_event_name":"SubagentStop","agent_id":"child-1"}'
send_hook pane-codex-subagent idle '{"session_id":"parent-session","hook_event_name":"Stop"}'
send_hook pane-codex-subagent release '{"session_id":"parent-session","hook_event_name":"SessionEnd"}'

python3 - "$request_log" <<'PY'
import json
import sys
from pathlib import Path

requests = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
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
        assert params.get("source") == "gardn:codex", req
        assert params.get("agent") == "codex", req
        assert isinstance(params.get("seq"), int), req
        launch_env = params.get("launch_env", {})
        assert launch_env.get("CODEX_HOME"), req
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

for pane in ("pane-codex-allowed", "pane-codex-blocked", "pane-codex-compact", "pane-codex-subagent"):
    assert_common(pane)
    assert_single_session_identity(pane)

assert_contains_in_order("pane-codex-allowed", ["working", "idle"])
if not releases_for("pane-codex-allowed"):
    raise SystemExit("pane-codex-allowed: missing release")
assert_contains_in_order("pane-codex-blocked", ["working", "blocked"])
assert_contains_in_order("pane-codex-compact", ["working"])
assert_contains_in_order("pane-codex-subagent", ["working", "idle"])
if states_for("pane-codex-subagent").count("idle") != 1:
    raise SystemExit(f"pane-codex-subagent: child stop should not idle parent; observed {states_for('pane-codex-subagent')}")
if not releases_for("pane-codex-subagent"):
    raise SystemExit("pane-codex-subagent: missing release")

print("codex status test ok: OpenRouter real cli routes correctly; hook seam reports working, idle, blocked, compacting, release, and subagent parent authority")
PY
