---
status: superseded
---

# Supersede native Git diff sessions

Gardn no longer owns an in-app native Git diff viewer. Workspace-scoped Review actions resolve the target repository through ADR 0019's observed-repo rules, then open the configured external review command in a managed command tab.

The default external review command is `hunk diff --watch`. Users can set `[commands].review` to another terminal review UI or command. Gardn owns repo-target selection and managed command-tab reuse; the external command owns file lists, hunk navigation, staging, unstaging, discard/restore operations, syntax highlighting, and any agent handoff workflow. Browser, Editor, and GitHub are separate curated roles rather than alternate meanings of the review setting.

## Consequences

Native diff parser, renderer, syntax-highlighting, staging, hunk-selection, and selected-diff agent-payload behavior are not active Gardn product contracts.

Agent integration smokes cover lifecycle/status reporting, not whether agents understand Gardn-selected diff payloads.

Persisted snapshots may continue to tolerate old native-diff fields for compatibility, but new behavior should not produce native diff sessions.
