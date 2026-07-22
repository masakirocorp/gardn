# Manual QA matrix

Use this guide to select and run the manual checks that Oh My Herdr's automated suite cannot prove reliably. It complements `just check`; it does not repeat state, protocol, socket, PTY, or render behavior already covered by automated tests.

Run M01-M08 before tagging a release, then run M09 against the published artifacts. Run affected P1 cases when changing their surface, and run the full P1 set for broad platform, terminal, or lifecycle changes.

## Test record

Record this environment before each run:

- commit SHA and Oh My Herdr version
- binary source and checksum
- OS and architecture
- terminal application and version
- shell
- session and config namespace

Record each selected case as `PASS`, `FAIL`, or `BLOCKED`. For failures, preserve the relevant Oh My Herdr logs, exact reproduction steps, and a screenshot or short recording when presentation matters. Track each defect separately and link it from the run record.

## Matrix

| ID | Priority | Surface | Required environment | Manual risk |
| --- | --- | --- | --- | --- |
| M01 | P0 | First launch and core TUI | macOS arm64, Ghostty, and one non-Kitty terminal | Visible layout, focus, hit targets, onboarding |
| M02 | P0 | Terminal input and output | Real terminal, IME, mouse-reporting app | Unicode width, paste, keyboard protocol, selection, graphics |
| M03 | P0 | Detach, reattach, named sessions | Two terminal windows | Process continuity and session isolation |
| M04 | P0 | Two live app clients | Two terminals with different dimensions | Foreground ownership and client-local UI isolation |
| M05 | P0 | Restore and persistence | Rich saved session | Layout, cwd, history, and session identity after restart |
| M06 | P0 | Live handoff and update | Long-running PTY and TCP listener | Process loss, duplicate ownership, stale sockets |
| M07 | P0 | Real agent lifecycle | Grok Build and one established integration | Authentication, lifecycle reporting, parent state, restore |
| M08 | P0 | Remote attach and bootstrap | Reachable Linux SSH host | Transport, bootstrap, compatibility prompt, reconnect |
| M09 | P0 | Release artifacts | macOS arm64, Linux x86_64, Windows x86_64 | Interactive behavior of downloaded binaries |
| M10 | P1 | Host bridges | macOS and Linux where available | Clipboard, URL, toast, notification, and sound helpers |
| M11 | P1 | Mouse, responsive UI, external tools | Wide and narrow terminals | Drag geometry, compact layout, commands, ports |
| M12 | P1 | Sleep, wake, and recovery | macOS laptop and abrupt client loss | Recovery under real OS lifecycle events |

## M01: First launch and core TUI

1. Use an isolated `omh-dev` configuration or disposable OS user and a named QA session.
2. Launch with no server, complete onboarding, and confirm the first shell is usable.
3. Create two workspaces, a group, three tabs, and a three-pane layout.
4. Navigate the sidebar, tabs, global menu, navigator, command palette, Settings, help, and confirmation dialogs once by keyboard and once by mouse.
5. Resize from wide to approximately `60x20`, then return to wide.

Pass when no control becomes inaccessible or misleading, no stale hover or focus remains, the layout stays coherent, and destructive dialogs identify the correct target.

## M02: Terminal input and output

1. Type and paste ASCII, multiline text, CJK, emoji, combining characters, and an IME-composed phrase into a shell and editor.
2. Exercise arrows, modifiers, function keys, Kitty CSI-u input, and legacy application key modes in an editor or TUI.
3. Produce long scrollback; scroll, search, enter copy mode, drag-select, double-click-select, and paste the copied result.
4. Run a mouse-reporting application and test normal mouse handling plus configured right-click passthrough.
5. Display an OSC 8 hyperlink and a Kitty image where supported.

Pass when input has no dropped or duplicated bytes, character widths remain aligned, modifiers do not stick, selections and scroll position remain stable, and supported links and images render and clear correctly.

## M03: Detach, reattach, and named sessions

1. Start a visible counter and a local HTTP listener in separate panes.
2. Detach, wait for additional counter output, and reattach.
3. Open a second named session and verify its workspaces and processes are isolated.
4. Close one client abruptly, reconnect, and verify both workloads remain alive.

Pass when output advances while detached, pane targets remain usable, the server survives client loss, and no state crosses named-session boundaries.

## M04: Two live app clients

1. Attach two real terminal windows with materially different dimensions.
2. Open the navigator, Settings, copy mode, and a modal in client A while client B continues normal pane interaction.
3. Type from the foreground client, then transfer foreground ownership to the other client and type again.
4. Detach each client independently.

Pass when overlays, selections, scroll positions, and navigation remain client-local; input reaches only the intended pane; and PTY size follows the active client without oscillation or corruption.

