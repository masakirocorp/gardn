## gardn@0.9.3

### Stabilize OMP conversations after resizing

Loaded OMP conversations no longer enter a resize and repaint loop after the host window changes size.

## gardn@0.9.2

### Preserve official release identity in packaged binaries

GitHub release binaries now keep their release channel, tag, and build cohort when Cargo runs through Turborepo. Official installs use the production namespace instead of the development namespace.

## gardn@0.9.1

### Keep release CI compatible with Rust 1.98

Release checks now pass with the Rust 1.98 lint set used by GitHub Actions.

## gardn@0.9.0

### Add a Follow Up queue to the Agents sidebar

The Agents sidebar now keeps an always-visible Follow Up section. Drag an agent onto it to remember that pane; it stays queued with its real working/blocked/idle state and a waiting age until you submit a plain Enter in that terminal. The queue is shared across attached clients and survives restore. There is no manual dismiss control.

### Make agent lifecycle and prompts more reliable

Gardn now keeps agent ownership until the process exits, restores supported agent sessions more accurately, and handles prompt submission and blocked forms without premature idle transitions. Integration support now includes current Windows install paths and the latest Hermes, Pi, OpenCode, Cursor, Devin, Grok Build, and related lifecycle behavior.

### Improve Local API and session correctness

Pane and agent reads now report when older requested rows were omitted. Local API errors, event delivery, workspace focus, Git metadata, pane history, and session restoration now remain consistent across reconnects, alternate screens, workspace changes, and client handoff.

### Managed SSH worker lifecycle

Gardn now installs and updates versioned SSH execution workers automatically when a connection starts. Protocol-compatible workers with live runtimes stay active until they are unused. After a coordinator restart, restored remote panes reconnect their saved SSH connection and re-adopt the live worker runtime. Worker bridges use dedicated SSH transports so a stopped coordinator cannot leave orphaned remote bridge processes that block the next reconnect.

Development builds now install SSH workers from matching local build cohorts instead of published release assets. `just install-dev` installs `gardn-dev` and both Linux worker sidecars as one operation. If a sidecar is missing, Gardn names that command instead of exposing the internal build cohort and filesystem path. Gardn verifies the staged worker's source identity, target, protocols, lifecycle, and capabilities before it publishes the worker manifest.

Removing an SSH connection now shows its impact across all sessions before confirmation. The removal fences new work, drains remote panes, updates dormant session placement, removes only worker bindings owned by that connection, and keeps durable retry state if any step cannot finish safely. When remote cleanup is unavailable, the failure screen warns that remote work might remain and offers local removal, retry, or cancel.

### ENG-88 UI/mobile/config/tab-label/navigator port

Target SHAs: 4ffd99c2, 4421c0fe, 2ff5dd2f, db1ef28d, 14d8e933, b44ca3b3,
32e3d7b7, bc764c83, f54d8e8c, 2a1a8d64, 1e1d0632, 010afe53.

Implemented:
- Compact auto tab labels + readable active labels via `Workspace::tab_display_name` and display-width helpers.
- Configurable pane gaps / borders / hide-single-tab-row / collapsed-sidebar mode added to `UiConfig`, `AppState`, and config reload.
- Shared `ui/text.rs` display-width utilities with CJK-aware measurement.
- Mobile switcher agents-first ordering.
- Navigator search commands preserved (copy-mode search UI intentionally left to Eng86CopyMode).
- Plugin-driven tab rename refreshes tab bar via `emit_layout_updated_event` -> plugin context -> tab label.

### Speed up Rust development builds

Development builds now compile the vendored terminal engine without release optimization, omit routine debug information, exclude unused Ratatui features, and cache the stable Local API contract in a separate workspace crate. Release builds remain fully optimized, and the `debugging` profile still provides full LLDB symbols.

### Manage agent integrations on SSH hosts

The Integrations settings can now inspect, install, update, and uninstall agent integrations on Local or a configured SSH execution host.

Remote agent panes now send lifecycle reports through a restricted, token-authenticated worker endpoint. The coordinator Local API socket and unrelated profile arguments or environment variables are not exposed to the remote pane.

