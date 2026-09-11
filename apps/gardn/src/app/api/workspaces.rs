use crate::api::schema::{
    EventData, EventEnvelope, EventKind, ResponseResult, WorkspaceCreateParams, WorkspaceInfo,
    WorkspaceRenameParams, WorkspaceTarget,
};
use crate::app::api_helpers::pane_agent_status;
use crate::app::{App, ClientViewState, Mode};

use super::responses::{encode_error, encode_success};

impl App {
    pub(super) fn handle_workspace_list(&mut self, id: String) -> String {
        self.with_default_client_view(|app, view| app.handle_workspace_list_for_view(view, id))
    }

    pub(crate) fn workspace_info_for_view(
        &self,
        view: &ClientViewState,
        index: usize,
    ) -> WorkspaceInfo {
        let ws = &self.state.workspaces[index];
        let (agg_state, seen) = ws.aggregate_state(&self.state.terminals);
        let active_tab = view
            .active_tab_index_for_workspace(&self.state, index)
            .unwrap_or(0);
        WorkspaceInfo {
            workspace_id: self.public_workspace_id(index),
            group_id: ws.group_id.clone(),
            default_location: crate::api::schema::resource_location_params_from(
                &ws.default_location,
            ),
            number: index + 1,
            label: ws.display_name_from(&self.state.terminals, &self.terminal_runtimes),
            focused: view.active_workspace == Some(index),
            pane_count: ws.public_pane_numbers.len(),
            tab_count: ws.tabs.len(),
            active_tab_id: self.public_tab_id(index, active_tab).unwrap_or_else(|| {
                let number = ws
                    .tabs
                    .get(active_tab)
                    .map(|tab| tab.number())
                    .unwrap_or(active_tab + 1);
                format!("{}:{}", ws.id, number)
            }),
            agent_status: pane_agent_status(agg_state, seen),
        }
    }

    pub(super) fn handle_workspace_get(&mut self, id: String, target: WorkspaceTarget) -> String {
        self.with_default_client_view(|app, view| {
            app.handle_workspace_get_for_view(view, id, target)
        })
    }

    pub(super) fn handle_workspace_create_disposition(
        &mut self,
        id: String,
        params: WorkspaceCreateParams,
    ) -> crate::api::ApiRequestDisposition {
        self.with_default_client_view(move |app, view| {
            app.handle_workspace_create_with(
                super::invocation::ApiInvocationContext::ambient(view),
                id,
                params,
            )
        })
    }

