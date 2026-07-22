---
packages:
  omh: minor
  omh-docs: minor
  omh-nix: patch
---

### Remove built-in worktree management

Oh My Herdr no longer creates, opens, lists, or removes Git worktrees. Use Worktrunk, another Git tool, or your coding agent to manage checkouts, then open the resulting directory as an Oh My Herdr workspace.

The worktree CLI commands, socket API methods and events, settings, dialogs, grouping behavior, and documentation have been removed together. Existing workspace and Git status behavior is unchanged.
