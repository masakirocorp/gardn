---
packages:
  omh: patch
---

# Preserve official release identity in packaged binaries

GitHub release binaries now keep their release channel, tag, and build cohort when Cargo runs through Turborepo. Official installs use the production namespace instead of the development namespace.
