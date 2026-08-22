---
status: accepted
---

# Restore invalid or legacy snapshots into stable workspace shapes

Gardn restore is resilient to old or partially invalid session snapshots when it can recover a stable workspace shape. Pre-tabs workspace snapshots are migrated into a one-tab current snapshot; missing pane cwd paths warn and fall back to `$HOME`, then `/`; and metadata-only workspaces can be restored as empty workspace shells with their saved id when present, saved name, group, and identity cwd.

This favors preserving user workspace identity over rejecting the whole session because a pane path disappeared, a terminal was pruned, or an older snapshot format lacks current fields. The restored result may contain fewer live terminals than the snapshot once described, but it keeps the workspace structure users named and grouped.

This is separate from ADR 0009 and ADR 0036. ADR 0009 records the split between session snapshots, history, and handoff state; ADR 0036 records save-time clearing of factory-default state. This ADR records restore-time recovery semantics for legacy, missing-path, and terminal-less workspace snapshots.

## Current rationale

`[INFERENCE]` Gardn restores stable workspace shells because session files are user-facing continuity, not strict crash dumps. Dropping an entire workspace because one cwd vanished or one legacy layout needs migration would lose names, grouping, and workflow context that are still valid.

## Consequences

Restore changes should prefer targeted recovery over whole-session rejection when workspace identity can still be represented. New snapshot migrations need deterministic fallback rules, and restore-time cwd recovery should keep warning and falling back to safe host paths rather than silently using unrelated current-process or workspace paths.
