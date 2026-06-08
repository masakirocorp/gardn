---
status: accepted
---

# Isolate multi-agent work in task worktrees

Hako uses a shared integration checkout for final landing and isolated task worktrees for bigger, risky, parallel, or dirty-shared-checkout work. Read-only investigation can happen in the shared checkout. Small linear changes may happen in the default main worktree when the working tree is clean and no unrelated implementation is in progress.

Worktrunk (`wt`) is the task-worktree tool for Hako. It owns worktree creation, switching, listing, merging, and removal. Worktrunk's configured path template owns task worktree locations. Task branches use `<tracker-key>-<slug>` when a tracker ticket exists.

Current rationale: agents in one checkout share mutable files, generated artifacts, test outputs, branches, and commits. Parallel agents editing the same checkout can corrupt each other's assumptions or accidentally include unrelated work. Separate worktrees provide each task with its own index, working tree, and branch while preserving a shared integration checkout for final landing. Worktrunk is used because it makes worktree creation, navigation, status listing, merging, and cleanup closer to branch-level ergonomics.

## Considered options

- Let every agent work directly in the shared checkout. Rejected because unrelated edits, generated files, formatter passes, and commits become ambiguous under parallel work.
- Require task worktrees for every task. Rejected because read-only investigation and small clean linear edits would pay unnecessary workflow overhead.
- Use plain `git worktree` as the normal workflow. Rejected because it preserves isolation but leaves agents and humans to manually coordinate path templates, branch names, switching, merge steps, and cleanup.
- Use Worktrunk as the normal task-worktree workflow. Accepted because it preserves the isolation invariant while reducing manual worktree friction.

## Consequences

When using a task worktree, all code edits, tests, validation, and commits happen on the task branch inside that worktree. Do not treat the task branch as the final landing branch. After validation, land the final commit(s) on `origin/master` through an equivalent `wt merge` flow, then remove the task worktree and delete the local and remote task branch with `wt remove`. A session already inside an isolated task worktree should keep using it and must not create nested worktrees.

Agents should not assume ownership of unrelated changes in the shared checkout. If the shared checkout is not clean or unrelated implementation is already in progress, new non-trivial editing should move to a Worktrunk task worktree instead of trying to distinguish ownership by memory. This ADR records collaboration safety policy; it does not change release or upstream-sync policy.

Historical rationale beyond the current repository instructions is `[INFERENCE]`: this policy likely exists because Hako is often changed by multiple agent sessions, and filesystem-level isolation is more reliable than relying on chat context to prevent accidental overlap.
