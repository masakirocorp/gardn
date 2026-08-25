#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/gardn-agent-test-models.sh

primary_model="${GARDN_TEST_MODEL:-$GARDN_TEST_DEFAULT_MODEL}"
if [[ -z "${GARDN_TEST_ACTIVE_MODEL:-}" ]]; then
  gardn_test_unique_candidates "$primary_model" "${GARDN_TEST_FALLBACK_MODELS:-}" \
    | gardn_test_available_candidates \
    | gardn_test_run_with_fallbacks "$0" "$@"
  exit $?
fi

model="$GARDN_TEST_ACTIVE_MODEL"
gardn_test_configure_model "$model"
repo_dir="${GARDN_REPO_DIR:-/repo}"
hook_path="$repo_dir/apps/gardn/src/integration/assets/qwen/gardn-agent-session.sh"
workdir="${GARDN_QWEN_STATUS_TEST_DIR:-$(mktemp -d)}"
socket_path="$workdir/gardn.sock"
request_log="$workdir/gardn-requests.jsonl"
output="$workdir/qwen-screen.txt"
qwen_home="$workdir/qwen-home"

if [[ ! -f "$hook_path" ]]; then
  echo "qwen status test needs gardn repo mounted at $repo_dir" >&2
  exit 1
fi
mkdir -p "$qwen_home/hooks" "$workdir/run"
cp "$hook_path" "$qwen_home/hooks/gardn-agent-session.sh"
chmod +x "$qwen_home/hooks/gardn-agent-session.sh"
cat > "$qwen_home/settings.json" <<EOF_CONFIG
{
  "selectedAuthType": "openai",
  "security": {
    "auth": {"selectedType": "openai"},
    "folderTrust": {"enabled": false}
  },
  "ui": {"showStatusInTitle": true},
  "hooks": {
    "SessionStart": [{
      "matcher": "*",
      "hooks": [{
        "type": "command",
        "command": "bash $qwen_home/hooks/gardn-agent-session.sh session",
        "timeout": 10000
      }]
    }]
  }
}
EOF_CONFIG

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
QWEN_HOME="$qwen_home" \
GARDN_ENV=1 \
GARDN_SOCKET_PATH="$socket_path" \
GARDN_PANE_ID=pane-qwen \
QWEN_CODE_NO_UPDATE_NOTIFIER=1 \
python3 - "$model" "$output" "$request_log" <<'PY'
import fcntl
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
env = os.environ.copy()
env.update({"TERM": "xterm-256color", "COLORTERM": "truecolor", "COLUMNS": "120", "LINES": "40"})
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
proc = subprocess.Popen(
    ["qwen", "--model", model],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    cwd=os.environ.get("GARDN_QWEN_WORKDIR", "/work"),
    env=env,
    start_new_session=True,
)
os.close(slave)
raw = bytearray()

def clean(value):
    value = re.sub(rb"\x1b\][^\x07]*(?:\x07|\x1b\\)", b"", value)
    value = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", value)
    return value.replace(b"\r", b"").decode("utf-8", "replace")

def read_until(predicate, timeout, label, start=0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        readable, _, _ = select.select([master], [], [], 0.5)
        if readable:
            try:
                raw.extend(os.read(master, 65536))
            except OSError:
                break
        view = bytes(raw[start:])
        if predicate(view, clean(view)):
            return
        if proc.poll() is not None and not readable:
            break
    raise RuntimeError(f"timed out waiting for {label}; process={proc.poll()} tail={clean(bytes(raw))[-2000:]!r}")

idle = re.compile(r"(?im)^\s*>\s*(?:type\s*)?.*(?:message|@path/to/file)")
working = re.compile(r"(?i)(?:esc to cancel|◐)")
session = re.compile(r'"method"\s*:\s*"pane\.report_agent_session"')
try:
    read_until(lambda _raw, text: bool(idle.search(text)), 30, "initial idle composer")
    read_until(
        lambda _raw, _text: Path(request_log).exists() and bool(session.search(Path(request_log).read_text(errors="replace"))),
        15,
        "Qwen session report",
    )
    start = len(raw)
    os.write(master, b"Reply with exactly GARDN_QWEN_STATUS_IDLE.\r")
    read_until(lambda payload, text: bool(working.search(payload.decode("utf-8", "replace"))) or bool(working.search(text)), 90, "working status", start)
    read_until(lambda _raw, text: "GARDN_QWEN_STATUS_IDLE" in text and bool(idle.search(text)), 120, "eventual idle composer", start)
    Path(output_path).write_text(clean(bytes(raw)), encoding="utf-8")
    print("qwen live status test ok: initial idle -> working -> idle with session identity")
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
  if gardn_test_retryable_status_or_output "$status" "$output"; then
    echo "retryable Qwen/OpenRouter provider failure with $model" >&2
    exit 75
  fi
  exit "$status"
fi

python3 - "$request_log" <<'PY'
import json
import sys
from pathlib import Path
requests = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
sessions = [item for item in requests if item.get("method") == "pane.report_agent_session"]
if not sessions:
    raise SystemExit("Qwen emitted no session report")
params = sessions[-1].get("params", {})
assert params.get("pane_id") == "pane-qwen", params
assert params.get("source") == "gardn:qwen", params
assert params.get("agent") == "qwen", params
assert params.get("agent_session_id"), params
PY
