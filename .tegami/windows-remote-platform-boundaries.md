---
packages:
  omh: patch
---

### Restore Windows builds for mixed-host support

Windows builds now keep Unix-only SSH execution workers behind the platform boundary while preserving explicit unsupported results for SSH connection actions.
