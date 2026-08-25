#!/usr/bin/env bash
set -euo pipefail
repo_dir="${GARDN_REPO_DIR:-/repo}"
hook_source="$repo_dir/apps/gardn/src/integration/assets/antigravity_cli/gardn-agent-session.sh"
workdir="${GARDN_ANTIGRAVITY_STATUS_TEST_DIR:-$(mktemp -d)}"
account_home="$(python3 -c 'import os, pwd; print(pwd.getpwuid(os.getuid()).pw_dir)')"
config_dir="$account_home/.gemini/antigravity-cli"
socket_path="$workdir/gardn.sock"; request_log="$workdir/gardn-requests.jsonl"; output="$workdir/antigravity-screen.txt"
[[ -f "$hook_source" ]] || { echo "Antigravity status test needs Gardn repo mounted at $repo_dir" >&2; exit 1; }
if [[ "${GARDN_ANTIGRAVITY_REAL:-0}" == 1 ]]; then
  [[ -n "${GEMINI_API_KEY:-}" ]] || { echo "GEMINI_API_KEY is required for selected Antigravity real smoke" >&2; exit 64; }
  unset GOOGLE_GEMINI_BASE_URL
  expected=GARDN_ANTIGRAVITY_REAL_OK
  prompt="Reply with exactly GARDN_ANTIGRAVITY_REAL_OK."
else
  : "${GARDN_DETERMINISTIC_PROVIDER_URL:?Antigravity deterministic harness requires provider}"
  export GEMINI_API_KEY=gardn-deterministic-key
  export GOOGLE_GEMINI_BASE_URL="$GARDN_DETERMINISTIC_PROVIDER_URL"
  expected=GARDN_PROVIDER_OK
  prompt="Reply with exactly GARDN_PROVIDER_OK."
fi
mkdir -p "$config_dir/hooks"; cp "$hook_source" "$config_dir/hooks/gardn-agent-session.sh"; chmod +x "$config_dir/hooks/gardn-agent-session.sh"
printf '{"modelProvider":"gemini","colorScheme":"terminal"}\n' > "$config_dir/settings.json"
printf '{"gardn":{"PreInvocation":[{"type":"command","command":"bash %s session","timeout":10}]}}\n' "$config_dir/hooks/gardn-agent-session.sh" > "$config_dir/hooks.json"
python3 - "$socket_path" "$request_log" <<'PY' &
import json, os, socket, sys, time
path, log = sys.argv[1:3]
try: os.unlink(path)
except FileNotFoundError: pass
server=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); server.bind(path); server.listen(8); server.settimeout(.2)
with open(log,"a",encoding="utf-8") as out:
  deadline=time.time()+300
  while time.time()<deadline:
    try: conn,_=server.accept()
    except TimeoutError: continue
    except OSError: break
    with conn:
      data=b""
      while not data.endswith(b"\n"):
        chunk=conn.recv(4096)
        if not chunk: break
        data+=chunk
      if not data: continue
      out.write(data.decode("utf-8","replace")); out.flush()
      try:
        req=json.loads(data); conn.sendall((json.dumps({"id":req.get("id"),"result":{"type":"ok"}})+"\n").encode())
      except Exception: pass
PY
socket_pid=$!; trap 'kill "$socket_pid" >/dev/null 2>&1 || true' EXIT
for _ in $(seq 1 50); do [[ -S "$socket_path" ]] && break; sleep .1; done
[[ -S "$socket_path" ]] || { echo "fake Gardn socket did not start" >&2; exit 1; }
printf '{"conversationId":"gardn-antigravity-fixture"}\n' | \
  GARDN_ENV=1 GARDN_SOCKET_PATH="$socket_path" GARDN_PANE_ID=pane-antigravity \
  bash "$config_dir/hooks/gardn-agent-session.sh" session >/dev/null
set +e
GARDN_ENV=1 GARDN_SOCKET_PATH="$socket_path" GARDN_PANE_ID=pane-antigravity \
  agy -p "$prompt" --output-format json >"$output" 2>"${output}.stderr"
status=$?
set -e
if [[ "$status" -ne 0 ]] || ! grep -Fq "$expected" "$output"; then
  cat "$output" >&2
  cat "${output}.stderr" >&2
  [[ -n "${GARDN_DETERMINISTIC_PROVIDER_LOG:-}" ]] && cat "$GARDN_DETERMINISTIC_PROVIDER_LOG" >&2
  [[ "$status" -ne 0 ]] && exit "$status"
  exit 1
fi
python3 - "$request_log" <<'PY'
import json, sys
from pathlib import Path
items=[json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
sessions=[x for x in items if x.get("method")=="pane.report_agent_session"]
if not sessions: raise SystemExit("Antigravity emitted no session report")
params=sessions[-1].get("params",{}); assert params.get("pane_id")=="pane-antigravity",params; assert params.get("source")=="gardn:antigravity_cli",params; assert params.get("agent")=="agy",params
PY
printf 'Antigravity %s Gemini status test ok\n' "${GARDN_ANTIGRAVITY_REAL:+real}"
