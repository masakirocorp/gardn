# Oh My Herdr features

This is the product feature reference for Oh My Herdr.

## Workspace model

### Sessions

A session is a persistent Oh My Herdr runtime with its own sockets, panes, tabs, workspaces, and saved state.

- **Default session** — `omh` launches or attaches to the default background session.
- **Named sessions** — `omh --session <name>` and `omh session attach <name>` select separate runtime namespaces.
- **Detach / reattach** — clients can detach while panes and agents continue running in the server.
- **Remote attach** — `omh --remote <target>` attaches to an Oh My Herdr server over SSH.
- **Remote bootstrap** — remote attach can detect the remote platform, reuse an existing compatible binary, or install a matching Oh My Herdr binary before connecting.
- **Remote server restart flow** — remote attach checks protocol/version compatibility and can prompt to stop or restart an incompatible remote server.
- **SSH keepalive fallback** — remote attach can add private generated SSH keepalive defaults without overriding your own SSH config.
- **Direct terminal attach** — `omh terminal attach <terminal-id>` and `omh agent attach <target>` attach directly to a single server-owned terminal.
- **Attach takeover** — direct attach is exclusive by default; `--takeover` can claim a terminal attachment from another client.
- **Multiple clients** — more than one client can connect to a server; each client owns its navigation and sidebar view, while the foreground interactive client drives shared runtime size, focus, theme, and keybindings.
- **Clipboard bridging** — thin clients forward OSC 52 clipboard writes locally and can bridge local clipboard-image paste into server panes.
- **Live server handoff** — supported updates can move live pane PTYs and session state into a replacement server so running pane processes survive a server swap.

### Workspaces

A workspace contains tabs, panes, cwd metadata, and agent state rollups.

- **Workspace creation and focus** — create, focus, rename, close, list, and inspect workspaces from the TUI, CLI, or socket API.
- **Workspace sidebar** — expanded workspace rows show the workspace name, activity state, and git/cwd summary.
- **Configurable sidebar metadata** — `[ui.sidebar.agents]` and `[ui.sidebar.spaces]` rows accept built-in tokens and `$custom` metadata reported through the socket API; defaults preserve compact workspace and agent labels across expanded, collapsed, and mobile views.
- **New-client sidebar defaults** — every app client starts with all spaces visible. `ui.sidebar.initial_state` and `ui.sidebar.initial_agent_scope` choose its initial expansion and agent scope; defaults are `expanded` and `all`, and one client's runtime changes never seed another client.
- **Workspace navigator** — search and filter groups, workspaces, tabs, and panes by text or state; open it with `prefix+w`, any desktop context-bar segment, or **Open workspace navigator** in the command palette. Group rows use their configured accents, while descendant labels remain neutral and agent-state colors appear only when meaningful. Group and workspace disclosure arrows expand each branch without leaving the navigator; `Space` toggles the visibly highlighted branch, `E` expands every branch, and `C` collapses the tree to its group roots, while clicking a row name focuses it. The tall, stable modal uses the same title/close header, dividers, detail row, and footer hints as other app modals, so branches do not move pointer targets between clicks.
- **Desktop context bar** — an independent bottom row shows the attached client's active group / workspace / tab path, plus the focused pane name when a tab is split, on the left and live topology counts on the right. It is visible by default; set `ui.context_bar = "never"` to hide it persistently, or toggle one client temporarily with `prefix+Down`. Every path segment opens the same workspace navigator with the matching group, space, tab, or pane visibly selected; narrow terminals drop counts before shortening the active path.
- **Workspace groups** — group workspaces, filter the sidebar by group, collapse groups, and assign per-group theme accent colors that tint group labels, tabs, menus, and related group UI.
- **Group lifecycle** — create, rename, delete, focus, and switch groups from the TUI, CLI, or socket API; reorder groups by dragging headers in the all-groups sidebar.
- **Group icons** — group creation and rename flows can choose from a curated set of 20 distinct icons.
- **Move between groups** — move workspaces between groups from the TUI/sidebar group workflows.
- **Public IDs** — CLI and socket API commands target workspaces, tabs, panes, and groups with public IDs; raw pane IDs remain compatibility inputs and are remapped after live handoff where possible.
- **Live cwd labels** — workspace labels can follow active pane cwd unless manually renamed.
- **Git worktrees** — list, create, open, and remove Git worktrees from the CLI and socket API.
- **Worktree safety** — generated worktree names are slugged into safe checkout paths, removal leaves branches intact, and dirty worktrees require forced removal.
- **Git summaries** — workspace summaries roll up added, modified, deleted, and conflicted files across detected repository roots.

