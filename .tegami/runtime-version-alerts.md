---
packages:
  gardn: patch
---

# Show runtime version mismatches

Gardn now classifies the running client and server versions in `gardn status`. Compatible thin clients can attach across release-version skew. The terminal title and exit message keep the required restart or update action visible without changing the server-rendered TUI.

The macOS menu panel also keeps a version notice visible for the selected coordinator. It reports stale servers, stale clients, incompatible protocols, and damaged app-to-CLI installations.
