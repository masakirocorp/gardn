use crate::api::schema::{
    EventData, EventEnvelope, EventKind, ResponseResult, TabCreateParams, TabInfo, TabListParams,
    TabRenameParams, TabTarget,
};
use crate::app::{view_state::ClientViewState, App, Mode};

use super::super::api_helpers::{pane_agent_status, tab_attention_priority};

use super::responses::{encode_error, encode_success};

impl App {
    pub(super) fn handle_tab_list(&mut self, id: String, params: TabListParams) -> String {
        let tabs = if let Some(workspace_id) = params.workspace_id {
            let Some(ws_idx) = self.parse_workspace_id(&workspace_id) else {
                return workspace_not_found(id, &workspace_id);
            };
            let Some(ws) = self.state.workspaces.get(ws_idx) else {
                return workspace_not_found(id, &workspace_id);
            };
            (0..ws.tabs.len())
                .filter_map(|tab_idx| self.tab_info(ws_idx, tab_idx))
                .collect()
        } else {
            let mut tabs = Vec::new();
            for (ws_idx, ws) in self.state.workspaces.iter().enumerate() {
                for tab_idx in 0..ws.tabs.len() {
                    if let Some(tab) = self.tab_info(ws_idx, tab_idx) {
                        tabs.push(tab);
                    }
                }
            }
            tabs
        };

        encode_success(id, ResponseResult::TabList { tabs })
    }

