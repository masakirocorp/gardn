use std::time::Instant;

use crate::app::state::{ToastKind, ToastNotification, ToastTarget};

use super::{App, ClientViewState};

struct PendingAgentResumeCandidate {
    pane_id: crate::layout::PaneId,
    terminal_id: crate::terminal::TerminalId,
    location: crate::execution_host::ResourceLocation,
    plan: crate::agent_resume::AgentResumePlan,
    rows: u16,
    cols: u16,
}

impl App {
    fn has_pending_agent_resume_pane_without_runtime(&self) -> bool {
        self.state.workspaces.iter().any(|workspace| {
            workspace.tabs.iter().any(|tab| {
                tab.panes.values().any(|pane| {
                    self.terminal_runtimes
                        .get(&pane.attached_terminal_id)
                        .is_none()
                        && self
                            .state
                            .terminals
                            .get(&pane.attached_terminal_id)
                            .is_some_and(|terminal| terminal.pending_agent_resume_plan.is_some())
                })
            })
        })
    }

    pub(crate) fn sync_pending_agent_resume_deadline(&mut self, now: Instant) {
        if !self.has_pending_agent_resume_pane_without_runtime() {
            self.pending_agent_resume_deadline = None;
            return;
        }
        self.pending_agent_resume_deadline
            .get_or_insert(now + super::PENDING_AGENT_RESUME_THEME_WAIT);
    }

    pub(crate) fn pending_agent_resume_due(&self, now: Instant) -> bool {
        self.pending_agent_resume_deadline
            .is_some_and(|deadline| now >= deadline)
    }

    pub(crate) fn start_pending_agent_resumes(&mut self, allow_empty_theme: bool) -> bool {
        let pending = self.pending_agent_resume_candidates();
        let changed = self.start_pending_agent_resume_candidates(pending, allow_empty_theme);

        if changed {
            self.schedule_session_save();
        }
        if !self.has_pending_agent_resume_pane_without_runtime() {
            self.pending_agent_resume_deadline = None;
        } else if self.pending_agent_resume_candidates().is_empty() {
            self.pending_agent_resume_deadline =
                Some(Instant::now() + super::PENDING_AGENT_RESUME_THEME_WAIT);
        }
        changed
    }

    pub(crate) fn start_pending_agent_resumes_for_client_view(
        &mut self,
        view: &ClientViewState,
        allow_empty_theme: bool,
    ) -> bool {
        let pending = self.pending_agent_resume_candidates_for_client_view(view);
        let changed = self.start_pending_agent_resume_candidates(pending, allow_empty_theme);

        if changed {
            self.schedule_session_save();
        }
        if !self.has_pending_agent_resume_pane_without_runtime() {
            self.pending_agent_resume_deadline = None;
        } else if self
            .pending_agent_resume_candidates_for_client_view(view)
            .is_empty()
        {
            self.pending_agent_resume_deadline =
                Some(Instant::now() + super::PENDING_AGENT_RESUME_THEME_WAIT);
        }
        changed
    }

    fn start_pending_agent_resume_candidates(
        &mut self,
        pending: Vec<PendingAgentResumeCandidate>,
        allow_empty_theme: bool,
    ) -> bool {
        let mut changed = false;
        for PendingAgentResumeCandidate {
            pane_id,
            terminal_id,
            location,
            plan,
            rows,
            cols,
        } in pending
        {
            if self.terminal_runtimes.get(&terminal_id).is_some() {
                continue;
            }
            changed |= self.start_pending_agent_resume(
                pane_id,
                terminal_id,
                location,
                plan,
                rows,
                cols,
                allow_empty_theme,
            );
        }

        changed
    }

    fn pending_agent_resume_candidates(&self) -> Vec<PendingAgentResumeCandidate> {
        let Some(ws_idx) = self.state.active else {
            return Vec::new();
        };
        let Some(ws) = self.state.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let Some(tab) = ws.tabs.get(ws.active_tab) else {
            return Vec::new();
        };
        self.pending_agent_resume_candidates_for_tab(tab, &self.state.view.pane_infos)
    }

