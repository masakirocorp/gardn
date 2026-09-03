---
packages:
  gardn: patch
---

# Coalesce pane pointer events

Enable host DEC 1016 when the focused pane requests SGR pixels, then convert those reports back to pane coordinates. Keep the latest pane move or drag in each 16ms frame. Accumulate wheel ticks in that same interval and write every tick on flush so trackpad scrolling is not dropped.
