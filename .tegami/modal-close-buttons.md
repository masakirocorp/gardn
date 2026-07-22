---
packages:
  omh: patch
---

### Standardize responsive modal layouts

Modals now share close, footer, action-row, list, and text-field geometry so rendering and mouse hit targets stay aligned across normal and narrow terminals. Long Unicode names truncate by terminal-cell width without hiding right-aligned shortcuts or status metadata, focused inputs keep their cursor end visible, and command palette, agent profile, Git repository, navigator, keybind, and product-announcement surfaces now use the same visual hierarchy.
