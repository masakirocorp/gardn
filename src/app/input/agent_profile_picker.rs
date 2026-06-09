use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::app::{
    agent_profile_picker::{agent_profile_picker_filtered_entries, workspace_agent_profile_ids},
    state::{AppState, Mode},
    App,
};

use super::{modal::modal_action_from_buttons, modal::ModalAction, ScrollbarClickTarget};

pub(super) fn open_new_agent_picker_for_workspace(state: &mut AppState, ws_idx: usize) {
    let profile_ids = workspace_agent_profile_ids(state, ws_idx).collect::<Vec<_>>();
    match profile_ids.as_slice() {
        [] => {}
        [profile_id] => {
            state.request_agent_profile_tab = Some((ws_idx, profile_id.clone()));
            state.return_to_active_workspace_mode();
        }
        _ => {
            state.selected = ws_idx;
            state.active = Some(ws_idx);
            state.agent_profile_picker.ws_idx = ws_idx;
            state.agent_profile_picker.query.clear();
            state.agent_profile_picker.selected = 0;
            state.agent_profile_picker.scroll = 0;
            state.mode = Mode::AgentProfilePicker;
        }
    }
}

pub(super) fn close_agent_profile_picker(state: &mut AppState) {
    state.return_to_active_workspace_mode();
}

impl App {
    pub(crate) fn handle_agent_profile_picker_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => close_agent_profile_picker(&mut self.state),
            KeyCode::Enter => self.launch_selected_agent_profile(),
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_selected_agent_profile_favorite();
            }
            KeyCode::Up => {
                move_agent_profile_picker_selection(&mut self.state, false);
            }
            KeyCode::Down => {
                move_agent_profile_picker_selection(&mut self.state, true);
            }
            KeyCode::PageUp => {
                scroll_agent_profile_picker_rows(&mut self.state, -super::MODAL_PAGE_SCROLL_ROWS)
            }
            KeyCode::PageDown => {
                scroll_agent_profile_picker_rows(&mut self.state, super::MODAL_PAGE_SCROLL_ROWS)
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_agent_profile_picker_selection(&mut self.state, false);
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_agent_profile_picker_selection(&mut self.state, true);
            }
            KeyCode::Backspace => {
                self.state.agent_profile_picker.query.pop();
                clamp_agent_profile_picker_selection(&mut self.state);
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.state.agent_profile_picker.query.push(c);
                clamp_agent_profile_picker_selection(&mut self.state);
            }
            _ => {}
        }
    }

    fn launch_selected_agent_profile(&mut self) {
        let entries = agent_profile_picker_filtered_entries(&self.state);
        let Some(entry) = entries
            .get(self.state.agent_profile_picker.selected)
            .cloned()
        else {
            return;
        };
        let ws_idx = self.state.agent_profile_picker.ws_idx;
        self.state.request_agent_profile_tab = Some((ws_idx, entry.profile_id));
        self.state.return_to_active_workspace_mode();
    }

    fn toggle_selected_agent_profile_favorite(&mut self) {
        let entries = agent_profile_picker_filtered_entries(&self.state);
        let Some(entry) = entries.get(self.state.agent_profile_picker.selected) else {
            return;
        };
        if let Some(group_idx) = self
            .state
            .workspaces
            .get(self.state.agent_profile_picker.ws_idx)
            .and_then(|workspace| self.state.group_index_by_id(&workspace.group_id))
        {
            self.state
                .toggle_group_agent_profile_favorite(group_idx, &entry.profile_id);
        }
    }
}

pub(super) fn agent_profile_picker_action_button_at(
    state: &AppState,
    col: u16,
    row: u16,
) -> Option<ModalAction> {
    let inner = agent_profile_picker_inner_rect(state)?;
    let (start, close) = crate::ui::agent_profile_picker_button_rects(inner);
    modal_action_from_buttons(
        col,
        row,
        &[(start, ModalAction::Apply), (close, ModalAction::Close)],
    )
}

pub(super) fn agent_profile_picker_contains_point(state: &AppState, col: u16, row: u16) -> bool {
    agent_profile_picker_popup_rect(state).is_some_and(|popup| {
        col >= popup.x
            && col < popup.x + popup.width
            && row >= popup.y
            && row < popup.y + popup.height
    })
}

