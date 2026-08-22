---
status: accepted
---

# Merge terminal metadata by source and expiry

Gardn treats agent metadata as source-scoped terminal presentation data, not as a global pane label replacement. `pane.report_metadata` validates that a report names a metadata source, sets or clears at least one metadata field, does not set and clear the same field in one report, and optionally scopes metadata to an agent label or an authority source. The app converts valid reports into `AppEvent::HookMetadataReported`, and terminal state applies them with `TerminalState::set_agent_metadata`.

Metadata reports are sequence-gated per metadata source. If a report includes `seq`, Gardn accepts it only when the sequence is greater than the last accepted sequence for that source; reports without `seq` are accepted. Accepted sequence counters are stored in terminal semantic state so handoff restore does not replay older metadata reports after a server replacement.

Within a source, metadata is field-aware. A report can set title, display agent, custom status, and state labels; clear flags remove only the named fields. A clear report can also refresh source guards such as agent label and applies-to-source when those guard values are present; clearing guards to `None` requires replacing the source entry with a no-clear report. If the report sets presentation fields or provides a TTL, Gardn updates that source's reported time and replaces the previous TTL/pending deadline. Visible reports with a TTL are then scheduled for expiry. If a report has no clear flags, it replaces that source's metadata entry with the supplied fields rather than merging with old values.

Effective presentation is merged across valid source entries at read time. Title and display-agent values use the newest reported metadata field for each category. Metadata custom status uses the newest metadata field and, when absent, Gardn may reveal hook-authority custom status unless visible screen signals mask it. State labels are merged per state key by reported time. Metadata is visible only while its optional agent-label guard matches the terminal's effective agent label and its optional applies-to-source guard matches the active hook authority source.

TTL expiry is part of the same contract. `next_agent_metadata_expiry` schedules the nearest visible metadata deadline. When expiry runs, Gardn removes metadata whose deadline has elapsed, recomputes effective state/presentation, emits pane updates when the visible presentation changed, and clears expiry-pending flags for metadata hidden by guards. Captured semantic snapshots persist only valid metadata with remaining TTL; restore rebuilds metadata with fresh reported times and remaining TTL duration, so valid fields and sequence gates survive handoff but original per-field timestamps and tie-break order do not.

This is separate from ADR 0009's session snapshot/history/handoff split and ADR 0016's integration authority decision. ADR 0009 decides where terminal semantic state is persisted for handoff. ADR 0016 separates integration report authority from launch profiles. This ADR records how accepted metadata reports are merged, expired, and surfaced once they reach terminal state.

## Current rationale

`[INFERENCE]` Gardn merges metadata by source because multiple integrations can know different presentation facts about the same terminal. Replacing all presentation state globally on every report would let a narrow update, such as a custom status change, erase title or state-label data from another source.

`[INFERENCE]` Sequence gating protects integrations that deliver metadata asynchronously or retry reports. Without per-source sequence counters, stale metadata could overwrite newer pane presentation after reconnects, hook restarts, or live handoff.

`[INFERENCE]` TTL and guard semantics keep metadata useful without making it permanent. Integration-provided titles, display names, statuses, and labels may describe a transient agent screen or authority source; once the guard stops matching or the TTL expires, Gardn should fall back to the remaining terminal and hook presentation state.

## Consequences

New metadata fields should define whether they merge by source, by field timestamp, by state key, or by replacement. They should also define how clear flags and TTL interact before becoming part of `AgentMetadataReport`.

New integration paths should use `pane.report_metadata` or the same normalized report path instead of mutating pane labels or terminal presentation directly. Reports with ordering requirements should provide per-source sequence numbers.

Handoff and restore code should preserve metadata report sequences and remaining TTL, but should not persist expired metadata, original field timestamp ordering, or metadata as normal session snapshot state outside the terminal semantic handoff path.
