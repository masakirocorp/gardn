#!/usr/bin/env bash
set -euo pipefail

target="${1:?usage: omh-agent-tests-target <agent>}"
case "$target" in
  opencode)
    exec omh-agent-tests-opencode-status
    ;;
  pi|omp)
    export OMH_PI_OMP_STATUS_TARGET="$target"
    exec omh-agent-tests-pi-omp-status
    ;;
  claude)
    exec omh-agent-tests-claude-status
    ;;
  codex)
    exec omh-agent-tests-codex-status
    ;;
  copilot|devin|droid|kimi|hermes)
    export OMH_REMAINING_STATUS_TARGET="$target"
    exec omh-agent-tests-remaining-status
    ;;
  cursor)
    exec omh-agent-tests-cursor-proxy-status
    ;;
  qoder)
    exec omh-agent-tests-qoder-proxy-status
    ;;
  maki)
    exec omh-agent-tests-maki-status
    ;;
  *)
    echo "unknown agent test target: $target" >&2
    exit 2
    ;;
esac
