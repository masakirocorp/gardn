#!/usr/bin/env bash
set -euo pipefail

model="${HAKO_SMOKE_QODER_PROXY_MODEL:-${HAKO_SMOKE_MODEL:-poolside/laguna-m.1:free}}"
workdir="${HAKO_QODER_PROXY_STATUS_SMOKE_DIR:-$(mktemp -d)}"
proxy_log="$workdir/qoder-proxy.log"

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
openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$workdir/qoder.key" \
  -out "$workdir/qoder.crt" \
  -days 1 \
  -subj "/CN=api1.qoder.sh" \
  -addext "subjectAltName=DNS:api1.qoder.sh" >/dev/null 2>&1

proxy_pid=""
trap '[[ -n "${proxy_pid:-}" ]] && kill "$proxy_pid" >/dev/null 2>&1 || true' EXIT

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
mkdir -p "$home"
(
  cd "$workdir"
  HOME="$home" \
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

python3 - "$workdir/qoder-output.jsonl" "$proxy_log" <<'PY'
import sys
from pathlib import Path
output = Path(sys.argv[1]).read_text(errors='replace')
proxy = Path(sys.argv[2]).read_text(errors='replace')
if 'HAKO_QODER_PROXY_OK' not in output:
    raise SystemExit(f'qoder proxy smoke did not produce marker: {output[-1000:]}')
for needle in ['model-list status=200', 'openrouter-request', 'openrouter-complete']:
    if needle not in proxy:
        raise SystemExit(f'qoder proxy log missing {needle}: {proxy[-1000:]}')
print('qoder proxy status test ok: real Qoder CLI completed through local OpenRouter-backed inference proxy')
PY