    fn pending_agent_resume_candidates_for_client_view(
        &self,
        view: &ClientViewState,
    ) -> Vec<PendingAgentResumeCandidate> {
        let Some(ws_idx) = view.active_workspace else {
            return Vec::new();
        };
        let Some(ws) = self.state.workspaces.get(ws_idx) else {
            return Vec::new();
        };
        let tab_idx = view
            .active_tab_for_workspace(&ws.id)
            .unwrap_or(ws.active_tab);
        let Some(tab) = ws.tabs.get(tab_idx) else {
            return Vec::new();
        };
        self.pending_agent_resume_candidates_for_tab(tab, &view.computed.pane_infos)
    }

    fn pending_agent_resume_candidates_for_tab(
        &self,
        tab: &crate::workspace::Tab,
        pane_infos: &[crate::layout::PaneInfo],
    ) -> Vec<PendingAgentResumeCandidate> {
        let mut pending = Vec::new();
        for pane_id in tab.layout.pane_ids() {
            let Some(pane) = tab.panes.get(&pane_id) else {
                continue;
            };
            if self
                .terminal_runtimes
                .get(&pane.attached_terminal_id)
                .is_some()
            {
                continue;
            }
            let Some(info) = pane_infos.iter().find(|info| info.id == pane_id) else {
                continue;
            };
            let Some(terminal) = self.state.terminals.get(&pane.attached_terminal_id) else {
                continue;
            };
            let Some(plan) = terminal.pending_agent_resume_plan.clone() else {
                continue;
            };
            pending.push(PendingAgentResumeCandidate {
                pane_id,
                terminal_id: pane.attached_terminal_id.clone(),
                location: terminal.location.clone(),
                plan,
                rows: info.inner_rect.height,
                cols: info.inner_rect.width,
            });
        }
        pending
    }

