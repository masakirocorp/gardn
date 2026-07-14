# ENG-88 UI/mobile/config/tab-label/navigator port

Target SHAs: 4ffd99c2, 4421c0fe, 2ff5dd2f, db1ef28d, 14d8e933, b44ca3b3,
32e3d7b7, bc764c83, f54d8e8c, 2a1a8d64, 1e1d0632, 010afe53.

Implemented in worktree:
- Compact auto tab labels + readable active labels via `Workspace::tab_display_name` and display-width helpers.
- Configurable pane gaps / borders / hide-single-tab-row / collapsed-sidebar mode added to `UiConfig`, `AppState`, and config reload.
- Shared `ui/text.rs` display-width utilities with CJK-aware measurement.
- Mobile switcher agents-first ordering and worktree-tree grouping.
- Navigator search commands preserved (copy-mode search UI intentionally left to Eng86CopyMode).
- Plugin-driven tab rename refreshes tab bar via `emit_layout_updated_event` -> plugin context -> tab label.

Compile status: hako crate still has 104 errors from concurrent sibling agent work on
overlay pane ownership, copy-mode search, runtime mutation dispatch, and client input
fields. The UI/mobile/config edits themselves no longer introduce errors; remaining
failures are cross-agent integration blockers.

Ledger updated: upstream-port-map.json entries added with status `in-progress`.
