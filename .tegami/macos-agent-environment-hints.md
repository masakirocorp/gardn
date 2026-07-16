---
packages:
  omh: patch
---

### Detect wrapped agents

macOS wrappers can now identify a hidden foreground agent with the process-scoped `OMH_AGENT` environment hint, matching Oh My Herdr's existing Linux behavior without accepting upstream-branded variables. Oh My Herdr-managed profiles set the hint automatically from their selected supported agent kind, including when a profile command runs through a wrapper.
