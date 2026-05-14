use std::process::{Command, Stdio};

use bytes::Bytes;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Direction;

use crate::{
    app::{
        state::{key_matches, AppState, Mode},
        App,
    },
    input::TerminalKey,
    layout::NavDirection,
};

pub(crate) fn terminal_direct_navigation_action(
    state: &AppState,
    key: &KeyEvent,
) -> Option<NavigateAction> {
    let kb = &state.keybinds;
    if kb
        .previous_workspace
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousWorkspace);
    }
    if kb
        .next_workspace
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextWorkspace);
    }
    if kb
        .previous_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousGroup);
    }
    if kb
        .next_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextGroup);
    }
    if kb
        .previous_agent
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousAgent);
    }
    if kb
        .next_agent
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextAgent);
    }
    if kb
        .previous_tab
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousTab);
    }
    if kb
        .next_tab
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextTab);
    }
    if kb
        .focus_pane_left
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::FocusPaneLeft);
    }
    if kb
        .focus_pane_down
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::FocusPaneDown);
    }
    if kb
        .focus_pane_up
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::FocusPaneUp);
    }
    if kb
        .focus_pane_right
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::FocusPaneRight);
    }
    None
}

impl App {
    pub(crate) fn handle_navigate_key(&mut self, raw_key: TerminalKey) {
        let key = raw_key.as_key_event();
        self.state.update_dismissed = true;

        if self.state.is_prefix(&key) {
            if !self.pass_through_key_to_focused_pane(raw_key) {
                leave_navigate_mode(&mut self.state);
            }
            return;
        }

        if key.code == KeyCode::Esc {
            leave_navigate_mode(&mut self.state);
            return;
        }

        if let Some(action) = navigate_action_for_key(&self.state, &key) {
            execute_navigate_action(&mut self.state, action);
            return;
        }

        if handle_navigate_reserved_key(&mut self.state, key) {
            return;
        }

        if let Some(binding) = navigate_custom_command_for_key(&self.state, &key) {
            self.launch_custom_command(binding);
        }
    }

    fn pass_through_key_to_focused_pane(&mut self, key: TerminalKey) -> bool {
        let Some(ws) = self.state.active.and_then(|i| self.state.workspaces.get(i)) else {
            return false;
        };
        let Some(rt) = ws.focused_runtime() else {
            return false;
        };

        let bytes = rt.encode_terminal_key(key);
        if bytes.is_empty() || rt.try_send_bytes(Bytes::from(bytes)).is_err() {
            return false;
        }

        self.state.mode = Mode::Terminal;
        true
    }