### Tabs

A tab belongs to one workspace and contains one or more panes.

- **Tab lifecycle** — create, focus, rename, close, list, and inspect tabs.
- **Tab bar** — click tabs, close hovered tabs with the inline close button, use overflow scrolling, and switch with keybindings.
- **Tab drag reorder** — reorder tabs in the tab bar by dragging, with a drop indicator.
- **Tab-aware state** — workspace and agent UI can include tab context for agents and notifications.

### Panes

A pane is a terminal runtime inside a tab layout.

- **Pane splitting** — split panes vertically or horizontally.
- **Pane move** — move panes into another tab, a new tab, or a new workspace from the CLI or socket API.
- **Pane focus and zoom** — focus by direction, cycle panes, and zoom the focused pane.
- **Pane resize** — resize interactively from resize mode or by dragging borders.
- **Pane labels** — set manual pane labels; optionally show detected agent labels on pane borders.
- **Pane close** — close panes with confirmation where configured.
- **Scrollback** — scroll panes, edit scrollback in `$EDITOR`, and read visible/recent output through the API.
- **Pane history** — persist recent screen history to `session-history.json` by default.
- **Terminal identity** — panes advertise Oh My Herdr's terminal layer instead of leaking the outer terminal identity.
- **Snapshot restore** — saved sessions restore groups, active selections, sidebar sizing and arrangement, tabs, pane layouts, focus, zoom, cwd, labels, and agent session references.
- **Text selection** — mouse dragging leaves pane text highlighted until the next click or keypress. Keyboard copy mode remains available for explicit selection and copying.
- **Automatic selection copy** — `ui.copy_on_select` defaults to `true` and copies a drag selection on mouse-up. Set it to `false` to retain the highlight without writing to the clipboard. Double-click always selects and copies the clicked word.
- **Keyboard protocol encoding** — pane input honors negotiated terminal keyboard protocols, including Kitty CSI u and legacy modified-key sequences.

## Agent awareness

Oh My Herdr detects and tracks coding agents running inside panes.

### Agent states

- **Blocked** — agent needs user input, approval, or intervention.
- **Working** — agent is actively running.
- **Done** — agent finished work and has not been seen yet.
- **Idle** — agent is done and seen, or otherwise waiting without attention.
- **Unknown** — no supported agent state is currently detectable.

### Detection

Oh My Herdr combines foreground-process detection, terminal-screen heuristics, and optional integration reports.

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
- Kilo Code CLI
- Maki


- **Manifest rules** — bundled per-agent TOML manifests define screen, OSC title, and OSC progress matching rules for every built-in agent family, including OMP. Screen rules can provide strong visible evidence; OSC-only rules are fallback evidence and do not override hook authority as visible UI.
- **Manifest updates** — Oh My Herdr can cache newer remote manifests, reject downgrades or incompatible engine versions, reload local manifests through `omh server reload-agent-manifests`, and report updated detection rules through the normal toast/update path.
- **Wrapped-process hints** — Oh My Herdr-managed profiles automatically set `OMH_AGENT=<agent>` from the selected supported agent kind, so host-visible wrappers remain detectable on Linux and macOS. Set the hint explicitly only when launching a wrapper manually inside an arbitrary pane. The hint is process-scoped; avoid exporting it globally. Upstream-branded hint names are not accepted.

### Agent UI

