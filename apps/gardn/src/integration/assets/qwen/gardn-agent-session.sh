#!/bin/sh
# GARDN_INTEGRATION_ID=qwen
# GARDN_INTEGRATION_VERSION=1

[ "${1:-}" = "session" ] || exit 0
[ "${GARDN_ENV:-}" = "1" ] || exit 0
[ -n "${GARDN_PANE_ID:-}" ] || exit 0
[ -n "${GARDN_SOCKET_PATH:-}" ] || exit 0
if [ -n "${GARDN_BIN_PATH:-}" ]; then
    [ -x "$GARDN_BIN_PATH" ] || exit 0
else
    command -v gardn >/dev/null 2>&1 || exit 0
fi
command -v python3 >/dev/null 2>&1 || exit 0

python3 -c '
import json
import os
import subprocess
import sys
import time

try:
    payload = json.load(sys.stdin)
    session_id = payload.get("session_id")
    source = payload.get("source")
    if not isinstance(session_id, str) or not session_id:
        raise ValueError
    command = os.environ.get("GARDN_BIN_PATH") or "gardn"
    args = [
        command, "pane", "report-agent-session", os.environ["GARDN_PANE_ID"],
        "--source", "gardn:qwen", "--agent", "qwen",
        "--agent-session-id", session_id, "--seq", str(time.time_ns()),
    ]
    if source in ("startup", "resume", "clear", "compact", "branch"):
        args.extend(["--session-start-source", source])
    subprocess.run(
        args,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=1,
        check=False,
    )
except Exception:
    pass
' 2>/dev/null || true
