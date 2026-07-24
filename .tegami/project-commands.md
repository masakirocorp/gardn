---
packages:
  omh: patch
  omh-docs: patch
---

### Configure Git, Diff, and IDE project commands

Settings > Commands now owns three independent project launchers: Git, Diff, and IDE. They default to LazyGit, Hunk watch mode, and Fresh, remain freely editable, and can be hidden individually by clearing a field. Git and Diff require an observed repository; IDE remains available for any workspace and uses its repository or working directory. All three actions are available from the command palette and workspace/new-tab menus, ordered IDE, Git, then Diff after Tab and Agent. Curated launchers preserve host terminal colors when Oh My Herdr's theme is Terminal and use an appropriate adapter for named themes. If LazyGit, Hunk, or Fresh is missing, the managed tab shows install guidance and the tool's GitHub URL.
