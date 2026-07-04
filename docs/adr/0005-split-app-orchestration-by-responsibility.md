---
status: accepted
---

# Split app orchestration by responsibility

Hako keeps the `app` module split by responsibility instead of letting `App` or `AppState` become the only place application behavior lives. `AppState` owns the in-memory application/UI model — durable session data plus transient mode, view, selection, notification, command/port, and request state — while avoiding channels and async runtime ownership. `App` owns runtime orchestration resources such as `TerminalRuntimeRegistry`, event/API/input channels, timers and deadlines, render notification, host-terminal side effects, config/session I/O, and external API effects.

`actions.rs` holds mostly reusable `AppState` transitions and queries, with explicit runtime or file-system seams where command execution, selection/copy, git status, or command discovery needs `TerminalRuntimeRegistry` or project files. `input/` translates decoded key, mouse, and paste interactions into state changes or `App` runtime actions. `runtime.rs` drives event-loop and recurring runtime work: internal/API/raw-input draining, resize polling, port scans, command/git/update refreshes, session saves, agent-resume work, timers, and loop deadlines.

This is accepted because Hako has a large TUI surface area: terminal forwarding, pane/window layout, modals, settings, copy mode, mouse interactions, agent state, API requests, session persistence, and runtime polling all need to cooperate without one god object owning every rule.

The current module header documents the intended split: `state.rs` contains `AppState`, `Mode`, and pure data structs; `actions.rs` contains state mutations testable without PTYs/async; and the input module translates key/mouse input into actions. In current source, that header describes the common path and intent, not a guarantee that every `AppState` method is pure.

## Considered options

- Put all application behavior on `App`. Rejected because runtime orchestration, state mutation, and input dispatch would become inseparable and harder to test without PTYs or async I/O.
- Put all application behavior on `AppState`. Rejected because `AppState` would have to own or know too much about runtime resources, channels, timers, and input sources.
- Split by responsibility, while allowing explicit boundary crossings where state transitions need runtime context. Accepted because it keeps the common path boring and testable without hiding the places that genuinely need `TerminalRuntimeRegistry`, project files, request flags, or `App` runtime capabilities.

## Consequences

New app behavior should be placed by responsibility. Application/UI model state, durable session data, transient mode/view data, and simple data helpers belong in `state.rs`. Reusable deterministic state transitions belong in `actions.rs` or focused `AppState` impls; transitions that need runtime state should accept `TerminalRuntimeRegistry` explicitly or set a request flag consumed by `App` rather than storing runtime handles in `AppState`.

Decoded input routing belongs under `input/`; async/runtime polling and recurring side effects belong in `runtime.rs`; terminal creation/session/config/API helpers belong in focused app submodules. Existing request flags such as `request_new_workspace`, `request_new_tab`, `request_reload_config`, `request_open_git_diff_command`, `request_clipboard_write`, `request_command_action`, and `terminal_runtime_shutdowns` are part of this boundary: state/input can request side effects, while `App`/runtime performs them.

This ADR does not require every existing method to be pure just because it is in an `AppState` impl. Existing seams such as state transitions that accept `TerminalRuntimeRegistry` are allowed when runtime context is explicit. The maintenance rule is to avoid adding broad, unrelated responsibilities to `App`, `AppState`, or `app/mod.rs` when a focused module already owns the concept.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this split likely exists to keep the core TUI model understandable and testable as Hako grew from simple pane management into agent detection, API control, settings, sessions, updates, and multi-client runtime behavior.
