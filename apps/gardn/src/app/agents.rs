use std::path::PathBuf;

use super::{terminal_targets::TerminalTargetError, App, Mode};
use crate::api::schema::{AgentStartParams, SplitDirection};

impl App {
    pub(super) fn collect_agent_infos(&self) -> Vec<crate::api::schema::AgentInfo> {
        self.state
            .workspaces
            .iter()
            .enumerate()
            .flat_map(|(ws_idx, ws)| {
                ws.tabs.iter().flat_map(move |tab| {
                    tab.layout
                        .pane_ids()
                        .into_iter()
                        .filter_map(move |pane_id| self.agent_info(ws_idx, pane_id))
                })
            })
            .collect()
    }

    pub(super) fn agent_info_for_target(
        &self,
        target: &str,
    ) -> Result<crate::api::schema::AgentInfo, TerminalTargetError> {
        let resolved = self.resolve_agent_target(target)?;
        self.agent_info(resolved.ws_idx, resolved.pane_id)
            .ok_or_else(|| TerminalTargetError::NotFound {
                target: target.to_string(),
            })
    }

    pub(super) fn focus_agent_target(
        &mut self,
        target: &str,
    ) -> Result<crate::api::schema::AgentInfo, TerminalTargetError> {
        let resolved = self.resolve_agent_target(target)?;
        self.state
            .focus_workspace_tab_pane(resolved.ws_idx, resolved.tab_idx, resolved.pane_id);
        self.state.mark_active_tab_seen();
        self.state.mode = Mode::Terminal;
        self.agent_info(resolved.ws_idx, resolved.pane_id)
            .ok_or_else(|| TerminalTargetError::NotFound {
                target: target.to_string(),
            })
    }

    pub(super) fn rename_agent_target(
        &mut self,
        target: &str,
        name: Option<String>,
    ) -> Result<crate::api::schema::AgentInfo, AgentRenameError> {
        let resolved = self
            .resolve_agent_target(target)
            .map_err(AgentRenameError::Target)?;
        let normalized_name = name.and_then(|name| {
            let trimmed = name.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });

        if let Some(name) = normalized_name.as_deref() {
            let conflicts = self.agent_name_conflicts(name, &resolved.terminal_id);
            if !conflicts.is_empty() {
                return Err(AgentRenameError::DuplicateName {
                    name: name.to_string(),
                    candidates: conflicts,
                });
            }
        }

        let Some(terminal) = self
            .state
            .terminals
            .values_mut()
            .find(|terminal| terminal.id.to_string() == resolved.terminal_id)
        else {
            return Err(AgentRenameError::Target(TerminalTargetError::NotFound {
                target: target.to_string(),
            }));
        };
        match normalized_name {
            Some(name) => {
                terminal.set_agent_name(name.clone());
                terminal.set_manual_label(name);
            }
            None => terminal.clear_agent_name(),
        }
        self.state.mark_session_dirty();
        self.agent_info(resolved.ws_idx, resolved.pane_id)
            .ok_or_else(|| {
                AgentRenameError::Target(TerminalTargetError::NotFound {
                    target: target.to_string(),
                })
            })
    }

    pub(super) fn start_agent(
        &mut self,
        params: AgentStartParams,
        extra_env: Vec<(String, String)>,
    ) -> Result<AgentStartOutcome, AgentStartError> {
        let view = self.default_client_view.clone_reconciled(&self.state);
        self.start_agent_for_view(&view, params, extra_env)
    }

