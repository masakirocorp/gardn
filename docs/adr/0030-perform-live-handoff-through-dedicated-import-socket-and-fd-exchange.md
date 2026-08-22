---
status: accepted
---

# Perform live handoff through a dedicated import socket and FD exchange

Gardn performs live handoff through a dedicated Unix import socket, not through the wire-protocol client socket or Local API socket. The old server binds a temporary handoff socket with restricted permissions, spawns a replacement server in `server --handoff-import` mode, and authenticates that replacement with a one-use token before sending the handoff manifest. The manifest carries the handoff protocol version, source build/protocol versions, expected replacement version/protocol constraints, the handoff snapshot, and per-pane runtime handoff state.

The replacement validates the manifest before the old server transfers file descriptors. The old server sends duplicated pane runtime fds over the handoff Unix stream with `SCM_RIGHTS` and waits for a `restored` acknowledgement after the replacement has rebuilt `App` state from the handoff snapshot and imported runtimes. Only then does the old server remove its public API/client sockets and wait for the replacement to bind them and report `ready`.

Commit is explicit. After the replacement reports ready, the old server sends `committed`, drains runtimes for handoff, marks those runtimes as preserved, waits briefly for a best-effort `owned` acknowledgement, and then exits without saving a normal cold session over the imported live runtimes. The replacement waits for `committed`, assumes handoff ownership, unpauses imported readers, schedules a repaint nudge for the first client attach, and continues as the server.

Rollback is explicit before commit. While handoff is in progress, the old server disconnects clients, rejects pending client connections, pauses runtime readers, duplicates up to the handoff fd limit, and can kill the failed import child. If validation, fd transfer, restore, or ready/commit coordination fails before ownership is committed, the old server unpauses readers, removes the temporary handoff socket, restores public sockets when needed, and remains responsible for the live runtimes.

This is separate from ADR 0009's snapshot/history/handoff state split: that ADR records what state is captured for handoff, while this ADR records the transport and ownership protocol used to move live runtimes between server processes. It is also separate from ADR 0015's capability gate: capability discovery decides whether the updater may attempt live handoff; the dedicated import socket and fd exchange define how the attempt is performed once selected.

## Current rationale

`[INFERENCE]` Gardn uses a dedicated import socket and fd exchange because wire-protocol client and Local API sockets are public session surfaces with incompatible message contracts. Live handoff needs a short-lived private channel that can authenticate the replacement process, transfer PTY/runtime file descriptors, coordinate public socket ownership, and keep the old server able to roll back until the replacement proves it has restored and bound the session.

## Consequences

New live handoff behavior should keep the import protocol separate from client attach and Local API traffic. Public sockets should not be removed until the replacement has restored imported runtimes, and imported runtimes should not be abandoned by the old server until commit has succeeded.

New handoff compatibility checks should belong in the manifest validation or the higher-level capability gate. They should not depend on clients being connected or on normal session restore paths.
