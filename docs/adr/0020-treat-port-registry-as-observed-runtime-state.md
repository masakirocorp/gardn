---
status: accepted
---

# Treat port registry as observed runtime state

Oh My Herdr treats ports as host runtime observations attached to pane-owned process trees, not as configured resources, workspace identity, or project-command state. `App::refresh_ports` gathers pane process trees from live terminal runtimes every 2 seconds, asks the platform boundary for active TCP listeners where platform TCP discovery is implemented, and syncs those `PortObservation` values into `AppState::port_registry` only when the listener pid maps back to a known pane process tree.

`PortRegistry` owns the in-memory port model. A `PortEndpoint` is keyed by bind address and TCP port, stores transport, exposure, scheme, optional URL, owners, first/last seen times, and active/stale state. `PortExposure` is derived from the bind address: loopback is `localhost`, unspecified is all interfaces, and other addresses are LAN. Oh My Herdr currently records only TCP listeners and keeps `PortScheme::Unknown`; protocol naming or URL construction is not guessed by the registry.

A `PortOwner` is best-effort attribution, not durable ownership. The only current confidence value is `ProcessTree`: the listener pid belonged to the process tree under a pane runtime's child process. Owners carry workspace id, tab index, pane id, pid, optional command, and confidence so UI can focus a likely pane, but Oh My Herdr does not treat that owner as the source of truth for the listener's lifecycle. Shared listeners can produce multiple owners for the same endpoint, and the sidebar renders one row per owner in the current activity scope.

Missing observations mark existing endpoints `Stale`; stale endpoints are pruned after 5 seconds. This makes the sidebar resilient to transient scan gaps while keeping the model convergent. Unowned listeners are hidden rather than promoted into global app state, and the platform scanner returning no listeners is treated as an empty observation pass, not a persisted port deletion command. On unsupported platform paths, TCP discovery returns no listeners.

This is separate from ADR 0018's workspace/group identity split and ADR 0019's project command lifecycle. A project command may start a dev server, but the command run tracks the launched command by command id and terminal id; the port registry independently observes the resulting host listener and associates it back to pane process trees when possible.

## Current rationale

`[INFERENCE]` Oh My Herdr keeps ports observational because listener state is inherently external to Oh My Herdr: processes can bind, release, fork, crash, or share ports without going through Oh My Herdr's command surfaces. Deriving the panel from host observations avoids stale user-authored port config and avoids making workspace snapshots responsible for runtime resources they cannot recreate.

`[INFERENCE]` Owner attribution is intentionally confidence-tagged because pid ancestry is useful for focus and context but is not a hard product identity. Keeping attribution best-effort lets the UI help users jump to a server pane without pretending Oh My Herdr owns the socket.

## Consequences

New port discovery sources should feed observations through `PortRegistry` and attach confidence to any owner attribution. They should not persist endpoints in config or session snapshots unless a later ADR introduces a separate user-authored port feature.

Port UI should preserve active/stale and exposure labels, and should handle shared-owner endpoints without collapsing them into a single owner. Click-to-focus behavior should stay best-effort: stale or disappeared pane targets may no-op at the final focus step rather than turning ports into durable pane identity.

Features that need command semantics should use project-command/run state, not ports. Features that need workspace identity should use workspace/group ids, not bind addresses or port numbers.
