#!/usr/bin/env bash
set -euo pipefail

bins=(claude codex opencode copilot hermes droid kimi kiro-cli cursor-agent qoder qodercli omp pi jq python3 node pnpm git)
missing=0
for bin in "${bins[@]}"; do
  if ! command -v "$bin" >/dev/null 2>&1; then
    printf 'missing: %s\n' "$bin" >&2
    missing=1
  fi
done

if (( missing )); then
  exit 1
fi

printf 'agent smoke image ok\n'
printf 'node: '; node --version
printf 'pnpm: '; pnpm --version
for bin in claude codex opencode copilot hermes droid kimi kiro-cli cursor-agent qoder qodercli omp pi; do
  printf '%s: ' "$bin"
  "$bin" --version 2>&1 | tr '\n' ' '
  printf '\n'
done
