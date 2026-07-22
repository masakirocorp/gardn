use bytes::Bytes;

use crate::api::schema::{
    EventData, EventEnvelope, EventKind, PaneClearAgentAuthorityParams, PaneCurrentParams,
    PaneDirection, PaneEdgesParams, PaneEdgesResult, PaneFocusDirectionParams,
    PaneFocusDirectionReason, PaneFocusDirectionResult, PaneLayoutPane, PaneLayoutParams,
    PaneLayoutRect, PaneLayoutSnapshot, PaneLayoutSplit, PaneListParams, PaneMoveDestination,
    PaneMoveParams, PaneMoveReason, PaneMoveResult, PaneNeighborParams, PaneNeighborResult,
    PaneProcessInfo, PaneProcessInfoParams, PaneProcessInfoProcess, PaneReadParams, PaneReadResult,
    PaneReleaseAgentParams, PaneRenameParams, PaneReportAgentParams, PaneReportAgentSessionParams,
    PaneReportMetadataParams, PaneResizeParams, PaneResizeReason, PaneResizeResult,
    PaneSendInputParams, PaneSendKeysParams, PaneSendTextParams, PaneSplitParams, PaneSwapParams,
    PaneSwapReason, PaneSwapResult, PaneTarget, PaneZoomMode, PaneZoomParams, PaneZoomReason,
    PaneZoomResult, ReadFormat, ReadSource, ResponseResult, SplitDirection,
};
use crate::app::{view_state::ClientViewState, App, Mode};
use crate::layout::{find_in_direction, NavDirection, Node, PaneId, TileLayout};

use super::super::api_helpers::{
    detect_state_from_api, encode_api_keys, encode_api_text, normalize_custom_status,
    normalize_metadata_tokens, normalize_reported_agent_label,
    MAX_METADATA_TOKEN_KEYS_PER_RESOURCE,
};
use super::responses::{encode_error, encode_success};

const METADATA_SOURCE_MAX_CHARS: usize = 80;
const METADATA_TTL_MIN_MS: u64 = 1;
const METADATA_TTL_MAX_MS: u64 = 86_400_000;

