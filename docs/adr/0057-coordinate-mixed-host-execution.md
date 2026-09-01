---
status: accepted
---

# Coordinate mixed-host execution from one Session Namespace

Gardn places Local and SSH-backed resources in one Session Namespace under one coordinator. The coordinator owns Shared Session State, Session Snapshot, Groups, Workspaces, Tabs, Panes, layouts, Public IDs, SSH Connection Profile catalog, stable Execution Host IDs, Group and Workspace default Resource Locations, durable Terminal Placement metadata, creation policy, routing, Local API, client-view coordination, and user-visible host health. Local and SSH execution adapters own host runtime operations on their Execution Host: live PTYs and child processes, Terminal I/O, resize, signal, and teardown, Terminal Core/parser state for that host, live cwd and process-tree observation, Host Path validation and completion, Git/worktree operations, port and process discovery, host-side agent and command launch, and runtime liveness or adoption proofs.

A Resource Location is the atomic pair `{ execution_host_id, host_path }`. Host and path are never separate optionals for inheritance, persistence, API input, cache identity, errors, or restore. Terminal Placement is the immutable actual Execution Host and launch location of a Terminal Runtime. A mixed Workspace has no single Workspace host: Pane and Terminal actions route through the target Terminal Placement; Workspace filesystem, Git, worktree, and command actions use an explicitly selected host-qualified Observed Repo or, only when the action is default-scoped, the Workspace Default Location.

## Machine roles

Three roles stay distinct:

- **Coordinator Host** — runs the Gardn server for the Session Namespace, owns Shared Session State and routing, and launches system OpenSSH for managed connections.
- **Rendering Client Host** — runs the user's outer terminal and owns desktop effects such as clipboard delivery, URL opening, notifications, outer-terminal graphics, and input-source behavior.
- **Execution Host** — runs a Terminal and performs its filesystem, process, Git, agent, and port operations.

The built-in Local Execution Host is the Coordinator Host for that Session Namespace, not necessarily the Rendering Client Host. Settings → Connections uses the Coordinator Host's OpenSSH binary, `~/.ssh/config`, `ssh-agent`, known-hosts policy, and network reachability. Credentials and host-key policy stay with OpenSSH; Gardn never stores private keys or passphrases.

## Ownership boundaries

Local satisfies the same internal execution-host interface through an in-process adapter around the current terminal-runtime registry. SSH uses a persistent remote execution worker reached through a framed, authenticated Execution Worker Protocol distinct from the Local API and client wire protocol. One logical managed connection per active host binding may use bounded OpenSSH child or control channels; each remote Pane is not an unrelated `ssh -t` process.

Saving an SSH Connection Profile authorizes the coordinator to install and update its checksum-verified, versioned worker automatically when that host is used. A protocol-compatible worker that owns live runtimes remains active until those runtimes end; updates do not terminate active work only to replace the worker.

For SSH, the worker owns the full execution-side Terminal Runtime and continuously drains the PTY even while disconnected. The coordinator registry holds a runtime proxy or replica outside `AppState`. Attach returns ordered deltas after an acknowledged revision or a complete canonical checkpoint; raw PTY bytes are never silently dropped. Input, signal, and resize keep per-runtime ordering. ADR 0056 Tab Control remains the sole normal-client source of canonical size and interactive input; watchers stay view-only until explicit takeover.

Public IDs remain coordinator-owned and unique within one Session Namespace. Worker runtime IDs never become public IDs. Runtime adoption matches the full host-binding, worker-instance, runtime-id, and runtime-incarnation tuple, never PID alone. Create and close are crash-consistent: provisional worker runtimes are committed only after durable coordinator mapping, and offline close records a termination-pending outcome until acknowledgement or an explicit Forget without terminating action.

Interactive Creation Context is captured from the invoking Client View State and carries one Resource Location plus a resolution reason. API calls with explicit targets do not consult client view state. Workspace creation uses explicit location, then the target Group default location, then Local. Tab creation uses explicit location, then the invoking client's focused live Terminal location when that terminal is on the same execution host as the Workspace default, then the Workspace default location. Agent creation is not covered by that same-host tab rule. Split uses explicit location or the source Terminal live location and fails explicitly when that source is unavailable. Workspace Default Location is durable user-selected state and is not rewritten by pane or tab close, cwd changes, restore, or Group moves.

Plugin manifests, installations, and plugin code remain Coordinator Host-owned under ADR 0051. Plugin-requested Terminal or command work uses normal host-routed placement. Agent integrations are host-owned resources: Settings resolves Local or one saved SSH Execution Host explicitly, inspection and mutation run on that host, and mutating worker operations are serialized per worker. Remote agent hooks report through a restricted token-authenticated worker endpoint that exposes only pane-report methods. The coordinator's Local API socket, SSH profile arguments, and coordinator environment never enter remote hook configuration. CLI and Local API integration-management commands remain Coordinator Host-only until their public contracts gain an explicit host selector.

