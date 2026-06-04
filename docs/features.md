# Hako features

This is the product feature reference for Hako.

## Workspace model

### Sessions

A session is a persistent Hako runtime with its own sockets, panes, tabs, workspaces, and saved state.

- **Default session** — `hako` launches or attaches to the default background session.
- **Named sessions** — `hako --session <name>` and `hako session attach <name>` select separate runtime namespaces.
- **Detach / reattach** — clients can detach while panes and agents continue running in the server.
- **Remote attach** — `hako --remote <target>` attaches to a Hako server over SSH.
- **Remote bootstrap** — remote attach can detect the remote platform, reuse an existing compatible binary, or install a matching Hako binary before connecting.
- **Remote server restart flow** — remote attach checks protocol/version compatibility and can prompt to stop or restart an incompatible remote server.
- **SSH keepalive fallback** — remote attach can add private generated SSH keepalive defaults without overriding your own SSH config.
- **Direct terminal attach** — `hako terminal attach <terminal-id>` and `hako agent attach <target>` attach directly to a single server-owned terminal.
- **Attach takeover** — direct attach is exclusive by default; `--takeover` can claim a terminal attachment from another client.
- **Multiple clients** — more than one client can connect to a server; the foreground interactive client drives shared runtime size, theme, and keybindings.
- **Clipboard bridging** — thin clients forward OSC 52 clipboard writes locally and can bridge local clipboard-image paste into server panes.
- **Live server handoff** — supported updates can move live pane PTYs and session state into a replacement server so running pane processes survive a server swap.

### Workspaces

A workspace contains tabs, panes, cwd metadata, and agent state rollups.

- **Workspace creation and focus** — create, focus, rename, close, list, and inspect workspaces from the TUI, CLI, or socket API.
- **Workspace sidebar** — expanded workspace rows show the workspace name, activity state, and git/cwd summary.
- **Workspace navigator** — search and filter workspaces, tabs, and panes by text or state.
- **Workspace groups** — group workspaces, filter the sidebar by group, collapse groups, and assign group theme overrides for mode plus light/dark palettes.
- **Group lifecycle** — create, rename, delete, focus, and switch groups from the TUI, CLI, or socket API.
- **Group icons** — group creation and rename flows can assign group icons.
- **Move between groups** — move workspaces between groups with `hako workspace move-to-group` or the socket API.
- **Live cwd labels** — workspace labels can follow active pane cwd unless manually renamed.
- **Git worktrees** — list, create, open, and remove Git worktrees from the CLI and socket API.
- **Worktree safety** — generated worktree names are slugged into safe checkout paths, removal leaves branches intact, and dirty worktrees require forced removal.
- **Git summaries** — workspace summaries roll up added, modified, deleted, and conflicted files across detected repository roots.

### Tabs

A tab belongs to one workspace and contains one or more panes.

- **Tab lifecycle** — create, focus, rename, close, list, and inspect tabs.
- **Tab bar** — click tabs, use overflow scrolling, and switch with keybindings.
- **Tab drag reorder** — reorder tabs in the tab bar by dragging, with a drop indicator.
- **Tab-aware state** — workspace and agent UI can include tab context for agents and notifications.

### Panes

A pane is a terminal runtime inside a tab layout.

- **Pane splitting** — split panes vertically or horizontally.
- **Pane focus and zoom** — focus by direction, cycle panes, and zoom the focused pane.
- **Pane resize** — resize interactively from resize mode or by dragging borders.
- **Pane labels** — set manual pane labels; optionally show detected agent labels on pane borders.
- **Pane close** — close panes with confirmation where configured.
- **Scrollback** — scroll panes, edit scrollback in `$EDITOR`, and read visible/recent output through the API.
- **Optional pane history** — persist screen history to `session-history.json` when `[experimental].pane_history` is enabled.
- **Terminal identity** — panes advertise Hako's terminal layer instead of leaking the outer terminal identity.
- **Snapshot restore** — saved sessions restore groups, active selections, sidebar layout, tabs, pane layouts, focus, zoom, cwd, labels, and agent session references.
- **Selection copy** — drag-selected pane text copies on mouse-up and keeps the highlight until the next click or keypress.
- **Keyboard protocol encoding** — pane input honors negotiated terminal keyboard protocols, including Kitty CSI u and legacy modified-key sequences.

## Agent awareness

Hako detects and tracks coding agents running inside panes.

### Agent states

- **Blocked** — agent needs user input, approval, or intervention.
- **Working** — agent is actively running.
- **Done** — agent finished work and has not been seen yet.
- **Idle** — agent is done and seen, or otherwise waiting without attention.
- **Unknown** — no supported agent state is currently detectable.

### Detection

Hako combines foreground-process detection, terminal-screen heuristics, and optional integration reports.

Supported built-in detection includes:

- pi
- Oh My Pi / OMP
- Claude Code
- Codex
- Gemini CLI
- Cursor agent
- Antigravity
- Cline
- OpenCode
- GitHub Copilot CLI
- Kimi
- Kiro
- Droid
- Amp
- Grok CLI
- Hermes agent

