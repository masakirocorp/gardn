#!/bin/sh
# installed by hako
# safe to edit. this hook only activates inside hako-managed panes.
# HAKO_INTEGRATION_ID=codex
# HAKO_INTEGRATION_VERSION=3

set -eu

action="${1:-}"
cat >/dev/null 2>/dev/null || true

case "$action" in
  working|idle|blocked|release) ;;
  *) exit 0 ;;
esac

[ "${HAKO_ENV:-}" = "1" ] || exit 0
[ -n "${HAKO_SOCKET_PATH:-}" ] || exit 0
[ -n "${HAKO_PANE_ID:-}" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

HAKO_ACTION="$action" python3 - <<'PY'
import json
import os
import random
import socket
import time

source = "hako:codex"
action = os.environ.get("HAKO_ACTION", "")
pane_id = os.environ.get("HAKO_PANE_ID")
socket_path = os.environ.get("HAKO_SOCKET_PATH")

if not pane_id or not socket_path:
    raise SystemExit(0)

request_id = f"{source}:{int(time.time() * 1000)}:{random.randrange(1_000_000):06d}"
report_seq = time.time_ns()
if action == "release":
    request = {
        "id": request_id,
        "method": "pane.release_agent",
        "params": {
            "pane_id": pane_id,
            "source": source,
            "agent": "codex",
            "seq": report_seq,
        },
    }
else:
    request = {
        "id": request_id,
        "method": "pane.report_agent",
        "params": {
            "pane_id": pane_id,
            "source": source,
            "agent": "codex",
            "state": action,
            "seq": report_seq,
        },
    }

try:
    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(0.5)
    client.connect(socket_path)
    client.sendall((json.dumps(request) + "\n").encode())
    try:
        client.recv(4096)
    except Exception:
        pass
    client.close()
except Exception:
    pass
PY
