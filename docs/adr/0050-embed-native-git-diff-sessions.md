---
status: superseded
---

# Supersede native Git diff sessions

Gardn no longer owns an in-app native Git diff viewer. Workspace-scoped Git diff actions resolve the target repository through ADR 0019's observed-repo rules, then open the configured external diff command in a managed command tab.

The default external diff command is `hunk diff --watch`. Users can set `[commands].diff` to another terminal diff UI or command. Gardn owns repo-target selection and managed command-tab reuse; the external command owns file lists, hunk navigation, staging, unstaging, discard/restore operations, syntax highlighting, and any agent handoff workflow. Git UI and IDE launchers are separate `[commands].git` and `[commands].ide` roles rather than alternate meanings of the diff setting.

## Consequences

Native diff parser, renderer, syntax-highlighting, staging, hunk-selection, and selected-diff agent-payload behavior are not active Gardn product contracts.

Agent integration smokes cover lifecycle/status reporting, not whether agents understand Gardn-selected diff payloads.

Persisted snapshots may continue to tolerate old native-diff fields for compatibility, but new behavior should not produce native diff sessions.
