---
packages:
  omh: minor
  omh-docs: minor
---

### Expand plugin automation and pane surfaces

Plugins can declare once-per-server startup commands and open split, tab, zoomed, or client-owned popup panes. Popup focus, input, sizing, and teardown stay isolated to the client that opened them, while ordinary plugin panes keep their attribution as they move through session layouts.

Installed and linked plugins now use one user-level registry shared by named sessions. Installation, uninstallation, linking, and listing continue to work when no server is running, and plugin-provided environment values cannot replace Oh My Herdr's protected runtime context.