pub(super) fn hover_agent_profile_picker_selection(state: &mut AppState, col: u16, row: u16) {
    let Some((list_area, rows, viewport)) = agent_profile_picker_viewport(state) else {
        return;
    };
    let Some(row_idx) = viewport.hit_visual_row(list_area, col, row) else {
        return;
    };

    if let Some(Some(entry_idx)) = rows.get(row_idx) {
        state.agent_profile_picker.selected = *entry_idx;
    }
}

pub(super) fn scroll_agent_profile_picker_rows(state: &mut AppState, delta: i16) {
    let max_scroll = agent_profile_picker_max_scroll(state);
    let next = if delta.is_negative() {
        state
            .agent_profile_picker
            .scroll
            .saturating_sub(delta.unsigned_abs() as usize)
    } else {
        state
            .agent_profile_picker
            .scroll
            .saturating_add(delta as usize)
            .min(max_scroll)
    };
    state.agent_profile_picker.scroll = next.min(max_scroll);
}

pub(super) fn agent_profile_picker_scrollbar_target_at(
    state: &AppState,
    col: u16,
    row: u16,
) -> Option<ScrollbarClickTarget> {
    let metrics = agent_profile_picker_scroll_metrics(state)?;
    let track = agent_profile_picker_scrollbar_track(state)?;
    if !(col >= track.x
        && col < track.x + track.width
        && row >= track.y
        && row < track.y + track.height)
    {
        return None;
    }
    if let Some(grab_row_offset) = crate::ui::scrollbar_thumb_grab_offset(metrics, track, row) {
        Some(ScrollbarClickTarget::Thumb { grab_row_offset })
    } else {
        Some(ScrollbarClickTarget::Track {
            offset_from_bottom: crate::ui::scrollbar_offset_from_row(metrics, track, row),
        })
    }
}

pub(super) fn agent_profile_picker_offset_for_drag_row(
    state: &AppState,
    row: u16,
    grab_row_offset: u16,
) -> Option<usize> {
    let metrics = agent_profile_picker_scroll_metrics(state)?;
    let track = agent_profile_picker_scrollbar_track(state)?;
    Some(crate::ui::scrollbar_offset_from_drag_row(
        metrics,
        track,
        row,
        grab_row_offset,
    ))
}

pub(super) fn set_agent_profile_picker_offset_from_bottom(
    state: &mut AppState,
    offset_from_bottom: usize,
) {
    let Some((_, _, viewport)) = agent_profile_picker_viewport(state) else {
        state.agent_profile_picker.scroll = 0;
        return;
    };
    state.agent_profile_picker.scroll = viewport.scroll_from_offset_from_bottom(offset_from_bottom);
}

fn clamp_agent_profile_picker_selection(state: &mut AppState) {
    let count = agent_profile_picker_filtered_entries(state).len();
    if count == 0 {
        state.agent_profile_picker.selected = 0;
        state.agent_profile_picker.scroll = 0;
        return;
    }

    state.agent_profile_picker.selected = state.agent_profile_picker.selected.min(count - 1);
    ensure_agent_profile_picker_selection_visible(state);
}

fn move_agent_profile_picker_selection(state: &mut AppState, down: bool) -> bool {
    let count = agent_profile_picker_filtered_entries(state).len();
    if count == 0 {
        state.agent_profile_picker.selected = 0;
        state.agent_profile_picker.scroll = 0;
        return false;
    }
    let next = if down {
        (state.agent_profile_picker.selected + 1).min(count - 1)
    } else {
        state.agent_profile_picker.selected.saturating_sub(1)
    };
    let changed = next != state.agent_profile_picker.selected;
    state.agent_profile_picker.selected = next;
    ensure_agent_profile_picker_selection_visible(state);
    changed
}

fn ensure_agent_profile_picker_selection_visible(state: &mut AppState) {
    let Some((_, rows, viewport)) = agent_profile_picker_viewport(state) else {
        state.agent_profile_picker.scroll = 0;
        return;
    };

    let Some(selected_row) = rows
        .iter()
        .position(|row| *row == Some(state.agent_profile_picker.selected))
    else {
        state.agent_profile_picker.scroll = viewport.scroll();
        return;
    };
    let first_section_row = selected_row
        .checked_sub(1)
        .filter(|idx| rows.get(*idx).is_some_and(Option::is_none));
    state.agent_profile_picker.scroll = viewport.ensure_visible(selected_row, first_section_row);
}