- **Activity sidebar** — shows agents grouped by state across the current workspace, current group, or all workspaces; entries sort newest activity first and show compact relative activity age.
- **Agent focus** — focus agents from the activity panel, command surfaces, CLI, or socket API.
- **Agent labels** — manual, detected, and integration-reported labels are surfaced in lists and pane borders.
- **Agent metadata tokens** — pane metadata token patches are exposed consistently through pane/agent API snapshots and rendered without leaking one client's sidebar view into another.
- **State notifications** — background state changes can trigger Oh My Herdr toasts, terminal toasts, system toasts, and sounds.
- **Integration authority** — installed hooks either report native session identity for restore or report state directly. Claude Code, Codex, Pi, OMP, OpenCode, Hermes, Copilot, Qoder-style, and Grok Build integrations can report state directly; Kimi, Droid, and Cursor use session identity plus screen detection for state.
- **Missing integration warning** — if screen detection sees an integration-capable agent such as Codex but no accepted Oh My Herdr hook, session, or metadata report arrives for that pane, Oh My Herdr shows a pane-targeted toast with the matching `omh integration install <agent>` command.


### Agent profiles

- **System profiles** — Oh My Herdr exposes one read-only system profile for each supported integration target.
- **Custom profiles** — add or edit profile-specific commands from Settings > Agents. Oh My Herdr persists them to `[agent_profiles]`; known-family wrappers automatically receive the selected kind as `OMH_AGENT`, keep native profile/tooling restore behavior, and cannot override that managed identity through profile environment entries. `custom` unsupported agents are labeled `custom · launch-only`.
- **Group favorites and defaults** — group settings can promote favorite profiles with `ctrl+f` and set a default with `ctrl+d`. Favorites appear before available profiles while both sections keep the global profile order. When a group default is set, `new agent` starts it directly instead of opening the picker.
- **New agent launch** — choose `new agent` from the command palette, space context menu, tab context menu, or the tab `+` dropdown. Oh My Herdr starts the group default or only available profile immediately, or opens a favorites-first profile picker when multiple profiles are available.

### Agent session restore

Oh My Herdr resumes supported agents into native agent sessions during session restore by default. Set `[session].resume_agents_on_restore = false` to disable it.

- Supported restore sources come from installed integrations that report session references.
- Duplicate session references are deduplicated during a restore pass.
- Native agent restore suppresses pane-history replay so the resumed agent owns its conversation history.
- Restored agents launch as one-shot executable or shell-wrapper commands with their saved environment. OMP restores reconcile safe `.omp` and `.omp-*` session paths with the matching profile wrapper and environment before launch.

## Navigation and interaction

### Prefix mode

Oh My Herdr uses a prefix key before most built-in shortcuts. The default prefix is `ctrl+b`.

On macOS, `[experimental].switch_ascii_input_source_in_prefix = true` temporarily switches the host input source to an ASCII-capable layout while prefix mode is active, then restores the previous source when prefix mode exits.

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
- Configure `ui.right_click_passthrough_modifier` to send modified right-click hold/drag gestures to mouse-reporting pane apps while normal right-click keeps Oh My Herdr menus.
- Select pane text for copy workflows.
- **Mobile layout** — narrow terminals use a compact header and scrollable switcher for spaces, tabs, agents, and global menu actions.

### Copy mode

Copy mode is client-local: one attached client's cursor, selection, search, and scroll position do not affect another client. It supports directional `/` and `?` search with `n`/`N` repeats, tmux-style word motions, and full- or half-page navigation with the configured prefix.

### Navigator

The navigator is a workspace/tab/pane chooser.

- Search text matches whitespace-separated terms.
- Filter chips select blocked, working, idle, or done targets.
- Workspace rows can expand and collapse.
- Selection accepts a workspace, tab, or pane target.
- Mouse hover moves selection; row clicks accept targets.

### Command palette and command panel

Oh My Herdr can discover and run project commands. The command palette is also a general action surface for app, workspace, group, tab, pane, layout, agent-scope, settings, reload, notification, and detach/quit actions.

- Commands are scoped from the active workspace or selected workspace while navigating.
- Command rows are grouped by repo and branch context.
- Command status sections include running, failed, unknown, and stopped commands.
- Custom keybindings can launch shell helpers or pane commands.
- **Panel actions** — command rows can run, focus, expand, or stop commands from the right sidebar.
- **Git diff command** — command palette and contextual Git actions open the configured external diff command in a managed tab for the selected repository. The default command is `lazygit`; set `[git].diff_command` to use another terminal Git UI or plain `git diff`.
- **Command discovery** — Oh My Herdr discovers VS Code tasks, package scripts, just recipes, Make targets, and defaults for common Cargo, Go, Java, Python, .NET, PHP, and Ruby projects.
- **Managed reruns** — rerunning a managed command focuses an existing run or restarts a stopped/failed run in the same pane instead of spawning duplicates.

