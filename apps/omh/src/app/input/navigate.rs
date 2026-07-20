use std::{
    fs, io,
    io::Write,
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Direction;

use crate::{
    app::{
        state::{AppState, Mode},
        App,
    },
    input::TerminalKey,
    layout::NavDirection,
    terminal::TerminalRuntimeRegistry,
};

#[cfg(test)]
pub(crate) fn terminal_direct_navigation_action(
    state: &AppState,
    key: TerminalKey,
) -> Option<NavigateAction> {
    action_for_key(state, key, BindingDispatch::Direct)
}

pub(crate) fn terminal_direct_non_indexed_navigation_action(
    state: &AppState,
    key: TerminalKey,
) -> Option<NavigateAction> {
    non_indexed_action_for_key(state, key, BindingDispatch::Direct)
}

pub(crate) fn terminal_direct_indexed_navigation_action(
    state: &AppState,
    key: TerminalKey,
) -> Option<NavigateAction> {
    indexed_navigation_action(state, key, BindingDispatch::Direct)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionContext {
    Direct,
    Prefix,
    Navigate,
}

#[derive(Clone, Copy)]
struct CustomCommandTarget {
    ws_idx: usize,
    tab_idx: usize,
    pane_id: crate::layout::PaneId,
}

impl App {
    pub(crate) fn handle_prefix_key(&mut self, raw_key: TerminalKey) {
        let key = raw_key.as_key_event();
        self.state.update_dismissed = true;

        if self.state.is_prefix_key(raw_key) {
            if !self.pass_through_key_to_focused_pane(raw_key) {
                leave_command_mode(&mut self.state);
            }
            return;
        }

        if key.code == KeyCode::Esc {
            leave_command_mode(&mut self.state);
            return;
        }

        if let Some(action) =
            non_indexed_action_for_key(&self.state, raw_key, BindingDispatch::Prefix)
        {
            self.execute_prefix_key_action(action);
            return;
        }

        if let Some(binding) = command_for_key(&self.state, raw_key, BindingDispatch::Prefix) {
            self.launch_custom_command(binding, ActionContext::Prefix);
            return;
        }

        if let Some(action) =
            indexed_navigation_action(&self.state, raw_key, BindingDispatch::Prefix)
        {
            self.execute_prefix_key_action(action);
            return;
        }

        leave_command_mode(&mut self.state);
    }

    pub(crate) fn handle_navigate_key(&mut self, raw_key: TerminalKey) {
        let key = raw_key.as_key_event();
        self.state.update_dismissed = true;

        if key.code == KeyCode::Esc || self.state.is_prefix_key(raw_key) {
            leave_navigate_mode(&mut self.state);
            return;
        }

        if let Some(action) = navigate_reserved_action_for_key(&self.state, raw_key) {
            self.execute_tui_navigate_action(action, ActionContext::Navigate);
            return;
        }

        if let Some(action) = navigate_mode_non_indexed_action_for_key(&self.state, raw_key) {
            if action == NavigateAction::EditScrollback {
                self.launch_focused_scrollback_editor();
            } else {
                self.execute_tui_navigate_action(action, ActionContext::Navigate);
            }
            self.selection_autoscroll_deadline = None;
            return;
        }

        if let Some(binding) = command_for_key(&self.state, raw_key, BindingDispatch::Prefix) {
            self.launch_custom_command(binding, ActionContext::Navigate);
            return;
        }

        if let Some(action) = navigate_mode_indexed_action_for_key(&self.state, raw_key) {
            self.execute_tui_navigate_action(action, ActionContext::Navigate);
            self.selection_autoscroll_deadline = None;
        }
    }

    fn execute_prefix_key_action(&mut self, action: NavigateAction) {
        if action == NavigateAction::EditScrollback {
            let previous_mode = self.state.mode;
            self.launch_focused_scrollback_editor();
            finish_action_context(&mut self.state, ActionContext::Prefix, previous_mode);
        } else {
            self.execute_tui_navigate_action(action, ActionContext::Prefix);
        }
        self.selection_autoscroll_deadline = None;
    }
    pub(crate) fn execute_tui_navigate_action(
        &mut self,
        action: NavigateAction,
        context: ActionContext,
    ) {
        let workspace_target = |state: &AppState| match context {
            ActionContext::Direct | ActionContext::Prefix => state.active,
            ActionContext::Navigate => {
                Some(state.selected).filter(|idx| state.workspace_in_active_group(*idx))
            }
        };

        match action {
            NavigateAction::NewWorkspace => {
                self.dispatch_runtime_mutation(
                    "tui.workspace.create",
                    crate::api::schema::Method::WorkspaceCreate(
                        crate::api::schema::WorkspaceCreateParams {
                            cwd: None,
                            focus: true,
                            label: None,
                            env: Default::default(),
                        },
                    ),
                );
                leave_navigate_mode(&mut self.state);
            }
            NavigateAction::CloseWorkspace => {
                if let Some(ws_idx) = workspace_target(&self.state) {
                    if self.state.confirm_close {
                        self.state.selected = ws_idx;
                        super::modal::open_confirm_close(&mut self.state);
                    } else if let Some(workspace_id) =
                        self.state.workspaces.get(ws_idx).map(|ws| ws.id.clone())
                    {
                        self.runtime_workspace_close("tui.workspace.close", workspace_id);
                        leave_navigate_mode(&mut self.state);
                    }
                }
            }
            NavigateAction::SwitchWorkspace(ws_idx) => {
                if let Some(workspace_id) =
                    self.state.workspaces.get(ws_idx).map(|ws| ws.id.clone())
                {
                    self.runtime_workspace_focus("tui.workspace.focus", workspace_id);
                    leave_navigate_mode(&mut self.state);
                }
            }
            NavigateAction::SwitchTab(tab_idx) => {
                if let Some(ws_idx) = self.state.active {
                    if let Some(tab_id) = self.public_tab_id(ws_idx, tab_idx) {
                        self.runtime_tab_focus("tui.tab.focus", tab_id);
                        leave_navigate_mode(&mut self.state);
                    }
                }
            }
            NavigateAction::PreviousWorkspace | NavigateAction::NextWorkspace => {
                let delta = if matches!(action, NavigateAction::PreviousWorkspace) {
                    -1
                } else {
                    1
                };
                let visible = self.state.sidebar_visible_workspace_indices();
                if let Some(current) = self
                    .state
                    .active
                    .and_then(|idx| visible.iter().position(|candidate| *candidate == idx))
                {
                    if let Some(ws_idx) = current
                        .checked_add_signed(delta)
                        .and_then(|idx| visible.get(idx))
                        .copied()
                    {
                        if let Some(workspace_id) =
                            self.state.workspaces.get(ws_idx).map(|ws| ws.id.clone())
                        {
                            self.runtime_workspace_focus("tui.workspace.focus", workspace_id);
                            leave_navigate_mode(&mut self.state);
                        }
                    }
                }
            }
            NavigateAction::NewTab => {
                if self.state.active.is_some() {
                    if self.state.prompt_new_tab_name {
                        super::modal::open_new_tab_dialog(&mut self.state);
                    } else {
                        self.dispatch_runtime_mutation(
                            "tui.tab.create",
                            crate::api::schema::Method::TabCreate(
                                crate::api::schema::TabCreateParams {
                                    workspace_id: None,
                                    cwd: None,
                                    focus: true,
                                    label: None,
                                    env: Default::default(),
                                },
                            ),
                        );
                        leave_navigate_mode(&mut self.state);
                    }
                }
            }
            NavigateAction::PreviousTab | NavigateAction::NextTab => {
                if let Some(ws_idx) = self.state.active {
                    let tab_count = self
                        .state
                        .workspaces
                        .get(ws_idx)
                        .map_or(0, |ws| ws.tabs.len());
                    if tab_count > 0 {
                        let current = self.state.workspaces[ws_idx].active_tab_index();
                        let delta = if matches!(action, NavigateAction::PreviousTab) {
                            -1
                        } else {
                            1
                        };
                        if let Some(tab_idx) = current.checked_add_signed(delta) {
                            if let Some(tab_id) = self.public_tab_id(ws_idx, tab_idx) {
                                self.runtime_tab_focus("tui.tab.focus", tab_id);
                                leave_navigate_mode(&mut self.state);
                            }
                        }
                    }
                }
            }
            NavigateAction::CloseTab => {
                let Some(ws_idx) = self.state.active else {
                    return;
                };
                let Some((tab_count, active_tab, workspace_id)) = self
                    .state
                    .workspaces
                    .get(ws_idx)
                    .map(|ws| (ws.tabs.len(), ws.active_tab_index(), ws.id.clone()))
                else {
                    return;
                };
                if tab_count <= 1 {
                    if self.state.confirm_implicit_worktree_group_close(ws_idx) {
                        super::modal::open_confirm_close(&mut self.state);
                    } else {
                        self.runtime_workspace_close("tui.workspace.close", workspace_id);
                        leave_navigate_mode(&mut self.state);
                    }
                } else if let Some(tab_id) = self.public_tab_id(ws_idx, active_tab) {
                    self.runtime_tab_close("tui.tab.close", tab_id);
                    leave_navigate_mode(&mut self.state);
                }
            }
            NavigateAction::FocusPaneLeft
            | NavigateAction::FocusPaneDown
            | NavigateAction::FocusPaneUp
            | NavigateAction::FocusPaneRight => {
                let direction = match action {
                    NavigateAction::FocusPaneLeft => crate::api::schema::PaneDirection::Left,
                    NavigateAction::FocusPaneDown => crate::api::schema::PaneDirection::Down,
                    NavigateAction::FocusPaneUp => crate::api::schema::PaneDirection::Up,
                    NavigateAction::FocusPaneRight => crate::api::schema::PaneDirection::Right,
                    _ => unreachable!(),
                };
                self.runtime_pane_focus_direction(
                    "tui.pane.focus_direction",
                    crate::api::schema::PaneFocusDirectionParams {
                        pane_id: None,
                        direction,
                    },
                );
            }
            NavigateAction::SplitVertical | NavigateAction::SplitHorizontal => {
                let direction = if matches!(action, NavigateAction::SplitVertical) {
                    crate::api::schema::SplitDirection::Right
                } else {
                    crate::api::schema::SplitDirection::Down
                };
                self.runtime_pane_split(
                    "tui.pane.split",
                    crate::api::schema::PaneSplitParams {
                        workspace_id: None,
                        target_pane_id: None,
                        direction,
                        ratio: None,
                        cwd: None,
                        focus: true,
                        env: Default::default(),
                    },
                );
                leave_navigate_mode(&mut self.state);
            }
            NavigateAction::ClosePane => {
                let Some(ws_idx) = self.state.active else {
                    return;
                };
                let Some(pane_id) = self
                    .state
                    .workspaces
                    .get(ws_idx)
                    .and_then(|ws| ws.focused_pane_id())
                else {
                    return;
                };
                if self.state.close_pane_would_close_workspace(ws_idx, pane_id)
                    && self.state.confirm_implicit_worktree_group_close(ws_idx)
                {
                    super::modal::open_confirm_close(&mut self.state);
                } else if let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) {
                    self.runtime_pane_close("tui.pane.close", public_pane_id);
                    leave_navigate_mode(&mut self.state);
                }
            }
            NavigateAction::Zoom => {
                self.runtime_pane_zoom(
                    "tui.pane.zoom",
                    crate::api::schema::PaneZoomParams {
                        pane_id: None,
                        mode: crate::api::schema::PaneZoomMode::Toggle,
                    },
                );
                leave_navigate_mode(&mut self.state);
            }
            NavigateAction::CyclePaneNext | NavigateAction::CyclePanePrevious => {
                let reverse = matches!(action, NavigateAction::CyclePanePrevious);
                if let Some(ws_idx) = self.state.active {
                    if let Some(ws) = self.state.workspaces.get(ws_idx) {
                        let tab_idx = ws.active_tab_index();
                        let ids = ws
                            .tabs
                            .get(tab_idx)
                            .map(|tab| tab.layout.pane_ids())
                            .unwrap_or_default();
                        if let Some(current) = ws
                            .focused_pane_id()
                            .and_then(|id| ids.iter().position(|candidate| *candidate == id))
                        {
                            if !ids.is_empty() {
                                let target = if reverse {
                                    ids[(current + ids.len() - 1) % ids.len()]
                                } else {
                                    ids[(current + 1) % ids.len()]
                                };
                                if let Some(public_pane_id) = self.public_pane_id(ws_idx, target) {
                                    self.runtime_pane_focus("tui.pane.focus", public_pane_id);
                                    leave_navigate_mode(&mut self.state);
                                }
                            }
                        }
                    }
                }
            }
            NavigateAction::LastPane => {
                if let Some(target) = self.state.previous_pane_focus.clone() {
                    if let Some(public_pane_id) = self
                        .state
                        .workspaces
                        .iter()
                        .position(|ws| ws.id == target.workspace_id)
                        .and_then(|ws_idx| self.public_pane_id(ws_idx, target.pane_id))
                    {
                        self.runtime_pane_focus("tui.pane.focus", public_pane_id);
                        leave_navigate_mode(&mut self.state);
                    }
                }
            }
            NavigateAction::ReloadConfig => {
                self.dispatch_runtime_mutation(
                    "tui.server.reload_config",
                    crate::api::schema::Method::ServerReloadConfig(
                        crate::api::schema::EmptyParams::default(),
                    ),
                );
                leave_navigate_mode(&mut self.state);
            }
            _ => {
                execute_navigate_action_in_context(
                    &mut self.state,
                    &mut self.terminal_runtimes,
                    action,
                    context,
                );
            }
        }
    }
    fn pass_through_key_to_focused_pane(&mut self, key: TerminalKey) -> bool {
        let Some(ws_idx) = self.state.active else {
            return false;
        };
        let Some(rt) = self
            .state
            .focused_runtime_in_workspace(&self.terminal_runtimes, ws_idx)
        else {
            return false;
        };

        let bytes = rt.encode_terminal_key(key);
        if bytes.is_empty() || rt.try_send_bytes(Bytes::from(bytes)).is_err() {
            return false;
        }

        self.state.mode = Mode::Terminal;
        true
    }

    fn custom_command_target(&self) -> Option<CustomCommandTarget> {
        let ws_idx = self.state.active?;
        let workspace = self.state.workspaces.get(ws_idx)?;
        let tab_idx = workspace.active_tab_index();
        let pane_id = workspace.focused_pane_id()?;
        workspace
            .tabs
            .get(tab_idx)?
            .panes
            .contains_key(&pane_id)
            .then_some(CustomCommandTarget {
                ws_idx,
                tab_idx,
                pane_id,
            })
    }

    fn custom_command_target_for_view(
        &self,
        client_view: &super::super::ClientViewState,
    ) -> Option<CustomCommandTarget> {
        let ws_idx = client_view.active_workspace?;
        let (tab_idx, pane_id) = client_view.focused_pane_for_workspace(&self.state, ws_idx)?;
        Some(CustomCommandTarget {
            ws_idx,
            tab_idx,
            pane_id,
        })
    }

    pub(crate) fn launch_custom_command(
        &mut self,
        binding: crate::config::CustomCommandKeybind,
        context: ActionContext,
    ) {
        let target = self.custom_command_target();
        self.launch_custom_command_at(None, binding, context, target);
    }

    pub(crate) fn launch_custom_command_for_view(
        &mut self,
        client_view: &mut super::super::ClientViewState,
        binding: crate::config::CustomCommandKeybind,
        context: ActionContext,
    ) {
        let target = self.custom_command_target_for_view(client_view);
        self.launch_custom_command_at(Some(client_view), binding, context, target);
    }

    fn launch_custom_command_at(
        &mut self,
        mut client_view: Option<&mut super::super::ClientViewState>,
        binding: crate::config::CustomCommandKeybind,
        context: ActionContext,
        target: Option<CustomCommandTarget>,
    ) {
        let previous_mode = self.state.mode;
        let previous_toast = self.state.toast.clone();
        let result = match binding.action {
            crate::config::CustomCommandAction::Shell => {
                self.spawn_custom_command(&binding, target).map(|_| None)
            }
            crate::config::CustomCommandAction::Pane => target
                .ok_or_else(|| std::io::Error::other("no active workspace"))
                .and_then(|target| {
                    self.spawn_pane_command(
                        &binding.command,
                        Vec::new(),
                        target,
                        client_view
                            .as_deref()
                            .map(super::super::ClientViewState::id),
                    )
                    .map(Some)
                }),
            crate::config::CustomCommandAction::PluginAction => {
                let client_selection = client_view.as_deref().map(|view| view.selection.as_ref());
                self.invoke_plugin_action_from_keybind_at(
                    binding.command.clone(),
                    target.map(|target| (target.ws_idx, target.pane_id)),
                    client_selection,
                )
            }
            .map(|_| None)
            .map_err(std::io::Error::other),
        };
        match result {
            Ok(new_pane) => {
                if let (Some(client_view), Some((ws_idx, tab_idx, pane_id))) =
                    (client_view.as_deref_mut(), new_pane)
                {
                    client_view.focus_client_overlay(&self.state, ws_idx, tab_idx, pane_id);
                } else if client_view.is_none() {
                    finish_custom_command_context(&mut self.state, context, previous_mode);
                }
            }
            Err(err) => {
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::NeedsAttention,
                    title: "custom command failed".to_string(),
                    context: err.to_string(),
                    position: None,
                    target: None,
                });
                self.sync_toast_deadline(previous_toast);
                if client_view.is_none() {
                    finish_custom_command_context(&mut self.state, context, previous_mode);
                }
            }
        }
    }

    fn custom_command_env(
        &self,
        target: Option<CustomCommandTarget>,
    ) -> (Vec<(String, String)>, Option<std::path::PathBuf>) {
        let mut env = vec![(
            crate::api::SOCKET_PATH_ENV_VAR.to_string(),
            crate::api::socket_path().display().to_string(),
        )];
        if let Ok(current_exe) = std::env::current_exe() {
            env.push((
                "OMH_BIN_PATH".to_string(),
                current_exe.display().to_string(),
            ));
        }
        let mut cwd = None;
        if let Some(target) = target {
            env.push((
                "OMH_ACTIVE_WORKSPACE_ID".to_string(),
                self.public_workspace_id(target.ws_idx),
            ));
            if let Some(tab_id) = self.public_tab_id(target.ws_idx, target.tab_idx) {
                env.push(("OMH_ACTIVE_TAB_ID".to_string(), tab_id));
            }
            if let Some(pane_id) = self.public_pane_id(target.ws_idx, target.pane_id) {
                env.push(("OMH_ACTIVE_PANE_ID".to_string(), pane_id));
            }
            if let Some(pane_cwd) = self
                .state
                .workspaces
                .get(target.ws_idx)
                .and_then(|workspace| workspace.tabs.get(target.tab_idx))
                .and_then(|tab| {
                    tab.cwd_for_pane(
                        target.pane_id,
                        &self.state.terminals,
                        &self.terminal_runtimes,
                    )
                })
            {
                env.push((
                    "OMH_ACTIVE_PANE_CWD".to_string(),
                    pane_cwd.display().to_string(),
                ));
                if pane_cwd.is_dir() {
                    cwd = Some(pane_cwd);
                }
            }
        }
        (env, cwd)
    }

    fn spawn_custom_command(
        &mut self,
        binding: &crate::config::CustomCommandKeybind,
        target: Option<CustomCommandTarget>,
    ) -> std::io::Result<()> {
        let mut command = crate::platform::detached_custom_command_process(&binding.command);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let (env, cwd) = self.custom_command_env(target);
        command.envs(env);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        let child = command.spawn()?;
        self.detached_custom_command_children.push(child);
        Ok(())
    }

    pub(crate) fn launch_focused_scrollback_editor(&mut self) {
        let previous_toast = self.state.toast.clone();
        match self.open_focused_scrollback_in_editor() {
            Ok(()) => self.sync_toast_deadline(previous_toast),
            Err(err) => {
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::NeedsAttention,
                    title: "edit scrollback failed".to_string(),
                    context: err.to_string(),
                    position: None,
                    target: None,
                });
                self.sync_toast_deadline(previous_toast);
            }
        }
    }

    fn open_focused_scrollback_in_editor(&mut self) -> std::io::Result<()> {
        let ws_idx = self
            .state
            .active
            .ok_or_else(|| std::io::Error::other("no active workspace"))?;
        let ws = self
            .state
            .workspaces
            .get(ws_idx)
            .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
        let pane_id = ws
            .focused_pane_id()
            .ok_or_else(|| std::io::Error::other("no focused pane"))?;
        let scrollback = self
            .state
            .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
            .ok_or_else(|| std::io::Error::other("focused pane has no scrollback runtime"))?
            .recent_text(usize::MAX);

        let path = write_scrollback_temp_file(&scrollback)?;

        let argv = match crate::platform::scrollback_editor_argv(&path) {
            Ok(argv) => argv,
            Err(err) => {
                let _ = fs::remove_file(&path);
                return Err(err);
            }
        };
        let target = self
            .custom_command_target()
            .ok_or_else(|| std::io::Error::other("no active workspace"))?;
        let (env, cwd) = self.custom_command_env(Some(target));
        let (_, new_pane) =
            match self.spawn_overlay_argv_command(&argv, cwd, env, vec![path.clone()]) {
                Ok(result) => result,
                Err(err) => {
                    let _ = fs::remove_file(&path);
                    return Err(err);
                }
            };
        let terminal_id = new_pane.terminal.id.clone();
        self.terminal_runtimes
            .insert(terminal_id.clone(), new_pane.runtime);
        self.state
            .remove_alias_shadowed_by_new_pane(new_pane.pane_id);
        self.state.terminals.insert(terminal_id, new_pane.terminal);

        if let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) {
            self.state.toast = Some(crate::app::state::ToastNotification {
                kind: crate::app::state::ToastKind::Finished,
                title: "opened scrollback".to_string(),
                context: format!("focused pane {public_pane_id}"),
                position: None,
                target: None,
            });
        }
        Ok(())
    }

    fn spawn_pane_command(
        &mut self,
        command: &str,
        temp_files: Vec<std::path::PathBuf>,
        target: CustomCommandTarget,
        client_owner: Option<u64>,
    ) -> std::io::Result<(usize, usize, crate::layout::PaneId)> {
        let (rows, cols) = self.state.estimate_pane_size();
        let new_rows = rows.max(4);
        let new_cols = cols.max(10);
        let (env, cwd) = self.custom_command_env(Some(target));
        let previous_zoomed = self.state.workspaces[target.ws_idx].tabs[target.tab_idx].zoomed;

        #[cfg(test)]
        if self.state.workspaces[target.ws_idx]
            .test_runtimes
            .contains_key(&target.pane_id)
        {
            let ws = &mut self.state.workspaces[target.ws_idx];
            let previous_active_tab = ws.active_tab;
            let previous_layout_focus = ws.tabs[target.tab_idx].layout.focused();
            ws.active_tab = target.tab_idx;
            ws.tabs[target.tab_idx].layout.focus_pane(target.pane_id);
            let new_pane_id = ws.test_split(Direction::Horizontal);
            if client_owner.is_some() {
                ws.tabs[target.tab_idx]
                    .layout
                    .focus_pane(previous_layout_focus);
                ws.active_tab = previous_active_tab;
            } else {
                ws.tabs[target.tab_idx].layout.focus_pane(new_pane_id);
                ws.tabs[target.tab_idx].zoomed = true;
            }
            self.overlay_panes.insert(
                new_pane_id,
                super::super::OverlayPaneState {
                    ws_idx: target.ws_idx,
                    tab_idx: target.tab_idx,
                    owner: if client_owner.is_some() {
                        super::super::OverlayPaneOwner::Client
                    } else {
                        super::super::OverlayPaneOwner::Shared {
                            previous_focus: target.pane_id,
                            previous_zoomed,
                        }
                    },
                    temp_files,
                },
            );
            if let Some(view_id) = client_owner {
                self.state
                    .client_overlay_owners
                    .insert(new_pane_id, view_id);
            }
            if client_owner.is_none() {
                self.state.mode = Mode::Terminal;
            }
            return Ok((target.ws_idx, target.tab_idx, new_pane_id));
        }

        let (tab_idx, new_pane) = {
            let ws = self
                .state
                .workspaces
                .get_mut(target.ws_idx)
                .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
            match ws.split_pane_custom_command(
                target.pane_id,
                Direction::Horizontal,
                new_rows,
                new_cols,
                cwd,
                command,
                env,
                self.state.pane_scrollback_limit_bytes,
                self.state.host_terminal_theme,
                false,
            ) {
                Some(Ok(result)) => result,
                Some(Err(err)) => return Err(err),
                None => return Err(std::io::Error::other("focused pane disappeared")),
            }
        };
        let new_pane_id = new_pane.pane_id;
        self.terminal_runtimes
            .insert(new_pane.terminal.id.clone(), new_pane.runtime);
        self.state
            .terminals
            .insert(new_pane.terminal.id.clone(), new_pane.terminal);
        self.overlay_panes.insert(
            new_pane_id,
            super::super::OverlayPaneState {
                ws_idx: target.ws_idx,
                tab_idx,
                owner: if client_owner.is_some() {
                    super::super::OverlayPaneOwner::Client
                } else {
                    super::super::OverlayPaneOwner::Shared {
                        previous_focus: target.pane_id,
                        previous_zoomed,
                    }
                },
                temp_files,
            },
        );
        if let Some(view_id) = client_owner {
            self.state
                .client_overlay_owners
                .insert(new_pane_id, view_id);
        }
        self.state.remove_alias_shadowed_by_new_pane(new_pane_id);
        if client_owner.is_none() {
            let ws = &mut self.state.workspaces[target.ws_idx];
            ws.active_tab = tab_idx;
            ws.tabs[tab_idx].layout.focus_pane(new_pane_id);
            ws.tabs[tab_idx].zoomed = true;
            self.state.mode = Mode::Terminal;
        }
        Ok((target.ws_idx, tab_idx, new_pane_id))
    }

    pub(crate) fn spawn_overlay_argv_command(
        &mut self,
        argv: &[String],
        cwd: Option<std::path::PathBuf>,
        extra_env: Vec<(String, String)>,
        temp_files: Vec<std::path::PathBuf>,
    ) -> std::io::Result<(usize, crate::workspace::NewPane)> {
        let Some(ws_idx) = self.state.active else {
            return Err(std::io::Error::other("no active workspace"));
        };
        let previous_focus_target = self.state.current_pane_focus_target();
        let (rows, cols) = self.state.estimate_pane_size();
        let new_rows = rows.max(4);
        let new_cols = cols.max(10);

        let ws = self
            .state
            .workspaces
            .get(ws_idx)
            .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
        let previous_focus = ws
            .focused_pane_id()
            .ok_or_else(|| std::io::Error::other("no focused pane"))?;
        let cwd = cwd.or_else(|| {
            ws.active_tab().and_then(|tab| {
                tab.cwd_for_pane(
                    previous_focus,
                    &self.state.terminals,
                    &self.terminal_runtimes,
                )
            })
        });

        let (tab_idx, new_pane, workspace_id) = {
            let ws = self
                .state
                .workspaces
                .get_mut(ws_idx)
                .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
            let previous_zoomed = ws.active_tab().map(|tab| tab.zoomed).unwrap_or(false);
            let result = ws.split_pane_argv_command(
                previous_focus,
                Direction::Horizontal,
                new_rows,
                new_cols,
                cwd,
                argv,
                extra_env,
                self.state.pane_scrollback_limit_bytes,
                self.state.host_terminal_theme,
                true,
            );
            let (tab_idx, new_pane) = match result {
                Some(Ok(result)) => result,
                Some(Err(err)) => return Err(err),
                None => return Err(std::io::Error::other("focused pane disappeared")),
            };
            ws.tabs
                .get_mut(tab_idx)
                .ok_or_else(|| std::io::Error::other("plugin overlay tab disappeared"))?
                .zoomed = true;
            self.overlay_panes.insert(
                new_pane.pane_id,
                super::super::OverlayPaneState {
                    ws_idx,
                    tab_idx,
                    owner: super::super::OverlayPaneOwner::Shared {
                        previous_focus,
                        previous_zoomed,
                    },
                    temp_files,
                },
            );
            (tab_idx, new_pane, ws.id.clone())
        };

        let new_focus_target = crate::app::state::PaneFocusTarget {
            workspace_id,
            pane_id: new_pane.pane_id,
        };
        if previous_focus_target.as_ref() != Some(&new_focus_target) {
            self.state.previous_pane_focus = previous_focus_target;
        }
        self.state.switch_workspace_tab(ws_idx, tab_idx);
        self.state.mode = Mode::Terminal;
        Ok((ws_idx, new_pane))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingDispatch {
    Direct,
    Prefix,
}

pub(crate) fn command_for_key(
    state: &AppState,
    key: TerminalKey,
    dispatch: BindingDispatch,
) -> Option<crate::config::CustomCommandKeybind> {
    state
        .keybinds
        .custom_commands
        .iter()
        .find(|binding| match dispatch {
            BindingDispatch::Direct => binding.bindings.matches_direct_key(key),
            BindingDispatch::Prefix => binding.bindings.matches_prefix_key(key),
        })
        .cloned()
}

fn navigate_reserved_action_for_key(state: &AppState, key: TerminalKey) -> Option<NavigateAction> {
    let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Tab => Some(NavigateAction::CyclePaneNext),
        KeyCode::BackTab => Some(NavigateAction::CyclePanePrevious),
        KeyCode::Left => Some(NavigateAction::FocusPaneLeft),
        KeyCode::Right => Some(NavigateAction::FocusPaneRight),
        KeyCode::Up if state.keybinds.navigate.pane_up.matches_direct_key(key) => {
            Some(NavigateAction::FocusPaneUp)
        }
        KeyCode::Down if state.keybinds.navigate.pane_down.matches_direct_key(key) => {
            Some(NavigateAction::FocusPaneDown)
        }
        _ => None,
    }
}

