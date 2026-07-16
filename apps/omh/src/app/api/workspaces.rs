use std::path::PathBuf;

use crate::api::schema::{
    EventData, EventEnvelope, EventKind, ResponseResult, WorkspaceCreateParams, WorkspaceInfo,
    WorkspaceRenameParams, WorkspaceTarget,
};
use crate::app::api_helpers::pane_agent_status;
use crate::app::{App, ClientViewState, Mode};

use super::responses::{encode_error, encode_success};

impl App {
    pub(super) fn handle_workspace_list(&mut self, id: String) -> String {
        encode_success(
            id,
            ResponseResult::WorkspaceList {
                workspaces: self
                    .state
                    .workspaces
                    .iter()
                    .enumerate()
                    .map(|(idx, _)| self.workspace_info(idx))
                    .collect(),
            },
        )
    }

    fn workspace_info_for_view(&self, view: &ClientViewState, index: usize) -> WorkspaceInfo {
        let ws = &self.state.workspaces[index];
        let (agg_state, seen) = ws.aggregate_state(&self.state.terminals);
        let active_tab = view
            .active_tab_for_workspace(&ws.id)
            .unwrap_or(ws.active_tab);
        WorkspaceInfo {
            workspace_id: self.public_workspace_id(index),
            group_id: ws.group_id.clone(),
            number: index + 1,
            label: ws.display_name_from(&self.state.terminals, &self.terminal_runtimes),
            focused: view.active_workspace == Some(index),
            pane_count: ws.public_pane_numbers.len(),
            tab_count: ws.tabs.len(),
            active_tab_id: self
                .public_tab_id(index, active_tab)
                .unwrap_or_else(|| format!("{}:{}", ws.id, active_tab + 1)),
            agent_status: pane_agent_status(agg_state, seen),
            worktree: ws
                .worktree_space()
                .map(|space| crate::api::schema::WorkspaceWorktreeInfo {
                    repo_key: space.key.clone(),
                    repo_name: space.label.clone(),
                    repo_root: space.repo_root.display().to_string(),
                    checkout_path: space.checkout_path.display().to_string(),
                    is_linked_worktree: space.is_linked_worktree,
                }),
        }
    }

    pub(super) fn handle_workspace_get(&mut self, id: String, target: WorkspaceTarget) -> String {
        let Some(index) = self.parse_workspace_id(&target.workspace_id) else {
            return workspace_not_found(id, &target.workspace_id);
        };
        let Some(_) = self.state.workspaces.get(index) else {
            return workspace_not_found(id, &target.workspace_id);
        };

        encode_success(
            id,
            ResponseResult::WorkspaceInfo {
                workspace: self.workspace_info(index),
            },
        )
    }

