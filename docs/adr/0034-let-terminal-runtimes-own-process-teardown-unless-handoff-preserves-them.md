---
status: accepted
---

# Let terminal runtimes own process teardown unless handoff preserves them

Oh My Herdr keeps live terminal runtimes outside `AppState`, and those runtimes own process teardown. `PaneRuntime`/`TerminalRuntime` owns the PTY actor, terminal I/O, detector task, child pid tracking, and the shutdown policy for the platform session process set rooted at the child pid. `AppState`, workspaces, tabs, and panes describe layout and metadata; removing their state does not itself decide how OS processes die.

Default runtime shutdown is destructive. Dropping a runtime aborts the detector task, shuts down the PTY actor, and, unless the runtime was explicitly preserved, walks the platform session process list rooted at the child pid. If the session list is empty, Oh My Herdr falls back to the child pid. Oh My Herdr sends Hangup, Terminate, and Kill with short grace waits, checking process liveness and child wait completion between signals before warning that a session is still alive.

Explicit `TerminalRuntime::shutdown` follows the same process policy and then marks the runtime so the subsequent drop does not signal twice. This is why call sites remove a runtime and call `shutdown()` when closing panes, stopping command runs, failed native resume launches, or cleaning up tests: the live runtime is the owner that can close channels, stop background work, and signal the platform session process set.

Live handoff is the deliberate exception. Before commit, the old server may pause runtime readers and still owns rollback. After commit, it drains runtimes for handoff and calls `preserve_for_handoff`, which attempts to release the PTY actor after commit, aborts detection, and sets `preserve_processes_on_drop` so dropping the old runtime does not kill the imported terminal/session processes. If actor release fails, Oh My Herdr logs and drop can still close the actor handle while preserving processes. The replacement server calls `assume_handoff_ownership` on imported runtimes after commit; that clears preservation so the new owner will perform normal teardown later.

This is separate from ADR 0001's state/runtime split: ADR 0001 records why live runtimes stay out of `AppState`, while this ADR records the teardown responsibility attached to those runtimes. It is separate from ADR 0003's platform boundary: platform APIs enumerate sessions, check liveness, and send signals, while runtime teardown decides when to call them. It is also separate from ADR 0009's handoff snapshot state and ADR 0015's live handoff capability gate: those decide what can be handed off and whether handoff may be attempted, while this ADR records process ownership before and after commit.

## Current rationale

`[INFERENCE]` Oh My Herdr places teardown on terminal runtimes because only the runtime has the live PTY, child pid, detection task, and I/O ownership needed to close a terminal safely. Letting pure workspace state or render code signal processes would break the state/runtime boundary and make live handoff unsafe, because the old server must sometimes drop runtime handles without killing the processes it just transferred.

## Consequences

New code that disposes of a live terminal should remove the runtime and call its shutdown path, not rely on deleting pane/workspace state to clean up processes. AppState/workspace code may enqueue terminal ids for cleanup, but the app/runtime layer must still remove `TerminalRuntime` values and call `shutdown()`.

New live handoff behavior must preserve old runtimes only after commit and must ensure the replacement assumes ownership before it becomes responsible for later teardown. Preservation should remain an explicit handoff state, not a general “detach without killing” mode.
