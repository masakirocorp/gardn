---
status: accepted
---

# Keep AppState rendering read-only

Oh My Herdr separates view computation from drawing. `compute_view*` functions take `&mut AppState` and may reconcile view geometry, scroll bounds, pane sizes, mobile layout, hit areas, and cached `ViewState`; `render*` functions take `&AppState` and must not mutate app, workspace, or layout state.

This is accepted because drawing should be a projection of already computed app state, not another place where app-state transitions happen. The current UI entry points encode that boundary: the application render loop calls `compute_view_with_*(&mut self.state, ...)` before `render_with_runtime_registry(&self.state, ...)`, and headless virtual rendering follows the same sequence. `ViewState` is explicitly the computed geometry consumed by render and mouse handling.

## Considered options

- Let render compute and mutate app geometry while drawing. Rejected because repeated draws, headless renders, and terminal-frame retries could produce hidden app-state transitions.
- Compute all geometry in input handlers. Rejected because geometry depends on the actual frame area, mobile threshold, pane layout, runtime cell size, and current overlays.
- Keep a dedicated computation step before drawing. Accepted because app-state mutations are localized and render remains a predictable read-only projection of `AppState`.

## Consequences

New UI features should put layout reconciliation, scroll clamping, pane resizing, hit-area calculation, and `ViewState` updates in `compute_view*` or code called from that phase. Runtime resizing is a compute-view side effect; headless non-foreground renders use `compute_view_without_resizing_panes` so client-local frame size does not resize shared pane runtimes. Rendering code for panes, sidebars, settings, onboarding, release notes, dialogs, menus, and other overlays should accept immutable app state and avoid app/workspace/layout mutation.

Rendering is not side-effect free in the strict functional sense: it writes to the ratatui frame, may read through `TerminalRuntimeRegistry`, and terminal backend render caches may update behind runtime `&self`. Those runtime reads and cache refreshes must remain idempotent from the user-visible app-state perspective; app/workspace transitions and layout reconciliation stay outside render.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this boundary likely exists to keep terminal rendering repeatable across ratatui frames, headless virtual clients, mouse hit-testing, and mobile/desktop layout changes.
