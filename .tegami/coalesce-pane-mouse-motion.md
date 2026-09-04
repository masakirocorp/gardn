---
packages:
  gardn: patch
---

# Coalesce pane mouse motion

Keep the latest pane mouse-move in each input batch, then write at most one move per 16ms frame. Clicks and wheel still flush the pending move first so pixel-mouse apps such as terminal-browser are not flooded with Kitty frames.