    pub(super) fn launch_custom_command(&mut self, binding: crate::config::CustomCommandKeybind) {
        let previous_toast = self.state.toast.clone();
        let result = match binding.action {
            crate::config::CustomCommandAction::Shell => self.spawn_custom_command(&binding),
            crate::config::CustomCommandAction::Pane => self.spawn_pane_command(&binding.command),
        };
        match result {
            Ok(()) => leave_navigate_mode(&mut self.state),
            Err(err) => {
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::NeedsAttention,
                    title: "custom command failed".to_string(),
                    context: err.to_string(),
                    target: None,
                });
                self.sync_toast_deadline(previous_toast);
            }
        }
    }

    fn custom_command_env(&self) -> (Vec<(String, String)>, Option<std::path::PathBuf>) {
        let mut env = vec![(
            crate::api::SOCKET_PATH_ENV_VAR.to_string(),
            crate::api::socket_path().display().to_string(),
        )];
        if let Ok(current_exe) = std::env::current_exe() {
            env.push((
                "HERDR_BIN_PATH".to_string(),
                current_exe.display().to_string(),
            ));
        }

        let mut cwd = None;
        if let Some(ws_idx) = self.state.active {
            env.push((
                "HERDR_ACTIVE_WORKSPACE_ID".to_string(),
                self.public_workspace_id(ws_idx),
            ));
            if let Some(workspace) = self.state.workspaces.get(ws_idx) {
                let tab_idx = workspace.active_tab_index();
                if let Some(tab_id) = self.public_tab_id(ws_idx, tab_idx) {
                    env.push(("HERDR_ACTIVE_TAB_ID".to_string(), tab_id));
                }
                if let Some(pane_id) = workspace.focused_pane_id() {
                    if let Some(public_pane_id) = self.public_pane_id(ws_idx, pane_id) {
                        env.push(("HERDR_ACTIVE_PANE_ID".to_string(), public_pane_id));
                    }
                    if let Some(pane_cwd) = workspace
                        .active_tab()
                        .and_then(|tab| tab.cwd_for_pane(pane_id))
                    {
                        env.push((
                            "HERDR_ACTIVE_PANE_CWD".to_string(),
                            pane_cwd.display().to_string(),
                        ));
                        if pane_cwd.is_dir() {
                            cwd = Some(pane_cwd);
                        }
                    }
                }
            }
        }
        (env, cwd)
    }

    fn spawn_custom_command(
        &self,
        binding: &crate::config::CustomCommandKeybind,
    ) -> std::io::Result<()> {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-lc")
            .arg(&binding.command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let (env, cwd) = self.custom_command_env();
        command.envs(env);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command.spawn()?;
        Ok(())
    }

    fn spawn_pane_command(&mut self, command: &str) -> std::io::Result<()> {
        let Some(ws_idx) = self.state.active else {
            return Err(std::io::Error::other("no active workspace"));
        };
        let (rows, cols) = self.state.estimate_pane_size();
        let new_rows = rows.max(4);
        let new_cols = cols.max(10);
        let (env, _) = self.custom_command_env();

        let ws = self
            .state
            .workspaces
            .get_mut(ws_idx)
            .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
        let tab_idx = ws.active_tab_index();
        let previous_focus = ws
            .focused_pane_id()
            .ok_or_else(|| std::io::Error::other("no focused pane"))?;
        let previous_zoomed = ws.active_tab().map(|tab| tab.zoomed).unwrap_or(false);
        let cwd = ws
            .active_tab()
            .and_then(|tab| tab.cwd_for_pane(previous_focus));
        let new_pane_id = ws.split_focused_command(
            Direction::Horizontal,
            new_rows,
            new_cols,
            cwd,
            command,
            &env,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
        )?;
        ws.active_tab_mut()
            .expect("workspace must have an active tab")
            .layout
            .focus_pane(new_pane_id);
        ws.active_tab_mut()
            .expect("workspace must have an active tab")
            .zoomed = true;
        self.overlay_panes.insert(
            new_pane_id,
            super::super::OverlayPaneState {
                ws_idx,
                tab_idx,
                previous_focus,
                previous_zoomed,
            },
        );
        self.state.mode = Mode::Terminal;
        Ok(())
    }
}

fn navigate_custom_command_for_key(
    state: &AppState,
    key: &KeyEvent,
) -> Option<crate::config::CustomCommandKeybind> {
    state
        .keybinds
        .custom_commands
        .iter()
        .find(|binding| key_matches(key, binding.key.0, binding.key.1))
        .cloned()
}

