---
status: accepted
---

# Keep rendering pure

Hako separates view computation from drawing. `compute_view*` functions take `&mut AppState` and may reconcile view geometry, scroll bounds, pane sizes, mobile layout, hit areas, and cached `ViewState`; `render*` functions take `&AppState` and draw from the already computed state without mutating it.

This is accepted because drawing should be a projection of current state, not another place where state transitions happen. The current UI entry points encode that boundary: the application render loop calls `compute_view_with_*(&mut self.state, ...)` before `render_with_runtime_registry(&self.state, ...)`, and headless virtual rendering follows the same sequence. `ViewState` is explicitly the computed geometry consumed by render and mouse handling.

## Considered options

- Let render compute and mutate geometry while drawing. Rejected because repeated draws, headless renders, and terminal-frame retries could produce hidden state transitions.
- Compute all geometry in input handlers. Rejected because geometry depends on the actual frame area, mobile threshold, pane layout, runtime cell size, and current overlays.
- Keep a dedicated computation step before drawing. Accepted because mutations are localized and render remains a predictable read-only projection.

## Consequences

New UI features should put layout reconciliation, scroll clamping, pane resizing, hit-area calculation, and `ViewState` updates in `compute_view*` or code called from that phase. Rendering code for panes, sidebars, settings, onboarding, release notes, dialogs, menus, and other overlays should accept immutable app state and draw only. Code that needs runtime data during drawing may read through `TerminalRuntimeRegistry`, but it must not use render as a place to mutate app or workspace state.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this boundary likely exists to keep terminal rendering repeatable across ratatui frames, headless virtual clients, mouse hit-testing, and mobile/desktop layout changes.
