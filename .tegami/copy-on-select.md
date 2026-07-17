---
packages:
  omh: patch
---

### Add configurable selection copying

Mouse dragging always leaves pane text selected. `[ui].copy_on_select` controls only whether a drag selection is copied automatically on mouse-up; double-click still selects and copies a word.
Selections remain client-local and stay aligned with visible text when a client is scrolled into terminal history.
