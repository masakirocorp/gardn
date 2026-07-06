#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_openrouter_bare_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_SMOKE_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
repo_dir="${HAKO_REPO_DIR:-/repo}"
workdir="${HAKO_PI_OMP_STATUS_DIR:-$(mktemp -d)}"
socket_path="$workdir/hako.sock"
request_log="$workdir/requests.jsonl"


mkdir -p "$workdir"
rm -f "$socket_path" "$request_log"

SOCKET_PATH="$socket_path" REQUEST_LOG="$request_log" python3 - <<'PY' &
import json
import os
import socket

socket_path = os.environ["SOCKET_PATH"]
request_log = os.environ["REQUEST_LOG"]
try:
    os.unlink(socket_path)
except FileNotFoundError:
    pass

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen()
while True:
    conn, _ = server.accept()
    data = b""
    while not data.endswith(b"\n"):
        chunk = conn.recv(65536)
        if not chunk:
            break
        data += chunk
    if data.strip():
        request = json.loads(data.decode("utf-8"))
        with open(request_log, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(request) + "\n")
        response = {"id": request.get("id"), "result": {"type": "ok"}}
        conn.sendall((json.dumps(response) + "\n").encode("utf-8"))
    conn.close()
PY
server_pid=$!
trap 'kill "$server_pid" >/dev/null 2>&1 || true' EXIT

for _ in $(seq 1 50); do
  [[ -S "$socket_path" ]] && break
  sleep 0.1
done
[[ -S "$socket_path" ]] || { echo "status socket did not start" >&2; exit 1; }

run_agent() {
  local agent="$1"
  local extension="$2"
  local pane="$3"
  local scenario="$4"
  local tools="$5"
  local prompt="$6"
  local dir="$workdir/$agent-$scenario"
  mkdir -p "$dir/config" "$dir/agent" "$dir/project"
  if ! (
    cd "$dir/project"
    HAKO_ENV=1 \
    HAKO_SOCKET_PATH="$socket_path" \
    HAKO_PANE_ID="$pane" \
    PI_CONFIG_DIR="$dir/config" \
    PI_CODING_AGENT_DIR="$dir/agent" \
    timeout "${HAKO_PI_OMP_STATUS_TIMEOUT:-180}" "$agent" \
      -p \
      --model "openrouter/$model" \
      --tools "$tools" \
      --auto-approve \
      -e "$extension" \
      "$prompt" >"$dir/output.txt" 2>&1
  ); then
    printf '%s\n' "$agent $scenario smoke failed; output:" >&2
    sed -n '1,200p' "$dir/output.txt" >&2
    return 1
  fi
}

run_basic_agent() {
  local agent="$1"
  local extension="$2"
  local pane="$3"
  run_agent "$agent" "$extension" "$pane" basic none "Reply exactly HAKO_${agent^^}_STATUS_OK"
}

run_subagent_agent() {
  local agent="$1"
  local extension="$2"
  local pane="$3"
  run_agent "$agent" "$extension" "$pane" subagent task,yield "Launch one subagent with assignment: reply exactly CHILD_OK. Then reply exactly HAKO_${agent^^}_SUBAGENT_OK."
}

run_basic_agent omp "$repo_dir/src/integration/assets/omp/hako-agent-state.ts" pane-omp-real
run_basic_agent pi "$repo_dir/src/integration/assets/pi/hako-agent-state.ts" pane-pi-real
run_subagent_agent omp "$repo_dir/src/integration/assets/omp/hako-agent-state.ts" pane-omp-subagent
run_subagent_agent pi "$repo_dir/src/integration/assets/pi/hako-agent-state.ts" pane-pi-subagent

REQUEST_LOG="$request_log" WORKDIR="$workdir" python3 - <<'PY'
import json
import os
import sys
from pathlib import Path