### Preserve host ANSI colors in pane applications

Pane applications that query terminal colors now receive the active host ANSI palette instead of libghostty defaults. Application-defined palette colors still take precedence until the application resets them.

### Add direct pane graphics APIs

Local API clients can now set, inspect, stream, and clear pane graphics. Direct file-backed frames reduce repeated image copies when the attached terminal supports them, while bounded inline frames remain available as a fallback.

### Refresh project dependencies

Updated the Rust application, website, repository tooling, Nix inputs, and GitHub Actions to current policy-compatible releases. The refresh preserves the Local API and client wire contracts while improving clean and incremental Rust build times.

### Improve remote and Windows reliability

Windows can now run as a local thin client for remote Linux and macOS sessions and can read clipboard images for remote paste. Remote attach, client shutdown, terminal restoration, process ownership, and Windows input-method handling now recover more reliably from disconnects and process exits.

### Clearer settings resource workflows

Agent profiles now use a compact browse-and-edit flow with a separate delete area. SSH connections now open to a status and control screen, keep editing as an explicit secondary action, and present new connections as a focused form with the SSH target first. Keyboard navigation now follows the visible connection actions, including connection tests and removal. Connection test results remain legible above the settings dialog. Mouse removal now starts immediately for attached clients, shows animated inventory progress while it calculates the removal impact, and scrolls the completed result into view.

### Hierarchical settings navigation

General Settings now uses an expandable left sidebar instead of horizontal tabs. Category subsections jump directly to their content while the right panel keeps the existing heading, description, controls, and scrolling behavior. All settings screens now separate their heading and description from controls with a horizontal rule. Blank rows divide logical option groups and independent command fields. Group Settings keeps its compact tabs, and Space Settings keeps its single-panel layout. Controls, menus, navigation, status text, and modal labels now use consistent title casing throughout the app.

### Improve terminal input and mouse behavior

Gardn now preserves native terminal key events more accurately, including Kitty keyboard releases, shifted keys, text input, and Windows ConPTY input. Mouse selection, URL clicks, horizontal scrolling, and per-pane right-click routing now follow the pane and host-terminal state consistently.

### Add terminal UI controls

You can now filter keybinding help by shortcut, action, or section. Settings > Appearance > Panes exposes pane borders, pane scrollbars, pane gaps, and the single-tab bar. Settings > Appearance > Window edits the outer terminal window-title template. Settings > Behavior exposes workspace naming, copy-on-select, agent-session resume, and every accepted right-click passthrough modifier combination. Settings > Advanced > Server configures the headless terminal size. You can also choose dot or symbol agent status indicators and route right-clicks to individual panes. Disabling automatic copy now keeps mouse selections visible until you copy or clear them. Context bar, sidebar, and status indicator choices now save without corrupting `config.toml`.

Settings > Commands now identifies each launcher by purpose instead of exposing its internal working-directory rules. Each command can be reset to its built-in value, and one action resets all four.

Settings now covers the remaining stable config values that belong in the modal. Behavior > Terminal edits the default shell and shell startup mode. Notifications expands popups with background-alert delay and in-app toast position, and adds clipboard copy confirmation. Advanced > Updates toggles version and manifest checks. Custom agent profiles can be disabled without changing their identity.

Arbitrary theme token overrides and `ui.accent` compatibility were removed. Appearance now uses built-in themes plus the six terminal accent choices.

## gardn@0.3.2

### Complete cross-platform release packaging

Release builds now preserve Cargo arguments on Windows so all five platform binaries can be published from one tag.

### Open GitHub projects in managed tabs

Add a configurable GitHub project command that opens ghui in a managed Space tab from Settings, the command palette, and project menus.

### Match group accents to terminal themes

Match group accent choices to the active theme palette, including Terminal themes.

### Complete live handoff responses

Fixed a live handoff race that could close the API connection before the success response reached the client, even though the replacement server started correctly.

### Improve remote and server reliability

