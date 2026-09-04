# Gardn

Gardn is a terminal workspace manager for AI coding agents. This glossary names product concepts that should stay consistent across UI, config, snapshots, and documentation.

## Language

**Agent**:
A coding assistant process detected or managed inside a pane, such as Codex, Claude, OpenCode, OMP, or PI. An agent is the running thing, not the saved launch configuration.
_Avoid_: Bot, assistant, profile

**Agent Profile**:
A reusable launch identity for an agent family, with a stable id, display name, agent kind, launch command, parsed argv, and optional environment. Agent profiles are global; groups can favorite profiles but do not redefine them.
_Avoid_: Agent command, launcher, preset

**Agent Kind**:
A Gardn-supported agent family that can back an agent profile. Agent kinds come from the same supported targets as agent integrations.
_Avoid_: Agent type, command type

**System Agent Profile**:
A built-in read-only agent profile supplied by Gardn for a supported agent family. System agent profiles are virtual defaults and are layered with user agent profiles.
_Avoid_: Default command, built-in launcher

**User Agent Profile**:
An editable user-defined agent profile stored in configuration. User agent profiles model personal or organization-specific wrappers such as profile-specific OMP, PI, OpenCode, or Codex commands.
_Avoid_: Custom command, wrapper entry

**Agent Profile Order**:
The single global display order for agent profiles. Groups inherit this order; they do not define independent profile ordering.
_Avoid_: Group order, ranking

**Group Profile Favorite**:
A per-group promotion of a global agent profile. Favorite profiles appear before non-favorites for that group, with both sections sorted by the global agent profile order.
_Avoid_: Visibility, permission, policy, priority

**Group Default Agent Profile**:
The launchable group favorite Gardn starts directly from New Agent for workspaces in that group.
_Avoid_: Hidden default, global default

**New Agent**:
A launch surface for starting an agent profile as a new tab in a specific workspace or group context. New Agent starts the configured command and environment directly, uses group favorites to organize profiles, and does not edit the profile catalog.
_Avoid_: Run command, start bot

**Shared Session State**:
The durable workspace, runtime-adjacent, committed configuration, host-qualified placement, and API-visible state owned by one Gardn coordinator Session Namespace and converged across attached clients.
_Avoid_: Global app view, client screen state, execution-worker state

**Client View State**:
The per-normal-app-client navigation, modal, selection, scroll, and computed geometry state that describes what that client is looking at or editing without changing another client's view.
_Avoid_: Shared app state, runtime state

**Tab Control**:
The interactive ownership slot for one stable tab identity. A tab has at most one normal app-client controller; a free tab may be claimed by the first client or by a client switching to it, while an occupied tab is view-only until explicit takeover. Controller navigation, disconnect, or direct terminal attach releases control, and watchers are never auto-promoted.
_Avoid_: Foreground ownership, terminal attach ownership

**Watcher**:
A normal app client viewing a tab controlled by another client. A watcher keeps navigation, scroll, copy, and search local and sees the controller-sized canonical tab; it must explicitly take control before its resize or interactive input can affect that tab.
_Avoid_: Controller, attach owner

**Client Surface State**:
The per-connection host terminal facts and render transport state, such as terminal size, cell size, negotiated render encoding, render baseline, host graphics cache, staged clipboard files, and writer channels.
_Avoid_: Workspace state, session snapshot

**Command Palette**:
A contextual command surface synthesized from Gardn's current app state, keybindings, and fixed app actions.
_Avoid_: Static command registry, shell palette

**Companion Fork**:
A separately released upstream fork whose exact version Gardn requires for one curated integration. Masakiro owns the fork's product behavior and release assets, while the fork keeps its own source, license, and update cadence outside the Gardn binary.
_Avoid_: Vendored source, bundled dependency, arbitrary PATH tool

**Native Agent Resume**:
The restore-time flow that launches a supported agent back into its own saved conversation session from a trusted session reference and preserved launch context.
_Avoid_: Pane history replay, generic relaunch

**Integration Authority**:
The permission a socket API integration source has to report agent state, session identity, or presentation metadata for a pane. Authority is about a running pane report, not how an agent was launched.
_Avoid_: Agent profile, launcher permission

**Agent Metadata**:
Presentation details reported by an integration source for a running terminal, such as title, display agent, custom status, or state labels. Agent metadata decorates an agent; it is not the agent state itself.
_Avoid_: Pane label, agent state

