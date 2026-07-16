---
packages:
  omh: patch
---

### Preserve OMP profiles during session restore

Restored OMP sessions now retain their profile wrapper, environment, integrations, and credential context, including recovery from snapshots previously saved with conflicting default-profile launch data.