Improved remote and server reliability: high-latency remote handshakes get a longer connection window, remote helper installs work with non-POSIX login shells, SSH authentication failures include actionable guidance, and `gardn server stop` waits for both sockets to become unreachable before returning.

## gardn@0.3.1

### Restore Windows builds for mixed-host support

Windows builds now keep Unix-only SSH execution workers behind the platform boundary while preserving explicit unsupported results for SSH connection actions.

## gardn@0.3.0

### Automate live agents from the CLI and Local API

Use `gardn agent start`, `prompt`, `send-keys`, and `wait` to launch and drive a named agent without changing the pane layout unexpectedly. Prompt waits follow the same agent and pane through real status transitions, stop cleanly on disconnect, and report stalled or ended agents explicitly.

Plugins and other Local API clients can also install a client-local filtered and sorted agent view without changing what another attached client sees.

### Improve agent status in pane borders

Pane borders can now show an agent name with its live status, while Pi and OMP agents remain working through compaction. Pi becomes idle only after its root session emits a settled event while actually idle, so stale settlement signals do not end active work.

### Distinguish agents by tab and pane

Agent sidebar rows now append tab and pane context only when multiple running agents would otherwise share the same workspace label.

### Add changelog menu

The global menu now has a stable changelog entry backed by Tegami-managed release notes.

### Remove nonexistent Homebrew update prompts

Gardn no longer checks or recommends Homebrew updates before a Homebrew formula exists.

### Configure new-client sidebar defaults

New clients now start expanded with all spaces and agents visible. The settings modal and generated config can customize the initial expansion and agent scope without changing existing clients.

### Fix attached-client navigation and dialogs

Attached clients now highlight the agent pane they focused, route rename and new-group dialog clicks to the visible dialog, and let first-run onboarding continue with Enter or the Continue button without changing another client's view.

### Refine collapsed sidebar navigation

Collapsed sidebars now preserve group context and agent status sections, reveal full workspace details on hover or keyboard selection, and keep the help and expand controls together at the bottom of the rail. Group creation and rename dialogs now use single-cell icons so compact rows remain aligned.

### Keep configuration issues accessible

Gardn now reports startup configuration problems once, keeps them accessible from the bottom-left status menu, and provides a numbered diagnostics modal with reload, close, and click-outside dismissal.

### Add configurable selection copying

Mouse dragging always leaves pane text selected. `[ui].copy_on_select` controls only whether a drag selection is copied automatically on mouse-up; double-click still selects and copies a word.
Selections remain client-local and stay aligned with visible text when a client is scrolled into terminal history.
After selecting text, the next click now activates workspace, tab, and control targets normally.

### Keep unnamed pane numbers dense

Unnamed pane labels now use their current position within each tab, so closing or moving panes no longer leaves gaps in the sidebar, context bar, or workspace navigator. Stable pane IDs remain unchanged.

### Remove built-in worktree management

Gardn no longer creates, opens, lists, or removes Git worktrees. Use Worktrunk, another Git tool, or your coding agent to manage checkouts, then open the resulting directory as an Gardn workspace.

The worktree CLI commands, socket API methods and events, settings, dialogs, grouping behavior, and documentation have been removed together. Existing workspace and Git status behavior is unchanged.

### Add first-class Grok Build integration

Install and manage a native Grok Build hook from Gardn Settings or `gardn integration`. Grok panes now report session identity and authoritative parent-agent activity without being misidentified by compatibility hooks.

### Fix group creation in the sidebar

New groups now start with an initial space without leaving the all-groups view, group context menus create spaces in the selected group, and the expanded sidebar labels the all-groups scope as **groups**.

### Fix keyboard and terminal input

User keybindings now reliably override built-in defaults, shifted shortcuts work across supported terminal protocols, physical Escape remains distinguishable from VT mouse/control sequences on Windows, and key releases no longer duplicate pane input.

### Update the embedded terminal engine

Updated `libghostty-vt` to the July 16 snapshot while preserving ordered color replies, child color overrides, grapheme clusters, and Kitty graphics rendering.

### Follow live system appearance changes