    fn start_pending_agent_resume(
        &mut self,
        pane_id: crate::layout::PaneId,
        terminal_id: crate::terminal::TerminalId,
        location: crate::execution_host::ResourceLocation,
        plan: crate::agent_resume::AgentResumePlan,
        rows: u16,
        cols: u16,
        allow_empty_theme: bool,
    ) -> bool {
        let host_terminal_theme = self.state.host_terminal_theme;
        if host_terminal_theme.is_empty() && !allow_empty_theme {
            return false;
        }

        let Some(resume_command) = shell_command_from_plan(&plan) else {
            tracing::warn!(
                pane = pane_id.raw(),
                terminal = %terminal_id,
                agent = %plan.agent,
                "failed to start deferred agent resume with empty argv"
            );
            self.notify_agent_restore_failed(pane_id, &plan, "restore command missing", None);
            if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                terminal.clear_agent_runtime_identity_after_respawn();
            }
            return true;
        };
        let runtime = if location.is_local() {
            let Some(launch_env) = self
                .find_pane(pane_id)
                .and_then(|(ws_idx, _)| self.pane_launch_env(ws_idx, pane_id, plan.env.clone()))
            else {
                return false;
            };
            let cwd = location.path.as_path().to_path_buf();
            let shell_config =
                crate::pane::PaneShellConfig::new(&self.state.default_shell, self.state.shell_mode);
            match plan.command_resolution {
                crate::agent_resume::AgentResumeCommandResolution::External => {
                    crate::terminal::TerminalRuntime::spawn_argv_command(
                        pane_id,
                        rows,
                        cols,
                        cwd,
                        &plan.argv,
                        &launch_env,
                        self.state.pane_scrollback_limit_bytes,
                        host_terminal_theme,
                        self.event_tx.clone(),
                        self.render_notify.clone(),
                        self.render_dirty.clone(),
                    )
                }
                crate::agent_resume::AgentResumeCommandResolution::ShellWrapper => {
                    crate::terminal::TerminalRuntime::spawn_profile_command(
                        pane_id,
                        rows,
                        cols,
                        cwd,
                        shell_config,
                        &resume_command,
                        &launch_env,
                        self.state.pane_scrollback_limit_bytes,
                        host_terminal_theme,
                        self.event_tx.clone(),
                        self.render_notify.clone(),
                        self.render_dirty.clone(),
                    )
                }
            }
            .map_err(|error| error.to_string())
        } else {
            let Some(command) = remote_resume_command_spec(&plan, &resume_command) else {
                self.notify_agent_restore_failed(
                    pane_id,
                    &plan,
                    "restore command missing",
                    Some(&resume_command),
                );
                if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                    terminal.clear_agent_runtime_identity_after_respawn();
                }
                return true;
            };
            let Some(hosts) = self.execution_hosts.as_mut() else {
                self.notify_agent_restore_failed(
                    pane_id,
                    &plan,
                    "execution host manager unavailable",
                    Some(&resume_command),
                );
                return true;
            };
            hosts.create_terminal(
                terminal_id.clone(),
                pane_id,
                location,
                rows,
                cols,
                self.state.pane_scrollback_limit_bytes,
                host_terminal_theme,
                self.event_tx.clone(),
                Some(command),
                plan.env.clone(),
            )
        };
        let runtime = match runtime {
            Ok(runtime) => runtime,
            Err(err) => {
                tracing::warn!(
                    pane = pane_id.raw(),
                    terminal = %terminal_id,
                    host = %self.state.terminals.get(&terminal_id)
                        .map(|terminal| terminal.location.execution_host_id.as_str())
                        .unwrap_or("unknown"),
                    agent = %plan.agent,
                    err = %err,
                    "failed to launch deferred agent resume"
                );
                self.notify_agent_restore_failed(
                    pane_id,
                    &plan,
                    "restore launch failed",
                    Some(&resume_command),
                );
                if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                    terminal.clear_agent_runtime_identity_after_respawn();
                }
                return true;
            }
        };

        self.terminal_runtimes.insert(terminal_id.clone(), runtime);
        if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
            terminal.pending_agent_resume_plan = None;
            terminal.launch_env = plan.env;
            terminal.respawn_shell_on_exit = true;
        }
        true
    }

    fn notify_agent_restore_failed(
        &mut self,
        pane_id: crate::layout::PaneId,
        plan: &crate::agent_resume::AgentResumePlan,
        reason: &str,
        command: Option<&str>,
    ) {
        let target = self.find_pane(pane_id).map(|(ws_idx, _)| ToastTarget {
            workspace_id: self.public_workspace_id(ws_idx),
            pane_id,
        });
        let agent_name = crate::agent_profiles::AgentKind::ALL
            .iter()
            .find(|kind| kind.as_str() == plan.agent)
            .map(|kind| kind.display_name())
            .unwrap_or(&plan.agent);
        self.state.toast = Some(ToastNotification {
            kind: ToastKind::NeedsAttention,
            title: format!("Couldn't Restore {agent_name} Session"),
            context: restore_failure_context(reason, command),
            position: None,
            target,
        });
    }
}

fn remote_resume_command_spec(
    plan: &crate::agent_resume::AgentResumePlan,
    shell_command: &str,
) -> Option<crate::execution_host::protocol::CommandSpec> {
    match plan.command_resolution {
        crate::agent_resume::AgentResumeCommandResolution::External => {
            let (program, args) = plan.argv.split_first()?;
            Some(crate::execution_host::protocol::CommandSpec {
                program: program.clone(),
                args: args.to_vec(),
                env: Vec::new(),
            })
        }
        crate::agent_resume::AgentResumeCommandResolution::ShellWrapper => {
            // Execution Worker Protocol v1 has no coordinator profile catalog.
            // Resolve saved relative commands in the worker's POSIX shell instead
            // of consulting the coordinator host or falling back to Local.
            Some(crate::execution_host::protocol::CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-lc".to_string(), shell_command.to_string()],
                env: Vec::new(),
            })
        }
    }
}

fn shell_command_from_plan(plan: &crate::agent_resume::AgentResumePlan) -> Option<String> {
    let command = shell_command_from_argv(&plan.argv)?;
    if plan.env.is_empty() {
        return Some(command);
    }

    let mut prefixed = String::new();
    for (idx, (key, value)) in plan.env.iter().enumerate() {
        if idx > 0 {
            prefixed.push(' ');
        }
        prefixed.push_str(key);
        prefixed.push('=');
        prefixed.push_str(&shell_quote(value));
    }
    prefixed.push(' ');
    prefixed.push_str(&command);
    Some(prefixed)
}

