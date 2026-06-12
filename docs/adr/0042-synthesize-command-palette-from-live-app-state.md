---
status: accepted
---

# Synthesize command palette from live app state

Hako builds command palette entries from the current `AppState` instead of maintaining a static registry. Fixed app actions are combined with active workspace tabs, visible workspaces, workspace groups, active-workspace agent availability, and custom keybind commands, then filtered by the query and sorted by stable section order.

This makes the palette a contextual command surface: switch-to-tab and switch-to-space entries reflect what exists now, indexed shortcut labels follow the same visible ordering as the rest of the app, and the New Agent entry appears only when the active workspace can launch an agent profile. A static registry would be simpler, but it would either expose irrelevant commands or duplicate state-specific discovery logic outside the app state model.

This is separate from ADR 0019's project command discovery. Project commands are one source of launchable command runs; this ADR records the broader command-palette projection that includes navigation, layout, agent, app, and custom commands.

## Current rationale

`[INFERENCE]` Hako synthesizes the palette from live state so keyboard-driven navigation and command discovery stay aligned with the mouse-first UI and group-filtered workspace model. The section order gives the generated entries predictable placement without freezing the palette into a manually maintained list.

## Consequences

New command-palette entries should be derived from the state that makes them valid, not added as always-visible actions when they depend on current tabs, workspaces, groups, agents, or custom commands. Shortcut labels shown in the palette should follow the same indexed ordering users can invoke elsewhere.