## M05: Restore and persistence

1. Build a session containing groups, custom accents or icons, several workspaces and tabs, split layouts, zoom, cwd changes, labels, scrollback, and a resumable agent session.
2. Record the visible state, stop the server cleanly, and relaunch the same session.
3. Verify layout, active targets, cwd, labels, history, and agent identity.
4. Resume the agent and verify conversation continuity without replayed pane-history noise.

Pass when no workspace is lost, active tabs and panes remain correct, cwd and labels persist, focus remains coherent, and the agent session is neither lost nor duplicated.

## M06: Live handoff and update

1. Run a counter, an interactive shell, and a TCP listener with a recognizable response.
2. Perform the supported handoff or update from the current binary to the candidate binary.
3. During and after handoff, probe the listener, type into the shell, and confirm counter continuity.
4. Reattach a fresh client and inspect status and logs.

Pass when PTYs and the listener survive, every pane has one owner, input remains live, output is not duplicated, and no stale socket or surprise restart appears.

## M07: Real agent lifecycle

1. Install the candidate Grok Build integration through Settings and confirm its status is current.
2. Launch real Grok Build and exercise prompt submission, a tool call, a permission or elicitation block, compaction, a subagent, stop or idle, and session end.
3. Verify Oh My Herdr's working, blocked, idle, and release transitions. Verify child completion never idles or releases the parent and the pane is never labeled as another agent.
4. Restart or restore and verify native Grok session continuity.
5. Repeat the core working, blocked, and idle path with an established direct integration such as Claude Code or Codex.
6. Uninstall Grok and verify manifest detection remains a usable fallback and missing-integration guidance is accurate.

Pass when state matches the visible agent, identity remains stable, restore works, and install or uninstall changes only Oh My Herdr-owned integration files.

## M08: Remote attach and bootstrap

1. Attach to a clean Linux host over SSH with no running Oh My Herdr server and exercise bootstrap.
2. Create a pane workload, interrupt the SSH connection, and reconnect.
3. Repeat with an older remote Oh My Herdr binary to exercise the compatibility and restart prompt.
4. Verify resize, keyboard input, direct terminal attach, and clipboard behavior supported by the client and host pair.

Pass when prompts are accurate, no silent destructive restart occurs, workloads survive transport loss, and reconnect selects the intended session.

## M09: Downloaded release artifacts

Use downloaded release artifacts rather than local Cargo builds.

1. On macOS arm64, Linux x86_64, and Windows x86_64, verify the filename and checksum, executable launch, `--version`, status, first server start, and interactive shell input.
2. Exercise create, split, detach, and reattach once on each platform.
3. On Windows, use a real ConPTY terminal and verify resize, modified keys, paste, and clean shutdown.
4. Smoke the macOS x86_64 and Linux aarch64 artifacts on native hardware or supported emulation when available.

Pass when the version matches the tag, no runtime dependency is missing, and the core interaction path works on each required platform.

## M10: Host bridges

Exercise OSC 52 text copy, image paste, URL opening, terminal toast, system notification, default and custom sounds, and missing-helper fallback on each applicable OS.

Pass when each enabled bridge reaches the host once, disabled or missing helpers fail safely, and remote panes do not write to the wrong clipboard.

## M11: Mouse, responsive UI, and external tools

1. Drag workspace or group rows, tabs, and pane borders; scroll every list and modal; test context menus and inline close controls.
2. Exercise the compact layout at narrow widths.
3. Discover, rerun, and stop a project command; focus a real port owner; and open the configured Git diff tool.

Pass when drop targets and hit areas match their visuals, compact layouts retain required controls, reruns do not duplicate managed commands, and port focus selects the owning pane.

## M12: Sleep, wake, and recovery

1. Leave a counter and listener active, sleep and wake macOS, then reattach.
2. Abruptly kill a client during resize or input, relaunch, and verify stale state converges.
3. Repeat after restarting the terminal application.

Pass when the server and workloads survive, sockets recover, no stuck mouse or input mode remains, and no manual state-file cleanup is required.

## Release gate

A release is manually cleared when:

- M01-M08 pass against the release candidate, including M08 against a real Linux SSH host
- M09 passes against the published macOS arm64, Linux x86_64, and Windows x86_64 artifacts
- the published macOS x86_64 and Linux aarch64 artifacts launch and report the correct version on native hardware or supported emulation
- no unresolved failure risks data or process loss, wrong input targeting, unsafe destructive action, unusable rendering, broken restore, or release artifact startup
- every P1 failure has a linked issue and an explicit ship or no-ship decision

After preserving evidence, remove QA sessions, integrations, and remote test state.