    pub(super) fn handle_workspace_create_with(
        &mut self,
        mut invocation: super::invocation::ApiInvocationContext<'_>,
        id: String,
        params: WorkspaceCreateParams,
    ) -> crate::api::ApiRequestDisposition {
        invocation.view_mut().reconcile(&self.state);
        if params.cwd.is_some() && params.location.is_some() {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "invalid_params",
                "cwd and location cannot be used together".to_string(),
            ));
        }
        let explicit = match explicit_workspace_location(params.cwd, params.location) {
            Ok(location) => location,
            Err(error) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "invalid_params",
                    error,
                ))
            }
        };
        let group_id = self
            .state
            .groups
            .get(invocation.view().active_group)
            .or_else(|| self.state.groups.first())
            .map(|group| group.id.clone())
            .unwrap_or_default();
        let group_default = self.group_default_location(&group_id);
        let local_fallback = match crate::execution_host::ResourceLocation::local(
            self.resolve_new_terminal_cwd(None),
        ) {
            Ok(location) => location,
            Err(error) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "invalid_params",
                    error.to_string(),
                ))
            }
        };
        let location = crate::execution_host::placement::resolve_workspace_creation(
            explicit,
            group_default,
            local_fallback,
        );
        let extra_env = match super::env::normalize_launch_env(params.env) {
            Ok(env) => env,
            Err((code, message)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(id, &code, message))
            }
        };
        let should_focus = params.focus || invocation.view().active_workspace.is_none();
        let label = params.label;
        let begin_focus = false;
        if !location.is_local() {
            match self.begin_remote_workspace(location, begin_focus, group_id, None, extra_env) {
                Ok(terminal_id) => {
                    let mut pending_focus = None;
                    if should_focus {
                        if let Some(workspace_id) = self.pending_remote_workspace_id(&terminal_id) {
                            invocation.view_mut().pending_active_workspace =
                                Some(workspace_id.clone());
                            pending_focus =
                                Some(crate::api::PendingFocusMarker::Workspace { workspace_id });
                        }
                    }
                    return crate::api::ApiRequestDisposition::Deferred(
                        crate::api::DeferredRemoteCreate {
                            terminal_id,
                            request_id: id,
                            kind: crate::api::DeferredRemoteCreateKind::WorkspaceCreate { label },
                            focus: should_focus,
                            client_view_id: Some(invocation.client_view_id()),
                            pending_focus,
                        },
                    );
                }
                Err(err) => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "workspace_create_failed",
                        err,
                    ));
                }
            }
        }
        let result = self
            .create_workspace_with_launch_env_in_group(
                location.path.as_path().to_path_buf(),
                begin_focus,
                group_id,
                extra_env,
            )
            .map_err(|error| error.to_string());
        match result {
            Ok(index) => {
                if should_focus {
                    focus_workspace_in_view(&self.state, invocation.view_mut(), index);
                } else {
                    invocation.view_mut().reconcile(&self.state);
                }
                if let Some(label) = label {
                    if let Some(workspace) = self.state.workspaces.get_mut(index) {
                        workspace.set_custom_name(label);
                        crate::logging::workspace_renamed(&workspace.id);
                    }
                }
                let workspace = self.workspace_info_for_view(invocation.view(), index);
                let Some(tab) = self.tab_info_for_view(invocation.view(), index, 0) else {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "workspace_create_failed",
                        "new workspace initial tab unavailable",
                    ));
                };
                let Some(root_pane_id) = self
                    .state
                    .workspaces
                    .get(index)
                    .and_then(|workspace| workspace.terminal_tab(0).ok())
                    .map(|tab| tab.root_pane)
                else {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "workspace_create_failed",
                        "new workspace initial terminal tab unavailable",
                    ));
                };
                let Some(root_pane) =
                    self.pane_info_for_view(invocation.view(), index, root_pane_id)
                else {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "workspace_create_failed",
                        "new workspace initial pane unavailable",
                    ));
                };
                self.emit_event(EventEnvelope {
                    event: EventKind::WorkspaceCreated,
                    data: EventData::WorkspaceCreated {
                        workspace: workspace.clone(),
                    },
                });
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
                crate::api::ApiRequestDisposition::Respond(encode_success(
                    id,
                    ResponseResult::WorkspaceCreated {
                        workspace,
                        tab,
                        root_pane,
                    },
                ))
            }
            Err(err) => crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "workspace_create_failed",
                err,
            )),
        }
    }

    pub(super) fn handle_workspace_list_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
    ) -> String {
        view.reconcile(&self.state);
        encode_success(
            id,
            ResponseResult::WorkspaceList {
                workspaces: self
                    .state
                    .workspaces
                    .iter()
                    .enumerate()
                    .map(|(idx, _)| self.workspace_info_for_view(view, idx))
                    .collect(),
            },
        )
    }

    pub(super) fn handle_workspace_get_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        target: WorkspaceTarget,
    ) -> String {
        view.reconcile(&self.state);
        let Some(index) = self.parse_workspace_id(&target.workspace_id) else {
            return workspace_not_found(id, &target.workspace_id);
        };
        let Some(_) = self.state.workspaces.get(index) else {
            return workspace_not_found(id, &target.workspace_id);
        };

        encode_success(
            id,
            ResponseResult::WorkspaceInfo {
                workspace: self.workspace_info_for_view(view, index),
            },
        )
    }

    pub(super) fn handle_workspace_focus_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        target: WorkspaceTarget,
    ) -> String {
        view.reconcile(&self.state);
        let Some(index) = self.parse_workspace_id(&target.workspace_id) else {
            return workspace_not_found(id, &target.workspace_id);
        };
        if self.state.workspaces.get(index).is_none() {
            return workspace_not_found(id, &target.workspace_id);
        }
        focus_workspace_in_view(&self.state, view, index);

        encode_success(
            id,
            ResponseResult::WorkspaceInfo {
                workspace: self.workspace_info_for_view(view, index),
            },
        )
    }

    pub(super) fn handle_workspace_rename_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: WorkspaceRenameParams,
    ) -> String {
        view.reconcile(&self.state);
        let Some(index) = self.parse_workspace_id(&params.workspace_id) else {
            return workspace_not_found(id, &params.workspace_id);
        };
        let Some(ws) = self.state.workspaces.get_mut(index) else {
            return workspace_not_found(id, &params.workspace_id);
        };
        ws.set_custom_name(params.label.clone());
        crate::logging::workspace_renamed(&ws.id);
        self.schedule_session_save();
        self.emit_event(EventEnvelope {
            event: EventKind::WorkspaceRenamed,
            data: EventData::WorkspaceRenamed {
                workspace_id: self.public_workspace_id(index),
                label: params.label,
            },
        });

        encode_success(
            id,
            ResponseResult::WorkspaceInfo {
                workspace: self.workspace_info_for_view(view, index),
            },
        )
    }

    pub(super) fn handle_workspace_close_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        target: WorkspaceTarget,
    ) -> String {
        view.reconcile(&self.state);
        let Some(index) = self.parse_workspace_id(&target.workspace_id) else {
            return workspace_not_found(id, &target.workspace_id);
        };
        if self.state.workspaces.get(index).is_none() {
            return workspace_not_found(id, &target.workspace_id);
        }
        let workspace_id = self.public_workspace_id(index);
        let workspace = self.workspace_info_for_view(view, index);
        let pane_ids = self
            .state
            .workspaces
            .get(index)
            .map(|ws| {
                ws.terminal_tabs()
                    .flat_map(|(_, tab)| tab.layout.pane_ids())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.close_workspace_for_client_view(view, index);
        for pane_id in pane_ids {
            self.state.plugin_panes.remove(&pane_id);
        }
        self.shutdown_detached_terminal_runtimes();
        self.emit_event(EventEnvelope {
            event: EventKind::WorkspaceClosed,
            data: EventData::WorkspaceClosed {
                workspace_id,
                workspace: Some(workspace),
            },
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_workspace_focus(&mut self, id: String, target: WorkspaceTarget) -> String {
        self.with_default_client_view(|app, view| {
            app.handle_workspace_focus_for_view(view, id, target)
        })
    }

    pub(super) fn handle_workspace_rename(
        &mut self,
        id: String,
        params: WorkspaceRenameParams,
    ) -> String {
        self.with_default_client_view(|app, view| {
            app.handle_workspace_rename_for_view(view, id, params)
        })
    }

    pub(super) fn handle_workspace_close(&mut self, id: String, target: WorkspaceTarget) -> String {
        self.with_default_client_view(|app, view| {
            app.handle_workspace_close_for_view(view, id, target)
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

fn focus_workspace_in_view(state: &crate::app::AppState, view: &mut ClientViewState, index: usize) {
    view.reconcile(state);
    if let Some(workspace) = state.workspaces.get(index) {
        view.active_workspace = Some(index);
        view.selected_workspace = index;
        if let Some(group_idx) = state
            .groups
            .iter()
            .position(|group| group.id == workspace.group_id)
        {
            view.select_group(state, group_idx);
        }
        view.mode = Mode::Terminal;
        view.reconcile(state);
    }
}

fn explicit_workspace_location(
    cwd: Option<String>,
    requested: Option<crate::api::schema::ResourceLocationParams>,
) -> Result<Option<crate::execution_host::ResourceLocation>, String> {
    if let Some(requested) = requested {
        let host_id = crate::execution_host::ExecutionHostId::new(requested.execution_host_id)
            .map_err(|error| error.to_string())?;
        let path = crate::execution_host::HostPath::new(requested.path)
            .map_err(|error| error.to_string())?;
        return Ok(Some(crate::execution_host::ResourceLocation::new(
            host_id, path,
        )));
    }
    cwd.map(crate::execution_host::ResourceLocation::local)
        .transpose()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::api::invocation::{ApiInvocationContext, ApiInvocationOrigin};
    use crate::{api::schema::SuccessResponse, config::Config, workspace::Workspace};

    fn app_with_workspace() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("issue")];
        app.default_client_view = ClientViewState::from_default_client_state(&app.state);
        app
    }

    #[test]
    fn api_workspace_close_closes_workspace() {
        let mut app = app_with_workspace();

        let response = app.handle_workspace_close(
            "req".into(),
            WorkspaceTarget {
                workspace_id: app.state.workspaces[0].id.clone(),
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(success.id, "req");
        assert!(app.state.workspaces.is_empty());
    }

    #[test]
    fn api_workspace_close_event_includes_final_snapshot() {
        let event_hub = crate::api::EventHub::default();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(&Config::default(), true, None, api_rx, event_hub.clone());
        app.state.workspaces = app_with_workspace().state.workspaces;
        app.default_client_view = ClientViewState::from_default_client_state(&app.state);
        let workspace_id = app.state.workspaces[0].id.clone();

        let response = app.handle_workspace_close(
            "req".into(),
            WorkspaceTarget {
                workspace_id: workspace_id.clone(),
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(success.id, "req");
        let events = event_hub.events_after(0);
        assert!(events.iter().any(|(_, event)| {
            matches!(
                &event.data,
                EventData::WorkspaceClosed {
                    workspace_id: closed_id,
                    workspace: Some(workspace),
                } if closed_id == &workspace_id
                    && workspace.workspace_id == workspace_id
            )
        }));
    }

    #[test]
    fn api_workspace_close_of_earlier_workspace_keeps_focused_identity() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.state.ensure_test_terminals();
        let focused_id = app.state.workspaces[1].id.clone();
        let closing_id = app.state.workspaces[0].id.clone();

        let mut view = ClientViewState::from_default_client_state(&app.state);
        view.active_workspace = Some(1);
        view.selected_workspace = 1;
        view.mode = Mode::Navigate;
        view.reconcile(&app.state);

        let response = app.handle_workspace_close_for_view(
            &mut view,
            "req".into(),
            WorkspaceTarget {
                workspace_id: closing_id,
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(success.id, "req");
        assert_eq!(app.state.workspaces.len(), 2);
        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 0);
        assert_eq!(
            app.state.workspaces[view.active_workspace.unwrap()].id,
            focused_id
        );
        assert_eq!(view.mode, Mode::Navigate);
    }

    #[tokio::test]
    async fn workspace_create_inherits_active_group_default_location() {
        let mut app = app_with_workspace();
        let group_default_path = std::env::temp_dir().join("gardn-group-default-inherit");
        std::fs::create_dir_all(&group_default_path).expect("create group default path");
        let group_default = crate::execution_host::ResourceLocation::local(&group_default_path)
            .expect("local group default");
        let group_idx = app.state.create_group_with_icon_and_default_location(
            "remote-side".into(),
            crate::app::state::DEFAULT_GROUP_ICON.into(),
            Some(group_default.clone()),
        );
        app.default_client_view.select_group(&app.state, group_idx);
        app.default_client_view.group_filter_enabled = true;
        let group_id = app.state.groups[group_idx].id.clone();
        let workspaces_before = app.state.workspaces.len();

        let disposition = app.handle_workspace_create_disposition(
            "ws-create-inherit".into(),
            WorkspaceCreateParams {
                cwd: None,
                location: None,
                focus: false,
                label: Some("inherited".into()),
                env: Default::default(),
            },
        );

        let response = match disposition {
            crate::api::ApiRequestDisposition::Respond(response) => response,
            other => panic!("expected immediate workspace create response, got {other:?}"),
        };
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(success.id, "ws-create-inherit");
        assert_eq!(app.state.workspaces.len(), workspaces_before + 1);
        let created = app
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.custom_name.as_deref() == Some("inherited"))
            .expect("created workspace");
        assert_eq!(created.group_id, group_id);
        assert_eq!(created.default_location, group_default);
    }

    #[tokio::test]
    async fn ambient_and_for_view_workspace_create_share_local_semantics() {
        let mut ambient_app = app_with_workspace();
        let mut view_app = app_with_workspace();
        let group_default_path = std::env::temp_dir().join("gardn-group-default-shared-semantics");
        std::fs::create_dir_all(&group_default_path).expect("create group default path");
        let group_default = crate::execution_host::ResourceLocation::local(&group_default_path)
            .expect("local group default");

        for app in [&mut ambient_app, &mut view_app] {
            let group_idx = app.state.create_group_with_icon_and_default_location(
                "shared-side".into(),
                crate::app::state::DEFAULT_GROUP_ICON.into(),
                Some(group_default.clone()),
            );
            app.state.workspaces[0].group_id = app.state.groups[group_idx].id.clone();
            app.default_client_view.select_group(&app.state, group_idx);
            app.default_client_view.group_filter_enabled = true;
        }

        let params = WorkspaceCreateParams {
            cwd: None,
            location: None,
            focus: false,
            label: Some("shared-impl".into()),
            env: Default::default(),
        };
        let ambient = match ambient_app
            .handle_workspace_create_disposition("ambient".into(), params.clone())
        {
            crate::api::ApiRequestDisposition::Respond(response) => response,
            other => panic!("expected ambient respond, got {other:?}"),
        };
        let mut view = view_app
            .default_client_view
            .fork_for_attached_client(&view_app.state);
        let for_view = match view_app.handle_workspace_create_with(
            ApiInvocationContext::with_origin(ApiInvocationOrigin::ClientLocal, &mut view),
            "for-view".into(),
            params,
        ) {
            crate::api::ApiRequestDisposition::Respond(response) => response,
            other => panic!("expected for_view respond, got {other:?}"),
        };

        let ambient_body: serde_json::Value = serde_json::from_str(&ambient).expect("json");
        let view_body: serde_json::Value = serde_json::from_str(&for_view).expect("json");
        assert_eq!(
            ambient_body["result"]["type"], view_body["result"]["type"],
            "ambient and for_view must share success shape"
        );
        assert_eq!(
            ambient_body["result"]["workspace"]["label"],
            view_body["result"]["workspace"]["label"]
        );
        assert_eq!(
            ambient_body["result"]["workspace"]["default_location"],
            view_body["result"]["workspace"]["default_location"]
        );
        assert_eq!(ambient_body["result"]["workspace"]["focused"], false);
        assert_eq!(view_body["result"]["workspace"]["focused"], false);
        assert!(
            ambient_body["result"]["workspace"]
                .get("default_execution_host_id")
                .is_none(),
            "host id must come only from default_location"
        );
        assert!(view_body["result"]["workspace"]
            .get("default_execution_host_id")
            .is_none());

        let ambient_created = ambient_app
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.custom_name.as_deref() == Some("shared-impl"))
            .expect("ambient workspace");
        let view_created = view_app
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.custom_name.as_deref() == Some("shared-impl"))
            .expect("view workspace");
        assert_eq!(ambient_created.default_location, group_default);
        assert_eq!(view_created.default_location, group_default);
        assert_eq!(
            ambient_created.group_id,
            ambient_app.state.groups[ambient_app.default_client_view.active_group].id
        );
        assert_eq!(
            view_created.group_id,
            view_app.state.groups[view.active_group].id
        );
    }

    #[tokio::test]
    async fn deferred_remote_workspace_create_focus_true_only_initiator_after_ack() {
        let mut app = app_with_workspace();
        app.state.ensure_test_terminals();

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:focus-ws").unwrap();
        app.execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        let mut initiator = ClientViewState::from_default_client_state(&app.state);
        initiator.active_workspace = Some(0);
        initiator.selected_workspace = 0;
        let mut other = ClientViewState::from_default_client_state(&app.state);
        other.active_workspace = Some(0);
        other.selected_workspace = 0;
        other.reconcile(&app.state);

        let ambient_active = app.default_client_view.active_workspace;
        let disposition = app.handle_workspace_create_with(
            ApiInvocationContext::with_origin(ApiInvocationOrigin::ClientLocal, &mut initiator),
            "ws-remote-focus".into(),
            WorkspaceCreateParams {
                cwd: None,
                location: Some(crate::api::schema::ResourceLocationParams {
                    execution_host_id: host_id.as_str().to_string(),
                    path: "/srv/focus".into(),
                }),
                focus: true,
                label: Some("remote-focused".into()),
                env: Default::default(),
            },
        );
        let deferred = match disposition {
            crate::api::ApiRequestDisposition::Deferred(deferred) => deferred,
            other => panic!("expected deferred remote create, got {other:?}"),
        };
        assert!(
            initiator.pending_active_workspace.is_some(),
            "initiator should keep pending workspace focus until ACK"
        );
        assert!(other.pending_active_workspace.is_none());
        assert_eq!(app.default_client_view.active_workspace, ambient_active);
        assert_eq!(app.state.workspaces.len(), 1);

        let (respond_to, response_rx) = std::sync::mpsc::channel();
        let (terminal_id, pending) =
            crate::app::PendingRemoteApiResponse::from_deferred(deferred, respond_to);
        app.store_pending_remote_api_response(terminal_id.clone(), pending);

        let resolved = crate::execution_host::ResourceLocation::new(
            host_id,
            crate::execution_host::HostPath::new("/srv/focus").unwrap(),
        );
        app.complete_remote_creation_ready(
            terminal_id,
            crate::execution_host::protocol::RuntimeIdentity::new(
                crate::execution_host::protocol::HostBindingGeneration::new(1),
                crate::execution_host::protocol::WorkerInstanceId::new("worker-a").unwrap(),
                crate::execution_host::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
                crate::execution_host::protocol::RuntimeIncarnation::new(1),
            ),
            resolved,
        );
        assert!(app.finish_remote_api_completions());

        let response = response_rx
            .try_recv()
            .expect("ACK must deliver one API success");
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(body["result"]["type"], "workspace_created");
        assert_eq!(body["result"]["workspace"]["focused"], true);

        // The default view stays put; initiator pending focus resolves on reconcile.
        assert_eq!(app.default_client_view.active_workspace, ambient_active);
        assert_eq!(app.state.workspaces.len(), 2);
        initiator.reconcile(&app.state);
        other.reconcile(&app.state);
        assert_eq!(initiator.active_workspace, Some(1));
        assert_eq!(other.active_workspace, Some(0));
        assert!(initiator.pending_active_workspace.is_none());
    }

    #[tokio::test]
    async fn deferred_remote_workspace_create_focus_false_changes_neither_client() {
        let mut app = app_with_workspace();
        app.state.ensure_test_terminals();

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:nofocus-ws").unwrap();
        app.execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        let mut initiator = ClientViewState::from_default_client_state(&app.state);
        let mut other = ClientViewState::from_default_client_state(&app.state);
        initiator.active_workspace = Some(0);
        other.active_workspace = Some(0);

        let disposition = app.handle_workspace_create_with(
            ApiInvocationContext::with_origin(ApiInvocationOrigin::ClientLocal, &mut initiator),
            "ws-remote-nofocus".into(),
            WorkspaceCreateParams {
                cwd: None,
                location: Some(crate::api::schema::ResourceLocationParams {
                    execution_host_id: host_id.as_str().to_string(),
                    path: "/srv/nofocus".into(),
                }),
                focus: false,
                label: None,
                env: Default::default(),
            },
        );
        let deferred = match disposition {
            crate::api::ApiRequestDisposition::Deferred(deferred) => deferred,
            other => panic!("expected deferred remote create, got {other:?}"),
        };
        assert!(initiator.pending_active_workspace.is_none());

        let (respond_to, response_rx) = std::sync::mpsc::channel();
        let (terminal_id, pending) =
            crate::app::PendingRemoteApiResponse::from_deferred(deferred, respond_to);
        app.store_pending_remote_api_response(terminal_id.clone(), pending);
        app.complete_remote_creation_ready(
            terminal_id,
            crate::execution_host::protocol::RuntimeIdentity::new(
                crate::execution_host::protocol::HostBindingGeneration::new(1),
                crate::execution_host::protocol::WorkerInstanceId::new("worker-b").unwrap(),
                crate::execution_host::protocol::WorkerRuntimeId::new("runtime-b").unwrap(),
                crate::execution_host::protocol::RuntimeIncarnation::new(1),
            ),
            crate::execution_host::ResourceLocation::new(
                host_id,
                crate::execution_host::HostPath::new("/srv/nofocus").unwrap(),
            ),
        );
        assert!(app.finish_remote_api_completions());
        let response = response_rx.try_recv().expect("success response");
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(body["result"]["workspace"]["focused"], false);

        initiator.reconcile(&app.state);
        other.reconcile(&app.state);
        assert_eq!(initiator.active_workspace, Some(0));
        assert_eq!(other.active_workspace, Some(0));
        assert_eq!(app.default_client_view.active_workspace, Some(0));
    }

    #[tokio::test]
    async fn deferred_remote_workspace_create_failure_clears_only_initiator_marker() {
        let mut app = app_with_workspace();
        app.state.ensure_test_terminals();

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:fail-ws").unwrap();
        app.execution_hosts
            .as_mut()
            .unwrap()
            .connect_test_host(host_id.clone());

        let mut initiator = ClientViewState::from_default_client_state(&app.state);
        initiator.active_workspace = Some(0);
        initiator.selected_workspace = 0;
        let mut other = ClientViewState::from_default_client_state(&app.state);
        other.active_workspace = Some(0);
        other.selected_workspace = 0;
        // Simulate another client's independent pending focus on a different workspace id.
        other.pending_active_workspace = Some("other-client-marker".into());

        let disposition = app.handle_workspace_create_with(
            ApiInvocationContext::with_origin(ApiInvocationOrigin::ClientLocal, &mut initiator),
            "ws-remote-fail".into(),
            WorkspaceCreateParams {
                cwd: None,
                location: Some(crate::api::schema::ResourceLocationParams {
                    execution_host_id: host_id.as_str().to_string(),
                    path: "/srv/fail".into(),
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
        let initiator_marker = initiator
            .pending_active_workspace
            .clone()
            .expect("initiator pending workspace marker");
        assert_eq!(
            deferred.pending_focus,
            Some(crate::api::PendingFocusMarker::Workspace {
                workspace_id: initiator_marker.clone(),
            })
        );
        // Replacement marker must survive cleanup of the failed create.
        initiator.pending_active_workspace = Some("newer-replacement".into());

        let (respond_to, response_rx) = std::sync::mpsc::channel();
        let (terminal_id, pending) =
            crate::app::PendingRemoteApiResponse::from_deferred(deferred, respond_to);
        app.store_pending_remote_api_response(terminal_id.clone(), pending);
        app.complete_remote_creation_failed(terminal_id, "worker refused".into());
        assert!(app.finish_remote_api_completions());
        let _ = response_rx.try_recv().expect("failure response");

        let effects = app.take_client_view_effects();
        assert_eq!(effects.len(), 1);
        for effect in &effects {
            let _ = initiator.apply_client_view_effect(effect);
            let _ = other.apply_client_view_effect(effect);
        }
        assert_eq!(
            initiator.pending_active_workspace.as_deref(),
            Some("newer-replacement"),
            "newer replacement marker must not be cleared"
        );
        assert_eq!(
            other.pending_active_workspace.as_deref(),
            Some("other-client-marker"),
            "other client markers must stay untouched"
        );
    }
}
