# Architecture Decision Records

ADRs record current architectural decisions that future maintainers and agents should preserve unless a later ADR supersedes them. Linear tracks ADR workflow; this directory is the source of truth for ADR content.

- [0001 — Keep live terminal runtimes outside AppState](0001-separate-state-from-runtime.md): Hako stores workspace structure and terminal metadata in state, while live terminal runtimes stay behind `TerminalRuntimeRegistry`.
- [0002 — Keep AppState rendering read-only](0002-keep-rendering-pure.md): `compute_view*` reconciles view/layout state, and `render*` draws from the computed `AppState` without mutating app, workspace, or layout state.
- [0003 — Isolate reusable platform behavior behind platform APIs](0003-isolate-platform-behavior.md): reusable macOS/Linux process and host-integration behavior belongs behind `src/platform/` APIs instead of being duplicated at call sites.
- [0004 — Decouple fallback screen detection from terminal viewport state](0004-decouple-detection-from-viewport.md): fallback agent detection classifies recent bottom-of-buffer text while runtime handles sampling, process identification, stabilization, and hook arbitration.
- [0005 — Split app orchestration by responsibility](0005-split-app-orchestration-by-responsibility.md): app behavior is placed across state, actions, input, runtime, and focused helpers instead of accumulating in one god object.
- [0006 — Version the wire protocol by release](0006-version-wire-protocol-by-release.md): server/client compatibility is tracked with an explicit protocol version reviewed against tagged Hako releases.
- [0007 — Isolate multi-agent work in task worktrees](0007-isolate-multi-agent-work-in-task-worktrees.md): Worktrunk-backed task worktrees isolate bigger, risky, parallel, or dirty-shared-checkout work so the shared checkout remains safe for integration.
- [0008 — Treat upstream as signal in the product fork](0008-treat-upstream-as-signal.md): upstream Herdr changes are candidate input that Hako ports by invariant while preserving Hako product identity.
- [0009 — Separate session snapshot, history, and handoff state](0009-separate-session-snapshot-history-and-handoff.md): durable layout/launch state, optional scrollback history, and handoff-only terminal semantics stay on separate persistence paths.
- [0010 — Keep client input byte-framed and server-decoded](0010-keep-client-input-byte-framed.md): thin clients frame input bytes while the server owns normal app input semantics and routing.
- [0011 — Make direct terminal attach exclusive by default](0011-make-direct-terminal-attach-exclusive.md): one direct attach client owns writable access to a terminal unless another client explicitly takes over.
- [0012 — Use GitHub Releases for direct updates](0012-use-github-releases-for-direct-updates.md): direct installs update from fixed GitHub Release binary assets, while package-managed installs defer to their manager.
