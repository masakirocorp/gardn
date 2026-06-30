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

**Group Default Agent Profile**:
The launchable group favorite Hako starts directly from New Agent for workspaces in that group.
_Avoid_: Hidden default, global default

**New Agent**:
A launch surface for starting an agent profile as a new tab in a specific workspace or group context. New Agent starts the configured command and environment directly, uses group favorites to organize profiles, and does not edit the profile catalog.
_Avoid_: Run command, start bot

**Shared Session State**:
The durable workspace, runtime-adjacent, committed configuration, and API-visible state owned by one Hako server session and converged across attached clients.
_Avoid_: Global app view, client screen state

**Client View State**:
The per-normal-app-client navigation, modal, selection, scroll, and computed geometry state that describes what that client is looking at or editing without changing another client's view.
_Avoid_: Shared app state, runtime state

**Client Surface State**:
The per-connection host terminal facts and render transport state, such as terminal size, cell size, negotiated render encoding, render baseline, host graphics cache, staged clipboard files, and writer channels.
_Avoid_: Workspace state, session snapshot

**Command Palette**:
A contextual command surface synthesized from Hako's current app state, keybindings, and fixed app actions.
_Avoid_: Static command registry, shell palette

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
Process identity, terminal-tail fallback detection, strong visible screen signals, hook state reports, and seen/unseen UI state Hako uses to decide a terminal's user-facing agent state.
_Avoid_: Agent metadata, fallback detection

**Effective Presentation**:
The current user-facing title, display agent, custom status, and state-label set after Hako combines valid metadata with terminal and integration state.
_Avoid_: Raw metadata, session state

**Maintenance Guardrail**:
A repo-level automated check that rejects narrow, mechanically detectable maintenance or test-quality regressions.
_Avoid_: Review checklist, full test suite

**Config File**:
The user-editable TOML file that defines Hako's persistent product settings. It is a public configuration surface, not a dump of runtime state.
_Avoid_: Settings cache, state file

**Live Config Reload**:
The runtime flow that reloads `config.toml` into an already-running app or server and applies valid sections while reporting invalid ones.
_Avoid_: Restart, migration

**Settings Row**:
A typed row in Hako's settings modal that defines how a setting or explanatory element renders, participates in selection, and maps between visual rows and logical options.
_Avoid_: Config entry, raw list item

**Modal Geometry Primitive**:
A shared UI helper that defines modal layout, scrolling, tab visibility, or mouse hit-testing across overlay surfaces.
_Avoid_: One-off modal math, settings row

**Workspace**:
A named working area that groups related tabs, panes, and terminal sessions for one project or task.
_Avoid_: Project, folder, session

**Workspace Default Directory**:
The per-workspace cwd Hako uses when the workspace has no live pane cwd, such as after its last tab is closed. It is editable from the space settings modal and is workspace state, not group presentation and not global config.
_Avoid_: Global default directory, group cwd

**Creation Context**:
The source workspace, group, and cwd information Hako uses when creating a new workspace, tab, or agent-profile tab. Cwd inheritance uses live focused pane cwd when available and the workspace default directory otherwise.
_Avoid_: Global default, launch profile

**Observed Repo**:
A Git repository root Hako has learned from a workspace's default directory, from a pane cwd inside that workspace, or from a direct child of such a non-Git cwd. Observed repos are user-created context, not the result of recursive filesystem crawling.
_Avoid_: Discovered child repo, scanned repo

**Observed Repo Status**:
The cached working-tree summary for one observed repo root, refreshed in the background and read by Git action surfaces. Unknown status is not the same as clean status.
_Avoid_: Picker status, live git status

**Diff Source**:
The Git-native bucket Hako asks Git to render inside a native diff session: changed worktree files, staged index files, or explicit compare changes. The source determines which patch operations are valid.
_Avoid_: Review type, PR mode

**Native Diff Analysis**:
Render-neutral source-side analysis attached to a native diff session during refresh. It records syntax roles for old and new file contents so rendering stays theme-aware and does no syntax parsing on frame draw.
_Avoid_: Render highlight, syntax theme

**Syntax Role**:
A Hako-owned semantic color role such as keyword, string, comment, type, function, property, punctuation, or markup. Syntax engines map source ranges to roles; Hako themes map roles to terminal colors.
_Avoid_: Tree-sitter capture, Syntect scope

