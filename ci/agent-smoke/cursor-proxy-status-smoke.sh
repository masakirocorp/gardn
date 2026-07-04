#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_SMOKE_CURSOR_MODEL:-${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_openrouter_bare_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_SMOKE_CURSOR_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
repo_dir="${HAKO_REPO_DIR:-/repo}"
workdir="${HAKO_CURSOR_PROXY_STATUS_SMOKE_DIR:-$(mktemp -d)}"
socket_path="$workdir/hako.sock"
request_log="$workdir/hako-requests.jsonl"
proxy_log="$workdir/cursor-proxy.log"


if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "cursor proxy status test needs OPENROUTER_API_KEY" >&2
  exit 1
fi
if [[ "$(id -u)" != "0" ]]; then
  echo "cursor proxy status test must run as root so it can trust a test CA and bind :443" >&2
  exit 1
fi
for domain in api2.cursor.sh api2geo.cursor.sh api2direct.cursor.sh agentn.api5.cursor.sh agent.api5.cursor.sh; do
  if ! getent hosts "$domain" | grep -q '127\.0\.0\.1'; then
    echo "cursor proxy status test needs docker --add-host $domain:127.0.0.1" >&2
    exit 1
  fi
done

mkdir -p "$workdir" "$HOME/.cursor"

python3 - "$socket_path" "$request_log" <<'PY' &
import json, os, socket, sys, time
socket_path, request_log = sys.argv[1:3]
try: os.unlink(socket_path)
except FileNotFoundError: pass
server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(socket_path)
server.listen(16)
server.settimeout(0.2)
deadline = time.time() + 300
with open(request_log, 'a', encoding='utf-8') as out:
    while time.time() < deadline:
        try: conn, _ = server.accept()
        except TimeoutError: continue
        except OSError: break
        with conn:
            conn.settimeout(1)
            data = b''
            while not data.endswith(b'\n'):
                try: chunk = conn.recv(4096)
                except TimeoutError: break
                if not chunk: break
                data += chunk
            if not data: continue
            out.write(data.decode('utf-8', 'replace'))
            out.flush()
            try:
                req = json.loads(data)
                conn.sendall((json.dumps({'id': req.get('id'), 'result': {'type': 'ok'}}) + '\n').encode())
            except Exception:
                pass
PY
server_pid=$!
proxy_pid=""
trap '[[ -n "${proxy_pid:-}" ]] && kill "$proxy_pid" >/dev/null 2>&1 || true; kill "$server_pid" >/dev/null 2>&1 || true' EXIT
for _ in $(seq 1 50); do [[ -S "$socket_path" ]] && break; sleep 0.1; done
[[ -S "$socket_path" ]] || { echo "fake hako socket did not start" >&2; exit 1; }

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$workdir/cursor.key" \
  -out "$workdir/cursor.crt" \
  -days 1 \
  -subj "/CN=api2.cursor.sh" \
  -addext "subjectAltName=DNS:api2.cursor.sh,DNS:api2geo.cursor.sh,DNS:api2direct.cursor.sh,DNS:agentn.api5.cursor.sh,DNS:agent.api5.cursor.sh" >/dev/null 2>&1
cp "$workdir/cursor.crt" /usr/local/share/ca-certificates/hako-cursor-proxy.crt
update-ca-certificates >/dev/null

