---
status: superseded
---

# Supersede native Git diff sessions

Oh My Herdr no longer owns an in-app native Git diff viewer. Workspace-scoped Git diff actions resolve the target repository through ADR 0019's observed-repo rules, then open the configured external diff command in a managed command tab.

The default external command is `lazygit`. Users can set `[git].diff_command` to another terminal Git UI or command. Oh My Herdr owns repo-target selection and managed command-tab reuse; the external command owns file lists, hunk navigation, staging, unstaging, discard/restore operations, syntax highlighting, and any agent handoff workflow.

## Consequences

Native diff parser, renderer, syntax-highlighting, staging, hunk-selection, and selected-diff agent-payload behavior are not active Oh My Herdr product contracts.

Agent integration smokes cover lifecycle/status reporting, not whether agents understand Oh My Herdr-selected diff payloads.

Persisted snapshots may continue to tolerate old native-diff fields for compatibility, but new behavior should not produce native diff sessions.