pub(super) fn handle_navigate_reserved_key(state: &mut AppState, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => {
            super::modal::request_quit_or_detach(state);
            leave_navigate_mode(state);
            true
        }
        KeyCode::Enter => {
            if state.workspace_in_active_group(state.selected) {
                state.switch_workspace(state.selected);
                leave_navigate_mode(state);
            }
            true
        }
        KeyCode::Char(c @ '1'..='9') => {
            let idx = (c as usize) - ('1' as usize);
            if let Some(ws_idx) = state.visible_workspace_indices().get(idx).copied() {
                state.switch_workspace(ws_idx);
                leave_navigate_mode(state);
            }
            true
        }
        KeyCode::Char('s') => {
            super::settings::open_settings(state);
            true
        }
        KeyCode::Char('?') => {
            super::modal::open_keybind_help(state);
            true
        }
        KeyCode::Up => {
            let visible = state.visible_workspace_indices();
            if let Some(pos) = visible.iter().position(|idx| *idx == state.selected) {
                if let Some(prev) = pos.checked_sub(1).and_then(|idx| visible.get(idx)) {
                    state.selected = *prev;
                    state.ensure_workspace_visible(state.selected);
                }
            }
            true
        }
        KeyCode::Down => {
            let visible = state.visible_workspace_indices();
            if let Some(pos) = visible.iter().position(|idx| *idx == state.selected) {
                if let Some(next) = visible.get(pos + 1) {
                    state.selected = *next;
                    state.ensure_workspace_visible(state.selected);
                }
            }
            true
        }
        KeyCode::Char('h') | KeyCode::Left => {
            state.navigate_pane(NavDirection::Left);
            true
        }
        KeyCode::Char('j') => {
            state.navigate_pane(NavDirection::Down);
            true
        }
        KeyCode::Char('k') => {
            state.navigate_pane(NavDirection::Up);
            true
        }
        KeyCode::Char('l') | KeyCode::Right => {
            state.navigate_pane(NavDirection::Right);
            true
        }
        KeyCode::Tab => {
            state.cycle_pane(false);
            true
        }
        KeyCode::BackTab => {
            state.cycle_pane(true);
            true
        }
        _ => false,
    }
}

#[allow(dead_code)] // exercised in input unit tests; production uses App::handle_navigate_key
pub(crate) fn handle_navigate_key(state: &mut AppState, key: KeyEvent) {
    state.update_dismissed = true;

    if state.is_prefix(&key) || key.code == KeyCode::Esc {
        leave_navigate_mode(state);
        return;
    }

    if let Some(action) = navigate_action_for_key(state, &key) {
        execute_navigate_action(state, action);
        return;
    }

    let _ = handle_navigate_reserved_key(state, key);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigateAction {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    PreviousWorkspace,
    NextWorkspace,
    OpenGroupMenu,
    NewGroup,
    RenameGroup,
    DeleteGroup,
    ToggleGroupFilter,
    PreviousGroup,
    NextGroup,
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
    Fullscreen,
    EnterResizeMode,
    ToggleSidebar,
    ToggleRightSidebar,
    OpenCommandPalette,
    ReloadConfig,
    OpenNotificationTarget,
    Detach,
}

fn navigate_action_for_key(state: &AppState, key: &KeyEvent) -> Option<NavigateAction> {
    let kb = &state.keybinds;
    if key_matches(key, kb.new_workspace.0, kb.new_workspace.1) {
        return Some(NavigateAction::NewWorkspace);
    }
    if key_matches(key, kb.rename_workspace.0, kb.rename_workspace.1) {
        return Some(NavigateAction::RenameWorkspace);
    }
    if key_matches(key, kb.close_workspace.0, kb.close_workspace.1) {
        return Some(NavigateAction::CloseWorkspace);
    }
    if kb
        .previous_workspace
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousWorkspace);
    }
    if kb
        .next_workspace
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextWorkspace);
    }
    if kb
        .open_group_menu
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::OpenGroupMenu);
    }
    if kb
        .new_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NewGroup);
    }
    if kb
        .rename_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::RenameGroup);
    }
    if kb
        .delete_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::DeleteGroup);
    }
    if kb
        .toggle_group_filter
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::ToggleGroupFilter);
    }
    if kb
        .previous_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousGroup);
    }
    if kb
        .next_group
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextGroup);
    }
    if kb
        .previous_agent
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousAgent);
    }
    if kb
        .next_agent
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextAgent);
    }
    if kb
        .open_agent_menu
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::OpenAgentMenu);
    }
    if key_matches(key, kb.new_tab.0, kb.new_tab.1) {
        return Some(NavigateAction::NewTab);
    }
    if kb
        .rename_tab
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::RenameTab);
    }
    if kb
        .previous_tab
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::PreviousTab);
    }
    if kb
        .next_tab
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::NextTab);
    }
    if kb
        .close_tab
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::CloseTab);
    }
    if kb
        .rename_pane
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::RenamePane);
    }
    if key_matches(key, kb.split_vertical.0, kb.split_vertical.1) {
        return Some(NavigateAction::SplitVertical);
    }
    if key_matches(key, kb.split_horizontal.0, kb.split_horizontal.1) {
        return Some(NavigateAction::SplitHorizontal);
    }
    if key_matches(key, kb.close_pane.0, kb.close_pane.1) {
        return Some(NavigateAction::ClosePane);
    }
    if key_matches(key, kb.fullscreen.0, kb.fullscreen.1) {
        return Some(NavigateAction::Fullscreen);
    }
    if key_matches(key, kb.resize_mode.0, kb.resize_mode.1) {
        return Some(NavigateAction::EnterResizeMode);
    }
    if key_matches(key, kb.toggle_sidebar.0, kb.toggle_sidebar.1) {
        return Some(NavigateAction::ToggleSidebar);
    }
    if kb
        .toggle_right_sidebar
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::ToggleRightSidebar);
    }
    if key_matches(key, kb.command_palette.0, kb.command_palette.1) {
        return Some(NavigateAction::OpenCommandPalette);
    }
    if kb
        .reload_config
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::ReloadConfig);
    }
    if kb
        .open_notification_target
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::OpenNotificationTarget);
    }
    if kb
        .detach
        .is_some_and(|(code, mods)| key_matches(key, code, mods))
    {
        return Some(NavigateAction::Detach);
    }
    None
}