### Agent UI

- **Activity sidebar** — shows agents grouped by state across the current workspace, current group, or all workspaces.
- **Agent focus** — focus agents from the activity panel, command surfaces, CLI, or socket API.
- **Agent labels** — manual, detected, and integration-reported labels are surfaced in lists and pane borders.
- **State notifications** — background state changes can trigger Hako toasts, terminal toasts, system toasts, and sounds.
- **Integration authority** — Claude Code, Codex, and OpenCode integrations report session identity while state comes from screen detection; Pi, OMP, Copilot, Hermes, and Qoder-style integrations can still report state directly.

### Agent session restore

Hako resumes supported agents into native agent sessions during session restore by default. Set `[session].resume_agents_on_restore = false` to disable it.

- Supported restore sources come from installed integrations that report session references.
- Duplicate session references are deduplicated during a restore pass.
- Native agent restore suppresses pane-history replay so the resumed agent owns its conversation history.
- Restored agents launch through the restored pane shell, preserving pane environment setup before the native resume command runs.

## Navigation and interaction

### Prefix mode

Hako uses a prefix key before most built-in shortcuts. The default prefix is `ctrl+b`.

Default prefix actions include:

- workspace navigator
- command palette
- settings
- keybinding help
- new / rename / close workspace
- new / rename / close tab
- tab switching
- pane focus, split, resize, zoom, and close
- sidebar toggle
- detach
- reload config
- open notification target

### Mouse support

Mouse capture is enabled by default.

- Click workspaces, groups, tabs, panes, agents, commands, ports, and modal controls.
- Drag pane borders to resize.
- Drag workspace rows to reorder.
- Scroll lists, panes, modals, and scrollbars.
- Right-click where context menus are available.
- Configure `ui.right_click_passthrough_modifier` to send modified right-click hold/drag gestures to mouse-reporting pane apps while normal right-click keeps Hako menus.
- Select pane text for copy workflows.
- **Mobile layout** — narrow terminals use a compact header and scrollable switcher for spaces, tabs, agents, and global menu actions.

### Navigator

The navigator is a workspace/tab/pane chooser.

- Search text matches whitespace-separated terms.
- Filter chips select blocked, working, idle, or done targets.
- Workspace rows can expand and collapse.
- Selection accepts a workspace, tab, or pane target.
- Mouse hover moves selection; row clicks accept targets.

### Command palette and command panel

Hako can discover and run project commands. The command palette is also a general action surface for app, workspace, group, tab, pane, layout, agent-scope, settings, reload, notification, and detach/quit actions.

- Commands are scoped from the active workspace or selected workspace while navigating.
- Command rows are grouped by repo and branch context.
- Command status sections include running, failed, unknown, and stopped commands.
- Custom keybindings can launch shell helpers or pane commands.
- **Panel actions** — command rows can run, focus, expand, or stop commands from the right sidebar.
- **Git diff command** — the command palette can open a Git diff panel when the current context is inside a Git repository. When Hunk 0.14 or newer is installed, Hako launches it with a generated custom theme based on the target workspace/group theme.
- **Command discovery** — Hako discovers VS Code tasks, package scripts, just recipes, Make targets, and defaults for common Cargo, Go, Java, Python, .NET, PHP, and Ruby projects.
- **Managed reruns** — rerunning a managed command focuses an existing run or restarts a stopped/failed run in the same pane instead of spawning duplicates.

### Activity panels

The right sidebar can show agents, commands, and ports. Port entries include active/stale state, exposure labels, owner context, and click-to-focus behavior when an owner pane is known.
Shared ports can list multiple owner panes when more than one command owns the same listener.

### Settings modal

Settings are edited in an in-app modal.

Tabs include:

- Theme
- Sound
- Toast
- Pane Labels
- Integrations
- Experiments

The modal supports keyboard navigation, mouse navigation, scrollbars, save/cancel semantics, and install/update/uninstall actions in the integrations tab.

### Help and confirmations

Hako includes a scrollable keybinding help modal generated from current bindings, including custom command bindings. Destructive actions such as workspace close and group delete use confirmation dialogs that show the affected target.

### Global menu

The global menu exposes settings, keybinding help, config reload, update/release-note actions, and detach from sidebar and mobile menu surfaces.

## Integrations

Hako ships installable integrations for agents that report semantic state, native session identity, or both over the socket API.

Built-in installable integrations:

- pi
- OMP
- Claude Code
- Codex
- OpenCode
- Hermes

Integration management supports:

- install
- uninstall
- status checks
- outdated-version detection
- in-app integration management

Integration install side effects are agent-specific: pi and OMP install extensions, Claude and Codex install/update hooks or settings, OpenCode installs a plugin, and Hermes installs/enables a plugin.

Claude Code, Codex, and OpenCode integrations report native session identity for restore; Hako reads their visible terminal UI for state. Pi, OMP, Hermes, and Qoder-style integrations can report state directly.

Integration path overrides include `PI_CODING_AGENT_DIR`, `PI_CONFIG_DIR`, `CLAUDE_CONFIG_DIR`, and `CODEX_HOME`. OMP install/status checks scan `.omp` and `.omp-*` extension directories.

