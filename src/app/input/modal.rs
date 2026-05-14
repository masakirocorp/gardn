use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Direction, Rect};

use crate::{
    app::state::{
        key_matches, AppState, ContextMenuKind, ContextMenuState, MenuListState, Mode,
        DEFAULT_GROUP_ICON,
    },
    layout::NavDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModalAction {
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

pub(super) fn modal_action_from_buttons<A: Copy>(
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
    Quit,
    WhatsNew,
    Keybinds,
    ReloadConfig,
    Settings,
}

pub(super) fn global_menu_actions(state: &AppState) -> Vec<GlobalMenuAction> {
    let mut actions = vec![
        GlobalMenuAction::Settings,
        GlobalMenuAction::Keybinds,
        GlobalMenuAction::ReloadConfig,
    ];
    if state.update_available.is_some() || state.latest_release_notes_available {
        actions.push(GlobalMenuAction::WhatsNew);
    }
    actions.push(GlobalMenuAction::Quit);
    actions
}

pub(super) fn open_global_menu(state: &mut AppState) {
    state.global_menu = MenuListState::new(0);
    state.mode = Mode::GlobalMenu;
}

pub(super) fn open_group_menu(state: &mut AppState) {
    let highlighted = if state.group_filter_enabled {
        state.active_group + 3
    } else {
        0
    };
    state.group_menu = MenuListState::new(highlighted);
    state.mode = Mode::GroupMenu;
}

pub(super) fn open_agent_menu(state: &mut AppState) {
    let highlighted = match state.agent_panel_scope {
        crate::app::state::AgentPanelScope::AllWorkspaces => 0,
        crate::app::state::AgentPanelScope::CurrentWorkspace => 2,
        crate::app::state::AgentPanelScope::CurrentGroup => 4,
    };
    state.agent_menu = MenuListState::new(highlighted);
    state.mode = Mode::AgentMenu;
}

pub(super) fn open_keybind_help(state: &mut AppState) {
    state.keybind_help.scroll = 0;
    state.mode = Mode::KeybindHelp;
}

fn open_update_release_notes(state: &mut AppState) {
    let Some(notes) = crate::release_notes::load_latest() else {
        return;
    };

    state.release_notes = Some(crate::app::state::ReleaseNotesState {
        version: notes.version,
        body: notes.body,
        scroll: 0,
        preview: notes.preview,
    });
    state.mode = Mode::ReleaseNotes;
}

pub(super) fn request_quit_or_detach(state: &mut AppState) {
    if state.quit_detaches {
        state.detach_requested = true;
    } else {
        state.should_quit = true;
    }
}

pub(super) fn apply_global_menu_action(state: &mut AppState, action: GlobalMenuAction) {
    match action {
        GlobalMenuAction::Quit => {
            leave_modal(state);
            request_quit_or_detach(state);
        }
        GlobalMenuAction::WhatsNew => open_update_release_notes(state),
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
            if let Some(action) = actions.get(state.global_menu.highlighted).copied() {
                apply_global_menu_action(state, action);
            }
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
                    .group_menu_action_for_row(state.group_menu.highlighted)
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
                    .group_menu_action_for_row(state.group_menu.highlighted)
                    .is_some()
                {
                    break;
                }
            }
        }
        KeyCode::Enter => {
            let Some(action) = state.group_menu_action_for_row(state.group_menu.highlighted) else {
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
                super::sidebar::GroupMenuAction::NewGroup => open_new_group_dialog(state),
            }
        }
        _ => {}
    }
}

