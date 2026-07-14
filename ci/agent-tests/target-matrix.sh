#!/usr/bin/env bash
set -euo pipefail

all_targets='["opencode","pi","omp","claude","codex","copilot","cursor","qoder","devin","droid","kimi","hermes","maki"]'
target="${1:-all}"
case "$target" in
  all|'')
    printf '%s\n' "$all_targets"
    ;;
  opencode|pi|omp|claude|codex|copilot|cursor|qoder|devin|droid|kimi|hermes|maki)
    printf '["%s"]\n' "$target"
    ;;
  *)
    echo "unknown agent test target: $target" >&2
    exit 2
    ;;
esac