request_log = Path(os.environ["REQUEST_LOG"])
workdir = Path(os.environ["WORKDIR"])
requests = [json.loads(line) for line in request_log.read_text(encoding="utf-8").splitlines() if line.strip()]
reports = [req for req in requests if req.get("method") == "pane.report_agent"]
releases = [req for req in requests if req.get("method") == "pane.release_agent"]



def is_child_session_path(parent_path, candidate_path):
    if not parent_path.endswith(".jsonl"):
        return False
    return candidate_path.startswith(parent_path[:-6] + "/")


def session_roots(paths):
    roots = []
    for path in sorted(paths):
        if any(is_child_session_path(root, path) for root in roots):
            continue
        roots.append(path)
    return roots


def assert_single_session_root(agent, session_paths, release_paths):
    roots = session_roots(session_paths)
    if len(roots) != 1:
        raise SystemExit(f"{agent}: expected one session identity, observed {sorted(session_paths)}")
    if roots[0] not in release_paths:
        raise SystemExit(f"{agent}: expected parent session release, observed releases {sorted(release_paths)}")
    for path in session_paths:
        if path != roots[0] and not is_child_session_path(roots[0], path):
            raise SystemExit(f"{agent}: unrelated child session identity {path} for parent {roots[0]}")

def for_pane(collection, pane_id):
    return [req for req in collection if req.get("params", {}).get("pane_id") == pane_id]


def states_for(pane_id):
    return [req.get("params", {}).get("state") for req in for_pane(reports, pane_id)]


def assert_agent(agent, scenario, pane_id, marker_suffix):
    output = (workdir / f"{agent}-{scenario}" / "output.txt").read_text(encoding="utf-8")
    marker = f"HAKO_{agent.upper()}_{marker_suffix}"
    if marker not in output:
        print(f"{agent} {scenario}: missing output marker {marker}; output was {output!r}", file=sys.stderr)
        raise SystemExit(75)

    pane_reports = for_pane(reports, pane_id)
    pane_releases = for_pane(releases, pane_id)
    if not pane_reports:
        raise SystemExit(f"{agent}: no pane.report_agent calls")
    if not pane_releases:
        raise SystemExit(f"{agent}: no pane.release_agent calls")
    states = states_for(pane_id)
    if "idle" not in states or "working" not in states:
        raise SystemExit(f"{agent}: expected idle and working states, observed {states}")

    expected_source = f"hako:{agent}"
    expected_config = str(workdir / f"{agent}-{scenario}" / "config")
    expected_agent_dir = str(workdir / f"{agent}-{scenario}" / "agent")
    session_paths = set()
    release_paths = set()
    for req in pane_reports + pane_releases:
        params = req.get("params", {})
        if params.get("source") != expected_source:
            raise SystemExit(f"{agent}: wrong source in {req}")
        if params.get("agent") != agent:
            raise SystemExit(f"{agent}: wrong agent in {req}")
        if not isinstance(params.get("seq"), int):
            raise SystemExit(f"{agent}: missing numeric seq in {req}")
        path = params.get("agent_session_path")
        if not isinstance(path, str) or not path:
            raise SystemExit(f"{agent}: missing agent_session_path in {req}")
        session_paths.add(path)
        if req in pane_releases:
            release_paths.add(path)
        launch_env = params.get("launch_env")
        if launch_env != {"PI_CONFIG_DIR": expected_config, "PI_CODING_AGENT_DIR": expected_agent_dir}:
            raise SystemExit(f"{agent}: wrong launch_env {launch_env}")
    assert_single_session_root(agent, session_paths, release_paths)


assert_agent("omp", "basic", "pane-omp-real", "STATUS_OK")
assert_agent("pi", "basic", "pane-pi-real", "STATUS_OK")
assert_agent("omp", "subagent", "pane-omp-subagent", "SUBAGENT_OK")
assert_agent("pi", "subagent", "pane-pi-subagent", "SUBAGENT_OK")
print("pi/omp status test ok: real cli reports session root identity, working, idle, release, launch env, and scoped subagent identity")
PY
