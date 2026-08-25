#!/usr/bin/env bash
set -euo pipefail
if [[ -n "${GARDN_DETERMINISTIC_PROVIDER_URL:-}" ]]; then
  model_spec="gardn/gardn-tool"
  export GARDN_KILO_EXPECTED_TEXT="GARDN_PROVIDER_TOOL_OK"
else
  source /usr/local/lib/gardn-agent-test-models.sh
  primary_model="${GARDN_TEST_MODEL:-$GARDN_TEST_DEFAULT_MODEL}"
  if [[ -z "${GARDN_TEST_ACTIVE_MODEL:-}" ]]; then
    gardn_test_unique_candidates "$primary_model" "${GARDN_TEST_FALLBACK_MODELS:-}" \
      | gardn_test_available_candidates \
      | gardn_test_run_with_fallbacks "$0" "$@"
    exit $?
  fi
  model="$GARDN_TEST_ACTIVE_MODEL"
  model_spec="$(gardn_test_provider_model "$model")"
  gardn_test_configure_model "$model"
  export GARDN_KILO_EXPECTED_TEXT="GARDN_KILO_STATUS_OK"
fi
repo_dir="${GARDN_REPO_DIR:-/repo}"
plugin_path="$repo_dir/apps/gardn/src/integration/assets/kilo/gardn-agent-state.js"
workdir="${GARDN_KILO_STATUS_TEST_DIR:-$(mktemp -d)}"
socket_path="$workdir/gardn.sock"
request_log="$workdir/gardn-requests.jsonl"
output="$workdir/kilo-screen.txt"
config_home="$workdir/config"

if [[ ! -f "$plugin_path" ]]; then
  echo "kilo status test needs gardn repo mounted at $repo_dir" >&2
  exit 1
fi
mkdir -p "$config_home/kilo/plugin" "$workdir/run"
cp "$plugin_path" "$config_home/kilo/plugin/gardn-agent-state.js"
if [[ -n "${GARDN_DETERMINISTIC_PROVIDER_URL:-}" ]]; then
  cat > "$config_home/kilo/kilo.jsonc" <<EOF_CONFIG
{
  "model": "gardn/gardn-tool",
  "permission": {"bash": "ask"},
  "provider": {
    "gardn": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Gardn deterministic provider",
      "options": {
        "baseURL": "${GARDN_DETERMINISTIC_PROVIDER_URL}/v1",
        "apiKey": "gardn-deterministic-key"
      },
      "models": {
        "gardn-tool": {
          "name": "Gardn deterministic tool model",
          "tool_call": true,
          "limit": {"context": 32768, "output": 4096}
        }
      }
    }
  }
}
EOF_CONFIG
fi

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
            data = b""
            while not data.endswith(b"\n"):
                chunk = conn.recv(4096)
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
[[ -S "$socket_path" ]] || { echo "fake gardn socket did not start" >&2; exit 1; }

set +e
XDG_CONFIG_HOME="$config_home" \
GARDN_ENV=1 \
GARDN_SOCKET_PATH="$socket_path" \
GARDN_PANE_ID=pane-kilo \
python3 - "$model_spec" "$output" "$request_log" <<'PY'
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

model, output_path, request_log = sys.argv[1:4]
deterministic = bool(os.environ.get("GARDN_DETERMINISTIC_PROVIDER_URL"))
prompt = (
    "Use the bash tool to run exactly: printf GARDN_KILO_STATUS_OK. Do not answer until it runs."
    if deterministic
    else "Reply with exactly GARDN_KILO_STATUS_OK."
)
env = os.environ.copy()
env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "COLUMNS": "120", "LINES": "40"})
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
proc = subprocess.Popen(
    [
        "kilo", "run", "--interactive", "--model", model, prompt,
    ],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    cwd=os.environ.get("GARDN_KILO_WORKDIR", "/work"),
    env=env,
    start_new_session=True,
)
os.close(slave)
raw = bytearray()

def clean(value):
    value = re.sub(rb"\x1b\][^\x07]*(?:\x07|\x1b\\)", b"", value)
    value = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", value)
    return value.replace(b"\r", b"").decode("utf-8", "replace")

def requests():
    path = Path(request_log)
    if not path.exists():
        return []
    result = []
    for line in path.read_text(errors="replace").splitlines():
        try:
            result.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return result

def states():
    return [
        item.get("params", {}).get("state")
        for item in requests()
        if item.get("method") == "pane.report_agent"
    ]

def read_until(predicate, timeout, label):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        readable, _, _ = select.select([master], [], [], 0.25)
        if readable:
            try:
                raw.extend(os.read(master, 65536))
            except OSError:
                break
        if predicate():
            return
        if proc.poll() is not None and not readable:
            break
    raise RuntimeError(f"timed out waiting for {label}; states={states()} process={proc.poll()} tail={clean(bytes(raw))[-2000:]!r}")

try:
    read_until(lambda: "working" in states(), 60, "working report")
    working_index = states().index("working")
    if deterministic:
        read_until(lambda: "blocked" in states(), 120, "blocked permission report")
        blocked_index = states().index("blocked")
        os.write(master, b"\r")
        read_until(lambda: "working" in states()[blocked_index + 1 :], 30, "working recovery report")
        working_index = blocked_index + 1 + states()[blocked_index + 1 :].index("working")
    read_until(lambda: "idle" in states()[working_index + 1 :], 120, "idle completion report")
    read_until(lambda: os.environ["GARDN_KILO_EXPECTED_TEXT"] in clean(bytes(raw)), 15, "completion marker")
    Path(output_path).write_text(clean(bytes(raw)), encoding="utf-8")
    print(
        "kilo deterministic status test ok: working -> blocked -> working -> idle"
        if deterministic
        else "kilo live status test ok: working -> idle"
    )
except Exception:
    Path(output_path).write_text(clean(bytes(raw)), encoding="utf-8")
    raise
finally:
    if proc.poll() is None:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
    os.close(master)
PY
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  if [[ -z "${GARDN_DETERMINISTIC_PROVIDER_URL:-}" ]] && gardn_test_retryable_status_or_output "$status" "$output"; then
    echo "retryable Kilo/OpenRouter provider failure with $model" >&2
    exit 75
  fi
  exit "$status"
fi

python3 - "$request_log" "${GARDN_DETERMINISTIC_PROVIDER_URL:+deterministic}" <<'PY'
import json
import sys
from pathlib import Path
requests = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
sessions = [item for item in requests if item.get("method") == "pane.report_agent_session"]
reports = [item for item in requests if item.get("method") == "pane.report_agent"]
if not sessions:
    raise SystemExit("Kilo emitted no session report")
for item in sessions + reports:
    params = item.get("params", {})
    assert params.get("pane_id") == "pane-kilo", params
    assert params.get("source") == "gardn:kilo", params
    assert params.get("agent") == "kilo", params
states = [item.get("params", {}).get("state") for item in reports]
expected_states = (
    ("working", "blocked", "working", "idle")
    if len(sys.argv) > 2 and sys.argv[2] == "deterministic"
    else ("working", "idle")
)
position = 0
for expected in expected_states:
    try:
        position = states.index(expected, position) + 1
    except ValueError as error:
        raise SystemExit(f"Kilo missing {expected}; observed {states}") from error
PY