## Coexistence with a standalone remote coordinator

The same physical remote machine may run two independent roles at once:

1. **Execution worker** for another coordinator's mixed Local/SSH Session Namespace.
2. **Standalone coordinator/server** attachable through `gardn --remote TARGET`.

Those roles MUST NOT share state or runtime ownership. Role and namespace selection precedes every path, status, bootstrap, update, handoff, and cleanup lookup. An execution worker uses a worker-only entry point and protocol scoped by coordinator installation, Session Namespace UUID, and host-binding generation. It MUST NOT invoke `remote-client-bridge`, unqualified `status server`, `server stop`, `server live-handoff`, `session list`/`delete`, normal Session Snapshot restore, or paths under the standalone session data tree.

For `gardn --remote TARGET --session S`, every remote status or protocol probe, installation decision, stop, shutdown wait, handoff, and bridge command MUST be qualified by `S`. Omission MUST fail rather than target the default Session Namespace. Existing `gardn --remote` remains whole-server attach to another coordinator; it never inserts a Workspace, Pane, or Terminal into the caller's mixed session. Direct terminal attach remains a separate exclusive action routed through the target Terminal Placement under ADR 0011.

The roles may execute the same checksum-verified, protocol-compatible immutable `gardn` artifact and reuse low-level code, but they run as separate role-explicit process trees with isolated sockets, locks, state roots, runtime namespaces, control leases, persistence, update ownership, and cleanup. Identical public IDs, runtime IDs, human session names, directories, or PIDs across roles do not imply shared identity. Starting, stopping, disconnecting, deleting, handing off, or updating one role does not restart, terminate, migrate, or garbage-collect resources owned by the other. Ordinary project directories may coexist on disk without granting Gardn ownership across roles.

**Use for resources** uses the current Coordinator Host's saved SSH Connection Profile and the worker protocol. **Open standalone Gardn** is client navigation equivalent to a fresh `gardn --remote <raw-openssh-target> --session S` from the Rendering Client Host; it never resolves a profile display name, reuses a worker control channel, or inserts resources into the current Session Namespace. A standalone remote coordinator may itself manage mixed hosts; in that namespace, Local means that remote coordinator machine.

## Persistence and restore

Session Snapshot stores stable Execution Host references, Group and Workspace default Resource Locations, Terminal Placement, launch metadata, and last-known user-visible state. It does not store PTY handles, SSH processes, credentials, or unqualified remote paths. Legacy unqualified paths migrate exactly once to `{ local_execution_host_id, path }`. New remote paths are validated only by their worker and never fall back to coordinator HOME, `/`, current cwd, or a name-matched profile. Missing profiles or unavailable hosts restore layouts and metadata as unresolved or unavailable placeholders; a worker handshake is the only proof a runtime survived.

Coordinator live handoff transfers remote worker leases and runtime proxies. Local PTY file-descriptor exchange under ADR 0030 remains the Local handoff path and does not move remote runtimes. An update either proves the replacement can adopt every active runtime or blocks or degrades explicitly before ownership changes.

## Current rationale

Users need Local and SSH-backed Groups, Workspaces, Tabs, Panes, agents, and commands in one normal interface with Group and Workspace defaults that can each select Local or an SSH host. Whole-server `gardn --remote` cannot present that mixed view. Federating independent complete servers would still need a unified ownership center for Groups, defaults, Public IDs, and routing, while adding conflicting snapshots and APIs. One coordinator with host-qualified placement and Local/SSH execution adapters keeps Shared Session State single-owner, preserves durable-state versus live-runtime separation, and lets remote workers own only host operations. Keeping standalone remote coordinators as a separate role preserves the existing whole-server attach path without collapsing two product modes into one process tree.

## Consequences

New placement, creation, snapshot, API, Git, port, agent, command, plugin, clipboard, graphics, URL, and restore work must carry host-qualified Resource Location or Terminal Placement and fail explicitly rather than falling back across hosts. Local remains the built-in Execution Host on the Coordinator Host. Multi-client rules from ADR 0052 and ADR 0056 continue: Shared Session State and host connection health converge; Client View State, connection drafts, and authentication-challenge ownership stay per client; Tab Control remains explicit.

