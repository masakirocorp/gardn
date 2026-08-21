---
packages:
  omh: patch
---

# Improve terminal input and mouse behavior

Oh My Herdr now preserves native terminal key events more accurately, including Kitty keyboard releases, shifted keys, text input, and Windows ConPTY input. Mouse selection, URL clicks, horizontal scrolling, and per-pane right-click routing now follow the pane and host-terminal state consistently.