pub(super) fn handle_navigate_reserved_key(state: &mut AppState, key: TerminalKey) -> bool {
    let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
    if modifiers.is_empty() {
        match code {
            KeyCode::Enter => {
                if state.workspace_in_active_group(state.selected) {
                    state.switch_workspace(state.selected);
                    leave_navigate_mode(state);
                }
                return true;
            }
            KeyCode::Char(c @ ('1'..='9' | '0')) => {
                let idx = if c == '0' {
                    9
                } else {
                    (c as usize) - ('1' as usize)
                };
                if let Some(ws_idx) = state.sidebar_visible_workspace_indices().get(idx).copied() {
                    state.switch_workspace(ws_idx);
                    leave_navigate_mode(state);
                }
                return true;
            }
            KeyCode::Tab => {
                state.cycle_pane(false);
                return true;
            }
            KeyCode::BackTab => {
                state.cycle_pane(true);
                return true;
            }
            KeyCode::Left => {
                state.navigate_pane(NavDirection::Left);
                return true;
            }
            KeyCode::Right => {
                state.navigate_pane(NavDirection::Right);
                return true;
            }
            _ => {}
        }
    }

    if state.keybinds.navigate.workspace_up.matches_direct_key(key) {
        move_selected_workspace_by_visible_delta(state, -1);
        return true;
    }
    if state
        .keybinds
        .navigate
        .workspace_down
        .matches_direct_key(key)
    {
        move_selected_workspace_by_visible_delta(state, 1);
        return true;
    }
    if state.keybinds.navigate.pane_left.matches_direct_key(key) {
        state.navigate_pane(NavDirection::Left);
        return true;
    }
    if state.keybinds.navigate.pane_down.matches_direct_key(key) {
        state.navigate_pane(NavDirection::Down);
        return true;
    }
    if state.keybinds.navigate.pane_up.matches_direct_key(key) {
        state.navigate_pane(NavDirection::Up);
        return true;
    }
    if state.keybinds.navigate.pane_right.matches_direct_key(key) {
        state.navigate_pane(NavDirection::Right);
        return true;
    }

    false
}

