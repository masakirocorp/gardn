---
status: accepted
---

# Separate state from runtime

Hako keeps application and workspace state as pure data, while live runtime resources stay outside that state. `AppState` owns workspace structure, pane attachments, view state, and terminal metadata through `TerminalState`; `TerminalRuntimeRegistry` owns live terminal runtimes keyed by terminal id. `PaneState` remains viewport/attachment state, while `TerminalRuntime` owns PTYs, parser backends, detector tasks, channels, and shutdown behavior.

This is accepted because workspace behavior must be testable without real PTYs or an async runtime. The current code states this boundary directly: `AppState` is "pure data, no channels or async runtime", `AppState::test_new()` creates state with no channels or PTYs, and `TerminalRuntimeRegistry` sits outside `AppState` so the application layer owns PTYs, parser backends, detector tasks, and channels.

## Considered options

- Store live pane runtimes inside `AppState`. Rejected because it couples pure workspace behavior to PTYs, async tasks, parser state, and shutdown order.
- Keep pane-local viewport state, terminal metadata, and live runtime as one pane object. Rejected because Hako needs stable terminal identity and metadata without making every workspace mutation depend on live terminal resources.
- Keep runtime resources outside state, with explicit lookup through terminal ids. Accepted because it preserves a pure state model while still allowing runtime-aware operations to receive `TerminalRuntimeRegistry` at the boundary.

## Consequences

New workspace, pane, and app-state behavior should be implementable and testable through `AppState::test_new()` and `Workspace::test_new()` without spawning PTYs. Code that needs live terminal output, input encoding, scrollback, parser state, process control, or runtime shutdown must cross an explicit runtime boundary by accepting or using `TerminalRuntimeRegistry`. Runtime resources must not be added to production `AppState`, `Workspace`, `Tab`, or `PaneState`; test-only runtime fields are allowed only behind `#[cfg(test)]` seams.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this boundary likely exists to keep Hako's core workspace model deterministic while allowing terminal/runtime behavior to remain asynchronous and platform-sensitive.