fn agent_profile_picker_viewport(
    state: &AppState,
) -> Option<(Rect, Vec<Option<usize>>, crate::ui::ModalListViewport)> {
    let (list_area, rows) = agent_profile_picker_rows_for_input(state)?;
    let viewport = crate::ui::ModalListViewport::new(
        rows.len(),
        list_area.height as usize,
        state.agent_profile_picker.scroll,
    );
    Some((list_area, rows, viewport))
}

fn agent_profile_picker_rows_for_input(state: &AppState) -> Option<(Rect, Vec<Option<usize>>)> {
    let list_area = agent_profile_picker_list_area(state)?;
    if list_area.height == 0 {
        return None;
    }

    let entries = agent_profile_picker_filtered_entries(state);
    if entries.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    let mut last_section = None;
    for (idx, entry) in entries.iter().enumerate() {
        if last_section != Some(entry.section) {
            if last_section.is_some() {
                rows.push(None);
            }
            rows.push(None);
            last_section = Some(entry.section);
        }
        rows.push(Some(idx));
    }

    Some((list_area, rows))
}

fn agent_profile_picker_list_area(state: &AppState) -> Option<Rect> {
    crate::ui::agent_profile_picker_list_area(agent_profile_picker_screen_rect(state))
}

fn agent_profile_picker_inner_rect(state: &AppState) -> Option<Rect> {
    crate::ui::agent_profile_picker_inner_rect(agent_profile_picker_screen_rect(state))
}

fn agent_profile_picker_popup_rect(state: &AppState) -> Option<Rect> {
    crate::ui::agent_profile_picker_popup_rect(agent_profile_picker_screen_rect(state))
}

fn agent_profile_picker_screen_rect(state: &AppState) -> Rect {
    let sidebar = state.view.sidebar_rect;
    let terminal = state.view.terminal_area;
    let right_sidebar = state.view.right_sidebar_rect;
    let x = sidebar.x.min(terminal.x).min(right_sidebar.x);
    let y = sidebar.y.min(terminal.y).min(right_sidebar.y);
    let right = (sidebar.x + sidebar.width)
        .max(terminal.x + terminal.width)
        .max(right_sidebar.x + right_sidebar.width);
    let bottom = (sidebar.y + sidebar.height)
        .max(terminal.y + terminal.height)
        .max(right_sidebar.y + right_sidebar.height);
    Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}

fn agent_profile_picker_scroll_metrics(state: &AppState) -> Option<crate::pane::ScrollMetrics> {
    let (_, _, viewport) = agent_profile_picker_viewport(state)?;
    Some(viewport.metrics())
}

fn agent_profile_picker_max_scroll(state: &AppState) -> usize {
    agent_profile_picker_viewport(state)
        .map(|(_, _, viewport)| viewport.max_scroll())
        .unwrap_or(0)
}

fn agent_profile_picker_scrollbar_track(state: &AppState) -> Option<Rect> {
    let (list_area, _, viewport) = agent_profile_picker_viewport(state)?;
    viewport.scroll_area(list_area).track
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn app_with_space() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app
    }

    #[test]
    fn picker_ctrl_f_toggles_target_group_favorite() {
        let mut app = app_with_space();
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        app.handle_agent_profile_picker_key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        ));

        assert_eq!(
            app.state.groups[app.state.active_group].favorite_agent_profile_ids,
            vec!["system:pi".to_string()]
        );
        assert!(app.state.session_dirty);
    }

    #[test]
    fn picker_uses_workspace_group_favorites_when_active_group_differs() {
        let mut app = app_with_space();
        let group_idx = app.state.create_group("side".to_string());
        let group_id = app.state.groups[group_idx].id.clone();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("side"));
        app.state.workspaces[1].group_id = group_id;
        app.state.groups[group_idx]
            .favorite_agent_profile_ids
            .push("system:codex".to_string());
        app.state.active_group = 0;

        open_new_agent_picker_for_workspace(&mut app.state, 1);
        let entries = agent_profile_picker_filtered_entries(&app.state);

        assert_eq!(entries[0].section, "favorites");
        assert_eq!(entries[0].profile_id, "system:codex");
    }

    #[test]
    fn picker_enter_enqueues_profile_launch() {
        let mut app = app_with_space();
        app.state.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:shell-builtin".to_string()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "shell-builtin".to_string(),
                    name: "shell builtin".to_string(),
                    kind: crate::agent_profiles::AgentKind::Omp,
                    command: "cd .".to_string(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((0, "user:shell-builtin".to_string()))
        );
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
    }
}