**Changed Files**:
Unstaged tracked edits plus untracked files in a repo. These can be viewed, staged, or destructively discarded through Git's worktree restore semantics.
_Avoid_: Dirty changes, unstaged-only changes

**Staged Files**:
Index changes that would be committed. These can be viewed and unstaged through Git's index restore semantics; unstage does not delete the user's edits.
_Avoid_: Cached diff, commit preview when referring to mutable state

**Compare Changes**:
A read-only diff between refs, usually a base branch and `HEAD`. Compare changes can span commits/history and are a separate explicit diff mode, not part of the default changed/staged session.
_Avoid_: PR review, branch mode

**Workspace Repo Target**:
The Git repository Hako should use for a workspace-scoped Git action. If exactly one observed repo exists, it is the target; if several observed repos exist, the user must choose; if none exist, the action is unavailable.
_Avoid_: Focused pane repo, random child repo


**Workspace Group**:
A presentation and workflow grouping for workspaces, with its own name, icon, accent, and agent-profile preferences. A workspace group filters and organizes workspaces; it is not the owner of tabs, panes, or terminal runtimes.
_Avoid_: Workspace parent, project

**Git Worktree**:
A Git checkout that belongs to the same repository family as other checkouts through shared Git metadata. Hako can show Git worktrees as separate workspaces while keeping their repository provenance linked.
_Avoid_: Workspace group, task worktree when referring to Hako's product feature

**Worktree Source**:
The parent source Hako uses as authority for listing, creating, and opening Git worktrees for a repository family. A worktree source is usually a parent checkout and may also be a bare repo root.
_Avoid_: Current checkout, linked checkout source

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

**Terminal Runtime**:
The live server-owned runtime for a terminal, including PTY/I/O ownership, process lifecycle, detector tasks, and render/update channels. Terminal runtime is not persisted app state.
_Avoid_: AppState, pane metadata

**Terminal Core**:
The embedded terminal-emulation engine Hako uses to turn PTY bytes into terminal state, render data, input modes, and terminal responses.
_Avoid_: Pane, terminal runtime

**Inner Terminal Identity**:
The `TERM`/`COLORTERM` contract Hako advertises to processes running inside a pane.
_Avoid_: Host terminal identity, outer terminal

**Viewport**:
The portion of terminal history currently visible in a pane. Scrolling changes the viewport without changing the terminal tail.
_Avoid_: Screen, buffer

**PTY Output Compatibility Rewrite**:
A narrow mutation of PTY bytes before terminal-core processing for a named compatibility case.
_Avoid_: Terminal emulation, fallback detection

**Host Terminal Theme**:
The foreground/background colors reported by the foreground client's outer terminal and cached by Hako for pane terminal defaults.
_Avoid_: App theme, child OSC override

**Host Graphics**:
Image placements displayed by the user's host terminal outside normal text cells. Host graphics must stay synchronized with Hako's current workspace/tab view because repainting text does not necessarily clear terminal-managed image placements.
_Avoid_: Terminal core graphics, image upload

**Terminal Tail**:
The recent bottom portion of a terminal's output. Fallback agent detection reads the terminal tail, not the user's current viewport.
_Avoid_: Viewport, visible text

**Agent State**:
Hako's current understanding of whether an agent is working, blocked, idle, or unknown.
_Avoid_: Status when referring to Linear workflow status

**State Notification**:
A user-facing alert derived from an agent state change, such as needs-attention or finished. A state notification is about the product event before choosing Hako, terminal, system, sound, or no delivery.
_Avoid_: Toast when delivery channel is not yet chosen

**Notification Target**:
The workspace and pane a Hako notification can focus. Notification targets exist for in-app navigation; external terminal or system notifications should not imply they can focus Hako.
_Avoid_: Deep link, delivery target

**Fallback Screen Detection**:
Agent-state inference from terminal tail text. Fallback screen detection is separate from explicit agent reports and should not be treated as the only source of agent state.
_Avoid_: Hook, report, source of truth

**Host Platform**:
The operating system environment Hako runs on, such as macOS or Linux. Host platform behavior covers process inspection, clipboard, URL opening, notifications, and host input source integration.
_Avoid_: Runtime, terminal