impl App {
    pub(super) fn handle_pane_focus(&mut self, id: String, target: PaneTarget) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&target.pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };
        let Some(tab_idx) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.find_tab_index_for_pane(pane_id))
        else {
            return pane_not_found(id, &target.pane_id);
        };
        self.state.focus_pane_in_workspace(ws_idx, pane_id);
        self.state.mark_active_tab_seen();
        self.state.switch_workspace_tab(ws_idx, tab_idx);
        self.state.mode = Mode::Terminal;
        self.schedule_session_save();
        let Some(pane) = self.pane_info(ws_idx, pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };
        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_focus_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        target: PaneTarget,
    ) -> String {
        view.reconcile(&self.state);
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, Some(&target.pane_id))
        else {
            return pane_not_found(id, &target.pane_id);
        };
        let Some(tab_idx) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.find_tab_index_for_pane(pane_id))
        else {
            return pane_not_found(id, &target.pane_id);
        };
        view.focus_pane_in_workspace(&self.state, ws_idx, tab_idx, pane_id);
        view.active_workspace = Some(ws_idx);
        view.selected_workspace = ws_idx;
        view.mode = Mode::Terminal;
        let Some(pane) = self.pane_info(ws_idx, pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };
        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_split(&mut self, id: String, params: PaneSplitParams) -> String {
        let target = match params.target_pane_id.as_deref() {
            Some(target_pane_id) => self.parse_pane_id(target_pane_id),
            None => match params.workspace_id.as_deref() {
                Some(workspace_id) => self.parse_workspace_id(workspace_id).and_then(|ws_idx| {
                    let pane_id = self.state.workspaces.get(ws_idx)?.focused_pane_id()?;
                    Some((ws_idx, pane_id))
                }),
                None => self.resolve_optional_pane(None),
            },
        };
        let Some((ws_idx, target_pane_id)) = target else {
            return pane_not_found(
                id,
                params.target_pane_id.as_deref().unwrap_or("active pane"),
            );
        };
        let extra_env = match super::env::normalize_launch_env(params.env) {
            Ok(env) => env,
            Err((code, message)) => return encode_error(id, &code, message),
        };
        let (rows, cols) = self.state.estimate_pane_size();
        let split_cwd = params.cwd.map(std::path::PathBuf::from).or_else(|| {
            self.state.workspaces.get(ws_idx).and_then(|ws| {
                let tab_idx = ws.find_tab_index_for_pane(target_pane_id)?;
                ws.tabs.get(tab_idx)?.cwd_for_pane(
                    target_pane_id,
                    &self.state.terminals,
                    &self.terminal_runtimes,
                )
            })
        });
        let default_shell = self.state.default_shell.clone();
        let scrollback_limit_bytes = self.state.pane_scrollback_limit_bytes;
        let host_terminal_theme = self.state.host_terminal_theme;
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return pane_not_found(
                id,
                params.target_pane_id.as_deref().unwrap_or("active pane"),
            );
        };
        let direction = match params.direction {
            crate::api::schema::SplitDirection::Right => ratatui::layout::Direction::Horizontal,
            crate::api::schema::SplitDirection::Down => ratatui::layout::Direction::Vertical,
        };

        let shell_config = crate::pane::PaneShellConfig::new(&default_shell, self.state.shell_mode);
        let split_result = match params.ratio {
            Some(ratio) => ws.split_pane_with_ratio(
                target_pane_id,
                direction,
                ratio,
                rows,
                cols,
                split_cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
                extra_env,
                params.focus,
            ),
            None => ws.split_pane(
                target_pane_id,
                direction,
                rows,
                cols,
                split_cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
                extra_env,
                params.focus,
            ),
        };
        let (target_tab_idx, new_pane) = match split_result {
            Some(Ok(result)) => result,
            Some(Err(err)) => return encode_error(id, "pane_split_failed", err.to_string()),
            None => {
                return pane_not_found(
                    id,
                    params.target_pane_id.as_deref().unwrap_or("active pane"),
                )
            }
        };
        if params.focus {
            self.state.switch_workspace(ws_idx);
            self.state.switch_tab(target_tab_idx);
            self.state.mode = Mode::Terminal;
        }
        self.terminal_runtimes
            .insert(new_pane.terminal.id.clone(), new_pane.runtime);
        self.state
            .remove_alias_shadowed_by_new_pane(new_pane.pane_id);
        self.state
            .terminals
            .insert(new_pane.terminal.id.clone(), new_pane.terminal);
        self.schedule_session_save();
        let pane = self.pane_info(ws_idx, new_pane.pane_id).unwrap();
        self.emit_event(EventEnvelope {
            event: EventKind::PaneCreated,
            data: EventData::PaneCreated { pane: pane.clone() },
        });
        self.emit_layout_updated_event(ws_idx, target_tab_idx);

        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_split_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: PaneSplitParams,
    ) -> String {
        view.reconcile(&self.state);
        let target = match params.target_pane_id.as_deref() {
            Some(target_pane_id) => self.parse_pane_id(target_pane_id),
            None => match params.workspace_id.as_deref() {
                Some(workspace_id) => self.parse_workspace_id(workspace_id).and_then(|ws_idx| {
                    view.focused_pane_for_workspace(&self.state, ws_idx)
                        .map(|(_, pane_id)| (ws_idx, pane_id))
                }),
                None => view.active_workspace.and_then(|ws_idx| {
                    view.focused_pane_for_workspace(&self.state, ws_idx)
                        .map(|(_, pane_id)| (ws_idx, pane_id))
                }),
            },
        };
        let Some((ws_idx, target_pane_id)) = target else {
            return pane_not_found(
                id,
                params.target_pane_id.as_deref().unwrap_or("active pane"),
            );
        };
        let extra_env = match super::env::normalize_launch_env(params.env) {
            Ok(env) => env,
            Err((code, message)) => return encode_error(id, &code, message),
        };
        let (rows, cols) = self.state.estimate_pane_size();
        let split_cwd = params.cwd.map(std::path::PathBuf::from).or_else(|| {
            self.state.workspaces.get(ws_idx).and_then(|ws| {
                let tab_idx = ws.find_tab_index_for_pane(target_pane_id)?;
                ws.tabs.get(tab_idx)?.cwd_for_pane(
                    target_pane_id,
                    &self.state.terminals,
                    &self.terminal_runtimes,
                )
            })
        });
        let default_shell = self.state.default_shell.clone();
        let scrollback_limit_bytes = self.state.pane_scrollback_limit_bytes;
        let host_terminal_theme = self.state.host_terminal_theme;
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return pane_not_found(
                id,
                params.target_pane_id.as_deref().unwrap_or("active pane"),
            );
        };
        let direction = match params.direction {
            crate::api::schema::SplitDirection::Right => ratatui::layout::Direction::Horizontal,
            crate::api::schema::SplitDirection::Down => ratatui::layout::Direction::Vertical,
        };

        let shell_config = crate::pane::PaneShellConfig::new(&default_shell, self.state.shell_mode);
        let split_result = match params.ratio {
            Some(ratio) => ws.split_pane_with_ratio(
                target_pane_id,
                direction,
                ratio,
                rows,
                cols,
                split_cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
                extra_env,
                false,
            ),
            None => ws.split_pane(
                target_pane_id,
                direction,
                rows,
                cols,
                split_cwd,
                scrollback_limit_bytes,
                host_terminal_theme,
                shell_config,
                extra_env,
                false,
            ),
        };
        let (target_tab_idx, new_pane) = match split_result {
            Some(Ok(result)) => result,
            Some(Err(err)) => return encode_error(id, "pane_split_failed", err.to_string()),
            None => {
                return pane_not_found(
                    id,
                    params.target_pane_id.as_deref().unwrap_or("active pane"),
                )
            }
        };
        self.terminal_runtimes
            .insert(new_pane.terminal.id.clone(), new_pane.runtime);
        self.state
            .remove_alias_shadowed_by_new_pane(new_pane.pane_id);
        self.state
            .terminals
            .insert(new_pane.terminal.id.clone(), new_pane.terminal);
        if params.focus {
            view.focus_pane_in_workspace(&self.state, ws_idx, target_tab_idx, new_pane.pane_id);
        } else {
            view.reconcile(&self.state);
        }
        self.schedule_session_save();
        let pane = self
            .pane_info_for_view(view, ws_idx, new_pane.pane_id)
            .unwrap();
        self.emit_event(EventEnvelope {
            event: EventKind::PaneCreated,
            data: EventData::PaneCreated { pane: pane.clone() },
        });

        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_list(&mut self, id: String, params: PaneListParams) -> String {
        match self.collect_panes_for_workspace(params.workspace_id.as_deref()) {
            Ok(panes) => encode_success(id, ResponseResult::PaneList { panes }),
            Err((code, message)) => encode_error(id, &code, message),
        }
    }

    pub(super) fn handle_pane_list_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneListParams,
    ) -> String {
        match self.collect_panes_for_workspace_for_view(view, params.workspace_id.as_deref()) {
            Ok(panes) => encode_success(id, ResponseResult::PaneList { panes }),
            Err((code, message)) => encode_error(id, &code, message),
        }
    }

    pub(super) fn handle_pane_current_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneCurrentParams,
    ) -> String {
        let target = match params.caller_pane_id.as_deref() {
            Some(caller_pane_id) => self.parse_pane_id(caller_pane_id),
            None => self.resolve_optional_pane_for_view(view, None),
        };
        let Some((ws_idx, pane_id)) = target else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(pane) = self.pane_info_for_view(view, ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };

        encode_success(id, ResponseResult::PaneCurrent { pane })
    }

    pub(super) fn handle_pane_get_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        target: PaneTarget,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&target.pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };
        let Some(pane) = self.pane_info_for_view(view, ws_idx, pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };

        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_current(&mut self, id: String, params: PaneCurrentParams) -> String {
        let target = match params.caller_pane_id.as_deref() {
            Some(caller_pane_id) => self.parse_pane_id(caller_pane_id),
            None => self.resolve_optional_pane(None),
        };
        let Some((ws_idx, pane_id)) = target else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(pane) = self.pane_info(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };

        encode_success(id, ResponseResult::PaneCurrent { pane })
    }

    pub(super) fn handle_pane_get(&mut self, id: String, target: PaneTarget) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&target.pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };
        let Some(pane) = self.pane_info(ws_idx, pane_id) else {
            return pane_not_found(id, &target.pane_id);
        };

        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_layout(&mut self, id: String, params: PaneLayoutParams) -> String {
        let Some((ws_idx, pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref()) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(id, ResponseResult::PaneLayout { layout })
    }

    pub(super) fn handle_pane_layout_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneLayoutParams,
    ) -> String {
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(layout) = self.pane_layout_snapshot_for_view(view, ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(id, ResponseResult::PaneLayout { layout })
    }

    pub(super) fn handle_pane_process_info(
        &mut self,
        id: String,
        params: PaneProcessInfoParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref()) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some((runtime, _workspace_id)) = self.lookup_runtime(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let shell_pid = runtime.child_pid();
        let shell_pid = (shell_pid != 0).then_some(shell_pid);
        let foreground_job = shell_pid.and_then(crate::detect::foreground_job);
        let foreground_process_group_id = foreground_job.as_ref().map(|job| job.process_group_id);
        let foreground_processes = foreground_job
            .map(|job| {
                job.processes
                    .into_iter()
                    .map(|process| PaneProcessInfoProcess {
                        pid: process.pid,
                        name: process.name,
                        argv0: process.argv0,
                        argv: process.argv,
                        cmdline: process.cmdline,
                        cwd: crate::platform::process_cwd(process.pid)
                            .map(|cwd| cwd.display().to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();

        encode_success(
            id,
            ResponseResult::PaneProcessInfo {
                process_info: PaneProcessInfo {
                    pane_id: public_pane_id,
                    shell_pid,
                    foreground_process_group_id,
                    tty: None,
                    foreground_processes,
                },
            },
        )
    }

    pub(super) fn handle_pane_process_info_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneProcessInfoParams,
    ) -> String {
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some((runtime, _workspace_id)) = self.lookup_runtime(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let shell_pid = runtime.child_pid();
        let shell_pid = (shell_pid != 0).then_some(shell_pid);
        let foreground_job = shell_pid.and_then(crate::detect::foreground_job);
        let foreground_process_group_id = foreground_job.as_ref().map(|job| job.process_group_id);
        let foreground_processes = foreground_job
            .map(|job| {
                job.processes
                    .into_iter()
                    .map(|process| PaneProcessInfoProcess {
                        pid: process.pid,
                        name: process.name,
                        argv0: process.argv0,
                        argv: process.argv,
                        cmdline: process.cmdline,
                        cwd: crate::platform::process_cwd(process.pid)
                            .map(|cwd| cwd.display().to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();

        encode_success(
            id,
            ResponseResult::PaneProcessInfo {
                process_info: PaneProcessInfo {
                    pane_id: public_pane_id,
                    shell_pid,
                    foreground_process_group_id,
                    tty: None,
                    foreground_processes,
                },
            },
        )
    }

    pub(super) fn handle_pane_neighbor(
        &mut self,
        id: String,
        params: PaneNeighborParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref()) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(source_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let neighbor_pane_id = self
            .directional_pane_target(ws_idx, tab_idx, pane_id, params.direction)
            .and_then(|pane_id| self.public_pane_id(ws_idx, pane_id));
        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(
            id,
            ResponseResult::PaneNeighbor {
                neighbor: PaneNeighborResult {
                    pane_id: source_public_id,
                    direction: params.direction,
                    neighbor_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_neighbor_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneNeighborParams,
    ) -> String {
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(source_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let neighbor_pane_id = self
            .directional_pane_target(ws_idx, tab_idx, pane_id, params.direction)
            .and_then(|pane_id| self.public_pane_id(ws_idx, pane_id));
        let Some(layout) = self.pane_layout_snapshot_for_view(view, ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(
            id,
            ResponseResult::PaneNeighbor {
                neighbor: PaneNeighborResult {
                    pane_id: source_public_id,
                    direction: params.direction,
                    neighbor_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_edges(&mut self, id: String, params: PaneEdgesParams) -> String {
        let Some((ws_idx, pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref()) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(tab) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
        else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let area = self.state.view.terminal_area;
        let Some(info) = tab
            .layout
            .panes(area)
            .into_iter()
            .find(|info| info.id == pane_id)
        else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(pane_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(
            id,
            ResponseResult::PaneEdges {
                edges: PaneEdgesResult {
                    pane_id: pane_public_id,
                    left: info.rect.x <= area.x,
                    right: info.rect.x + info.rect.width >= area.x + area.width,
                    up: info.rect.y <= area.y,
                    down: info.rect.y + info.rect.height >= area.y + area.height,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_edges_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneEdgesParams,
    ) -> String {
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(tab) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
        else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let area = normalized_terminal_area(self.state.view.terminal_area);
        let Some(info) = tab
            .layout
            .panes(area)
            .into_iter()
            .find(|info| info.id == pane_id)
        else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(pane_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(layout) = self.pane_layout_snapshot_for_view(view, ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(
            id,
            ResponseResult::PaneEdges {
                edges: PaneEdgesResult {
                    pane_id: pane_public_id,
                    left: info.rect.x <= area.x,
                    right: info.rect.x + info.rect.width >= area.x + area.width,
                    up: info.rect.y <= area.y,
                    down: info.rect.y + info.rect.height >= area.y + area.height,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_focus_direction(
        &mut self,
        id: String,
        params: PaneFocusDirectionParams,
    ) -> String {
        let Some((ws_idx, source_pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(source_pane_id)
        else {
            return pane_not_found(
                id,
                &self
                    .public_pane_id(ws_idx, source_pane_id)
                    .unwrap_or_default(),
            );
        };
        let Some(source_public_id) = self.public_pane_id(ws_idx, source_pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let target =
            self.directional_pane_target(ws_idx, tab_idx, source_pane_id, params.direction);
        let reason = target
            .is_none()
            .then_some(PaneFocusDirectionReason::NoNeighbor);

        if let Some(target_pane_id) = target {
            self.state.focus_pane_in_workspace(ws_idx, target_pane_id);
            self.state.switch_workspace_tab(ws_idx, tab_idx);
            self.state.mode = Mode::Terminal;
        }
        let focused_pane_id = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .map(|tab| tab.layout.focused())
            .and_then(|pane_id| self.public_pane_id(ws_idx, pane_id));
        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        if target.is_some() {
            self.emit_layout_updated_snapshot(layout.clone());
        }
        encode_success(
            id,
            ResponseResult::PaneFocusDirection {
                focus: PaneFocusDirectionResult {
                    changed: target.is_some(),
                    reason,
                    source_pane_id: source_public_id,
                    focused_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_focus_direction_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: PaneFocusDirectionParams,
    ) -> String {
        let Some((ws_idx, source_pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.find_tab_index_for_pane(source_pane_id))
        else {
            return pane_not_found(
                id,
                &self
                    .public_pane_id(ws_idx, source_pane_id)
                    .unwrap_or_default(),
            );
        };
        let Some(source_public_id) = self.public_pane_id(ws_idx, source_pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let target =
            self.directional_pane_target(ws_idx, tab_idx, source_pane_id, params.direction);
        let reason = target
            .is_none()
            .then_some(PaneFocusDirectionReason::NoNeighbor);

        if let Some(target_pane_id) = target {
            view.focus_pane_in_workspace(&self.state, ws_idx, tab_idx, target_pane_id);
        }
        let focused_pane_id = view
            .focused_pane_for_workspace(&self.state, ws_idx)
            .and_then(|(focused_tab_idx, pane_id)| (focused_tab_idx == tab_idx).then_some(pane_id))
            .and_then(|pane_id| self.public_pane_id(ws_idx, pane_id));
        let Some(layout) = self.pane_layout_snapshot_for_view(view, ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };

        encode_success(
            id,
            ResponseResult::PaneFocusDirection {
                focus: PaneFocusDirectionResult {
                    changed: target.is_some(),
                    reason,
                    source_pane_id: source_public_id,
                    focused_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_resize(&mut self, id: String, params: PaneResizeParams) -> String {
        let Some((ws_idx, pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref()) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(pane_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };

        let amount = params
            .amount
            .filter(|amount| amount.is_finite())
            .unwrap_or(0.05)
            .abs()
            .min(0.5);
        let direction: NavDirection = params.direction.into();
        let area = normalized_terminal_area(self.state.view.terminal_area);
        let changed = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
            .is_some_and(|tab| {
                if tab.layout.pane_count() <= 1 {
                    return false;
                }
                let before = layout_split_ratios(tab.layout.root());
                tab.layout.focus_pane(pane_id);
                tab.layout.resize_focused(direction, amount, area);
                before != layout_split_ratios(tab.layout.root())
            });
        if changed {
            self.schedule_session_save();
        }

        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let focused_pane_id = layout.focused_pane_id.clone();
        if changed {
            self.emit_layout_updated_snapshot(layout.clone());
        }

        encode_success(
            id,
            ResponseResult::PaneResize {
                resize: PaneResizeResult {
                    changed,
                    reason: (!changed).then_some(PaneResizeReason::Unchanged),
                    pane_id: pane_public_id,
                    focused_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_resize_for_view(
        &mut self,
        view: &ClientViewState,
        id: String,
        params: PaneResizeParams,
    ) -> String {
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(pane_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };

        let amount = params
            .amount
            .filter(|amount| amount.is_finite())
            .unwrap_or(0.05)
            .abs()
            .min(0.5);
        let direction: NavDirection = params.direction.into();
        let area = normalized_terminal_area(self.state.view.terminal_area);
        let changed = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
            .is_some_and(|tab| {
                if tab.layout.pane_count() <= 1 {
                    return false;
                }
                let before = layout_split_ratios(tab.layout.root());
                tab.layout.resize_pane(pane_id, direction, amount, area);
                before != layout_split_ratios(tab.layout.root())
            });
        if changed {
            self.schedule_session_save();
        }

        let Some(layout) = self.pane_layout_snapshot_for_view(view, ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let focused_pane_id = layout.focused_pane_id.clone();

        encode_success(
            id,
            ResponseResult::PaneResize {
                resize: PaneResizeResult {
                    changed,
                    reason: (!changed).then_some(PaneResizeReason::Unchanged),
                    pane_id: pane_public_id,
                    focused_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_swap(&mut self, id: String, params: PaneSwapParams) -> String {
        let directional = params.direction.is_some();
        let explicit = params.source_pane_id.is_some() || params.target_pane_id.is_some();
        if directional == explicit {
            return encode_error(
                id,
                "invalid_pane_swap",
                "provide either direction with optional pane_id, or source_pane_id and target_pane_id",
            );
        }

        let (ws_idx, tab_idx, source_pane_id, target_pane_id, reason) = if let Some(direction) =
            params.direction
        {
            let Some((ws_idx, source_pane_id)) =
                self.resolve_swap_source(params.pane_id.as_deref())
            else {
                return encode_error(id, "pane_not_found", "source pane not found");
            };
            let Some(tab_idx) =
                self.state.workspaces[ws_idx].find_tab_index_for_pane(source_pane_id)
            else {
                return pane_not_found(
                    id,
                    &self
                        .public_pane_id(ws_idx, source_pane_id)
                        .unwrap_or_default(),
                );
            };
            let target = self.directional_pane_target(ws_idx, tab_idx, source_pane_id, direction);
            match target {
                Some(target_pane_id) => {
                    (ws_idx, tab_idx, source_pane_id, Some(target_pane_id), None)
                }
                None => (
                    ws_idx,
                    tab_idx,
                    source_pane_id,
                    None,
                    Some(PaneSwapReason::NoNeighbor),
                ),
            }
        } else {
            let Some(source_raw) = params.source_pane_id.as_deref() else {
                return encode_error(id, "invalid_pane_swap", "missing source_pane_id");
            };
            let Some(target_raw) = params.target_pane_id.as_deref() else {
                return encode_error(id, "invalid_pane_swap", "missing target_pane_id");
            };
            let source = self
                .parse_pane_id(source_raw)
                .and_then(|(ws_idx, pane_id)| {
                    let tab_idx = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id)?;
                    Some((ws_idx, tab_idx, pane_id))
                });
            let target = self
                .parse_pane_id(target_raw)
                .and_then(|(ws_idx, pane_id)| {
                    let tab_idx = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id)?;
                    Some((ws_idx, tab_idx, pane_id))
                });
            let response_context = source
                .map(|(ws_idx, tab_idx, _)| (ws_idx, tab_idx))
                .or_else(|| target.map(|(ws_idx, tab_idx, _)| (ws_idx, tab_idx)))
                .or_else(|| {
                    let ws_idx = self.state.active?;
                    let tab_idx = self.state.workspaces.get(ws_idx)?.active_tab_index();
                    Some((ws_idx, tab_idx))
                });
            let Some((ws_idx, tab_idx)) = response_context else {
                return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
            };
            let source_pane_id = source
                .map(|(_, _, pane_id)| pane_id)
                .or_else(|| {
                    self.state
                        .workspaces
                        .get(ws_idx)?
                        .tabs
                        .get(tab_idx)
                        .map(|tab| tab.layout.focused())
                })
                .unwrap_or(PaneId::from_raw(0));
            let target_pane_id = target.map(|(_, _, pane_id)| pane_id);
            let reason = match (source, target) {
                (None, _) | (_, None) => Some(PaneSwapReason::NotFound),
                (Some((_, _, source)), Some((_, _, target))) if source == target => {
                    Some(PaneSwapReason::SamePane)
                }
                (Some((source_ws, source_tab, _)), Some((target_ws, target_tab, _)))
                    if source_ws != target_ws || source_tab != target_tab =>
                {
                    Some(PaneSwapReason::CrossTab)
                }
                _ => None,
            };
            (ws_idx, tab_idx, source_pane_id, target_pane_id, reason)
        };

        let mut changed = false;
        if reason.is_none() {
            if let Some(target_pane_id) = target_pane_id {
                let previous_focus = self.state.current_pane_focus_target();
                if let Some(tab) = self
                    .state
                    .workspaces
                    .get_mut(ws_idx)
                    .and_then(|ws| ws.tabs.get_mut(tab_idx))
                {
                    let (root, has_source, has_target) = clone_layout_with_swapped_panes(
                        tab.layout.root(),
                        source_pane_id,
                        target_pane_id,
                    );
                    changed = has_source && has_target;
                    if changed {
                        tab.layout = TileLayout::from_saved(root, source_pane_id);
                        self.state.switch_workspace_tab(ws_idx, tab_idx);
                        self.state
                            .record_pane_focus_change(previous_focus, ws_idx, source_pane_id);
                        self.state.mark_session_dirty();
                        self.schedule_session_save();
                    }
                }
            }
        }

        let source_public_id = match params.source_pane_id {
            Some(raw) => self
                .parse_pane_id(&raw)
                .and_then(|(idx, pane_id)| {
                    self.state
                        .workspaces
                        .get(idx)?
                        .find_tab_index_for_pane(pane_id)?;
                    self.public_pane_id(idx, pane_id)
                })
                .unwrap_or(raw),
            None => self
                .public_pane_id(ws_idx, source_pane_id)
                .unwrap_or_default(),
        };
        let target_public_id = match params.target_pane_id {
            Some(raw) => self
                .parse_pane_id(&raw)
                .and_then(|(idx, pane_id)| {
                    self.state
                        .workspaces
                        .get(idx)?
                        .find_tab_index_for_pane(pane_id)?;
                    self.public_pane_id(idx, pane_id)
                })
                .or(Some(raw)),
            None => target_pane_id.and_then(|pane_id| self.public_pane_id(ws_idx, pane_id)),
        };
        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let focused_pane_id = layout.focused_pane_id.clone();
        if changed {
            self.emit_layout_updated_snapshot(layout.clone());
        }

        encode_success(
            id,
            ResponseResult::PaneSwap {
                swap: PaneSwapResult {
                    changed,
                    reason,
                    source_pane_id: source_public_id,
                    target_pane_id: target_public_id,
                    focused_pane_id,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_zoom(&mut self, id: String, params: PaneZoomParams) -> String {
        let Some((ws_idx, pane_id)) = self.resolve_optional_pane(params.pane_id.as_deref()) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = self.state.workspaces[ws_idx].find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(pane_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let command = match params.mode {
            PaneZoomMode::Toggle => PaneZoomCommand::Toggle,
            PaneZoomMode::On => PaneZoomCommand::On,
            PaneZoomMode::Off => PaneZoomCommand::Off,
        };
        let Some(outcome) = self.apply_pane_zoom(ws_idx, pane_id, command) else {
            return pane_not_found(id, &pane_public_id);
        };
        if outcome.changed || outcome.focus_changed {
            self.schedule_session_save();
        }
        self.state.mode = Mode::Terminal;
        let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let focused_pane_id = layout.focused_pane_id.clone();

        if outcome.changed || outcome.focus_changed {
            self.emit_layout_updated_snapshot(layout.clone());
        }
        encode_success(
            id,
            ResponseResult::PaneZoom {
                zoom: PaneZoomResult {
                    changed: outcome.changed || outcome.focus_changed,
                    zoom_changed: outcome.changed,
                    focus_changed: outcome.focus_changed,
                    reason: outcome.reason.map(|reason| match reason {
                        PaneZoomNoopReason::SinglePane => PaneZoomReason::SinglePane,
                        PaneZoomNoopReason::AlreadyZoomed => PaneZoomReason::AlreadyZoomed,
                        PaneZoomNoopReason::AlreadyUnzoomed => PaneZoomReason::AlreadyUnzoomed,
                    }),
                    pane_id: pane_public_id,
                    focused_pane_id,
                    zoomed: outcome.zoomed,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_zoom_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: PaneZoomParams,
    ) -> String {
        let Some((ws_idx, pane_id)) =
            self.resolve_optional_pane_for_view(view, params.pane_id.as_deref())
        else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(ws) = self.state.workspaces.get(ws_idx) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let Some(tab_idx) = ws.find_tab_index_for_pane(pane_id) else {
            return pane_not_found(
                id,
                &self.public_pane_id(ws_idx, pane_id).unwrap_or_default(),
            );
        };
        let Some(tab) = ws.tabs.get(tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let Some(pane_public_id) = self.public_pane_id(ws_idx, pane_id) else {
            return encode_error(id, "pane_not_found", "pane not found");
        };
        let command = match params.mode {
            PaneZoomMode::Toggle => PaneZoomCommand::Toggle,
            PaneZoomMode::On => PaneZoomCommand::On,
            PaneZoomMode::Off => PaneZoomCommand::Off,
        };
        let pane_count = tab.layout.pane_count();
        let workspace_id = ws.id.clone();
        let tab_number = tab_idx + 1;
        let was_zoomed = view.tab_is_zoomed(&workspace_id, tab_number);
        let mut zoomed = was_zoomed;
        let mut zoom_changed = false;
        let mut reason = None;

        match command {
            PaneZoomCommand::Toggle if pane_count <= 1 => {
                reason = Some(PaneZoomNoopReason::SinglePane);
            }
            PaneZoomCommand::Toggle => {
                zoomed = !was_zoomed;
                zoom_changed = true;
            }
            PaneZoomCommand::On if pane_count <= 1 => {
                reason = Some(PaneZoomNoopReason::SinglePane);
            }
            PaneZoomCommand::On if was_zoomed => {
                reason = Some(PaneZoomNoopReason::AlreadyZoomed);
            }
            PaneZoomCommand::On => {
                zoomed = true;
                zoom_changed = true;
            }
            PaneZoomCommand::Off if !was_zoomed => {
                reason = Some(PaneZoomNoopReason::AlreadyUnzoomed);
            }
            PaneZoomCommand::Off => {
                zoomed = false;
                zoom_changed = true;
            }
        }

        if zoom_changed {
            view.set_tab_zoomed(&workspace_id, tab_number, zoomed);
        }
        let focus_changed = view.focus_pane_in_workspace(&self.state, ws_idx, tab_idx, pane_id);
        let Some(layout) = self.pane_layout_snapshot_for_view(view, ws_idx, tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let focused_pane_id = layout.focused_pane_id.clone();

        encode_success(
            id,
            ResponseResult::PaneZoom {
                zoom: PaneZoomResult {
                    changed: zoom_changed || focus_changed,
                    zoom_changed,
                    focus_changed,
                    reason: reason.map(|reason| match reason {
                        PaneZoomNoopReason::SinglePane => PaneZoomReason::SinglePane,
                        PaneZoomNoopReason::AlreadyZoomed => PaneZoomReason::AlreadyZoomed,
                        PaneZoomNoopReason::AlreadyUnzoomed => PaneZoomReason::AlreadyUnzoomed,
                    }),
                    pane_id: pane_public_id,
                    focused_pane_id,
                    zoomed,
                    layout,
                },
            },
        )
    }

    pub(super) fn handle_pane_rename(&mut self, id: String, params: PaneRenameParams) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(terminal_id) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.terminal_id(pane_id))
            .cloned()
        else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(terminal) = self.state.terminals.get_mut(&terminal_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        match params.label.map(|label| label.trim().to_string()) {
            Some(label) if !label.is_empty() => terminal.set_manual_label(label),
            _ => terminal.clear_manual_label(),
        }
        self.state.mark_session_dirty();
        let pane = self.pane_info(ws_idx, pane_id).unwrap();

        encode_success(id, ResponseResult::PaneInfo { pane })
    }

    pub(super) fn handle_pane_read(&mut self, id: String, params: PaneReadParams) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some((pane, workspace_id)) = self.lookup_runtime(ws_idx, pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(tab_idx) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.find_tab_index_for_pane(pane_id))
        else {
            return pane_not_found(id, &params.pane_id);
        };
        let requested_lines = params.lines.unwrap_or(80).min(1000) as usize;
        let text = match params.format {
            ReadFormat::Text => match params.source {
                ReadSource::Visible => pane.visible_text(),
                ReadSource::Recent => pane.recent_text(requested_lines),
                ReadSource::RecentUnwrapped => pane.recent_unwrapped_text(requested_lines),
                ReadSource::Detection => pane.detection_text(),
            },
            ReadFormat::Ansi => match params.source {
                ReadSource::Visible => pane.visible_ansi(),
                ReadSource::Recent => pane.recent_ansi(requested_lines),
                ReadSource::RecentUnwrapped => pane.recent_unwrapped_ansi(requested_lines),
                ReadSource::Detection => pane.detection_text(),
            },
        };

        encode_success(
            id,
            ResponseResult::PaneRead {
                read: PaneReadResult {
                    pane_id: params.pane_id,
                    workspace_id,
                    tab_id: self.public_tab_id(ws_idx, tab_idx).unwrap(),
                    source: params.source,
                    format: params.format,
                    text,
                    revision: 0,
                    truncated: false,
                },
            },
        )
    }

    pub(super) fn handle_pane_report_agent(
        &mut self,
        id: String,
        params: PaneReportAgentParams,
    ) -> String {
        let Some((_ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(agent_label) = normalize_reported_agent_label(&params.agent) else {
            return invalid_agent(id);
        };
        self.handle_internal_event(crate::events::AppEvent::HookStateReported {
            pane_id,
            session_ref: crate::agent_resume::session_ref_from_report(
                &params.source,
                &agent_label,
                params.agent_session_id,
                params.agent_session_path,
            ),
            launch_env: crate::agent_resume::launch_env_from_report(
                &params.source,
                &agent_label,
                params.launch_env,
            ),
            source: params.source,
            agent_label,
            state: detect_state_from_api(params.state),
            message: params.message,
            custom_status: normalize_custom_status(params.custom_status),
            seq: params.seq,
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_report_agent_session(
        &mut self,
        id: String,
        params: PaneReportAgentSessionParams,
    ) -> String {
        let Some((_ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(agent_label) = normalize_reported_agent_label(&params.agent) else {
            return invalid_agent(id);
        };
        let session_start_source =
            crate::agent_resume::normalize_session_start_source(params.session_start_source);
        self.handle_internal_event(crate::events::AppEvent::HookSessionReported {
            pane_id,
            session_ref: crate::agent_resume::session_ref_from_report(
                &params.source,
                &agent_label,
                params.agent_session_id,
                params.agent_session_path,
            ),
            launch_env: crate::agent_resume::launch_env_from_report(
                &params.source,
                &agent_label,
                params.launch_env,
            ),
            source: params.source,
            agent_label,
            seq: params.seq,
            session_start_source,
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_report_metadata(
        &mut self,
        id: String,
        params: PaneReportMetadataParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let agent_label = match params.agent.as_deref() {
            Some(agent) => match normalize_reported_agent_label(agent) {
                Some(agent_label) => Some(agent_label),
                None => return invalid_agent(id),
            },
            None => None,
        };
        let source = match normalize_metadata_source(params.source) {
            Ok(source) => source,
            Err(message) => return encode_error(id, "invalid_metadata_source", message),
        };
        let raw_title_set = params.title.is_some();
        let raw_display_agent_set = params.display_agent.is_some();
        let raw_custom_status_set = params.custom_status.is_some();
        let raw_state_labels_set = !params.state_labels.is_empty();
        let raw_tokens_set = !params.tokens.is_empty();
        let ttl = match normalize_metadata_ttl(params.ttl_ms) {
            Ok(ttl) => ttl,
            Err(message) => return encode_error(id, "invalid_metadata_ttl", message),
        };
        let title = normalize_presentation_text(params.title);
        let display_agent = normalize_presentation_text(params.display_agent);
        let custom_status = normalize_custom_status(params.custom_status);
        let applies_to_source = match params.applies_to_source {
            Some(applies_to_source) => match normalize_metadata_source(applies_to_source) {
                Ok(applies_to_source) => Some(applies_to_source),
                Err(message) => return encode_error(id, "invalid_metadata_source", message),
            },
            None => None,
        };
        let state_labels = match normalize_state_labels(params.state_labels) {
            Ok(labels) => labels,
            Err(status) => {
                return encode_error(
                    id,
                    "invalid_state_label",
                    format!("unknown state label: {status}"),
                );
            }
        };
        let tokens = if raw_tokens_set {
            match normalize_metadata_tokens(params.tokens) {
                Ok(tokens) => tokens,
                Err(message) => return encode_error(id, "invalid_metadata_tokens", message),
            }
        } else {
            std::collections::HashMap::new()
        };
        let Some(terminal_id) = self.state.terminal_id_for_pane(ws_idx, pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(terminal) = self.state.terminals.get(&terminal_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        if terminal.metadata_token_count_after_patch(&source, &tokens)
            > MAX_METADATA_TOKEN_KEYS_PER_RESOURCE
        {
            return encode_error(
                id,
                "invalid_metadata_tokens",
                format!(
                    "pane metadata may contain at most {MAX_METADATA_TOKEN_KEYS_PER_RESOURCE} tokens"
                ),
            );
        }
        if raw_title_set && params.clear_title
            || raw_display_agent_set && params.clear_display_agent
            || raw_custom_status_set && params.clear_custom_status
            || raw_state_labels_set && params.clear_state_labels
        {
            return encode_error(
                id,
                "invalid_metadata_request",
                "cannot set and clear the same metadata field",
            );
        }
        if title.is_none()
            && display_agent.is_none()
            && custom_status.is_none()
            && state_labels.is_empty()
            && !params.clear_title
            && !params.clear_display_agent
            && !params.clear_custom_status
            && tokens.is_empty()
            && !params.clear_state_labels
        {
            return encode_error(
                id,
                "invalid_metadata_request",
                "missing metadata field to set or clear",
            );
        }
        self.handle_internal_event(crate::events::AppEvent::HookMetadataReported {
            pane_id,
            source,
            agent_label,
            applies_to_source,
            title,
            display_agent,
            custom_status,
            state_labels,
            tokens,
            clear_title: params.clear_title,
            clear_display_agent: params.clear_display_agent,
            clear_custom_status: params.clear_custom_status,
            clear_state_labels: params.clear_state_labels,
            seq: params.seq,
            ttl,
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_clear_agent_authority(
        &mut self,
        id: String,
        params: PaneClearAgentAuthorityParams,
    ) -> String {
        let Some((_ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        self.handle_internal_event(crate::events::AppEvent::HookAuthorityCleared {
            pane_id,
            source: params.source,
            seq: params.seq,
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_release_agent(
        &mut self,
        id: String,
        params: PaneReleaseAgentParams,
    ) -> String {
        let Some((_ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(agent_label) = normalize_reported_agent_label(&params.agent) else {
            return invalid_agent(id);
        };
        let session_ref = crate::agent_resume::session_ref_from_report(
            &params.source,
            &agent_label,
            params.agent_session_id,
            params.agent_session_path,
        );
        self.handle_internal_event(crate::events::AppEvent::HookAgentReleased {
            pane_id,
            source: params.source,
            known_agent: crate::detect::parse_agent_label(&agent_label),
            session_ref,
            agent_label,
            seq: params.seq,
        });

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_send_text(
        &mut self,
        id: String,
        params: PaneSendTextParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(runtime) = self.lookup_runtime_sender(ws_idx, pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        if let Err(err) = runtime.try_send_bytes(Bytes::from(params.text)) {
            return encode_error(id, "pane_send_failed", err.to_string());
        }

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_send_input(
        &mut self,
        id: String,
        params: PaneSendInputParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(runtime) = self.lookup_runtime_sender(ws_idx, pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let encoded_keys = match encode_api_keys(runtime, &params.keys) {
            Ok(encoded_keys) => encoded_keys,
            Err(key) => return encode_error(id, "invalid_key", format!("unsupported key {key}")),
        };
        if !params.text.is_empty() {
            let text_bytes = encode_api_text(runtime, &params.text);
            if let Err(err) = runtime.try_send_bytes(Bytes::from(text_bytes)) {
                return encode_error(id, "pane_send_failed", err.to_string());
            }
        }
        for bytes in encoded_keys {
            if let Err(err) = runtime.try_send_bytes(Bytes::from(bytes)) {
                return encode_error(id, "pane_send_failed", err.to_string());
            }
        }

        encode_success(id, ResponseResult::Ok {})
    }

    pub(super) fn handle_pane_move(&mut self, id: String, params: PaneMoveParams) -> String {
        let PaneMoveParams {
            pane_id,
            destination,
            focus,
        } = params;
        let Some((source_ws_idx, source_pane_id)) = self.parse_pane_id(&pane_id) else {
            return pane_not_found(id, &pane_id);
        };
        let Some(source_tab_idx) =
            self.state.workspaces[source_ws_idx].find_tab_index_for_pane(source_pane_id)
        else {
            return pane_not_found(id, &pane_id);
        };
        let previous_pane_id = self
            .public_pane_id(source_ws_idx, source_pane_id)
            .unwrap_or_else(|| pane_id.clone());
        let previous_workspace_id = self.public_workspace_id(source_ws_idx);
        let Some(previous_tab_id) = self.public_tab_id(source_ws_idx, source_tab_idx) else {
            return encode_error(id, "tab_not_found", "source tab not found");
        };
        let Some(source_terminal_id) = self.state.workspaces[source_ws_idx].tabs[source_tab_idx]
            .terminal_id(source_pane_id)
            .cloned()
        else {
            return pane_not_found(id, &pane_id);
        };

        if self.state.workspaces[source_ws_idx].tabs[source_tab_idx].zoomed {
            let Some(layout) = self.pane_layout_snapshot(source_ws_idx, source_tab_idx) else {
                return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
            };
            let Some(pane) = self.pane_info(source_ws_idx, source_pane_id) else {
                return pane_not_found(id, &pane_id);
            };
            return encode_unchanged_pane_move(
                id,
                PaneMoveReason::ZoomedTab,
                previous_pane_id,
                previous_workspace_id,
                previous_tab_id,
                pane,
                Some(layout.clone()),
                layout,
            );
        }

        let resolved = match destination {
            PaneMoveDestination::Tab {
                tab_id,
                target_pane_id,
                split,
                ratio,
            } => {
                let Some((target_ws_idx, target_tab_idx)) = self.parse_tab_id(&tab_id) else {
                    return encode_error(id, "tab_not_found", format!("tab {tab_id} not found"));
                };
                if source_ws_idx == target_ws_idx && source_tab_idx == target_tab_idx {
                    let Some(layout) = self.pane_layout_snapshot(source_ws_idx, source_tab_idx)
                    else {
                        return encode_error(
                            id,
                            "pane_layout_unavailable",
                            "pane layout unavailable",
                        );
                    };
                    let Some(pane) = self.pane_info(source_ws_idx, source_pane_id) else {
                        return pane_not_found(id, &pane_id);
                    };
                    return encode_unchanged_pane_move(
                        id,
                        PaneMoveReason::SameTab,
                        previous_pane_id,
                        previous_workspace_id,
                        previous_tab_id,
                        pane,
                        Some(layout.clone()),
                        layout,
                    );
                }
                if self.state.workspaces[target_ws_idx].tabs[target_tab_idx].zoomed {
                    let Some(source_layout) =
                        self.pane_layout_snapshot(source_ws_idx, source_tab_idx)
                    else {
                        return encode_error(
                            id,
                            "pane_layout_unavailable",
                            "pane layout unavailable",
                        );
                    };
                    let Some(target_layout) =
                        self.pane_layout_snapshot(target_ws_idx, target_tab_idx)
                    else {
                        return encode_error(
                            id,
                            "pane_layout_unavailable",
                            "pane layout unavailable",
                        );
                    };
                    let Some(pane) = self.pane_info(source_ws_idx, source_pane_id) else {
                        return pane_not_found(id, &pane_id);
                    };
                    return encode_unchanged_pane_move(
                        id,
                        PaneMoveReason::ZoomedTab,
                        previous_pane_id,
                        previous_workspace_id,
                        previous_tab_id,
                        pane,
                        Some(source_layout),
                        target_layout,
                    );
                }
                let target_pane_id = match target_pane_id {
                    Some(raw) => {
                        let Some((pane_ws_idx, target_pane_id)) = self.parse_pane_id(&raw) else {
                            return encode_error(
                                id,
                                "target_pane_not_found",
                                format!("target pane {raw} not found"),
                            );
                        };
                        if pane_ws_idx != target_ws_idx
                            || self.state.workspaces[pane_ws_idx]
                                .find_tab_index_for_pane(target_pane_id)
                                != Some(target_tab_idx)
                        {
                            return encode_error(
                                id,
                                "target_pane_not_found",
                                format!("target pane {raw} is not in tab {tab_id}"),
                            );
                        }
                        target_pane_id
                    }
                    None => self.state.workspaces[target_ws_idx].tabs[target_tab_idx]
                        .layout
                        .focused(),
                };
                let Some(public_tab_id) = self.public_tab_id(target_ws_idx, target_tab_idx) else {
                    return encode_error(id, "tab_not_found", format!("tab {tab_id} not found"));
                };
                ResolvedPaneMoveDestination::ExistingTab {
                    tab_id: public_tab_id,
                    target_pane_id,
                    split,
                    ratio: ratio.unwrap_or(0.5),
                    cross_workspace: source_ws_idx != target_ws_idx,
                }
            }
            PaneMoveDestination::NewTab {
                workspace_id,
                label,
            } => {
                let workspace_id = if let Some(workspace_id) = workspace_id {
                    let Some(ws_idx) = self.parse_workspace_id(&workspace_id) else {
                        return encode_error(
                            id,
                            "workspace_not_found",
                            format!("workspace {workspace_id} not found"),
                        );
                    };
                    self.public_workspace_id(ws_idx)
                } else {
                    previous_workspace_id.clone()
                };
                ResolvedPaneMoveDestination::NewTab {
                    workspace_id,
                    label,
                }
            }
            PaneMoveDestination::NewWorkspace { label, tab_label } => {
                ResolvedPaneMoveDestination::NewWorkspace { label, tab_label }
            }
        };

        let previous_focus = self.state.current_pane_focus_target();
        let taken = match self.state.workspaces[source_ws_idx].take_pane_for_move(source_pane_id) {
            Some(taken) => taken,
            None => return encode_error(id, "pane_move_failed", "source pane could not be moved"),
        };
        let source_removed_tab_id = taken.removed_tab_idx.map(|_| previous_tab_id.clone());
        let source_workspace_empty = taken.workspace_empty;
        let moved = taken.moved;
        let cross_workspace = match &resolved {
            ResolvedPaneMoveDestination::ExistingTab {
                cross_workspace, ..
            } => *cross_workspace,
            ResolvedPaneMoveDestination::NewTab { workspace_id, .. } => {
                workspace_id != &previous_workspace_id
            }
            ResolvedPaneMoveDestination::NewWorkspace { .. } => true,
        };
        let mut closed_workspace_id = None;
        let closed_workspace_info = if source_workspace_empty && cross_workspace {
            Some(self.workspace_info(source_ws_idx))
        } else {
            None
        };
        if cross_workspace {
            if let Some(ws) = self.state.workspaces.get_mut(source_ws_idx) {
                ws.unregister_moved_pane(source_pane_id);
            }
            self.state
                .public_pane_id_aliases
                .insert(previous_pane_id.clone(), source_pane_id);
        }
        if source_workspace_empty && cross_workspace {
            self.state.workspaces.remove(source_ws_idx);
            closed_workspace_id = Some(previous_workspace_id.clone());
            if self.state.workspaces.is_empty() {
                self.state.active = None;
                self.state.selected = 0;
            } else {
                if let Some(active) = self.state.active {
                    if active == source_ws_idx {
                        self.state.active =
                            Some(source_ws_idx.min(self.state.workspaces.len() - 1));
                    } else if active > source_ws_idx {
                        self.state.active = Some(active - 1);
                    }
                }
                if self.state.selected == source_ws_idx {
                    self.state.selected = source_ws_idx.min(self.state.workspaces.len() - 1);
                } else if self.state.selected > source_ws_idx {
                    self.state.selected -= 1;
                }
            }
        }

        let mut created_workspace = false;
        let mut created_tab = false;
        let (target_ws_idx, target_tab_idx, moved_pane_id) = match resolved {
            ResolvedPaneMoveDestination::ExistingTab {
                tab_id,
                target_pane_id,
                split,
                ratio,
                cross_workspace: _,
            } => {
                let Some((target_ws_idx, target_tab_idx)) = self.parse_tab_id(&tab_id) else {
                    return encode_error(id, "pane_move_failed", "target tab disappeared");
                };
                let previous_target_focus = self.state.workspaces[target_ws_idx].tabs
                    [target_tab_idx]
                    .layout
                    .focused();
                let moved_pane_id = match self.state.workspaces[target_ws_idx]
                    .insert_moved_pane_into_tab(
                        target_tab_idx,
                        target_pane_id,
                        moved,
                        split_direction_to_layout(split),
                        ratio,
                    ) {
                    Ok(pane_id) => pane_id,
                    Err(_) => {
                        return encode_error(
                            id,
                            "pane_move_failed",
                            "target pane could not be split",
                        )
                    }
                };
                if !focus {
                    self.state.workspaces[target_ws_idx].tabs[target_tab_idx]
                        .layout
                        .focus_pane(previous_target_focus);
                }
                (target_ws_idx, target_tab_idx, moved_pane_id)
            }
            ResolvedPaneMoveDestination::NewTab {
                workspace_id,
                label,
            } => {
                let Some(target_ws_idx) = self.parse_workspace_id(&workspace_id) else {
                    return encode_error(id, "pane_move_failed", "target workspace disappeared");
                };
                let moved_pane_id = moved.pane_id;
                let target_tab_idx = self.state.workspaces[target_ws_idx]
                    .create_tab_from_existing_pane(
                        moved,
                        label,
                        self.event_tx.clone(),
                        self.render_notify.clone(),
                        self.render_dirty.clone(),
                    );
                created_tab = true;
                (target_ws_idx, target_tab_idx, moved_pane_id)
            }
            ResolvedPaneMoveDestination::NewWorkspace { label, tab_label } => {
                let identity_cwd = self
                    .state
                    .terminals
                    .get(&source_terminal_id)
                    .map(|terminal| terminal.cwd.clone())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/".into()));
                let moved_pane_id = moved.pane_id;
                let workspace = crate::workspace::Workspace::from_existing_pane(
                    label,
                    tab_label,
                    identity_cwd,
                    moved,
                    self.event_tx.clone(),
                    self.render_notify.clone(),
                    self.render_dirty.clone(),
                );
                self.state.workspaces.push(workspace);
                created_workspace = true;
                created_tab = true;
                (self.state.workspaces.len() - 1, 0, moved_pane_id)
            }
        };

        if focus || self.state.active.is_none() {
            self.state.switch_workspace(target_ws_idx);
            self.state.switch_tab(target_tab_idx);
            self.state
                .record_pane_focus_change(previous_focus, target_ws_idx, moved_pane_id);
            self.state.mode = Mode::Terminal;
        }
        self.state.remove_alias_shadowed_by_new_pane(moved_pane_id);
        self.state.mark_session_dirty();
        self.schedule_session_save();

        let Some(pane) = self.pane_info(target_ws_idx, moved_pane_id) else {
            return encode_error(id, "pane_move_failed", "moved pane is unavailable");
        };
        let source_layout = if closed_workspace_id.is_none() {
            self.parse_tab_id(&previous_tab_id)
                .and_then(|(ws_idx, tab_idx)| self.pane_layout_snapshot(ws_idx, tab_idx))
        } else {
            None
        };
        let Some(target_layout) = self.pane_layout_snapshot(target_ws_idx, target_tab_idx) else {
            return encode_error(id, "pane_layout_unavailable", "pane layout unavailable");
        };
        let focused_pane_id = target_layout.focused_pane_id.clone();
        let created_workspace_info = if created_workspace {
            Some(self.workspace_info(target_ws_idx))
        } else {
            None
        };
        let created_tab_info = if created_tab {
            self.tab_info(target_ws_idx, target_tab_idx)
        } else {
            None
        };
        let move_result = PaneMoveResult {
            changed: true,
            reason: None,
            previous_pane_id: previous_pane_id.clone(),
            previous_workspace_id: previous_workspace_id.clone(),
            previous_tab_id: previous_tab_id.clone(),
            pane: Box::new(pane.clone()),
            source_layout: source_layout.clone().map(Box::new),
            target_layout: Box::new(target_layout),
            created_workspace: created_workspace_info.clone(),
            created_tab: created_tab_info.clone(),
            closed_workspace_id: closed_workspace_id.clone(),
            closed_tab_id: source_removed_tab_id.clone(),
            focused_pane_id,
        };

        if let Some(closed_tab_id) = &source_removed_tab_id {
            self.emit_event(EventEnvelope {
                event: EventKind::TabClosed,
                data: EventData::TabClosed {
                    tab_id: closed_tab_id.clone(),
                    workspace_id: previous_workspace_id.clone(),
                },
            });
        }
        if let Some(closed_workspace_id) = &closed_workspace_id {
            self.emit_event(EventEnvelope {
                event: EventKind::WorkspaceClosed,
                data: EventData::WorkspaceClosed {
                    workspace_id: closed_workspace_id.clone(),
                    workspace: closed_workspace_info.clone(),
                },
            });
        }
        if let Some(workspace) = &created_workspace_info {
            self.emit_event(EventEnvelope {
                event: EventKind::WorkspaceCreated,
                data: EventData::WorkspaceCreated {
                    workspace: workspace.clone(),
                },
            });
        }
        if let Some(tab) = &created_tab_info {
            self.emit_event(EventEnvelope {
                event: EventKind::TabCreated,
                data: EventData::TabCreated { tab: tab.clone() },
            });
        }
        self.emit_event(EventEnvelope {
            event: EventKind::PaneMoved,
            data: EventData::PaneMoved {
                previous_pane_id,
                previous_workspace_id,
                previous_tab_id,
                pane: Box::new(pane),
                created_workspace: created_workspace_info,
                created_tab: created_tab_info,
                closed_workspace_id,
                closed_tab_id: source_removed_tab_id,
            },
        });
        if let Some(source_layout) = source_layout {
            self.emit_layout_updated_snapshot(source_layout);
        }
        self.emit_layout_updated_snapshot((*move_result.target_layout).clone());

        encode_success(id, ResponseResult::PaneMove { move_result })
    }

    pub(super) fn pane_layout_snapshot(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<PaneLayoutSnapshot> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let tab = ws.tabs.get(tab_idx)?;
        let mut area = self.state.view.terminal_area;
        if area.width == 0 || area.height == 0 {
            area = ratatui::layout::Rect::new(0, 0, 80, 24);
        }
        let focused_pane_id = self.public_pane_id(ws_idx, tab.layout.focused())?;
        let panes = tab
            .layout
            .panes(area)
            .into_iter()
            .filter_map(|pane| {
                Some(PaneLayoutPane {
                    pane_id: self.public_pane_id(ws_idx, pane.id)?,
                    focused: pane.is_focused,
                    rect: layout_rect(pane.rect),
                })
            })
            .collect();
        let splits = tab
            .layout
            .splits(area)
            .into_iter()
            .enumerate()
            .map(|(index, split)| PaneLayoutSplit {
                id: format!("split-{index}"),
                direction: split_direction(split.direction),
                ratio: split_ratio(tab.layout.root(), &split.path).unwrap_or(0.5),
                rect: layout_rect(split.area),
            })
            .collect();
        Some(PaneLayoutSnapshot {
            workspace_id: self.public_workspace_id(ws_idx),
            tab_id: self.public_tab_id(ws_idx, tab_idx)?,
            zoomed: tab.zoomed,
            area: layout_rect(area),
            focused_pane_id,
            panes,
            splits,
        })
    }

    pub(crate) fn emit_layout_updated_event(&mut self, ws_idx: usize, tab_idx: usize) {
        if let Some(layout) = self.pane_layout_snapshot(ws_idx, tab_idx) {
            self.emit_layout_updated_snapshot(layout);
        }
    }

    pub(super) fn emit_layout_updated_snapshot(&mut self, layout: PaneLayoutSnapshot) {
        self.emit_event(EventEnvelope {
            event: EventKind::LayoutUpdated,
            data: EventData::LayoutUpdated { layout },
        });
    }

    pub(crate) fn layout_update_target_after_pane_removal(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
    ) -> Option<(usize, usize)> {
        let tab_idx = self
            .state
            .workspaces
            .get(ws_idx)?
            .find_tab_index_for_pane(pane_id)?;
        let pane_count = self
            .state
            .workspaces
            .get(ws_idx)?
            .tabs
            .get(tab_idx)?
            .layout
            .pane_count();
        (pane_count > 1).then_some((ws_idx, tab_idx))
    }

    fn pane_layout_snapshot_for_view(
        &self,
        view: &ClientViewState,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Option<PaneLayoutSnapshot> {
        let ws = self.state.workspaces.get(ws_idx)?;
        let tab = ws.tabs.get(tab_idx)?;
        let area = normalized_terminal_area(self.state.view.terminal_area);
        let focused = view
            .focused_pane_for_tab(&ws.id, tab_idx + 1)
            .filter(|pane_id| tab.panes.contains_key(pane_id))
            .unwrap_or_else(|| tab.layout.focused());
        let focused_pane_id = self.public_pane_id(ws_idx, focused)?;
        let panes = tab
            .layout
            .panes(area)
            .into_iter()
            .filter_map(|pane| {
                Some(PaneLayoutPane {
                    pane_id: self.public_pane_id(ws_idx, pane.id)?,
                    focused: pane.id == focused,
                    rect: layout_rect(pane.rect),
                })
            })
            .collect();
        let splits = tab
            .layout
            .splits(area)
            .into_iter()
            .enumerate()
            .map(|(index, split)| PaneLayoutSplit {
                id: format!("split-{index}"),
                direction: split_direction(split.direction),
                ratio: split_ratio(tab.layout.root(), &split.path).unwrap_or(0.5),
                rect: layout_rect(split.area),
            })
            .collect();
        Some(PaneLayoutSnapshot {
            workspace_id: self.public_workspace_id(ws_idx),
            tab_id: self.public_tab_id(ws_idx, tab_idx)?,
            zoomed: view.tab_is_zoomed(&ws.id, tab_idx + 1),
            area: layout_rect(area),
            focused_pane_id,
            panes,
            splits,
        })
    }

    fn resolve_optional_pane(&self, pane_id: Option<&str>) -> Option<(usize, PaneId)> {
        match pane_id {
            Some(pane_id) => self.parse_pane_id(pane_id),
            None => {
                let ws_idx = self.state.active?;
                let pane_id = self.state.workspaces.get(ws_idx)?.focused_pane_id()?;
                Some((ws_idx, pane_id))
            }
        }
    }

    fn resolve_optional_pane_for_view(
        &self,
        view: &ClientViewState,
        pane_id: Option<&str>,
    ) -> Option<(usize, PaneId)> {
        match pane_id {
            Some(pane_id) => self.parse_pane_id(pane_id),
            None => {
                let ws_idx = view.active_workspace?;
                view.focused_pane_for_workspace(&self.state, ws_idx)
                    .map(|(_, pane_id)| (ws_idx, pane_id))
            }
        }
    }

    fn resolve_swap_source(&self, pane_id: Option<&str>) -> Option<(usize, PaneId)> {
        self.resolve_optional_pane(pane_id)
    }

    fn directional_pane_target(
        &self,
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
        direction: PaneDirection,
    ) -> Option<PaneId> {
        let tab = self.state.workspaces.get(ws_idx)?.tabs.get(tab_idx)?;
        let panes = tab
            .layout
            .panes(normalized_terminal_area(self.state.view.terminal_area));
        let focused = panes.iter().find(|info| info.id == pane_id)?;
        find_in_direction(focused, direction.into(), &panes)
    }

    fn apply_pane_zoom(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        command: PaneZoomCommand,
    ) -> Option<PaneZoomOutcome> {
        let tab_idx = self
            .state
            .workspaces
            .get(ws_idx)?
            .find_tab_index_for_pane(pane_id)?;
        let focus_changed = self.state.focus_pane_in_workspace(ws_idx, pane_id);
        let tab = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))?;
        let pane_count = tab.layout.pane_count();
        let mut zoom_changed = false;
        let mut reason = None;

        match command {
            PaneZoomCommand::Toggle if pane_count <= 1 => {
                reason = Some(PaneZoomNoopReason::SinglePane);
            }
            PaneZoomCommand::Toggle => {
                tab.zoomed = !tab.zoomed;
                zoom_changed = true;
            }
            PaneZoomCommand::On if pane_count <= 1 => {
                reason = Some(PaneZoomNoopReason::SinglePane);
            }
            PaneZoomCommand::On if tab.zoomed => {
                reason = Some(PaneZoomNoopReason::AlreadyZoomed);
            }
            PaneZoomCommand::On => {
                tab.zoomed = true;
                zoom_changed = true;
            }
            PaneZoomCommand::Off if !tab.zoomed => {
                reason = Some(PaneZoomNoopReason::AlreadyUnzoomed);
            }
            PaneZoomCommand::Off => {
                tab.zoomed = false;
                zoom_changed = true;
            }
        }

        Some(PaneZoomOutcome {
            changed: zoom_changed,
            focus_changed,
            reason,
            zoomed: tab.zoomed,
        })
    }

    pub(super) fn handle_pane_close(&mut self, id: String, target: PaneTarget) -> String {
        match self.close_pane(id.clone(), &target) {
            Ok(()) => encode_success(id, ResponseResult::Ok {}),
            Err(response) => response,
        }
    }

    /// Close a pane; `Err` carries the encoded error response.
    pub(super) fn close_pane(&mut self, id: String, target: &PaneTarget) -> Result<(), String> {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&target.pane_id) else {
            return Err(pane_not_found(id, &target.pane_id));
        };

        if self.public_pane_id(ws_idx, pane_id).is_none() {
            return Err(pane_not_found(id, &target.pane_id));
        }
        let workspace_id = self.public_workspace_id(ws_idx);
        let layout_update_target = self.layout_update_target_after_pane_removal(ws_idx, pane_id);
        let closing_workspace = self.state.close_pane_would_close_workspace(ws_idx, pane_id);
        let workspace_snapshot = closing_workspace.then(|| self.workspace_info(ws_idx));

        let terminal_id = self.state.terminal_id_for_pane(ws_idx, pane_id);
        let should_close_workspace = {
            let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
                return Err(pane_not_found(id, &target.pane_id));
            };
            ws.close_pane(pane_id)
        };
        self.state.plugin_panes.remove(&pane_id);
        if should_close_workspace {
            self.state.selected = ws_idx;
            self.state.close_selected_workspace();
            self.shutdown_detached_terminal_runtimes();
            self.emit_event(EventEnvelope {
                event: EventKind::PaneClosed,
                data: EventData::PaneClosed {
                    pane_id: target.pane_id.clone(),
                    workspace_id: workspace_id.clone(),
                },
            });
            self.emit_event(EventEnvelope {
                event: EventKind::WorkspaceClosed,
                data: EventData::WorkspaceClosed {
                    workspace_id,
                    workspace: workspace_snapshot,
                },
            });
        } else {
            self.state.remove_unattached_terminal_ids(terminal_id);
            self.shutdown_detached_terminal_runtimes();
            self.schedule_session_save();
            self.emit_event(EventEnvelope {
                event: EventKind::PaneClosed,
                data: EventData::PaneClosed {
                    pane_id: target.pane_id.clone(),
                    workspace_id,
                },
            });
            if let Some((ws_idx, tab_idx)) = layout_update_target {
                self.emit_layout_updated_event(ws_idx, tab_idx);
            }
        }

        Ok(())
    }

    pub(super) fn handle_pane_send_keys(
        &mut self,
        id: String,
        params: PaneSendKeysParams,
    ) -> String {
        let Some((ws_idx, pane_id)) = self.parse_pane_id(&params.pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let Some(runtime) = self.lookup_runtime_sender(ws_idx, pane_id) else {
            return pane_not_found(id, &params.pane_id);
        };
        let encoded_keys = match encode_api_keys(runtime, &params.keys) {
            Ok(encoded_keys) => encoded_keys,
            Err(key) => return encode_error(id, "invalid_key", format!("unsupported key {key}")),
        };
        for bytes in encoded_keys {
            if let Err(err) = runtime.try_send_bytes(Bytes::from(bytes)) {
                return encode_error(id, "pane_send_failed", err.to_string());
            }
        }

        encode_success(id, ResponseResult::Ok {})
    }
}

enum PaneZoomCommand {
    Toggle,
    On,
    Off,
}

enum PaneZoomNoopReason {
    SinglePane,
    AlreadyZoomed,
    AlreadyUnzoomed,
}

struct PaneZoomOutcome {
    changed: bool,
    focus_changed: bool,
    reason: Option<PaneZoomNoopReason>,
    zoomed: bool,
}

enum ResolvedPaneMoveDestination {
    ExistingTab {
        tab_id: String,
        target_pane_id: PaneId,
        split: SplitDirection,
        ratio: f32,
        cross_workspace: bool,
    },
    NewTab {
        workspace_id: String,
        label: Option<String>,
    },
    NewWorkspace {
        label: Option<String>,
        tab_label: Option<String>,
    },
}

impl From<PaneDirection> for NavDirection {
    fn from(direction: PaneDirection) -> Self {
        match direction {
            PaneDirection::Left => Self::Left,
            PaneDirection::Right => Self::Right,
            PaneDirection::Up => Self::Up,
            PaneDirection::Down => Self::Down,
        }
    }
}

fn normalized_terminal_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    if area.width == 0 || area.height == 0 {
        ratatui::layout::Rect::new(0, 0, 80, 24)
    } else {
        area
    }
}

fn layout_rect(rect: ratatui::layout::Rect) -> PaneLayoutRect {
    PaneLayoutRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn split_direction(direction: ratatui::layout::Direction) -> SplitDirection {
    match direction {
        ratatui::layout::Direction::Horizontal => SplitDirection::Right,
        ratatui::layout::Direction::Vertical => SplitDirection::Down,
    }
}

fn split_direction_to_layout(direction: SplitDirection) -> ratatui::layout::Direction {
    match direction {
        SplitDirection::Right => ratatui::layout::Direction::Horizontal,
        SplitDirection::Down => ratatui::layout::Direction::Vertical,
    }
}

fn split_ratio(node: &Node, path: &[bool]) -> Option<f32> {
    match node {
        Node::Split {
            ratio,
            first,
            second,
            ..
        } => {
            if path.is_empty() {
                Some(*ratio)
            } else if path[0] {
                split_ratio(second, &path[1..])
            } else {
                split_ratio(first, &path[1..])
            }
        }
        Node::Pane(_) => None,
    }
}

fn layout_split_ratios(root: &Node) -> Vec<(Vec<bool>, u32)> {
    fn collect(node: &Node, path: &mut Vec<bool>, ratios: &mut Vec<(Vec<bool>, u32)>) {
        if let Node::Split {
            ratio,
            first,
            second,
            ..
        } = node
        {
            ratios.push((path.clone(), ratio.to_bits()));
            path.push(false);
            collect(first, path, ratios);
            path.pop();
            path.push(true);
            collect(second, path, ratios);
            path.pop();
        }
    }

    let mut ratios = Vec::new();
    collect(root, &mut Vec::new(), &mut ratios);
    ratios
}

fn clone_layout_with_swapped_panes(
    node: &Node,
    source: PaneId,
    target: PaneId,
) -> (Node, bool, bool) {
    match node {
        Node::Pane(pane_id) if *pane_id == source => (Node::Pane(target), true, false),
        Node::Pane(pane_id) if *pane_id == target => (Node::Pane(source), false, true),
        Node::Pane(pane_id) => (Node::Pane(*pane_id), false, false),
        Node::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let (first, first_has_source, first_has_target) =
                clone_layout_with_swapped_panes(first, source, target);
            let (second, second_has_source, second_has_target) =
                clone_layout_with_swapped_panes(second, source, target);
            (
                Node::Split {
                    direction: *direction,
                    ratio: *ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                },
                first_has_source || second_has_source,
                first_has_target || second_has_target,
            )
        }
    }
}

fn encode_unchanged_pane_move(
    id: String,
    reason: PaneMoveReason,
    previous_pane_id: String,
    previous_workspace_id: String,
    previous_tab_id: String,
    pane: crate::api::schema::PaneInfo,
    source_layout: Option<PaneLayoutSnapshot>,
    target_layout: PaneLayoutSnapshot,
) -> String {
    let focused_pane_id = target_layout.focused_pane_id.clone();
    encode_success(
        id,
        ResponseResult::PaneMove {
            move_result: PaneMoveResult {
                changed: false,
                reason: Some(reason),
                previous_pane_id,
                previous_workspace_id,
                previous_tab_id,
                pane: Box::new(pane),
                source_layout: source_layout.map(Box::new),
                target_layout: Box::new(target_layout),
                created_workspace: None,
                created_tab: None,
                closed_workspace_id: None,
                closed_tab_id: None,
                focused_pane_id,
            },
        },
    )
}

fn normalize_metadata_source(source: String) -> Result<String, String> {
    let normalized: String = source
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(METADATA_SOURCE_MAX_CHARS)
        .collect();
    let normalized = normalized.trim().to_string();
    if normalized.is_empty() {
        Err("metadata source must not be empty".into())
    } else {
        Ok(normalized)
    }
}

fn normalize_metadata_ttl(ttl_ms: Option<u64>) -> Result<Option<std::time::Duration>, String> {
    let Some(ttl_ms) = ttl_ms else {
        return Ok(None);
    };
    if !(METADATA_TTL_MIN_MS..=METADATA_TTL_MAX_MS).contains(&ttl_ms) {
        return Err(format!(
            "metadata ttl_ms must be between {METADATA_TTL_MIN_MS} and {METADATA_TTL_MAX_MS}"
        ));
    }
    Ok(Some(std::time::Duration::from_millis(ttl_ms)))
}

fn normalize_presentation_text(value: Option<String>) -> Option<String> {
    let trimmed = value?.trim().to_string();
    let normalized: String = trimmed
        .chars()
        .filter(|ch| !ch.is_control())
        .take(80)
        .collect();
    (!normalized.trim().is_empty()).then(|| normalized.trim().to_string())
}

fn normalize_state_labels(
    labels: std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    labels
        .into_iter()
        .map(|(status, label)| {
            let status = status.trim().to_ascii_lowercase();
            if !matches!(
                status.as_str(),
                "idle" | "working" | "blocked" | "done" | "unknown"
            ) {
                return Err(status);
            }
            Ok(normalize_presentation_text(Some(label)).map(|label| (status, label)))
        })
        .filter_map(Result::transpose)
        .collect()
}

fn pane_not_found(id: String, pane_id: &str) -> String {
    encode_error(id, "pane_not_found", format!("pane {pane_id} not found"))
}

fn invalid_agent(id: String) -> String {
    encode_error(id, "invalid_agent", "agent label must not be empty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::{
        api::schema::{SplitDirection, SuccessResponse},
        config::Config,
        workspace::Workspace,
    };

    fn test_app() -> App {
        app_with_test_workspace().0
    }
    fn app_with_test_workspace() -> (App, String) {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("metadata")];
        app.state.ensure_test_terminals();
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let public_pane_id = app.public_pane_id(0, pane_id).unwrap();
        (app, public_pane_id)
    }

    #[test]
    fn pane_metadata_rejects_tokens_above_resource_limit() {
        let (mut app, pane_id) = app_with_test_workspace();

        for batch in 0..2 {
            let tokens = (0..16)
                .map(|index| (format!("token_{batch}_{index}"), Some("value".to_string())))
                .collect();
            let response = app.handle_pane_report_metadata(
                format!("batch-{batch}"),
                PaneReportMetadataParams {
                    pane_id: pane_id.clone(),
                    source: "user:test".into(),
                    agent: None,
                    applies_to_source: None,
                    title: None,
                    display_agent: None,
                    custom_status: None,
                    state_labels: HashMap::new(),
                    tokens,
                    clear_title: false,
                    clear_display_agent: false,
                    clear_custom_status: false,
                    clear_state_labels: false,
                    seq: None,
                    ttl_ms: None,
                },
            );
            let response: serde_json::Value = serde_json::from_str(&response).unwrap();
            assert_eq!(response["result"]["type"], "ok");
        }

        let response = app.handle_pane_report_metadata(
            "overflow".into(),
            PaneReportMetadataParams {
                pane_id,
                source: "user:test".into(),
                agent: None,
                applies_to_source: None,
                title: None,
                display_agent: None,
                custom_status: None,
                state_labels: HashMap::new(),
                tokens: HashMap::from([("overflow".into(), Some("value".into()))]),
                clear_title: false,
                clear_display_agent: false,
                clear_custom_status: false,
                clear_state_labels: false,
                seq: None,
                ttl_ms: None,
            },
        );
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["error"]["code"], "invalid_metadata_tokens");
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("at most 32 tokens"));
    }

    #[test]

    fn pane_move_to_existing_tab_preserves_pane_id_and_terminal() {
        let mut app = test_app();

        let source = app.state.workspaces[0].tabs[0].root_pane;
        let source_terminal = app.state.workspaces[0].tabs[0]
            .terminal_id(source)
            .unwrap()
            .clone();
        let target_tab = app.state.workspaces[0].test_add_tab(Some("target"));
        app.state.ensure_test_terminals();
        let source_public = app.public_pane_id(0, source).unwrap();
        let source_tab_public = app.public_tab_id(0, 0).unwrap();
        let target_pane = app.state.workspaces[0].tabs[target_tab].root_pane;
        let target_public = app.public_pane_id(0, target_pane).unwrap();
        let target_tab_public = app.public_tab_id(0, target_tab).unwrap();

        let response = app.handle_pane_move(
            "move".into(),
            PaneMoveParams {
                pane_id: source_public.clone(),
                destination: PaneMoveDestination::Tab {
                    tab_id: target_tab_public.clone(),
                    target_pane_id: Some(target_public),
                    split: SplitDirection::Right,
                    ratio: Some(0.25),
                },
                focus: true,
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PaneMove { move_result } = success.result else {
            panic!("expected pane move response");
        };
        assert!(move_result.changed);
        assert_eq!(move_result.previous_pane_id, source_public);
        assert_eq!(move_result.previous_tab_id, source_tab_public);
        assert_eq!(move_result.pane.pane_id, move_result.previous_pane_id);
        assert_eq!(move_result.pane.tab_id, target_tab_public);
        assert_eq!(move_result.pane.terminal_id, source_terminal.to_string());
        assert_eq!(move_result.closed_tab_id, Some(source_tab_public));
        assert_eq!(move_result.closed_workspace_id, None);
        assert_eq!(move_result.target_layout.panes.len(), 2);
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs[0].layout.focused(), source);
        assert_eq!(
            app.state.workspaces[0].tabs[0].terminal_id(source),
            Some(&source_terminal)
        );
    }

    #[test]
    fn pane_move_across_workspaces_keeps_previous_public_id_as_alias() {
        let mut app = test_app();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("target"));
        app.state.ensure_test_terminals();
        let source = app.state.workspaces[0].tabs[0].root_pane;
        let target_pane = app.state.workspaces[1].tabs[0].root_pane;
        let previous_pane_id = app.public_pane_id(0, source).unwrap();
        let previous_workspace_id = app.public_workspace_id(0);
        let target_tab_id = app.public_tab_id(1, 0).unwrap();
        let target_pane_id = app.public_pane_id(1, target_pane).unwrap();

        let response = app.handle_pane_move(
            "move".into(),
            PaneMoveParams {
                pane_id: previous_pane_id.clone(),
                destination: PaneMoveDestination::Tab {
                    tab_id: target_tab_id.clone(),
                    target_pane_id: Some(target_pane_id),
                    split: SplitDirection::Down,
                    ratio: None,
                },
                focus: false,
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::PaneMove { move_result } = success.result else {
            panic!("expected pane move response");
        };
        assert!(move_result.changed);
        assert_eq!(move_result.closed_workspace_id, Some(previous_workspace_id));
        assert_eq!(move_result.pane.tab_id, target_tab_id);
        assert_ne!(move_result.pane.pane_id, previous_pane_id);
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.parse_pane_id(&previous_pane_id), Some((0, source)));
    }
    #[test]
    fn api_pane_focus_marks_repeated_done_focus_seen() {
        let mut app = test_app();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.outer_terminal_focus = Some(false);

        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.state.terminals.get_mut(&terminal_id).unwrap().state = crate::detect::AgentState::Idle;
        app.state.workspaces[0].tabs[0]
            .panes
            .get_mut(&pane_id)
            .unwrap()
            .seen = false;
        app.state.workspaces[0].tabs[0].layout.focus_pane(pane_id);
        let public_pane_id = app.public_pane_id(0, pane_id).unwrap();

        let response = app.handle_pane_focus(
            "req".into(),
            PaneTarget {
                pane_id: public_pane_id,
            },
        );

        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        assert!(matches!(success.result, ResponseResult::PaneInfo { .. }));
        assert!(app.state.workspaces[0].tabs[0].panes[&pane_id].seen);
    }
}
