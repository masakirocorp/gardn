---
status: accepted
---

# Layer agent state evidence by source precedence

Gardn derives effective agent state from layered evidence: process/profile identity, terminal-tail fallback detection, non-visible OSC title/progress fallback evidence, strong visible screen signals, hook state reports, and pane seen/unseen state. No source wins unconditionally. Strong visible working for the same agent can override a hook idle/blocked report, hook blocked reports override ordinary fallback state, strong visible blockers for the same agent can override non-blocked hook state, visible idle can stale a working hook only after its grace window, and otherwise hook state wins over fallback state. OSC-only evidence can move fallback state when no stronger source is present, but it cannot set `visible_*` or override hook authority as live screen chrome.

This avoids both failure modes of a single-source model. Pure terminal heuristics miss hook-reported state; pure hook authority can become stale or miss process exits, visible prompts, interrupts, and terminal completion. Process-exit and agent-change observations clear matching non-newer hook authority, and detector process probes continue even while a full-lifecycle hook source owns ordinary state reports. Stale or different-agent visible signals do not override newer hook authority.

This is separate from ADR 0004, ADR 0016, and ADR 0024. ADR 0004 records how fallback detection reads terminal tail text and treats OSC strings as non-visible evidence; ADR 0016 records agent profile and integration authority boundaries; ADR 0024 records metadata merging by source and expiry. Agent metadata decorates effective presentation; hook state reports participate in agent-state arbitration. This ADR records the final state-evidence precedence used to present and notify agent status.

## Current rationale

`[INFERENCE]` Gardn layers state evidence because agent status is both operational and UX-critical: it drives sidebar state, notifications, labels, and automation-facing status. A simpler detector-only or hook-only model would be easier to reason about locally but would make stale hooks or ambiguous terminal text visible to users as wrong status.

Hook integrations can emit many correct intermediate transitions while one user-visible operation is still settling. Gardn therefore treats hook reports as evidence for an internal state model first, then applies presentation hysteresis only at the reporting edge. Working and blocked evidence should surface immediately because they indicate active work or needed user action. Idle evidence is delayed briefly so parent/subagent handoffs and fast tool completion bursts do not flicker the activity sidebar through transient idle states. This delay is intentionally presentation-only: newer working or blocked evidence cancels the pending idle report, while the underlying hook evidence remains exact.

## Consequences

New agent integrations or detectors should identify which evidence strength they provide and how it composes with existing process, screen, OSC, hook, and seen/unseen signals. Changes must preserve terminal completion, process-exit cleanup, same-agent visible UI safeguards, lifecycle-authority process probing, and hook authority ordering while keeping agent metadata as presentation data rather than core state evidence.
