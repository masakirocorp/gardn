#!/usr/bin/env bash
set -euo pipefail

smoke_model_lib="${HAKO_AGENT_SMOKE_MODELS_LIB:-/usr/local/lib/hako-agent-smoke-models.sh}"
seam_only="${HAKO_MAKI_STATUS_SEAM_ONLY:-0}"
repo_dir="${HAKO_REPO_DIR:-/repo}"
manifest="$repo_dir/apps/hako/src/manifests/maki.toml"

if [[ "$seam_only" == "1" ]]; then
  python3 - "$manifest" <<'PY'
import re
import sys
import tomllib
from pathlib import Path

manifest_path = Path(sys.argv[1])
manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))

# These are captured screen lines from Maki's ratatui output. The production
# manifest is loaded here rather than copying its patterns into the smoke.
fixtures = [
    ("working", "output\n ⠋ [BUILD]"),
    ("idle", "output\n [PLAN]"),
    (
        "blocked",
        "Permission Required\n\nY Allow   N Deny\n [BASH]",
    ),
    (
        "blocked",
        "Plan complete\nEnter confirm\nSpace toggle parallel\n [PLAN]",
    ),
    ("idle", "history\n❯ "),
]


def rust_regex(pattern):
    return re.sub(
        r"\\x\{([0-9A-Fa-f]+)\}",
        lambda match: chr(int(match.group(1), 16)),
        pattern,
    )


def gate_matches(gate, screen):
    lowered = screen.lower()
    if gate.get("contains") and not all(value.lower() in lowered for value in gate["contains"]):
        return False
    if gate.get("regex") and not all(re.search(rust_regex(value), screen) for value in gate["regex"]):
        return False
    if gate.get("line_regex"):
        lines = screen.splitlines()
        if not any(re.search(rust_regex(value), line) for value in gate["line_regex"] for line in lines):
            return False
    if gate.get("any") and not any(gate_matches(value, screen) for value in gate["any"]):
        return False
    if gate.get("all") and not all(gate_matches(value, screen) for value in gate["all"]):
        return False
    if gate.get("not") and any(gate_matches(value, screen) for value in gate["not"]):
        return False
    return bool(
        gate.get("contains")
        or gate.get("regex")
        or gate.get("line_regex")
        or gate.get("any")
        or gate.get("all")
        or gate.get("not")
    )


rules = sorted(manifest["rules"], key=lambda rule: rule.get("priority", 0), reverse=True)
for expected_state, screen in fixtures:
    matches = [rule for rule in rules if gate_matches(rule, screen)]
    if not matches:
        raise SystemExit(f"{expected_state}: no production Maki manifest rule matched {screen!r}")
    rule = matches[0]
    if rule.get("state") != expected_state:
        raise SystemExit(
            f"{expected_state}: production manifest selected {rule['id']} ({rule.get('state')}) for {screen!r}"
        )
    evidence_key = "visible_blocker" if expected_state == "blocked" else f"visible_{expected_state}"
    if not rule.get(evidence_key, False):
        raise SystemExit(f"{expected_state}: selected rule {rule['id']} is not visible evidence")

print("maki status seam test ok: production manifest matches working, idle, blocked, plan-complete, and narrow-prompt screen output")
PY
  exit 0
fi

if [[ ! -f "$smoke_model_lib" ]]; then
  echo "Maki status smoke needs $smoke_model_lib" >&2
  exit 1
fi
source "$smoke_model_lib"

primary_model="${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_openrouter_prefixed_candidates \
    | hako_smoke_non_openai_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_SMOKE_MODEL "$@"
  exit $?
fi

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "Maki status smoke needs OPENROUTER_API_KEY" >&2
  exit 1
fi
model="$HAKO_SMOKE_ACTIVE_MODEL"
case "$model" in
  openrouter/*) model_spec="$model" ;;
  *) model_spec="openrouter/$model" ;;
esac
workdir="${HAKO_MAKI_STATUS_SMOKE_DIR:-$(mktemp -d)}"
output="$workdir/maki-screen.txt"
mkdir -p "$workdir"

set +e
python3 - "$model_spec" "$output" <<'PY'
import os
import fcntl
import pty
import re
import select
import signal
import struct
import termios
import subprocess
import sys
import time
from pathlib import Path

model, output_path = sys.argv[1:3]
env = os.environ.copy()
env.update({
    "TERM": "xterm-256color",
    "COLORTERM": "truecolor",
    "COLUMNS": "120",
    "LINES": "40",
    "NO_COLOR": "",
})

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
proc = subprocess.Popen(
    ["maki", "--model", model],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    cwd=os.environ.get("HAKO_MAKI_WORKDIR", "/work"),
    env=env,
    start_new_session=True,
)
os.close(slave)
raw = bytearray()

def clean(value):
    value = re.sub(rb"\x1b\][^\x07]*(?:\x07|\x1b\\)", b"", value)
    value = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", value)
    value = value.replace(b"\r", b"")
    return value.decode("utf-8", "replace")

def read_until(predicate, timeout, label, start=0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        wait = max(0.05, min(0.5, deadline - time.monotonic()))
        readable, _, _ = select.select([master], [], [], wait)
        if readable:
            try:
                raw.extend(os.read(master, 65536))
            except OSError:
                break
        screen = clean(bytes(raw[start:]))
        if predicate(screen):
            return screen
        if proc.poll() is not None and not readable:
            break
    screen = clean(bytes(raw[start:]))
    raise RuntimeError(f"timed out waiting for {label}; process={proc.poll()} tail={screen[-2000:]!r}")

def send(value):
    os.write(master, value.encode("utf-8"))

try:
    idle = re.compile(r"(?m)^ \[(?:BUILD|PLAN|BASH)\]")
    working = re.compile(r"(?m)^ (?:[\u2800-\u28ff]){1,2} \[(?:BUILD|PLAN|BASH)\]")
    blocked = re.compile(r"(?is)permission required.*(?:y allow.*n deny|confirm allow|confirm deny)|plan complete.*enter confirm")
    splash = re.compile(r"v\d+\.\d+\.\d+")

    read_until(splash.search, 15, "Maki splash screen")
    send("\r")

    read_until(idle.search, 45, "initial idle Maki status bar")
    start = len(raw)
    send("Use the bash tool to run exactly: printf HAKO_MAKI_STATUS_OK. Do not answer until the command has run.\r")
    read_until(working.search, 90, "working Maki status bar", start)
    start = len(raw)
    read_until(blocked.search, 120, "blocked Maki permission or plan-complete panel", start)
    start = len(raw)
    send("n")
    time.sleep(0.2)
    send("\r")
    read_until(idle.search, 30, "idle Maki status bar after denying the request", start)
    Path(output_path).write_text(clean(bytes(raw)), encoding="utf-8")
    print("maki real status smoke ok: idle -> working -> blocked -> idle screen transitions")
except Exception as exc:
    Path(output_path).write_text(clean(bytes(raw)), encoding="utf-8")
    print(f"Maki real CLI screen smoke failed: {exc}", file=sys.stderr)
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
  sed -n '1,240p' "$output" >&2 || true
  exit "$status"
fi