fn move_selected_workspace_by_visible_delta(state: &mut AppState, delta: isize) {
    let visible = state.sidebar_visible_workspace_indices();
    let Some(pos) = visible.iter().position(|idx| *idx == state.selected) else {
        return;
    };
    let Some(next) = pos
        .checked_add_signed(delta)
        .and_then(|idx| visible.get(idx))
    else {
        return;
    };
    state.selected = *next;
    state.ensure_workspace_visible(state.selected);
}

#[allow(dead_code)] // exercised in input unit tests; production uses App::handle_navigate_key
pub(crate) fn handle_navigate_key(state: &mut AppState, key: KeyEvent) {
    let mut terminal_runtimes = TerminalRuntimeRegistry::new();
    state.update_dismissed = true;
    let terminal_key = TerminalKey::from(key);

    if state.is_prefix_key(terminal_key) || key.code == KeyCode::Esc {
        leave_navigate_mode(state);
        return;
    }

    if handle_navigate_reserved_key(state, terminal_key) {
        return;
    }

    if let Some(action) = navigate_mode_action_for_key(state, terminal_key) {
        execute_navigate_action_in_context(
            state,
            &mut terminal_runtimes,
            action,
            ActionContext::Navigate,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigateAction {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    SwitchWorkspace(usize),
    SwitchTab(usize),
    FocusAgent(usize),
    WorkspacePicker,
    PreviousWorkspace,
    NextWorkspace,
    OpenGroupMenu,
    NewGroup,
    RenameGroup,
    DeleteGroup,
    ToggleGroupFilter,
    PreviousGroup,
    NextGroup,
    SwitchGroup(usize),
    PreviousAgent,
    NextAgent,
    OpenAgentMenu,
    NewTab,
    RenameTab,
    PreviousTab,
    NextTab,
    CloseTab,
    RenamePane,
    FocusPaneLeft,
    FocusPaneDown,
    FocusPaneUp,
    FocusPaneRight,
    SplitVertical,
    SplitHorizontal,
    ClosePane,
    EditScrollback,
    CopyMode,
    Zoom,
    ToggleContextBar,
    EnterResizeMode,
    ToggleSidebar,
    ToggleRightSidebar,
    OpenCommandPalette,
    CyclePaneNext,
    CyclePanePrevious,
    LastPane,
    Help,
    Settings,
    ReloadConfig,
    OpenNotificationTarget,
    Detach,
}

pub(crate) fn indexed_navigation_action(
    state: &AppState,
    key: TerminalKey,
    dispatch: BindingDispatch,
) -> Option<NavigateAction> {
    let kb = &state.keybinds;
    let trigger_matches = |binding: &crate::config::IndexedKeybind| match dispatch {
        BindingDispatch::Direct => binding.trigger.is_direct(),
        BindingDispatch::Prefix => binding.trigger.is_prefix(),
    };

    for binding in &kb.switch_tab {
        if trigger_matches(binding) {
            if let Some(idx) = binding.matched_index(key) {
                return Some(NavigateAction::SwitchTab(idx));
            }
        }
    }
    for binding in &kb.switch_workspace {
        if trigger_matches(binding) {
            if let Some(idx) = binding.matched_index(key) {
                return state
                    .sidebar_visible_workspace_indices()
                    .get(idx)
                    .copied()
                    .map(NavigateAction::SwitchWorkspace);
            }
        }
    }
    for binding in &kb.switch_group {
        if trigger_matches(binding) {
            if let Some(idx) = binding.matched_index(key) {
                return Some(NavigateAction::SwitchGroup(idx));
            }
        }
    }
    for binding in &kb.focus_agent {
        if trigger_matches(binding) {
            if let Some(idx) = binding.matched_index(key) {
                return Some(NavigateAction::FocusAgent(idx));
            }
        }
    }

    None
}

fn action_matches(
    bindings: &crate::config::ActionKeybinds,
    key: TerminalKey,
    dispatch: BindingDispatch,
) -> bool {
    match dispatch {
        BindingDispatch::Direct => bindings.matches_direct_key(key),
        BindingDispatch::Prefix => bindings.matches_prefix_key(key),
    }
}

#[cfg(test)]
pub(crate) fn action_for_key(
    state: &AppState,
    key: TerminalKey,
    dispatch: BindingDispatch,
) -> Option<NavigateAction> {
    non_indexed_action_for_key(state, key, dispatch)
        .or_else(|| indexed_navigation_action(state, key, dispatch))
}

pub(crate) fn non_indexed_action_for_key(
    state: &AppState,
    key: TerminalKey,
    dispatch: BindingDispatch,
) -> Option<NavigateAction> {
    let kb = &state.keybinds;
    for (bindings, action) in [
        (&kb.help, NavigateAction::Help),
        (&kb.settings, NavigateAction::Settings),
        (&kb.workspace_picker, NavigateAction::WorkspacePicker),
        (&kb.goto, NavigateAction::WorkspacePicker),
        (&kb.new_workspace, NavigateAction::NewWorkspace),
        (&kb.rename_workspace, NavigateAction::RenameWorkspace),
        (&kb.close_workspace, NavigateAction::CloseWorkspace),
        (&kb.previous_workspace, NavigateAction::PreviousWorkspace),
        (&kb.next_workspace, NavigateAction::NextWorkspace),
        (&kb.open_group_menu, NavigateAction::OpenGroupMenu),
        (&kb.new_group, NavigateAction::NewGroup),
        (&kb.rename_group, NavigateAction::RenameGroup),
        (&kb.delete_group, NavigateAction::DeleteGroup),
        (&kb.toggle_group_filter, NavigateAction::ToggleGroupFilter),
        (&kb.previous_group, NavigateAction::PreviousGroup),
        (&kb.next_group, NavigateAction::NextGroup),
        (&kb.previous_agent, NavigateAction::PreviousAgent),
        (&kb.next_agent, NavigateAction::NextAgent),
        (&kb.open_agent_menu, NavigateAction::OpenAgentMenu),
        (&kb.new_tab, NavigateAction::NewTab),
        (&kb.rename_tab, NavigateAction::RenameTab),
        (&kb.previous_tab, NavigateAction::PreviousTab),
        (&kb.next_tab, NavigateAction::NextTab),
        (&kb.close_tab, NavigateAction::CloseTab),
        (&kb.rename_pane, NavigateAction::RenamePane),
        (&kb.edit_scrollback, NavigateAction::EditScrollback),
        (&kb.copy_mode, NavigateAction::CopyMode),
        (&kb.focus_pane_left, NavigateAction::FocusPaneLeft),
        (&kb.focus_pane_down, NavigateAction::FocusPaneDown),
        (&kb.focus_pane_up, NavigateAction::FocusPaneUp),
        (&kb.focus_pane_right, NavigateAction::FocusPaneRight),
        (&kb.cycle_pane_next, NavigateAction::CyclePaneNext),
        (&kb.cycle_pane_previous, NavigateAction::CyclePanePrevious),
        (&kb.last_pane, NavigateAction::LastPane),
        (&kb.split_vertical, NavigateAction::SplitVertical),
        (&kb.split_horizontal, NavigateAction::SplitHorizontal),
        (&kb.close_pane, NavigateAction::ClosePane),
        (&kb.zoom, NavigateAction::Zoom),
        (&kb.resize_mode, NavigateAction::EnterResizeMode),
        (&kb.toggle_sidebar, NavigateAction::ToggleSidebar),
        (&kb.toggle_context_bar, NavigateAction::ToggleContextBar),
        (&kb.toggle_right_sidebar, NavigateAction::ToggleRightSidebar),
        (&kb.command_palette, NavigateAction::OpenCommandPalette),
        (&kb.reload_config, NavigateAction::ReloadConfig),
        (
            &kb.open_notification_target,
            NavigateAction::OpenNotificationTarget,
        ),
        (&kb.detach, NavigateAction::Detach),
    ] {
        if action_matches(bindings, key, dispatch) {
            return Some(action);
        }
    }
    None
}

fn navigate_mode_action_for_key(state: &AppState, key: TerminalKey) -> Option<NavigateAction> {
    navigate_mode_non_indexed_action_for_key(state, key)
        .or_else(|| navigate_mode_indexed_action_for_key(state, key))
}

fn navigate_mode_non_indexed_action_for_key(
    state: &AppState,
    key: TerminalKey,
) -> Option<NavigateAction> {
    let action = non_indexed_action_for_key(state, key, BindingDispatch::Prefix)?;
    if matches!(
        action,
        NavigateAction::FocusPaneLeft
            | NavigateAction::FocusPaneDown
            | NavigateAction::FocusPaneUp
            | NavigateAction::FocusPaneRight
    ) {
        return None;
    }
    Some(action)
}

fn navigate_mode_indexed_action_for_key(
    state: &AppState,
    key: TerminalKey,
) -> Option<NavigateAction> {
    indexed_navigation_action(state, key, BindingDispatch::Prefix)
}

#[cfg(test)]
pub(super) fn execute_navigate_action(state: &mut AppState, action: NavigateAction) {
    let mut terminal_runtimes = TerminalRuntimeRegistry::new();
    execute_navigate_action_in_context(
        state,
        &mut terminal_runtimes,
        action,
        ActionContext::Navigate,
    );
}

pub(crate) fn execute_navigate_action_in_context(
    state: &mut AppState,
    terminal_runtimes: &mut TerminalRuntimeRegistry,
    action: NavigateAction,
    context: ActionContext,
) {
    let previous_mode = state.mode;
    match action {
        NavigateAction::NewWorkspace => {
            state.request_new_workspace = true;
            leave_navigate_mode(state);
        }
        NavigateAction::RenameWorkspace => {
            if let Some(ws_idx) = workspace_action_target(state, context) {
                super::modal::open_rename_workspace(state, terminal_runtimes, ws_idx);
            }
        }
        NavigateAction::CloseWorkspace => {
            if state.workspace_in_active_group(state.selected) {
                if state.confirm_close {
                    super::modal::open_confirm_close(state);
                } else {
                    state.close_selected_workspace_from_ui();
                }
            }
        }
        NavigateAction::SwitchWorkspace(idx) => {
            if idx < state.workspaces.len() {
                state.switch_workspace(idx);
                leave_navigate_mode(state);
            }
        }
        NavigateAction::SwitchTab(idx) => {
            let tab_exists = state
                .active
                .and_then(|ws_idx| state.workspaces.get(ws_idx))
                .is_some_and(|ws| idx < ws.tabs.len());
            if tab_exists {
                state.switch_tab(idx);
                leave_navigate_mode(state);
            }
        }
        NavigateAction::SwitchGroup(idx) => {
            if idx < state.groups.len() {
                state.switch_group(idx);
                leave_navigate_mode(state);
            }
        }
        NavigateAction::FocusAgent(idx) => {
            if state.focus_agent_entry(idx) {
                leave_navigate_mode(state);
            }
        }
        NavigateAction::WorkspacePicker => state.open_navigator(),
        NavigateAction::PreviousWorkspace => {
            state.previous_workspace();
            leave_navigate_mode(state);
        }
        NavigateAction::NextWorkspace => {
            state.next_workspace();
            leave_navigate_mode(state);
        }
        NavigateAction::OpenGroupMenu => super::modal::open_group_menu(state),
        NavigateAction::NewGroup => super::modal::open_new_group_dialog(state),
        NavigateAction::RenameGroup => super::modal::open_rename_group(state),
        NavigateAction::DeleteGroup => {
            super::modal::open_confirm_delete_group(state, state.active_group)
        }
        NavigateAction::ToggleGroupFilter => {
            state.toggle_group_filter();
            leave_navigate_mode(state);
        }
        NavigateAction::PreviousGroup => {
            state.previous_group();
            leave_navigate_mode(state);
        }
        NavigateAction::NextGroup => {
            state.next_group();
            leave_navigate_mode(state);
        }
        NavigateAction::PreviousAgent => {
            state.previous_agent();
            leave_navigate_mode(state);
        }
        NavigateAction::NextAgent => {
            state.next_agent();
            leave_navigate_mode(state);
        }
        NavigateAction::OpenAgentMenu => super::modal::open_agent_menu(state),
        NavigateAction::NewTab => {
            if state.active.is_some() {
                if state.prompt_new_tab_name {
                    super::modal::open_new_tab_dialog(state);
                } else {
                    state.request_new_tab = true;
                    leave_navigate_mode(state);
                }
            }
        }
        NavigateAction::RenameTab => super::modal::open_rename_active_tab(state, false),
        NavigateAction::PreviousTab => {
            state.previous_tab();
            leave_navigate_mode(state);
        }
        NavigateAction::NextTab => {
            state.next_tab();
            leave_navigate_mode(state);
        }
        NavigateAction::CloseTab => {
            if !state.close_tab() {
                leave_navigate_mode(state);
            }
        }
        NavigateAction::RenamePane => {
            if let Some(pane_id) = state
                .active
                .and_then(|ws_idx| state.workspaces.get(ws_idx))
                .and_then(|ws| ws.focused_pane_id())
            {
                super::modal::open_rename_pane(state, pane_id);
            }
        }
        NavigateAction::FocusPaneLeft => state.navigate_pane(NavDirection::Left),
        NavigateAction::FocusPaneDown => state.navigate_pane(NavDirection::Down),
        NavigateAction::FocusPaneUp => state.navigate_pane(NavDirection::Up),
        NavigateAction::FocusPaneRight => state.navigate_pane(NavDirection::Right),
        NavigateAction::SplitVertical => {
            state.split_pane(terminal_runtimes, Direction::Horizontal);
            leave_navigate_mode(state);
        }
        NavigateAction::SplitHorizontal => {
            state.split_pane(terminal_runtimes, Direction::Vertical);
            leave_navigate_mode(state);
        }
        NavigateAction::ClosePane => {
            if !state.close_pane() {
                leave_navigate_mode(state);
            }
        }
        NavigateAction::EditScrollback => {}
        NavigateAction::CopyMode => state.enter_copy_mode(terminal_runtimes),
        NavigateAction::Zoom => {
            state.toggle_zoom();
            leave_navigate_mode(state);
        }
        NavigateAction::EnterResizeMode => state.mode = Mode::Resize,
        NavigateAction::ToggleSidebar => {
            state.sidebar_collapsed = !state.sidebar_collapsed;
            state.mark_session_dirty();
            leave_navigate_mode(state);
        }
        NavigateAction::ToggleContextBar => {
            let visible = state.context_bar_is_visible(state.context_bar_visibility_override);
            state.context_bar_visibility_override = Some(!visible);
            leave_navigate_mode(state);
        }
        NavigateAction::ToggleRightSidebar => {
            if state.view.right_sidebar_rect != ratatui::layout::Rect::default() {
                state.right_sidebar_collapsed = !state.right_sidebar_collapsed;
                state.mark_session_dirty();
            }
            leave_navigate_mode(state);
        }
        NavigateAction::OpenCommandPalette => super::command_palette::open_command_palette(state),
        NavigateAction::CyclePaneNext => {
            state.cycle_pane(false);
            leave_navigate_mode(state);
        }
        NavigateAction::CyclePanePrevious => {
            state.cycle_pane(true);
            leave_navigate_mode(state);
        }
        NavigateAction::LastPane => {
            state.last_pane();
            leave_navigate_mode(state);
        }
        NavigateAction::Help => super::modal::open_keybind_help(state),
        NavigateAction::Settings => super::settings::open_settings(state),
        NavigateAction::ReloadConfig => {
            state.request_reload_config = true;
            leave_navigate_mode(state);
        }
        NavigateAction::OpenNotificationTarget => {
            state.focus_toast_target();
            if state.mode == Mode::Navigate {
                leave_navigate_mode(state);
            }
        }
        NavigateAction::Detach => {
            super::modal::request_detach(state);
            leave_navigate_mode(state);
        }
    }

    finish_action_context(state, context, previous_mode);
}

fn workspace_action_target(state: &AppState, context: ActionContext) -> Option<usize> {
    match context {
        ActionContext::Direct | ActionContext::Prefix => state.active,
        ActionContext::Navigate => {
            Some(state.selected).filter(|idx| state.workspace_in_active_group(*idx))
        }
    }
}
fn leave_navigate_mode(state: &mut AppState) {
    state.return_to_active_workspace_mode();
}

fn finish_action_context(state: &mut AppState, context: ActionContext, previous_mode: Mode) {
    if matches!(context, ActionContext::Direct | ActionContext::Prefix)
        && state.mode == previous_mode
    {
        leave_command_mode(state);
    }
}

fn finish_custom_command_context(
    state: &mut AppState,
    context: ActionContext,
    previous_mode: Mode,
) {
    if context == ActionContext::Navigate {
        leave_navigate_mode(state);
    } else {
        finish_action_context(state, context, previous_mode);
    }
}

fn leave_command_mode(state: &mut AppState) {
    state.return_to_active_workspace_mode();
}

fn write_scrollback_temp_file(content: &str) -> io::Result<std::path::PathBuf> {
    let mut last_collision = None;
    for attempt in 0..16 {
        let path = unique_scrollback_path(attempt);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(content.as_bytes())?;
                return Ok(path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                last_collision = Some(err);
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_collision.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "failed to create unique scrollback temp file",
        )
    }))
}

fn unique_scrollback_path(attempt: u32) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "omh-scrollback-{}-{nanos}-{attempt}.txt",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Direction;

    use super::super::{state_with_workspaces, unique_temp_path, wait_for_file};
    use super::*;
    use crate::{
        app::{state::Group, App},
        config::Config,
        input::TerminalKey,
        terminal::TerminalState,
        workspace::Workspace,
    };

    fn mark_worktree_space_member(state: &mut AppState, ws_idx: usize, key: &str) {
        state.workspaces[ws_idx].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: key.into(),
            label: "omh".into(),
            repo_root: "/repo/omh".into(),
            checkout_path: format!("/repo/omh-{ws_idx}").into(),
            is_linked_worktree: ws_idx != 0,
        });
    }

    #[test]
    fn custom_rename_key_enters_rename_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.rename_workspace = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::RenameWorkspace);
        assert_eq!(state.name_input, "test");
    }

    #[test]
    fn rename_workspace_prefills_live_terminal_cwd_label() {
        let mut state = state_with_workspaces(&["stale"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.workspaces[0].panes[&root]
            .attached_terminal_id
            .clone();
        state.workspaces[0].custom_name = None;
        state.workspaces[0].identity_cwd = "/__omh_original__".into();
        state.terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id, "/__omh_projects__".into()),
        );
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.rename_workspace = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::RenameWorkspace);
        assert_eq!(state.name_input, "__omh_projects__");
        assert_eq!(state.workspaces[0].display_name(), "__omh_original__");
    }

    #[test]
    fn prefix_rename_workspace_targets_active_workspace_not_stale_selection() {
        let mut state = state_with_workspaces(&["main", "issue"]);
        let mut terminal_runtimes = TerminalRuntimeRegistry::new();
        state.active = Some(1);
        state.selected = 0;
        state.mode = Mode::Prefix;

        execute_navigate_action_in_context(
            &mut state,
            &mut terminal_runtimes,
            NavigateAction::RenameWorkspace,
            ActionContext::Prefix,
        );

        assert_eq!(state.mode, Mode::RenameWorkspace);
        assert_eq!(state.selected, 1);
        assert_eq!(state.name_input, "issue");
    }

    #[test]
    fn custom_new_workspace_key_requests_and_exits_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.new_workspace = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert!(state.request_new_workspace);
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn close_tab_action_empties_workspace_when_closing_last_tab() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Navigate;
        state.active = Some(0);
        state.selected = 0;
        state.confirm_close = true;

        execute_navigate_action(&mut state, NavigateAction::CloseTab);

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.workspaces.len(), 1);
        assert!(state.workspaces[0].tabs.is_empty());
    }

    #[test]
    fn close_workspace_action_deletes_last_space_and_shows_empty_group() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Navigate;
        state.active = Some(0);
        state.selected = 0;
        state.confirm_close = false;

        execute_navigate_action(&mut state, NavigateAction::CloseWorkspace);

        assert_eq!(state.mode, Mode::Navigate);
        assert!(state.workspaces.is_empty());
        assert_eq!(state.active, None);
    }

    #[test]
    fn custom_sidebar_toggle_key_toggles_and_exits_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.toggle_sidebar = crate::config::ActionKeybinds::prefix("g");
        assert!(!state.sidebar_collapsed);

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert!(state.sidebar_collapsed);
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn custom_resize_key_enters_resize_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.resize_mode = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Resize);
    }

    #[test]
    fn custom_reload_config_key_requests_reload_and_exits_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.reload_config = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert!(state.request_reload_config);
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn custom_open_notification_key_focuses_current_toast_target() {
        let mut state = state_with_workspaces(&["one", "two"]);
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Navigate;
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.open_notification_target = crate::config::ActionKeybinds::prefix("g");
        let target_workspace_id = state.workspaces[1].id.clone();
        let target_pane = state.workspaces[1].tabs[0].root_pane;
        state.toast = Some(crate::app::state::ToastNotification {
            kind: crate::app::state::ToastKind::NeedsAttention,
            title: "pi needs attention".into(),
            context: "two".into(),
            position: None,
            target: Some(crate::app::state::ToastTarget {
                workspace_id: target_workspace_id,
                pane_id: target_pane,
            }),
        });

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.active, Some(1));
        assert_eq!(state.selected, 1);
        assert_eq!(state.workspaces[1].focused_pane_id(), Some(target_pane));
        assert!(state.toast.is_none());
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn movement_action_stays_in_navigate_mode() {
        let mut state = state_with_workspaces(&["a", "b"]);
        state.selected = 0;

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );

        assert_eq!(state.selected, 1);
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn keyboard_movement_skips_collapsed_group_workspaces() {
        let mut state = state_with_workspaces(&["a", "b", "c"]);
        let side_group = state.create_group("side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.toggle_workspace_group(side_group);
        state.selected = 0;

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );

        assert_eq!(state.selected, 2);
    }

    #[test]
    fn navigate_workspace_keys_are_configurable() {
        let mut state = state_with_workspaces(&["a", "b"]);
        let config: Config = toml::from_str(
            r#"
[keys]
navigate_workspace_down = "j"
navigate_pane_down = "ctrl+j"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();
        state.selected = 0;

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty()),
        );

        assert_eq!(state.selected, 1);
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn navigate_pane_keys_are_configurable() {
        let mut state = state_with_workspaces(&["test"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let below = state.workspaces[0].test_split(Direction::Vertical);
        state.workspaces[0].layout.focus_pane(root);
        state.view.pane_infos = state.workspaces[0]
            .active_tab()
            .unwrap()
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 80, 24));
        let config: Config = toml::from_str(
            r#"
[keys]
navigate_workspace_down = "j"
navigate_pane_down = "ctrl+j"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        );

        assert_eq!(state.workspaces[0].focused_pane_id(), Some(below));
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn focus_pane_prefix_rhs_does_not_create_navigate_mode_pane_shortcut() {
        let mut state = state_with_workspaces(&["test"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let below = state.workspaces[0].test_split(Direction::Vertical);
        state.workspaces[0].layout.focus_pane(root);
        state.view.pane_infos = state.workspaces[0]
            .active_tab()
            .unwrap()
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 80, 24));
        let config: Config = toml::from_str(
            r#"
[keys]
focus_pane_down = "prefix+f"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::empty()),
        );
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(root));

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty()),
        );
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(below));
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn customized_navigate_pane_key_disables_matching_prefix_rhs_fallback() {
        let mut state = state_with_workspaces(&["test"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let below = state.workspaces[0].test_split(Direction::Vertical);
        state.workspaces[0].layout.focus_pane(root);
        state.view.pane_infos = state.workspaces[0]
            .active_tab()
            .unwrap()
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 80, 24));
        let config: Config = toml::from_str(
            r#"
[keys]
navigate_pane_down = "ctrl+j"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty()),
        );
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(root));

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(below));
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn left_and_right_arrows_remain_permanent_navigate_pane_aliases() {
        let mut state = state_with_workspaces(&["test"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let right = state.workspaces[0].test_split(Direction::Horizontal);
        state.workspaces[0].layout.focus_pane(right);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 24));
        let config: Config = toml::from_str(
            r#"
[keys]
navigate_pane_left = "ctrl+h"
navigate_pane_right = "ctrl+l"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Left, KeyModifiers::empty()),
        );
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(root));
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 24));

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Right, KeyModifiers::empty()),
        );
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(right));
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn mobile_workspace_keyboard_navigation_keeps_selected_row_visible() {
        let mut state = state_with_workspaces(&["a", "b", "c", "d"]);
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Navigate;
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 44, 8));
        assert_eq!(state.mobile_switcher_scroll, 0);

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );

        assert_eq!(state.selected, 1);
        assert_eq!(state.mobile_switcher_scroll, 1);
    }

    #[test]
    fn terminal_direct_agent_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.next_agent = crate::config::ActionKeybinds::direct("alt+a");

        let action = terminal_direct_navigation_action(
            &state,
            TerminalKey::new(KeyCode::Char('a'), KeyModifiers::ALT),
        );

        assert_eq!(action, Some(NavigateAction::NextAgent));
    }

    #[test]
    fn default_goto_key_routes_to_session_navigator() {
        let state = state_with_workspaces(&["test"]);

        let action = non_indexed_action_for_key(
            &state,
            TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()),
            BindingDispatch::Prefix,
        );

        assert_eq!(action, Some(NavigateAction::WorkspacePicker));
    }

    #[test]
    fn configured_goto_key_routes_to_session_navigator() {
        let mut state = state_with_workspaces(&["test"]);
        let config: Config = toml::from_str("[keys]\ngoto = \"ctrl+alt+g\"\n").unwrap();
        state.keybinds = config.keybinds();

        let action = non_indexed_action_for_key(
            &state,
            TerminalKey::new(
                KeyCode::Char('g'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            BindingDispatch::Direct,
        );

        assert_eq!(action, Some(NavigateAction::WorkspacePicker));
    }

    #[test]
    fn terminal_direct_focus_pane_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.focus_pane_left = crate::config::ActionKeybinds::direct("alt+left");

        let action = terminal_direct_navigation_action(
            &state,
            TerminalKey::new(KeyCode::Left, KeyModifiers::ALT),
        );

        assert_eq!(action, Some(NavigateAction::FocusPaneLeft));
    }

    #[test]
    fn navigate_group_shortcuts_map_to_navigation_actions() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.open_group_menu = crate::config::ActionKeybinds::prefix("ctrl+g");
        state.keybinds.new_group = crate::config::ActionKeybinds::prefix("alt+g");
        state.keybinds.rename_group = crate::config::ActionKeybinds::prefix("shift+g");
        state.keybinds.delete_group = crate::config::ActionKeybinds::prefix("ctrl+shift+g");
        state.keybinds.toggle_group_filter = crate::config::ActionKeybinds::prefix("f6");
        state.keybinds.previous_group = crate::config::ActionKeybinds::prefix("ctrl+[");
        state.keybinds.next_group = crate::config::ActionKeybinds::prefix("ctrl+]");

        let cases = [
            (
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
                NavigateAction::OpenGroupMenu,
            ),
            (
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::ALT),
                NavigateAction::NewGroup,
            ),
            (
                KeyEvent::new(KeyCode::Char('g'), KeyModifiers::SHIFT),
                NavigateAction::RenameGroup,
            ),
            (
                KeyEvent::new(
                    KeyCode::Char('g'),
                    KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                ),
                NavigateAction::DeleteGroup,
            ),
            (
                KeyEvent::new(KeyCode::F(6), KeyModifiers::empty()),
                NavigateAction::ToggleGroupFilter,
            ),
            (
                KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL),
                NavigateAction::PreviousGroup,
            ),
            (
                KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL),
                NavigateAction::NextGroup,
            ),
        ];

        for (key, expected) in cases {
            assert_eq!(
                action_for_key(&state, TerminalKey::from(key), BindingDispatch::Prefix),
                Some(expected)
            );
        }
    }

    #[test]
    fn navigate_agent_and_right_sidebar_shortcuts_map_to_navigation_actions() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.open_agent_menu = crate::config::ActionKeybinds::prefix("alt+a");
        state.keybinds.toggle_right_sidebar = crate::config::ActionKeybinds::prefix("alt+b");

        assert_eq!(
            action_for_key(
                &state,
                TerminalKey::new(KeyCode::Char('a'), KeyModifiers::ALT),
                BindingDispatch::Prefix,
            ),
            Some(NavigateAction::OpenAgentMenu)
        );
        assert_eq!(
            action_for_key(
                &state,
                TerminalKey::new(KeyCode::Char('b'), KeyModifiers::ALT),
                BindingDispatch::Prefix,
            ),
            Some(NavigateAction::ToggleRightSidebar)
        );
    }

    #[test]
    fn toggle_right_sidebar_shortcut_collapses_visible_right_sidebar() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Navigate;
        state.view.right_sidebar_rect = ratatui::layout::Rect::new(80, 0, 28, 24);
        state.keybinds.toggle_right_sidebar = crate::config::ActionKeybinds::prefix("alt+b");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT),
        );

        assert!(state.right_sidebar_collapsed);
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn terminal_direct_group_shortcuts_only_switch_groups() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.open_group_menu = crate::config::ActionKeybinds::prefix("ctrl+g");
        state.keybinds.previous_group = crate::config::ActionKeybinds::direct("ctrl+[");
        state.keybinds.next_group = crate::config::ActionKeybinds::direct("ctrl+]");

        assert_eq!(
            terminal_direct_navigation_action(
                &state,
                TerminalKey::new(KeyCode::Char('['), KeyModifiers::CONTROL),
            ),
            Some(NavigateAction::PreviousGroup)
        );
        assert_eq!(
            terminal_direct_navigation_action(
                &state,
                TerminalKey::new(KeyCode::Char(']'), KeyModifiers::CONTROL),
            ),
            Some(NavigateAction::NextGroup)
        );
        assert_eq!(
            terminal_direct_navigation_action(
                &state,
                TerminalKey::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
            ),
            None
        );
    }

    #[test]
    fn terminal_direct_indexed_tab_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        let config: Config = toml::from_str("[keys]\nswitch_tab = \"ctrl+3\"\n").unwrap();
        state.keybinds.switch_tab = config.keybinds().switch_tab;

        let action = terminal_direct_navigation_action(
            &state,
            TerminalKey::new(KeyCode::Char('3'), KeyModifiers::CONTROL),
        );

        assert_eq!(action, Some(NavigateAction::SwitchTab(2)));
    }

    #[test]
    fn terminal_direct_indexed_group_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        let config: Config = toml::from_str("[keys]\nswitch_group = \"ctrl+1..0\"\n").unwrap();
        state.keybinds.switch_group = config.keybinds().switch_group;

        let action = terminal_direct_navigation_action(
            &state,
            TerminalKey::new(KeyCode::Char('0'), KeyModifiers::CONTROL),
        );

        assert_eq!(action, Some(NavigateAction::SwitchGroup(9)));
    }

    #[test]
    fn indexed_workspace_shortcut_respects_active_group_filter() {
        let mut state = state_with_workspaces(&["a", "b", "c"]);
        state.groups.push(Group {
            id: "side".into(),
            name: "side".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        state.workspaces[1].group_id = "side".into();
        state.workspaces[2].group_id = "side".into();
        state.active_group = 1;
        state.group_filter_enabled = true;
        let config: Config = toml::from_str("[keys]\nswitch_workspace = \"ctrl+1..9\"\n").unwrap();
        state.keybinds.switch_workspace = config.keybinds().switch_workspace;

        let action = terminal_direct_navigation_action(
            &state,
            TerminalKey::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
        );

        assert_eq!(action, Some(NavigateAction::SwitchWorkspace(2)));
    }
    #[test]
    fn literal_symbol_binding_takes_precedence_over_shifted_indexed_alias() {
        let mut state = state_with_workspaces(&["one", "two"]);
        let config: Config = toml::from_str(
            r#"
[keys]
help = "prefix+!"
switch_workspace = "prefix+shift+1..9"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();

        let action = action_for_key(
            &state,
            TerminalKey::new(KeyCode::Char('!'), KeyModifiers::empty()),
            BindingDispatch::Prefix,
        );

        assert_eq!(action, Some(NavigateAction::Help));
    }

    #[test]
    fn literal_symbol_custom_command_is_checked_before_shifted_indexed_alias() {
        let mut state = state_with_workspaces(&["one", "two"]);
        let config: Config = toml::from_str(
            r#"
[keys]
switch_workspace = "prefix+shift+1..9"

[[keys.command]]
key = "prefix+!"
command = "echo literal"
"#,
        )
        .unwrap();
        state.keybinds = config.keybinds();

        let key = TerminalKey::new(KeyCode::Char('!'), KeyModifiers::empty());
        assert!(command_for_key(&state, key, BindingDispatch::Prefix).is_some());
        assert_eq!(
            indexed_navigation_action(&state, key, BindingDispatch::Prefix),
            Some(NavigateAction::SwitchWorkspace(0))
        );
    }

    #[tokio::test]
    async fn navigate_mode_runs_prefix_action_rhs_without_pressing_prefix_again() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Navigate;

        app.handle_navigate_key(TerminalKey::new(KeyCode::Char('n'), KeyModifiers::SHIFT));

        assert_eq!(app.state.workspaces.len(), 2);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[tokio::test]
    async fn prefix_focus_pane_is_one_shot_and_returns_to_terminal() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        let root = app.state.workspaces[0].tabs[0].root_pane;
        let right = app.state.workspaces[0].test_split(Direction::Horizontal);
        app.state.workspaces[0].layout.focus_pane(right);
        app.state.view.pane_infos = app.state.workspaces[0]
            .active_tab()
            .unwrap()
            .layout
            .panes(ratatui::layout::Rect::new(0, 0, 80, 24));

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::Char('h'), KeyModifiers::empty()))
            .await;

        assert_eq!(app.state.workspaces[0].focused_pane_id(), Some(root));
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[tokio::test]
    async fn no_op_prefix_action_exits_prefix_mode() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::Char('o'), KeyModifiers::empty()))
            .await;

        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[tokio::test]
    async fn unmatched_prefix_rhs_exits_prefix_mode() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::F(12), KeyModifiers::empty()))
            .await;

        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[tokio::test]
    async fn prefix_help_matches_enhanced_shifted_question_mark() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(
            TerminalKey::new(KeyCode::Char('/'), KeyModifiers::SHIFT)
                .with_shifted_codepoint('?' as u32),
        )
        .await;

        assert_eq!(app.state.mode, Mode::KeybindHelp);
    }

    #[test]
    fn navigate_mode_help_is_binding_driven() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.help = crate::config::ActionKeybinds::prefix("f");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
        );
        assert_eq!(state.mode, Mode::Navigate);

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::KeybindHelp);
    }

    #[test]
    fn modified_navigate_local_key_can_be_bound_as_prefix_rhs() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.toggle_sidebar = crate::config::ActionKeybinds::prefix("shift+h");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT),
        );

        assert!(state.sidebar_collapsed);
    }

    #[test]
    fn empty_state_new_tab_is_no_op() {
        let mut state = crate::app::state::AppState::test_new();
        let mut terminal_runtimes = TerminalRuntimeRegistry::new();
        state.mode = Mode::Prefix;

        execute_navigate_action_in_context(
            &mut state,
            &mut terminal_runtimes,
            NavigateAction::NewTab,
            ActionContext::Prefix,
        );

        assert_eq!(state.mode, Mode::Navigate);
        assert!(!state.creating_new_tab);
        assert!(!state.request_new_tab);
        assert!(state.workspaces.is_empty());
    }

    #[test]
    fn closing_linked_worktree_closes_workspace_without_removing_checkout() {
        let mut state = state_with_workspaces(&["main", "issue"]);
        state.selected = 1;
        state.active = Some(1);
        state.mode = Mode::Navigate;
        state.confirm_close = false;
        state.workspaces[1].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "omh".into(),
            repo_root: "/repo/omh".into(),
            checkout_path: "/repo/omh-issue".into(),
            is_linked_worktree: true,
        });

        execute_navigate_action(&mut state, NavigateAction::CloseWorkspace);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "main");
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn prefix_close_pane_last_parent_group_pane_opens_confirmation() {
        let mut state = state_with_workspaces(&["main", "issue"]);
        mark_worktree_space_member(&mut state, 0, "repo-key");
        mark_worktree_space_member(&mut state, 1, "repo-key");
        state.selected = 1;
        state.active = Some(0);
        state.mode = Mode::Navigate;

        execute_navigate_action(&mut state, NavigateAction::ClosePane);

        assert_eq!(state.selected, 0);
        assert_eq!(state.mode, Mode::ConfirmClose);
        assert_eq!(state.workspaces.len(), 2);
    }
    #[tokio::test]
    async fn custom_command_runs_from_prefix_key_in_navigate_mode() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let output_path = unique_temp_path("custom-command-keybind");
        let command = format!(
            "printf '%s\\n%s\\n%s\\n' \"$OMH_ACTIVE_WORKSPACE_ID\" \"$OMH_ACTIVE_TAB_ID\" \"$OMH_ACTIVE_PANE_ID\" > '{}'",
            output_path.display()
        );
        app.state.keybinds.goto = crate::config::ActionKeybinds::default();
        app.state.keybinds.custom_commands = vec![crate::config::CustomCommandKeybind {
            bindings: crate::config::ActionKeybinds::prefix("g"),
            label: "prefix+g".into(),
            command,
            action: crate::config::CustomCommandAction::Shell,
            description: None,
        }];

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        assert_eq!(app.state.mode, Mode::Prefix);

        app.handle_key(TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()))
            .await;

        let content = wait_for_file(&output_path);
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], app.state.workspaces[0].id);
        assert_eq!(lines[1], format!("{}:t1", app.state.workspaces[0].id));
        assert_eq!(lines[2], format!("{}:p1", app.state.workspaces[0].id));
        assert_eq!(app.state.mode, Mode::Terminal);

        let _ = std::fs::remove_file(output_path);
    }

    #[tokio::test]
    async fn pane_overlay_command_opens_and_closes_after_exit() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        let (workspace, terminal, runtime) = Workspace::new(
            std::env::current_dir().unwrap_or_else(|_| "/".into()),
            24,
            80,
            app.state.pane_scrollback_limit_bytes,
            app.state.host_terminal_theme,
            crate::pane::PaneShellConfig::new(&app.state.default_shell, app.state.shell_mode),
            app.event_tx.clone(),
            app.render_notify.clone(),
            app.render_dirty.clone(),
        )
        .expect("workspace should spawn");
        app.state.workspaces = vec![workspace];
        app.terminal_runtimes.insert(terminal.id.clone(), runtime);
        app.state.terminals.insert(terminal.id.clone(), terminal);
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let output_path = unique_temp_path("custom-pane-command");
        let command = format!("printf done > '{}'", output_path.display());
        app.state.keybinds.goto = crate::config::ActionKeybinds::default();
        app.state.keybinds.custom_commands = vec![crate::config::CustomCommandKeybind {
            bindings: crate::config::ActionKeybinds::prefix("g"),
            label: "prefix+g".into(),
            command,
            action: crate::config::CustomCommandAction::Pane,
            description: None,
        }];

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()))
            .await;

        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
        assert_eq!(app.terminal_runtimes.len(), 2);
        assert!(app.state.workspaces[0].tabs[0].zoomed);

        let _ = wait_for_file(&output_path);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if app.drain_internal_events()
                && app.state.workspaces[0].tabs[0].layout.pane_count() == 1
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 1);
        assert!(!app.state.workspaces[0].tabs[0].zoomed);
        assert_eq!(app.state.mode, Mode::Terminal);
        let _ = std::fs::remove_file(output_path);

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn edit_scrollback_key_opens_focused_runtime_scrollback_in_editor_pane() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        workspace.tabs[0].runtimes.insert(
            root_pane,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                20,
                5,
                4096,
                b"alpha\nbeta\n",
            ),
        );
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let output_path = unique_temp_path("edit-scrollback");
        let _editor_env = crate::config::TestEnvVar::set(
            "EDITOR",
            format!("sh -c 'cp \"$1\" {}' sh", output_path.display()),
        );
        app.state.keybinds.goto = crate::config::ActionKeybinds::default();
        app.state.keybinds.edit_scrollback = crate::config::ActionKeybinds::prefix("g");

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()))
            .await;

        let content = wait_for_file(&output_path);
        assert!(content.contains("alpha"));
        assert!(content.contains("beta"));
        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(
            app.state.terminals.values().any(|terminal| terminal
                .launch_argv
                .as_ref()
                .is_some_and(|argv| argv.first().is_some_and(|program| program == "/bin/sh"))),
            "scrollback editor should launch through argv overlay path"
        );

        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    fn zoom_action_exits_navigate_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.workspaces[0].test_split(Direction::Horizontal);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.zoom = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert!(state.workspaces[0].zoomed);
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn focus_pane_action_keeps_zoomed_when_changing_focus() {
        let mut state = state_with_workspaces(&["test"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let right = state.workspaces[0].test_split(Direction::Horizontal);
        state.workspaces[0].layout.focus_pane(root);
        state.workspaces[0].zoomed = true;
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 100, 20));

        execute_navigate_action(&mut state, NavigateAction::FocusPaneRight);

        assert!(state.workspaces[0].zoomed);
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(right));
    }

    #[test]
    fn question_mark_opens_keybind_help_from_navigate() {
        let mut state = state_with_workspaces(&["test"]);

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT),
        );

        assert_eq!(state.mode, Mode::KeybindHelp);
    }

    #[test]
    fn command_palette_key_opens_command_palette_from_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.goto = crate::config::ActionKeybinds::default();
        state.keybinds.command_palette = crate::config::ActionKeybinds::prefix("g");

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::CommandPalette);
        assert!(state.command_palette.query.is_empty());
    }

    #[test]
    fn new_tab_action_opens_dialog_without_creating_tab() {
        let mut state = state_with_workspaces(&["test"]);

        execute_navigate_action(&mut state, NavigateAction::NewTab);

        assert_eq!(state.mode, Mode::RenameTab);
        assert!(state.creating_new_tab);
        assert_eq!(state.name_input, "2");
        assert!(state.name_input_replace_on_type);
        assert!(!state.request_new_tab);
        assert_eq!(state.workspaces[0].tabs.len(), 1);
    }

    #[test]
    fn new_tab_action_can_skip_rename_dialog() {
        let mut state = state_with_workspaces(&["test"]);
        state.prompt_new_tab_name = false;

        execute_navigate_action(&mut state, NavigateAction::NewTab);

        assert_eq!(state.mode, Mode::Terminal);
        assert!(!state.creating_new_tab);
        assert!(state.request_new_tab);
        assert!(state.requested_new_tab_name.is_none());
    }

    #[test]
    fn navigate_q_detaches_in_persistence_mode() {
        let mut state = crate::app::state::AppState::test_new();
        state.detach_exits = false;

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()),
        );

        assert!(state.detach_requested);
        assert!(!state.should_quit);
    }
}
