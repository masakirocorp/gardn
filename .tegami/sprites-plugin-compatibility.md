---
packages:
  gardn: minor
---

### Support the Sprites plugin

Gardn accepts upstream plugin API requirements through 0.9.0. The new `gardn plugin config-dir <plugin_id>` command creates and prints the plugin configuration directory. On macOS and Linux, wrapped agents can use the upstream agent hint when `GARDN_AGENT` is absent. Plugin actions use the selected pane's context. Compatibility keeps Gardn's executable, sockets, configuration, and state separate from the upstream application.

The `gardn plugin disable <plugin_id>` command remains available and persists the disabled state. Plugin manifests and commands reject the reserved IDs `.` and `..` instead of treating shared directories as plugin storage.

### Add native Sprites workspaces

Sprites is now an optional native integration under Settings > Integrations. It is off
by default. Create a remote workspace from the command palette, Space menu, or tab menu.
Sprite Manager keeps resource inventory separate from local panes and exposes explicit
connect, start, resume, transfer, checkpoint, and cleanup actions. Disabling the integration
detaches local terminals without stopping remote sessions. Automation uses durable operation
IDs, bounded waits, creation limits, and scoped destructive approvals.
Workspace imports exclude files reached through directory symlinks or Windows junctions.
Source selection resolves native path aliases before checking the Git worktree boundary.
Pull protects ignored local files from remote overwrites and deletions. Connection admission
cannot race Restore or Destroy, and Create retries keep their original resource identity.
Foreign Connect and Shell open in the current Space. Local pane splits stay unavailable
inside Sprite terminals. Settings preserve printable input and confirmed disable state.
Delayed manager results do not intercept another screen's input.
