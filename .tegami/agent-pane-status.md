---
packages:
  omh: patch
---

### Improve agent status in pane borders

Pane borders can now show an agent name with its live status, while Pi and OMP agents remain working through compaction. Pi becomes idle only after its root session emits a settled event while actually idle, so stale settlement signals do not end active work.
