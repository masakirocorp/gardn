---
packages:
  gardn: patch
---

# Focused pane graphics blit

When the focused tab is a single pane or zoomed, Gardn paints only that pane's Kitty images. A virtual placement that already covers the whole grid is drawn as one image instead of walking every cell. Split tabs keep the existing compositor. Local compositor uploads larger than 8KiB use a Kitty temp file instead of base64 pixel bytes, and that dump is not held inside synchronized output.