**Agent State Evidence**:
Process identity, terminal-tail fallback detection, strong visible screen signals, hook state reports, and seen/unseen UI state Gardn uses to decide a terminal's user-facing agent state.
_Avoid_: Agent metadata, fallback detection

**Effective Presentation**:
The current user-facing title, display agent, custom status, and state-label set after Gardn combines valid metadata with terminal and integration state.
_Avoid_: Raw metadata, session state

**Maintenance Guardrail**:
A repo-level automated check that rejects narrow, mechanically detectable maintenance or test-quality regressions.
_Avoid_: Review checklist, full test suite

**Public Web Content**:
Authored marketing pages and user-facing product documentation under `website/**`. It explains released behavior for product users and is distinct from maintainer-facing root `docs/**`.
_Avoid_: Maintainer documentation, generated reference

**Generated Reference**:
Disposable, versioned website build content recreated from authoritative Rust contracts, tagged binaries, or release metadata. It mirrors machine shape but is neither authored guidance nor a source of truth.
_Avoid_: Public prose, compatibility contract

**Config File**:
The user-editable TOML file that defines Gardn's persistent product settings. It is a public configuration surface, not a dump of runtime state.
_Avoid_: Settings cache, state file

**Live Config Reload**:
The runtime flow that reloads `config.toml` into an already-running app or server and applies valid sections while reporting invalid ones.
_Avoid_: Restart, migration

**Settings Row**:
A typed row in Gardn's settings modal that defines how a setting or explanatory element renders, participates in selection, and maps between visual rows and logical options.
_Avoid_: Config entry, raw list item

**Modal Geometry Primitive**:
A shared UI helper that defines modal layout, scrolling, tab visibility, or mouse hit-testing across overlay surfaces.
_Avoid_: One-off modal math, settings row

**Workspace**:
A named working area that groups related tabs, panes, and terminal sessions for one project or task.
_Avoid_: Project, folder, session

**Workspace Default Directory**:
Legacy name for a Workspace's default working directory before host-qualified placement. Prefer Workspace Default Location when an Execution Host is in scope; otherwise this remains the Local path form of that default.
_Avoid_: Global default directory, group cwd

**Creation Context**:
The source Group, Workspace, and Resource Location Gardn uses when creating a new workspace, tab, split, or agent-profile tab. Interactive creation captures it from the invoking client's view; host and path travel together and must not be recombined across hosts.
_Avoid_: Global default, launch profile

**Observed Repo**:
A Git repository root Gardn has learned from a workspace's default directory, from a pane cwd inside that workspace, or from a direct child of such a non-Git cwd. Observed repos are user-created context, not the result of recursive filesystem crawling.
_Avoid_: Discovered child repo, scanned repo

**Observed Repo Status**:
The cached working-tree summary for one observed repo root, refreshed in the background and read by Git action surfaces. Unknown status is not the same as clean status.
_Avoid_: Picker status, live git status


**Workspace Repo Target**:
The Git repository Gardn should use for a workspace-scoped Git action. If exactly one observed repo exists, it is the target; if several observed repos exist, the user must choose; if none exist, the action is unavailable.
_Avoid_: Focused pane repo, random child repo


**Workspace Group**:
A presentation and workflow grouping for workspaces, with its own name, icon, accent, and agent-profile preferences. A workspace group filters and organizes workspaces; it is not the owner of tabs, panes, or terminal runtimes. It may store an optional future-Workspace default Resource Location without owning existing Workspace or Terminal runtimes.
_Avoid_: Workspace parent, project


**Coordinator Host**:
The machine running the Gardn server for a Session Namespace. It owns Shared Session State, routing, SSH Connection Profiles, and system OpenSSH for managed connections.
_Avoid_: Rendering client, execution worker, using `Local` as a host label when the client is remote

**Rendering Client Host**:
The machine running the user's outer terminal or thin client. It owns desktop effects such as clipboard delivery, URL opening, notifications, outer-terminal graphics, and input-source behavior.
_Avoid_: Coordinator Host, Execution Host

