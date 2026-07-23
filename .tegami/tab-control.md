---
packages:
  omh: patch
  omh-docs: patch
---

### Keep multi-client terminal layouts stable

Each shared tab now has one explicit interactive controller instead of following whichever client was most recently active. Other clients render the controller-sized terminal canvas in a client-local viewport, can navigate, focus, scroll, search, and copy without resizing the PTY or changing terminal content, and can take control explicitly with `prefix+t` or the persistent desktop/mobile control action. Control is released without auto-promoting a watcher when the controller changes tabs, disconnects, or enters direct terminal attach.
