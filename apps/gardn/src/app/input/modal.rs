use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::{
    app::{
        state::{
            AppState, ContextMenuKind, ContextMenuState, ModalListState, PaneCloseConsequence,
            PaneZoomState,
        },
        ClientViewState,
    },
    input::TerminalKey,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalAction {
    Save,
    Clear,
    Cancel,
    Apply,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModalKeyBinding {
    Enter,
    Esc,
}

impl ModalKeyBinding {
    fn matches(self, key: &KeyEvent) -> bool {
        match self {
            Self::Enter => key.code == KeyCode::Enter,
            Self::Esc => key.code == KeyCode::Esc,
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
    UpdateReady,
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
    if state.update_available.is_some() {
        actions.push(GlobalMenuAction::UpdateReady);
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

pub(crate) fn pane_context_menu_state(
    state: &AppState,
    view: &ClientViewState,
    ws_idx: usize,
    pane_id: crate::layout::PaneId,
    zoom: PaneZoomState,
    can_mutate: bool,
) -> Option<ContextMenuState> {
    let workspace = state.workspaces.get(ws_idx)?;
    let pane = workspace.pane_state(pane_id)?;
    let tab_idx = workspace.find_tab_index_for_pane(pane_id)?;
    let tab = workspace.terminal_tab(tab_idx).ok()?;
    let (x, y) = view
        .computed
        .pane_infos
        .iter()
        .find(|info| info.id == pane_id)
        .map(|info| (info.rect.x.saturating_add(1), info.rect.y.saturating_add(1)))
        .unwrap_or((1, 1));
    let menu = ContextMenuState {
        kind: ContextMenuKind::Pane {
            ws_idx,
            pane_id,
            has_manual_label: state
                .terminals
                .get(&pane.attached_terminal_id)
                .and_then(|terminal| terminal.manual_label.as_ref())
                .is_some(),
            right_click_passthrough: pane.right_click_passthrough,
            zoom,
            close: PaneCloseConsequence::for_tab(tab.layout.pane_count(), workspace.tabs.len()),
            can_mutate,
        },
        x,
        y,
        list: ModalListState::hidden(0),
    };
    (!menu.items().is_empty()).then_some(menu)
}

pub(crate) fn context_menu_state_for_pane(
    state: &AppState,
    view: &ClientViewState,
    ws_idx: usize,
    pane_id: crate::layout::PaneId,
    zoom: PaneZoomState,
    can_mutate: bool,
) -> Option<ContextMenuState> {
    let workspace = state.workspaces.get(ws_idx)?;
    let pane = workspace.pane_state(pane_id)?;
    let is_agent = state
        .terminals
        .get(&pane.attached_terminal_id)
        .is_some_and(|terminal| terminal.is_agent_terminal());
    if is_agent {
        let (x, y) = view
            .computed
            .pane_infos
            .iter()
            .find(|info| info.id == pane_id)
            .map(|info| (info.rect.x.saturating_add(1), info.rect.y.saturating_add(1)))
            .unwrap_or((1, 1));
        return Some(ContextMenuState {
            kind: state.agent_context_menu_kind(&view.agent_follow_up, ws_idx, pane_id)?,
            x,
            y,
            list: ModalListState::hidden(0),
        });
    }
    pane_context_menu_state(state, view, ws_idx, pane_id, zoom, can_mutate)
}

pub(crate) fn request_detach(state: &mut AppState) {
    if state.detach_exits {
        state.should_quit = true;
    } else {
        state.detach_requested = true;
    }
}

pub(crate) fn insert_keybind_help_query_text(
    help: &mut crate::app::state::KeybindHelpState,
    text: &str,
) -> bool {
    let text = if help.search_focused {
        text
    } else if let Some(query) = text.strip_prefix('/') {
        help.search_focused = true;
        query
    } else {
        return false;
    };
    help.query
        .extend(text.chars().filter(|ch| !ch.is_control()));
    help.scroll = 0;
    true
}

pub(super) fn keybind_help_back(help: &mut crate::app::state::KeybindHelpState) -> bool {
    if help.search_focused {
        help.query.clear();
        help.search_focused = false;
        help.scroll = 0;
        return false;
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeybindHelpKeyResult {
    Handled,
    Leave,
}

pub(crate) fn apply_keybind_help_key(
    help: &mut crate::app::state::KeybindHelpState,
    max_scroll: u16,
    key: TerminalKey,
) -> KeybindHelpKeyResult {
    if help.search_focused {
        match key.code {
            KeyCode::Up => scroll_keybind_help(help, max_scroll, -1),
            KeyCode::Down => scroll_keybind_help(help, max_scroll, 1),
            KeyCode::PageUp => {
                scroll_keybind_help(help, max_scroll, -super::MODAL_PAGE_SCROLL_ROWS)
            }
            KeyCode::PageDown => {
                scroll_keybind_help(help, max_scroll, super::MODAL_PAGE_SCROLL_ROWS)
            }
            KeyCode::Home => help.scroll = 0,
            KeyCode::End => help.scroll = max_scroll,
            KeyCode::Backspace => {
                help.query.pop();
                help.scroll = 0;
            }
            KeyCode::Char('u') if key.modifiers == KeyModifiers::CONTROL => {
                help.query.clear();
                help.scroll = 0;
            }
            KeyCode::Esc => {
                let _ = keybind_help_back(help);
            }
            KeyCode::Enter => return KeybindHelpKeyResult::Leave,
            _ => {
                if let Some(character) = keybind_help_text_char(&key) {
                    let _ = insert_keybind_help_query_text(help, &character.to_string());
                }
            }
        }
        return KeybindHelpKeyResult::Handled;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => scroll_keybind_help(help, max_scroll, -1),
        KeyCode::Down | KeyCode::Char('j') => scroll_keybind_help(help, max_scroll, 1),
        KeyCode::PageUp => scroll_keybind_help(help, max_scroll, -super::MODAL_PAGE_SCROLL_ROWS),
        KeyCode::PageDown => scroll_keybind_help(help, max_scroll, super::MODAL_PAGE_SCROLL_ROWS),
        KeyCode::Home => help.scroll = 0,
        KeyCode::End => help.scroll = max_scroll,
        _ if keybind_help_text_char(&key) == Some('/') => {
            help.search_focused = true;
            help.scroll = 0;
        }
        KeyCode::Esc => {
            if keybind_help_back(help) {
                return KeybindHelpKeyResult::Leave;
            }
        }
        KeyCode::Enter => return KeybindHelpKeyResult::Leave,
        _ if keybind_help_text_char(&key) == Some('?') => return KeybindHelpKeyResult::Leave,
        _ => {}
    }
    KeybindHelpKeyResult::Handled
}

fn scroll_keybind_help(
    help: &mut crate::app::state::KeybindHelpState,
    max_scroll: u16,
    delta: i16,
) {
    let current = help.scroll as i16;
    help.scroll = current.saturating_add(delta).clamp(0, max_scroll as i16) as u16;
}

fn keybind_help_text_char(key: &TerminalKey) -> Option<char> {
    if !key.modifiers.difference(KeyModifiers::SHIFT).is_empty() {
        return None;
    }
    if let Some(character) = key.generated_text.as_deref().and_then(|text| {
        let mut characters = text.chars();
        let character = characters.next()?;
        (characters.next().is_none() && !character.is_control()).then_some(character)
    }) {
        return Some(character);
    }
    if let Some(character) = key.shifted_codepoint.and_then(char::from_u32) {
        return Some(character);
    }
    let KeyCode::Char(character) = key.code else {
        return None;
    };
    Some(character)
}

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

#[cfg(test)]
mod tests {
    use ratatui::layout::Direction;

    use super::super::state_with_workspaces;
    use super::*;
    use crate::app::state::Mode;
    use crate::app::App;

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

    #[tokio::test]
    async fn tab_rename_updates_captured_snapshot() {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state = state_with_workspaces(&["test"]);
        app.default_client_view = ClientViewState::from_default_client_state(&app.state);
        app.default_client_view.mode = Mode::RenameTab;
        app.default_client_view.name_input = "logs".into();

        app.handle_key(TerminalKey::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        ))
        .await;

        assert_eq!(app.state.workspaces[0].tabs[0].custom_name(), Some("logs"));
        let snapshot = crate::persist::capture(
            &app.state.groups,
            &app.state.session_namespace_id,
            &app.state.remote_termination_tombstones,
            &app.state.workspaces,
            &app.state.terminals,
            &app.terminal_runtimes,
            &app.default_client_view,
            &app.default_client_view.agent_follow_up,
        );
        assert_eq!(
            snapshot.workspaces[0].tabs[0]
                .as_terminal()
                .and_then(|tab| tab.custom_name.as_deref()),
            Some("logs")
        );
    }

    #[test]
    fn watching_client_agent_pane_menu_only_exposes_view_local_zoom() {
        let mut state = state_with_workspaces(&["main"]);
        let pane_id = state.workspaces[0].test_split(Direction::Horizontal);
        state.ensure_test_terminals();
        let terminal_id = state.workspaces[0]
            .pane_state(pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_agent_name("codex".into());
        let view = ClientViewState::from_default_client_state(&state);

        let menu =
            pane_context_menu_state(&state, &view, 0, pane_id, PaneZoomState::Available, false)
                .expect("watching client has a zoom action");
        assert_eq!(menu.items(), &["zoom"]);

        assert!(pane_context_menu_state(
            &state,
            &view,
            0,
            pane_id,
            PaneZoomState::Unavailable,
            false,
        )
        .is_none());
    }
}
