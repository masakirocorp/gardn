#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ZSH_LOGIN=(env TERM=xterm-256color zsh -lic)
PI_SESSION_MANAGER_MODULE="${PI_SESSION_MANAGER_MODULE:-$HOME/.bun/install/global/node_modules/@earendil-works/pi-coding-agent/dist/core/session-manager.js}"

echo "== hako restore command tests =="
cargo test --locked planner_preserves_launch_command_for_every_resumable_agent
cargo test --locked restore_plan_preserves_saved_launch_command_for_every_resumable_agent
cargo test --locked pending_agent_resume_executes_every_supported_restore_argv_shape

if ! command -v zsh >/dev/null 2>&1; then
  echo "missing zsh; cannot verify profile shell aliases" >&2
  exit 1
fi

echo "== profile wrapper availability =="
"${ZSH_LOGIN[@]}" '
  for cmd in omp-mk oc-mk codex-mk; do
    if type "$cmd" >/dev/null 2>&1; then
      echo "$cmd: available"
    else
      echo "$cmd: missing"
      exit 1
    fi
  done
'

echo "== profile wrapper CLI probes =="
"${ZSH_LOGIN[@]}" 'omp-mk --version'
"${ZSH_LOGIN[@]}" 'oc-mk --version'
"${ZSH_LOGIN[@]}" 'codex-mk --version'

echo "== opencode profile session scope =="
"${ZSH_LOGIN[@]}" 'oc-mk session list >/dev/null'

echo "== codex profile resume surface =="
"${ZSH_LOGIN[@]}" 'codex-mk resume --help >/dev/null'

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
