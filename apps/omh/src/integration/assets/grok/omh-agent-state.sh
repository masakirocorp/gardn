#!/bin/sh
# installed by Oh My Herdr
# managed by Oh My Herdr; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# OMH_INTEGRATION_ID=grok
# OMH_INTEGRATION_VERSION=1

set -eu

action="${1:-}"
hook_input_file="$(mktemp "${TMPDIR:-/tmp}/omh-grok-hook.XXXXXX")" || exit 0
trap 'rm -f "$hook_input_file"' EXIT HUP INT TERM
cat >"$hook_input_file" 2>/dev/null || true

case "$action" in
  session|working|idle|blocked|release) ;;
  *) exit 0 ;;
esac

[ "${OMH_ENV:-}" = "1" ] || exit 0
[ -n "${OMH_SOCKET_PATH:-}" ] || exit 0
[ -n "${OMH_PANE_ID:-}" ] || exit 0
command -v python3 >/dev/null 2>&1 || exit 0

OMH_ACTION="$action" OMH_HOOK_INPUT_FILE="$hook_input_file" python3 - <<'PY'
import json
import os
import random
import socket
import time

source = "omh:grok"
agent = "grok"
action = os.environ.get("OMH_ACTION", "")
pane_id = os.environ.get("OMH_PANE_ID")
socket_path = os.environ.get("OMH_SOCKET_PATH")
hook_input_file = os.environ.get("OMH_HOOK_INPUT_FILE")

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


hook_event_name = first_text("hookEventName", "hook_event_name") or os.environ.get(
    "GROK_HOOK_EVENT", ""
)
notification_type = first_text("notificationType", "notification_type") or ""
is_subagent = bool(hook_input.get("agentId") or hook_input.get("agent_id")) or hook_event_name in (
    "SubagentStart",
    "SubagentStop",
)

if hook_event_name == "SubagentStop":
    raise SystemExit(0)
if is_subagent and action in ("idle", "release"):
    raise SystemExit(0)
if action == "blocked" and notification_type and notification_type not in (
    "permission_prompt",
    "elicitation_dialog",
):
    raise SystemExit(0)
if (
    action == "idle"
    and hook_event_name == "Notification"
    and notification_type
    and notification_type != "idle_prompt"
):
    raise SystemExit(0)

session_id = os.environ.get("GROK_SESSION_ID") or first_text("sessionId", "session_id")
agent_session_id = session_id if isinstance(session_id, str) and session_id else None
launch_env = {
    key: value
    for key in ("GROK_HOME",)
    if isinstance((value := os.environ.get(key)), str) and value
}
request_id = f"{source}:{int(time.time() * 1000)}:{random.randrange(1_000_000):06d}"
report_seq = time.time_ns()

if action == "session":
    if not agent_session_id:
        raise SystemExit(0)
    request = {
        "id": request_id,
        "method": "pane.report_agent_session",
        "params": {
            "pane_id": pane_id,
            "source": source,
            "agent": agent,
            "seq": report_seq,
            "agent_session_id": agent_session_id,
            "launch_env": launch_env,
        },
    }
elif action == "release":
    request = {
        "id": request_id,
        "method": "pane.release_agent",
        "params": {
            "pane_id": pane_id,
            "source": source,
            "agent": agent,
            "seq": report_seq,
        },
    }
    if agent_session_id:
        request["params"]["agent_session_id"] = agent_session_id
else:
    request = {
        "id": request_id,
        "method": "pane.report_agent",
        "params": {
            "pane_id": pane_id,
            "source": source,
            "agent": agent,
            "state": action,
            "seq": report_seq,
            "launch_env": launch_env,
        },
    }
    if agent_session_id:
        request["params"]["agent_session_id"] = agent_session_id

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
