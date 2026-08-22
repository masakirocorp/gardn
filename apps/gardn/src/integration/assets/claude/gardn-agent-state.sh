#!/bin/sh
# installed by Gardn
# managed by Gardn; reinstalling or updating the integration overwrites this file.
# add custom hooks beside this file instead of editing it.
# GARDN_INTEGRATION_ID=claude
# GARDN_INTEGRATION_VERSION=4

set -eu

# Grok Build loads Claude compatibility hooks. Its native Gardn hook owns Grok panes.
[ -z "${GROK_HOOK_EVENT:-}" ] || exit 0

action="${1:-}"
hook_input_file="$(mktemp "${TMPDIR:-/tmp}/gardn-claude-hook.XXXXXX")" || exit 0
trap 'rm -f "$hook_input_file"' EXIT HUP INT TERM
cat >"$hook_input_file" 2>/dev/null || true

case "$action" in
  session|working|idle|blocked|release) ;;
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

source = "gardn:claude"
agent = "claude"
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


hook_event_name = first_text("hook_event_name", "hookEventName") or ""
notification_type = first_text("notification_type", "notificationType") or ""
is_subagent = bool(hook_input.get("agent_id")) or hook_event_name in ("SubagentStart", "SubagentStop")

# Subagent completion is not parent completion. Claude can emit recap/summary
# SubagentStop events after the parent turn is already idle; never let those
# revive or idle the parent pane.
if hook_event_name == "SubagentStop":
    raise SystemExit(0)
if is_subagent and action in ("idle", "release"):
    raise SystemExit(0)
if action == "blocked" and notification_type and notification_type not in (
    "permission_prompt",
    "elicitation_dialog",
):
    raise SystemExit(0)
if action == "idle" and notification_type and notification_type != "idle_prompt":
    raise SystemExit(0)

session_id = first_text("session_id", "sessionId")
agent_session_id = session_id if session_id else None
transcript_path = first_text("transcript_path", "transcriptPath")
agent_session_path = transcript_path if transcript_path else None
session_start_source = (
    first_text("source", "session_start_source")
    if hook_event_name == "SessionStart"
    else None
)
if session_start_source not in ("startup", "resume", "clear", "compact"):
    session_start_source = None
launch_env = {
    key: value
    for key in ("CLAUDE_CONFIG_DIR",)
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
    if agent_session_path:
        request["params"]["agent_session_path"] = agent_session_path
    if session_start_source:
        request["params"]["session_start_source"] = session_start_source
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
    if agent_session_path:
        request["params"]["agent_session_path"] = agent_session_path
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
    if agent_session_path:
        request["params"]["agent_session_path"] = agent_session_path
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
