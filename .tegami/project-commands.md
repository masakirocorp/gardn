---
packages:
  omh: patch
  omh-docs: patch
---

### Configure Git, Diff, and IDE project commands

Settings > Commands now owns three independent repository-scoped launchers: Git, Diff, and IDE. They default to LazyGit, Hunk watch mode, and Fresh, remain freely editable, and can be hidden individually by clearing a field. All three actions are available from the command palette; Diff remains available from the new-tab menu and contextual Git actions. Curated LazyGit uses its native terminal palette, Hunk launches with terminal theme auto-detection, and Fresh uses its built-in terminal theme.
