---
packages:
  omh: patch
---

# Preserve host ANSI colors in pane applications

Pane applications that query terminal colors now receive the active host ANSI palette instead of libghostty defaults. Application-defined palette colors still take precedence until the application resets them.
