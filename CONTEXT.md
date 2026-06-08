# Hako

Hako is a terminal workspace manager for AI coding agents. This glossary names product concepts that should stay consistent across UI, config, snapshots, and documentation.

## Language

**Agent**:
A coding assistant process detected or managed inside a pane, such as Codex, Claude, OpenCode, OMP, or PI. An agent is the running thing, not the saved launch configuration.
_Avoid_: Bot, assistant, profile

**Agent Profile**:
A reusable launch identity for an agent family, with a stable id, display name, agent kind, launch argv, and optional environment. Agent profiles are global; groups can favorite profiles but do not redefine them.
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

**Workspace**:
A named working area that groups related tabs, panes, and terminal sessions for one project or task.
_Avoid_: Project, folder, session

**Tab**:
A layout surface inside a workspace. A tab contains one or more panes and has one active pane.
_Avoid_: Window, workspace

**Pane**:
A visible slot in a tab that displays and interacts with an attached terminal. A pane is the UI placement of a terminal, not the terminal process itself.
_Avoid_: Terminal, process

**Terminal**:
A running shell or agent session that Hako can display, send input to, and track. A terminal may be shown in a pane and may outlive a particular pane placement.
_Avoid_: Pane, viewport

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
