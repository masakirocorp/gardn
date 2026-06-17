---
status: accepted
---

# Embed native Git diff sessions

Hako adds native Git diff sessions instead of relying only on an external diff TUI. A native diff session is a normal Hako tab/pane scoped to one workspace repo target. It renders Git's standard changed/staged buckets in one review surface: changed files come from the worktree, staged files come from the index, and explicit compare changes are a separate mode rather than part of the default session.

Native diff is an agent-first review surface, not only a human diff viewer. It supports copying selected file/hunk context and sending selected diff context to an existing or newly launched agent. New-agent delivery must queue the prompt until the launched pane has reported a known agent, so Hako does not paste into a shell before the target CLI is ready. Diff-agent payloads refresh the selected diff before sending/copying so stale cached hunks are not handed to agents.

This extends ADR 0019's project-command scoping and observed repo target model, and depends on ADR 0016's profile catalog when a diff is sent to a new agent. ADR 0019 decides which repository a workspace-scoped Git action targets. ADR 0016 decides how profile launches resolve to actual agent commands. This ADR decides what Hako does after a repo target is chosen.

## Current rationale

The external Hunk integration is useful but runs as a nested TUI inside a PTY. That makes performance, theme integration, input behavior, and future patch operations depend on another app's render loop. Hako already owns workspace grouping, target selection, accents, and terminal layout, so the default Git review flow should eventually be native.

The native model follows established Git GUI semantics instead of inventing a new source picker. A repository review shows changed files and staged files as separate buckets in the same session. Operations are bucket-specific: changed hunks/files can be staged or discarded through worktree restore semantics; staged hunks/files can be unstaged through index restore semantics; compare changes are read-only unless a later ADR records an explicit patch-application workflow.

Native diff sessions default to watched refresh, preserving the behavior users get from the current `hunk diff --watch` launcher. Refresh should be debounced, preserve selection by bucket/path/hunk identity where possible, keep the last successful view if refresh fails, and avoid refreshing through destructive confirmations.

Native diff interaction is mouse-first but keeps compact keyboard accelerators for review flow. Hunk navigation autoscrolls the selected hunk header to the top of the viewport. Vertical and horizontal scrolling are bounded so users cannot pan into empty diff space. Split mode draws a persistent center divider; optional wrapping stays inside each side of the split; overflow markers indicate clipped content only when additional content remains.

Native diff syntax highlighting is source-side analysis, not render-time decoration. Hako loads old and new file contents during diff refresh, applies Tree-sitter for first-class agent workflow languages, falls back to Syntect/plain text for long-tail languages, and stores Hako-owned syntax roles. Rendering maps those roles through the active Hako palette while preserving diff backgrounds, gutters, and line actions. Diff row backgrounds simulate transparency by blending semantic red/green/neutral tints against the pane background; terminals do not provide portable alpha for cell backgrounds. Syntax analysis is budgeted across the whole session so a large diff degrades to readable plain text instead of blocking review.

First-class diff languages are TypeScript/TSX, JavaScript/JSX, Python, Rust, Go, Java, JSON/JSONC, Markdown, YAML, TOML, Shell, HTML, and CSS. MDX currently uses Markdown analysis rather than JSX island analysis because Hako does not vendor a maintained Rust MDX grammar. Removed lines use old-path analysis, added lines use new-path analysis, split mode uses old analysis on the left and new analysis on the right, and unified context prefers the new side when present.

## Considered options

- Keep Hunk as the only diff UI. Rejected because nested TUI performance and interaction semantics are outside Hako's control.
- Replace Hunk immediately with a native viewer. Rejected because Hunk still provides mature external review behavior while the native viewer is new.
- Add a source picker before opening a diff. Rejected because it makes the common path heavier and asks users to choose Git internals before seeing their changes.
- Show changed and staged buckets in one native session. Accepted because it matches common Git GUIs, keeps operations source-aware, and avoids modal chaining.
- Use a JavaScript/Pierre-based diff engine directly. Rejected because it would add a JS runtime boundary to a feature whose main purpose is reducing nested-runtime overhead.
- Parse Git patches into Hako-owned structs rather than rendering library types directly. Accepted because UI, persistence, refresh, selection, and mutation semantics must stay independent from the parsing crate.
- Highlight syntax in the render path. Rejected because frame rendering must stay pure and cheap; syntax work belongs to diff refresh.
- Use raw syntax-library colors. Rejected because native diff rendering must follow Hako themes, group accents, and terminal-background transparency.

- Treat native diff as a human-only viewer. Rejected because Hako is agents-first; the review surface must make it cheap to hand precise file/hunk context to the active workflow agent.
- Send text to a newly launched agent immediately after spawning its pane. Rejected because the prompt can land in the shell or pre-agent startup output. Accepted behavior queues until the pane reports a known agent and only then writes the payload.

## Consequences

A native diff tab owns one repo root and one live diff session state. Fake-monorepo workspaces still use ADR 0019's observed repo picker, then open one diff tab per chosen repo.

Native diff sessions parse Git patches into Hako-owned structs rather than rendering library types directly. This keeps UI, persistence, refresh, selection, mutation semantics, syntax analysis, and agent payload construction independent from parsing/highlighting crates.

Agent payload actions should preserve scope: selected hunk when a hunk is selected, selected file otherwise. Payload generation should re-read current Git state before building text. The agent-smoke matrix in ADR 0049 is the source of truth for which supported CLIs have live native diff-send coverage; generic Rust tests alone are not sufficient proof that every real agent can receive and answer the payload.

Hunk remains available as an external fallback/advanced command while native diff support matures. Once native diff covers the everyday review path, Hako can demote or remove the external default without preserving a compatibility shim for nonexistent external users.
