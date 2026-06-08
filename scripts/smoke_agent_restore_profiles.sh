#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ZSH_LOGIN=(env TERM=xterm-256color zsh -lic)
PI_SESSION_MANAGER_MODULE="${PI_SESSION_MANAGER_MODULE:-$HOME/.bun/install/global/node_modules/@earendil-works/pi-coding-agent/dist/core/session-manager.js}"
OMP_RESTORE_WRAPPER="${OMP_RESTORE_WRAPPER:-omp}"
OPENCODE_RESTORE_WRAPPER="${OPENCODE_RESTORE_WRAPPER:-opencode}"
CODEX_RESTORE_WRAPPER="${CODEX_RESTORE_WRAPPER:-codex}"

echo "== hako restore command tests =="
cargo test --locked planner_preserves_launch_command_for_every_resumable_agent
cargo test --locked restore_plan_preserves_saved_launch_command_for_every_resumable_agent
cargo test --locked pending_agent_resume_executes_profile_environment
cargo test --locked pending_agent_resume_executes_every_supported_restore_argv_shape

if ! command -v zsh >/dev/null 2>&1; then
  echo "missing zsh; cannot verify shell-visible restore wrappers" >&2
  exit 1
fi

echo "== configured restore wrapper availability =="
OMP_RESTORE_WRAPPER="$OMP_RESTORE_WRAPPER" \
OPENCODE_RESTORE_WRAPPER="$OPENCODE_RESTORE_WRAPPER" \
CODEX_RESTORE_WRAPPER="$CODEX_RESTORE_WRAPPER" \
"${ZSH_LOGIN[@]}" '
  run_wrapper() {
    case "$1" in
      *[!A-Za-z0-9_./-]*|"") echo "invalid wrapper command: $1" >&2; exit 1 ;;
    esac
    eval "$1 $2"
  }
  for cmd in "$OMP_RESTORE_WRAPPER" "$OPENCODE_RESTORE_WRAPPER" "$CODEX_RESTORE_WRAPPER"; do
    if type "$cmd" >/dev/null 2>&1; then
      echo "$cmd: available"
    else
      echo "$cmd: missing"
      exit 1
    fi
  done
  echo "== configured restore wrapper CLI probes =="
  run_wrapper "$OMP_RESTORE_WRAPPER" "--version"
  run_wrapper "$OPENCODE_RESTORE_WRAPPER" "--version"
  run_wrapper "$CODEX_RESTORE_WRAPPER" "--version"
  echo "== opencode restore wrapper session scope =="
  run_wrapper "$OPENCODE_RESTORE_WRAPPER" "session list >/dev/null"
  echo "== codex restore wrapper resume surface =="
  run_wrapper "$CODEX_RESTORE_WRAPPER" "resume --help >/dev/null"
'
if ! command -v bun >/dev/null 2>&1; then
  echo "bun missing; cannot verify OMP SessionManager behavior" >&2
  exit 1
fi
if [[ ! -f "$PI_SESSION_MANAGER_MODULE" ]]; then
  echo "missing Pi/OMP SessionManager module: $PI_SESSION_MANAGER_MODULE" >&2
  exit 1
fi

echo "== omp session-dir narrowing repro/fix =="
PI_SESSION_MANAGER_MODULE="$PI_SESSION_MANAGER_MODULE" bun -e '
  import { mkdtempSync, mkdirSync, writeFileSync } from "fs";
  import { join } from "path";
  import { tmpdir } from "os";

  const { SessionManager } = await import(process.env.PI_SESSION_MANAGER_MODULE);
  const root = mkdtempSync(join(tmpdir(), "hako-omp-restore-smoke-"));
  const cwd = process.cwd();
  const projectDir = join(root, "sessions", "-projects-masakiro-hako");
  const childDir = join(projectDir, "2026-child");
  mkdirSync(childDir, { recursive: true });

  const header = (id) => JSON.stringify({
    type: "session",
    version: 3,
    id,
    timestamp: "2026-06-08T00:00:00.000Z",
    cwd,
  }) + "\n";
  const rootSession = join(projectDir, "2026-root_root.jsonl");
  const childSession = join(childDir, "RightSidebarHierarchyReview.jsonl");
  writeFileSync(rootSession, header("root"));
  writeFileSync(childSession, header("child"));

  const narrowed = SessionManager.open(childSession);
  if (narrowed.getSessionDir() !== childDir) {
    throw new Error(`expected bare path restore to narrow to child dir, got ${narrowed.getSessionDir()}`);
  }

  const fixed = SessionManager.open(childSession, projectDir);
  if (fixed.getSessionDir() !== projectDir) {
    throw new Error(`expected path+sessionDir restore to keep project dir, got ${fixed.getSessionDir()}`);
  }

  const fixedList = await SessionManager.list(cwd, fixed.getSessionDir());
  if (!fixedList.some((session) => session.path === rootSession)) {
    throw new Error("expected project root session in fixed /resume listing");
  }
'

echo "agent restore profile smoke passed"
