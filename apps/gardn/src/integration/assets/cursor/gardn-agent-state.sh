#!/bin/sh
# installed by Gardn
# managed by Gardn; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# GARDN_INTEGRATION_ID=cursor
# GARDN_INTEGRATION_VERSION=3

set -eu

# Grok Build loads Cursor compatibility hooks. Its native Gardn hook owns Grok panes.
[ -z "${GROK_HOOK_EVENT:-}" ] || exit 0

action="${1:-}"
hook_input_file="$(mktemp "${TMPDIR:-/tmp}/gardn-cursor-hook.XXXXXX")" || exit 0
trap 'rm -f "$hook_input_file"' EXIT HUP INT TERM
cat >"$hook_input_file" 2>/dev/null || true

case "$action" in
  working|idle|release) ;;
  *) exit 0 ;;
esac

[ "${GARDN_ENV:-}" = "1" ] || exit 0
[ -n "${GARDN_SOCKET_PATH:-}" ] || exit 0
[ -n "${GARDN_PANE_ID:-}" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

GARDN_ACTION="$action" GARDN_HOOK_INPUT_FILE="$hook_input_file" python3 - <<'PY'
import json
import os
import random
import socket
import time

source = "gardn:cursor"
action = os.environ.get("GARDN_ACTION", "")
pane_id = os.environ.get("GARDN_PANE_ID")
socket_path = os.environ.get("GARDN_SOCKET_PATH")
hook_input_file = os.environ.get("GARDN_HOOK_INPUT_FILE")

if not pane_id or not socket_path:
    raise SystemExit(0)

hook_input = {}
if hook_input_file:
    try:
        with open(hook_input_file, encoding="utf-8") as handle:
            content = handle.read()
        if content.strip():
            parsed = json.loads(content)
            if isinstance(parsed, dict):
                hook_input = parsed
    except Exception:
        hook_input = {}

def first_text(*keys):
    for key in keys:
        value = hook_input.get(key)
        if isinstance(value, str) and value:
            return value
    return None

request_id = f"{source}:{int(time.time() * 1000)}:{random.randrange(1_000_000):06d}"
report_seq = time.time_ns()
session_id = first_text("session_id", "sessionId", "conversation_id", "conversationId", "chat_id", "chatId")

def launch_env():
    return {
        key: value
        for key in ("CURSOR_CONFIG_DIR",)
        if isinstance((value := os.environ.get(key)), str) and value
    }

def send(method, params):
    request = {
        "id": request_id,
        "method": method,
        "params": {
            "pane_id": pane_id,
            "source": source,
            "agent": "cursor",
            "seq": report_seq,
            **params,
        },
    }
    if session_id:
        request["params"]["agent_session_id"] = session_id
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

if action == "release":
    send("pane.release_agent", {})
else:
    params = {"state": action}
    env = launch_env()
    if env:
        params["launch_env"] = env
    send("pane.report_agent", params)
PY
