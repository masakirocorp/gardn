#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_SMOKE_QODER_PROXY_MODEL:-${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_openrouter_bare_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_SMOKE_QODER_PROXY_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
workdir="${HAKO_QODER_PROXY_STATUS_SMOKE_DIR:-$(mktemp -d)}"
proxy_log="$workdir/qoder-proxy.log"
repo_dir="${HAKO_REPO_DIR:-/repo}"
socket_path="$workdir/hako.sock"
request_log="$workdir/hako-requests.jsonl"


if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "qoder proxy status test needs OPENROUTER_API_KEY" >&2
  exit 1
fi
if [[ -z "${QODER_PERSONAL_ACCESS_TOKEN:-}" ]]; then
  echo "qoder proxy status test needs QODER_PERSONAL_ACCESS_TOKEN" >&2
  exit 1
fi
if [[ "$(id -u)" != "0" ]]; then
  echo "qoder proxy status test must run as root so it can bind :443" >&2
  exit 1
fi
if ! getent hosts api1.qoder.sh | grep -q '127\.0\.0\.1'; then
  echo "qoder proxy status test needs docker --add-host api1.qoder.sh:127.0.0.1" >&2
  exit 1
fi

mkdir -p "$workdir"

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
  -keyout "$workdir/qoder.key" \
  -out "$workdir/qoder.crt" \
  -days 1 \
  -subj "/CN=qoder.sh" \
  -addext "subjectAltName=DNS:api1.qoder.sh,DNS:api2.qoder.sh" >/dev/null 2>&1

HAKO_QODER_PROXY_CERT="$workdir/qoder.crt" \
HAKO_QODER_PROXY_KEY="$workdir/qoder.key" \
HAKO_QODER_PROXY_LOG="$proxy_log" \
OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
HAKO_SMOKE_QODER_PROXY_MODEL="$model" \
node /usr/local/bin/hako-agent-qoder-openrouter-proxy &
proxy_pid=$!
for _ in $(seq 1 50); do
  grep -q 'qoder-proxy-listening' "$proxy_log" 2>/dev/null && break
  sleep 0.1
done
grep -q 'qoder-proxy-listening' "$proxy_log" || { echo "qoder proxy did not start" >&2; exit 1; }

home="$workdir/home"
mkdir -p "$home/.qoder/hooks"
cp "$repo_dir/src/integration/assets/qodercli/hako-agent-state.sh" "$home/.qoder/hooks/hako-agent-state.sh"
chmod +x "$home/.qoder/hooks/hako-agent-state.sh"
cat > "$home/.qoder/settings.json" <<EOF_HOOKS
{
  "general": {
    "enableAutoUpdate": false
  },
  "hooks": {
    "SessionStart": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash $home/.qoder/hooks/hako-agent-state.sh idle", "timeout": 10}]}],
    "UserPromptSubmit": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash $home/.qoder/hooks/hako-agent-state.sh working", "timeout": 10}]}],
    "PreToolUse": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash $home/.qoder/hooks/hako-agent-state.sh working", "timeout": 10}]}],
    "PermissionRequest": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash $home/.qoder/hooks/hako-agent-state.sh blocked", "timeout": 10}]}],
    "Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash $home/.qoder/hooks/hako-agent-state.sh idle", "timeout": 10}]}],
    "SessionEnd": [{"matcher": "*", "hooks": [{"type": "command", "command": "bash $home/.qoder/hooks/hako-agent-state.sh release", "timeout": 10}]}]
  }
}
EOF_HOOKS
set +e
(
  cd "$workdir"
  HOME="$home" \
  HAKO_ENV=1 \
  HAKO_SOCKET_PATH="$socket_path" \
  HAKO_PANE_ID="pane-qoder-proxy" \
  NODE_EXTRA_CA_CERTS="$workdir/qoder.crt" \
  SSL_CERT_FILE="$workdir/qoder.crt" \
  REQUESTS_CA_BUNDLE="$workdir/qoder.crt" \
  timeout "${HAKO_QODER_PROXY_STATUS_SMOKE_TIMEOUT:-180}" qodercli \
    -p \
    --output-format json \
    --permission-mode dont_ask \
    --model "${HAKO_SMOKE_QODER_CLI_MODEL:-Qwen3.7-Max}" \
    "Reply exactly HAKO_QODER_PROXY_OK" >"$workdir/qoder-output.jsonl" 2>&1
)
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  python3 - "$workdir/qoder-output.jsonl" "$proxy_log" <<'PY' >&2
import sys
from pathlib import Path
for path in sys.argv[1:]:
    text = Path(path).read_text(errors='replace') if Path(path).exists() else ''
    print(f'--- {path} ---')
    print('\n'.join(text.splitlines()[:160]))
PY
  exit "$status"
fi

python3 - "$workdir/qoder-output.jsonl" "$proxy_log" "$request_log" <<'PY'
import json, sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors='replace')
proxy = Path(sys.argv[2]).read_text(errors='replace')
requests = [json.loads(line) for line in Path(sys.argv[3]).read_text(errors='replace').splitlines() if line.strip()]
if 'HAKO_QODER_PROXY_OK' not in output:
    raise SystemExit(f'qoder proxy smoke did not produce marker: {output[-1000:]}')
if 'model-list status=200' not in proxy:
    raise SystemExit(f'qoder proxy log missing model-list status=200: {proxy[-1000:]}')
for needle in ['openrouter-request', 'openrouter-complete']:
    if needle not in proxy:
        raise SystemExit(f'qoder proxy log missing {needle}: {proxy[-1000:]}')
reports = [req for req in requests if req.get('method') == 'pane.report_agent']
releases = [req for req in requests if req.get('method') == 'pane.release_agent']
states = [req.get('params', {}).get('state') for req in reports]
if 'working' not in states:
    raise SystemExit(f'qoder hook smoke did not report working state: {requests}')
if 'idle' not in states:
    raise SystemExit(f'qoder hook smoke did not report idle state: {requests}')
if not releases:
    raise SystemExit(f'qoder hook smoke did not release agent: {requests}')
for req in reports + releases:
    params = req.get('params', {})
    if params.get('source') != 'hako:qodercli' or params.get('agent') != 'qodercli':
        raise SystemExit(f'qoder hook smoke reported wrong source/agent: {req}')
print('qoder proxy status test ok: real Qoder CLI completed through local OpenRouter-backed inference proxy and emitted Hako status hooks')
PY
