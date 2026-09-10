---
packages:
  gardn: patch
  gardn-docs: patch
---

### Route notifications through one coordinator

Gardn now routes state and explicit notifications through one coordinator. In-app, terminal, system, and sound presenters receive one typed request with stable workspace, tab, and terminal targets. Notification results report whether delivery was queued or suppressed.
