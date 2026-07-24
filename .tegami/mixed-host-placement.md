---
packages:
  omh: patch
  omh-docs: patch
---

### Run selected resources on SSH hosts

Oh My Herdr sessions can now place Groups, Workspaces, tabs, panes, and agents on configured SSH execution hosts while local resources remain available in the same session. Connection settings, CLI commands, and the Local API expose explicit host-and-path placement. Host-routed terminals preserve their remote runtime across client reconnects and route Git, worktree, command, process, port, path, and agent operations to the owning host. Plugin pane processes remain Local-only in plugin v1 and fail explicitly when a remote placement is selected.