**Local API**:
The newline-delimited JSON control surface exposed by a running Hako server for status, server control, workspace/pane/agent operations, waits, event subscriptions, integrations, and capability discovery.
_Avoid_: Wire protocol, render stream

**Local API Event**:
A recent app event emitted on the Local API stream for automation clients, such as workspace, tab, pane, or agent changes. Local API events are operational signals, not durable audit records.
_Avoid_: Audit log, wire frame

**Wire Protocol**:
The binary server/client message contract used by Hako clients to attach to a running Hako server. The wire protocol is separate from the Local API socket, though Local API status/ping reports its protocol version.
_Avoid_: API, command protocol

**Render Stream**:
The wire-protocol flow of visual frame updates from a Hako server to a thin client. Render streams are per-client and droppable; they are not durable state or Local API events.
_Avoid_: Audit log, control channel

**Render Encoding**:
The negotiated representation used for one thin client's render stream, such as semantic frame data or terminal ANSI bytes.
_Avoid_: Protocol version, client mode

**Protocol Version**:
The numeric compatibility marker for the wire protocol. Hako currently treats protocol compatibility as an exact match between client and server protocol values.
_Avoid_: App version, release version

**Protocol Payload**:
The length-prefixed body carried by the wire protocol or a related thin-client transport message.
_Avoid_: Render stream, terminal bytes

**Product Fork**:
A fork that has its own product identity, release line, docs, website, update channel, and repository policy. Hako is a product fork of upstream Herdr, not a mirror.
_Avoid_: Downstream mirror, rebrand branch

**Upstream Signal**:
An upstream Herdr change treated as candidate evidence for a Hako invariant. Upstream signal must be checked against Hako context before it becomes Hako behavior.
_Avoid_: Upstream authority, automatic merge

**Session Namespace**:
A default or named Hako scope for a running server and persistence context.
_Avoid_: CLI flag, workspace, app instance

**Factory-Default Session State**:
The built-in empty Hako state: no workspaces, the unrenamed default group selected, and default sidebar/group-filter presentation.
_Avoid_: Empty workspace, reset snapshot

**Session Snapshot**:
The durable saved shape of a Hako session: groups, workspaces, tabs, panes, layout, active/selected/sidebar state, pane cwd/label/seen state, launch argv/env, and resumable agent-session refs. It excludes pane scrollback and handoff-only terminal semantics.
_Avoid_: History, handoff snapshot

**Restore Recovery**:
The restore-time policy of preserving stable workspace identity while migrating legacy snapshots or replacing invalid saved paths with safe fallbacks.
_Avoid_: Live handoff, factory-default save clearing

**Session History**:
Optional saved terminal scrollback used to restore pane contents when a fresh runtime is spawned.
_Avoid_: Session snapshot, terminal semantics

**Live Handoff**:
A server replacement flow that transfers live terminal runtimes and session state to a new Hako server so terminal/session processes can survive the server swap.
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
A Hako client process attached to a running Hako server. A thin client renders server frames and forwards framed input bytes; the server owns normal app input semantics.
_Avoid_: Server, app instance

**Foreground Client**:
The full app thin client whose host surface currently owns shared runtime size, outer-terminal focus, host theme, and app-facing keybinding context.
_Avoid_: Attach owner, server

**Clipboard Image Paste Bridge**:
The explicit paste flow where the pasting thin client reads a local clipboard image and Hako delivers a temporary file path to the terminal instead of raw image bytes.
_Avoid_: Clipboard sync, image upload

**Semantic Input**:
Decoded key, mouse, paste, outer-focus, and host terminal color/theme reply events interpreted in the context of Hako's current mode, foreground client, and keybindings.
_Avoid_: Terminal bytes, stdin chunk

**Terminal Key**:
A decoded keyboard event with key code, modifiers, event kind, and optional shifted character information. A terminal key is the semantic event Hako handles after parsing input bytes.
_Avoid_: Escape sequence, keybinding

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

**Nix Flake Path**:
The optional Nix-native package, app, check, dev shell, and overlay surface for users who install or develop Hako through Nix.
_Avoid_: Release channel, direct updater

**Release Asset**:
A platform-specific Hako binary attached to a GitHub Release with the stable name Hako's updater expects for that host platform.
_Avoid_: Artifact when referring to the user-downloadable update binary