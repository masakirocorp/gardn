---
packages:
  omh: patch
---

# Improve Local API and session correctness

Pane and agent reads now report when older requested rows were omitted. Local API errors, event delivery, workspace focus, Git metadata, pane history, and session restoration now remain consistent across reconnects, alternate screens, workspace changes, and client handoff.