    pub(super) fn handle_tab_list_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: TabListParams,
    ) -> String {
        let tabs = if let Some(workspace_id) = params.workspace_id {
            let Some(ws_idx) = self.parse_workspace_id(&workspace_id) else {
                return workspace_not_found(id, &workspace_id);
            };
            let Some(ws) = self.state.workspaces.get(ws_idx) else {
                return workspace_not_found(id, &workspace_id);
            };
            (0..ws.tabs.len())
                .filter_map(|tab_idx| self.tab_info_for_view(view, ws_idx, tab_idx))
                .collect()
        } else {
            let mut tabs = Vec::new();
            for (ws_idx, ws) in self.state.workspaces.iter().enumerate() {
                for tab_idx in 0..ws.tabs.len() {
                    if let Some(tab) = self.tab_info_for_view(view, ws_idx, tab_idx) {
                        tabs.push(tab);
                    }
                }
            }
            tabs
        };

        encode_success(id, ResponseResult::TabList { tabs })
    }

    pub(super) fn handle_tab_get(&mut self, id: String, target: TabTarget) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&target.tab_id) else {
            return tab_not_found(id, &target.tab_id);
        };
        let Some(tab) = self.tab_info(ws_idx, tab_idx) else {
            return tab_not_found(id, &target.tab_id);
        };

        encode_success(id, ResponseResult::TabInfo { tab })
    }

    pub(super) fn handle_tab_get_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        target: TabTarget,
    ) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&target.tab_id) else {
            return tab_not_found(id, &target.tab_id);
        };
        let Some(tab) = self.tab_info_for_view(view, ws_idx, tab_idx) else {
            return tab_not_found(id, &target.tab_id);
        };

        encode_success(id, ResponseResult::TabInfo { tab })
    }

    pub(super) fn handle_tab_create_disposition(
        &mut self,
        id: String,
        params: TabCreateParams,
    ) -> crate::api::ApiRequestDisposition {
        self.handle_tab_create_with(
            super::invocation::ApiInvocationContext::ambient(),
            id,
            params,
        )
    }

    pub(super) fn handle_tab_create_disposition_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: TabCreateParams,
    ) -> crate::api::ApiRequestDisposition {
        self.handle_tab_create_with(
            super::invocation::ApiInvocationContext::for_view(view),
            id,
            params,
        )
    }

    fn handle_tab_create_with(
        &mut self,
        mut invocation: super::invocation::ApiInvocationContext<'_>,
        id: String,
        params: TabCreateParams,
    ) -> crate::api::ApiRequestDisposition {
        let TabCreateParams {
            workspace_id,
            cwd,
            location,
            focus,
            label,
            env,
        } = params;
        if let Some(view) = invocation.view_mut() {
            view.reconcile(&self.state);
        }
        let ws_idx = if let Some(workspace_id) = workspace_id {
            let Some(ws_idx) = self.parse_workspace_id(&workspace_id) else {
                return crate::api::ApiRequestDisposition::Respond(workspace_not_found(
                    id,
                    &workspace_id,
                ));
            };
            ws_idx
        } else if let Some(active) = invocation
            .view()
            .and_then(|view| view.active_workspace)
            .or(self.state.active)
        {
            active
        } else {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "workspace_not_found",
                "no active workspace",
            ));
        };
        if cwd.is_some() && location.is_some() {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "invalid_params",
                "cwd and location cannot be used together".to_string(),
            ));
        }
        let focused_pane = if let Some(view) = invocation.view() {
            view.focused_pane_for_workspace(&self.state, ws_idx)
                .map(|(_, pane_id)| pane_id)
        } else {
            self.state
                .workspaces
                .get(ws_idx)
                .and_then(|workspace| workspace.focused_pane_id())
        };
        let location =
            match tab_creation_location(&self.state, ws_idx, focused_pane, cwd.clone(), location) {
                Ok(location) => location,
                Err(error) => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "invalid_params",
                        error,
                    ))
                }
            };
        let cwd = location.path.as_path().to_path_buf();
        let (rows, cols) = self.state.estimate_pane_size();
        let default_shell = self.state.default_shell.clone();
        let scrollback_limit_bytes = self.state.pane_scrollback_limit_bytes;
        let host_terminal_theme = self.state.host_terminal_theme;
        let shell_mode = self.state.shell_mode;
        let event_tx = self.event_tx.clone();
        let render_notify = self.render_notify.clone();
        let render_dirty = self.render_dirty.clone();
        let extra_env = match super::env::normalize_launch_env(env) {
            Ok(env) => env,
            Err((code, message)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(id, &code, message))
            }
        };
        let client_local = invocation.is_client_local();
        let begin_focus = focus && !client_local;
        if !location.is_local() {
            match self.begin_remote_tab(ws_idx, location, begin_focus, None, extra_env) {
                Ok(terminal_id) => {
                    let mut pending_focus = None;
                    if focus {
                        if let Some(view) = invocation.view_mut() {
                            if self.pending_remote_creation_target(&terminal_id).is_some() {
                                let workspace_id = self.state.workspaces[ws_idx].id.clone();
                                // Prefer tab index once committed; pending_active_tabs holds future index.
                                let pending_tab_idx = self.state.workspaces[ws_idx].tabs.len();
                                view.pending_active_tabs
                                    .insert(workspace_id.clone(), pending_tab_idx);
                                pending_focus = Some(crate::api::PendingFocusMarker::Tab {
                                    workspace_id,
                                    tab_idx: pending_tab_idx,
                                });
                            }
                        }
                    }
                    return crate::api::ApiRequestDisposition::Deferred(
                        crate::api::DeferredRemoteCreate {
                            terminal_id,
                            request_id: id,
                            kind: crate::api::DeferredRemoteCreateKind::TabCreate { label },
                            focus,
                            client_view_id: invocation.client_view_id(),
                            pending_focus,
                        },
                    );
                }
                Err(err) => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "tab_create_failed",
                        err,
                    ))
                }
            }
        }
        let result = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .ok_or_else(|| "workspace disappeared".to_string())
            .and_then(|ws| {
                ws.create_tab_with_handles_and_env(
                    rows,
                    cols,
                    cwd,
                    scrollback_limit_bytes,
                    host_terminal_theme,
                    crate::pane::PaneShellConfig::new(&default_shell, shell_mode),
                    extra_env,
                    event_tx,
                    render_notify,
                    render_dirty,
                )
                .map_err(|error| error.to_string())
            });
        match result {
            Ok((tab_idx, terminal, runtime)) => {
                self.terminal_runtimes.insert(terminal.id.clone(), runtime);
                self.state.terminals.insert(terminal.id.clone(), terminal);
                self.state.remove_alias_shadowed_by_new_pane(
                    self.state.workspaces[ws_idx].tabs[tab_idx].root_pane,
                );
                if let Some(label) = label {
                    let workspace_id = self.state.workspaces[ws_idx].id.clone();
                    let tab_id = self
                        .public_tab_id(ws_idx, tab_idx)
                        .unwrap_or_else(|| format!("{}:{}", workspace_id, tab_idx + 1));
                    if let Some(tab) = self
                        .state
                        .workspaces
                        .get_mut(ws_idx)
                        .and_then(|ws| ws.tabs.get_mut(tab_idx))
                    {
                        tab.set_custom_name(label);
                        crate::logging::tab_renamed(&workspace_id, &tab_id);
                    }
                }
                if let Some(view) = invocation.view_mut() {
                    if focus {
                        let root_pane = self.state.workspaces[ws_idx].tabs[tab_idx].root_pane;
                        view.focus_pane_in_workspace(&self.state, ws_idx, tab_idx, root_pane);
                    } else {
                        view.reconcile(&self.state);
                    }
                } else if focus {
                    self.state.switch_workspace(ws_idx);
                    self.state.switch_tab(tab_idx);
                    self.state.mode = Mode::Terminal;
                }
                self.schedule_session_save();
                let (tab, root_pane, created) = if let Some(view) = invocation.view() {
                    (
                        self.tab_info_for_view(view, ws_idx, tab_idx).unwrap(),
                        self.root_pane_info_for_view(view, ws_idx, tab_idx)
                            .expect("new tab should have a root pane"),
                        self.tab_created_result_for_view(view, ws_idx, tab_idx)
                            .expect("new tab should produce a complete create response"),
                    )
                } else {
                    (
                        self.tab_info(ws_idx, tab_idx).unwrap(),
                        self.root_pane_info(ws_idx, tab_idx)
                            .expect("new tab should have a root pane"),
                        self.tab_created_result(ws_idx, tab_idx)
                            .expect("new tab should produce a complete create response"),
                    )
                };
                self.emit_event(EventEnvelope {
                    event: EventKind::TabCreated,
                    data: EventData::TabCreated { tab: tab.clone() },
                });
                self.emit_event(EventEnvelope {
                    event: EventKind::PaneCreated,
                    data: EventData::PaneCreated {
                        pane: root_pane.clone(),
                    },
                });
                crate::api::ApiRequestDisposition::Respond(encode_success(id, created))
            }
            Err(err) => crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "tab_create_failed",
                err,
            )),
        }
    }

    pub(super) fn handle_tab_focus(&mut self, id: String, target: TabTarget) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&target.tab_id) else {
            return tab_not_found(id, &target.tab_id);
        };
        self.state.switch_workspace(ws_idx);
        self.state.switch_tab(tab_idx);
        let tab = self.tab_info(ws_idx, tab_idx).unwrap();

        encode_success(id, ResponseResult::TabInfo { tab })
    }

    pub(super) fn handle_tab_focus_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        target: TabTarget,
    ) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&target.tab_id) else {
            return tab_not_found(id, &target.tab_id);
        };
        let Some(root_pane) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .map(|tab| tab.root_pane)
        else {
            return tab_not_found(id, &target.tab_id);
        };
        view.focus_pane_in_workspace(&self.state, ws_idx, tab_idx, root_pane);
        self.state.mark_active_tab_seen_for_view(view);
        let tab = self.tab_info_for_view(view, ws_idx, tab_idx).unwrap();

        encode_success(id, ResponseResult::TabInfo { tab })
    }

    pub(super) fn handle_tab_rename(&mut self, id: String, params: TabRenameParams) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&params.tab_id) else {
            return tab_not_found(id, &params.tab_id);
        };
        let workspace_id = self.state.workspaces[ws_idx].id.clone();
        let tab_id = self
            .public_tab_id(ws_idx, tab_idx)
            .unwrap_or_else(|| format!("{}:{}", workspace_id, tab_idx + 1));
        let Some(tab) = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
        else {
            return tab_not_found(id, &params.tab_id);
        };
        tab.set_custom_name(params.label.clone());
        crate::logging::tab_renamed(&workspace_id, &tab_id);
        self.schedule_session_save();
        self.emit_event(EventEnvelope {
            event: EventKind::TabRenamed,
            data: EventData::TabRenamed {
                tab_id: self.public_tab_id(ws_idx, tab_idx).unwrap(),
                workspace_id: self.public_workspace_id(ws_idx),
                label: params.label,
            },
        });
        let tab = self.tab_info(ws_idx, tab_idx).unwrap();

        encode_success(id, ResponseResult::TabInfo { tab })
    }

    pub(super) fn handle_tab_rename_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: TabRenameParams,
    ) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&params.tab_id) else {
            return tab_not_found(id, &params.tab_id);
        };
        let workspace_id = self.state.workspaces[ws_idx].id.clone();
        let tab_id = self
            .public_tab_id(ws_idx, tab_idx)
            .unwrap_or_else(|| format!("{}:{}", workspace_id, tab_idx + 1));
        let Some(tab) = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
        else {
            return tab_not_found(id, &params.tab_id);
        };
        tab.set_custom_name(params.label.clone());
        crate::logging::tab_renamed(&workspace_id, &tab_id);
        self.schedule_session_save();
        self.emit_event(EventEnvelope {
            event: EventKind::TabRenamed,
            data: EventData::TabRenamed {
                tab_id: self.public_tab_id(ws_idx, tab_idx).unwrap(),
                workspace_id: self.public_workspace_id(ws_idx),
                label: params.label,
            },
        });
        let tab = self.tab_info_for_view(view, ws_idx, tab_idx).unwrap();

        encode_success(id, ResponseResult::TabInfo { tab })
    }

    pub(super) fn handle_tab_close(&mut self, id: String, target: TabTarget) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&target.tab_id) else {
            return tab_not_found(id, &target.tab_id);
        };
        let terminal_ids = self.state.terminal_ids_for_tab(ws_idx, tab_idx);
        let pane_ids = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .map(|tab| tab.layout.pane_ids())
            .unwrap_or_default();
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return tab_not_found(id, &target.tab_id);
        };
        let workspace_id = ws.id.clone();
        if !ws.close_tab_allow_empty(tab_idx) {
            return encode_error(
                id,
                "tab_close_failed",
                format!("tab {} could not be closed", target.tab_id),
            );
        }
        for pane_id in pane_ids {
            self.state.plugin_panes.remove(&pane_id);
        }
        self.state.remove_unattached_terminal_ids(terminal_ids);
        self.shutdown_detached_terminal_runtimes();
        self.schedule_session_save();
        self.emit_event(EventEnvelope {
            event: EventKind::TabClosed,
            data: EventData::TabClosed {
                tab_id: target.tab_id,
                workspace_id,
            },
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_tab_close_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        target: TabTarget,
    ) -> String {
        let Some((ws_idx, tab_idx)) = self.parse_tab_id(&target.tab_id) else {
            return tab_not_found(id, &target.tab_id);
        };
        let terminal_ids = self.state.terminal_ids_for_tab(ws_idx, tab_idx);
        let pane_ids = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .map(|tab| tab.layout.pane_ids())
            .unwrap_or_default();
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return tab_not_found(id, &target.tab_id);
        };
        let workspace_id = ws.id.clone();
        if !ws.close_tab_allow_empty(tab_idx) {
            return encode_error(
                id,
                "tab_close_failed",
                format!("tab {} could not be closed", target.tab_id),
            );
        }
        for pane_id in pane_ids {
            self.state.plugin_panes.remove(&pane_id);
        }
        self.state.remove_unattached_terminal_ids(terminal_ids);
        self.shutdown_detached_terminal_runtimes();
        view.reconcile(&self.state);
        self.schedule_session_save();
        self.emit_event(EventEnvelope {
            event: EventKind::TabClosed,
            data: EventData::TabClosed {
                tab_id: target.tab_id,
                workspace_id,
            },
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(crate) fn tab_info_for_view(
        &self,
        view: &ClientViewState,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<TabInfo> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let tab = ws.tabs.get(tab_idx)?;
        let (agg_state, seen) = tab
            .panes
            .values()
            .filter_map(|pane| {
                self.state
                    .terminals
                    .get(&pane.attached_terminal_id)
                    .map(|terminal| (terminal.state, pane.seen))
            })
            .max_by_key(|(state, seen)| tab_attention_priority(*state, *seen))
            .unwrap_or((crate::detect::AgentState::Unknown, true));
        Some(TabInfo {
            tab_id: self.public_tab_id(ws_idx, tab_idx)?,
            workspace_id: self.public_workspace_id(ws_idx),
            number: tab_idx + 1,
            label: ws.tab_display_name(tab_idx)?,
            focused: view.active_workspace == Some(ws_idx)
                && view.active_tab_index_for_workspace(&self.state, ws_idx) == Some(tab_idx),
            pane_count: tab.panes.len(),
            agent_status: pane_agent_status(agg_state, seen),
        })
    }

    pub(crate) fn root_pane_info_for_view(
        &self,
        view: &ClientViewState,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<crate::api::schema::PaneInfo> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let tab = ws.tabs.get(tab_idx)?;
        self.pane_info_for_view(view, ws_idx, tab.root_pane)
    }

    pub(crate) fn tab_created_result_for_view(
        &self,
        view: &ClientViewState,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<ResponseResult> {
        Some(ResponseResult::TabCreated {
            tab: self.tab_info_for_view(view, ws_idx, tab_idx)?,
            root_pane: self.root_pane_info_for_view(view, ws_idx, tab_idx)?,
        })
    }
}

fn workspace_not_found(id: String, workspace_id: &str) -> String {
    encode_error(
        id,
        "workspace_not_found",
        format!("workspace {workspace_id} not found"),
    )
}

fn tab_not_found(id: String, tab_id: &str) -> String {
    encode_error(id, "tab_not_found", format!("tab {tab_id} not found"))
}

fn tab_creation_location(
    state: &crate::app::state::AppState,
    ws_idx: usize,
    focused_pane: Option<crate::layout::PaneId>,
    cwd: Option<String>,
    requested: Option<crate::api::schema::ResourceLocationParams>,
) -> Result<crate::execution_host::ResourceLocation, String> {
    let explicit = if let Some(requested) = requested {
        let host_id = crate::execution_host::ExecutionHostId::new(requested.execution_host_id)
            .map_err(|error| error.to_string())?;
        let path = crate::execution_host::HostPath::new(requested.path)
            .map_err(|error| error.to_string())?;
        Some(crate::execution_host::ResourceLocation::new(host_id, path))
    } else if let Some(cwd) = cwd {
        Some(
            crate::execution_host::ResourceLocation::local(cwd)
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let workspace = state
        .workspaces
        .get(ws_idx)
        .ok_or_else(|| "workspace not found".to_string())?;
    let focused_terminal = focused_pane.and_then(|pane_id| {
        let pane = workspace.pane_state(pane_id)?;
        state
            .terminals
            .get(&pane.attached_terminal_id)
            .map(|terminal| terminal.location.clone())
    });
    let workspace_default = workspace.default_location.clone();
    Ok(crate::execution_host::placement::resolve_tab_creation(
        explicit,
        focused_terminal,
        workspace_default,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, workspace::Workspace};

    fn app_with_workspace() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("tab-focus")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.ensure_test_terminals();
        app
    }

    #[tokio::test]
    async fn deferred_remote_tab_create_failure_clears_only_initiator_marker() {
        let mut app = app_with_workspace();
        let host_id = crate::execution_host::ExecutionHostId::new("ssh:fail-tab").unwrap();
        app.execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        let workspace_id = app.state.workspaces[0].id.clone();
        let mut initiator = ClientViewState::from_default_client_state(&app.state);
        initiator.active_workspace = Some(0);
        let mut other = ClientViewState::from_default_client_state(&app.state);
        other.active_workspace = Some(0);
        other.pending_active_tabs.insert(workspace_id.clone(), 99);

        let disposition = app.handle_tab_create_disposition_for_view(
            &mut initiator,
            "tab-remote-fail".into(),
            TabCreateParams {
                workspace_id: Some(workspace_id.clone()),
                cwd: None,
                location: Some(crate::api::schema::ResourceLocationParams {
                    execution_host_id: host_id.as_str().to_string(),
                    path: "/srv/fail-tab".into(),
                }),
                focus: true,
                label: None,
                env: Default::default(),
            },
        );
        let deferred = match disposition {
            crate::api::ApiRequestDisposition::Deferred(deferred) => deferred,
            other => panic!("expected deferred remote create, got {other:?}"),
        };
        let pending_idx = *initiator
            .pending_active_tabs
            .get(&workspace_id)
            .expect("initiator pending tab");
        assert_eq!(
            deferred.pending_focus,
            Some(crate::api::PendingFocusMarker::Tab {
                workspace_id: workspace_id.clone(),
                tab_idx: pending_idx,
            })
        );
        // Newer replacement index must survive exact cleanup.
        initiator
            .pending_active_tabs
            .insert(workspace_id.clone(), pending_idx + 5);

        let (respond_to, response_rx) = std::sync::mpsc::channel();
        let (terminal_id, pending) =
            crate::app::PendingRemoteApiResponse::from_deferred(deferred, respond_to);
        app.store_pending_remote_api_response(terminal_id.clone(), pending);
        app.complete_remote_creation_failed(terminal_id, "worker refused tab".into());
        assert!(app.finish_remote_api_completions());
        let _ = response_rx.try_recv().expect("failure response");

        let effects = app.take_client_view_effects();
        assert_eq!(effects.len(), 1);
        for effect in &effects {
            let _ = initiator.apply_client_view_effect(effect);
            let _ = other.apply_client_view_effect(effect);
        }
        assert_eq!(
            initiator.pending_active_tabs.get(&workspace_id).copied(),
            Some(pending_idx + 5),
            "newer replacement tab marker must survive"
        );
        assert_eq!(
            other.pending_active_tabs.get(&workspace_id).copied(),
            Some(99),
            "other client tab marker must stay"
        );
    }
}
