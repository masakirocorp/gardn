---
packages:
  hako: patch
---

### Detect wrapped macOS agents

macOS wrappers can now identify a hidden foreground agent with the process-scoped `HAKO_AGENT` environment hint, matching Hako's existing Linux behavior without accepting upstream-branded variables.