pub(crate) fn handle_agent_menu_key(state: &mut AppState, key: KeyEvent) {
    let labels = state.agent_menu_labels();
    match key.code {
        KeyCode::Esc => leave_modal(state),
        KeyCode::Up | KeyCode::Char('k') => {
            let mut idx = state.agent_menu.highlighted;
            while idx > 0 {
                idx -= 1;
                if state.agent_menu_action_for_row(idx).is_some() {
                    state.agent_menu.highlighted = idx;
                    break;
                }
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let mut idx = state.agent_menu.highlighted;
            while idx + 1 < labels.len() {
                idx += 1;
                if state.agent_menu_action_for_row(idx).is_some() {
                    state.agent_menu.highlighted = idx;
                    break;
                }
            }
        }
        KeyCode::Enter => {
            let Some(action) = state.agent_menu_action_for_row(state.agent_menu.highlighted) else {
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

pub(crate) fn handle_keybind_help_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => state.scroll_keybind_help(-1),
        KeyCode::Down | KeyCode::Char('j') => state.scroll_keybind_help(1),
        KeyCode::PageUp => state.scroll_keybind_help(-8),
        KeyCode::PageDown => state.scroll_keybind_help(8),
        KeyCode::Home => state.keybind_help.scroll = 0,
        KeyCode::End => state.keybind_help.scroll = state.keybind_help_max_scroll(),
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => leave_modal(state),
        _ => {}
    }
}

pub(super) fn open_rename_workspace(state: &mut AppState, ws_idx: usize) {
    state.selected = ws_idx;
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_group_target = None;
    state.rename_pane_target = None;
    state.name_input = state.workspaces[ws_idx].display_name();
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
    state.creating_new_tab = false;
    state.creating_new_group = false;
    state.group_icon_picker_open = false;
    state.requested_new_tab_name = None;
    state.rename_pane_target = Some(pane_id);
    state.name_input = pane.manual_label.clone().unwrap_or_default();
    state.name_input_replace_on_type = pane.manual_label.is_none();
    state.mode = Mode::RenamePane;
}

fn next_new_tab_default_name(state: &AppState) -> String {
    state
        .active
        .and_then(|i| state.workspaces.get(i))
        .map(|ws| (ws.tabs.len() + 1).to_string())
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

pub(super) fn open_new_group_dialog(state: &mut AppState) {
    state.creating_new_group = true;
    state.creating_new_tab = false;
    state.group_icon_input = DEFAULT_GROUP_ICON.to_string();
    state.group_icon_picker_open = false;
    state.rename_group_target = None;
    state.requested_new_tab_name = None;
    state.rename_pane_target = None;
    state.name_input = next_new_group_default_name(state);
    state.name_input_replace_on_type = true;
    state.mode = Mode::RenameGroup;
}

pub(super) fn leave_modal(state: &mut AppState) {
    if state.active.is_some() {
        state.mode = Mode::Terminal;
    } else {
        state.mode = Mode::Navigate;
    }
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
                    let default_name = next_new_group_default_name(state);
                    let name = if new_name.is_empty() {
                        default_name
                    } else {
                        new_name
                    };
                    let group_idx =
                        state.create_group_with_icon(name, state.group_icon_input.clone());
                    state.switch_group(group_idx);
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
                        if let Some(ws) = state.workspaces.get_mut(ws_idx) {
                            if let Some(pane) = ws.pane_state_mut(pane_id) {
                                pane.set_manual_label(new_name);
                                state.mark_session_dirty();
                            }
                        }
                    }
                }
                _ => {}
            }
            state.creating_new_tab = false;
            state.creating_new_group = false;
            state.group_icon_picker_open = false;
            state.rename_group_target = None;
            state.rename_pane_target = None;
            state.name_input.clear();
            state.name_input_replace_on_type = false;
            leave_modal(state);
        }
        ModalAction::Clear => {
            state.name_input.clear();
            state.name_input_replace_on_type = false;
        }
        ModalAction::Cancel => {
            state.creating_new_tab = false;
            state.creating_new_group = false;
            state.group_icon_picker_open = false;
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

pub(crate) fn handle_rename_key(state: &mut AppState, key: KeyEvent) {
    if let Some(action) = modal_action_from_key(&key, RENAME_ACTIONS) {
        apply_rename_action(state, action);
        return;
    }

    match key.code {
        KeyCode::Backspace => {
            if state.name_input_replace_on_type {
                state.name_input.clear();
                state.name_input_replace_on_type = false;
            } else {
                state.name_input.pop();
            }
        }
        KeyCode::Char(c) => {
            if state.name_input_replace_on_type {
                state.name_input.clear();
                state.name_input_replace_on_type = false;
            }
            state.name_input.push(c);
        }
        _ => {}
    }
}

pub(crate) fn handle_resize_key(state: &mut AppState, key: KeyEvent) {
    if key.code == KeyCode::Esc
        || key.code == KeyCode::Enter
        || key_matches(
            &key,
            state.keybinds.resize_mode.0,
            state.keybinds.resize_mode.1,
        )
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

pub(super) fn confirm_close_accept(state: &mut AppState) {
    state.close_selected_workspace();
    if state.workspaces.is_empty() {
        state.mode = Mode::Navigate;
    } else {
        state.mode = Mode::Terminal;
    }
}

pub(super) fn confirm_close_cancel(state: &mut AppState) {
    state.mode = Mode::Navigate;
}

pub(super) fn confirm_delete_group_accept(state: &mut AppState) {
    if let Some(group_idx) = state.confirm_delete_group.take() {
        let _ = state.delete_group(group_idx);
    }
    if state.active.is_some() {
        state.mode = Mode::Terminal;
    } else {
        state.mode = Mode::Navigate;
    }
}

pub(super) fn confirm_delete_group_cancel(state: &mut AppState) {
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

pub(super) fn apply_context_menu_action(state: &mut AppState, menu: ContextMenuState, idx: usize) {
    let item = menu.items().get(idx).copied();
    match (menu.kind, item) {
        (ContextMenuKind::Workspace { ws_idx }, Some("rename")) => {
            open_rename_workspace(state, ws_idx);
        }
        (ContextMenuKind::Group { group_idx, .. }, Some("rename")) => {
            open_rename_group_at(state, group_idx);
        }
        (ContextMenuKind::Group { group_idx, .. }, Some("theme")) => {
            super::settings::open_group_theme_settings(state, group_idx);
        }
        (
            ContextMenuKind::Group {
                group_idx,
                can_delete: true,
            },
            Some("delete"),
        ) => {
            open_confirm_delete_group(state, group_idx);
        }
        (ContextMenuKind::Workspace { ws_idx }, Some("close")) => {
            state.selected = ws_idx;
            if state.confirm_close {
                open_confirm_close(state);
            } else {
                state.close_selected_workspace();
                state.mode = Mode::Navigate;
            }
        }
        (ContextMenuKind::Tab { ws_idx, tab_idx }, Some("new tab")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            state.switch_tab(tab_idx);
            open_new_tab_dialog(state);
        }
        (ContextMenuKind::Tab { ws_idx, tab_idx }, Some("rename")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            state.switch_tab(tab_idx);
            open_rename_active_tab(state, false);
        }
        (ContextMenuKind::Tab { ws_idx, tab_idx }, Some("close")) => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            state.switch_tab(tab_idx);
            state.close_tab();
            if state.mode != Mode::ConfirmClose {
                state.mode = if state.active.is_some() {
                    Mode::Terminal
                } else {
                    Mode::Navigate
                };
            }
        }
        (ContextMenuKind::Pane { pane_id, .. }, Some("rename pane")) => {
            open_rename_pane(state, pane_id);
        }
        (ContextMenuKind::Pane { pane_id, .. }, Some("clear pane name")) => {
            if let Some(ws_idx) = state.active {
                if let Some(ws) = state.workspaces.get_mut(ws_idx) {
                    if let Some(pane) = ws.pane_state_mut(pane_id) {
                        pane.clear_manual_label();
                        state.mark_session_dirty();
                    }
                }
            }
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("split vertical")) => {
            state.split_pane(Direction::Horizontal);
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("split horizontal")) => {
            state.split_pane(Direction::Vertical);
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("fullscreen")) => {
            state.toggle_fullscreen();
            state.mode = Mode::Terminal;
        }
        (ContextMenuKind::Pane { .. }, Some("close pane")) => {
            state.close_pane();
            state.mode = if state.active.is_some() {
                Mode::Terminal
            } else {
                Mode::Navigate
            };
        }
        _ => leave_modal(state),
    }
}

pub(crate) fn handle_context_menu_key(state: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            state.context_menu = None;
            leave_modal(state);
        }
        KeyCode::Up => {
            if let Some(menu) = &mut state.context_menu {
                menu.list.move_prev();
            }
        }
        KeyCode::Down => {
            if let Some(menu) = &mut state.context_menu {
                menu.list.move_next(menu.items().len());
            }
        }
        KeyCode::Enter => {
            if let Some(menu) = state.context_menu.take() {
                let idx = menu.list.highlighted;
                apply_context_menu_action(state, menu, idx);
            }
        }
        _ => {}
    }
}

impl AppState {
    pub(super) fn global_menu_item_at(&self, col: u16, row: u16) -> Option<GlobalMenuAction> {
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

    pub(super) fn group_menu_item_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<super::sidebar::GroupMenuAction> {
        let row_idx = self.group_menu_row_at(col, row)?;
        self.group_menu_action_for_row(row_idx)
    }

    pub(super) fn group_menu_row_at(&self, col: u16, row: u16) -> Option<usize> {
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

    pub(super) fn agent_menu_item_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<super::sidebar::AgentMenuAction> {
        let row_idx = self.agent_menu_row_at(col, row)?;
        self.agent_menu_action_for_row(row_idx)
    }

    pub(super) fn agent_menu_row_at(&self, col: u16, row: u16) -> Option<usize> {
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
    use ratatui::layout::Rect;

    use super::super::{capture_snapshot, state_with_workspaces};
    use super::*;

    #[test]
    fn custom_resize_key_exits_resize_mode() {
        let mut state = state_with_workspaces(&["test"]);
        state.mode = Mode::Resize;
        state.keybinds.resize_mode = (KeyCode::Char('g'), KeyModifiers::empty());
        state.keybinds.resize_mode_label = "g".into();

        handle_resize_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn rename_modal_keyboard_and_mouse_share_actions() {
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

        state.view.sidebar_rect = Rect::new(0, 0, 26, 20);
        state.view.terminal_area = Rect::new(26, 0, 80, 20);
        state.mode = Mode::RenameWorkspace;
        state.name_input = "mouse".into();
        let inner = state.rename_modal_inner().unwrap();
        let (save, _, _) = crate::ui::rename_button_rects(inner);
        let action = modal_action_from_buttons(save.x, save.y, &[(save, ModalAction::Save)]);
        assert_eq!(action, Some(ModalAction::Save));
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
    fn closing_first_auto_tab_resets_remaining_auto_tab_and_next_prompt() {
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

        assert_eq!(state.workspaces[0].tabs[0].display_name(), "1");
        assert!(state.workspaces[0].tabs[0].custom_name.is_none());

        open_new_tab_dialog(&mut state);
        assert_eq!(state.name_input, "2");
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
    fn closing_last_tab_from_context_menu_prompts_to_close_workspace() {
        let mut state = state_with_workspaces(&["test"]);
        let menu = ContextMenuState {
            kind: ContextMenuKind::Tab {
                ws_idx: 0,
                tab_idx: 0,
            },
            x: 0,
            y: 0,
            list: MenuListState::new(2),
        };

        apply_context_menu_action(&mut state, menu, 2);

        assert_eq!(state.mode, Mode::ConfirmClose);
        assert_eq!(state.workspaces.len(), 1);
    }
}
