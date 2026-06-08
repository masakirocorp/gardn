---
status: accepted
---

# Decouple fallback screen detection from terminal viewport state

Hako's fallback screen detector classifies owned text snapshots derived from the terminal's recent bottom-of-buffer rows, not the user's current scrolled viewport. Runtime code is the only layer that reads terminal state: it samples `detection_text()`, maps a platform `ForegroundJob` to an `Agent` for process identification, and then calls `detect_agent(agent, &content)`. `detect_agent` and the per-agent detectors remain pure `&str` pattern classifiers returning `AgentDetection { state, visible_blocker, visible_idle, visible_working }`, which `TerminalState` may later arbitrate with hook authority.

In this ADR, `visible_*` means strong live UI chrome present in the sampled terminal tail, not necessarily visible in the user's currently scrolled viewport.

Observed behavior: fallback detection follows the live terminal tail even when the user scrolls the viewport away from the bottom. Unit tests in `src/pane/terminal.rs` protect that `detection_text()` stays equal to recent bottom text while `visible_text()` follows scrollback, and that bottom detection remains sane across resize. `[INFERENCE]` This also keeps published fallback status stable when headless/client rendering uses client-driven geometry or a viewport different from the live tail.

## Considered options

- Detect from the current visible viewport. Rejected because a user scrolling through history could make Hako publish stale fallback agent state.
- Let per-agent detector rules read or mutate parser, viewport, pane scroll, render, or `ViewState` directly. Rejected because detection would become entangled with rendering, scrolling, and copy-mode behavior.
- Keep per-agent state detection as pure text-pattern classification over a bottom-of-buffer text seam, with terminal sampling, process identification, hook authority, publishing, and stabilization handled outside the pattern matcher. Accepted because it keeps detector rules testable and lets runtime code arbitrate process probes, visible signals, hook authority, and stabilization separately.

## Consequences

New per-agent state detectors should be implemented as pattern matching over `&str` content in `src/detect.rs` and should return `AgentDetection` metadata through `detect_agent`. Process-identification helpers in the detection module may inspect platform-owned `ForegroundJob` data, but per-agent detector rules should not depend on viewport offset, scroll metrics, render state, parser internals, or `ViewState`. Runtime code decides when to sample terminal text, identify foreground processes, synthesize process-exit fallback, stabilize noisy results, merge hook authority, and publish events; the per-agent detector remains a pure classification boundary.

Historical rationale beyond the current source and repository instructions is `[INFERENCE]`: this boundary likely exists to keep agent status reliable across scrolling, resizing, headless rendering, and multiple clients while keeping per-agent heuristics easy to test with text fixtures.
