#!/usr/bin/env bash
set -euo pipefail

model="${HAKO_SMOKE_CURSOR_MODEL:-${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}}"
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
    "sessionStart": [{"hooks": [{"type": "command", "command": "bash $HOME/.cursor/hako-agent-state.sh idle", "timeout": 10}]}],
    "beforeSubmitPrompt": [{"hooks": [{"type": "command", "command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}]}],
    "beforeShellExecution": [{"hooks": [{"type": "command", "command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}]}],
    "beforeMCPExecution": [{"hooks": [{"type": "command", "command": "bash $HOME/.cursor/hako-agent-state.sh working", "timeout": 10}]}],
    "stop": [{"hooks": [{"type": "command", "command": "bash $HOME/.cursor/hako-agent-state.sh idle", "timeout": 10}]}],
    "sessionEnd": [{"hooks": [{"type": "command", "command": "bash $HOME/.cursor/hako-agent-state.sh release", "timeout": 10}]}]
  }
}
EOF_HOOKS

HAKO_CURSOR_PROXY_CERT="$workdir/cursor.crt" \
HAKO_CURSOR_PROXY_KEY="$workdir/cursor.key" \
HAKO_CURSOR_PROXY_LOG="$proxy_log" \
OPENROUTER_API_KEY="$OPENROUTER_API_KEY" \
HAKO_SMOKE_CURSOR_MODEL="$model" \
node /usr/local/bin/hako-agent-cursor-openrouter-proxy &
proxy_pid=$!
for _ in $(seq 1 50); do
  grep -q 'cursor-proxy-listening' "$proxy_log" 2>/dev/null && break
  sleep 0.1
done
grep -q 'cursor-proxy-listening' "$proxy_log" || { echo "cursor proxy did not start" >&2; exit 1; }

(
  cd "$workdir"
  HAKO_SOCKET="$socket_path" \
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
    "Reply exactly HAKO_CURSOR_PROXY_OK" >"$workdir/cursor-output.txt" 2>&1
)

python3 - "$workdir/cursor-output.txt" "$proxy_log" "$request_log" <<'PY'
import json, sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors='replace')
proxy = Path(sys.argv[2]).read_text(errors='replace')
requests = [json.loads(line) for line in Path(sys.argv[3]).read_text(errors='replace').splitlines() if line.strip()]
if 'HAKO_CURSOR_PROXY_OK' not in output:
    raise SystemExit(f'cursor proxy smoke did not produce marker: {output[-1000:]}')
for needle in ['unary', 'agent-stream', 'openrouter-request', 'openrouter-complete']:
    if needle not in proxy:
        raise SystemExit(f'cursor proxy log missing {needle}: {proxy[-1000:]}')
print('cursor proxy status test ok: real Cursor CLI completed through local OpenRouter proxy; Cursor status remains covered by hook-seam smoke because --print does not emit hooks')
PY
