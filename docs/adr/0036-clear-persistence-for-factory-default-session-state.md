---
status: accepted
---

# Clear persistence for factory-default session state

Gardn treats factory-default app state as the absence of a saved session, not as a session snapshot containing no workspaces. When session save runs with no workspaces, only the unrenamed default group, active group zero, group filtering enabled, and default sidebar state, Gardn clears the session and history files instead of writing an empty `SessionSnapshot`.

This keeps cold start semantics simple: no persisted file means use the built-in default state. Writing an explicit empty snapshot would make "nothing saved" and "saved empty app" distinct states, forcing restore and migration code to preserve an extra state that users cannot meaningfully manage.

This is separate from ADR 0009. ADR 0009 records which data belongs in session snapshots, session history, and handoff snapshots; this ADR records when the normal session snapshot should not exist at all.

## Current rationale

`[INFERENCE]` Gardn clears factory-default state so the persistence layer converges back to the same baseline a fresh install would use. That avoids stale empty files, avoids restoring a user-visible shell of nothing, and lets future default-state changes apply when the user has not created durable workspace state.

## Consequences

Save paths that remove the final workspace or return the UI to factory defaults should be allowed to delete persisted session files. New default-state fields must be included in the default-state predicate when they would otherwise make an untouched app persist as a custom session.