    pub(super) fn handle_workspace_create(
        &mut self,
        id: String,
        params: WorkspaceCreateParams,
    ) -> String {
        let cwd = params.cwd.map(PathBuf::from).unwrap_or_else(|| {
            let follow_cwd = self.workspace_creation_source().and_then(|ws_idx| {
                self.focused_pane_cwd_in_workspace(ws_idx)
                    .or_else(|| self.seed_cwd_from_workspace(ws_idx))
            });
            self.resolve_new_terminal_cwd(follow_cwd)
        });
        let extra_env = match super::env::normalize_launch_env(params.env) {
            Ok(env) => env,
            Err((code, message)) => return encode_error(id, &code, message),
        };
        let should_focus = params.focus || self.state.active.is_none();
        match self.create_workspace_with_launch_env(cwd, should_focus, extra_env) {
            Ok(index) => {
                if let Some(label) = params.label {
                    if let Some(workspace) = self.state.workspaces.get_mut(index) {
                        workspace.set_custom_name(label);
                        crate::logging::workspace_renamed(&workspace.id);
                    }
                }
                let workspace = self.workspace_info(index);
                let tab = self
                    .tab_info(index, 0)
                    .expect("new workspace should have an initial tab");
                let root_pane = self
                    .root_pane_info(index, 0)
                    .expect("new workspace should have an initial root pane");
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
                encode_success(
                    id,
                    self.workspace_created_result(index)
                        .expect("new workspace should produce a complete create response"),
                )
            }
            Err(err) => encode_error(id, "workspace_create_failed", err.to_string()),
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

    pub(super) fn handle_workspace_create_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: WorkspaceCreateParams,
    ) -> String {
        view.reconcile(&self.state);
        let cwd = params.cwd.map(PathBuf::from).unwrap_or_else(|| {
            let follow_cwd = workspace_creation_source_for_view(&self.state, view)
                .and_then(|ws_idx| self.seed_cwd_from_workspace(ws_idx));
            self.resolve_new_terminal_cwd(follow_cwd)
        });
        let should_focus = params.focus || view.active_workspace.is_none();
        let label = params.label;
        let extra_env = match super::env::normalize_launch_env(params.env) {
            Ok(env) => env,
            Err((code, message)) => return encode_error(id, &code, message),
        };
        let ambient_focus = AmbientWorkspaceFocus::capture(self);
        match self.create_workspace_with_launch_env(cwd, false, extra_env) {
            Ok(index) => {
                ambient_focus.restore_if_valid(self);
                if should_focus {
                    focus_workspace_in_view(&self.state, view, index);
                } else {
                    view.reconcile(&self.state);
                }
                if let Some(label) = label {
                    if let Some(workspace) = self.state.workspaces.get_mut(index) {
                        workspace.set_custom_name(label);
                        crate::logging::workspace_renamed(&workspace.id);
                    }
                }
                let workspace = self.workspace_info_for_view(view, index);
                let tab = self
                    .tab_info(index, 0)
                    .expect("new workspace should have an initial tab");
                let root_pane = self
                    .pane_info_for_view(view, index, self.state.workspaces[index].tabs[0].root_pane)
                    .expect("new workspace should have an initial root pane");
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
                encode_success(
                    id,
                    ResponseResult::WorkspaceCreated {
                        workspace,
                        tab,
                        root_pane,
                    },
                )
            }
            Err(err) => {
                ambient_focus.restore_if_valid(self);
                encode_error(id, "workspace_create_failed", err.to_string())
            }
        }
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
                ws.tabs
                    .iter()
                    .flat_map(|tab| tab.layout.pane_ids())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let ambient_focus = AmbientWorkspaceFocus::capture(self);
        self.state.selected = index;
        self.state.close_selected_workspace();
        ambient_focus.restore_if_valid(self);
        view.reconcile(&self.state);
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
        let Some(index) = self.parse_workspace_id(&target.workspace_id) else {
            return workspace_not_found(id, &target.workspace_id);
        };
        if self.state.workspaces.get(index).is_none() {
            return workspace_not_found(id, &target.workspace_id);
        }
        self.state.switch_workspace(index);

        encode_success(
            id,
            ResponseResult::WorkspaceInfo {
                workspace: self.workspace_info(index),
            },
        )
    }

    pub(super) fn handle_workspace_rename(
        &mut self,
        id: String,
        params: WorkspaceRenameParams,
    ) -> String {
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
                workspace: self.workspace_info(index),
            },
        )
    }

    pub(super) fn handle_workspace_close(&mut self, id: String, target: WorkspaceTarget) -> String {
        let Some(index) = self.parse_workspace_id(&target.workspace_id) else {
            return workspace_not_found(id, &target.workspace_id);
        };
        if self.state.workspaces.get(index).is_none() {
            return workspace_not_found(id, &target.workspace_id);
        }
        let workspace_id = self.public_workspace_id(index);
        let workspace = self.workspace_info(index);
        let pane_ids = self
            .state
            .workspaces
            .get(index)
            .map(|ws| {
                ws.tabs
                    .iter()
                    .flat_map(|tab| tab.layout.pane_ids())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.state.selected = index;
        self.state.close_selected_workspace();
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
}

fn workspace_not_found(id: String, workspace_id: &str) -> String {
    encode_error(
        id,
        "workspace_not_found",
        format!("workspace {workspace_id} not found"),
    )
}

struct AmbientWorkspaceFocus {
    active: Option<usize>,
    selected: usize,
    active_group: usize,
    mode: Mode,
}

impl AmbientWorkspaceFocus {
    fn capture(app: &App) -> Self {
        Self {
            active: app.state.active,
            selected: app.state.selected,
            active_group: app.state.active_group,
            mode: app.state.mode,
        }
    }

    fn restore_if_valid(self, app: &mut App) {
        if self
            .active
            .is_some_and(|idx| idx >= app.state.workspaces.len())
        {
            return;
        }
        if self.selected >= app.state.workspaces.len() && !app.state.workspaces.is_empty() {
            return;
        }
        if self.active_group >= app.state.groups.len() && !app.state.groups.is_empty() {
            return;
        }
        app.state.active = self.active;
        app.state.selected = self
            .selected
            .min(app.state.workspaces.len().saturating_sub(1));
        app.state.active_group = self
            .active_group
            .min(app.state.groups.len().saturating_sub(1));
        app.state.mode = self.mode;
    }
}

fn focus_workspace_in_view(state: &crate::app::AppState, view: &mut ClientViewState, index: usize) {
    if let Some(workspace) = state.workspaces.get(index) {
        view.active_workspace = Some(index);
        view.selected_workspace = index;
        if let Some(group_idx) = state
            .groups
            .iter()
            .position(|group| group.id == workspace.group_id)
        {
            view.active_group = group_idx;
        }
        view.mode = Mode::Terminal;
        view.reconcile(state);
    }
}

fn workspace_creation_source_for_view(
    state: &crate::app::AppState,
    view: &ClientViewState,
) -> Option<usize> {
    if view.mode == Mode::Navigate
        && state.workspaces.get(view.selected_workspace).is_some()
        && workspace_in_view_group(state, view, view.selected_workspace)
    {
        return Some(view.selected_workspace);
    }

    view.active_workspace
        .filter(|idx| workspace_in_view_group(state, view, *idx))
        .or_else(|| {
            state
                .workspaces
                .get(view.selected_workspace)
                .filter(|_| workspace_in_view_group(state, view, view.selected_workspace))
                .map(|_| view.selected_workspace)
        })
}

fn workspace_in_view_group(
    state: &crate::app::AppState,
    view: &ClientViewState,
    index: usize,
) -> bool {
    if !view.group_filter_enabled {
        return state.workspaces.get(index).is_some();
    }

    let active_group_id = state
        .groups
        .get(view.active_group)
        .map(|group| group.id.as_str())
        .unwrap_or(crate::workspace::DEFAULT_GROUP_ID);
    state
        .workspaces
        .get(index)
        .is_some_and(|workspace| workspace.group_id == active_group_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::schema::SuccessResponse, config::Config, workspace::Workspace};

    fn app_with_linked_worktree() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("issue")];
        app.state.workspaces[0].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "omh".into(),
            repo_root: "/repo/omh".into(),
            checkout_path: "/repo/omh-issue".into(),
            is_linked_worktree: true,
        });
        app
    }

    #[test]
    fn api_workspace_close_closes_linked_worktree_workspace_only() {
        let mut app = app_with_linked_worktree();

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
    fn api_workspace_close_event_includes_final_worktree_snapshot() {
        let event_hub = crate::api::EventHub::default();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(&Config::default(), true, None, api_rx, event_hub.clone());
        app.state.workspaces = app_with_linked_worktree().state.workspaces;
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
                    && workspace
                        .worktree
                        .as_ref()
                        .is_some_and(|worktree| worktree.is_linked_worktree)
            )
        }));
    }
}
