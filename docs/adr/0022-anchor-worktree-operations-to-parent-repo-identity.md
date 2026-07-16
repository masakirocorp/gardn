---
status: accepted
---

# Anchor worktree operations to parent repo identity

Oh My Herdr treats Git worktree operations as repository-family actions anchored to a parent source, not as independent actions scoped to whichever checkout path the user happens to target. `git_space_metadata` separates repository identity from checkout identity: `GitSpaceMetadata.key` is derived from the canonical Git common directory, while `checkout_key` is derived from the canonical checkout root. Linked worktrees therefore share a repo key but have distinct checkout keys.

The worktree API keeps that split in `WorktreeSource`. A source has an optional source workspace, a source checkout path, a source repo root, a repo key, and a repo name. `worktree.create` and `worktree.open` reject linked-worktree sources with `linked_worktree_source`; they start from the repo parent workspace or parent source instead. `worktree.list` is more permissive: it accepts linked checkout paths or workspaces, prefers explicit worktree membership when present, otherwise derives the parent source with `parent_checkout_path_for_space`, and reports the resolved source in `WorktreeSourceInfo`.

Oh My Herdr records explicit worktree provenance in `WorktreeSpaceMembership`: repo key, label, repo root, checkout path, and whether the workspace is a linked worktree. This provenance is separate from workspace identity. `open_workspace_idx_for_checkout` uses explicit membership first, then cached Git checkout metadata, then resolved identity cwd so the API can find already-open checkouts without treating raw paths as stable public IDs.

Creating or opening a linked checkout ensures the parent source membership exists. If the parent source workspace is not open, Oh My Herdr can open it without focusing it, mark it as the non-linked source, and then mark the target workspace as linked. This keeps list/open responses tied to the same repo family even when the user's current focus is inside a linked checkout.

This is separate from ADR 0007's agent task-worktree isolation and ADR 0018's workspace/group identity split. ADR 0007 governs how agents should isolate their own repository edits with Worktrunk. This ADR records Oh My Herdr's product/API model for Git worktree features visible to users. ADR 0018 says workspace groups are presentation filters; this ADR says Git worktree families are repository provenance.

## Current rationale

`[INFERENCE]` Oh My Herdr anchors worktree operations to the parent source because `git worktree` operations are repository-family operations: listing, adding, and resolving branches are safest when executed against a stable source checkout or bare repo root rather than from an arbitrary linked checkout that may itself be temporary, dirty, or scheduled for removal.

`[INFERENCE]` Keeping repo identity and checkout identity separate lets Oh My Herdr show linked checkouts as independent workspaces while still answering repo-family questions such as "which worktrees belong to this repository?" and "is this checkout already open?" without conflating workspace IDs, paths, and Git common-dir identity.

## Consequences

New worktree APIs should preserve the parent-source rule for create/open operations. If an API accepts a linked checkout for convenience, it should explicitly resolve it to the parent source and report that source instead of silently using the linked checkout as authority.

Workspace and UI features should use workspace IDs and groups for presentation, and worktree membership for Git worktree provenance. They should not use checkout paths or Git common-dir keys as public workspace identity.

Future worktree behavior should keep linked-worktree removal and dirty-checkout safety separate from source selection. A linked checkout can be a target for open, and an Oh My Herdr-managed linked workspace can be a target for remove, but neither should become the authoritative source for creating or listing the repository's worktree family.
