---
packages:
  gardn: patch
---

### Make Gardn.app own the macOS CLI

Gardn.app now owns `~/.local/bin/gardn` and updates through Sparkle. Standalone `gardn update` refuses when Gardn.app is installed.
