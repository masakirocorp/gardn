---
packages:
  omh: patch
---

### Preserve named sessions during live handoff

Oh My Herdr now keeps explicitly selected named sessions on their own API and client sockets during live handoff, even when inherited environment variables contain stale socket overrides.