When Appearance is set to System, Gardn now refreshes colors from the foreground host terminal while running, so Gardn and pane terminal defaults follow light/dark changes without a restart.

### Publish versioned Local API reference

The public website now builds deterministic request, response, event, error, and enum reference from a specified Gardn binary. Versioned schema artifacts include product and protocol metadata, while authored guidance documents the local socket transport, trust boundary, lifecycle, compatibility, errors, and subscription behavior.

### Detect wrapped agents

macOS wrappers can now identify a hidden foreground agent with the process-scoped `GARDN_AGENT` environment hint, matching Gardn's existing Linux behavior without accepting upstream-branded variables. Gardn-managed profiles set the hint automatically from their selected supported agent kind, including when a profile command runs through a wrapper.

### Detect Maki agent activity

Gardn now detects Maki panes as working, idle, or blocked and supports per-agent sound overrides for Maki notifications.

### Launch the Gardn product website

The public website now explains the persistent terminal workspace workflow, supported platforms, agent-aware features, and source installation paths through a dedicated marketing experience. Download and release pages remain explicitly gated until verified public binaries exist, and complete social metadata, responsive layouts, accessible motion, and built-site checks keep the public entry points accurate.

### Run selected resources on SSH hosts

Gardn sessions can now place Groups, Workspaces, tabs, panes, and agents on configured SSH execution hosts while local resources remain available in the same session. Connection settings, CLI commands, and the Local API expose explicit host-and-path placement. Host-routed terminals preserve their remote runtime across client reconnects and route Git, worktree, command, process, port, path, and agent operations to the owning host. Plugin pane processes remain Local-only in plugin v1 and fail explicitly when a remote placement is selected.

### Refine the mobile workspace layout

Narrow terminals now keep a one-row, group-accented workspace and tab header above the terminal. The mobile switcher uses the shared group, space, tab, and pane hierarchy, expands only the active path, shows agent state on pane rows, and keeps mouse and keyboard targeting aligned with the rendered rows.

### Standardize responsive modal layouts

Modals now share close, footer, action-row, list, and text-field geometry so rendering and mouse hit targets stay aligned across normal and narrow terminals. Long Unicode names truncate by terminal-cell width without hiding right-aligned shortcuts or status metadata, focused inputs keep their cursor end visible, and command palette, agent profile, Git repository, navigator, keybind, and product-announcement surfaces now use the same visual hierarchy.

### Organize source workspace

The source tree now uses a pnpm/Turborepo workspace with the Gardn app in `apps/gardn` and separate docs and Nix release-note scopes.

### Preserve named sessions during live handoff

Gardn now keeps explicitly selected named sessions on their own API and client sockets during live handoff, even when inherited environment variables contain stale socket overrides.

### Unify native Rust task orchestration

Turborepo now discovers the Cargo workspace directly and orchestrates Rust quality gates, website contract inputs, platform CI, and release builds in one mixed-language task graph.

### Simplify workspace navigator branches

The workspace navigator now shows pane rows only for split tabs and renders workspaces or tabs without visible children as leaves, without disclosure controls. Singleton pane names and agent metadata remain searchable through their workspace or tab row.

### Rename the product to Gardn

This release is a breaking clean cutover to the Gardn product identity. Install and invoke `gardn`, use `~/.config/gardn` and `GARDN_*` environment variables, and expect `gardn-*` release assets from `masakirocorp/gardn`. The old product executable, environment namespace, runtime paths, repository URLs, and release asset names are not compatibility aliases.

Gardn remains the upstream project and attribution source. Intentional Gardn compatibility names continue to use the Gardn namespace.

### Preserve OMP profiles during session restore

Restored OMP sessions now retain their profile wrapper, environment, integrations, and credential context, including recovery from snapshots previously saved with conflicting default-profile launch data.

### Expand plugin automation and pane surfaces

Plugins can declare once-per-server startup commands and open split, tab, zoomed, or client-owned popup panes. Popup focus, input, sizing, and teardown stay isolated to the client that opened them, while ordinary plugin panes keep their attribution as they move through session layouts.