### Activity panels

The right sidebar can show agents, commands, and ports. Port entries include active/stale state, exposure labels, owner context, and click-to-focus behavior when an owner pane is known.
Shared ports can list multiple owner panes when more than one pane/process-tree owner is attributed to the same listener.

### Settings modal

Settings are edited in an in-app modal.

Tabs include:

- Appearance
- Notifications
- Behavior
- Agents
- Integrations
- Advanced

The modal supports keyboard navigation, mouse navigation, scrollbars, immediate settings updates, a top-right `esc close` affordance, a responsive tab bar, and install/update/uninstall actions in the integrations tab. Appearance owns theme, sidebar, and pane-label settings; notifications owns sounds and toasts; behavior owns prompts, terminal defaults, and the worktree directory.

### Help and confirmations

Oh My Herdr includes a scrollable keybinding help modal generated from current bindings, including custom command bindings. Destructive actions such as workspace close and group delete use confirmation dialogs that show the affected target.

### Global menu

The global menu exposes settings, keybinding help, config reload, update/release-note actions, and detach from sidebar and mobile menu surfaces.

## Integrations

Oh My Herdr ships installable integrations for agents that report semantic state, native session identity, or both over the socket API.

Built-in installable integrations:

- pi
- OMP
- Claude Code
- Codex
- Grok Build
- OpenCode
- Hermes

Integration management supports:

- install
- uninstall
- status checks
- outdated-version detection
- in-app integration management

Integration install side effects are agent-specific: pi and OMP install extensions, Claude, Codex, Grok Build, Kimi, Droid, Cursor, Copilot, and Qoder-style CLIs install/update hooks or settings, OpenCode installs a plugin, and Hermes installs/enables a plugin.

Claude Code, Codex, Pi, OMP, OpenCode, Hermes, Copilot, Qoder-style, and Grok Build integrations can report state directly. The Grok Build integration reports native session identity plus parent-agent working, blocked, idle, and release transitions while ignoring child-agent completion as a parent completion. Its Oh My Herdr-owned hook also prevents Grok's Claude and Cursor compatibility hooks from claiming Grok panes.

