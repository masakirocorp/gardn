#!/usr/bin/env bash
set -euo pipefail
repo_dir="${GARDN_REPO_DIR:-/repo}"
hook_source="$repo_dir/apps/gardn/src/integration/assets/antigravity_cli/gardn-agent-session.sh"
workdir="${GARDN_ANTIGRAVITY_STATUS_TEST_DIR:-$(mktemp -d)}"
home="$workdir/home"; config_dir="$home/.gemini/antigravity-cli"
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
HOME="$home" GARDN_ENV=1 GARDN_SOCKET_PATH="$socket_path" GARDN_PANE_ID=pane-antigravity \
python3 - "$prompt" "$expected" "$output" <<'PY'
import fcntl, os, pty, re, select, signal, struct, subprocess, sys, termios, time
from pathlib import Path
prompt, expected, output = sys.argv[1:4]
env=os.environ.copy(); env.update({"TERM":"xterm-256color","COLORTERM":"truecolor","COLUMNS":"120","LINES":"40"})
master,slave=pty.openpty(); fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack("HHHH",40,120,0,0))
proc=subprocess.Popen(["agy"],stdin=slave,stdout=slave,stderr=slave,env=env,cwd="/work",start_new_session=True); os.close(slave)
raw=bytearray(); sent=False; selected_theme=False; theme_selected_at=None; deadline=time.monotonic()+90
try:
  while time.monotonic()<deadline:
    readable,_,_=select.select([master],[],[],.25)
    if readable:
      try: raw.extend(os.read(master,65536))
      except OSError: break
    text=re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]",b"",bytes(raw)).replace(b"\r",b"").decode("utf-8","replace")
    if not selected_theme and "Choose your color scheme:" in text:
      os.write(master,b"\r")
      selected_theme=True
      theme_selected_at=time.monotonic()
    if not sent and "Gemini API key" in text and (theme_selected_at is None or time.monotonic()-theme_selected_at>.5):
      os.write(master,prompt.encode()+b"\r")
      sent=True
    if sent and expected in text: break
    if proc.poll() is not None and not readable: break
  Path(output).write_text(text,encoding="utf-8")
  if expected not in text: raise RuntimeError(f"Antigravity completion missing {expected}; process={proc.poll()} tail={text[-2000:]!r}")
finally:
  if proc.poll() is None:
    os.killpg(proc.pid,signal.SIGTERM)
    try: proc.wait(timeout=5)
    except subprocess.TimeoutExpired: os.killpg(proc.pid,signal.SIGKILL)
  os.close(master)
PY
python3 - "$request_log" <<'PY'
import json, sys
from pathlib import Path
items=[json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
sessions=[x for x in items if x.get("method")=="pane.report_agent_session"]
if not sessions: raise SystemExit("Antigravity emitted no session report")
params=sessions[-1].get("params",{}); assert params.get("pane_id")=="pane-antigravity",params; assert params.get("source")=="gardn:antigravity_cli",params; assert params.get("agent")=="agy",params
PY
printf 'Antigravity %s Gemini status test ok\n' "${GARDN_ANTIGRAVITY_REAL:+real}"
