# Hako

Hako is a terminal workspace manager for AI coding agents. This glossary names product concepts that should stay consistent across UI, config, snapshots, and documentation.

## Language

**Agent**:
A coding assistant process detected or managed inside a pane, such as Codex, Claude, OpenCode, OMP, or PI. An agent is the running thing, not the saved launch configuration.
_Avoid_: Bot, assistant, profile

**Agent Profile**:
A reusable launch identity for an agent family, with a stable id, display name, agent kind, launch command, parsed argv, and optional environment. Agent profiles are global; groups can favorite profiles but do not redefine them.
_Avoid_: Agent command, launcher, preset

**Agent Kind**:
A Hako-supported agent family that can back an agent profile. Agent kinds come from the same supported targets as agent integrations.
_Avoid_: Agent type, command type

**System Agent Profile**:
A built-in read-only agent profile supplied by Hako for a supported agent family. System agent profiles are virtual defaults and are layered with user agent profiles.
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

**New Agent**:
A launch surface for starting an agent profile as a new tab in a specific workspace or group context. New Agent starts the configured command and environment directly, uses group favorites to organize profiles, and does not edit the profile catalog.
_Avoid_: Run command, start bot

**Integration Authority**:
The permission a socket API integration source has to report agent state, session identity, or presentation metadata for a pane. Authority is about a running pane report, not how an agent was launched.
_Avoid_: Agent profile, launcher permission

**Config File**:
The user-editable TOML file that defines Hako's persistent product settings. It is a public configuration surface, not a dump of runtime state.
_Avoid_: Settings cache, state file

**Live Config Reload**:
The runtime flow that reloads `config.toml` into an already-running app or server and applies valid sections while reporting invalid ones.
_Avoid_: Restart, migration

**Workspace**:
A named working area that groups related tabs, panes, and terminal sessions for one project or task.
_Avoid_: Project, folder, session

**Workspace Group**:
A presentation and workflow grouping for workspaces, with its own name, icon, accent, and agent-profile preferences. A workspace group filters and organizes workspaces; it is not the owner of tabs, panes, or terminal runtimes.
_Avoid_: Workspace parent, project

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
A discovered command associated with a project root and launchable from Hako's command surfaces.
_Avoid_: Task when referring to Hako's managed command catalog

**Command Run**:
A Hako-managed terminal tab created for a project command, tracked so the same command can be focused, stopped, or restarted instead of duplicated.
_Avoid_: Shell command, one-off terminal

**Port Entry**:
An observed host TCP listener shown in Hako's activity surfaces, with active/stale and exposure labels. A port entry is runtime observation, not user-authored config or workspace identity.
_Avoid_: Port config, forwarded port

**Port Owner**:
A best-effort pane attribution for an observed port entry. A port owner is useful for context and focus, but it does not mean Hako owns the socket lifecycle.
_Avoid_: Socket owner, command owner

**Terminal**:
A running shell or agent session that Hako can display, send input to, and track. A terminal may be shown in a pane and may outlive a particular pane placement.
_Avoid_: Pane, viewport

**Terminal Core**:
The embedded terminal-emulation engine Hako uses to turn PTY bytes into terminal state, render data, input modes, and terminal responses.
_Avoid_: Pane, terminal runtime

**Viewport**:
The portion of terminal history currently visible in a pane. Scrolling changes the viewport without changing the terminal tail.
_Avoid_: Screen, buffer

**Terminal Tail**:
The recent bottom portion of a terminal's output. Fallback agent detection reads the terminal tail, not the user's current viewport.
_Avoid_: Viewport, visible text

**Agent State**:
Hako's current understanding of whether an agent is working, blocked, idle, or unknown.
_Avoid_: Status when referring to Linear workflow status

**Fallback Screen Detection**:
Agent-state inference from terminal tail text. Fallback screen detection is separate from explicit agent reports and should not be treated as the only source of agent state.
_Avoid_: Hook, report, source of truth

**Host Platform**:
The operating system environment Hako runs on, such as macOS or Linux. Host platform behavior covers process inspection, clipboard, URL opening, notifications, and host input source integration.
_Avoid_: Runtime, terminal

**Local API**:
The newline-delimited JSON control surface exposed by a running Hako server for status, server control, workspace/pane/agent operations, waits, event subscriptions, integrations, and capability discovery.
_Avoid_: Wire protocol, render stream

**Wire Protocol**:
The binary server/client message contract used by Hako clients to attach to a running Hako server. The wire protocol is separate from the public API socket, though public API status/ping reports its protocol version.
_Avoid_: API, command protocol

**Protocol Version**:
The numeric compatibility marker for the wire protocol. Hako currently treats protocol compatibility as an exact match between client and server protocol values.
_Avoid_: App version, release version

**Product Fork**:
A fork that has its own product identity, release line, docs, website, update channel, and repository policy. Hako is a product fork of upstream Herdr, not a mirror.
_Avoid_: Downstream mirror, rebrand branch

**Upstream Signal**:
An upstream Herdr change treated as candidate evidence for a Hako invariant. Upstream signal must be checked against Hako context before it becomes Hako behavior.
_Avoid_: Upstream authority, automatic merge

**Session Snapshot**:
The durable saved shape of a Hako session: groups, workspaces, tabs, panes, layout, active/selected/sidebar state, pane cwd/label/seen state, launch argv/env, and resumable agent-session refs. It excludes pane scrollback and handoff-only terminal semantics.
_Avoid_: History, handoff snapshot

**Session History**:
Optional saved terminal scrollback used to restore pane contents when a fresh runtime is spawned.
_Avoid_: Session snapshot, terminal semantics

**Live Handoff**:
A server replacement flow that transfers live pane runtimes and session state to a new Hako server so pane processes can survive the server swap.
_Avoid_: Restart, cold restore

**Handoff Snapshot**:
The `SessionSnapshot` produced by `capture_handoff` for live server replacement. Unlike a normal save, it may populate per-pane terminal semantics so the replacement server can preserve live agent presentation.
_Avoid_: Normal save, history

**Terminal Semantics**:
Agent-facing terminal presentation and arbitration state such as detected agent, fallback signals/state, hook authority, agent metadata snapshots, effective state/revision, hook/metadata report sequence counters, and last meaningful activity timestamp.
_Avoid_: Scrollback, runtime, terminal bytes

**Thin Client**:
A Hako client process attached to a running Hako server. A thin client renders server frames and forwards framed input bytes; the server owns normal app input semantics.
_Avoid_: Server, app instance

**Semantic Input**:
Decoded key, mouse, paste, outer-focus, and host terminal color/theme reply events interpreted in the context of Hako's current mode, foreground client, and keybindings.
_Avoid_: Terminal bytes, stdin chunk

**Direct Terminal Attach**:
A thin-client mode that attaches to one terminal runtime and sends raw input bytes directly to it, bypassing normal app semantic input.
_Avoid_: App client, pane attach

**Attach Owner**:
The single direct terminal attach connection currently admitted as writable for a terminal runtime. A later attach request must request takeover to replace the owner.
_Avoid_: Foreground client, observer

**Direct Install**:
A Hako binary installed outside a package manager and owned by Hako's own updater.
_Avoid_: Standalone when ownership matters

**Managed Install**:
A Hako binary installed and owned by a package manager such as Homebrew, mise, or Nix.
_Avoid_: Direct install, self-managed install

**Release Asset**:
A platform-specific Hako binary attached to a GitHub Release with the stable name Hako's updater expects for that host platform.
_Avoid_: Artifact when referring to the user-downloadable update binary