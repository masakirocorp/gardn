#!/usr/bin/env bash
set -euo pipefail
target="${OMH_PI_OMP_STATUS_TARGET:-all}"
case "$target" in
  all|pi|omp) ;;
  *)
    echo "unknown Pi/OMP status test target: $target" >&2
    exit 2
    ;;
esac
source /usr/local/lib/omh-agent-test-models.sh
primary_model="${OMH_TEST_MODEL:-poolside/laguna-m.1:free}"
if [[ -z "${OMH_TEST_ACTIVE_MODEL:-}" ]]; then
  omh_test_unique_candidates "$primary_model" "${OMH_TEST_FALLBACK_MODELS:-}" \
    | omh_test_opencode_candidates \
    | omh_test_run_with_fallbacks "$0" OMH_TEST_MODEL "$@"
  exit $?
fi

model="$OMH_TEST_ACTIVE_MODEL"
repo_dir="${OMH_REPO_DIR:-/repo}"
workdir="${OMH_PI_OMP_STATUS_DIR:-$(mktemp -d)}"
socket_path="$workdir/omh.sock"
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
  if ! OMH_ENV=1 \
    OMH_SOCKET_PATH="$socket_path" \
    OMH_PANE_ID="$pane" \
    PI_CONFIG_DIR="$dir/config" \
    PI_CODING_AGENT_DIR="$dir/agent" \
    python3 - "$agent" "$model" "$tools" "$extension" "$prompt" "$pane" \
      "$request_log" "$dir/output.txt" "$dir/project" "${OMH_PI_OMP_STATUS_TIMEOUT:-180}" <<'PY'
import fcntl
import json
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

agent, model, tools, extension, prompt, pane, request_log, output_path, workdir, timeout_raw = sys.argv[1:]
timeout = float(timeout_raw)
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
argv = [agent, "--model", model]
if tools == "none":
    argv.append("--no-tools")
else:
    argv.extend(["--tools", tools])
if agent == "omp":
    argv.append("--auto-approve")
argv.extend(["-e", extension])

proc = subprocess.Popen(
    argv,
    stdin=slave,
    stdout=slave,
    stderr=slave,
    cwd=workdir,
    env={**os.environ, "TERM": "xterm-256color", "COLUMNS": "120", "LINES": "40"},
    start_new_session=True,
    close_fds=True,
)
os.close(slave)
raw = bytearray()


def read_output(wait=0.2):
    readable, _, _ = select.select([master], [], [], wait)
    if master not in readable:
        return
    try:
        raw.extend(os.read(master, 65536))
    except OSError:
        pass


def pane_states():
    try:
        lines = Path(request_log).read_text(encoding="utf-8").splitlines()
    except FileNotFoundError:
        return []
    states = []
    for line in lines:
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        params = request.get("params", {})
        if request.get("method") == "pane.report_agent" and params.get("pane_id") == pane:
            states.append(params.get("state"))
    return states


def wait_for_state(predicate, deadline, label):
    while time.monotonic() < deadline:
        read_output()
        states = pane_states()
        if predicate(states):
            return
        if proc.poll() is not None:
            break
    raise RuntimeError(f"timed out waiting for {label}; process={proc.poll()} states={pane_states()}")


try:
    started = time.monotonic()
    wait_for_state(lambda states: "idle" in states, started + min(30, timeout), "initial idle state")
    # OMP enables the Kitty keyboard protocol, so special keys must use CSI-u
    # instead of legacy CR bytes. Fresh CI config roots also enter first-run setup.
    enter = b"\x1b[13u"
    if "working" not in pane_states():
        time.sleep(0.5)
        os.write(master, enter)
        time.sleep(0.7)
        os.write(master, b"\x03")
        time.sleep(0.2)
        os.write(master, enter)
        time.sleep(0.5)
    if "working" not in pane_states():
        prompt_deadline = min(started + timeout, time.monotonic() + 30)
        while proc.poll() is None and time.monotonic() < prompt_deadline:
            os.write(master, prompt.encode() + enter)
            try:
                wait_for_state(
                    lambda states: "working" in states,
                    min(prompt_deadline, time.monotonic() + 5),
                    "working state",
                )
                break
            except RuntimeError:
                continue
    wait_for_state(
        lambda states: "working" in states and states[-1] == "idle",
        started + timeout,
        "working-to-idle lifecycle",
    )
    if proc.poll() is None:
        command = b"/exit" if agent == "omp" else b"/quit"
        os.write(master, command + enter)
        exit_deadline = time.monotonic() + 20
        while proc.poll() is None and time.monotonic() < exit_deadline:
            read_output()
        if proc.poll() is None:
            raise RuntimeError("timed out waiting for graceful /exit")
        read_output(0)
    if proc.returncode != 0:
        raise RuntimeError(f"{agent} exited with status {proc.returncode}")
