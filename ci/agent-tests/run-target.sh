#!/usr/bin/env bash
set -euo pipefail

target="${1:?usage: gardn-agent-tests-target <agent>}"
case "$target" in
  opencode)
    exec gardn-agent-tests-opencode-status
    ;;
  pi|omp)
    export GARDN_PI_OMP_STATUS_TARGET="$target"
    exec gardn-agent-tests-pi-omp-status
    ;;
  claude)
    exec gardn-agent-tests-claude-status
    ;;
  codex)
    exec gardn-agent-tests-codex-status
    ;;
  copilot|devin|droid|kimi|hermes)
    export GARDN_REMAINING_STATUS_TARGET="$target"
    exec gardn-agent-tests-remaining-status
    ;;
  cursor)
    exec gardn-agent-tests-cursor-proxy-status
    ;;
  qoder)
    exec gardn-agent-tests-qoder-proxy-status
    ;;
  maki)
    exec gardn-agent-tests-maki-status
    ;;
  qwen)
    exec gardn-agent-tests-qwen-status
    ;;
  kilo)
    exec gardn-agent-tests-kilo-status
    ;;
  *)
    echo "unknown agent test target: $target" >&2
    exit 2
    ;;
esac