    pub(super) fn start_agent_for_view(
        &mut self,
        invoking_view: &crate::app::ClientViewState,
        params: AgentStartParams,
        extra_env: Vec<(String, String)>,
    ) -> Result<AgentStartOutcome, AgentStartError> {
        let name = params.name.trim().to_string();
        if name.is_empty() {
            return Err(AgentStartError::InvalidName);
        }
        if params.argv.is_empty() {
            return Err(AgentStartError::EmptyArgv);
        }
        let conflicts = self.agent_name_conflicts(&name, "");
        if !conflicts.is_empty() {
            return Err(AgentStartError::DuplicateName {
                name,
                candidates: conflicts,
            });
        }

        if params.cwd.is_some() && params.location.is_some() {
            return Err(AgentStartError::CwdLocationConflict);
        }
        let cwd_was_explicit = params.cwd.is_some();
        let cwd = params
            .cwd
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"));
        let requested_location = params
            .location
            .map(|location| {
                let host_id =
                    crate::execution_host::ExecutionHostId::new(location.execution_host_id)
                        .map_err(|error| AgentStartError::InvalidLocation(error.to_string()))?;
                let path = crate::execution_host::HostPath::new(location.path)
                    .map_err(|error| AgentStartError::InvalidLocation(error.to_string()))?;
                Ok(crate::execution_host::ResourceLocation::new(host_id, path))
            })
            .transpose()?;
        let argv = params.argv;
        let focus = params.focus;
        let (rows, cols) = self.state.estimate_pane_size();

        let placement = if let Some(tab_id) = params.tab_id {
            let (ws_idx, tab_idx) =
                self.parse_tab_id(&tab_id)
                    .ok_or_else(|| AgentStartError::TargetNotFound {
                        target: tab_id.clone(),
                    })?;
            if let Some(workspace_id) = params.workspace_id.as_deref() {
                let requested_ws_idx = self.parse_workspace_id(workspace_id).ok_or_else(|| {
                    AgentStartError::TargetNotFound {
                        target: workspace_id.to_string(),
                    }
                })?;
                if requested_ws_idx != ws_idx {
                    return Err(AgentStartError::PlacementConflict);
                }
            }
            let target_pane = self.state.workspaces[ws_idx].tabs[tab_idx].layout.focused();
            self.spawn_agent_split(
                ws_idx,
                target_pane,
                params.split.unwrap_or(SplitDirection::Right),
                cwd,
                &argv,
                extra_env,
                focus,
                requested_location.clone(),
                cwd_was_explicit,
            )?
        } else if let Some(workspace_id) = params.workspace_id {
            let ws_idx = self.parse_workspace_id(&workspace_id).ok_or_else(|| {
                AgentStartError::TargetNotFound {
                    target: workspace_id.clone(),
                }
            })?;
            let tab_idx = self.state.workspaces[ws_idx].active_tab;
            let target_pane = self.state.workspaces[ws_idx].tabs[tab_idx].layout.focused();
            self.spawn_agent_split(
                ws_idx,
                target_pane,
                params.split.unwrap_or(SplitDirection::Right),
                cwd,
                &argv,
                extra_env,
                focus,
                requested_location.clone(),
                cwd_was_explicit,
            )?
        } else if self.state.workspaces.is_empty() {
            self.spawn_agent_workspace(
                cwd,
                rows,
                cols,
                &argv,
                extra_env,
                focus,
                requested_location.clone(),
            )?
        } else if let Some((ws_idx, _tab_idx, target_pane)) =
            invoking_view.active_workspace.and_then(|ws_idx| {
                invoking_view
                    .focused_pane_for_workspace(&self.state, ws_idx)
                    .map(|(tab_idx, pane_id)| (ws_idx, tab_idx, pane_id))
            })
        {
            self.spawn_agent_split(
                ws_idx,
                target_pane,
                params.split.unwrap_or(SplitDirection::Right),
                cwd,
                &argv,
                extra_env,
                focus,
                requested_location,
                cwd_was_explicit,
            )?
        } else {
            let ws_idx = self.state.active.unwrap_or(0);
            let tab_idx = self.state.workspaces[ws_idx].active_tab;
            let target_pane = self.state.workspaces[ws_idx].tabs[tab_idx].layout.focused();
            self.spawn_agent_split(
                ws_idx,
                target_pane,
                params.split.unwrap_or(SplitDirection::Right),
                cwd,
                &argv,
                extra_env,
                focus,
                requested_location,
                cwd_was_explicit,
            )?
        };

        match placement {
            AgentStartPlacement::Committed {
                ws_idx,
                tab_idx,
                pane_id,
            } => {
                let terminal_id = self
                    .state
                    .workspaces
                    .get(ws_idx)
                    .and_then(|ws| ws.terminal_id(pane_id))
                    .cloned()
                    .ok_or_else(|| AgentStartError::SpawnFailed("terminal disappeared".into()))?;
                let Some(terminal) = self.state.terminals.get_mut(&terminal_id) else {
                    return Err(AgentStartError::SpawnFailed("terminal disappeared".into()));
                };
                terminal.set_agent_name(name.clone());
                terminal.set_manual_label(name);
                self.state.mark_session_dirty();

                let agent = self
                    .agent_info(ws_idx, pane_id)
                    .ok_or_else(|| AgentStartError::SpawnFailed("agent disappeared".into()))?;
                debug_assert_eq!(
                    Some(agent.tab_id.as_str()),
                    self.public_tab_id(ws_idx, tab_idx).as_deref()
                );
                Ok(AgentStartOutcome::Committed {
                    agent: Box::new(agent),
                    argv,
                })
            }
            AgentStartPlacement::Pending(terminal_id) => {
                self.configure_pending_remote_agent(
                    &terminal_id,
                    Some(name.clone()),
                    Some(name),
                    Some(argv.clone()),
                )
                .ok_or_else(|| {
                    AgentStartError::SpawnFailed(
                        "pending remote agent creation disappeared".to_string(),
                    )
                })?;
                Ok(AgentStartOutcome::Pending(terminal_id))
            }
        }
    }