pub(super) fn execute_navigate_action(state: &mut AppState, action: NavigateAction) {
    match action {
        NavigateAction::NewWorkspace => {
            state.request_new_workspace = true;
            leave_navigate_mode(state);
        }
        NavigateAction::RenameWorkspace => {
            if state.workspace_in_active_group(state.selected) {
                super::modal::open_rename_workspace(state, state.selected);
            }
        }
        NavigateAction::CloseWorkspace => {
            if state.workspace_in_active_group(state.selected) {
                if state.confirm_close {
                    super::modal::open_confirm_close(state);
                } else {
                    state.close_selected_workspace();
                    leave_navigate_mode(state);
                }
            }
        }
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
        NavigateAction::NewTab => super::modal::open_new_tab_dialog(state),
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
            state.close_tab();
            if state.mode != Mode::ConfirmClose {
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
            state.split_pane(Direction::Horizontal);
            leave_navigate_mode(state);
        }
        NavigateAction::SplitHorizontal => {
            state.split_pane(Direction::Vertical);
            leave_navigate_mode(state);
        }
        NavigateAction::ClosePane => {
            state.close_pane();
            leave_navigate_mode(state);
        }
        NavigateAction::Fullscreen => {
            state.toggle_fullscreen();
            leave_navigate_mode(state);
        }
        NavigateAction::EnterResizeMode => state.mode = Mode::Resize,
        NavigateAction::ToggleSidebar => {
            state.sidebar_collapsed = !state.sidebar_collapsed;
            state.mark_session_dirty();
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
            state.detach_requested = true;
            leave_navigate_mode(state);
        }
    }
}