## CLI and socket API

Hako exposes the same runtime model through the CLI and local Unix socket API.

### CLI areas

- **`hako status`** — show client/server status and protocol compatibility.
- **`hako session`** — list, attach, stop, and delete named sessions.
- **`hako workspace`** — manage workspaces.
- **`hako worktree`** — manage Git worktree checkouts.
- **`hako tab`** — manage tabs.
- **`hako pane`** — manage panes, read output, send input, report agent state, and run commands.
- **`hako agent`** — list, inspect, focus, read, send to, attach to, rename, and start agents.
- **`hako wait`** — wait for output matches or agent status changes.
- **`hako integration`** — install, uninstall, and inspect agent integrations.
- **`hako group`** — list, create, focus/switch, rename, and delete workspace groups.
- **`hako config reset-keys`** — remove custom keybindings while preserving the rest of the config.
- **`hako update`** — self-update supported binary installs; `--handoff` can preserve live panes while moving running sessions to the updated server.
- **`hako server`** — run the headless server, stop it, reload config, or trigger a live handoff.
- **Launch flags** — `--no-session`, `--default-config`, and `--remote-keybindings <local|server>` control startup and remote behavior.
- **JSON output** — status, session, and worktree commands expose machine-readable output where supported.
- **Read modes** — pane and agent reads support visible, recent, recent-unwrapped, ANSI, raw, and bounded line output.
- **Wait matching** — output waits support substring or regex matching, raw matching, timeouts, and agent-status waits.
- **Automation reads** — pane and agent output can be consumed as rendered visible text, recent scrollback, ANSI, or raw output for agent feedback loops.

### Socket API

The socket API supports typed request/response calls and event subscriptions.

API-visible domains include:

- server control
- workspaces
- worktrees
- tabs
- panes
- agents
- integrations
- output reads
- output waits
- event subscriptions
- workspace groups
- terminal attach
- integration authority reports
- render streaming
- protocol compatibility checks

## Appearance and notifications

### Themes

Hako supports terminal-derived colors and built-in palettes.

- **Theme source** — terminal colors or theme palettes.
- **Appearance mode** — system, light, or dark.
- **Light and dark palette selection** — choose separate palettes when system mode is enabled.
- **Custom token overrides** — override individual theme colors.
- **Group theme overrides** — assign per-group appearance mode and light/dark palette overrides; values matching global settings inherit future global changes.
- **Accent color** — configure highlight, border, and navigation accent color; when following terminal colors, choose separate terminal ANSI accents for light and dark appearances.

### Sound and toasts

- **Toast delivery** — off, Hako, terminal, or system.
- **Sound notifications** — request and done sounds for background agent activity.
- **Per-agent sounds** — agent-specific sound overrides.
- **Validation** — invalid or missing sound files fall back to defaults and emit diagnostics.
- **Terminal toast backends** — terminal toasts use supported terminal notification protocols, including tmux passthrough where available.
- **Custom sound files** — request/done sounds can use MP3 files resolved relative to the config file.
- **Sound disable switch** — `HAKO_DISABLE_SOUND` disables playback.

## Configuration

Configuration file: `~/.config/hako/config.toml`.

Configurable areas include:

- onboarding
- theme
- terminal shell and new-terminal cwd policy
- session restore
- keybindings
- indexed shortcuts
- custom command keybindings
- multiple bindings per action
- prefix-mode and direct key chords
- sidebar size and mouse behavior
- close and naming prompts
- agent panel scope
- pane border labels
- toast and sound settings
- worktree directory
- scrollback limit
- experimental features

## Updates and release notes

Hako uses GitHub Releases for update checks, release metadata, and binary downloads.

- The app can notify when a new release is available.
- `hako update` downloads and swaps supported binary installs.
- Homebrew and Nix-managed installs are blocked from self-update and should use their package manager.
- Live handoff can preserve running pane processes during updates when both the old and new server support the handoff protocol.
- In-app release notes can be shown after an update.
- Post-update checks can report outdated integrations.
- Product announcements can be shown separately from release notes and tracked as seen per version.
- First-run onboarding introduces the core workflow in-app.
- Update-ready previews can show release notes and the install command before the update is applied.

## Fork maintenance

Hako tracks upstream Herdr commits with an explicit port ledger.

- **Upstream port ledger** — `upstream-port-map.json` records each upstream commit as ported, superseded, skipped, or pending.
- **Ledger check** — `just upstream-status` reports upstream status and fails when commits are unclassified or still pending.
- **Sync guard integration** — upstream-sync reports include the ledger status so product-specific skips and superseded changes stay visible.
- **Hako-owned surfaces** — docs, website, release, and repository-process commits can be skipped with explicit reasons instead of silently reintroducing upstream identity.

## Experimental features

Experimental options currently include:

- nested Hako sessions
- local Kitty graphics rendering for attached clients
- pane history persistence
- CJK IME hidden-cursor anchoring
- agent-scoped CJK IME anchoring
- configurable CJK IME anchor cursor shape