**Extra Coordinator**:
A Gardn server the macOS menu extra observes. It is either a local Session Namespace on this Mac or a remote Coordinator Host identified by an OpenSSH target and optional session name. The extra's coordinator list is Rendering Client Host state, not Shared Session State, and it is not an Execution Host.
_Avoid_: SSH Connection Profile, Workspace host, LAN discovery


**Execution Host**:
A coordinator or SSH-reachable operating-system environment where Gardn creates and observes Terminal Runtimes and performs filesystem, process, Git, agent, and port operations.
_Avoid_: Workspace host, SSH profile, Rendering Client Host

**Execution Host Display Name**:
The user-facing name for an Execution Host. The coordinator name defaults to the coordinator machine's hostname and may be configured independently; SSH hosts use their SSH Connection Profile names. It is presentation metadata and never changes Execution Host identity.
_Avoid_: `Local` as a host label, hostname as an Execution Host ID

**SSH Connection Profile**:
Coordinator-owned configuration with a stable profile ID, display name, one OpenSSH target, and optional suggested directory. It contains no credential material and is related to, but not identical with, Execution Host identity.
_Avoid_: Remote session, credential store, raw SSH target alone

**Host Path**:
An opaque path interpreted and validated only by its Execution Host. Remote home, symlinks, existence, permissions, completion, and canonicalization are never resolved against another host's filesystem.
_Avoid_: Local PathBuf, unqualified cwd

**Resource Location**:
The inseparable pair of Execution Host identity and Host Path used for inheritance, persistence, API input, cache identity, errors, and restore.
_Avoid_: Bare path, host-only reference, Workspace host

**Workspace Group Default Location**:
An optional Resource Location that seeds future Workspaces created in a Group. Changing it does not move or rewrite existing Workspaces or Terminals.
_Avoid_: Group runtime owner, Workspace Default Location

**Workspace Default Location**:
The durable per-Workspace Resource Location used when no live focused Terminal provides Creation Context. It is user-selected workspace state, is not rewritten by pane or tab close, and does not change when the Workspace moves between Groups.
_Avoid_: Global default directory, group cwd, last focused path

**Terminal Placement**:
The immutable actual Execution Host and launch Resource Location of a Terminal Runtime. A Pane is only the visual placement of that Terminal; Tabs do not own a host.
_Avoid_: Workspace host, pane host, mutable reassignment


**Public ID**:
A user-facing identifier used by CLI and socket API commands to target workspaces, tabs, panes, groups, and agents without exposing runtime allocation details.
_Avoid_: Raw id, memory id

**Tab**:
A layout surface inside a workspace. A tab contains one or more panes and has one active pane.
_Avoid_: Window, workspace

**Pane**:
A visible slot in a tab that displays and interacts with an attached terminal. A pane is the UI placement of a terminal, not the terminal process itself.
_Avoid_: Terminal, process

**Project Command**:
A discovered command associated with a project root and launchable from Gardn's command surfaces.
_Avoid_: Task when referring to Gardn's managed command catalog

**Command Run**:
A Gardn-managed terminal tab created for a project command, tracked so the same command can be focused, stopped, or restarted instead of duplicated.
_Avoid_: Shell command, one-off terminal

**Port Entry**:
An observed host TCP listener shown in Gardn's activity surfaces, with active/stale and exposure labels. A port entry is runtime observation, not user-authored config or workspace identity.
_Avoid_: Port config, forwarded port

**Port Owner**:
A best-effort pane attribution for an observed port entry. A port owner is useful for context and focus, but it does not mean Gardn owns the socket lifecycle.
_Avoid_: Socket owner, command owner

**Terminal**:
A running shell or agent session that Gardn can display, send input to, and track. A terminal may be shown in a pane and may outlive a particular pane placement.
_Avoid_: Pane, viewport

**Terminal Runtime**:
The live execution-side runtime for a terminal, including PTY/I/O ownership, process lifecycle, detector tasks, and render/update channels on its Execution Host. Terminal runtime is not persisted app state; the coordinator may hold a proxy or replica outside AppState.
_Avoid_: AppState, pane metadata

**Terminal Core**:
The embedded terminal-emulation engine Gardn uses to turn PTY bytes into terminal state, render data, input modes, and terminal responses.
_Avoid_: Pane, terminal runtime

**Inner Terminal Identity**:
The `TERM`/`COLORTERM` contract Gardn advertises to processes running inside a pane.
_Avoid_: Host terminal identity, outer terminal

