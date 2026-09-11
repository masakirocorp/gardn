# Manual QA matrix

Use this guide to identify the release claim owner and run checks that require a real terminal, provider, remote host, or published artifact. Do not repeat deterministic checks when the required workflows passed for the candidate commit.

Run the applicable residual M01, M02, M04, M05, and M07 checks before tagging. Run M08 before tagging and M09 after publication. Run M10 through M12 when the release changes those areas.

## Release record

Record these facts for each candidate:

- the commit SHA and Gardn version
- the successful CI and Agent Fixture Tests run URLs whose `head_sha` equals the candidate SHA
- the successful trusted Live Agent Tests run URL whose `head_sha` equals the candidate SHA
- each selected manual check, its environment, and its `PASS`, `FAIL`, or `BLOCKED` result
- each artifact filename and checksum
- each linked defect

For a presentation failure, keep a screenshot or short recording. For another failure, keep the relevant Gardn logs and exact reproduction steps.

## M01 through M07 claim ownership

Each row is a `ReleaseQaClaim` with four fields. `milestone` is M01 through M07. `claim` is one user-visible behavior. `owner` is exactly `required-ci`, `trusted-canary`, or `manual-gui`. `evidence` names the exact workflow job and test selector, canary target, or manual procedure.

| milestone | claim | owner | evidence |
| --- | --- | --- | --- |
| M01 | **M01-CI.** Supported layouts, menus, dialogs, focus states, and hit targets remain usable at tested terminal sizes. | required-ci | `CI / check (ubuntu-latest, macos-26)` via `pnpm turbo run ci:test --filter=gardn`; `ui::mobile::tests::mobile_group_dropdown_uses_compact_rows_counts_and_visible_separator`, `ui::command_palette::tests::command_palette_renders_one_close_affordance_and_run_action`, `ui::dialogs::tests::confirm_close_overlay_renders_empty_workspace`, and `ui::git_repo_picker::tests::git_repo_picker_hit_test_uses_rendered_repo_row` |
| M01 | **M01-GUI.** First launch, focus, hover, and live resizing render correctly in real terminals. | manual-gui | [M01-GUI procedure](#m01-gui-first-launch-and-core-tui) |
| M02 | **M02-CI.** Keyboard and mouse protocol input preserves literal keys, modifiers, event types, text, buttons, and coordinates. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `input::model::tests::keyboard_enhancement_flags_stay_ime_compatible`, `input::parse::tests::parse_kitty_sequence_with_associated_emoji_text`, `input::encode::tests::kitty_ctrl_slash_and_ctrl_underscore_remain_distinct`, and `input::encode::tests::sgr_mouse_scroll_encodes_wheel_button_and_coordinates` |
| M02 | **M02-CI.** CJK text and Kitty graphics state preserve their deterministic cell and image data. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `ui::tabs::tests::tab_bar_renders_trailing_cjk_character`, `ghostty::tests::kitty_image_fingerprint_covers_full_payload`, and `ghostty::tests::kitty_image_fingerprint_refreshes_on_retransmission` |
| M02 | **M02-GUI-INPUT.** macOS IME composition, CJK, emoji, and combining text align in a real terminal. | manual-gui | [M02-GUI input procedure](#m02-gui-terminal-input-and-output) |
| M02 | **M02-GUI-OUTPUT.** A real mouse-reporting app, an OSC 8 link, and a Kitty image present and clear correctly. | manual-gui | [M02-GUI output procedure](#m02-gui-terminal-input-and-output) |
| M03 | **M03-CI.** Detach, abrupt client loss, reattach, workload continuity, and named-session isolation work through public process and socket boundaries. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `detach_reattach::processes_survive_during_and_after_detach`, `detach_reattach::output_accumulated_while_detached_visible_on_reattach`, `server_headless::server_persists_after_client_disconnect`, and `cli_wrapper::named_sessions_use_separate_servers_and_workspace_state` |
| M04 | **M04-CI.** Watchers cannot send pane input or resize until explicit takeover transfers input authority and PTY geometry. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `multi_client::multi_client_explicit_takeover_transfers_geometry_and_input_authority` |
| M04 | **M04-CI.** Watcher focus, scrolling, search, and copy-mode state remain local to the invoking client. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `app::tests::watcher_focus_navigation_reveals_an_offscreen_canonical_pane`, `app::tests::route_client_events_for_view_mouse_wheel_scrolls_sidebar_and_terminal_client_locally`, `app::tests::route_client_events_for_view_pastes_navigator_search_only_into_invoking_client_view`, and `app::tests::eng57_client_copy_mode_escape_exits_client_opened_copy_mode` |
| M04 | **M04-CI.** A controller disconnect leaves the watcher unpromoted, direct terminal attach rejects a second owner without takeover, and explicit takeover replaces the owner. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `multi_client::multi_client_controller_disconnect_leaves_watcher_free_without_promotion` and `server::headless::tests::terminal_attach_requires_explicit_takeover` |
| M04 | **M04-CI.** Local API focus uses explicit tab identity and does not expose a transient takeover state on the foreground client. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `server::headless::tests::api_tab_focus_does_not_paint_take_control_on_the_foreground_client` |
| M04 | **M04-GUI.** Different terminal windows crop or pad the controller canvas without a visual layout shift, and both app layouts expose the persistent **Take control** action. | manual-gui | [M04-GUI procedure](#m04-gui-two-live-app-clients) |
| M05 | **M05-CI.** A clean cold restart restores labels, active targets, pane layout, zoom, cwd, history, and one persisted agent identity without automatic agent resume. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `cli_wrapper::server_stop_then_restart_restores_rich_session` |
| M05 | **M05-CI.** Session snapshots preserve group name, icon, accent, membership, and filter state. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `persist::snapshot::tests::round_trip_groups_and_workspace_membership` |
| M05 | **M05-GUI.** Restored focus is visually coherent when the release changes restore presentation. | manual-gui | [M05-GUI procedure](#m05-gui-restore-focus) |
| M06 | **M06-CI.** Live handoff preserves one PTY master per pane, process input and output, HTTP listeners, named sockets, client handshake, and rollback. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `live_handoff::live_server_holds_one_pty_master_fd_per_pane`, `live_handoff::live_handoff_preserves_pane_process_io`, `live_handoff::live_handoff_preserves_python_http_server`, and `live_handoff::live_handoff_bad_expected_protocol_rolls_back_old_server` |
| M07 | **M07-CI.** Installed Grok hooks map parent lifecycle events and suppress child idle and release events. | required-ci | `CI / check (ubuntu-latest, macos-26)`; `integration::tests::install_and_uninstall_grok_manage_lifecycle_hooks` and `integration::tests::grok_hook_reports_parent_lifecycle_and_ignores_child_completion` |
| M07 | **M07-FIXTURES.** The pinned agent cohort and deterministic provider fixtures report the expected lifecycle states without provider credentials. | required-ci | `Agent Fixture Tests / checks`; `node --test ci/agent-tests/deterministic-provider.test.mjs` and both `ci/agent-tests/pi-omp-plugin-status-test.mjs` targets in `.github/workflows/agent-tests.yml` |
| M07 | **M07-CANARY.** One established direct integration completes a real provider turn and exercises its installed blocked-state hook seam at the candidate SHA. | trusted-canary | A successful `Live Agent Tests` run for target `claude` or `codex`, linked in the release record with `head_sha` equal to the candidate SHA |
| M07 | **M07-GROK.** The candidate integration works with an authenticated real Grok Build installation. | manual-gui | [M07-GROK procedure](#m07-grok-real-agent-lifecycle) |

M03 and M06 have no manual residual. Their deterministic claims are complete when the required workflow evidence passes at the candidate SHA.

## M01-GUI: First launch and core TUI

1. Use an isolated `gardn-dev` configuration or a disposable OS user.
2. Launch in Ghostty with no server. Complete onboarding and confirm that the first shell is usable.
3. Open the sidebar, global menu, command palette, Settings, help, and one destructive confirmation dialog.
4. Check keyboard focus, mouse hover, and visible hit targets.
5. Resize from wide to approximately `60x20`, then return to wide.
6. Repeat the presentation check in one non-Kitty terminal.

Pass when no control becomes inaccessible or misleading, no stale hover or focus remains, and the layout stays coherent.

## M02-GUI: Terminal input and output

1. On macOS, compose a phrase with an IME in a shell and an editor.
2. Type CJK, emoji, and combining characters. Confirm that the cursor and neighboring cells remain aligned.
3. Run a mouse-reporting application. Check normal mouse handling and configured right-click passthrough.
4. Open an OSC 8 hyperlink with the terminal's real pointer interaction.
5. Display and clear a Kitty image in a compatible terminal.

Pass when composition loses no text, visual cell alignment remains correct, mouse input reaches the intended target, the link opens, and the image paints and clears.

## M04-GUI: Two live app clients

1. Attach a wide desktop terminal and a materially narrower terminal to the same tab.
2. Keep one client as the watcher. Confirm that its viewport crops or pads the controller-sized canvas without moving the layout.
3. Confirm that both desktop and mobile layouts show the persistent **Take control** action.
4. Use **Take control** from the watcher. Confirm that the transition is visible and the canvas changes to the new controller's dimensions.

Pass when the watcher presentation remains stable before takeover and only explicit takeover changes the controller presentation.

## M05-GUI: Restore focus

Run this check only when the release changes restore presentation or focus rendering.

1. Stop a session with a non-default active tab and pane.
2. Restart the session.
3. Confirm that the visible active tab, pane border, and cursor focus agree.

Pass when Gardn shows one unambiguous restored focus target.

## M07-GROK: Real agent lifecycle

The pinned agent fixture image does not install Grok from a mutable network installer. Use an existing authenticated Grok Build installation.

1. Install the candidate Grok integration through Settings and confirm that its status is current.
2. Submit a prompt, run a tool, trigger a permission or elicitation block, compact, run a subagent, reach idle, and end the session.
3. Confirm that Gardn shows the parent as working, blocked, idle, and released at the matching times.
4. Confirm that child completion never idles or releases the parent.
5. Restart or restore Gardn and confirm that the native Grok session identity remains available.
6. Uninstall the integration and confirm that Gardn removes only its own hook files.

Pass when state follows the visible parent agent, identity remains stable, restore works, and install or uninstall changes only Gardn-owned files.

## M08: Remote attach and managed worker lifecycle

1. Attach to a clean Linux host over SSH with no running Gardn server and exercise standalone bootstrap.
2. Create a pane workload, interrupt the SSH connection, and reconnect.
3. Repeat with an older remote Gardn binary to exercise the standalone compatibility and restart prompt.
4. Verify resize, keyboard input, direct terminal attach, and clipboard behavior supported by the client and host pair.
5. Save the same host in **Settings > Connections**. Connect without a manual worker install and verify that Gardn installs the current managed worker.
6. Stop and restart the local coordinator while the remote terminal remains active. Verify that the saved connection reconnects without a manual **Connect** action and that the pane renders the preserved terminal output.
7. Keep a remote terminal active, connect with a newer compatible worker version, and verify that the active runtime is not interrupted. End the runtime and verify that the deferred worker update activates.
8. Reference the connection from two named local sessions. Start removal, verify that inventory lists both sessions and owned bindings, then confirm. Interrupt one removal after approval and restart the server to verify journal recovery.
9. Repeat with the remote host unavailable. Verify that full removal fails closed. Verify that the failure screen identifies the connection, warns that remote processes or files might remain, and offers **Remove Saved Connection**, **Try Again**, and **Cancel**.

Pass when prompts are accurate, transport loss and coordinator restart do not lose workloads, restored remote panes reconnect automatically, updates do not interrupt compatible live runtimes, retirement removes only Gardn-owned state, and an approved partial retirement resumes after restart.

## M09: Downloaded release artifacts

Use downloaded release artifacts, not local Cargo builds.

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
3. Discover, rerun, and stop a project command. Focus a real port owner.
4. Verify that **Settings > Commands** contains only Browser, Review, and Editor. Reset them and confirm `terminal-browser`, `hunk diff --watch`, and `fresh .`. Open each from the command palette and workspace menus.
5. With an authenticated `gh` CLI, open GitHub from the command palette and workspace menu. Confirm that it opens a native screen without creating a companion terminal pane. Open GitHub in a second app client and confirm that navigation in one client does not move the other.
6. In a Space with local GitHub checkouts, open GitHub with Automatic scope. Confirm that results stay within the discovered repositories. Narrow to one repository, then remove the narrowing. Confirm that the original scope returns. In a Space without discovered repositories, check the Group organization fallback and, without a Group organization, the signed-in user's queues.
7. Save exact repositories in **Space Settings > GitHub** with Enter. Reopen Settings and verify persistence. Confirm that changing scope closes the invalidated GitHub view. Reopen GitHub and check the saved scope. Repeat with Group organization mode. Confirm that repository narrowing never exposes an outside-scope repository.
8. Open Overview, pull requests, issues, and Actions. Apply a filter and confirm that it filters loaded results rather than claiming a complete server search. Use **More** to load another page. Inspect an Actions run, its jobs and steps, and a log link.
9. In a disposable repository, read a conversation, add a comment, edit your own comment, change labels, and close an issue. Exercise pull request draft state, file navigation, split and unified diffs, wrapping, whitespace controls, and an inline range review. Check safe merge, auto-merge, and queue actions only on disposable pull requests with the required repository settings.
10. Check native GitHub keyboard and mouse navigation, the active Gardn theme, scrollbars, and narrow-terminal rendering. Confirm that no companion installation, version, config, or theme setup is required. Confirm that admin merge, branch deletion, outside-scope browsing, review Space creation, agent handoffs, and a matching-Space action are absent.

Pass when hit areas match their visuals, compact layouts retain required controls, reruns reuse managed command tabs, and port focus selects the owning pane. GitHub must keep client navigation independent, enforce the configured Space scope, paginate explicitly, and apply mutations only to the selected target.

## M12: Sleep, wake, and recovery

1. Leave a counter and listener active, sleep and wake macOS, then reattach.
2. Abruptly kill a client during resize or input, relaunch, and verify stale state converges.
3. Repeat after restarting the terminal application.

Pass when the server and workloads survive, sockets recover, no stuck mouse or input mode remains, and no manual state-file cleanup is required.

## Release gate

Before tagging, require all of the following evidence against the exact candidate SHA:

- successful CI and Agent Fixture Tests workflow runs
- a linked successful trusted canary run
- every applicable `manual-gui` claim
- M08 against a real Linux SSH host
- each selected M10 through M12 check
- no unresolved failure that risks data, process continuity, input targeting, destructive actions, restore, or release startup

The tag-triggered Release workflow reruns CI and Agent Fixture Tests as local reusable workflows at the tag SHA. The publication job waits for both. The Release workflow does not run or enforce the trusted canary.

After publication, run M09 against the downloaded artifacts. Record the artifact checksums and M09 results before clearing the release.

After preserving evidence, remove QA sessions, integrations, and remote test state.
