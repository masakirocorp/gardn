use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Direction, Rect};

use crate::{
    app::state::{
        AppState, ContextMenuKind, ContextMenuState, ModalListState, Mode, NavigatorStateFilter,
        DEFAULT_GROUP_ICON,
    },
    input::TerminalKey,
    layout::NavDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalAction {
    Continue,
    Save,
    Clear,
    Cancel,
    Confirm,
    Apply,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModalKeyBinding {
    Enter,
    Esc,
    CtrlC,
}

impl ModalKeyBinding {
    fn matches(self, key: &KeyEvent) -> bool {
        match self {
            Self::Enter => key.code == KeyCode::Enter,
            Self::Esc => key.code == KeyCode::Esc,
            Self::CtrlC => {
                key.code == KeyCode::Char('c')
                    && key.modifiers == crossterm::event::KeyModifiers::CONTROL
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ModalActionSpec<A> {
    pub action: A,
    pub bindings: &'static [ModalKeyBinding],
}

pub(super) fn modal_action_from_key<A: Copy>(
    key: &KeyEvent,
    specs: &[ModalActionSpec<A>],
) -> Option<A> {
    specs
        .iter()
        .find(|spec| spec.bindings.iter().any(|binding| binding.matches(key)))
        .map(|spec| spec.action)
}

pub(crate) fn modal_action_from_buttons<A: Copy>(
    col: u16,
    row: u16,
    buttons: &[(Rect, A)],
) -> Option<A> {
    buttons.iter().find_map(|(rect, action)| {
        (col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height)
            .then_some(*action)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalMenuAction {
    ConfigIssue,
    Detach,
    UpdateIntegrations,
    Changelog,
    Keybinds,
    ReloadConfig,
    Settings,
}

pub(crate) fn global_menu_actions(state: &AppState) -> Vec<GlobalMenuAction> {
    let mut actions = Vec::new();
    if state.config_issue.is_some() {
        actions.push(GlobalMenuAction::ConfigIssue);
    }
    actions.push(GlobalMenuAction::Changelog);
    if state.integration_updates_available() {
        actions.push(GlobalMenuAction::UpdateIntegrations);
    }
    actions.extend([
        GlobalMenuAction::Settings,
        GlobalMenuAction::Keybinds,
        GlobalMenuAction::ReloadConfig,
        GlobalMenuAction::Detach,
    ]);
    actions
}

pub(super) fn open_config_diagnostics(state: &mut AppState) {
    state.config_diagnostics_scroll = 0;
    state.mode = Mode::ConfigDiagnostics;
}

pub(super) fn open_global_menu(state: &mut AppState) {
    state.global_menu = ModalListState::hidden(0);
    state.mode = Mode::GlobalMenu;
}

pub(super) fn open_group_menu(state: &mut AppState) {
    let highlighted = if state.group_filter_enabled {
        state.active_group + 2
    } else {
        1
    };
    state.group_menu = ModalListState::hidden(highlighted);
    state.mode = Mode::GroupMenu;
}

pub(super) fn open_agent_menu(state: &mut AppState) {
    let highlighted = match state.agent_panel_scope {
        crate::app::state::AgentPanelScope::AllWorkspaces => 1,
        crate::app::state::AgentPanelScope::CurrentWorkspace => 2,
        crate::app::state::AgentPanelScope::CurrentGroup => 3,
    };
    state.agent_menu = ModalListState::hidden(highlighted);
    state.mode = Mode::AgentMenu;
}

pub(super) fn open_keybind_help(state: &mut AppState) {
    state.keybind_help.scroll = 0;
    state.mode = Mode::KeybindHelp;
}

fn open_changelog(state: &mut AppState) {
    let notes = crate::release_notes::load_changelog();

    state.release_notes = Some(crate::app::state::ReleaseNotesState {
        version: notes.version,
        body: notes.body,
        scroll: 0,
        preview: notes.preview,
    });
    state.mode = Mode::ReleaseNotes;
}

pub(crate) fn request_detach(state: &mut AppState) {
    if state.detach_exits {
        state.should_quit = true;
    } else {
        state.detach_requested = true;
    }
}

pub(super) fn apply_global_menu_action(state: &mut AppState, action: GlobalMenuAction) {
    match action {
        GlobalMenuAction::ConfigIssue => open_config_diagnostics(state),
        GlobalMenuAction::Detach => {
            leave_modal(state);
            request_detach(state);
        }
        GlobalMenuAction::Changelog => open_changelog(state),
        GlobalMenuAction::UpdateIntegrations => super::settings::open_settings_at(
            state,
            crate::app::state::SettingsSection::Integrations,
        ),
        GlobalMenuAction::Keybinds => open_keybind_help(state),
        GlobalMenuAction::ReloadConfig => {
            state.request_reload_config = true;
            leave_modal(state);
        }
        GlobalMenuAction::Settings => super::settings::open_settings(state),
    }
}

pub(crate) fn handle_global_menu_key(state: &mut AppState, key: KeyEvent) {
    let actions = global_menu_actions(state);
    match key.code {
        KeyCode::Esc => leave_modal(state),
        KeyCode::Up | KeyCode::Char('k') => state.global_menu.move_prev(),
        KeyCode::Down | KeyCode::Char('j') => state.global_menu.move_next(actions.len()),
        KeyCode::Enter => {
            if let Some(action) = actions.get(state.global_menu.selected).copied() {
                apply_global_menu_action(state, action);
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_navigator_key(state: &mut AppState, key: KeyEvent) {
    if state.navigator.search_focused {
        match key.code {
            KeyCode::Esc => {
                if state.navigator.query.is_empty() {
                    state.navigator.search_focused = false;
                    leave_modal(state);
                } else {
                    state.navigator.query.clear();
                    state.navigator.state_filter = None;
                    state.navigator.search_focused = false;
                    state.clamp_navigator_selection();
                }
            }
            KeyCode::Enter => {
                state.accept_navigator_selection();
            }
            KeyCode::Backspace => {
                state.navigator.state_filter = None;
                state.navigator.query.pop();
                state.clamp_navigator_selection();
            }
            KeyCode::Up => state.move_navigator_selection(-1),
            KeyCode::Down => state.move_navigator_selection(1),
            KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
                state.move_navigator_selection(1)
            }
            KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                state.move_navigator_selection(-1)
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                state.navigator.query.clear();
                state.navigator.state_filter = None;
                state.clamp_navigator_selection();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                insert_navigator_search_text(state, &c.to_string());
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Esc => {
            if state.navigator.query.is_empty() && state.navigator.state_filter.is_none() {
                leave_modal(state);
            } else {
                state.navigator.query.clear();
                state.navigator.state_filter = None;
                state.clamp_navigator_selection();
            }
        }
        KeyCode::Enter => {
            state.accept_navigator_selection();
        }
        KeyCode::Char('/') => {
            state.navigator.query.clear();
            state.navigator.state_filter = None;
            state.navigator.search_focused = true;
            state.clamp_navigator_selection();
        }
        KeyCode::Backspace if state.navigator.state_filter.is_some() => {
            state.navigator.state_filter = None;
            state.clamp_navigator_selection();
        }
        KeyCode::Char('a') if key.modifiers.is_empty() => {
            state.navigator.query.clear();
            state.navigator.state_filter = None;
            state.clamp_navigator_selection();
        }
        KeyCode::Char('b') if key.modifiers.is_empty() => {
            state.navigator.query.clear();
            state.navigator.state_filter = Some(NavigatorStateFilter::Blocked);
            state.clamp_navigator_selection();
        }
        KeyCode::Char('w') if key.modifiers.is_empty() => {
            state.navigator.query.clear();
            state.navigator.state_filter = Some(NavigatorStateFilter::Working);
            state.clamp_navigator_selection();
        }
        KeyCode::Char('i') if key.modifiers.is_empty() => {
            state.navigator.query.clear();
            state.navigator.state_filter = Some(NavigatorStateFilter::Idle);
            state.clamp_navigator_selection();
        }
        KeyCode::Char('d') if key.modifiers.is_empty() => {
            state.navigator.query.clear();
            state.navigator.state_filter = Some(NavigatorStateFilter::Done);
            state.clamp_navigator_selection();
        }
        KeyCode::Char('e') if key.modifiers.is_empty() => {
            state.expand_all_navigator_branches();
        }
        KeyCode::Char('c') if key.modifiers.is_empty() => {
            state.collapse_all_navigator_branches();
        }
        KeyCode::Char('j') | KeyCode::Down => state.move_navigator_selection(1),
        KeyCode::Char('k') | KeyCode::Up => state.move_navigator_selection(-1),
        KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
            state.move_navigator_selection((state.navigator_body_rect().height / 2).max(1) as isize)
        }
        KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => state
            .move_navigator_selection(-((state.navigator_body_rect().height / 2).max(1) as isize)),
        KeyCode::Char(' ') => state.toggle_selected_navigator_branch(),
        KeyCode::Home => {
            state.navigator.list.select(0);
            state.ensure_navigator_selection_visible();
        }
        KeyCode::End | KeyCode::Char('G') => {
            state
                .navigator
                .list
                .select(state.navigator_rows().len().saturating_sub(1));
            state.ensure_navigator_selection_visible();
        }
        _ => {}
    }
}

pub(crate) fn handle_group_menu_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => leave_modal(state),
        KeyCode::Up | KeyCode::Char('k') => {
            let len = state.group_menu_labels().len();
            for _ in 0..len {
                state.group_menu.move_prev();
                if state
                    .group_menu_action_for_row(state.group_menu.selected)
                    .is_some()
                {
                    break;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let len = state.group_menu_labels().len();
            for _ in 0..len {
                state.group_menu.move_next(len);
                if state
                    .group_menu_action_for_row(state.group_menu.selected)
                    .is_some()
                {
                    break;
                }
            }
        }
        KeyCode::Enter => {
            let Some(action) = state.group_menu_action_for_row(state.group_menu.selected) else {
                return;
            };
            match action {
                super::sidebar::GroupMenuAction::AllSpaces => {
                    state.show_all_groups();
                    leave_modal(state);
                }
                super::sidebar::GroupMenuAction::Group(idx) => {
                    state.switch_group(idx);
                    leave_modal(state);
                }
                super::sidebar::GroupMenuAction::NewWorkspace => {
                    state.request_new_workspace = true;
                    leave_modal(state);
                }
                super::sidebar::GroupMenuAction::NewGroup => open_new_group_dialog(state),
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_agent_menu_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => leave_modal(state),
        KeyCode::Up | KeyCode::Char('k') => {
            let current = state.agent_menu.selected;
            let mut idx = current;
            while idx > 0 {
                idx -= 1;
                if state.agent_menu_action_for_row(idx).is_some() {
                    state.agent_menu.select(idx);
                    return;
                }
            }
            state.agent_menu.select(current);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let current = state.agent_menu.selected;
            let labels = state.agent_menu_labels();
            let mut idx = current;
            while idx + 1 < labels.len() {
                idx += 1;
                if state.agent_menu_action_for_row(idx).is_some() {
                    state.agent_menu.select(idx);
                    return;
                }
            }
            state.agent_menu.select(current);
        }
        KeyCode::Enter => {
            let Some(action) = state.agent_menu_action_for_row(state.agent_menu.selected) else {
                return;
            };
            apply_agent_menu_action(state, action);
            leave_modal(state);
        }
        _ => {}
    }
}

pub(super) fn apply_agent_menu_action(
    state: &mut AppState,
    action: super::sidebar::AgentMenuAction,
) {
    state.agent_panel_scope = match action {
        super::sidebar::AgentMenuAction::ThisSpace => {
            crate::app::state::AgentPanelScope::CurrentWorkspace
        }
        super::sidebar::AgentMenuAction::ThisGroup => {
            crate::app::state::AgentPanelScope::CurrentGroup
        }
        super::sidebar::AgentMenuAction::AllAgents => {
            crate::app::state::AgentPanelScope::AllWorkspaces
        }
    };
    state.agent_panel_scroll = 0;
    state.mark_session_dirty();
}
pub(crate) fn handle_config_diagnostics_key(state: &mut AppState, key: KeyEvent) {
    let max_scroll = crate::ui::config_diagnostics_max_scroll(
        state.screen_rect(),
        state.config_issue.as_ref(),
        &state.palette,
    );
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.config_diagnostics_scroll = state.config_diagnostics_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.config_diagnostics_scroll = state
                .config_diagnostics_scroll
                .saturating_add(1)
                .min(max_scroll);
        }
        KeyCode::PageUp => {
            state.config_diagnostics_scroll = state
                .config_diagnostics_scroll
                .saturating_sub(super::MODAL_PAGE_SCROLL_ROWS as u16);
        }
        KeyCode::PageDown => {
            state.config_diagnostics_scroll = state
                .config_diagnostics_scroll
                .saturating_add(super::MODAL_PAGE_SCROLL_ROWS as u16)
                .min(max_scroll);
        }
        KeyCode::Home => state.config_diagnostics_scroll = 0,
        KeyCode::End => state.config_diagnostics_scroll = max_scroll,
        KeyCode::Char('r') => {
            state.request_reload_config = true;
            leave_modal(state);
        }
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => leave_modal(state),
        _ => {}
    }
}

pub(crate) fn insert_navigator_search_text(state: &mut AppState, text: &str) {
    if !state.navigator.search_focused {
        return;
    }
    state.navigator.state_filter = None;
    state.navigator.query.push_str(text);
    state.clamp_navigator_selection();
}

pub(crate) fn handle_keybind_help_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => state.scroll_keybind_help(-1),
        KeyCode::Down | KeyCode::Char('j') => state.scroll_keybind_help(1),
        KeyCode::PageUp => state.scroll_keybind_help(-super::MODAL_PAGE_SCROLL_ROWS),
        KeyCode::PageDown => state.scroll_keybind_help(super::MODAL_PAGE_SCROLL_ROWS),
        KeyCode::Home => state.keybind_help.scroll = 0,
        KeyCode::End => state.keybind_help.scroll = state.keybind_help_max_scroll(),
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => leave_modal(state),
        _ => {}
    }
}

pub(super) fn open_rename_workspace(
    state: &mut AppState,
    terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ws_idx: usize,
) {
    state.selected = ws_idx;
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_group_target = None;
    state.rename_pane_target = None;
    state.name_input =
        state.workspaces[ws_idx].display_name_from(&state.terminals, terminal_runtimes);
    state.name_input_replace_on_type = false;
    state.mode = Mode::RenameWorkspace;
}

pub(super) fn open_rename_group(state: &mut AppState) {
    open_rename_group_at(state, state.active_group);
}

pub(super) fn open_rename_group_at(state: &mut AppState, group_idx: usize) {
    let Some(group) = state.groups.get(group_idx) else {
        return;
    };
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_input = group.icon.clone();
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_group_target = None;
    state.rename_pane_target = None;
    state.rename_group_target = Some(group_idx);
    state.name_input = group.name.clone();
    state.name_input_replace_on_type = false;
    state.mode = Mode::RenameGroup;
}

pub(super) fn open_rename_active_tab(state: &mut AppState, replace_on_type: bool) {
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_group_target = None;
    state.rename_pane_target = None;
    if let Some(ws) = state.active.and_then(|i| state.workspaces.get(i)) {
        if let Some(name) = ws.active_tab_display_name() {
            state.name_input = name;
            state.name_input_replace_on_type = replace_on_type;
            state.mode = Mode::RenameTab;
        }
    }
}

pub(super) fn open_rename_pane(state: &mut AppState, pane_id: crate::layout::PaneId) {
    let Some(ws) = state.active.and_then(|i| state.workspaces.get(i)) else {
        return;
    };
    let Some(pane) = ws.pane_state(pane_id) else {
        return;
    };
    let terminal = state.terminals.get(&pane.attached_terminal_id);
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_pane_target = Some(pane_id);
    state.name_input = terminal
        .and_then(|t| t.manual_label.clone())
        .unwrap_or_default();
    state.name_input_replace_on_type = terminal.and_then(|t| t.manual_label.as_ref()).is_none();
    state.mode = Mode::RenamePane;
}

fn next_new_tab_default_name(state: &AppState) -> String {
    state
        .active
        .and_then(|i| state.workspaces.get(i))
        .map(|ws| ws.next_public_tab_number.to_string())
        .unwrap_or_else(|| "1".to_string())
}

fn next_new_group_default_name(state: &AppState) -> String {
    format!("group {}", state.groups.len() + 1)
}

pub(super) fn open_new_tab_dialog(state: &mut AppState) {
    state.creating_new_tab = true;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_group_target = None;
    state.rename_pane_target = None;
    state.name_input = next_new_tab_default_name(state);
    state.name_input_replace_on_type = true;
    state.mode = Mode::RenameTab;
}

pub(super) fn request_new_tab_from_ui(state: &mut AppState) {
    if state.prompt_new_tab_name {
        open_new_tab_dialog(state);
    } else {
        state.request_new_tab = true;
        state.requested_new_tab_name = None;
        state.creating_new_tab = false;
        state.mode = Mode::Terminal;
    }
}

pub(super) fn open_new_group_dialog(state: &mut AppState) {
    state.creating_new_group = true;
    state.creating_new_tab = false;
    state.group_icon_input = DEFAULT_GROUP_ICON.to_string();
    state.group_default_directory_input.clear();
    state.group_modal_selected_field = 0;
    state.group_icon_picker_open = false;
    state.rename_group_target = None;
    state.requested_new_tab_name = None;
    state.rename_pane_target = None;
    state.name_input = next_new_group_default_name(state);
    state.name_input_replace_on_type = true;
    state.mode = Mode::RenameGroup;
}
pub(super) fn open_worktree_directory_editor(state: &mut AppState) {
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.group_default_directory_input.clear();
    state.group_modal_selected_field = 0;
    state.requested_new_tab_name = None;
    state.rename_group_target = None;
    state.rename_pane_target = None;
    state.name_input = state
        .settings
        .pending_worktree_directory
        .clone()
        .unwrap_or_else(|| state.worktree_directory.display().to_string());
    state.name_input_replace_on_type = false;
    state.mode = Mode::EditWorktreeDirectory;
}

pub(super) fn leave_modal(state: &mut AppState) {
    state.return_to_active_workspace_mode();
}

pub(super) const ONBOARDING_WELCOME_ACTIONS: &[ModalActionSpec<ModalAction>] = &[ModalActionSpec {
    action: ModalAction::Continue,
    bindings: &[ModalKeyBinding::Enter],
}];

pub(super) const RELEASE_NOTES_ACTIONS: &[ModalActionSpec<ModalAction>] = &[ModalActionSpec {
    action: ModalAction::Close,
    bindings: &[ModalKeyBinding::Enter, ModalKeyBinding::Esc],
}];

pub(super) const RENAME_ACTIONS: &[ModalActionSpec<ModalAction>] = &[
    ModalActionSpec {
        action: ModalAction::Save,
        bindings: &[ModalKeyBinding::Enter],
    },
    ModalActionSpec {
        action: ModalAction::Clear,
        bindings: &[ModalKeyBinding::CtrlC],
    },
    ModalActionSpec {
        action: ModalAction::Cancel,
        bindings: &[ModalKeyBinding::Esc],
    },
];

pub(super) const CONFIRM_CLOSE_ACTIONS: &[ModalActionSpec<ModalAction>] = &[
    ModalActionSpec {
        action: ModalAction::Confirm,
        bindings: &[ModalKeyBinding::Enter],
    },
    ModalActionSpec {
        action: ModalAction::Cancel,
        bindings: &[ModalKeyBinding::Esc],
    },
];

pub(super) const SETTINGS_ACTIONS: &[ModalActionSpec<ModalAction>] = &[
    ModalActionSpec {
        action: ModalAction::Apply,
        bindings: &[ModalKeyBinding::Enter],
    },
    ModalActionSpec {
        action: ModalAction::Close,
        bindings: &[ModalKeyBinding::Esc],
    },
];

pub(super) fn apply_rename_action(state: &mut AppState, action: ModalAction) {
    match action {
        ModalAction::Save => {
            let new_name = if state.name_input.trim().is_empty() {
                state.name_input.clone()
            } else {
                state.name_input.trim().to_string()
            };
            match state.mode {
                Mode::RenameWorkspace if !state.workspaces.is_empty() && !new_name.is_empty() => {
                    let workspace_id = state.workspaces[state.selected].id.clone();
                    state.workspaces[state.selected].set_custom_name(new_name);
                    crate::logging::workspace_renamed(&workspace_id);
                    state.mark_session_dirty();
                }
                Mode::RenameGroup if state.creating_new_group => {
                    let preserve_group_filter = state.group_filter_enabled;
                    let default_name = next_new_group_default_name(state);
                    let name = if new_name.is_empty() {
                        default_name
                    } else {
                        new_name
                    };
                    let default_directory = state.group_default_directory_input.trim().to_string();
                    let group_idx = state.create_group_with_icon_and_default_directory(
                        name,
                        state.group_icon_input.clone(),
                        (!default_directory.is_empty())
                            .then_some(std::path::PathBuf::from(default_directory)),
                    );
                    state.switch_group(group_idx);
                    state.group_filter_enabled = preserve_group_filter;
                    state.request_new_workspace = true;
                }
                Mode::RenameGroup if !new_name.is_empty() => {
                    let group_idx = state.rename_group_target.unwrap_or(state.active_group);
                    state.rename_group(group_idx, new_name);
                    state.set_group_icon(group_idx, state.group_icon_input.clone());
                }
                Mode::RenameTab if state.creating_new_tab => {
                    state.request_new_tab = true;
                    let default_name = next_new_tab_default_name(state);
                    state.requested_new_tab_name =
                        if new_name.is_empty() || new_name == default_name {
                            None
                        } else {
                            Some(new_name)
                        };
                }
                Mode::RenameTab => {
                    if let Some(ws_idx) = state.active {
                        if let Some(ws) = state.workspaces.get_mut(ws_idx) {
                            let workspace_id = ws.id.clone();
                            let active_tab = ws.active_tab;
                            if let Some(tab) = ws.active_tab_mut() {
                                let keep_auto_name =
                                    tab.is_auto_named() && new_name == tab.number.to_string();
                                if !new_name.is_empty() && !keep_auto_name {
                                    tab.set_custom_name(new_name);
                                    let tab_id = format!("{}:{}", workspace_id, active_tab + 1);
                                    crate::logging::tab_renamed(&workspace_id, &tab_id);
                                    state.mark_session_dirty();
                                }
                            }
                        }
                    }
                }
                Mode::RenamePane => {
                    if let (Some(ws_idx), Some(pane_id)) = (state.active, state.rename_pane_target)
                    {
                        if let Some(ws) = state.workspaces.get(ws_idx) {
                            if let Some(pane) = ws.pane_state(pane_id) {
                                let terminal_id = pane.attached_terminal_id.clone();
                                if let Some(terminal) = state.terminals.get_mut(&terminal_id) {
                                    terminal.set_manual_label(new_name);
                                    state.mark_session_dirty();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            state.creating_new_tab = false;
            state.creating_new_group = false;
            state.group_icon_picker_open = false;
            state.group_default_directory_input.clear();
            state.group_modal_selected_field = 0;
            state.rename_group_target = None;
            state.rename_pane_target = None;
            state.name_input.clear();
            state.name_input_replace_on_type = false;
            leave_modal(state);
        }
        ModalAction::Clear => {
            if state.mode == Mode::RenameGroup
                && state.creating_new_group
                && state.group_modal_selected_field == 1
            {
                state.group_default_directory_input.clear();
            } else {
                state.name_input.clear();
            }
            state.name_input_replace_on_type = false;
        }
        ModalAction::Cancel => {
            state.creating_new_tab = false;
            state.creating_new_group = false;
            state.group_icon_picker_open = false;
            state.group_default_directory_input.clear();
            state.group_modal_selected_field = 0;
            state.rename_group_target = None;
            state.requested_new_tab_name = None;
            state.rename_pane_target = None;
            state.name_input.clear();
            state.name_input_replace_on_type = false;
            leave_modal(state);
        }
        _ => {}
    }
}

impl crate::app::App {
    pub(crate) fn handle_rename_key_via_runtime(&mut self, key: KeyEvent) {
        let Some(action) = modal_action_from_key(&key, RENAME_ACTIONS) else {
            handle_rename_key(&mut self.state, key);
            return;
        };
        if action != ModalAction::Save {
            apply_rename_action(&mut self.state, action);
            return;
        }

        let new_name = self.state.name_input.trim().to_string();
        match self.state.mode {
            Mode::RenameWorkspace if !new_name.is_empty() => {
                if let Some(workspace_id) = self
                    .state
                    .workspaces
                    .get(self.state.selected)
                    .map(|workspace| workspace.id.clone())
                {
                    self.dispatch_runtime_mutation(
                        "tui.workspace.rename",
                        crate::api::schema::Method::WorkspaceRename(
                            crate::api::schema::WorkspaceRenameParams {
                                workspace_id,
                                label: new_name,
                            },
                        ),
                    );
                }
            }
            Mode::RenameTab if self.state.creating_new_tab => {
                if let Some(workspace_id) = self
                    .state
                    .active
                    .and_then(|idx| self.state.workspaces.get(idx))
                    .map(|workspace| workspace.id.clone())
                {
                    self.dispatch_runtime_mutation(
                        "tui.tab.create_named",
                        crate::api::schema::Method::TabCreate(
                            crate::api::schema::TabCreateParams {
                                workspace_id: Some(workspace_id),
                                cwd: None,
                                focus: true,
                                label: (!new_name.is_empty()).then_some(new_name),
                                env: Default::default(),
                            },
                        ),
                    );
                }
            }
            Mode::RenameTab => {
                if let Some(ws_idx) = self.state.active {
                    let tab_idx = self.state.workspaces[ws_idx].active_tab_index();
                    if let Some(tab_id) = self.public_tab_id(ws_idx, tab_idx) {
                        if !new_name.is_empty() {
                            self.dispatch_runtime_mutation(
                                "tui.tab.rename",
                                crate::api::schema::Method::TabRename(
                                    crate::api::schema::TabRenameParams {
                                        tab_id,
                                        label: new_name,
                                    },
                                ),
                            );
                        }
                    }
                }
            }
            Mode::RenamePane => {
                if let (Some(ws_idx), Some(pane_id)) =
                    (self.state.active, self.state.rename_pane_target)
                {
                    if let Some(pane_id) = self.public_pane_id(ws_idx, pane_id) {
                        self.runtime_pane_rename(
                            "tui.pane.rename",
                            crate::api::schema::PaneRenameParams {
                                pane_id,
                                label: Some(new_name),
                            },
                        );
                    }
                }
            }
            Mode::RenameGroup => {
                apply_rename_action(&mut self.state, action);
                return;
            }
            _ => {}
        }

        self.state.creating_new_tab = false;
        self.state.creating_new_group = false;
        self.state.group_icon_picker_open = false;
        self.state.group_default_directory_input.clear();
        self.state.group_modal_selected_field = 0;
        self.state.rename_group_target = None;
        self.state.rename_pane_target = None;
        self.state.requested_new_tab_name = None;
        self.state.name_input.clear();
        self.state.name_input_replace_on_type = false;
        leave_modal(&mut self.state);
    }
}

pub(super) fn apply_worktree_directory_action(
    state: &mut AppState,
    action: ModalAction,
) -> Option<String> {
    match action {
        ModalAction::Save => {
            let directory = state.name_input.trim().to_string();
            if directory.is_empty() {
                return None;
            }
            state.settings.pending_worktree_directory = Some(directory.clone());
            state.name_input.clear();
            state.name_input_replace_on_type = false;
            state.mode = Mode::Settings;
            Some(directory)
        }
        ModalAction::Clear => {
            state.name_input.clear();
            state.name_input_replace_on_type = false;
            None
        }
        ModalAction::Cancel => {
            state.name_input.clear();
            state.name_input_replace_on_type = false;
            state.mode = Mode::Settings;
            None
        }
        _ => None,
    }
}

fn clear_rename_input(state: &mut AppState) {
    active_rename_text_mut(state).clear();
    state.name_input_replace_on_type = false;
}

fn active_rename_text_mut(state: &mut AppState) -> &mut String {
    if state.mode == Mode::RenameGroup
        && state.creating_new_group
        && state.group_modal_selected_field == 1
    {
        &mut state.group_default_directory_input
    } else {
        &mut state.name_input
    }
}

fn rename_primary_input_selected(state: &AppState) -> bool {
    !(state.mode == Mode::RenameGroup
        && state.creating_new_group
        && state.group_modal_selected_field == 1)
}

pub(crate) fn insert_rename_input_text(state: &mut AppState, text: &str) {
    if state.name_input_replace_on_type && rename_primary_input_selected(state) {
        clear_rename_input(state);
    }
    active_rename_text_mut(state).push_str(text);
}

fn delete_rename_input_char(state: &mut AppState) {
    if state.name_input_replace_on_type && rename_primary_input_selected(state) {
        clear_rename_input(state);
    } else {
        active_rename_text_mut(state).pop();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenameWordDeleteClass {
    Word,
    Separator,
}

fn rename_word_delete_class(ch: char) -> RenameWordDeleteClass {
    if ch.is_alphanumeric() || ch == '_' {
        RenameWordDeleteClass::Word
    } else {
        RenameWordDeleteClass::Separator
    }
}

fn delete_rename_input_word(state: &mut AppState) {
    if state.name_input_replace_on_type && rename_primary_input_selected(state) {
        clear_rename_input(state);
        return;
    }

    let input = active_rename_text_mut(state);
    while input.chars().last().is_some_and(char::is_whitespace) {
        input.pop();
    }

    let Some(class) = input.chars().last().map(rename_word_delete_class) else {
        return;
    };

    while input
        .chars()
        .last()
        .is_some_and(|ch| !ch.is_whitespace() && rename_word_delete_class(ch) == class)
    {
        input.pop();
    }
}

pub(crate) fn handle_rename_key(state: &mut AppState, key: KeyEvent) {
    if let Some(action) = modal_action_from_key(&key, RENAME_ACTIONS) {
        apply_rename_action(state, action);
        return;
    }

    match key.code {
        KeyCode::Tab
            if state.mode == Mode::RenameGroup
                && state.creating_new_group
                && !key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            state.group_modal_selected_field = (state.group_modal_selected_field + 1) % 2;
            state.name_input_replace_on_type = false;
        }
        KeyCode::BackTab if state.mode == Mode::RenameGroup && state.creating_new_group => {
            state.group_modal_selected_field = (state.group_modal_selected_field + 1) % 2;
            state.name_input_replace_on_type = false;
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            active_rename_text_mut(state).clear();
            state.name_input_replace_on_type = false;
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            active_rename_text_mut(state).clear();
            state.name_input_replace_on_type = false;
        }
        KeyCode::Backspace
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            delete_rename_input_word(state);
        }
        KeyCode::Char('h' | 'w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_rename_input_word(state);
        }
        KeyCode::Backspace => delete_rename_input_char(state),
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            insert_rename_input_text(state, &c.to_string());
        }
        _ => {}
    }
}

pub(crate) fn handle_worktree_directory_key(state: &mut AppState, key: KeyEvent) {
    if let Some(action) = modal_action_from_key(&key, RENAME_ACTIONS) {
        apply_worktree_directory_action(state, action);
        return;
    }

    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            clear_rename_input(state);
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            clear_rename_input(state);
        }
        KeyCode::Backspace
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            delete_rename_input_word(state);
        }
        KeyCode::Char('h' | 'w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_rename_input_word(state);
        }
        KeyCode::Backspace => delete_rename_input_char(state),
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            if state.name_input_replace_on_type {
                clear_rename_input(state);
            }
            state.name_input.push(c);
        }
        _ => {}
    }
}

pub(crate) fn handle_resize_key(state: &mut AppState, raw_key: TerminalKey) {
    let key = raw_key.as_key_event();
    if key.code == KeyCode::Esc
        || key.code == KeyCode::Enter
        || state.keybinds.resize_mode.matches_prefix_key(raw_key)
        || state.keybinds.resize_mode.matches_direct_key(raw_key)
    {
        if state.active.is_some() {
            state.mode = Mode::Terminal;
        } else {
            state.mode = Mode::Navigate;
        }
        return;
    }

    match key.code {
        KeyCode::Char('h') | KeyCode::Left => state.resize_pane(NavDirection::Left),
        KeyCode::Char('l') | KeyCode::Right => state.resize_pane(NavDirection::Right),
        KeyCode::Char('j') | KeyCode::Down => state.resize_pane(NavDirection::Down),
        KeyCode::Char('k') | KeyCode::Up => state.resize_pane(NavDirection::Up),
        _ => {}
    }
}

pub(super) fn open_confirm_close(state: &mut AppState) {
    state.mode = Mode::ConfirmClose;
}

pub(super) fn open_confirm_delete_group(state: &mut AppState, group_idx: usize) {
    if group_idx < state.groups.len() && state.groups.len() > 1 {
        state.confirm_delete_group = Some(group_idx);
        state.mode = Mode::ConfirmDeleteGroup;
    }
}

pub(crate) fn confirm_close_accept(state: &mut AppState) {
    state.close_selected_workspace_from_ui();
}

pub(crate) fn confirm_close_cancel(state: &mut AppState) {
    state.mode = Mode::Navigate;
}

pub(crate) fn confirm_delete_group_accept(state: &mut AppState) {
    if let Some(group_idx) = state.confirm_delete_group.take() {
        let _ = state.delete_group(group_idx);
    }
    if state.active.is_some() {
        state.mode = Mode::Terminal;
    } else {
        state.mode = Mode::Navigate;
    }
}

pub(crate) fn confirm_delete_group_cancel(state: &mut AppState) {
    state.confirm_delete_group = None;
    state.mode = Mode::Navigate;
}

pub(crate) fn handle_confirm_close_key(state: &mut AppState, key: KeyEvent) {
    match modal_action_from_key(&key, CONFIRM_CLOSE_ACTIONS) {
        Some(ModalAction::Confirm) => confirm_close_accept(state),
        Some(ModalAction::Cancel) => confirm_close_cancel(state),
        _ => {}
    }
}

pub(crate) fn handle_confirm_delete_group_key(state: &mut AppState, key: KeyEvent) {
    match modal_action_from_key(&key, CONFIRM_CLOSE_ACTIONS) {
        Some(ModalAction::Confirm) => confirm_delete_group_accept(state),
        Some(ModalAction::Cancel) => confirm_delete_group_cancel(state),
        _ => {}
    }
}

fn activate_tab_context_target(state: &mut AppState, ws_idx: usize, tab_idx: usize) -> bool {
    if state
        .workspaces
        .get(ws_idx)
        .is_none_or(|workspace| tab_idx >= workspace.tabs.len())
    {
        return false;
    }
    state.selected = ws_idx;
    state.active = Some(ws_idx);
    state.switch_tab(tab_idx);
    true
}

fn close_other_tabs_from_context(state: &mut AppState, ws_idx: usize, tab_idx: usize) {
    if !activate_tab_context_target(state, ws_idx, tab_idx) {
        return;
    }
    let tab_count = state.workspaces[ws_idx].tabs.len();
    for idx in ((tab_idx + 1)..tab_count).rev() {
        let _ = state.close_tab_at(idx);
    }
    for idx in (0..tab_idx).rev() {
        let _ = state.close_tab_at(idx);
    }
    state.switch_tab(0);
}

pub(crate) fn apply_context_menu_action(
    state: &mut AppState,
    terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
    menu: ContextMenuState,
    idx: usize,
) {
    let item = menu.items().get(idx).copied();
    if item.is_some_and(|item| {
        ContextMenuState::item_is_separator(item) || ContextMenuState::item_is_section_header(item)
    }) {
        state.context_menu = Some(menu);
        state.mode = Mode::ContextMenu;
        return;
    }
    match (menu.kind, item) {
        (
            ContextMenuKind::Sidebar { group_idx } | ContextMenuKind::Group { group_idx, .. },
            Some("space"),
        ) => {
            state.switch_group(group_idx);
            state.request_new_workspace = true;
            leave_modal(state);
        }
        (
            ContextMenuKind::Sidebar { group_idx } | ContextMenuKind::Group { group_idx, .. },
            Some("group"),
        ) => {
            state.switch_group(group_idx);
            open_new_group_dialog(state);
        }
        (ContextMenuKind::Workspace { ws_idx, .. }, Some("agent")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            super::agent_profile_picker::open_new_agent_picker_for_workspace(state, ws_idx);
        }
        (ContextMenuKind::Workspace { ws_idx, .. }, Some("tab")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            request_new_tab_from_ui(state);
        }
        (ContextMenuKind::NewTabButton { ws_idx, .. }, Some("tab")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            request_new_tab_from_ui(state);
        }
        (ContextMenuKind::NewTabButton { ws_idx, .. }, Some("agent")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            super::agent_profile_picker::open_new_agent_picker_for_workspace(state, ws_idx);
        }
        (ContextMenuKind::Workspace { ws_idx, .. }, Some("diff"))
        | (ContextMenuKind::NewTabButton { ws_idx, .. }, Some("diff")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            state.request_open_git_diff_command = true;
            leave_modal(state);
        }
        (ContextMenuKind::Workspace { ws_idx, .. }, Some("rename")) => {
            open_rename_workspace(state, terminal_runtimes, ws_idx);
        }
        (ContextMenuKind::Workspace { ws_idx, .. }, Some("settings")) => {
            super::settings::open_workspace_settings(state, ws_idx);
        }
        (ContextMenuKind::Group { group_idx, .. }, Some("settings")) => {
            super::settings::open_group_settings(state, group_idx);
        }
        (
            ContextMenuKind::Group {
                group_idx,
                can_delete: true,
                ..
            },
            Some("delete"),
        ) => {
            open_confirm_delete_group(state, group_idx);
        }
        (ContextMenuKind::Workspace { ws_idx, .. }, Some("close")) => {
            state.selected = ws_idx;
            if state.confirm_close {
                open_confirm_close(state);
            } else {
                state.close_selected_workspace_from_ui();
            }
        }
        (
            ContextMenuKind::Tab {
                ws_idx, tab_idx, ..
            },
            Some("rename"),
        ) => {
            if activate_tab_context_target(state, ws_idx, tab_idx) {
                open_rename_active_tab(state, false);
            }
        }
        (
            ContextMenuKind::Tab {
                ws_idx, tab_idx, ..
            },
            Some("close"),
        ) => {
            if activate_tab_context_target(state, ws_idx, tab_idx) && !state.close_tab() {
                state.return_to_active_workspace_mode();
            }
        }
        (
            ContextMenuKind::Tab {
                ws_idx, tab_idx, ..
            },
            Some("close other tabs"),
        ) => {
            close_other_tabs_from_context(state, ws_idx, tab_idx);
        }
        (ContextMenuKind::Pane { pane_id, .. }, Some("rename pane")) => {
            open_rename_pane(state, pane_id);
        }
        (ContextMenuKind::Pane { pane_id, .. }, Some("clear pane name")) => {
            if let Some(ws_idx) = state.active {
                if let Some(ws) = state.workspaces.get(ws_idx) {
                    if let Some(pane) = ws.pane_state(pane_id) {
                        let terminal_id = pane.attached_terminal_id.clone();
                        if let Some(terminal) = state.terminals.get_mut(&terminal_id) {
                            terminal.clear_manual_label();
                            state.mark_session_dirty();
                        }
                    }
                }
            }
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("split vertical")) => {
            state.split_pane(terminal_runtimes, Direction::Horizontal);
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("split horizontal")) => {
            state.split_pane(terminal_runtimes, Direction::Vertical);
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("zoom")) => {
            state.toggle_zoom();
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("close pane")) => {
            if !state.close_pane() {
                state.return_to_active_workspace_mode();
            }
        }
        _ => leave_modal(state),
    }
}

pub(crate) fn handle_context_menu_key(
    state: &mut AppState,
    terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
    key: KeyEvent,
) {
    match key.code {
        KeyCode::Esc => {
            state.context_menu = None;
            leave_modal(state);
        }
        KeyCode::Up => {
            if let Some(menu) = &mut state.context_menu {
                menu.move_prev();
            }
        }
        KeyCode::Down => {
            if let Some(menu) = &mut state.context_menu {
                menu.move_next();
            }
        }
        KeyCode::Enter => {
            if let Some(menu) = state.context_menu.take() {
                let idx = menu.list.selected;
                apply_context_menu_action(state, terminal_runtimes, menu, idx);
            }
        }
        _ => {}
    }
}

impl AppState {
    pub(crate) fn global_menu_item_at(&self, col: u16, row: u16) -> Option<GlobalMenuAction> {
        let rect = self.global_menu_rect();
        if col <= rect.x
            || col >= rect.x + rect.width.saturating_sub(1)
            || row <= rect.y
            || row >= rect.y + rect.height.saturating_sub(1)
        {
            return None;
        }
        let idx = (row - rect.y - 1) as usize;
        global_menu_actions(self).get(idx).copied()
    }

    pub(crate) fn group_menu_item_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<super::sidebar::GroupMenuAction> {
        let row_idx = self.group_menu_row_at(col, row)?;
        self.group_menu_action_for_row(row_idx)
    }

    pub(crate) fn group_menu_row_at(&self, col: u16, row: u16) -> Option<usize> {
        let rect = self.group_menu_rect();
        if col <= rect.x
            || col >= rect.x + rect.width.saturating_sub(1)
            || row <= rect.y
            || row >= rect.y + rect.height.saturating_sub(1)
        {
            return None;
        }
        let idx = (row - rect.y - 1) as usize;
        (idx < self.group_menu_labels().len()).then_some(idx)
    }

    pub(crate) fn agent_menu_item_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<super::sidebar::AgentMenuAction> {
        let row_idx = self.agent_menu_row_at(col, row)?;
        self.agent_menu_action_for_row(row_idx)
    }

    pub(crate) fn agent_menu_row_at(&self, col: u16, row: u16) -> Option<usize> {
        let rect = self.agent_menu_rect();
        if col <= rect.x
            || col >= rect.x + rect.width.saturating_sub(1)
            || row <= rect.y
            || row >= rect.y + rect.height.saturating_sub(1)
        {
            return None;
        }
        let idx = (row - rect.y - 1) as usize;
        (idx < self.agent_menu_labels().len() && self.agent_menu_action_for_row(idx).is_some())
            .then_some(idx)
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::super::{capture_snapshot, state_with_workspaces};
    use super::*;

    fn config_env_lock() -> &'static std::sync::Mutex<()> {
        crate::config::test_config_env_lock()
    }

    fn temp_config_path(name: &str) -> std::path::PathBuf {
        let unique = format!(
            "omh-modal-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("config.toml")
    }

    fn outdated_codex_recommendation() -> crate::integration::IntegrationRecommendation {
        crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Codex,
            label: "codex",
            command: "codex",
            available: true,
            path: std::path::PathBuf::from("/tmp/omh-test-codex"),
            state: crate::integration::IntegrationStatusKind::Outdated,
        }
    }

    #[test]
    fn global_menu_update_integrations_entry_opens_integrations_settings() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![outdated_codex_recommendation()];

        let labels = state.global_menu_labels();
        let integrations_idx = labels
            .iter()
            .position(|label| *label == "integrations")
            .expect("outdated integration should surface a distinct global menu entry");
        assert_ne!(labels[integrations_idx], "settings");
        assert!(!state.global_menu_item_has_badge("settings"));
        assert!(state.global_menu_item_has_badge("integrations"));

        open_global_menu(&mut state);
        state.global_menu.selected = integrations_idx;
        handle_global_menu_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Settings);
        assert_eq!(
            state.settings.section,
            crate::app::state::SettingsSection::Integrations
        );
    }

    #[test]
    fn custom_resize_key_exits_resize_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Resize;
        state.keybinds.resize_mode = crate::config::ActionKeybinds::prefix("g");

        handle_resize_key(
            &mut state,
            TerminalKey::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn direct_resize_key_exits_resize_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Resize;
        state.keybinds.resize_mode = crate::config::ActionKeybinds::direct("ctrl+alt+r");

        handle_resize_key(
            &mut state,
            TerminalKey::new(
                KeyCode::Char('r'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
        );

        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn resize_key_exit_matches_enhanced_shifted_punctuation() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Resize;
        state.keybinds.resize_mode = crate::config::ActionKeybinds::prefix("?");

        handle_resize_key(
            &mut state,
            TerminalKey::new(KeyCode::Char('/'), KeyModifiers::SHIFT)
                .with_shifted_codepoint('?' as u32),
        );

        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn worktree_directory_editor_updates_pending_setting() {
        let mut state = state_with_workspaces(&["test"]);
        state.worktree_directory = std::path::PathBuf::from("/tmp/omh-worktrees");
        open_worktree_directory_editor(&mut state);

        state.name_input = "~/Projects/omh-worktrees".to_string();
        handle_worktree_directory_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Settings);
        assert_eq!(
            state.settings.pending_worktree_directory.as_deref(),
            Some("~/Projects/omh-worktrees")
        );
    }

    #[test]
    fn detach_requests_client_detach_in_persistence_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.detach_exits = false;

        request_detach(&mut state);

        assert!(state.detach_requested);
        assert!(!state.should_quit);
    }

    #[test]
    fn detach_exits_in_no_session_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.detach_exits = true;

        request_detach(&mut state);

        assert!(state.should_quit);
        assert!(!state.detach_requested);
    }

    #[test]
    fn global_menu_changelog_opens_saved_release_notes() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("changelog-saved-release-notes");
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);
        crate::release_notes::save_pending(env!("CARGO_PKG_VERSION"), "### Changed\n- Menu")
            .unwrap();

        let mut state = state_with_workspaces(&["test"]);
        state.latest_release_notes_available = true;

        assert!(global_menu_actions(&state).contains(&GlobalMenuAction::Changelog));

        apply_global_menu_action(&mut state, GlobalMenuAction::Changelog);

        assert_eq!(state.mode, Mode::ReleaseNotes);
        assert_eq!(
            state
                .release_notes
                .as_ref()
                .map(|notes| notes.body.as_str()),
            Some("### Changed\n- Menu")
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn global_menu_changelog_opens_bundled_changelog_without_saved_notes() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("changelog-empty-state");
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());

        let mut state = state_with_workspaces(&["test"]);

        assert!(global_menu_actions(&state).contains(&GlobalMenuAction::Changelog));

        apply_global_menu_action(&mut state, GlobalMenuAction::Changelog);

        assert_eq!(state.mode, Mode::ReleaseNotes);
        let notes = state
            .release_notes
            .as_ref()
            .expect("release notes modal state");
        assert_eq!(notes.body, "No public releases yet.");
    }

    #[test]
    fn rename_modal_keyboard_actions_update_workspace_name() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameWorkspace;
        state.name_input = "hello".into();

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(state.name_input.is_empty());

        state.name_input = "renamed".into();
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.workspaces[0].display_name(), "renamed");
        let snapshot = capture_snapshot(&state);
        assert_eq!(
            snapshot.workspaces[0].custom_name.as_deref(),
            Some("renamed")
        );
    }

    #[test]
    fn tab_rename_updates_captured_snapshot() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameTab;
        state.name_input = "logs".into();

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        let snapshot = capture_snapshot(&state);
        assert_eq!(
            snapshot.workspaces[0].tabs[0].custom_name.as_deref(),
            Some("logs")
        );
    }

    #[test]
    fn rename_cancel_returns_to_terminal_when_workspace_is_active() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameTab;
        state.name_input = "test".into();

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert!(state.name_input.is_empty());
    }

    #[test]
    fn rename_modal_replaces_prefilled_text_on_first_type() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameTab;
        state.name_input = "2".into();
        state.name_input_replace_on_type = true;

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::empty()),
        );
        assert_eq!(state.name_input, "n");
        assert!(!state.name_input_replace_on_type);

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::empty()),
        );
        assert_eq!(state.name_input, "ne");
    }

    #[test]
    fn rename_modal_replaces_prefilled_text_on_paste() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameTab;
        state.name_input = "2".into();
        state.name_input_replace_on_type = true;

        insert_rename_input_text(&mut state, "feature/logs");

        assert_eq!(state.name_input, "feature/logs");
        assert!(!state.name_input_replace_on_type);

        insert_rename_input_text(&mut state, "-copy");

        assert_eq!(state.name_input, "feature/logs-copy");
    }

    #[test]
    fn rename_modal_handles_line_editing_shortcuts() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameWorkspace;
        state.name_input = "website zero".into();

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
        );
        assert_eq!(state.name_input, "website zer");

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::CONTROL),
        );
        assert_eq!(state.name_input, "website ");

        state.name_input = "website-zero".into();
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT),
        );
        assert_eq!(state.name_input, "website-");

        state.name_input = "website-zero".into();
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.name_input, "website-");

        state.name_input = "website-zero".into();
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.name_input, "website-");

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::SUPER),
        );
        assert!(state.name_input.is_empty());

        state.name_input = "website zero".into();
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        );
        assert!(state.name_input.is_empty());
    }

    #[test]
    fn rename_modal_does_not_insert_modified_shortcut_chars() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::RenameWorkspace;
        state.name_input = "website".into();

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL),
        );
        assert_eq!(state.name_input, "website");

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT),
        );
        assert_eq!(state.name_input, "websiteZ");
    }

    #[test]
    fn navigator_search_accepts_pasted_text_when_focused() {
        let mut state = state_with_workspaces(&["alpha", "beta"]);
        state.mode = Mode::Navigator;
        state.navigator.search_focused = true;
        state.navigator.state_filter = Some(NavigatorStateFilter::Working);

        insert_navigator_search_text(&mut state, "beta");

        assert_eq!(state.navigator.query, "beta");
        assert_eq!(state.navigator.state_filter, None);
    }

    #[test]
    fn navigator_search_ignores_paste_when_search_is_not_focused() {
        let mut state = state_with_workspaces(&["alpha", "beta"]);
        state.mode = Mode::Navigator;
        state.navigator.search_focused = false;

        insert_navigator_search_text(&mut state, "beta");

        assert!(state.navigator.query.is_empty());
    }

    #[test]
    fn open_rename_active_tab_can_prefill_default_new_tab_name() {
        let mut state = state_with_workspaces(&["test"]);
        state.workspaces[0].test_add_tab(None);
        state.workspaces[0].switch_tab(1);

        open_rename_active_tab(&mut state, true);

        assert_eq!(state.mode, Mode::RenameTab);
        assert_eq!(state.name_input, "2");
        assert!(state.name_input_replace_on_type);
    }

    #[test]
    fn cancel_new_tab_dialog_leaves_workspace_unchanged() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_tab_dialog(&mut state);

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert!(!state.creating_new_tab);
        assert!(!state.request_new_tab);
        assert!(state.requested_new_tab_name.is_none());
        assert_eq!(state.workspaces[0].tabs.len(), 1);
    }

    #[test]
    fn saving_new_tab_dialog_requests_creation_with_name() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_tab_dialog(&mut state);
        state.name_input = "logs".into();
        state.name_input_replace_on_type = false;

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert!(!state.creating_new_tab);
        assert!(state.request_new_tab);
        assert_eq!(state.requested_new_tab_name.as_deref(), Some("logs"));
    }

    #[test]
    fn saving_new_group_dialog_creates_and_focuses_group() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_group_dialog(&mut state);
        state.name_input = "client".into();
        state.name_input_replace_on_type = false;

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Navigate);
        assert!(!state.creating_new_group);
        assert_eq!(state.groups.len(), 2);
        assert_eq!(state.groups[1].name, "client");
        assert_eq!(state.groups[1].icon, DEFAULT_GROUP_ICON);
        assert_eq!(state.active_group, 1);
        assert!(state.group_filter_enabled);
    }

    #[test]
    fn saving_new_group_dialog_stores_optional_default_directory() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_group_dialog(&mut state);
        state.name_input = "client".into();
        state.name_input_replace_on_type = false;
        state.group_default_directory_input = "/tmp/client".into();

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.groups[1].name, "client");
        assert_eq!(
            state.groups[1].default_directory.as_deref(),
            Some(std::path::Path::new("/tmp/client"))
        );
    }

    #[test]
    fn new_group_dialog_tabs_between_name_and_default_directory() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_group_dialog(&mut state);
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::empty()),
        );

        assert_eq!(state.group_modal_selected_field, 1);
        assert_eq!(state.group_default_directory_input, "/");
        assert_eq!(state.name_input, "group 2");
    }

    #[test]
    fn saving_new_tab_dialog_with_default_name_keeps_tab_auto_named() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_tab_dialog(&mut state);

        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert!(!state.creating_new_tab);
        assert!(state.request_new_tab);
        assert!(state.requested_new_tab_name.is_none());
    }

    #[test]
    fn closing_first_auto_tab_keeps_stable_remaining_auto_tab_and_next_prompt() {
        let mut state = state_with_workspaces(&["test"]);
        open_new_tab_dialog(&mut state);
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        state.workspaces[0].test_add_tab(state.requested_new_tab_name.as_deref());
        state.request_new_tab = false;
        state.requested_new_tab_name = None;

        state.workspaces[0].close_tab(0);
        state.workspaces[0].switch_tab(0);

        assert_eq!(state.workspaces[0].tabs[0].display_name(), "2");
        assert!(state.workspaces[0].tabs[0].custom_name.is_none());

        open_new_tab_dialog(&mut state);
        assert_eq!(state.name_input, "3");
    }

    #[test]
    fn renaming_auto_tab_to_its_default_number_keeps_it_auto_named() {
        let mut state = state_with_workspaces(&["test"]);
        state.workspaces[0].test_add_tab(None);
        state.workspaces[0].switch_tab(1);

        open_rename_active_tab(&mut state, false);
        handle_rename_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert!(state.workspaces[0].tabs[1].custom_name.is_none());
        assert_eq!(state.workspaces[0].tabs[1].display_name(), "2");
    }

    #[test]
    fn confirm_close_keyboard_actions_are_direct_not_focused() {
        let mut state = state_with_workspaces(&["a", "b"]);
        state.mode = Mode::ConfirmClose;
        state.selected = 1;

        handle_confirm_close_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::Navigate);
        assert_eq!(state.workspaces.len(), 2);

        state.mode = Mode::ConfirmClose;
        handle_confirm_close_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn group_context_menu_keyboard_skips_section_headers() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Work".to_string());
        state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Group {
                group_idx,
                can_delete: true,
            },
            x: 0,
            y: 0,
            list: ModalListState::new(2),
        });
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();

        handle_context_menu_key(
            &mut state,
            &mut terminal_runtimes,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.context_menu.as_ref().unwrap().list.selected, 5);
        assert_eq!(state.context_menu.as_ref().unwrap().list.visible(), Some(5));

        handle_context_menu_key(
            &mut state,
            &mut terminal_runtimes,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.context_menu.as_ref().unwrap().list.selected, 2);
        assert_eq!(state.context_menu.as_ref().unwrap().list.visible(), Some(2));

        state.context_menu.as_mut().unwrap().list = ModalListState::hidden(1);
        handle_context_menu_key(
            &mut state,
            &mut terminal_runtimes,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.context_menu.as_ref().unwrap().list.visible(), Some(1));

        state.context_menu.as_mut().unwrap().list = ModalListState::hidden(8);
        handle_context_menu_key(
            &mut state,
            &mut terminal_runtimes,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.context_menu.as_ref().unwrap().list.visible(), Some(8));
    }

    #[test]
    fn workspace_context_menu_new_agent_opens_profile_picker() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![
            crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Codex,
                label: "codex",
                command: "codex",
                available: true,
                path: std::path::PathBuf::from("/tmp/omh-test-codex"),
                state: crate::integration::IntegrationStatusKind::Current,
            },
            crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Claude,
                label: "claude",
                command: "claude",
                available: true,
                path: std::path::PathBuf::from("/tmp/omh-test-claude"),
                state: crate::integration::IntegrationStatusKind::Current,
            },
        ];
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let menu = ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 0,
                can_diff: false,
            },
            x: 0,
            y: 0,
            list: ModalListState::new(1),
        };

        apply_context_menu_action(&mut state, &mut terminal_runtimes, menu, 2);

        assert_eq!(state.mode, Mode::AgentProfilePicker);
        assert_eq!(state.agent_profile_picker.ws_idx, 0);
    }

    #[test]
    fn closing_last_tab_from_context_menu_empties_workspace() {
        let mut state = state_with_workspaces(&["test"]);
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let menu = ContextMenuState {
            kind: ContextMenuKind::Tab {
                ws_idx: 0,
                tab_idx: 0,
                can_diff: false,
            },
            x: 0,
            y: 0,
            list: ModalListState::new(1),
        };

        apply_context_menu_action(&mut state, &mut terminal_runtimes, menu, 1);

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.workspaces.len(), 1);
        assert!(state.workspaces[0].tabs.is_empty());
    }

    #[test]
    fn tab_context_menu_close_other_tabs_keeps_only_target_tab_in_target_workspace() {
        let mut state = state_with_workspaces(&["home", "api"]);
        state.workspaces[1].test_add_tab(Some("two"));
        state.workspaces[1].test_add_tab(Some("three"));
        state.workspaces[1].test_add_tab(Some("four"));
        state.active = Some(0);
        state.selected = 0;
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let menu = ContextMenuState {
            kind: ContextMenuKind::Tab {
                ws_idx: 1,
                tab_idx: 1,
                can_diff: false,
            },
            x: 0,
            y: 0,
            list: ModalListState::new(0),
        };
        let close_other_tabs = menu
            .items()
            .iter()
            .position(|item| *item == "close other tabs")
            .expect("tab menu exposes close other tabs");

        apply_context_menu_action(&mut state, &mut terminal_runtimes, menu, close_other_tabs);

        let remaining: Vec<_> = state.workspaces[1]
            .tabs
            .iter()
            .map(|tab| tab.display_name().to_string())
            .collect();
        assert_eq!(remaining, vec!["two"]);
        assert_eq!(state.workspaces[1].active_tab, 0);
        assert_eq!(state.active, Some(1));
        assert_eq!(state.selected, 1);
        assert_eq!(state.workspaces[0].tabs.len(), 1);
    }

    #[test]
    fn context_menu_close_pane_last_parent_group_pane_keeps_confirmation_mode() {
        let mut state = state_with_workspaces(&["main", "issue"]);
        state.active = Some(0);
        state.selected = 1;
        state.workspaces[0].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: "/repo/herdr".into(),
            is_linked_worktree: false,
        });
        state.workspaces[1].worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: "/repo/herdr-issue".into(),
            is_linked_worktree: true,
        });
        let pane_id = state.workspaces[0].tabs[0].root_pane;
        let menu = ContextMenuState {
            kind: ContextMenuKind::Pane {
                ws_idx: 0,
                pane_id,
                has_manual_label: false,
            },
            x: 0,
            y: 0,
            list: ModalListState::new(4),
        };
        let mut terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();

        apply_context_menu_action(&mut state, &mut terminal_runtimes, menu, 4);

        assert_eq!(state.selected, 0);
        assert_eq!(state.mode, Mode::ConfirmClose);
        assert_eq!(state.workspaces.len(), 2);
    }
    #[test]
    fn navigator_bulk_expansion_keys_change_the_visible_hierarchy() {
        let mut state = state_with_workspaces(&["home", "api"]);
        state.workspaces[0].test_split(ratatui::layout::Direction::Horizontal);
        state.navigator.expanded_groups =
            state.groups.iter().map(|group| group.id.clone()).collect();
        state.navigator.expanded_workspaces = state
            .workspaces
            .iter()
            .map(|workspace| workspace.id.clone())
            .collect();

        handle_navigator_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty()),
        );

        let collapsed_rows = state.navigator_rows();
        assert!(
            collapsed_rows
                .iter()
                .all(|row| row.is_group && !row.expanded),
            "C should leave only collapsed group roots visible"
        );

        handle_navigator_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::empty()),
        );

        let expanded_rows = state.navigator_rows();
        assert!(
            expanded_rows
                .iter()
                .filter(|row| row.is_group || row.is_workspace)
                .all(|row| row.expanded),
            "E should visibly expand every branch"
        );
        assert!(
            expanded_rows
                .iter()
                .any(|row| matches!(row.target, crate::app::state::NavigatorTarget::Pane { .. })),
            "expanded workspace branches should reveal their panes"
        );
    }
}
