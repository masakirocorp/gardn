use std::{
    fs, io,
    io::Write,
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};

use ratatui::layout::Direction;

use crate::{
    app::{state::AppState, App, ClientViewState},
    input::TerminalKey,
};

pub(crate) fn terminal_direct_non_indexed_navigation_action(
    state: &AppState,
    key: &TerminalKey,
) -> Option<NavigateAction> {
    non_indexed_action_for_key(state, key, BindingDispatch::Direct)
}

pub(crate) fn terminal_direct_indexed_navigation_action(
    state: &AppState,
    view: &ClientViewState,
    key: &TerminalKey,
) -> Option<NavigateAction> {
    indexed_navigation_action(state, view, key, BindingDispatch::Direct)
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

    pub(crate) fn launch_custom_command_for_view(
        &mut self,
        client_view: &mut super::super::ClientViewState,
        binding: crate::config::CustomCommandKeybind,
        context: ActionContext,
    ) {
        let target = self.custom_command_target_for_view(client_view);
        self.launch_custom_command_at(client_view, binding, context, target);
    }

    fn launch_custom_command_at(
        &mut self,
        client_view: &mut super::super::ClientViewState,
        binding: crate::config::CustomCommandKeybind,
        _context: ActionContext,
        target: Option<CustomCommandTarget>,
    ) {
        let previous_toast = self.state.toast.clone();
        let result = match binding.action {
            crate::config::CustomCommandAction::Shell => {
                self.spawn_custom_command(&binding, target).map(|_| None)
            }
            crate::config::CustomCommandAction::Pane => target
                .ok_or_else(|| std::io::Error::other("no active workspace"))
                .and_then(|target| {
                    self.spawn_pane_command(&binding.command, Vec::new(), target, client_view.id())
                        .map(Some)
                }),
            crate::config::CustomCommandAction::PluginAction => self
                .invoke_plugin_action_from_keybind_at(
                    binding.command.clone(),
                    target.map(|target| (target.ws_idx, target.pane_id)),
                    Some(client_view.selection.as_ref()),
                )
                .map(|_| None)
                .map_err(std::io::Error::other),
        };
        match result {
            Ok(Some((ws_idx, tab_idx, pane_id))) => {
                client_view.focus_client_overlay(&self.state, ws_idx, tab_idx, pane_id);
            }
            Ok(None) => {}
            Err(err) => {
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::NeedsAttention,
                    title: "Custom Command Failed".to_string(),
                    context: err.to_string(),
                    position: None,
                    target: None,
                });
                self.sync_toast_deadline(previous_toast);
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
                "GARDN_BIN_PATH".to_string(),
                current_exe.display().to_string(),
            ));
        }
        let mut cwd = None;
        if let Some(target) = target {
            env.push((
                "GARDN_ACTIVE_WORKSPACE_ID".to_string(),
                self.public_workspace_id(target.ws_idx),
            ));
            if let Some(tab_id) = self.public_tab_id(target.ws_idx, target.tab_idx) {
                env.push(("GARDN_ACTIVE_TAB_ID".to_string(), tab_id));
            }
            if let Some(pane_id) = self.public_pane_id(target.ws_idx, target.pane_id) {
                env.push(("GARDN_ACTIVE_PANE_ID".to_string(), pane_id));
            }
            if let Some(pane_cwd) = self
                .state
                .workspaces
                .get(target.ws_idx)
                .and_then(|workspace| workspace.terminal_tab(target.tab_idx).ok())
                .and_then(|tab| {
                    tab.cwd_for_pane(
                        target.pane_id,
                        &self.state.terminals,
                        &self.terminal_runtimes,
                    )
                })
            {
                env.push((
                    "GARDN_ACTIVE_PANE_CWD".to_string(),
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

    pub(crate) fn launch_focused_scrollback_editor_at(
        &mut self,
        client_view: &mut super::super::ClientViewState,
    ) {
        let previous_toast = self.state.toast.clone();
        if let Err(err) = self.open_focused_scrollback_in_editor(client_view) {
            self.state.toast = Some(crate::app::state::ToastNotification {
                kind: crate::app::state::ToastKind::NeedsAttention,
                title: "Edit Scrollback Failed".to_string(),
                context: err.to_string(),
                position: None,
                target: None,
            });
        }
        self.sync_toast_deadline(previous_toast);
    }

    fn open_focused_scrollback_in_editor(
        &mut self,
        client_view: &mut super::super::ClientViewState,
    ) -> std::io::Result<()> {
        let target = self
            .custom_command_target_for_view(client_view)
            .ok_or_else(|| std::io::Error::other("no focused pane"))?;
        let scrollback = self
            .state
            .runtime_for_pane_in_workspace(&self.terminal_runtimes, target.ws_idx, target.pane_id)
            .ok_or_else(|| std::io::Error::other("focused pane has no scrollback runtime"))?
            .recent_unwrapped_text_snapshot(usize::MAX)
            .text;
        let path = write_scrollback_temp_file(&scrollback)?;
        let argv = match crate::platform::scrollback_editor_argv(&path) {
            Ok(argv) => argv,
            Err(err) => {
                let _ = fs::remove_file(&path);
                return Err(err);
            }
        };
        let (env, cwd) = self.custom_command_env(Some(target));
        let (tab_idx, new_pane) = match self.spawn_overlay_argv_command(
            &argv,
            cwd,
            env,
            vec![path.clone()],
            target,
            client_view.id(),
        ) {
            Ok(result) => result,
            Err(err) => {
                let _ = fs::remove_file(&path);
                return Err(err);
            }
        };
        let new_pane_id = new_pane.pane_id;
        let terminal_id = new_pane.terminal.id.clone();
        self.terminal_runtimes
            .insert(terminal_id.clone(), new_pane.runtime);
        self.state.remove_alias_shadowed_by_new_pane(new_pane_id);
        self.state.terminals.insert(terminal_id, new_pane.terminal);
        client_view.focus_client_overlay(&self.state, target.ws_idx, tab_idx, new_pane_id);

        if let Some(public_pane_id) = self.public_pane_id(target.ws_idx, target.pane_id) {
            self.state.toast = Some(crate::app::state::ToastNotification {
                kind: crate::app::state::ToastKind::Finished,
                title: "Opened Scrollback".to_string(),
                context: format!("Focused pane {public_pane_id}"),
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
        client_owner: u64,
    ) -> std::io::Result<(usize, usize, crate::layout::PaneId)> {
        let (rows, cols) = self.state.estimate_pane_size();
        let new_rows = rows.max(4);
        let new_cols = cols.max(10);
        let (env, cwd) = self.custom_command_env(Some(target));
        let (tab_idx, new_pane) = {
            let workspace = self
                .state
                .workspaces
                .get_mut(target.ws_idx)
                .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
            match workspace.split_pane_custom_command(
                target.pane_id,
                Direction::Horizontal,
                new_rows,
                new_cols,
                cwd,
                command,
                env,
                self.state.pane_scrollback_limit_bytes,
                self.state.host_terminal_theme,
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
        self.overlay_panes
            .insert(new_pane_id, super::super::OverlayPaneState { temp_files });
        self.state
            .client_overlay_owners
            .insert(new_pane_id, client_owner);
        self.state.remove_alias_shadowed_by_new_pane(new_pane_id);
        Ok((target.ws_idx, tab_idx, new_pane_id))
    }

    fn spawn_overlay_argv_command(
        &mut self,
        argv: &[String],
        cwd: Option<std::path::PathBuf>,
        extra_env: Vec<(String, String)>,
        temp_files: Vec<std::path::PathBuf>,
        target: CustomCommandTarget,
        client_owner: u64,
    ) -> std::io::Result<(usize, crate::workspace::NewPane)> {
        let (rows, cols) = self.state.estimate_pane_size();
        let new_rows = rows.max(4);
        let new_cols = cols.max(10);
        let cwd = cwd.or_else(|| {
            self.state
                .workspaces
                .get(target.ws_idx)
                .and_then(|workspace| workspace.terminal_tab(target.tab_idx).ok())
                .and_then(|tab| {
                    tab.cwd_for_pane(
                        target.pane_id,
                        &self.state.terminals,
                        &self.terminal_runtimes,
                    )
                })
        });
        let (tab_idx, new_pane) = {
            let workspace = self
                .state
                .workspaces
                .get_mut(target.ws_idx)
                .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
            match workspace.split_pane_argv_command(
                target.pane_id,
                Direction::Horizontal,
                new_rows,
                new_cols,
                cwd,
                argv,
                extra_env,
                self.state.pane_scrollback_limit_bytes,
                self.state.host_terminal_theme,
            ) {
                Some(Ok(result)) => result,
                Some(Err(err)) => return Err(err),
                None => return Err(std::io::Error::other("focused pane disappeared")),
            }
        };
        self.overlay_panes.insert(
            new_pane.pane_id,
            super::super::OverlayPaneState { temp_files },
        );
        self.state
            .client_overlay_owners
            .insert(new_pane.pane_id, client_owner);
        Ok((tab_idx, new_pane))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingDispatch {
    Direct,
    Prefix,
}

pub(crate) fn command_for_key(
    state: &AppState,
    key: &TerminalKey,
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
    OpenContextMenu,
    NewTab,
    TakeTabControl,
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
    ZenMode,
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
    view: &ClientViewState,
    key: &TerminalKey,
    dispatch: BindingDispatch,
) -> Option<NavigateAction> {
    let kb = &state.keybinds;
    let actual_modifiers = crate::config::normalize_key_combo((key.code, key.modifiers)).1;

    for exact_modifiers in [true, false] {
        let trigger_matches = |binding: &crate::config::IndexedKeybind| {
            let dispatch_matches = match dispatch {
                BindingDispatch::Direct => binding.trigger.is_direct(),
                BindingDispatch::Prefix => binding.trigger.is_prefix(),
            };
            let expected_modifiers = crate::config::normalize_key_combo(binding.trigger.combo()).1;
            dispatch_matches && (actual_modifiers == expected_modifiers) == exact_modifiers
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
                    if let Some(ws_idx) = view
                        .sidebar_visible_workspace_indices(state)
                        .get(idx)
                        .copied()
                    {
                        return Some(NavigateAction::SwitchWorkspace(ws_idx));
                    }
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
    }

    None
}

fn action_matches(
    bindings: &crate::config::ActionKeybinds,
    key: &TerminalKey,
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
    key: &TerminalKey,
    dispatch: BindingDispatch,
) -> Option<NavigateAction> {
    non_indexed_action_for_key(state, key, dispatch).or_else(|| {
        let view = ClientViewState::from_default_client_state(state);
        indexed_navigation_action(state, &view, key, dispatch)
    })
}

pub(crate) fn non_indexed_action_for_key(
    state: &AppState,
    key: &TerminalKey,
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
        (&kb.open_context_menu, NavigateAction::OpenContextMenu),
        (&kb.new_tab, NavigateAction::NewTab),
        (&kb.take_control, NavigateAction::TakeTabControl),
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
        (&kb.zen_mode, NavigateAction::ZenMode),
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
        "gardn-scrollback-{}-{nanos}-{attempt}.txt",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, ModifierKeyCode};

    use super::super::{state_with_workspaces, unique_temp_path, wait_for_file};
    use super::*;
    use crate::{
        app::{state::Group, App, Mode},
        config::Config,
        input::TerminalKey,
        raw_input::{parse_raw_input_bytes_sync, RawInputEvent},
        workspace::Workspace,
    };
    fn app_with_test_workspaces(names: &[&str]) -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = names.iter().copied().map(Workspace::test_new).collect();
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app
    }

    #[test]
    fn default_take_control_key_maps_without_diagnostics_or_conflict() {
        let state = state_with_workspaces(&["test"]);

        assert!(crate::config::Config::default()
            .collect_diagnostics()
            .is_empty());
        assert_eq!(
            non_indexed_action_for_key(
                &state,
                &TerminalKey::new(KeyCode::Char('t'), KeyModifiers::empty()),
                BindingDispatch::Prefix,
            ),
            Some(NavigateAction::TakeTabControl)
        );
    }

    #[test]
    fn terminal_direct_agent_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.next_agent = crate::config::ActionKeybinds::direct("alt+a");

        let action = action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Char('a'), KeyModifiers::ALT),
            BindingDispatch::Direct,
        );

        assert_eq!(action, Some(NavigateAction::NextAgent));
    }

    #[test]
    fn default_goto_key_routes_to_session_navigator() {
        let state = state_with_workspaces(&["test"]);

        let action = non_indexed_action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()),
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
            &TerminalKey::new(
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

        let action = action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Left, KeyModifiers::ALT),
            BindingDispatch::Direct,
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
                action_for_key(&state, &TerminalKey::from(key), BindingDispatch::Prefix),
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
                &TerminalKey::new(KeyCode::Char('a'), KeyModifiers::ALT),
                BindingDispatch::Prefix,
            ),
            Some(NavigateAction::OpenAgentMenu)
        );
        assert_eq!(
            action_for_key(
                &state,
                &TerminalKey::new(KeyCode::Char('b'), KeyModifiers::ALT),
                BindingDispatch::Prefix,
            ),
            Some(NavigateAction::ToggleRightSidebar)
        );
    }

    #[test]
    fn default_shift_f10_maps_to_open_context_menu() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds = Config::default().keybinds();

        let action = non_indexed_action_for_key(
            &state,
            &TerminalKey::new(KeyCode::F(10), KeyModifiers::SHIFT),
            BindingDispatch::Direct,
        );

        assert_eq!(action, Some(NavigateAction::OpenContextMenu));
    }

    #[test]
    fn terminal_direct_group_shortcuts_only_switch_groups() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.open_group_menu = crate::config::ActionKeybinds::prefix("ctrl+g");
        state.keybinds.previous_group = crate::config::ActionKeybinds::direct("ctrl+[");
        state.keybinds.next_group = crate::config::ActionKeybinds::direct("ctrl+]");

        assert_eq!(
            action_for_key(
                &state,
                &TerminalKey::new(KeyCode::Char('['), KeyModifiers::CONTROL),
                BindingDispatch::Direct,
            ),
            Some(NavigateAction::PreviousGroup)
        );
        assert_eq!(
            action_for_key(
                &state,
                &TerminalKey::new(KeyCode::Char(']'), KeyModifiers::CONTROL),
                BindingDispatch::Direct,
            ),
            Some(NavigateAction::NextGroup)
        );
        assert_eq!(
            action_for_key(
                &state,
                &TerminalKey::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
                BindingDispatch::Direct,
            ),
            None
        );
    }

    #[test]
    fn terminal_direct_indexed_tab_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        let config: Config = toml::from_str("[keys]\nswitch_tab = \"ctrl+3\"\n").unwrap();
        state.keybinds.switch_tab = config.keybinds().switch_tab;

        let action = action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Char('3'), KeyModifiers::CONTROL),
            BindingDispatch::Direct,
        );

        assert_eq!(action, Some(NavigateAction::SwitchTab(2)));
    }

    #[test]
    fn terminal_direct_indexed_group_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        let config: Config = toml::from_str("[keys]\nswitch_group = \"ctrl+1..0\"\n").unwrap();
        state.keybinds.switch_group = config.keybinds().switch_group;

        let action = action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Char('0'), KeyModifiers::CONTROL),
            BindingDispatch::Direct,
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
            default_location: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
            github_organization: None,
        });
        state.workspaces[1].group_id = "side".into();
        state.workspaces[2].group_id = "side".into();
        let mut view = ClientViewState::from_default_client_state(&state);
        view.active_group = 1;
        view.group_filter_enabled = true;
        let config: Config = toml::from_str("[keys]\nswitch_workspace = \"ctrl+1..9\"\n").unwrap();
        state.keybinds.switch_workspace = config.keybinds().switch_workspace;

        let action = indexed_navigation_action(
            &state,
            &view,
            &TerminalKey::new(KeyCode::Char('2'), KeyModifiers::CONTROL),
            BindingDispatch::Direct,
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
            &TerminalKey::new(KeyCode::Char('!'), KeyModifiers::empty()),
            BindingDispatch::Prefix,
        );

        assert_eq!(action, Some(NavigateAction::Help));
    }
    #[test]
    fn prefix_shift_indexed_workspace_shortcut_maps_legacy_us_symbol_key() {
        let mut state = state_with_workspaces(&["one", "two"]);
        let config: Config =
            toml::from_str("[keys]\nswitch_workspace = \"prefix+shift+1..9\"\n").unwrap();
        state.keybinds.switch_workspace = config.keybinds().switch_workspace;

        let action = action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Char('@'), KeyModifiers::empty()),
            BindingDispatch::Prefix,
        );

        assert_eq!(action, Some(NavigateAction::SwitchWorkspace(1)));
    }

    #[test]
    fn prefix_shift_indexed_workspace_shortcut_maps_non_us_number_rows() {
        let mut state = state_with_workspaces(&["one", "two"]);
        let config: Config =
            toml::from_str("[keys]\nswitch_workspace = \"prefix+shift+1..9\"\n").unwrap();
        state.keybinds.switch_workspace = config.keybinds().switch_workspace;

        for key in [
            TerminalKey::new(KeyCode::Char('2'), KeyModifiers::SHIFT)
                .with_shifted_codepoint('"' as u32),
            TerminalKey::new(KeyCode::Char('é'), KeyModifiers::SHIFT)
                .with_shifted_codepoint('2' as u32),
        ] {
            assert_eq!(
                action_for_key(&state, &key, BindingDispatch::Prefix),
                Some(NavigateAction::SwitchWorkspace(1))
            );
        }
    }

    #[test]
    fn prefix_unshifted_indexed_shortcut_maps_shifted_french_number_row() {
        let mut state = state_with_workspaces(&["one"]);
        let config: Config = toml::from_str("[keys]\nswitch_tab = \"prefix+1..9\"\n").unwrap();
        state.keybinds.switch_tab = config.keybinds().switch_tab;

        let action = action_for_key(
            &state,
            &TerminalKey::new(KeyCode::Char('é'), KeyModifiers::SHIFT)
                .with_shifted_codepoint('2' as u32),
            BindingDispatch::Prefix,
        );

        assert_eq!(action, Some(NavigateAction::SwitchTab(1)));
    }

    #[tokio::test]
    async fn prefix_shift_indexed_workspace_shortcut_survives_modifier_press() {
        let mut app = app_with_test_workspaces(&["one", "two"]);
        let config: Config =
            toml::from_str("[keys]\nswitch_workspace = \"prefix+shift+1..9\"\n").unwrap();
        app.state.keybinds.switch_workspace = config.keybinds().switch_workspace;
        app.default_client_view.mode = Mode::Prefix;

        app.handle_key(TerminalKey::new(
            KeyCode::Modifier(ModifierKeyCode::LeftShift),
            KeyModifiers::SHIFT,
        ))
        .await;

        assert_eq!(app.default_client_view.mode, Mode::Prefix);

        let action = action_for_key(
            &app.state,
            &TerminalKey::new(KeyCode::Char('2'), KeyModifiers::SHIFT)
                .with_shifted_codepoint('"' as u32),
            BindingDispatch::Prefix,
        );
        assert_eq!(action, Some(NavigateAction::SwitchWorkspace(1)));
    }

    #[test]
    fn shifted_backslash_layout_prefers_horizontal_split_binding() {
        let config: Config = toml::from_str(
            r#"
[keys]
split_vertical = "prefix+|"
split_horizontal = 'prefix+\'
"#,
        )
        .unwrap();
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds = config.keybinds();
        let key = crate::input::parse_terminal_key_sequence("\x1b[124:92;2:1u").unwrap();
        assert_eq!(key.code, KeyCode::Char('|'));
        assert_eq!(key.modifiers, KeyModifiers::SHIFT);
        assert_eq!(key.shifted_codepoint, Some('\\' as u32));
        assert!(state
            .keybinds
            .split_horizontal
            .matches_prefix_key(&key.clone()));
        assert!(!state
            .keybinds
            .split_vertical
            .matches_prefix_key(&key.clone()));

        assert_eq!(
            action_for_key(&state, &key, BindingDispatch::Prefix),
            Some(NavigateAction::SplitHorizontal)
        );
        assert_eq!(
            action_for_key(
                &state,
                &TerminalKey::new(KeyCode::Char('|'), KeyModifiers::empty()),
                BindingDispatch::Prefix,
            ),
            Some(NavigateAction::SplitVertical)
        );
    }

    #[tokio::test]
    async fn kitty_shifted_alternate_without_modifier_prefers_reload_over_resize() {
        let mut app = app_with_test_workspaces(&["test"]);
        app.default_client_view.mode = Mode::Prefix;

        let mut events = parse_raw_input_bytes_sync(b"\x1b[114:82;1u");
        assert_eq!(events.len(), 1);
        let RawInputEvent::Key(key) = events.remove(0) else {
            panic!("expected key event");
        };
        assert_eq!(
            action_for_key(&app.state, &key.clone(), BindingDispatch::Prefix),
            Some(NavigateAction::ReloadConfig)
        );
        app.handle_key(key).await;

        assert_eq!(app.default_client_view.mode, Mode::Terminal);
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
        let view = ClientViewState::from_default_client_state(&state);

        let key = TerminalKey::new(KeyCode::Char('!'), KeyModifiers::empty());
        assert!(command_for_key(&state, &key, BindingDispatch::Prefix).is_some());
        assert_eq!(
            indexed_navigation_action(&state, &view, &key, BindingDispatch::Prefix),
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
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.mode = Mode::Navigate;

        app.handle_key(TerminalKey::new(KeyCode::Char('n'), KeyModifiers::SHIFT))
            .await;

        assert_eq!(app.state.workspaces.len(), 2);
        assert_eq!(app.default_client_view.mode, Mode::Terminal);
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
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.mode = Mode::Terminal;

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::Char('o'), KeyModifiers::empty()))
            .await;

        assert_eq!(app.default_client_view.mode, Mode::Terminal);
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
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.mode = Mode::Terminal;

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::F(12), KeyModifiers::empty()))
            .await;

        assert_eq!(app.default_client_view.mode, Mode::Terminal);
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
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.mode = Mode::Terminal;

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

        assert_eq!(app.default_client_view.mode, Mode::KeybindHelp);
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
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.mode = Mode::Terminal;

        let output_path = unique_temp_path("custom-command-keybind");
        let command = format!(
            "printf '%s\\n%s\\n%s\\n' \"$GARDN_ACTIVE_WORKSPACE_ID\" \"$GARDN_ACTIVE_TAB_ID\" \"$GARDN_ACTIVE_PANE_ID\" > '{}'",
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
        assert_eq!(app.default_client_view.mode, Mode::Prefix);

        app.handle_key(TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()))
            .await;

        let content = wait_for_file(&output_path);
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], app.state.workspaces[0].id);
        assert_eq!(lines[1], format!("{}:t1", app.state.workspaces[0].id));
        assert_eq!(lines[2], format!("{}:p1", app.state.workspaces[0].id));
        assert_eq!(app.default_client_view.mode, Mode::Terminal);

        let _ = std::fs::remove_file(output_path);
    }

    #[tokio::test]
    async fn edit_scrollback_key_preserves_logical_lines_in_editor_pane() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.terminal_tab(0).unwrap().root_pane;
        workspace.terminal_tab_mut(0).unwrap().runtimes.insert(
            root_pane,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                5,
                5,
                4096,
                b"ABCDEFGHIJ\r\nKLMNO",
            ),
        );
        app.state.workspaces = vec![workspace];
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.reconcile(&app.state);
        app.default_client_view.mode = Mode::Terminal;

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
        assert_eq!(content, "ABCDEFGHIJ\nKLMNO");
        assert_eq!(app.default_client_view.mode, Mode::Terminal);
        assert!(
            app.state.terminals.values().any(|terminal| terminal
                .launch_argv
                .as_ref()
                .is_some_and(|argv| argv.first().is_some_and(|program| program == "/bin/sh"))),
            "scrollback editor should launch through argv overlay path"
        );

        let _ = std::fs::remove_file(output_path);
    }
}
