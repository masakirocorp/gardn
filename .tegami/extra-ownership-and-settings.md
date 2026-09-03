---
packages:
  gardn: patch
---

# Fix extra Settings ownership and CLI recovery

The extra keeps one Settings window, only the installed app kills other copies, and a PATH `gardn` binary is no longer treated as app-owned just because Gardn.app exists.
