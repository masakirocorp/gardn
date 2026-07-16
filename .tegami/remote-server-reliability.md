---
packages:
  omh: patch
---

Improved remote and server reliability: high-latency remote handshakes get a longer connection window, remote helper installs work with non-POSIX login shells, SSH authentication failures include actionable guidance, and `omh server stop` waits for both sockets to become unreachable before returning.