cp "$repo_dir/src/integration/assets/cursor/hako-agent-state.sh" "$HOME/.cursor/hako-agent-state.sh"
chmod +x "$HOME/.cursor/hako-agent-state.sh"
cat > "$HOME/.cursor/hooks.json" <<EOF_HOOKS
{
  "version": 1,
  "hooks": {
    "sessionStart": [{"command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}],
    "beforeSubmitPrompt": [{"command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}],
    "beforeShellExecution": [{"command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}],
    "beforeMCPExecution": [{"command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}],
    "stop": [{"command": "bash $HOME/.cursor/hako-agent-state.sh idle", "timeout": 10}],
    "sessionEnd": [{"command": "bash $HOME/.cursor/hako-agent-state.sh release", "timeout": 10}]
  }
}
EOF_HOOKS

static_reply="HAKO_CURSOR_PROXY_OK"
HAKO_CURSOR_PROXY_CERT="$workdir/cursor.crt" \
HAKO_CURSOR_PROXY_KEY="$workdir/cursor.key" \
HAKO_CURSOR_PROXY_LOG="$proxy_log" \
OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
HAKO_SMOKE_CURSOR_MODEL="$model" \
HAKO_CURSOR_PROXY_STATIC_REPLY="$static_reply" \
node /usr/local/bin/hako-agent-cursor-openrouter-proxy &
proxy_pid=$!
for _ in $(seq 1 50); do
  grep -q 'cursor-proxy-listening' "$proxy_log" 2>/dev/null && break
  sleep 0.1
done
grep -q 'cursor-proxy-listening' "$proxy_log" || { echo "cursor proxy did not start" >&2; exit 1; }

generic_prompt='Reply exactly HAKO_CURSOR_PROXY_OK and nothing else.'

(
  cd "$workdir"
  HAKO_ENV=1 \
  HAKO_SOCKET_PATH="$socket_path" \
  HAKO_PANE_ID="pane-cursor-proxy" \
  NODE_EXTRA_CA_CERTS="$workdir/cursor.crt" \
  SSL_CERT_FILE="$workdir/cursor.crt" \
  REQUESTS_CA_BUNDLE="$workdir/cursor.crt" \
  CURSOR_API_KEY="$OPENROUTER_API_KEY" \
  timeout "${HAKO_CURSOR_PROXY_STATUS_SMOKE_TIMEOUT:-180}" cursor-agent \
    --print \
    --output-format text \
    --trust \
    --api-key "$OPENROUTER_API_KEY" \
    --model "$model" \
    "$generic_prompt" >"$workdir/cursor-output.txt" 2>&1
)

before_interactive_completions="$(grep -c 'static-complete' "$proxy_log" 2>/dev/null || true)"
(
  cd "$workdir"
  HAKO_ENV=1 \
  HAKO_SOCKET_PATH="$socket_path" \
  HAKO_PANE_ID="pane-cursor-proxy-interactive" \
  NODE_EXTRA_CA_CERTS="$workdir/cursor.crt" \
  SSL_CERT_FILE="$workdir/cursor.crt" \
  REQUESTS_CA_BUNDLE="$workdir/cursor.crt" \
  CURSOR_API_KEY="$OPENROUTER_API_KEY" \
  HAKO_CURSOR_INTERACTIVE_ARGS="$(printf '%s\n' --api-key "$OPENROUTER_API_KEY" --model "$model" "$generic_prompt")" \
  timeout "${HAKO_CURSOR_INTERACTIVE_STATUS_SMOKE_TIMEOUT:-120}" python3 - "$workdir/cursor-interactive-output.txt" "$proxy_log" "$before_interactive_completions" <<'PY' || true
import os, select, signal, subprocess, sys, time
out_path, proxy_log, before_count = sys.argv[1], sys.argv[2], int(sys.argv[3])
args = ["cursor-agent", *os.environ["HAKO_CURSOR_INTERACTIVE_ARGS"].splitlines()]
proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=False, preexec_fn=os.setsid)
deadline = time.time() + 90
buf = bytearray()
def completions():
    try:
        return open(proxy_log, encoding="utf-8", errors="replace").read().count("static-complete")
    except FileNotFoundError:
        return 0
try:
    while time.time() < deadline:
        ready, _, _ = select.select([proc.stdout], [], [], 0.2)
        if ready:
            chunk = os.read(proc.stdout.fileno(), 4096)
            if not chunk:
                break
            buf.extend(chunk)
        if completions() > before_count:
            break
        if proc.poll() is not None:
            break
finally:
    try:
        proc.stdin.write(b"\x03")
        proc.stdin.flush()
    except Exception:
        pass
    try:
        os.killpg(proc.pid, signal.SIGINT)
    except Exception:
        pass
    try:
        proc.wait(timeout=5)
    except Exception:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except Exception:
            pass
open(out_path, "wb").write(buf)
PY
)

python3 - "$workdir/cursor-output.txt" "$proxy_log" "$request_log" <<'PY'
import json, sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors='replace')
proxy = Path(sys.argv[2]).read_text(errors='replace')
requests = [json.loads(line) for line in Path(sys.argv[3]).read_text(errors='replace').splitlines() if line.strip()]
if 'HAKO_CURSOR_PROXY_OK' not in output:
    print(f'cursor proxy smoke did not return expected marker: {output[-1000:]}', file=sys.stderr)
    raise SystemExit(75)
for needle in ['unary', 'agent-stream', 'static-complete']:
    if needle not in proxy:
        raise SystemExit(f'cursor proxy log missing {needle}: {proxy[-1000:]}')
states = [req.get('params', {}).get('state') for req in requests if req.get('method') == 'pane.report_agent']
if 'working' not in states or not any(req.get('method') == 'pane.release_agent' for req in requests):
    raise SystemExit(f'cursor hook smoke did not observe working+release from real CLI hooks: {requests}')
print('cursor proxy status test ok: real Cursor CLI completed through deterministic local proxy and emitted Hako status hooks')
PY
