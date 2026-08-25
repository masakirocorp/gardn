#!/usr/bin/env bash
set -euo pipefail
repo_dir="${GARDN_REPO_DIR:-/repo}"
hook_source="$repo_dir/apps/gardn/src/integration/assets/mastracode/gardn-agent-state.sh"
workdir="${GARDN_MASTRACODE_STATUS_TEST_DIR:-$(mktemp -d)}"
home="$workdir/home"; mastra_home="$home/.mastracode"
socket_path="$workdir/gardn.sock"; request_log="$workdir/gardn-requests.jsonl"; output="$workdir/output.json"
provider_url="${GARDN_DETERMINISTIC_PROVIDER_URL:?MastraCode harness requires deterministic provider}"
[[ -f "$hook_source" ]] || { echo "MastraCode status test needs Gardn repo mounted at $repo_dir" >&2; exit 1; }
mkdir -p "$mastra_home/hooks"; cp "$hook_source" "$mastra_home/hooks/gardn-agent-state.sh"; chmod +x "$mastra_home/hooks/gardn-agent-state.sh"
python3 - "$mastra_home/hooks.json" "$mastra_home/hooks/gardn-agent-state.sh" <<'PY'
import json, sys
path, hook = sys.argv[1:3]
events = {"SessionStart":"session","UserPromptSubmit":"working","AgentStart":"working","PreToolUse":"working","PermissionRequest":"blocked","PermissionResult":"working","SubagentStart":"working","SubagentEnd":"working","Interrupt":"idle","AgentEnd":"idle","Stop":"idle"}
with open(path, "w", encoding="utf-8") as f: json.dump({event:[{"type":"command","command":f"bash {hook} {action}","timeout":10000}] for event, action in events.items()}, f)
PY
printf '{"models":{"modeDefaults":{"build":"gardn/gardn-tool"}},"customProviders":[{"name":"gardn","url":"%s/v1","apiKey":"gardn-deterministic-key","models":["gardn-tool"]}]}\n' "$provider_url" > "$mastra_home/settings.json"
python3 - "$socket_path" "$request_log" <<'PY' &
import json, os, socket, sys, time
path, log = sys.argv[1:3]
try: os.unlink(path)
except FileNotFoundError: pass
server=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); server.bind(path); server.listen(16); server.settimeout(.2)
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
printf '{"session_id":"gardn-mastracode-fixture"}\n' | \
  HOME="$home" GARDN_ENV=1 GARDN_SOCKET_PATH="$socket_path" GARDN_PANE_ID=pane-mastracode \
  bash "$mastra_home/hooks/gardn-agent-state.sh" session
set +e
HOME="$home" MASTRA_APP_DATA_DIR="$mastra_home" GARDN_ENV=1 GARDN_SOCKET_PATH="$socket_path" GARDN_PANE_ID=pane-mastracode MASTRACODE_DISABLE_MCP=1 MASTRACODE_DISABLE_MEMORY=1 mastracode \
 --settings "$mastra_home/settings.json" --permission-mode auto --max-turns 4 --timeout 120 --output json \
 --prompt "Use the shell tool to run exactly: printf GARDN_PROVIDER_TOOL_OK. Do not answer before running it." > "$output" 2>"${output}.stderr"
status=$?
set -e
if [[ "$status" -ne 0 ]] || ! grep -Fq GARDN_TOOL_COMPLETE "$output"; then
  cat "$output" >&2
  cat "${output}.stderr" >&2
  cat "$request_log" >&2
  [[ -n "${GARDN_DETERMINISTIC_PROVIDER_LOG:-}" ]] && cat "$GARDN_DETERMINISTIC_PROVIDER_LOG" >&2
  [[ "$status" -ne 0 ]] && exit "$status"
  exit 1
fi
python3 - "$request_log" <<'PY'
import json, sys
from pathlib import Path
items=[json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
sessions=[x for x in items if x.get("method")=="pane.report_agent_session"]
reports=[x for x in items if x.get("method")=="pane.report_agent"]
if not sessions: raise SystemExit("MastraCode emitted no session report")
for x in sessions+reports:
 p=x.get("params",{}); assert p.get("pane_id")=="pane-mastracode",p; assert p.get("source")=="gardn:mastracode",p; assert p.get("agent")=="mastracode",p
states=[x.get("params",{}).get("state") for x in reports]; position=0
for expected in ("working","blocked","working","idle"):
 try: position=states.index(expected,position)+1
 except ValueError as error: raise SystemExit(f"MastraCode missing {expected}; observed {states}") from error
PY
printf 'MastraCode deterministic lifecycle test ok\n'
