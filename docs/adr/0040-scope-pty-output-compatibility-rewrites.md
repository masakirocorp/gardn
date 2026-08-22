---
status: accepted
---

# Scope PTY output compatibility rewrites

Gardn normally feeds PTY output into the terminal core unchanged, but it permits narrowly scoped compatibility rewrites when a known foreground program would otherwise damage Gardn-owned presentation state. The current case strips CSI `3 J` / CSI `?3 J` scrollback-clear sequences only when Droid is detected as the foreground job and the pane is on the primary screen.

This is deliberately narrower than a general terminal policy. Alternate-screen programs and non-Droid foreground jobs keep their clear-history sequences, so Gardn preserves normal terminal behavior unless the compatibility case is identified by process evidence and limited to the primary screen where Gardn scrollback would be erased.

This is separate from ADR 0004's fallback screen detection. ADR 0004 records how Gardn reads terminal tail text to infer agent state; this ADR records the rare case where Gardn mutates PTY bytes before normal terminal processing for compatibility. It is also separate from ADR 0014: ADR 0014 records the vendored terminal core boundary, while this ADR records rare Gardn-owned pre-core byte rewrites.

## Current rationale

`[INFERENCE]` Gardn scopes output rewrites tightly because terminal byte streams are an application contract. A compatibility rewrite is acceptable only when it protects Gardn's pane history from a known app behavior and when the guard is specific enough to avoid changing unrelated terminal programs.

## Consequences

New PTY output rewrites need a named compatibility target, source-grounded detection, screen-mode guards where relevant, and tests showing non-target output remains unchanged. General-purpose terminal behavior should stay in the terminal core rather than accumulating ad hoc byte filters.
