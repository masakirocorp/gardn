---
status: accepted
---

# Write persistence through user symlinks

Hako session persistence follows existing symlinks before writing JSON files, including dangling and relative symlinks. The save path manually resolves the link target, creates the target parent directory, writes a temporary file beside the target, and renames that file into place.

This is deliberate because Hako's persisted session files are user-owned configuration-adjacent data. Users may manage them with tools such as stow or dotfile repositories, and replacing the link itself would silently break that topology. `fs::canonicalize` is not enough because a dangling symlink on first save is still a valid user-managed destination.

This is separate from ADR 0017's `config.toml` contract and ADR 0009's session snapshot split. Those ADRs define public config and persisted session content; this ADR records the filesystem write policy for session persistence files.

## Current rationale

`[INFERENCE]` Hako preserves symlinks so persistence remains compatible with user-managed config directories and first-save bootstrap. The cost is a little custom path resolution, but the alternative would surprise users by replacing their symlink with a regular JSON file.

## Consequences

New persistence files that are part of the same user-managed session data should write through symlink targets rather than replacing links. Code should keep dangling-link bootstrap working and should avoid `canonicalize` as the sole resolution strategy for write targets.
