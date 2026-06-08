# Architecture Decision Records

ADRs record current architectural decisions that future maintainers and agents should preserve unless a later ADR supersedes them. Linear tracks ADR workflow; this directory is the source of truth for ADR content.

- [0001 — Keep live terminal runtimes outside AppState](0001-separate-state-from-runtime.md): Hako stores workspace structure and terminal metadata in state, while live terminal runtimes stay behind `TerminalRuntimeRegistry`.
- [0002 — Keep AppState rendering read-only](0002-keep-rendering-pure.md): `compute_view*` reconciles view/layout state, and `render*` draws from the computed `AppState` without mutating app, workspace, or layout state.
- [0003 — Isolate reusable platform behavior behind platform APIs](0003-isolate-platform-behavior.md): reusable macOS/Linux process and host-integration behavior belongs behind `src/platform/` APIs instead of being duplicated at call sites.
- [0004 — Decouple fallback screen detection from terminal viewport state](0004-decouple-detection-from-viewport.md): fallback agent detection classifies recent bottom-of-buffer text while runtime handles sampling, process identification, stabilization, and hook arbitration.