**Viewport**:
The portion of terminal history currently visible in a pane. Scrolling changes the viewport without changing the terminal tail.
_Avoid_: Screen, buffer

**PTY Output Compatibility Rewrite**:
A narrow mutation of PTY bytes before terminal-core processing for a named compatibility case.
_Avoid_: Terminal emulation, fallback detection

**Host Terminal Theme**:
The foreground/background colors reported by the foreground client's outer terminal and cached by Gardn for pane terminal defaults.
_Avoid_: App theme, child OSC override

**Host Graphics**:
Image placements displayed by the user's host terminal outside normal text cells. Host graphics must stay synchronized with Gardn's current workspace/tab view because repainting text does not necessarily clear terminal-managed image placements.
_Avoid_: Terminal core graphics, image upload

**Terminal Tail**:
The recent bottom portion of a terminal's output. Fallback agent detection reads the terminal tail, not the user's current viewport.
_Avoid_: Viewport, visible text

**Agent State**:
Gardn's current understanding of whether an agent is working, blocked, idle, or unknown.
_Avoid_: Status when referring to Linear workflow status

**State Notification**:
A user-facing alert derived from an agent state change, such as needs-attention or finished. A state notification is about the product event before choosing Gardn, terminal, system, sound, or no delivery.
_Avoid_: Toast when delivery channel is not yet chosen

**Notification Target**:
The workspace and pane a Gardn notification can focus. Notification targets exist for in-app navigation; external terminal or system notifications should not imply they can focus Gardn.
_Avoid_: Deep link, delivery target

**Fallback Screen Detection**:
Agent-state inference from terminal tail text. Fallback screen detection is separate from explicit agent reports and should not be treated as the only source of agent state.
_Avoid_: Hook, report, source of truth

**Host Platform**:
The operating-system family of one concrete host role, such as macOS or Linux on a Coordinator Host, Rendering Client Host, or Execution Host. Host platform behavior covers process inspection, clipboard, URL opening, notifications, and host input-source integration for that role only; it is not a synonym for Local or for every machine in a session.
_Avoid_: Runtime, terminal, Local

**Local API**:
The newline-delimited JSON control surface exposed by a running Gardn server for status, server control, workspace/pane/agent operations, waits, event subscriptions, integrations, and capability discovery.
_Avoid_: Wire protocol, render stream

**Local API Event**:
A recent app event emitted on the Local API stream for automation clients, such as workspace, tab, pane, or agent changes. Local API events are operational signals, not durable audit records.
_Avoid_: Audit log, wire frame

**Wire Protocol**:
The binary server/client message contract used by Gardn clients to attach to a running Gardn server. The wire protocol is separate from the Local API socket, though Local API status/ping reports its protocol version.
_Avoid_: API, command protocol

**Render Stream**:
The wire-protocol flow of visual frame updates from a Gardn server to a thin client. Render streams are per-client and droppable; they are not durable state or Local API events.
_Avoid_: Audit log, control channel

**Render Encoding**:
The negotiated representation used for one thin client's render stream, such as semantic frame data or terminal ANSI bytes.
_Avoid_: Protocol version, client mode

**Protocol Version**:
The numeric compatibility marker for the wire protocol. Gardn currently treats protocol compatibility as an exact match between client and server protocol values.
_Avoid_: App version, release version

**Protocol Payload**:
The length-prefixed body carried by the wire protocol or a related thin-client transport message.
_Avoid_: Render stream, terminal bytes

**Product Fork**:
A fork that has its own product identity, release line, docs, website, update channel, and repository policy. Gardn is a product fork of `ogulcancelik/herdr`, not a mirror.
_Avoid_: Downstream mirror, rebrand branch

**Upstream Signal**:
A change from the upstream repository treated as candidate evidence for a Gardn invariant. Upstream signal must be checked against Gardn context before it becomes Gardn behavior.
_Avoid_: Upstream authority, automatic merge

**Session Namespace**:
A default or named Gardn scope for a running server and persistence context.
_Avoid_: CLI flag, workspace, app instance

**Factory-Default Session State**:
The built-in empty Gardn state: no workspaces, the unrenamed default group selected, and default sidebar/group-filter presentation.
_Avoid_: Empty workspace, reset snapshot