    pub(super) fn agent_start_error_body(
        &self,
        err: AgentStartError,
    ) -> crate::api::schema::ErrorBody {
        match err {
            AgentStartError::InvalidName => crate::api::schema::ErrorBody {
                code: "invalid_agent_name".into(),
                message: "agent name must not be empty".into(),
            },
            AgentStartError::EmptyArgv => crate::api::schema::ErrorBody {
                code: "invalid_agent_argv".into(),
                message: "agent start argv must not be empty".into(),
            },
            AgentStartError::TargetNotFound { target } => crate::api::schema::ErrorBody {
                code: "agent_placement_not_found".into(),
                message: format!("agent placement target {target} not found"),
            },
            AgentStartError::PlacementConflict => crate::api::schema::ErrorBody {
                code: "agent_placement_conflict".into(),
                message: "--tab must belong to --workspace".into(),
            },
            AgentStartError::CwdLocationConflict => crate::api::schema::ErrorBody {
                code: "agent_placement_conflict".into(),
                message: "cwd and location cannot be used together".into(),
            },
            AgentStartError::InvalidLocation(message) => crate::api::schema::ErrorBody {
                code: "invalid_agent_location".into(),
                message,
            },
            AgentStartError::SpawnFailed(message) => crate::api::schema::ErrorBody {
                code: "agent_start_failed".into(),
                message,
            },
            AgentStartError::DuplicateName { name, candidates } => crate::api::schema::ErrorBody {
                code: "agent_name_taken".into(),
                message: format!(
                    "agent name {name} is already used; candidates: {}",
                    candidates
                        .into_iter()
                        .map(|candidate| format!(
                            "terminal_id={} pane_id={} workspace_id={} tab_id={} cwd={} status={:?}",
                            candidate.terminal_id,
                            candidate.pane_id,
                            candidate.workspace_id,
                            candidate.tab_id,
                            candidate.cwd.unwrap_or_else(|| "unknown".into()),
                            candidate.agent_status,
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            },
        }
    }

    pub(super) fn agent_target_error_body(
        &self,
        err: TerminalTargetError,
    ) -> crate::api::schema::ErrorBody {
        match err {
            TerminalTargetError::NotFound { target } => crate::api::schema::ErrorBody {
                code: "agent_not_found".into(),
                message: format!("agent target {target} not found"),
            },
            TerminalTargetError::Ambiguous { target, candidates } => {
                crate::api::schema::ErrorBody {
                    code: "agent_target_ambiguous".into(),
                    message: format!(
                        "agent target {target} is ambiguous; candidates: {}",
                        candidates
                            .into_iter()
                            .map(|candidate| format!(
                                "terminal_id={} pane_id={} workspace_id={} tab_id={} cwd={} status={:?}",
                                candidate.terminal_id,
                                candidate.pane_id,
                                candidate.workspace_id,
                                candidate.tab_id,
                                candidate.cwd.unwrap_or_else(|| "unknown".into()),
                                candidate.agent_status,
                            ))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                }
            }
        }
    }

    pub(super) fn agent_rename_error_body(
        &self,
        err: AgentRenameError,
    ) -> crate::api::schema::ErrorBody {
        match err {
            AgentRenameError::Target(err) => self.agent_target_error_body(err),
            AgentRenameError::DuplicateName { name, candidates } => crate::api::schema::ErrorBody {
                code: "agent_name_taken".into(),
                message: format!(
                    "agent name {name} is already used; candidates: {}",
                    candidates
                        .into_iter()
                        .map(|candidate| format!(
                            "terminal_id={} pane_id={} workspace_id={} tab_id={} cwd={} status={:?}",
                            candidate.terminal_id,
                            candidate.pane_id,
                            candidate.workspace_id,
                            candidate.tab_id,
                            candidate.cwd.unwrap_or_else(|| "unknown".into()),
                            candidate.agent_status,
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            },
        }
    }

    fn spawn_agent_workspace(
        &mut self,
        cwd: PathBuf,
        rows: u16,
        cols: u16,
        argv: &[String],
        extra_env: Vec<(String, String)>,
        focus: bool,
        location: Option<crate::execution_host::ResourceLocation>,
    ) -> Result<AgentStartPlacement, AgentStartError> {
        let local_fallback = crate::execution_host::ResourceLocation::local(cwd.clone())
            .map_err(|error| AgentStartError::InvalidLocation(error.to_string()))?;
        let location = crate::execution_host::placement::resolve_workspace_creation(
            location,
            None,
            local_fallback,
        );
        if !location.is_local() {
            let command = command_spec_from_argv(argv)?;
            let terminal_id = self
                .begin_remote_workspace(
                    location,
                    focus,
                    self.state.active_group_id().to_string(),
                    Some(command),
                    extra_env,
                )
                .map_err(AgentStartError::SpawnFailed)?;
            return Ok(AgentStartPlacement::Pending(terminal_id));
        }
        let cwd = location.path.as_path().to_path_buf();
        let (ws, terminal, runtime) = crate::workspace::Workspace::new_argv_command_with_extra_env(
            cwd,
            rows,
            cols,
            argv,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
            self.event_tx.clone(),
            self.render_notify.clone(),
            self.render_dirty.clone(),
            extra_env,
        )
        .map_err(|err| AgentStartError::SpawnFailed(err.to_string()))?;
        self.terminal_runtimes.insert(terminal.id.clone(), runtime);
        self.state.terminals.insert(terminal.id.clone(), terminal);
        self.state.workspaces.push(ws);
        let ws_idx = self.state.workspaces.len() - 1;
        self.state
            .remove_alias_shadowed_by_new_pane(self.state.workspaces[ws_idx].tabs[0].root_pane);
        if focus || self.state.active.is_none() {
            self.state.switch_workspace(ws_idx);
            self.state.mode = Mode::Terminal;
        }
        self.schedule_session_save();
        let pane_id = self.state.workspaces[ws_idx].tabs[0].root_pane;
        Ok(AgentStartPlacement::Committed {
            ws_idx,
            tab_idx: 0,
            pane_id,
        })
    }

    fn spawn_agent_split(
        &mut self,
        ws_idx: usize,
        target_pane: crate::layout::PaneId,
        split: SplitDirection,
        cwd: PathBuf,
        argv: &[String],
        extra_env: Vec<(String, String)>,
        focus: bool,
        requested_location: Option<crate::execution_host::ResourceLocation>,
        cwd_was_explicit: bool,
    ) -> Result<AgentStartPlacement, AgentStartError> {
        let (rows, cols) = self.state.estimate_pane_size();
        let direction = match split {
            SplitDirection::Right => ratatui::layout::Direction::Horizontal,
            SplitDirection::Down => ratatui::layout::Direction::Vertical,
        };
        let mut split_source = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|workspace| workspace.pane_state(target_pane))
            .and_then(|pane| self.state.terminals.get(&pane.attached_terminal_id))
            .map(|terminal| terminal.location.clone());
        if cwd_was_explicit {
            let path = crate::execution_host::HostPath::new(cwd.clone())
                .map_err(|error| AgentStartError::InvalidLocation(error.to_string()))?;
            if let Some(location) = &mut split_source {
                location.path = path;
            }
        }
        let split_source = split_source.ok_or_else(|| AgentStartError::TargetNotFound {
            target: target_pane.raw().to_string(),
        })?;
        let location = crate::execution_host::placement::resolve_split_creation(
            requested_location,
            split_source,
        );
        if !location.is_local() {
            let command = command_spec_from_argv(argv)?;
            let terminal_id = self
                .begin_remote_split(
                    ws_idx,
                    target_pane,
                    direction,
                    None,
                    location,
                    focus,
                    Some(command),
                    extra_env,
                )
                .map_err(AgentStartError::SpawnFailed)?;
            return Ok(AgentStartPlacement::Pending(terminal_id));
        }
        let cwd = location.path.as_path().to_path_buf();
        let previous_focus = self.state.current_pane_focus_target();
        let result = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| {
                ws.split_pane_argv_command(
                    target_pane,
                    direction,
                    rows,
                    cols,
                    Some(cwd),
                    argv,
                    extra_env,
                    self.state.pane_scrollback_limit_bytes,
                    self.state.host_terminal_theme,
                    focus,
                )
            })
            .ok_or_else(|| AgentStartError::TargetNotFound {
                target: target_pane.raw().to_string(),
            })?
            .map_err(|err| AgentStartError::SpawnFailed(err.to_string()))?;
        self.terminal_runtimes
            .insert(result.1.terminal.id.clone(), result.1.runtime);
        self.state
            .remove_alias_shadowed_by_new_pane(result.1.pane_id);
        self.state
            .terminals
            .insert(result.1.terminal.id.clone(), result.1.terminal);
        if focus {
            self.state.switch_workspace(ws_idx);
            self.state.switch_tab(result.0);
            self.state.mode = Mode::Terminal;
            self.state
                .record_pane_focus_change(previous_focus, ws_idx, result.1.pane_id);
        }
        self.schedule_session_save();
        Ok(AgentStartPlacement::Committed {
            ws_idx,
            tab_idx: result.0,
            pane_id: result.1.pane_id,
        })
    }

    pub(super) fn agent_info(
        &self,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<crate::api::schema::AgentInfo> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let pane_state = ws.pane_state(pane_id)?;
        let terminal = self.state.terminals.get(&pane_state.attached_terminal_id)?;
        if !terminal.is_agent_terminal() && !self.state.is_agent_follow_up(ws_idx, pane_id) {
            return None;
        }
        let pane = self.pane_info(ws_idx, pane_id)?;
        Some(crate::api::schema::AgentInfo {
            terminal_id: pane.terminal_id,
            location: pane.location,
            name: terminal.agent_name.clone(),
            agent: pane.agent,
            title: pane.title,
            display_agent: pane.display_agent,
            agent_status: pane.agent_status,
            screen_detection_skipped: terminal.full_lifecycle_hook_authority_active(),
            custom_status: pane.custom_status,
            state_labels: pane.state_labels,
            tokens: pane.tokens,
            agent_session: pane.agent_session,
            workspace_id: pane.workspace_id,
            tab_id: pane.tab_id,
            pane_id: pane.pane_id,
            focused: pane.focused,
            cwd: pane.cwd,
            foreground_cwd: pane.foreground_cwd,
            revision: pane.revision,
            last_meaningful_agent_activity_unix_secs: terminal
                .last_meaningful_agent_activity_unix_secs(),
            follow_up: self.state.is_agent_follow_up(ws_idx, pane_id),
            follow_up_added_at_unix_secs: self.state.follow_up_added_at(ws_idx, pane_id),
        })
    }

    fn agent_name_conflicts(
        &self,
        name: &str,
        except_terminal_id: &str,
    ) -> Vec<crate::api::schema::AgentInfo> {
        self.collect_agent_infos()
            .into_iter()
            .filter(|agent| {
                agent.name.as_deref() == Some(name) && agent.terminal_id != except_terminal_id
            })
            .collect()
    }
}

pub(super) fn runtime_hosts_agent(
    runtime: &crate::terminal::TerminalRuntime,
    expected: crate::detect::Agent,
) -> bool {
    #[cfg(test)]
    if runtime.child_pid() == 0 {
        return true;
    }
    let Some(job) = crate::detect::foreground_job(runtime.child_pid()) else {
        return false;
    };
    crate::detect::identify_agent_in_job(&job)
        .map(|(agent, _)| agent)
        .or_else(|| {
            job.processes
                .iter()
                .find_map(|process| crate::platform::process_agent_hint(process.pid))
        })
        == Some(expected)
}

fn command_spec_from_argv(
    argv: &[String],
) -> Result<crate::execution_host::protocol::CommandSpec, AgentStartError> {
    let Some((program, args)) = argv.split_first() else {
        return Err(AgentStartError::EmptyArgv);
    };
    Ok(crate::execution_host::protocol::CommandSpec {
        program: program.clone(),
        args: args.to_vec(),
        env: Vec::new(),
    })
}

#[derive(Debug)]
pub(super) enum AgentStartOutcome {
    Committed {
        agent: Box<crate::api::schema::AgentInfo>,
        argv: Vec<String>,
    },
    Pending(crate::terminal::TerminalId),
}

enum AgentStartPlacement {
    Committed {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: crate::layout::PaneId,
    },
    Pending(crate::terminal::TerminalId),
}

#[derive(Debug)]
pub(super) enum AgentStartError {
    InvalidName,
    EmptyArgv,
    TargetNotFound {
        target: String,
    },
    PlacementConflict,
    CwdLocationConflict,
    InvalidLocation(String),
    SpawnFailed(String),
    DuplicateName {
        name: String,
        candidates: Vec<crate::api::schema::AgentInfo>,
    },
}

#[derive(Debug)]
pub(super) enum AgentRenameError {
    Target(TerminalTargetError),
    DuplicateName {
        name: String,
        candidates: Vec<crate::api::schema::AgentInfo>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::ResourceLocationParams;
    use crate::execution_host::protocol::CoordinatorMessage;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces.clear();
        app.state.terminals.clear();
        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
        app.state.active = None;
        app
    }

    fn agent_params(
        name: &str,
        location: Option<ResourceLocationParams>,
        argv: Vec<String>,
    ) -> AgentStartParams {
        AgentStartParams {
            name: name.to_string(),
            cwd: None,
            location,
            workspace_id: None,
            tab_id: None,
            split: None,
            focus: false,
            env: std::collections::HashMap::new(),
            argv,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remote_agent_launch_sends_host_qualified_path_and_command_to_worker() {
        let mut app = test_app();
        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox")
            .expect("test host id should be valid");
        let messages = app
            .execution_hosts
            .as_mut()
            .expect("test app should have an execution host manager")
            .connect_test_host(host_id.clone());

        let outcome = app
            .start_agent(
                agent_params(
                    "remote-worker",
                    Some(ResourceLocationParams {
                        execution_host_id: host_id.as_str().to_string(),
                        path: "/srv/project".to_string(),
                    }),
                    vec!["remote-agent".to_string(), "--resume".to_string()],
                ),
                Vec::new(),
            )
            .expect("remote agent should launch through the connected worker");
        let AgentStartOutcome::Pending(terminal_id) = outcome else {
            panic!("remote agent should remain pending until the worker reports readiness");
        };
        let messages: std::sync::MutexGuard<'_, Vec<CoordinatorMessage>> = match messages.lock() {
            Ok(messages) => messages,
            Err(poisoned) => poisoned.into_inner(),
        };
        let [CoordinatorMessage::CreateTerminal {
            location,
            command: Some(command),
            ..
        }] = messages.as_slice()
        else {
            panic!("expected one remote terminal creation message: {messages:?}");
        };
        assert_eq!(location.execution_host_id, host_id);
        assert_eq!(
            location.path.as_path(),
            std::path::Path::new("/srv/project")
        );
        assert_eq!(command.program, "remote-agent");
        assert_eq!(command.args, vec!["--resume"]);
        assert!(!app.state.terminals.contains_key(&terminal_id));
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn local_agent_launch_stays_on_local_execution_host() {
        let mut app = test_app();
        let outcome = app
            .start_agent(
                agent_params(
                    "local-worker",
                    None,
                    vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "sleep 1".to_string(),
                    ],
                ),
                Vec::new(),
            )
            .expect("local agent should launch");
        let AgentStartOutcome::Committed { agent, .. } = outcome else {
            panic!("local agent should commit synchronously");
        };

        let terminal = app
            .state
            .terminals
            .values()
            .find(|terminal| terminal.id.to_string() == agent.terminal_id)
            .expect("started agent terminal should exist");
        assert!(terminal.location.is_local());
        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn remote_project_command_routes_host_path_and_shell_command_to_worker() {
        let mut app = test_app();
        let outcome = app
            .start_agent(
                agent_params(
                    "remote-command-source",
                    None,
                    vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "sleep 1".to_string(),
                    ],
                ),
                Vec::new(),
            )
            .expect("source terminal should launch");
        let AgentStartOutcome::Committed { agent, .. } = outcome else {
            panic!("source terminal should be committed");
        };
        let terminal_id = app
            .state
            .terminals
            .keys()
            .find(|terminal_id| terminal_id.to_string() == agent.terminal_id)
            .cloned()
            .expect("source terminal should exist");
        if let Some(runtime) = app.terminal_runtimes.remove(&terminal_id) {
            runtime.shutdown();
        }

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox").unwrap();
        let location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/project").unwrap(),
        );
        let terminal = app.state.terminals.get_mut(&terminal_id).unwrap();
        terminal.location = location.clone();
        terminal.cwd = location.path.as_path().to_path_buf();

        let command = crate::commands::ProjectCommand::new(
            location.clone(),
            crate::commands::CommandSource::PackageJson,
            "dev",
            "npm run dev",
            crate::commands::CommandConfidence::Explicit,
        );
        let command_id = command.id.clone();
        app.state.command_catalog.push(command);
        let messages = app
            .execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        app.state.mode = Mode::CommandPalette;
        app.state.command_palette.query = "run project command: dev".to_string();
        app.handle_command_palette_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        let messages: std::sync::MutexGuard<'_, Vec<CoordinatorMessage>> = match messages.lock() {
            Ok(messages) => messages,
            Err(poisoned) => poisoned.into_inner(),
        };
        let [CoordinatorMessage::CreateTerminal {
            location: sent_location,
            command: Some(command),
            ..
        }] = messages.as_slice()
        else {
            panic!("expected one remote command creation message: {messages:?}");
        };
        assert_eq!(sent_location, &location);
        assert_eq!(command.program, "/bin/sh");
        assert_eq!(command.args, vec!["-lc", "npm run dev"]);
        drop(messages);
        let run = app
            .state
            .command_runs
            .get(&command_id)
            .expect("remote command run should be tracked");
        assert_eq!(&run.execution_host_id, &host_id);
        assert_eq!(run.status, crate::commands::CommandRunStatus::Running);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn remote_project_command_run_exit_rerun_launches_fresh_runtime() {
        let mut app = test_app();
        let outcome = app
            .start_agent(
                agent_params(
                    "remote-command-rerun-source",
                    None,
                    vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "sleep 1".to_string(),
                    ],
                ),
                Vec::new(),
            )
            .expect("source terminal should launch");
        let AgentStartOutcome::Committed { agent, .. } = outcome else {
            panic!("source terminal should be committed");
        };
        let source_terminal_id = app
            .state
            .terminals
            .keys()
            .find(|terminal_id| terminal_id.to_string() == agent.terminal_id)
            .cloned()
            .expect("source terminal should exist");
        if let Some(runtime) = app.terminal_runtimes.remove(&source_terminal_id) {
            runtime.shutdown();
        }

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox").unwrap();
        let location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/project").unwrap(),
        );
        {
            let terminal = app.state.terminals.get_mut(&source_terminal_id).unwrap();
            terminal.location = location.clone();
            terminal.cwd = location.path.as_path().to_path_buf();
        }

        let command = crate::commands::ProjectCommand::new(
            location.clone(),
            crate::commands::CommandSource::PackageJson,
            "dev",
            "npm run dev",
            crate::commands::CommandConfidence::Explicit,
        );
        let command_id = command.id.clone();
        app.state.command_catalog.push(command);
        let messages = app
            .execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        app.run_project_command_on_resolved_host(&command_id)
            .expect("first remote command launch should succeed");
        let first_terminal_id = app
            .state
            .command_runs
            .get(&command_id)
            .expect("command run should be tracked")
            .terminal_id
            .clone();
        assert!(app
            .pending_remote_creations
            .contains_key(&first_terminal_id));

        app.complete_remote_creation_ready(
            first_terminal_id.clone(),
            crate::execution_host::protocol::RuntimeIdentity::new(
                crate::execution_host::protocol::HostBindingGeneration::new(1),
                crate::execution_host::protocol::WorkerInstanceId::new("worker-a").unwrap(),
                crate::execution_host::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
                crate::execution_host::protocol::RuntimeIncarnation::new(1),
            ),
            location.clone(),
        );
        let _ = app.take_remote_creation_completions();
        assert!(app.state.terminals.contains_key(&first_terminal_id));
        assert_eq!(
            app.state.command_runs.get(&command_id).unwrap().status,
            crate::commands::CommandRunStatus::Running
        );

        // Simulate short-lived remote command exit while retaining the tab/scrollback.
        app.state
            .command_runs
            .get_mut(&command_id)
            .expect("command run should still exist")
            .status = crate::commands::CommandRunStatus::Stopped;

        app.run_project_command_on_resolved_host(&command_id)
            .expect("completed remote command should rerun");
        let second_terminal_id = app
            .state
            .command_runs
            .get(&command_id)
            .expect("rerun should keep command state")
            .terminal_id
            .clone();
        assert_ne!(
            first_terminal_id, second_terminal_id,
            "rerun must allocate a fresh remote runtime id"
        );
        assert!(app
            .pending_remote_creations
            .contains_key(&second_terminal_id));
        assert_eq!(
            app.state.command_runs.get(&command_id).unwrap().status,
            crate::commands::CommandRunStatus::Running
        );
        assert_eq!(
            app.state
                .command_runs
                .get(&command_id)
                .unwrap()
                .execution_host_id,
            host_id
        );

        let messages = match messages.lock() {
            Ok(messages) => messages.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let create_count = messages
            .iter()
            .filter(|message| {
                matches!(
                    message,
                    CoordinatorMessage::CreateTerminal {
                        command: Some(_),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            create_count, 2,
            "expected run then rerun creates: {messages:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn remote_project_command_worker_refusal_clears_run_for_retry() {
        let mut app = test_app();
        let outcome = app
            .start_agent(
                agent_params(
                    "remote-command-refuse-source",
                    None,
                    vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "sleep 1".to_string(),
                    ],
                ),
                Vec::new(),
            )
            .expect("source terminal should launch");
        let AgentStartOutcome::Committed { agent, .. } = outcome else {
            panic!("source terminal should be committed");
        };
        let source_terminal_id = app
            .state
            .terminals
            .keys()
            .find(|terminal_id| terminal_id.to_string() == agent.terminal_id)
            .cloned()
            .expect("source terminal should exist");
        if let Some(runtime) = app.terminal_runtimes.remove(&source_terminal_id) {
            runtime.shutdown();
        }

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox").unwrap();
        let location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/project").unwrap(),
        );
        {
            let terminal = app.state.terminals.get_mut(&source_terminal_id).unwrap();
            terminal.location = location.clone();
            terminal.cwd = location.path.as_path().to_path_buf();
        }

        let command = crate::commands::ProjectCommand::new(
            location.clone(),
            crate::commands::CommandSource::PackageJson,
            "build",
            "npm run build",
            crate::commands::CommandConfidence::Explicit,
        );
        let command_id = command.id.clone();
        app.state.command_catalog.push(command);
        let messages = app
            .execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        app.run_project_command_on_resolved_host(&command_id)
            .expect("initial remote launch should queue");
        let refused_terminal_id = app
            .state
            .command_runs
            .get(&command_id)
            .expect("command run should be pending")
            .terminal_id
            .clone();

        // Mirror the coordinator TerminalFailed path for non-API TUI launches.
        let was_pending = app
            .pending_remote_creations
            .contains_key(&refused_terminal_id);
        app.complete_remote_creation_failed(
            refused_terminal_id.clone(),
            "worker refused command capability".into(),
        );
        let cleared = app.clear_command_runs_for_terminal(&refused_terminal_id);
        assert!(was_pending);
        assert!(cleared);
        assert!(!app.state.command_runs.contains_key(&command_id));
        assert!(!app
            .pending_remote_creations
            .contains_key(&refused_terminal_id));

        app.run_project_command_on_resolved_host(&command_id)
            .expect("retry after refusal should launch again");
        let retry_terminal_id = app
            .state
            .command_runs
            .get(&command_id)
            .expect("retry should recreate command run")
            .terminal_id
            .clone();
        assert_ne!(refused_terminal_id, retry_terminal_id);
        assert!(app
            .pending_remote_creations
            .contains_key(&retry_terminal_id));

        let messages = match messages.lock() {
            Ok(messages) => messages.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let create_count = messages
            .iter()
            .filter(|message| {
                matches!(
                    message,
                    CoordinatorMessage::CreateTerminal {
                        command: Some(_),
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            create_count, 2,
            "expected refuse then retry creates: {messages:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn remote_workspace_command_discovery_feeds_catalog_and_launch_routing() {
        let mut app = test_app();
        // Source pane on a remote host with a cwd that discovery can target.
        let outcome = app
            .start_agent(
                agent_params(
                    "remote-discovery-source",
                    None,
                    vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "sleep 1".to_string(),
                    ],
                ),
                Vec::new(),
            )
            .expect("source terminal should launch");
        let AgentStartOutcome::Committed { agent, .. } = outcome else {
            panic!("source terminal should be committed");
        };
        let terminal_id = app
            .state
            .terminals
            .keys()
            .find(|terminal_id| terminal_id.to_string() == agent.terminal_id)
            .cloned()
            .expect("source terminal should exist");
        if let Some(runtime) = app.terminal_runtimes.remove(&terminal_id) {
            runtime.shutdown();
        }

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox").unwrap();
        let location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/project").unwrap(),
        );
        {
            let terminal = app.state.terminals.get_mut(&terminal_id).unwrap();
            terminal.location = location.clone();
            terminal.cwd = location.path.as_path().to_path_buf();
        }

        let messages = app
            .execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        // First refresh issues DiscoverProjectCommands (no local FS for remote).
        assert!(
            app.state.refresh_command_catalog_with_hosts(
                &app.terminal_runtimes,
                app.execution_hosts.as_mut(),
            ) || app.state.command_catalog.is_empty()
        );
        let request_id = {
            let locked = messages.lock().expect("message lock");
            locked.iter().find_map(|message| match message {
                CoordinatorMessage::DiscoverProjectCommands {
                    request_id,
                    location: sent_location,
                } if sent_location == &location => Some(*request_id),
                _ => None,
            })
        };
        let request_id = request_id.expect("discovery request should be sent for remote cwd");

        // Worker responds with host-qualified package script.
        app.execution_hosts.as_mut().unwrap().route_worker_message(
            host_id.clone(),
            crate::execution_host::protocol::WorkerMessage::ProjectCommandsResult {
                request_id,
                location: location.clone(),
                commands: vec![crate::execution_host::protocol::ProjectCommandSnapshot {
                    location: location.clone(),
                    source: crate::execution_host::protocol::ProjectCommandSource::PackageJson,
                    name: "dev".into(),
                    command: "npm run dev".into(),
                    confidence: crate::execution_host::protocol::ProjectCommandConfidence::Explicit,
                }],
                error: None,
            },
            &mut Vec::new(),
        );

        let _ = app.state.refresh_command_catalog_with_hosts(
            &app.terminal_runtimes,
            app.execution_hosts.as_mut(),
        );
        let command = app
            .state
            .command_catalog
            .iter()
            .find(|command| command.name == "dev")
            .cloned()
            .expect("discovered remote command should enter catalog");
        assert_eq!(command.location, location);
        assert_eq!(command.command, "npm run dev");

        messages.lock().expect("message lock").clear();
        let command_id = command.id.clone();
        app.run_project_command_on_resolved_host(&command_id)
            .expect("discovered remote command should launch");

        let messages = match messages.lock() {
            Ok(messages) => messages.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let create = messages.iter().find_map(|message| match message {
            CoordinatorMessage::CreateTerminal {
                location: sent_location,
                command: Some(command),
                ..
            } => Some((sent_location.clone(), command.clone())),
            _ => None,
        });
        let (sent_location, sent_command) =
            create.expect("launch should create remote terminal for discovered command");
        assert_eq!(sent_location, location);
        assert_eq!(sent_command.program, "/bin/sh");
        assert_eq!(sent_command.args, vec!["-lc", "npm run dev"]);
    }

    #[tokio::test]
    async fn remote_nested_cwd_discovery_and_configured_diff_are_host_routed() {
        let mut app = test_app();
        app.state.git_diff_command = "lazygit".to_string();
        let outcome = app
            .start_agent(
                agent_params(
                    "remote-nested-source",
                    None,
                    vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "sleep 1".to_string(),
                    ],
                ),
                Vec::new(),
            )
            .expect("source terminal should launch");
        let AgentStartOutcome::Committed { agent, .. } = outcome else {
            panic!("source terminal should be committed");
        };
        let terminal_id = app
            .state
            .terminals
            .keys()
            .find(|terminal_id| terminal_id.to_string() == agent.terminal_id)
            .cloned()
            .expect("source terminal should exist");
        if let Some(runtime) = app.terminal_runtimes.remove(&terminal_id) {
            runtime.shutdown();
        }

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox").unwrap();
        let nested = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/project/packages/app").unwrap(),
        );
        let root = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/project").unwrap(),
        );
        {
            let terminal = app.state.terminals.get_mut(&terminal_id).unwrap();
            terminal.location = nested.clone();
            terminal.cwd = nested.path.as_path().to_path_buf();
        }

        let messages = app
            .execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        let _ = app.state.refresh_command_catalog_with_hosts(
            &app.terminal_runtimes,
            app.execution_hosts.as_mut(),
        );
        let request_id = {
            let locked = messages.lock().expect("message lock");
            locked.iter().find_map(|message| match message {
                CoordinatorMessage::DiscoverProjectCommands {
                    request_id,
                    location: sent_location,
                } if sent_location == &nested => Some(*request_id),
                _ => None,
            })
        }
        .expect("discovery should target nested remote cwd");

        // Worker root-qualifies nested cwd discovery; completion keys the
        // request location Fresh and also publishes under the related root.
        app.execution_hosts.as_mut().unwrap().route_worker_message(
            host_id.clone(),
            crate::execution_host::protocol::WorkerMessage::ProjectCommandsResult {
                request_id,
                location: root.clone(),
                commands: vec![crate::execution_host::protocol::ProjectCommandSnapshot {
                    location: root.clone(),
                    source: crate::execution_host::protocol::ProjectCommandSource::PackageJson,
                    name: "dev".into(),
                    command: "npm run dev".into(),
                    confidence: crate::execution_host::protocol::ProjectCommandConfidence::Explicit,
                }],
                error: None,
            },
            &mut Vec::new(),
        );

        let _ = app.state.refresh_command_catalog_with_hosts(
            &app.terminal_runtimes,
            app.execution_hosts.as_mut(),
        );
        assert!(
            app.state.command_catalog.iter().any(|command| {
                command.name == "dev"
                    && command.location.execution_host_id == host_id
                    && command.location.path.as_path() == root.path.as_path()
            }),
            "nested cwd discovery must surface root-qualified remote commands"
        );
        assert!(
            app.state.command_catalog.iter().any(|command| {
                command.command.contains("lazygit")
                    && command.location.execution_host_id == host_id
                    && (command.location.path.as_path() == root.path.as_path()
                        || command.location.path.as_path() == nested.path.as_path())
            }),
            "configured git-diff must merge as host-routed at remote root: {:?}",
            app.state
                .command_catalog
                .iter()
                .map(|c| {
                    (
                        c.name.clone(),
                        c.command.clone(),
                        c.location.path.as_path().display().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        );
    }
}
