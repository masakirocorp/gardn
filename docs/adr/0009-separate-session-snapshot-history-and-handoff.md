---
status: accepted
---

# Separate session snapshot, history, and handoff state

Hako separates three persistence concerns: the durable `SessionSnapshot`, optional `SessionHistorySnapshot`, and handoff-only `TerminalSemanticSnapshot` fields. Normal `capture` records durable structure, active/selected/sidebar state, pane cwd/labels/seen flags, launch argv/env, and resumable `agent_session` refs; it leaves per-pane `terminal_semantics` unset. `capture_history` records pane scrollback as a separate history snapshot. `capture_handoff` uses the same `SessionSnapshot` shape but may populate per-pane `terminal_semantics` for terminals that carry semantic state, so a replacement server can continue live agent presentation.

This is accepted because restart persistence and live server handoff have different contracts. A cold restore may spawn fresh shells, resume native agent sessions, or seed initial terminal history from a history snapshot. A handoff restore imports live runtimes and must preserve richer semantic state such as detected agent, fallback state, hook authority, agent metadata snapshots, effective state revision, hook/metadata report sequence counters, and last meaningful activity timestamp.

## Considered options

- Store all terminal history and semantic state in the durable session snapshot. Rejected because normal restart persistence would become coupled to live terminal presentation and hook/fallback arbitration details.
- Store only structural state and discard history/semantics on every restore. Rejected because users expect scrollback continuity where possible and live handoff must preserve running agent presentation across server replacement.
- Keep structural snapshot, history snapshot, and handoff semantics as separate persistence paths. Accepted because it keeps ordinary session files focused on durable layout/launch state while allowing history and live semantics to be captured only where they are needed.

## Consequences

Changes to groups, workspaces, tabs, panes, layout, active/selected/sidebar state, pane cwd/label/seen state, launch argv/env, and resumable agent-session refs belong in the durable `SessionSnapshot` path. Screen history belongs in `SessionHistorySnapshot` and can be absent without invalidating the structural restore. Terminal semantic state belongs in `TerminalSemanticSnapshot` and should be included only when the capture mode is handoff-oriented and a terminal actually carries semantic state.

Restore code must preserve precedence between these paths. Saved `terminal_semantics` are restored when present. If semantics are absent, restore may seed an Idle detected agent from a native agent resume plan; imported handoff runtimes may also seed from a saved agent session. If native agent restore is available, saved pane history is not used as initial terminal history, including duplicate native sessions; otherwise `PaneHistorySnapshot` can seed a freshly spawned runtime. `restore_handoff` passes no history snapshot and is strict about imported runtime consumption because it preserves a live server transition rather than rebuilding a cold session.

Historical rationale beyond the current source is `[INFERENCE]`: this split likely exists because Hako needs durable restart files that remain stable and inspectable, while live handoff needs richer transient agent/runtime presentation to make update or server replacement feel continuous.