**Session Snapshot**:
The durable saved shape of a Gardn session: groups, workspaces, tabs, panes, layout, active/selected/sidebar state, host-qualified defaults and Terminal Placements, pane label/seen state, launch argv/env, and resumable agent-session refs. It excludes pane scrollback, live PTY handles, SSH processes, credentials, and handoff-only terminal semantics.
_Avoid_: History, handoff snapshot

**Restore Recovery**:
The restore-time policy of preserving stable workspace identity while migrating legacy snapshots or replacing invalid saved paths with safe fallbacks.
_Avoid_: Live handoff, factory-default save clearing

**Session History**:
Optional saved terminal scrollback used to restore pane contents when a fresh runtime is spawned.
_Avoid_: Session snapshot, terminal semantics

**Live Handoff**:
A server replacement flow that transfers live terminal runtimes and session state to a new Gardn server so terminal/session processes can survive the server swap.
_Avoid_: Restart, cold restore

**Handoff Import**:
The replacement-server side of live handoff, started on a private import socket to validate a manifest, receive terminal runtime PTY file descriptors, bind public sockets, and assume ownership after commit.
_Avoid_: Client attach, cold restore

**Handoff Snapshot**:
The `SessionSnapshot` produced by `capture_handoff` for live server replacement. Unlike a normal save, it may populate per-pane terminal semantics so the replacement server can preserve live agent presentation.
_Avoid_: Normal save, history

**Terminal Semantics**:
Agent-facing terminal presentation and arbitration state such as detected agent, fallback signals/state, hook authority, agent metadata snapshots, effective state/revision, hook/metadata report sequence counters, and last meaningful activity timestamp.
_Avoid_: Scrollback, runtime, terminal bytes

**Thin Client**:
A Gardn client process attached to a running Gardn server. A thin client renders server frames and forwards framed input bytes; the server owns normal app input semantics.
_Avoid_: Server, app instance

**Foreground Client**:
The full app thin client whose host surface supplies global outer-terminal focus, host theme, app-facing keybinding, and notification context. Foreground status does not own a tab's PTY size, canonical content, or interactive input; those follow Tab Control.
_Avoid_: Attach owner, tab controller

**Clipboard Image Paste Bridge**:
The explicit paste flow where the pasting thin client reads a local clipboard image and Gardn delivers a temporary file path to the terminal instead of raw image bytes.
_Avoid_: Clipboard sync, image upload

**Semantic Input**:
Decoded key, mouse, paste, outer-focus, and host terminal color/theme reply events interpreted in the context of the client's current mode, Tab Control role, foreground host context, and keybindings. A watcher can handle client-local view input but cannot mutate controlled-tab PTY state until explicit takeover.
_Avoid_: Terminal bytes, stdin chunk

**Terminal Key**:
A decoded keyboard event with key code, modifiers, event kind, and optional shifted character information. A terminal key is the semantic event Gardn handles after parsing input bytes.
_Avoid_: Escape sequence, keybinding

**Direct Terminal Attach**:
A thin-client mode that attaches to one terminal runtime and sends raw input bytes directly to it, bypassing normal app semantic input.
_Avoid_: App client, pane attach

**Attach Owner**:
The single direct terminal attach connection currently admitted as writable for a terminal runtime. A later attach request must request takeover to replace the owner.
_Avoid_: Foreground client, observer

**Direct Install**:
A Gardn binary installed outside a package manager and owned by Gardn's own updater.
_Avoid_: Standalone when ownership matters

**Release Line**:
The GitHub Release track a Direct Install follows. Stable follows the latest non-prerelease. Beta follows `vX.Y.Z-beta.N` prerelease tags. Both are official release builds and share Shared Session State under `~/.config/gardn`. Only one Coordinator Host may run that session at a time.
_Avoid_: Build channel, Session Namespace, preview channel

**Managed Install**:
A Gardn binary installed and owned by a package manager such as Homebrew, mise, or Nix.
_Avoid_: Direct install, self-managed install


**Nix Flake Path**:
The optional Nix-native package, app, check, dev shell, and overlay surface for users who install or develop Gardn through Nix.
_Avoid_: Release channel, direct updater

**Release Asset**:
A platform-specific Gardn binary attached to a GitHub Release with the stable name Gardn's updater expects for that host platform.
_Avoid_: Artifact when referring to the user-downloadable update binary