fn leave_navigate_mode(state: &mut AppState) {
    if state.active.is_some() {
        state.mode = Mode::Terminal;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::layout::Direction;

    use super::super::{state_with_workspaces, unique_temp_path, wait_for_file};
    use super::*;
    use crate::{app::App, config::Config, input::TerminalKey, workspace::Workspace};

    #[test]
    fn custom_rename_key_enters_rename_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.rename_workspace = (KeyCode::Char('g'), KeyModifiers::empty());
        state.keybinds.rename_workspace_label = "g".into();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::RenameWorkspace);
        assert_eq!(state.name_input, "test");
    }

    #[test]
    fn custom_new_workspace_key_requests_and_exits_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.new_workspace = (KeyCode::Char('g'), KeyModifiers::empty());
        state.keybinds.new_workspace_label = "g".into();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert!(state.request_new_workspace);
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn close_tab_action_prompts_when_last_tab_would_close_workspace() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Navigate;
        state.active = Some(0);
        state.selected = 0;
        state.confirm_close = true;

        execute_navigate_action(&mut state, NavigateAction::CloseTab);

        assert_eq!(state.mode, Mode::ConfirmClose);
        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn custom_sidebar_toggle_key_toggles_and_exits_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.toggle_sidebar = (KeyCode::Char('g'), KeyModifiers::empty());
        state.keybinds.toggle_sidebar_label = "g".into();
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
        state.keybinds.resize_mode = (KeyCode::Char('g'), KeyModifiers::empty());
        state.keybinds.resize_mode_label = "g".into();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Resize);
    }

    #[test]
    fn custom_reload_config_key_requests_reload_and_exits_navigate() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.reload_config = Some((KeyCode::Char('g'), KeyModifiers::empty()));
        state.keybinds.reload_config_label = Some("g".into());

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
        state.keybinds.open_notification_target = Some((KeyCode::Char('g'), KeyModifiers::empty()));
        state.keybinds.open_notification_target_label = Some("g".into());
        let target_workspace_id = state.workspaces[1].id.clone();
        let target_pane = state.workspaces[1].tabs[0].root_pane;
        state.toast = Some(crate::app::state::ToastNotification {
            kind: crate::app::state::ToastKind::NeedsAttention,
            title: "pi needs attention".into(),
            context: "two".into(),
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
        state.keybinds.next_agent = Some((KeyCode::Char('a'), KeyModifiers::ALT));
        state.keybinds.next_agent_label = Some("alt+a".into());

        let action = terminal_direct_navigation_action(
            &state,
            &KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT),
        );

        assert_eq!(action, Some(NavigateAction::NextAgent));
    }

    #[test]
    fn terminal_direct_focus_pane_shortcut_maps_to_navigation_action() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.focus_pane_left = Some((KeyCode::Left, KeyModifiers::ALT));
        state.keybinds.focus_pane_left_label = Some("alt+left".into());

        let action = terminal_direct_navigation_action(
            &state,
            &KeyEvent::new(KeyCode::Left, KeyModifiers::ALT),
        );

        assert_eq!(action, Some(NavigateAction::FocusPaneLeft));
    }

    #[test]
    fn navigate_group_shortcuts_map_to_navigation_actions() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.open_group_menu = Some((KeyCode::Char('g'), KeyModifiers::CONTROL));
        state.keybinds.new_group = Some((KeyCode::Char('g'), KeyModifiers::ALT));
        state.keybinds.rename_group = Some((KeyCode::Char('g'), KeyModifiers::SHIFT));
        state.keybinds.delete_group = Some((
            KeyCode::Char('g'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        state.keybinds.toggle_group_filter = Some((KeyCode::F(6), KeyModifiers::empty()));
        state.keybinds.previous_group = Some((KeyCode::Char('['), KeyModifiers::CONTROL));
        state.keybinds.next_group = Some((KeyCode::Char(']'), KeyModifiers::CONTROL));

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
            assert_eq!(navigate_action_for_key(&state, &key), Some(expected));
        }
    }

    #[test]
    fn navigate_agent_and_right_sidebar_shortcuts_map_to_navigation_actions() {
        let mut state = state_with_workspaces(&["test"]);
        state.keybinds.open_agent_menu = Some((KeyCode::Char('a'), KeyModifiers::ALT));
        state.keybinds.toggle_right_sidebar = Some((KeyCode::Char('b'), KeyModifiers::ALT));

        assert_eq!(
            navigate_action_for_key(
                &state,
                &KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT),
            ),
            Some(NavigateAction::OpenAgentMenu)
        );
        assert_eq!(
            navigate_action_for_key(
                &state,
                &KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT),
            ),
            Some(NavigateAction::ToggleRightSidebar)
        );
    }

    #[test]
    fn toggle_right_sidebar_shortcut_collapses_visible_right_sidebar() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Navigate;
        state.view.right_sidebar_rect = ratatui::layout::Rect::new(80, 0, 28, 24);
        state.keybinds.toggle_right_sidebar = Some((KeyCode::Char('b'), KeyModifiers::ALT));

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
        state.keybinds.open_group_menu = Some((KeyCode::Char('g'), KeyModifiers::CONTROL));
        state.keybinds.previous_group = Some((KeyCode::Char('['), KeyModifiers::CONTROL));
        state.keybinds.next_group = Some((KeyCode::Char(']'), KeyModifiers::CONTROL));

        assert_eq!(
            terminal_direct_navigation_action(
                &state,
                &KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL),
            ),
            Some(NavigateAction::PreviousGroup)
        );
        assert_eq!(
            terminal_direct_navigation_action(
                &state,
                &KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL),
            ),
            Some(NavigateAction::NextGroup)
        );
        assert_eq!(
            terminal_direct_navigation_action(
                &state,
                &KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL),
            ),
            None
        );
    }

    #[tokio::test]
    async fn custom_command_runs_from_prefix_key_in_navigate_mode() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
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
            "printf '%s\\n%s\\n%s\\n' \"$HERDR_ACTIVE_WORKSPACE_ID\" \"$HERDR_ACTIVE_TAB_ID\" \"$HERDR_ACTIVE_PANE_ID\" > '{}'",
            output_path.display()
        );
        app.state.keybinds.custom_commands = vec![crate::config::CustomCommandKeybind {
            key: (KeyCode::Char('g'), KeyModifiers::empty()),
            label: "g".into(),
            command,
            action: crate::config::CustomCommandAction::Shell,
        }];

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        assert_eq!(app.state.mode, Mode::Navigate);

        app.handle_key(TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()))
            .await;

        let content = wait_for_file(&output_path);
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], app.state.workspaces[0].id);
        assert_eq!(lines[1], format!("{}:1", app.state.workspaces[0].id));
        assert_eq!(lines[2], format!("{}-1", app.state.workspaces[0].id));
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
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        let workspace = Workspace::new(
            std::env::current_dir().unwrap_or_else(|_| "/".into()),
            24,
            80,
            app.state.pane_scrollback_limit_bytes,
            app.state.host_terminal_theme,
            app.event_tx.clone(),
            app.render_notify.clone(),
            app.render_dirty.clone(),
        )
        .expect("workspace should spawn");
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let output_path = unique_temp_path("custom-pane-command");
        let command = format!("printf done > '{}'", output_path.display());
        app.state.keybinds.custom_commands = vec![crate::config::CustomCommandKeybind {
            key: (KeyCode::Char('g'), KeyModifiers::empty()),
            label: "g".into(),
            command,
            action: crate::config::CustomCommandAction::Pane,
        }];

        app.handle_key(TerminalKey::new(
            app.state.prefix_code,
            app.state.prefix_mods,
        ))
        .await;
        app.handle_key(TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()))
            .await;

        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
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
    }

    #[test]
    fn fullscreen_action_exits_navigate_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.workspaces[0].test_split(Direction::Horizontal);
        state.keybinds.fullscreen = (KeyCode::Char('g'), KeyModifiers::empty());
        state.keybinds.fullscreen_label = "g".into();

        handle_navigate_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert!(state.workspaces[0].zoomed);
        assert_eq!(state.mode, Mode::Terminal);
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
        let key = state.keybinds.command_palette;

        handle_navigate_key(&mut state, KeyEvent::new(key.0, key.1));

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
    fn persistence_mode_navigate_q_detaches_instead_of_quitting_server() {
        let mut state = crate::app::state::AppState::test_new();
        state.quit_detaches = true;

        assert!(handle_navigate_reserved_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty())
        ));
        assert!(state.detach_requested);
        assert!(!state.should_quit);
    }
}
