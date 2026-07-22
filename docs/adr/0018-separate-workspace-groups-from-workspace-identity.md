---
status: accepted
---

# Separate workspace groups from workspace identity

Oh My Herdr separates workspace identity from workspace grouping. `Workspace` owns stable workspace identity, default cwd, tabs, panes, cwd and Git metadata, active tab, and workspace-scoped public pane numbering. `Group` owns sidebar/workflow presentation: name, icon, optional accent, favorite agent profiles, and default agent profile. A workspace points to its group with `Workspace.group_id`; groups do not own workspace trees, default directories, or terminal runtimes.

Group filtering is a view/navigation concern over the global workspace list. `AppState` stores `groups`, `active_group`, `group_filter_enabled`, and `workspaces` separately. `visible_workspace_indices` and `workspace_in_active_group` filter workspaces by `group_id` only when group filtering is enabled. `move_workspace_to_group` updates the workspace's `group_id`; `delete_group` refuses to delete the last group, and otherwise removes the group and closes workspaces whose `group_id` matched the deleted group. Showing all groups disables the filter without moving workspaces.

Oh My Herdr exposes public IDs separate from internal raw IDs. Workspaces expose `Workspace.id`; groups expose `Group.id`; tabs use `<workspace-id>:<1-based tab number>`; panes use `<workspace-id>-<workspace-scoped pane number>`. Parsers also accept compatibility forms: workspaces and groups accept `w_`/`g_` prefixed indices and bare numeric indices, tabs accept `t_<workspace-id-or-index>_<1-based tab number>`, and panes accept `p_<raw-pane-id>` or `p_<workspace-id-or-index>_<raw-pane-id>`. `Workspace::renumber_tabs` keeps tab numbers display-order based. Pane public numbers are runtime/session presentation handles: new panes append, pane removal compacts higher numbers, and restore/handoff rebuilds a compact `public_pane_numbers` map from restored layout order instead of persisting those public pane numbers.

Restore and handoff preserve this split. `SessionSnapshot` stores groups separately from workspaces; each workspace snapshot stores its own `group_id`. `groups_from_snapshot` preserves saved group ids, names, accents, and agent-profile preferences, normalizes icons, and backfills missing groups for workspaces that reference unknown group ids. ADR 0009 covers which structural fields belong in durable snapshots versus history/handoff-only state; this ADR records the identity model inside that structural snapshot. During live handoff, `handoff_pane_aliases` maps previous raw pane ids, saved env pane ids, and prior aliases to fresh `PaneId`s where the pane survived recreation, so legacy raw `p_` targets can keep resolving where possible.

Accent scope follows the same identity split. App-wide and cross-scope surfaces use the global accent. Surfaces that mutate, confirm, or launch into one concrete workspace/group use that target's effective group accent for modal chrome, selected rows, and primary controls. Cross-scope lists may keep global chrome while coloring each row's group-specific content with that row's group accent.

## Current rationale

`[INFERENCE]` Groups are not parents because Oh My Herdr needs all-groups navigation, drag reorder, filtering, per-group presentation, and workspace movement without changing workspace identity or terminal ownership. Public IDs are not raw runtime IDs because panes can be recreated during restore or handoff, while CLI/API users need handles that are scoped by workspace and stable enough for interactive use.

## Consequences

New group features should store presentation or workflow preferences on `Group`, not duplicate workspace/tab/pane state. New workspace features should live on `Workspace` or lower-level tab/pane state and reference the group only through `group_id` when they need group presentation.

CLI and API surfaces should prefer public workspace/tab/pane IDs. Raw pane IDs are compatibility inputs and must go through alias resolution. Ordinary restore should preserve saved/env pane-id metadata; live handoff should install `AppState.pane_id_aliases` for compatibility resolution when panes are recreated.