Integration path overrides include `PI_CODING_AGENT_DIR`, `PI_CONFIG_DIR`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME`, `KIMI_CODE_HOME`, and `CURSOR_CONFIG_DIR`. OMP install/status checks scan `.omp` and `.omp-*` extension directories.
- On Windows, installable integrations are limited to CLI hook integrations with supported path layouts: Claude, Codex, Copilot, Grok Build, Kimi, Droid, and Qoder-style CLIs.


## Plugins

Oh My Herdr plugin v1 lets local extensions add actions, panes, link handlers, and event hooks through the Oh My Herdr socket API and CLI.

Plugin manifests use `omh-plugin.toml` with `min_omh_version`. Oh My Herdr also accepts upstream-compatible `herdr-plugin.toml` and `min_herdr_version` aliases, but Oh My Herdr names are preferred for new plugins.

Plugins run unsandboxed as the current user. Remote installs show source, build commands, actions, panes, link handlers, and event hooks before install, and require confirmation unless `--yes` is passed.

Installed plugin registry entries survive live server handoff, so linked plugins remain available to the replacement server and later registry writes preserve the complete set.

Plugin panes are normal Oh My Herdr panes. Their pane attribution follows pane moves and is removed when tabs, workspaces, layouts, or plugins remove the pane.

Plugin commands receive `OMH_*` context variables, including plugin root/config/state directories and active workspace/tab/pane ids. Protected Oh My Herdr/plugin variables cannot be overwritten by plugin-provided env overrides.

## External tools

Oh My Herdr is a terminal workspace manager, so some features call user-installed tools instead of bundling every backend.

| Tool | Used for | Requirement |
| --- | --- | --- |
| `git` | Git status, repository discovery, worktree operations, and Git-aware project commands. | Required for Git-aware features. |
| Configured Git diff command | Repository review from command palette and contextual Git actions. Defaults to `lazygit`; configure `[git].diff_command` for another command. | Optional; required only when using the Git diff action. |
| Agent CLIs such as `pi`, `omp`, `claude`, `codex`, `grok`, `opencode`, `hermes`, `copilot`, `kimi`, `droid`, `qodercli`, and `cursor-agent` | Launching agent panes and installing/updating matching Oh My Herdr integrations. | Required only for the agent/profile the user launches or integrates. |
| `python3` | Installed hook scripts for agent integrations. | Required for hook-based state/session reports; hooks exit quietly when it is missing. |
| `curl` | Update checks, release downloads, manifest refreshes, and remote bootstrap downloads. | Required for those networked update/bootstrap features. |
| `ssh` | Remote attach, remote install, and remote client bridge. | Required for remote features. |
| `lsof` | Local TCP listener discovery for the ports panel. | Optional; missing or failing probes produce no port observations. |
| macOS `pbcopy`, `pbpaste`, `open`, `/usr/bin/osascript`, optional `terminal-notifier`, and optional `mdfind` | Clipboard, URL opening, and system notifications on macOS. | Platform helpers; Oh My Herdr falls back where possible. |
| Linux `xdg-open`, `notify-send`, `wl-copy`, `wl-paste`, `xclip`, and `xsel` | URL opening, system notifications, and clipboard/image paste on Linux. | Optional per feature and display server; missing helpers disable the matching bridge/fallback. |
| macOS `afplay` | Custom sound notification playback. | Required only for custom notification sound playback on macOS. |

## CLI and socket API

Oh My Herdr exposes the same runtime model through the CLI and local Unix socket API.

### CLI areas

- **`omh status`** — show client/server status and protocol compatibility.
- **Protocol guard** — operational CLI commands verify the server wire-protocol version before dispatch and return a request-correlated JSON error with update/restart guidance on mismatch; status checks and live handoff remain available for diagnosis and recovery.
- **`omh session`** — list, attach, stop, and delete named sessions.
- **`omh workspace`** — manage workspaces.
- **`omh worktree`** — manage Git worktree checkouts.
- **`omh tab`** — manage tabs.
- **`omh pane`** — manage panes, read output, send input, report agent state, and run commands.
- **`omh agent`** — list, inspect, focus, read, send to, attach to, rename, and start agents.
- **`omh agent explain`** — inspect why an agent pane is classified as idle, working, blocked, unknown, or skipped by manifest detection.
- **`omh wait`** — wait for output matches or agent status changes.
- **`omh integration`** — install, uninstall, and inspect agent integrations.
- **`omh group`** — list, create, focus/switch, rename, and delete workspace groups.
- **`omh config reset-keys`** — remove custom keybindings while preserving the rest of the config.
- **`omh update`** — self-update supported binary installs; `--handoff` can preserve live panes while moving running sessions to the updated server.
- **`omh server`** — run the headless server, stop it, reload config, or trigger a live handoff.
- **`omh api`** — print or write the generated public API schema and request a live session snapshot.
- **Launch flags** — `--no-session`, `--default-config`, and `--remote-keybindings <local|server>` control startup and remote behavior.
- **JSON output** — status, session, and worktree commands expose machine-readable output where supported.
- **Read modes** — pane and agent reads support visible, recent, recent-unwrapped, ANSI, raw, and bounded line output.
- **Wait matching** — output waits support substring or regex matching, raw matching, timeouts, and agent-status waits.
- **Automation reads** — pane and agent output can be consumed as rendered visible text, recent scrollback, ANSI, or raw output for agent feedback loops.

### Socket API

The socket API supports typed request/response calls and event subscriptions. It is the local JSON control plane; interactive render streaming and terminal attach use the separate client wire-protocol socket.

The public website combines authored transport, lifecycle, trust, compatibility, workflow, and error guidance with deterministic shape reference generated from a specified `omh` binary. Published schema JSON is immutable at `/api/<product-version>/schema.json`; the `/api/latest/schema.json` alias is reserved for release deployment. Generated Local API material excludes the separate internal client wire and handoff protocols.

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
- session snapshots
- terminal observe and control streams
- pane scroll state
- workspace groups
- integration authority reports
- protocol and capability ping

## Appearance and notifications

### Themes

Oh My Herdr supports terminal-derived colors and built-in palettes.

- **Theme source** — terminal colors or theme palettes.
- **Appearance mode** — system, light, or dark.
- **Light and dark palette selection** — choose separate palettes when system mode is enabled.
- **Live system sync** — in system mode, Oh My Herdr follows foreground host-terminal light/dark color changes while it is running and refreshes pane terminal defaults.
- **Custom token overrides** — override individual theme colors.
- **Group settings** — rename or delete groups, assign per-group theme accent colors, choose favorite/default agent profiles, or inherit the global accent from the group settings modal.
- **Accent color** — configure highlight, border, and navigation accent color; when following terminal colors, choose separate terminal ANSI accents for light and dark appearances.

### Sound and toasts

- **Toast delivery** — off, Oh My Herdr, terminal, or system.
- **Sound notifications** — request and done sounds for background agent activity.
- **Per-agent sounds** — agent-specific sound overrides.
- **Validation** — invalid or missing sound files fall back to defaults and emit diagnostics.
- **Terminal toast backends** — terminal toasts use supported terminal notification protocols, including tmux passthrough where available.
- **Custom sound files** — request/done sounds can use MP3 files resolved relative to the config file.
- **Sound disable switch** — `OMH_DISABLE_SOUND` disables playback.

## Configuration

Configuration file: `~/.config/omh/config.toml`.
Oh My Herdr treats `config.toml` as a stable hand-editable configuration surface. Settings modal changes rewrite their owned keys or sections, preserve unrelated sections, and reload the file into the running app after successful writes.

Runtime reload is section-scoped for live sections: valid sections apply, invalid sections keep the previous live settings and emit diagnostics through the app/server reload path.
- **Offline validation** — `omh config check` validates `config.toml`, prints diagnostics, and exits without starting or attaching to a session.
- **Configuration status** — startup and reload diagnostics raise one transient toast, then remain available from the bottom-left `config issue` status and its diagnostics modal until a successful reload clears them.


Configurable areas include:

- onboarding
- theme
- terminal shell and new-terminal cwd policy
- session restore
- keybindings
- indexed shortcuts
- custom command keybindings
  Shell actions and temporary pane actions run through the platform command interpreter: `/bin/sh` on Unix and `cmd.exe` on Windows.
- multiple bindings per action
- prefix-mode and direct key chords
- sidebar size, initial state, and mouse behavior
- close and naming prompts
- initial agent panel scope
- pane border labels
- toast and sound settings
- worktree directory
- scrollback limit
- experimental features

## Updates and release notes

Direct installs use GitHub Releases for update checks, release metadata, and binary downloads on Linux, macOS, and Windows. mise and Nix-managed installs are routed to their package manager instead of self-update.

- The app can notify when a new release or managed-install update is available.
- `omh update` downloads and swaps supported direct binary installs.
- mise and Nix-managed installs are blocked from self-update and should use their package manager.
- Live handoff can preserve running pane processes during updates when both the old and new server support the handoff protocol.
- Windows direct updates use the stable `omh-windows-x86_64.exe` release asset; Oh My Herdr does not use a preview channel.
- In-app release notes can be shown after an update.
- Post-update checks can report outdated integrations.
- Product announcements can be shown separately from release notes and tracked as seen per version.
- First-run onboarding introduces the core workflow in-app.
- Update-ready dialogs can show release notes and the install command before the update is applied.

## Fork maintenance

Oh My Herdr tracks upstream Herdr commits with an explicit port ledger.

- **Upstream port ledger** — `upstream-port-map.json` records each upstream commit as ported, superseded, skipped, or pending.
- **Ledger check** — `just upstream-status` reports upstream status and fails when commits are unclassified or still pending.
- **Sync guard integration** — upstream-sync reports include the ledger status so product-specific skips and superseded changes stay visible.
- **Oh My Herdr-owned surfaces** — docs, website, release, and repository-process commits can be skipped with explicit reasons instead of silently reintroducing upstream identity.

## Experimental features

Experimental options currently include:

- nested Oh My Herdr sessions
- local Kitty graphics rendering for attached clients
- CJK IME hidden-cursor anchoring
- agent-scoped CJK IME anchoring
- configurable CJK IME anchor cursor shape
- macOS prefix-mode ASCII input-source switching
