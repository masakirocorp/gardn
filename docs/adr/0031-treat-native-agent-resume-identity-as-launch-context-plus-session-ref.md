---
status: accepted
---

# Plan native agent resume from trusted session refs and launch context

Gardn treats native agent resume as a restore-time launch plan built from a trusted agent session reference plus the pane's recorded launch context. Installed official integrations may report session identity; Gardn validates those refs before storing or restoring them. For Pi and OMP, Gardn prefers an absolute session path when one is reported and falls back to an id; other supported agents restore from validated ids only. Snapshot restore rejects unsupported source/agent pairs and unsupported path refs instead of treating arbitrary labels as resumable sessions.

The resume command is agent-specific, but launch context can refine argv and environment. Gardn builds the base argv from the official source, agent, and session ref, then preserves a valid saved launch command as the command word without making that command part of the dedupe key. A plan declares whether its command is an external executable or a shell-resolved wrapper: executables launch directly with argv and environment, while wrappers run as one-shot login-shell commands rather than input typed into an interactive shell. Planner-generated commands are execution details and never become persisted launch context.

For OMP path refs under the user's home directory, Gardn infers wrapper commands only from `.omp` or safe `.omp-*` profile directories when no launch command was saved. The recognized session path is the profile identity anchor: restore reconciles conflicting `PI_CONFIG_DIR` and `PI_CODING_AGENT_DIR` values to that path before launch and deduplication. A generic saved `omp` command that conflicts with a non-default profile path is treated as planner residue, not user launch context. Reported launch env is allowlisted by source/agent before storage; restore planning carries syntactically valid saved launch env entries into the resume plan.

Deduplication uses the restore plan's session identity plus validated saved environment, not the pane id, agent name, or saved command. The base dedupe key includes source, agent, session-ref kind, and session-ref value; if the resume plan includes validated launch environment entries, they are appended to the dedupe key. During one restore pass, Gardn reserves the dedupe key before spawning so later panes do not start the same native session twice. Duplicate native sessions do not get a persisted agent session marker, and native resume suppresses pane-history replay even for duplicate panes because the native agent session owns conversation history.

Native resume is enabled by default through `[session].resume_agents_on_restore = true`, and disabling it stops Gardn from creating native resume plans while preserving the saved session snapshot data. With native resume disabled, restore can still use the structural pane/session metadata and history path according to ADR 0009; it just does not launch the native resume command for supported agents.

This is separate from ADR 0009's snapshot/history/handoff split: ADR 0009 records which persisted state exists and restore precedence between structure, history, and handoff semantics, while this ADR records how resumable agent-session refs become native launch plans and dedupe identities. It is also separate from ADR 0016's profile catalog and integration authority: profiles record launch command/env context and integrations report trusted session refs, but native resume combines both at restore time instead of making either one the sole identity.

## Current rationale

`[INFERENCE]` Gardn includes validated saved environment in native resume identity because the same agent session ref can be meaningful under different config directories. The trusted OMP path takes precedence over contradictory environment because silently selecting another profile can select the wrong integrations or credential broker. Resuming only by pane id would lose cross-pane duplicate protection; resuming only by agent name would collide unrelated sessions; replaying pane history into native resumes would mix Gardn's presentation history with the agent's own conversation storage.

## Consequences

New native resume integrations should define which official source/agent pairs and session-ref kinds are trusted. Path refs should be accepted only when the integration's native session model requires them.

New profile or environment support that affects native resume should be reflected in the launch context used to build the resume plan. Saved commands may refine argv; saved environment may refine both argv execution and the dedupe key. Plans must launch according to their declared executable-or-wrapper resolution mode, and same-session integration reports must not replace established launch context. Restore code should continue reserving dedupe keys before spawning and rolling back reservations only if the runtime spawn fails before the agent process starts.
