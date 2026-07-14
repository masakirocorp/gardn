#!/usr/bin/env bash
set -euo pipefail

target="${1:?usage: hako-agent-tests-target <agent>}"
case "$target" in
  opencode)
    exec hako-agent-tests-opencode-status
    ;;
  pi|omp)
    export HAKO_PI_OMP_STATUS_TARGET="$target"
    exec hako-agent-tests-pi-omp-status
    ;;
  claude)
    exec hako-agent-tests-claude-status
    ;;
  codex)
    exec hako-agent-tests-codex-status
    ;;
  copilot|devin|droid|kimi|hermes)
    export HAKO_REMAINING_STATUS_TARGET="$target"
    exec hako-agent-tests-remaining-status
    ;;
  cursor)
    exec hako-agent-tests-cursor-proxy-status
    ;;
  qoder)
    exec hako-agent-tests-qoder-proxy-status
    ;;
  maki)
    exec hako-agent-tests-maki-status
    ;;
  *)
    echo "unknown agent test target: $target" >&2
    exit 2
    ;;
esac
