---
status: accepted
---

# Keep live terminal runtimes outside AppState

Hako keeps durable workspace structure and terminal metadata in `AppState`, while live terminal runtimes stay outside `AppState` in `TerminalRuntimeRegistry`. This is specifically a terminal-runtime boundary: `AppState` owns workspace structure, pane attachments, view state, and terminal metadata through `TerminalState`; `TerminalRuntimeRegistry` owns `TerminalRuntime` handles keyed by terminal id. `TerminalRuntime` currently wraps the legacy `PaneRuntime`, which owns the PTY, parser backend, detector task, I/O channels, and shutdown behavior.

This is accepted because workspace behavior should be testable without spawning real PTYs. `AppState::test_new()` starts empty and spawns no PTYs; `Workspace::test_new()` also spawns no PTYs, though workspace/tab plumbing may still carry lightweight event and render-notification handles. `PaneState` remains pane attachment and per-pane UI state: attached terminal id, env pane alias, seen flag, plus test-only detection fields.

## Considered options

- Store live terminal runtimes inside `AppState`. Rejected because it couples workspace behavior to PTYs, parser state, detector tasks, I/O channels, and shutdown order.
- Keep pane-local UI state, terminal metadata, and live runtime as one pane object. Rejected because Hako needs stable terminal identity and metadata without making every workspace mutation depend on live terminal resources.
- Keep live terminal runtimes outside `AppState`, with explicit lookup through terminal ids. Accepted because it preserves a deterministic workspace model while still allowing runtime-aware operations to receive `TerminalRuntimeRegistry` at the boundary.

## Consequences

New workspace, pane, and app-state behavior should be implementable and testable through `AppState::test_new()` and `Workspace::test_new()` without spawning PTYs. Code that needs live terminal output, input encoding, scrollback, parser state, process control, runtime resizing, or runtime shutdown must cross an explicit runtime boundary by accepting or using `TerminalRuntimeRegistry`. Live terminal runtimes, PTYs, parser backends, detector tasks, and terminal I/O ownership must not be added to production `AppState`, `Workspace`, `Tab`, or `PaneState`; test-only runtime fields are allowed only behind `#[cfg(test)]` seams.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this boundary likely exists to keep Hako's core workspace model deterministic while allowing terminal/runtime behavior to remain asynchronous and platform-sensitive.
