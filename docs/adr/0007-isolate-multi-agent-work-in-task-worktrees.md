---
status: accepted
---

# Isolate multi-agent work in task worktrees

Hako uses `../hako` as the shared integration checkout and `../hako-worktrees/<task-slug>` for isolated task worktrees. Read-only investigation can happen in the shared checkout. Small linear changes may happen in the default main worktree when the working tree is clean and no unrelated implementation is in progress. Bigger features, risky refactors, parallel edits, or new non-trivial work when the shared checkout already contains unrelated changes should use a dedicated task worktree and a task branch named `<tracker-key>-<slug>` when a tracker ticket exists.

Current rationale: agents in one checkout share mutable files, generated artifacts, test outputs, branches, and commits. Parallel agents editing the same checkout can corrupt each other's assumptions or accidentally include unrelated work. Separate worktrees provide each task with its own index, working tree, and branch while preserving a shared integration checkout for final landing.

## Considered options

- Let every agent work directly in the shared checkout. Rejected because unrelated edits, generated files, formatter passes, and commits become ambiguous under parallel work.
- Require worktrees for every task. Rejected because read-only investigation and small clean linear edits would pay unnecessary workflow overhead.
- Use worktrees when isolation changes correctness or coordination risk. Accepted because it keeps simple work simple while protecting larger, risky, or concurrent changes.

## Consequences

When using a task worktree, all code edits, tests, validation, and commits happen on the task branch inside that worktree. Do not treat the task branch as the final landing branch. After validation, fast-forward the shared integration checkout at `../hako` to the task branch commit, push `origin/master` from `../hako`, then remove the task worktree and delete the local and remote task branch. A session already inside an isolated task worktree should keep using it and must not create nested worktrees.

Agents should not assume ownership of unrelated changes in the shared checkout. If the shared checkout is not clean or unrelated implementation is already in progress, new non-trivial editing should move to a task worktree instead of trying to distinguish ownership by memory. This ADR records collaboration safety policy; it does not change Git branching, release, or upstream-sync policy.

Historical rationale beyond the current repository instructions is `[INFERENCE]`: this policy likely exists because Hako is often changed by multiple agent sessions, and filesystem-level isolation is more reliable than relying on chat context to prevent accidental overlap.