Installed and linked plugins now use one user-level registry shared by named sessions. Installation, uninstallation, linking, and listing continue to work when no server is running, and plugin-provided environment values cannot replace Gardn's protected runtime context.

### Navigate mobile workspaces one level at a time

The compact mobile switcher now keeps an expandable agent summary visible above every hierarchy level, with triage, working, and idle counts plus direct navigation from each expanded agent row. Workspace navigation moves through groups, spaces, tabs, and panes one level at a time; a persistent breadcrumb, aligned counts, contextual creation actions, and a separate actions screen keep the interface easy to scan with either mouse or keyboard.

### Configure Git, Diff, and IDE project commands

Settings > Commands now owns three independent project launchers: Git, Diff, and IDE. They default to LazyGit, Hunk watch mode, and Fresh, remain freely editable, and can be hidden individually by clearing a field. Git and Diff require an observed repository; IDE remains available for any workspace and uses its repository or working directory. All three actions are available from the command palette and workspace/new-tab menus, ordered IDE, Git, then Diff after Tab and Agent. Curated launchers preserve host terminal colors when Gardn's theme is Terminal; named themes apply the matching external-tool theme and resolve standard ANSI colors through the same palette. Custom commands keep native terminal color behavior. If LazyGit, Hunk, or Fresh is missing, the managed tab shows install guidance and the tool's GitHub URL.

### Publish the public product manual

The website now provides task-first installation, onboarding, workspace, terminal, remote, integration, plugin, update, troubleshooting, configuration, CLI, keybinding, platform, and product-concept documentation. The root README is now a concise repository entry point, and website validation rejects stale product names, internal interfaces, unsupported installation claims, and missing public routes.

### Launch the public website

The Gardn product site and manual now publish from Cloudflare at `gardn.dev` with production canonical metadata, static search, immutable versioned Local API schemas, security headers, and explicit source-install and release gates. The launch review also removed an invalid tagged-source installation path before publication.

### Add a responsive workspace context bar

Desktop clients now show an independent, clickable, client-local group / workspace / tab path with focused pane context. Optional topology and section counters can be enabled globally with `ui.show_counters`; they are hidden by default across desktop and mobile views. Every path segment opens one tall, stable-height, grouped workspace navigator with its matching row visibly selected, configured group accents carried into the hierarchy, and conditional tab and pane rows already visible for the active group. The navigator is also available from the command palette, shares the app's standard modal header, close action, dividers, and footer hints, lets `Space` toggle the visibly highlighted branch, and provides `E`/`C` controls to expand or collapse the full hierarchy; the bar supports per-client toggling and remains usable on narrow terminals.

### Improve runtime reliability and config validation

Added `gardn config check`, more tolerant graceful shutdowns, safer custom-command child reaping, complete initial API request reads, and stronger local connection handling across supported platforms.

### Create from blank sidebar space

Right-click the blank area after the final group or space to open a compact creation menu. New spaces follow the active workspace's group.

### Add configurable sidebar metadata

Agent and workspace sidebar rows now support built-in and API-reported custom metadata tokens across expanded, collapsed, mobile, and attached-client views.

### Keep multi-client terminal layouts stable

Each shared tab now has one explicit interactive controller instead of following whichever client was most recently active. Other clients render the controller-sized terminal canvas in a client-local viewport, can navigate, focus, scroll, search, and copy without resizing the PTY or changing terminal content, and can take control explicitly with `prefix+t` or the persistent desktop/mobile control action. Control is released without auto-promoting a watcher when the controller changes tabs, disconnects, or enters direct terminal attach.

### Refine workspace and tab navigation

Interactive workspace creation can optionally ask for a name, the workspace navigator keeps hierarchy controls and direct-match results consistent, and overflowed tab bars return the active tab to view after focus changes without fighting deliberate manual scrolling.

### Focus terminals with Zen mode

Toggle Zen mode from the command palette or with `prefix+shift+z` to give terminal panes the full client viewport. Zen mode temporarily hides the sidebars, tab bar, mobile header, and context bar without changing other clients.
