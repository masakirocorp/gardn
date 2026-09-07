---
packages:
  gardn: patch
---

# Fix macOS app update ordering

Use a separate numeric build version for Sparkle so older Gardn.app installs can detect and install newer releases.
