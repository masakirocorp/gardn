---
packages:
  hako: patch
---

### Detect wrapped agents

macOS wrappers can now identify a hidden foreground agent with the process-scoped `HAKO_AGENT` environment hint, matching Hako's existing Linux behavior without accepting upstream-branded variables. Hako-managed profiles set the hint automatically from their selected supported agent kind, including when a profile command runs through a wrapper.
