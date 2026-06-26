---
status: accepted
---

# Inherit workspace context when creating workspaces and tabs

Hako treats interactive workspace and tab creation as contextual actions, not as context-free shell launches. When creating a workspace from navigate mode, Hako prefers the selected workspace if it is eligible under the current group filter; otherwise it uses the active eligible workspace, and finally the selected eligible workspace. In filtered group mode that usually means the active group; in all-groups mode the selected or active workspace can carry its own group. The new workspace inherits that source workspace's group, or falls back to the active group when no source is available.

Creation cwd follows the same source context before applying the user policy. Hako seeds `follow_cwd` from the source or active workspace's effective default cwd, which prefers the focused pane cwd in the active tab and falls back to that workspace's stored default directory. The default directory is updated before the last tab/pane cwd disappears and is persisted with the workspace. `terminal.new_cwd = "follow"` or an empty value uses that seed, then current process cwd, then `/`; `home` uses `$HOME`, then current cwd, then `/`; `current` uses the current process cwd; any other configured value is treated as a path with `~` and `~/` expansion when `$HOME` is available. New tabs and agent-profile tabs also seed cwd from their workspace before applying that policy.

The workspace settings modal is the explicit user seam for correcting that stored default directory when automatic inheritance is wrong or stale.

Workspace names are collision-checked inside the target group. Hako derives a base label from the initial cwd; if another workspace in the same group already has that custom/stored identity label, Hako assigns a custom name such as `<base> 2`. Workspaces in other groups do not force renaming, because ADR 0018 makes groups the presentation/workflow boundary while workspaces remain the owner of tabs, panes, and runtime identity.

Agent-profile tab creation uses the same cwd inheritance but keeps profile launch context separate. Hako resolves the cwd from the target workspace and `terminal.new_cwd`, starts the profile command through the pane shell context, and records the profile's parsed launch argv/env on the terminal for restore. That preserves ADR 0016's profile launch context without making profiles decide workspace/group placement.

## Current rationale

`[INFERENCE]` Hako inherits creation context because mouse-first workspace management usually means “create next to what I am looking at,” not “create in a global default namespace.” Group-local naming keeps groups usable as separate project/workflow scopes, while cwd inheritance makes new workspaces, tabs, and agent-profile tabs start from the user's current project context unless the config explicitly says otherwise.

## Consequences

New interactive workspace or tab creation surfaces should route through the same source-workspace, group, and cwd policy instead of inventing separate defaults for command palette, sidebar, or profile launches. Lower-level API/CLI creation paths may accept explicit cwd/group-like inputs and should be documented separately if their contract differs.

New naming behavior should preserve group-local collision checks. Global name uniqueness would conflict with workspace groups as independent presentation scopes.