SSH Connection Profile identity, Execution Host identity, host-binding generation, worker instance, and runtime incarnation stay distinct even when v1 keeps a 1:1 profile-to-host mapping. Profile rename and suggested-directory edits preserve binding; target or user changes that would reinterpret old placements are blocked while referenced or create a new binding generation. Terminal Placement is immutable; defaults may be reassigned, but a live or persisted Terminal is terminated or explicitly forgotten rather than moved to satisfy profile deletion.

Connection retirement is an installation-global, durable operation rather than profile-row deletion. The coordinator first inventories every named session and every owned worker binding, then requires approval of that exact plan. Execution serializes under an installation-global lock, fences new work, drains only Gardn-owned runtimes, rewrites affected durable defaults, removes owned bindings, and deletes the profile last. A journal keeps an accepted plan fenced and resumable after restart. If remote cleanup cannot run, the failure screen offers a separate explicit local removal that makes no claim about remote process or file removal.

Effect ownership is explicit: port keys include Execution Host identity; clipboard-image paste stages on the target Execution Host; URL and hyperlink actions run on the invoking Rendering Client Host; notifications keep coordinator policy and foreground-client delivery. Remote-unsupported bridges surface Unsupported rather than performing the wrong Local action.

## Rejected alternatives

- **One whole remote session per client process.** Rejected because it cannot show Local and SSH-backed resources together or honor independent Group and Workspace location defaults in one Session Namespace.
- **Client-only aggregation of independent complete servers.** Rejected because each server would own conflicting Groups, Workspaces, snapshots, APIs, and identifiers, while unified defaults and routing would still require a coordinator layer.
- **One plain SSH process per Pane.** Rejected because terminal-byte transport alone cannot provide durable runtime identity, reconnect or adoption, authoritative cwd, process inspection, Git/worktrees, ports, or consistent agent integration.
- **Silent remote-to-Local fallback.** Rejected because it reinterprets placement, corrupts cache and restore identity, and hides host failure.
- **Shared state or runtime ownership between an execution worker and a standalone remote coordinator on the same machine.** Rejected because equal names, paths, or PIDs must not imply shared identity, and lifecycle actions on one role must not mutate the other.

## Supersession and amendments

This ADR is accepted and amends the decision narrative of the following ADRs without rewriting those files:

- **ADR 0018** — Groups remain presentation and workflow scope and do not own Tabs, Panes, or Terminal Runtimes, but a Group may store an optional future-Workspace default Resource Location. Workspaces still own durable identity; a mixed Workspace has no single host owner for every Terminal.
- **ADR 0019** — Project-command discovery, managed runs, Observed Repos, and Git action targets become host-qualified. Roots, command ids, and repo caches include Execution Host identity; discovery and execution run on the selected host, not an inferred Workspace host.
- **ADR 0020** — Port endpoints and owners are observations on a specific Execution Host. Keys include Execution Host identity; offline scans mark stale or unknown and never collapse into empty Local observations; opening a remote listener requires an explicit forward or remains disabled.
- **ADR 0030** — Dedicated import-socket and local FD exchange remain the Local live-handoff path. Mixed-host coordinator handoff additionally transfers remote worker control leases and runtime proxies; remote PTYs are not moved by `SCM_RIGHTS`.
- **ADR 0031** — Native agent resume launch context, session refs, validated environment, and dedupe identity become host-qualified. Resume plans execute on the Terminal Placement's Execution Host; host identity participates in dedupe.
- **ADR 0033** — Creation Context carries one atomic Resource Location from the invoking Client View State. Inheritance no longer treats bare cwd as host-agnostic, and Workspace Default Location must not drift from pane or tab close or focused cwd rewrites.
- **ADR 0034** — Terminal runtimes still own process teardown, but ownership sits on the Execution Host adapter or worker that holds the live runtime. Coordinator-side proxies request termination; offline close uses durable termination-pending outcomes rather than assuming local drop semantics.
- **ADR 0047** — Legacy unqualified paths migrate once to Local Resource Locations. Missing or unavailable remote locations restore as unresolved placeholders and must not be validated with coordinator `PathBuf::exists` or rewritten to coordinator HOME or `/`.
- **ADR 0051** — Plugin manifests, installations, and plugin code remain Coordinator Host-owned. Plugin-requested Terminal and command work uses host-routed placement; v1 does not install or execute plugin code on workers.
- **ADR 0052** — Shared Session State expands to host-qualified placement, profile catalog references, host health, and runtime-proxy coordination while Client View State remains per client. Invoking-client focus is the only interactive creation focus; authentication-challenge ownership is client-local.
- **ADR 0056** — Explicit per-tab control continues across Local and SSH Terminals. Transport reconnect does not auto-promote watchers or apply stale watcher sizes; only the current controller supplies canonical input and size after reconnect.