fn shell_command_from_argv(argv: &[String]) -> Option<String> {
    let mut parts = argv.iter();
    let first = shell_quote(parts.next()?);
    let mut command = first;
    for part in parts {
        command.push(' ');
        command.push_str(&shell_quote(part));
    }
    Some(command)
}

const MAX_RESTORE_FAILURE_CONTEXT_LEN: usize = 110;

fn restore_failure_context(reason: &str, command: Option<&str>) -> String {
    let Some(command) = command else {
        return format!("{reason}; resume manually");
    };
    let context = format!("{reason}; manual: {command}");
    truncate_context(context)
}

fn truncate_context(mut context: String) -> String {
    if context.len() <= MAX_RESTORE_FAILURE_CONTEXT_LEN {
        return context;
    }
    let mut end = MAX_RESTORE_FAILURE_CONTEXT_LEN.saturating_sub(3);
    while end > 0 && !context.is_char_boundary(end) {
        end -= 1;
    }
    context.truncate(end);
    context.push_str("...");
    context
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'_' | b'-' | b'.' | b'/' | b':' | b'@' | b'%' | b'+' | b'='
            )
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    #[cfg(unix)]
    fn temp_restore_dir(test_name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test clock should be after epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("gardn-{test_name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp restore dir should be created");
        dir
    }

    #[cfg(unix)]
    fn recording_agent_script(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join(name);
        let output = dir.join(format!("{name}.argv"));
        let script = format!(
            "#!/bin/sh\n{{\n  printf '%s\\n' \"$0\"\n  for arg in \"$@\"; do\n    printf '%s\\n' \"$arg\"\n  done\n}} > '{}'\n",
            output.display()
        );
        std::fs::write(&path, script).expect("recording wrapper should be written");
        let mut perms = std::fs::metadata(&path)
            .expect("recording wrapper should have metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("recording wrapper should be executable");
        path
    }

    #[cfg(unix)]
    async fn run_pending_resume_and_read_recorded_argv(
        command: std::path::PathBuf,
        args: &[&str],
        output: &std::path::Path,
    ) -> (Vec<String>, Option<Vec<String>>) {
        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("restored");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        app.state.view.pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.default_shell = "/bin/sh".to_string();
        app.state.shell_mode = crate::config::ShellModeConfig::NonLogin;
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };

        let mut argv = vec![command.to_string_lossy().to_string()];
        argv.extend(args.iter().map(|arg| (*arg).to_string()));
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "test-agent".into(),
            argv,
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: Vec::new(),
            dedupe_key: format!("test\0{}", output.display()),
        });

        assert!(app.start_pending_agent_resumes(false));
        let expected_line_count = args.len() + 1;
        for _ in 0..100 {
            if let Ok(recorded) = std::fs::read_to_string(output) {
                if recorded.lines().count() == expected_line_count {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let recorded = std::fs::read_to_string(output).unwrap_or_else(|err| {
            panic!(
                "expected restored command to write argv file {}: {err}",
                output.display()
            )
        });
        let launch_argv = app
            .state
            .terminals
            .get(&terminal_id)
            .expect("terminal should survive launch")
            .launch_argv
            .clone();

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        (recorded.lines().map(str::to_string).collect(), launch_argv)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pending_agent_resume_executes_every_supported_restore_argv_shape() {
        let dir = temp_restore_dir("restore-argv");
        let cases: Vec<(&str, Vec<&str>)> = vec![
            ("custom-claude", vec!["--resume", "claude-session"]),
            ("custom-codex", vec!["resume", "codex-session"]),
            ("custom-copilot", vec!["--resume=copilot-session"]),
            ("custom-hermes", vec!["--resume", "hermes-session"]),
            ("custom-opencode", vec!["--session", "opencode-session"]),
            ("custom-pi", vec!["--session", "/tmp/pi-session.jsonl"]),
            (
                "custom-omp",
                vec![
                    "--resume",
                    "/tmp/parent/RightSidebarHierarchyReview.jsonl",
                    "--session-dir",
                    "/tmp/parent",
                ],
            ),
        ];

        for (command_name, expected_args) in cases {
            let command = recording_agent_script(&dir, command_name);
            let output = dir.join(format!("{command_name}.argv"));

            let (recorded, launch_argv) =
                run_pending_resume_and_read_recorded_argv(command.clone(), &expected_args, &output)
                    .await;
            let mut expected = vec![command.to_string_lossy().to_string()];
            expected.extend(expected_args.iter().map(|arg| (*arg).to_string()));

            assert_eq!(
                recorded, expected,
                "{command_name} restore argv should execute exactly"
            );
            assert_eq!(
                launch_argv, None,
                "{command_name} generated restore command must not become saved launch context"
            );
        }

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pending_agent_resume_executes_profile_environment() {
        let dir = temp_restore_dir("restore-env");
        let output = dir.join("profile-env.txt");
        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("restored");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        app.state.view.pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.default_shell = "/bin/sh".to_string();
        app.state.shell_mode = crate::config::ShellModeConfig::NonLogin;
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                format!("printf %s \"$CODEX_HOME\" > '{}'", output.display()),
            ],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: vec![("CODEX_HOME".into(), "/profiles/manual-codex".into())],
            dedupe_key: format!("gardn:codex\0codex\0Id\0{}", output.display()),
        });

        assert!(app.start_pending_agent_resumes(false));
        for _ in 0..40 {
            if output.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        assert_eq!(
            std::fs::read_to_string(&output).expect("profile env should be recorded"),
            "/profiles/manual-codex"
        );
        assert_eq!(
            app.state
                .terminals
                .get(&terminal_id)
                .expect("terminal should survive")
                .launch_env,
            vec![("CODEX_HOME".into(), "/profiles/manual-codex".into())]
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn restored_omp_profile_bypasses_conflicting_default_wrapper() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_restore_dir("restore-omp-profile");
        let home = dir.join("home");
        let session_dir = home.join(".omp-mk/agent/sessions/-projects-masakiro-gardn");
        std::fs::create_dir_all(&session_dir).expect("OMP session directory should be created");
        let session_path = session_dir.join("session.jsonl");
        std::fs::write(&session_path, b"session").expect("OMP session should exist");
        let output = dir.join("omp-mk.txt");
        let hostile_sentinel = dir.join("hostile-omp.txt");
        let wrapper = dir.join("omp-mk");
        let wrapper_script = format!(
            "#!/bin/sh\n\
             {{ printf '%s\\n' \"$PI_CONFIG_DIR\" \"$PI_CODING_AGENT_DIR\"; \
             for arg in \"$@\"; do printf '%s\\n' \"$arg\"; done; }} > '{}'\n",
            output.display(),
        );
        std::fs::write(&wrapper, wrapper_script).expect("OMP wrapper should be written");
        let mut wrapper_permissions = std::fs::metadata(&wrapper)
            .expect("OMP wrapper should have metadata")
            .permissions();
        wrapper_permissions.set_mode(0o755);
        std::fs::set_permissions(&wrapper, wrapper_permissions)
            .expect("OMP wrapper should be executable");

        let shell = dir.join("profile-shell");
        let shell_script = format!(
            "#!/bin/sh\n\
             PATH='{}':$PATH\n\
             export PATH\n\
             omp() {{ printf hostile > '{}'; }}\n\
             eval \"$2\"\n",
            dir.display(),
            hostile_sentinel.display(),
        );
        std::fs::write(&shell, shell_script).expect("profile shell should be written");
        let mut shell_permissions = std::fs::metadata(&shell)
            .expect("profile shell should have metadata")
            .permissions();
        shell_permissions.set_mode(0o755);
        std::fs::set_permissions(&shell, shell_permissions)
            .expect("profile shell should be executable");

        let plan = {
            let _lock = crate::integration::integration_env_lock();
            let _home = crate::config::TestEnvVar::set("HOME", &home);
            crate::agent_resume::plan_with_launch_context(
                "gardn:omp",
                "omp",
                &crate::agent_resume::AgentSessionRef::path(
                    session_path.to_string_lossy().into_owned(),
                )
                .expect("OMP session path should be valid"),
                Some(&["omp".to_string()]),
                &[
                    ("PI_CONFIG_DIR".into(), ".omp".into()),
                    (
                        "PI_CODING_AGENT_DIR".into(),
                        home.join(".omp/agent").to_string_lossy().into_owned(),
                    ),
                ],
            )
            .expect("OMP restore plan should be created")
        };

        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("restored");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        app.state.view.pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.default_shell = shell.to_string_lossy().into_owned();
        app.state.shell_mode = crate::config::ShellModeConfig::Login;
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(plan);

        assert!(app.start_pending_agent_resumes(false));
        for _ in 0..40 {
            if output.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }

        let recorded =
            std::fs::read_to_string(&output).expect("omp-mk wrapper should record its launch");
        assert_eq!(
            recorded.lines().collect::<Vec<_>>(),
            [
                ".omp-mk",
                home.join(".omp-mk/agent").to_string_lossy().as_ref(),
                "--resume",
                session_path.to_string_lossy().as_ref(),
                "--session-dir",
                session_dir.to_string_lossy().as_ref(),
            ]
        );
        assert!(
            !hostile_sentinel.exists(),
            "the conflicting default omp wrapper must not run"
        );
        let terminal = app
            .state
            .terminals
            .get(&terminal_id)
            .expect("terminal should survive launch");
        assert!(terminal.launch_argv.is_none());
        assert!(terminal.respawn_shell_on_exit);

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn pending_agent_resume_failure_shows_manual_restore_toast() {
        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("restored");
        let workspace_id = workspace.id.clone();
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        app.state.view.pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: Vec::new(),
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: Vec::new(),
            dedupe_key: "gardn:codex\0codex\0Id\0codex-session".into(),
        });

        assert!(app.start_pending_agent_resumes(false));

        let toast = app.state.toast.as_ref().expect("restore failure toast");
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "Couldn't Restore Codex Session");
        assert_eq!(toast.context, "restore command missing; resume manually");
        assert_eq!(
            toast.target,
            Some(ToastTarget {
                workspace_id,
                pane_id
            })
        );
        assert!(
            app.state
                .terminals
                .get(&terminal_id)
                .expect("terminal should survive")
                .pending_agent_resume_plan
                .is_none(),
            "failed restore should not retry forever"
        );
    }

    #[tokio::test]
    async fn pending_agent_resume_waits_for_host_theme_before_launch() {
        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("restored");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        let pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.view.pane_infos = pane_infos;
        let terminal = app
            .state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist");
        terminal.pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf '%s' 'restored agent: shell quoted | marker'; sleep 5".into(),
            ],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: Vec::new(),
            dedupe_key: "gardn:codex\0codex\0Id\0codex-session".into(),
        });

        assert!(!app.start_pending_agent_resumes(false));
        assert!(app.terminal_runtimes.get(&terminal_id).is_none());

        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };

        assert!(app.start_pending_agent_resumes(false));
        assert!(app.terminal_runtimes.get(&terminal_id).is_some());
        let terminal = app
            .state
            .terminals
            .get(&terminal_id)
            .expect("terminal should survive launch");
        assert!(terminal.pending_agent_resume_plan.is_none());
        assert!(terminal.respawn_shell_on_exit);

        let runtime = app
            .terminal_runtimes
            .get(&terminal_id)
            .expect("pending resume should leave an agent runtime");
        let marker = "restored agent: shell quoted | marker";
        for _ in 0..20 {
            if runtime
                .snapshot_history()
                .is_some_and(|text| text.contains(marker))
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert!(
            runtime
                .snapshot_history()
                .expect("runtime should expose terminal history")
                .contains(marker),
            "deferred restore should execute the resume argv"
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pending_agent_resume_can_launch_after_theme_wait_expires() {
        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("restored");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        app.state.view.pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "sleep 5".into()],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: Vec::new(),
            dedupe_key: "gardn:codex\0codex\0Id\0codex-session".into(),
        });

        app.sync_pending_agent_resume_deadline(std::time::Instant::now());
        assert!(!app.start_pending_agent_resumes(false));
        assert!(app.start_pending_agent_resumes(true));
        assert!(app.terminal_runtimes.get(&terminal_id).is_some());

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pending_agent_resume_keeps_hidden_panes_scheduled_after_visible_resumes_start() {
        let mut app = test_app();
        let active_workspace = crate::workspace::Workspace::test_new("active");
        let active_pane = active_workspace.tabs[0].root_pane;
        let active_terminal = active_workspace.terminal_id(active_pane).cloned().unwrap();
        let hidden_workspace = crate::workspace::Workspace::test_new("hidden");
        let hidden_pane = hidden_workspace.tabs[0].root_pane;
        let hidden_terminal = hidden_workspace.terminal_id(hidden_pane).cloned().unwrap();
        app.state.view.pane_infos = active_workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![active_workspace, hidden_workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        for terminal_id in [&active_terminal, &hidden_terminal] {
            app.state
                .terminals
                .get_mut(terminal_id)
                .expect("test terminal should exist")
                .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
                agent: "codex".into(),
                argv: vec!["/bin/sh".into(), "-c".into(), "sleep 5".into()],
                command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
                preserved_launch_argv: None,
                env: Vec::new(),
                dedupe_key: format!("gardn:codex\0codex\0Id\0{terminal_id}"),
            });
        }
        app.pending_agent_resume_deadline =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

        assert!(app.start_pending_agent_resumes(false));
        assert!(app.terminal_runtimes.get(&active_terminal).is_some());
        assert!(app.terminal_runtimes.get(&hidden_terminal).is_none());
        assert!(
            app.pending_agent_resume_deadline.is_some(),
            "hidden pending resumes should keep a wakeup deadline active after visible resumes start"
        );
        assert!(
            app.state
                .terminals
                .get(&hidden_terminal)
                .expect("hidden terminal should still exist")
                .pending_agent_resume_plan
                .is_some(),
            "hidden restored panes should wait until their tab has computed geometry"
        );

        app.state.active = Some(1);
        let hidden_pane_infos = app.state.workspaces[1].tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.view.pane_infos = hidden_pane_infos;

        assert!(app.start_pending_agent_resumes(false));
        assert!(app.terminal_runtimes.get(&hidden_terminal).is_some());
        assert!(
            app.state
                .terminals
                .get(&hidden_terminal)
                .expect("hidden terminal should still exist")
                .pending_agent_resume_plan
                .is_none(),
            "hidden restored panes should launch after their tab gets fresh geometry"
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pending_agent_resume_keeps_hidden_panes_scheduled_when_only_stale_geometry_exists() {
        let mut app = test_app();
        let previous_workspace = crate::workspace::Workspace::test_new("previous");
        let previous_pane = previous_workspace.tabs[0].root_pane;
        let previous_terminal = previous_workspace
            .terminal_id(previous_pane)
            .cloned()
            .unwrap();
        let current_workspace = crate::workspace::Workspace::test_new("current");
        app.state.view.pane_infos = previous_workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![previous_workspace, current_workspace];
        app.state.active = Some(1);
        app.state.ensure_test_terminals();
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        app.state
            .terminals
            .get_mut(&previous_terminal)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "sleep 5".into()],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: Vec::new(),
            dedupe_key: "gardn:codex\0codex\0Id\0codex-session".into(),
        });

        app.sync_pending_agent_resume_deadline(std::time::Instant::now());
        assert!(
            app.pending_agent_resume_deadline.is_some(),
            "hidden pending resumes should stay scheduled even when only stale geometry exists"
        );
        assert!(!app.start_pending_agent_resumes(false));
        assert!(
            app.pending_agent_resume_deadline.is_some(),
            "hidden pending resumes should remain retryable after a hidden-only resume pass"
        );
        assert!(app.terminal_runtimes.get(&previous_terminal).is_none());
        assert!(
            app.state
                .terminals
                .get(&previous_terminal)
                .expect("previous terminal should still exist")
                .pending_agent_resume_plan
                .is_some(),
            "a pane hidden by navigation should wait for a fresh visible geometry snapshot"
        );

        app.state.active = Some(0);
        let previous_pane_infos = app.state.workspaces[0].tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.view.pane_infos = previous_pane_infos;

        assert!(app.start_pending_agent_resumes(false));
        assert!(app.terminal_runtimes.get(&previous_terminal).is_some());
        assert!(
            app.state
                .terminals
                .get(&previous_terminal)
                .expect("previous terminal should still exist")
                .pending_agent_resume_plan
                .is_none(),
            "a pane hidden by navigation should launch after receiving fresh visible geometry"
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pending_agent_resume_launches_with_inner_rect_size() {
        let mut app = test_app();
        let mut workspace = crate::workspace::Workspace::test_new("split");
        let pane_id = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let terminal_id = workspace.terminal_id(pane_id).cloned().unwrap();
        app.state.view.pane_infos = vec![crate::layout::PaneInfo {
            id: pane_id,
            rect: ratatui::layout::Rect::new(0, 0, 100, 30),
            inner_rect: ratatui::layout::Rect::new(1, 1, 98, 28),
            scrollbar_rect: None,
            is_focused: true,
        }];
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app.state.host_terminal_theme = crate::terminal_theme::TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 220,
                b: 220,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 20,
                g: 20,
                b: 20,
            }),
            ..crate::terminal_theme::TerminalTheme::default()
        };
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist")
            .pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec!["/bin/sh".into(), "-c".into(), "sleep 5".into()],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: Vec::new(),
            dedupe_key: "gardn:codex\0codex\0Id\0codex-session".into(),
        });

        assert!(app.start_pending_agent_resumes(false));
        assert_eq!(
            app.terminal_runtimes
                .get(&terminal_id)
                .expect("pending resume should launch")
                .current_size(),
            (28, 98)
        );

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pending_remote_agent_resume_uses_terminal_host_and_path() {
        let mut app = test_app();
        let workspace = crate::workspace::Workspace::test_new("remote-resume");
        let pane_id = workspace.tabs[0].root_pane;
        let terminal_id = workspace
            .terminal_id(pane_id)
            .cloned()
            .expect("test workspace should have a terminal");
        app.state.view.pane_infos = workspace.tabs[0]
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 100, 30));
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox")
            .expect("test host id should be valid");
        let location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/agent")
                .expect("test host path should be valid"),
        );
        let messages = app
            .execution_hosts
            .as_mut()
            .expect("test app should have an execution host manager")
            .connect_test_host(host_id.clone());
        let terminal = app
            .state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist");
        terminal.location = location;
        terminal.pending_agent_resume_plan = Some(crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec!["codex".into(), "resume".into(), "session-42".into()],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: vec![("CODEX_HOME".into(), "/srv/profile".into())],
            dedupe_key: "gardn:codex\0codex\0Id\0session-42".into(),
        });

        assert!(app.start_pending_agent_resumes(true));

        let messages = messages
            .lock()
            .expect("test worker message lock should not be poisoned");
        let [crate::execution_host::protocol::CoordinatorMessage::CreateTerminal {
            location,
            command: Some(command),
            env,
            ..
        }] = messages.as_slice()
        else {
            panic!("expected one remote resume creation message: {messages:?}");
        };
        assert_eq!(location.execution_host_id, host_id);
        assert_eq!(location.path.as_path(), std::path::Path::new("/srv/agent"));
        assert_eq!(command.program, "codex");
        assert_eq!(command.args, vec!["resume", "session-42"]);
        assert_eq!(
            env.as_slice(),
            &[("CODEX_HOME".to_string(), "/srv/profile".to_string())]
        );
        assert!(app
            .state
            .terminals
            .get(&terminal_id)
            .is_some_and(|terminal| terminal.pending_agent_resume_plan.is_none()));
    }

    #[test]
    fn shell_command_from_argv_quotes_resume_arguments() {
        let argv = vec![
            "claude".to_string(),
            "--resume".to_string(),
            "session with ' quote".to_string(),
        ];

        assert_eq!(
            shell_command_from_argv(&argv).as_deref(),
            Some("claude --resume 'session with '\\'' quote'")
        );
        assert_eq!(shell_command_from_argv(&[]), None);

        let plan = crate::agent_resume::AgentResumePlan {
            agent: "codex".into(),
            argv: vec!["codex".into(), "resume".into(), "session".into()],
            command_resolution: crate::agent_resume::AgentResumeCommandResolution::External,
            preserved_launch_argv: None,
            env: vec![("CODEX_HOME".into(), "/profiles/codex with space".into())],
            dedupe_key: "codex".into(),
        };
        assert_eq!(
            shell_command_from_plan(&plan).as_deref(),
            Some("CODEX_HOME='/profiles/codex with space' codex resume session")
        );
    }
}