except Exception as exc:
    print(f"{agent} interactive test failed: {exc}", file=sys.stderr)
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
    raise
finally:
    Path(output_path).write_bytes(raw)
    os.close(master)
PY
  then
    printf '%s\n' "$agent $scenario test failed; output:" >&2
    sed -n '1,200p' "$dir/output.txt" >&2
    return 1
  fi
}

run_basic_agent() {
  local agent="$1"
  local extension="$2"
  local pane="$3"
  run_agent "$agent" "$extension" "$pane" basic none "Reply exactly OMH_${agent^^}_STATUS_OK"
}

run_subagent_agent() {
  local agent="$1"
  local extension="$2"
  local pane="$3"
  run_agent "$agent" "$extension" "$pane" subagent task,yield "Launch one subagent with assignment: reply exactly CHILD_OK. Then reply exactly OMH_${agent^^}_SUBAGENT_OK."
}

if [[ "$target" == "all" || "$target" == "omp" ]]; then
  run_basic_agent omp "$repo_dir/apps/omh/src/integration/assets/omp/omh-agent-state.ts" pane-omp-real
  run_subagent_agent omp "$repo_dir/apps/omh/src/integration/assets/omp/omh-agent-state.ts" pane-omp-subagent
fi
if [[ "$target" == "all" || "$target" == "pi" ]]; then
  run_basic_agent pi "$repo_dir/apps/omh/src/integration/assets/pi/omh-agent-state.ts" pane-pi-real
fi

REQUEST_LOG="$request_log" WORKDIR="$workdir" TARGET="$target" python3 - <<'PY'
import json
import os
import sys
from pathlib import Path

request_log = Path(os.environ["REQUEST_LOG"])
workdir = Path(os.environ["WORKDIR"])
target = os.environ["TARGET"]
if not request_log.exists():
    for output_path in sorted(workdir.glob("*/output.txt")):
        output = output_path.read_text(encoding="utf-8", errors="replace")
        print(f"{output_path.parent.name} output:\n{output}", file=sys.stderr)
    raise SystemExit("Pi/OMP test emitted no Oh My Herdr status requests")
requests = [json.loads(line) for line in request_log.read_text(encoding="utf-8").splitlines() if line.strip()]
reports = [req for req in requests if req.get("method") == "pane.report_agent"]
releases = [req for req in requests if req.get("method") == "pane.release_agent"]
session_reports = [req for req in requests if req.get("method") == "pane.report_agent_session"]



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
    marker = f"OMH_{agent.upper()}_{marker_suffix}"
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

    expected_source = f"omh:{agent}"
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
        if agent == "omp":
            if launch_env is not None:
                raise SystemExit(f"{agent}: state/release report must not carry launch_env {launch_env}")
        elif launch_env != {"PI_CONFIG_DIR": expected_config, "PI_CODING_AGENT_DIR": expected_agent_dir}:
            raise SystemExit(f"{agent}: wrong launch_env {launch_env}")
    if agent == "omp":
        pane_session_reports = for_pane(session_reports, pane_id)
        if not pane_session_reports:
            raise SystemExit("omp: missing pane.report_agent_session launch context")
        for req in pane_session_reports:
            launch_env = req.get("params", {}).get("launch_env")
            if launch_env != {
                "PI_CONFIG_DIR": expected_config,
                "PI_CODING_AGENT_DIR": expected_agent_dir,
            }:
                raise SystemExit(f"omp: wrong session launch_env {launch_env}")
    assert_single_session_root(agent, session_paths, release_paths)


selected_agents = ["omp", "pi"] if target == "all" else [target]
for agent in selected_agents:
    assert_agent(agent, "basic", f"pane-{agent}-real", "STATUS_OK")
    if agent == "omp":
        assert_agent(agent, "subagent", f"pane-{agent}-subagent", "SUBAGENT_OK")
scope = "pi/omp" if target == "all" else target
print(f"{scope} status test ok: real cli reports session root identity, working, idle, release, launch env, and OMP scoped subagent identity")
PY
