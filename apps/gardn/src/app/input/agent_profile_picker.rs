use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::app::{
    agent_profile_picker::{
        agent_profile_picker_entries_for_workspace, agent_profile_picker_filtered_entries,
        agent_profile_picker_filtered_entries_for_picker, AGENT_PROFILE_PICKER_TABS,
    },
    state::{AppState, Mode},
    view_state::ClientViewState,
    App,
};

use super::{modal::modal_action_from_buttons, modal::ModalAction, ScrollbarClickTarget};

pub(crate) fn open_new_agent_picker_for_workspace(state: &mut AppState, ws_idx: usize) {
    if let Some(default_profile_id) = state
        .workspaces
        .get(ws_idx)
        .and_then(|workspace| state.group_index_by_id(&workspace.group_id))
        .and_then(|group_idx| state.groups.get(group_idx))
        .and_then(|group| group.default_agent_profile_id.as_ref())
        .filter(|profile_id| {
            state
                .agent_profiles
                .get(profile_id)
                .is_some_and(|profile| state.agent_profile_launchable(profile))
        })
        .cloned()
    {
        state.request_agent_profile_tab = Some((ws_idx, default_profile_id));
        state.return_to_active_workspace_mode();
        return;
    }
    let entries = agent_profile_picker_entries_for_workspace(state, ws_idx);
    match entries.as_slice() {
        [] => {}
        [entry]
            if state
                .agent_profiles
                .get(&entry.profile_id)
                .is_some_and(|profile| state.agent_profile_launchable(profile)) =>
        {
            state.request_agent_profile_tab = Some((ws_idx, entry.profile_id.clone()));
            state.return_to_active_workspace_mode();
        }
        _ => {
            state.selected = ws_idx;
            state.agent_profile_picker.kind_filter = None;
            state.agent_profile_picker.ws_idx = ws_idx;
            state.agent_profile_picker.query.clear();
            state.agent_profile_picker.list.select(0);
            state.agent_profile_picker.list.hide();
            state.agent_profile_picker.scroll = 0;
            state.mode = Mode::AgentProfilePicker;
        }
    }
}

pub(crate) fn close_agent_profile_picker(state: &mut AppState) {
    state.return_to_active_workspace_mode();
}

pub(crate) fn handle_agent_profile_picker_key_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    key: KeyEvent,
) {
    if let Some(index) = agent_profile_picker_favorite_shortcut_index(key) {
        if view.can_mutate_tab() {
            launch_favorite_agent_profile_by_shortcut_for_view(state, view, index);
        } else {
            view.return_to_active_workspace_mode();
        }
        return;
    }

    match key.code {
        KeyCode::Esc => view.return_to_active_workspace_mode(),
        KeyCode::Enter if view.can_mutate_tab() => {
            launch_selected_agent_profile_for_view(state, view)
        }
        KeyCode::Enter => view.return_to_active_workspace_mode(),
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            toggle_selected_agent_profile_favorite_for_view(state, view);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            toggle_selected_agent_profile_default_for_view(state, view);
        }
        KeyCode::Up => move_agent_profile_picker_selection_for_view(state, view, false),
        KeyCode::Down => move_agent_profile_picker_selection_for_view(state, view, true),
        KeyCode::PageUp => {
            scroll_agent_profile_picker_rows_for_view(state, view, -super::MODAL_PAGE_SCROLL_ROWS)
        }
        KeyCode::PageDown => {
            scroll_agent_profile_picker_rows_for_view(state, view, super::MODAL_PAGE_SCROLL_ROWS)
        }
        KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => {
            move_agent_profile_picker_tab_for_view(state, view, false);
        }
        KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
            move_agent_profile_picker_tab_for_view(state, view, true);
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_agent_profile_picker_selection_for_view(state, view, false);
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_agent_profile_picker_selection_for_view(state, view, true);
        }
        KeyCode::Backspace => {
            view.agent_profile_picker.query.pop();
            clamp_agent_profile_picker_selection_for_view(state, view);
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            view.agent_profile_picker.query.push(c);
            clamp_agent_profile_picker_selection_for_view(state, view);
        }
        _ => {}
    }
}

impl App {
    pub(crate) fn handle_agent_profile_picker_key(&mut self, key: KeyEvent) {
        if let Some(index) = agent_profile_picker_favorite_shortcut_index(key) {
            self.launch_favorite_agent_profile_by_shortcut(index);
            return;
        }

        match key.code {
            KeyCode::Esc => close_agent_profile_picker(&mut self.state),
            KeyCode::Enter => self.launch_selected_agent_profile(),
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_selected_agent_profile_favorite();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_selected_agent_profile_default();
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
            KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => {
                move_agent_profile_picker_tab(&mut self.state, false);
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
                move_agent_profile_picker_tab(&mut self.state, true);
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
            .get(self.state.agent_profile_picker.list.selected)
            .cloned()
        else {
            return;
        };
        let ws_idx = self.state.agent_profile_picker.ws_idx;
        self.state.request_agent_profile_tab = Some((ws_idx, entry.profile_id));
        self.state.return_to_active_workspace_mode();
    }

    fn launch_favorite_agent_profile_by_shortcut(&mut self, favorite_idx: usize) {
        let Some(entry) = agent_profile_picker_filtered_entries(&self.state)
            .into_iter()
            .filter(|entry| entry.section == "favorites")
            .nth(favorite_idx)
        else {
            return;
        };
        let ws_idx = self.state.agent_profile_picker.ws_idx;
        self.state.request_agent_profile_tab = Some((ws_idx, entry.profile_id));
        self.state.return_to_active_workspace_mode();
    }

    fn toggle_selected_agent_profile_favorite(&mut self) {
        let entries = agent_profile_picker_filtered_entries(&self.state);
        let Some(entry) = entries.get(self.state.agent_profile_picker.list.selected) else {
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

    fn toggle_selected_agent_profile_default(&mut self) {
        let entries = agent_profile_picker_filtered_entries(&self.state);
        let Some(entry) = entries.get(self.state.agent_profile_picker.list.selected) else {
            return;
        };
        if let Some(group_idx) = self
            .state
            .workspaces
            .get(self.state.agent_profile_picker.ws_idx)
            .and_then(|workspace| self.state.group_index_by_id(&workspace.group_id))
        {
            self.state
                .toggle_group_default_agent_profile(group_idx, &entry.profile_id);
        }
    }
}

fn agent_profile_picker_favorite_shortcut_index(key: KeyEvent) -> Option<usize> {
    if !key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }

    match key.code {
        KeyCode::Char(c @ '1'..='9') => Some((c as usize) - ('1' as usize)),
        _ => None,
    }
}

fn launch_agent_profile_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    profile_id: String,
) {
    let ws_idx = view.agent_profile_picker.ws_idx;
    state.request_agent_profile_tab = Some((ws_idx, profile_id));
    if let Some(ws) = state.workspaces.get(ws_idx) {
        view.pending_active_tabs
            .insert(ws.id.clone(), ws.tabs.len());
    }
    view.return_to_active_workspace_mode();
}

fn launch_selected_agent_profile_for_view(state: &mut AppState, view: &mut ClientViewState) {
    let Some(entry) =
        agent_profile_picker_filtered_entries_for_picker(state, &view.agent_profile_picker)
            .get(view.agent_profile_picker.list.selected)
            .cloned()
    else {
        return;
    };
    launch_agent_profile_for_view(state, view, entry.profile_id);
}

fn launch_favorite_agent_profile_by_shortcut_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    favorite_idx: usize,
) {
    let Some(entry) =
        agent_profile_picker_filtered_entries_for_picker(state, &view.agent_profile_picker)
            .into_iter()
            .filter(|entry| entry.section == "favorites")
            .nth(favorite_idx)
    else {
        return;
    };
    launch_agent_profile_for_view(state, view, entry.profile_id);
}

fn selected_agent_profile_group_for_view(
    state: &AppState,
    view: &ClientViewState,
) -> Option<(usize, String)> {
    let entry = agent_profile_picker_filtered_entries_for_picker(state, &view.agent_profile_picker)
        .get(view.agent_profile_picker.list.selected)?
        .clone();
    let group_idx = state
        .workspaces
        .get(view.agent_profile_picker.ws_idx)
        .and_then(|workspace| state.group_index_by_id(&workspace.group_id))?;
    Some((group_idx, entry.profile_id))
}

fn toggle_selected_agent_profile_favorite_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
) {
    let Some((group_idx, profile_id)) = selected_agent_profile_group_for_view(state, view) else {
        return;
    };
    state.toggle_group_agent_profile_favorite(group_idx, &profile_id);
    clamp_agent_profile_picker_selection_for_view(state, view);
}

fn toggle_selected_agent_profile_default_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
) {
    let Some((group_idx, profile_id)) = selected_agent_profile_group_for_view(state, view) else {
        return;
    };
    state.toggle_group_default_agent_profile(group_idx, &profile_id);
    clamp_agent_profile_picker_selection_for_view(state, view);
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

pub(crate) fn agent_profile_picker_action_button_at_for_view(
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<ModalAction> {
    let inner = crate::ui::agent_profile_picker_inner_rect(view.screen_rect())?;
    let (start, close) = crate::ui::agent_profile_picker_button_rects(inner);
    modal_action_from_buttons(
        col,
        row,
        &[(start, ModalAction::Apply), (close, ModalAction::Close)],
    )
}

pub(crate) fn select_agent_profile_picker_tab_at_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    col: u16,
    row: u16,
) -> bool {
    let Some(tab_row) = agent_profile_picker_tab_row_for_view(view) else {
        return false;
    };
    if row != tab_row.y {
        return false;
    }

    let tab_idx =
        crate::ui::agent_profile_picker_tab_chevron_at(state, tab_row, col).or_else(|| {
            crate::ui::agent_profile_picker_tab_hit_areas(state, tab_row)
                .into_iter()
                .find_map(|(idx, rect)| {
                    (col >= rect.x && col < rect.x.saturating_add(rect.width)).then_some(idx)
                })
        });

    let Some(tab_idx) = tab_idx else {
        return false;
    };

    view.agent_profile_picker.kind_filter = AGENT_PROFILE_PICKER_TABS[tab_idx];
    view.agent_profile_picker.list.select(0);
    view.agent_profile_picker.scroll = 0;
    clamp_agent_profile_picker_selection_for_view(state, view);
    true
}

pub(crate) fn agent_profile_picker_contains_point_for_view(
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> bool {
    crate::ui::agent_profile_picker_popup_rect(view.screen_rect()).is_some_and(|popup| {
        col >= popup.x
            && col < popup.x + popup.width
            && row >= popup.y
            && row < popup.y + popup.height
    })
}

pub(crate) fn select_agent_profile_picker_tab_at(state: &mut AppState, col: u16, row: u16) -> bool {
    let Some(tab_row) = agent_profile_picker_tab_row(state) else {
        return false;
    };
    if row != tab_row.y {
        return false;
    }

    let tab_idx =
        crate::ui::agent_profile_picker_tab_chevron_at(state, tab_row, col).or_else(|| {
            crate::ui::agent_profile_picker_tab_hit_areas(state, tab_row)
                .into_iter()
                .find_map(|(idx, rect)| {
                    (col >= rect.x && col < rect.x.saturating_add(rect.width)).then_some(idx)
                })
        });

    let Some(tab_idx) = tab_idx else {
        return false;
    };

    state.agent_profile_picker.kind_filter = AGENT_PROFILE_PICKER_TABS[tab_idx];
    state.agent_profile_picker.list.select(0);
    state.agent_profile_picker.scroll = 0;
    clamp_agent_profile_picker_selection(state);
    true
}

pub(crate) fn agent_profile_picker_contains_point(state: &AppState, col: u16, row: u16) -> bool {
    agent_profile_picker_popup_rect(state).is_some_and(|popup| {
        col >= popup.x
            && col < popup.x + popup.width
            && row >= popup.y
            && row < popup.y + popup.height
    })
}

fn agent_profile_picker_selection_at(state: &AppState, col: u16, row: u16) -> Option<usize> {
    let (list, rows) = agent_profile_picker_viewport(state)?;
    let row_idx = list.hit_visual_row(col, row)?;
    rows.get(row_idx).and_then(|row| row.as_ref().copied())
}

pub(crate) fn hover_agent_profile_picker_selection(state: &mut AppState, col: u16, row: u16) {
    let hovered = agent_profile_picker_selection_at(state, col, row);
    state.agent_profile_picker.list.hover(hovered);
}

pub(crate) fn select_agent_profile_picker_selection(
    state: &mut AppState,
    col: u16,
    row: u16,
) -> bool {
    let Some(selected) = agent_profile_picker_selection_at(state, col, row) else {
        return false;
    };
    state.agent_profile_picker.list.select(selected);
    ensure_agent_profile_picker_selection_visible(state);
    true
}

fn agent_profile_picker_selection_at_for_view(
    state: &AppState,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<usize> {
    let (list, rows) = agent_profile_picker_viewport_for_view(state, view)?;
    let row_idx = list.hit_visual_row(col, row)?;
    rows.get(row_idx).and_then(|entry| entry.as_ref().copied())
}

pub(crate) fn hover_agent_profile_picker_selection_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    col: u16,
    row: u16,
) {
    let hovered = agent_profile_picker_selection_at_for_view(state, view, col, row);
    view.agent_profile_picker.list.hover(hovered);
}

pub(crate) fn select_agent_profile_picker_selection_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    col: u16,
    row: u16,
) -> bool {
    let Some(selected) = agent_profile_picker_selection_at_for_view(state, view, col, row) else {
        return false;
    };
    view.agent_profile_picker.list.select(selected);
    ensure_agent_profile_picker_selection_visible_for_view(state, view);
    true
}

pub(crate) fn scroll_agent_profile_picker_rows(state: &mut AppState, delta: i16) {
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

pub(crate) fn scroll_agent_profile_picker_rows_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    delta: i16,
) {
    let max_scroll = agent_profile_picker_max_scroll_for_view(state, view);
    let next = if delta.is_negative() {
        view.agent_profile_picker
            .scroll
            .saturating_sub(delta.unsigned_abs() as usize)
    } else {
        view.agent_profile_picker
            .scroll
            .saturating_add(delta as usize)
            .min(max_scroll)
    };
    view.agent_profile_picker.scroll = next.min(max_scroll);
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
    let Some((list, _)) = agent_profile_picker_viewport(state) else {
        state.agent_profile_picker.scroll = 0;
        return;
    };
    state.agent_profile_picker.scroll = list
        .viewport
        .scroll_from_offset_from_bottom(offset_from_bottom);
}

fn clamp_agent_profile_picker_selection(state: &mut AppState) {
    let count = agent_profile_picker_filtered_entries(state).len();
    if count == 0 {
        state.agent_profile_picker.list.select(0);
        state.agent_profile_picker.scroll = 0;
        return;
    }

    let selected = state.agent_profile_picker.list.selected.min(count - 1);
    state.agent_profile_picker.list.select(selected);
    ensure_agent_profile_picker_selection_visible(state);
}

fn move_agent_profile_picker_selection(state: &mut AppState, down: bool) -> bool {
    let count = agent_profile_picker_filtered_entries(state).len();
    if count == 0 {
        state.agent_profile_picker.list.select(0);
        state.agent_profile_picker.scroll = 0;
        return false;
    }
    let previous = state.agent_profile_picker.list.selected;
    if down {
        state.agent_profile_picker.list.move_next(count);
    } else {
        state.agent_profile_picker.list.move_prev();
    }
    ensure_agent_profile_picker_selection_visible(state);
    state.agent_profile_picker.list.selected != previous
}

fn move_agent_profile_picker_tab(state: &mut AppState, forward: bool) {
    let current = state.agent_profile_picker.kind_filter;
    let current_idx = AGENT_PROFILE_PICKER_TABS
        .iter()
        .position(|tab| *tab == current)
        .unwrap_or(0);
    let next_idx = if forward {
        (current_idx + 1) % AGENT_PROFILE_PICKER_TABS.len()
    } else {
        current_idx
            .checked_sub(1)
            .unwrap_or(AGENT_PROFILE_PICKER_TABS.len() - 1)
    };

    state.agent_profile_picker.kind_filter = AGENT_PROFILE_PICKER_TABS[next_idx];
    state.agent_profile_picker.list.select(0);
    state.agent_profile_picker.scroll = 0;
    clamp_agent_profile_picker_selection(state);
}

fn move_agent_profile_picker_selection_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    down: bool,
) {
    let count =
        agent_profile_picker_filtered_entries_for_picker(state, &view.agent_profile_picker).len();
    if count == 0 {
        view.agent_profile_picker.list.select(0);
        view.agent_profile_picker.scroll = 0;
        return;
    }
    if down {
        view.agent_profile_picker.list.move_next(count);
    } else {
        view.agent_profile_picker.list.move_prev();
    }
    ensure_agent_profile_picker_selection_visible_for_view(state, view);
}

fn move_agent_profile_picker_tab_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    forward: bool,
) {
    let current = view.agent_profile_picker.kind_filter;
    let current_idx = AGENT_PROFILE_PICKER_TABS
        .iter()
        .position(|tab| *tab == current)
        .unwrap_or(0);
    let next_idx = if forward {
        (current_idx + 1) % AGENT_PROFILE_PICKER_TABS.len()
    } else {
        current_idx
            .checked_sub(1)
            .unwrap_or(AGENT_PROFILE_PICKER_TABS.len() - 1)
    };
    view.agent_profile_picker.kind_filter = AGENT_PROFILE_PICKER_TABS[next_idx];
    view.agent_profile_picker.list.select(0);
    view.agent_profile_picker.scroll = 0;
    clamp_agent_profile_picker_selection_for_view(state, view);
}

fn agent_profile_picker_viewport_for_view(
    state: &AppState,
    view: &ClientViewState,
) -> Option<(crate::ui::ModalListGeometry, Vec<Option<usize>>)> {
    let rows = agent_profile_picker_rows_for_view(state, view)?;
    let list = crate::ui::agent_profile_picker_list_geometry(
        view.screen_rect(),
        rows.len(),
        view.agent_profile_picker.scroll,
    )?;
    Some((list, rows))
}

fn agent_profile_picker_rows_for_view(
    state: &AppState,
    view: &ClientViewState,
) -> Option<Vec<Option<usize>>> {
    let entries =
        agent_profile_picker_filtered_entries_for_picker(state, &view.agent_profile_picker);
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

    Some(rows)
}

fn agent_profile_picker_max_scroll_for_view(state: &AppState, view: &ClientViewState) -> usize {
    agent_profile_picker_viewport_for_view(state, view)
        .map(|(list, _)| list.viewport.max_scroll())
        .unwrap_or(0)
}

fn clamp_agent_profile_picker_selection_for_view(state: &AppState, view: &mut ClientViewState) {
    let count =
        agent_profile_picker_filtered_entries_for_picker(state, &view.agent_profile_picker).len();
    if count == 0 {
        view.agent_profile_picker.list.select(0);
        view.agent_profile_picker.scroll = 0;
    } else {
        let selected = view.agent_profile_picker.list.selected.min(count - 1);
        view.agent_profile_picker.list.select(selected);
        ensure_agent_profile_picker_selection_visible_for_view(state, view);
    }
}

fn ensure_agent_profile_picker_selection_visible_for_view(
    state: &AppState,
    view: &mut ClientViewState,
) {
    let Some((list, rows)) = agent_profile_picker_viewport_for_view(state, view) else {
        view.agent_profile_picker.scroll = 0;
        return;
    };

    let Some(selected_row) = rows
        .iter()
        .position(|row| *row == Some(view.agent_profile_picker.list.selected))
    else {
        view.agent_profile_picker.scroll = list.viewport.scroll();
        return;
    };
    let first_section_row = selected_row
        .checked_sub(1)
        .filter(|idx| rows.get(*idx).is_some_and(Option::is_none));
    view.agent_profile_picker.scroll = list
        .viewport
        .ensure_visible(selected_row, first_section_row);
}

fn ensure_agent_profile_picker_selection_visible(state: &mut AppState) {
    let Some((list, rows)) = agent_profile_picker_viewport(state) else {
        state.agent_profile_picker.scroll = 0;
        return;
    };

    let Some(selected_row) = rows
        .iter()
        .position(|row| *row == Some(state.agent_profile_picker.list.selected))
    else {
        state.agent_profile_picker.scroll = list.viewport.scroll();
        return;
    };
    let first_section_row = selected_row
        .checked_sub(1)
        .filter(|idx| rows.get(*idx).is_some_and(Option::is_none));
    state.agent_profile_picker.scroll = list
        .viewport
        .ensure_visible(selected_row, first_section_row);
}

fn agent_profile_picker_viewport(
    state: &AppState,
) -> Option<(crate::ui::ModalListGeometry, Vec<Option<usize>>)> {
    let rows = agent_profile_picker_rows_for_input(state)?;
    let list = crate::ui::agent_profile_picker_list_geometry(
        state.screen_rect(),
        rows.len(),
        state.agent_profile_picker.scroll,
    )?;
    Some((list, rows))
}

fn agent_profile_picker_rows_for_input(state: &AppState) -> Option<Vec<Option<usize>>> {
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

    Some(rows)
}

fn agent_profile_picker_tab_row(state: &AppState) -> Option<Rect> {
    let inner = agent_profile_picker_inner_rect(state)?;
    let label_width = 7;
    Some(Rect::new(
        inner.x.saturating_add(label_width),
        inner.y + 2,
        inner.width.saturating_sub(label_width),
        1,
    ))
}

fn agent_profile_picker_tab_row_for_view(view: &ClientViewState) -> Option<Rect> {
    let inner = crate::ui::agent_profile_picker_inner_rect(view.screen_rect())?;
    let label_width = 7;
    Some(Rect::new(
        inner.x.saturating_add(label_width),
        inner.y + 2,
        inner.width.saturating_sub(label_width),
        1,
    ))
}

fn agent_profile_picker_inner_rect(state: &AppState) -> Option<Rect> {
    crate::ui::agent_profile_picker_inner_rect(state.screen_rect())
}

fn agent_profile_picker_popup_rect(state: &AppState) -> Option<Rect> {
    crate::ui::agent_profile_picker_popup_rect(state.screen_rect())
}

fn agent_profile_picker_scroll_metrics(state: &AppState) -> Option<crate::pane::ScrollMetrics> {
    let (list, _) = agent_profile_picker_viewport(state)?;
    Some(list.metrics())
}

fn agent_profile_picker_max_scroll(state: &AppState) -> usize {
    agent_profile_picker_viewport(state)
        .map(|(list, _)| list.viewport.max_scroll())
        .unwrap_or(0)
}

fn agent_profile_picker_scrollbar_track(state: &AppState) -> Option<Rect> {
    let (list, _) = agent_profile_picker_viewport(state)?;
    list.scroll_area.track
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use ratatui::layout::Rect;

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
        install_all_integrations(&mut app);
        app
    }

    fn current_integration_for(
        kind: crate::agent_profiles::AgentKind,
    ) -> crate::integration::IntegrationRecommendation {
        crate::integration::IntegrationRecommendation {
            target: kind
                .integration_target()
                .expect("system kind has integration"),
            label: kind.as_str(),
            command: kind.system_command(),
            available: true,
            path: std::path::PathBuf::from("/tmp/gardn-test-integration"),
            state: crate::integration::IntegrationStatusKind::Current,
        }
    }

    fn install_all_integrations(app: &mut App) {
        app.state.integration_recommendations = crate::agent_profiles::AgentKind::SYSTEM
            .into_iter()
            .map(current_integration_for)
            .collect();
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
            vec!["system:claude".to_string()]
        );
        assert!(app.state.session_dirty);
    }

    #[test]
    fn picker_alt_number_launches_favorite_by_group_order() {
        let mut app = app_with_space();
        app.state.groups[app.state.active_group].favorite_agent_profile_ids =
            vec!["system:pi".to_string(), "system:omp".to_string()];
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::ALT));

        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((0, "system:pi".to_string()))
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn new_agent_uses_group_default_profile_without_picker() {
        let mut app = app_with_space();
        app.state.groups[app.state.active_group].default_agent_profile_id =
            Some("system:omp".to_string());

        open_new_agent_picker_for_workspace(&mut app.state, 0);

        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((0, "system:omp".to_string()))
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn new_agent_ignores_default_profile_without_installed_integration() {
        let _lock = crate::integration::integration_env_lock();
        let base = std::env::temp_dir().join(format!(
            "gardn-picker-ignore-unlaunchable-default-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let _codex_home_env = crate::config::TestEnvVar::remove("CODEX_HOME");
        let _home_env = crate::config::TestEnvVar::set("HOME", &home);
        let mut app = app_with_space();
        app.state.integration_recommendations.clear();
        app.state.groups[app.state.active_group].default_agent_profile_id =
            Some("system:omp".to_string());

        open_new_agent_picker_for_workspace(&mut app.state, 0);

        assert_eq!(app.state.request_agent_profile_tab, None);
        assert_eq!(app.state.mode, Mode::AgentProfilePicker);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn picker_hides_profiles_without_installed_integrations() {
        let mut app = app_with_space();
        app.state.integration_recommendations = vec![current_integration_for(
            crate::agent_profiles::AgentKind::Codex,
        )];

        let entries = crate::app::agent_profile_picker::agent_profile_picker_entries(&app.state);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].profile_id, "system:codex");
    }

    #[test]
    fn picker_keeps_custom_codex_profile_visible_with_profile_hook_warning() {
        let _lock = crate::integration::integration_env_lock();
        let base = std::env::temp_dir().join(format!(
            "gardn-picker-codex-profile-warning-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = base.join("home");
        std::fs::create_dir_all(home.join(".codex-mk")).unwrap();
        let _codex_home_env = crate::config::TestEnvVar::remove("CODEX_HOME");
        let _home_env = crate::config::TestEnvVar::set("HOME", &home);
        let mut app = app_with_space();
        app.state.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-mk".to_string()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-mk".to_string(),
                    name: "codex mk".to_string(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-mk".to_string(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        app.state.integration_recommendations = vec![current_integration_for(
            crate::agent_profiles::AgentKind::Codex,
        )];

        let entries = crate::app::agent_profile_picker::agent_profile_picker_entries(&app.state);
        let entry = entries
            .iter()
            .find(|entry| entry.profile_id == "user:codex-mk")
            .expect("custom codex profile remains visible while its profile hook needs install");
        let warning = entry
            .integration_warning
            .as_deref()
            .expect("visible custom codex profile carries an integration warning");

        assert_eq!(entry.name, "codex mk");
        assert!(warning.contains(".codex-mk"), "{warning}");
        assert!(
            warning.contains("gardn integration install codex"),
            "{warning}"
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn picker_keeps_outdated_integrations_available() {
        let mut app = app_with_space();
        app.state.integration_recommendations =
            vec![crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Codex,
                label: "codex",
                command: "codex",
                available: true,
                path: std::path::PathBuf::from("/tmp/gardn-test-integration"),
                state: crate::integration::IntegrationStatusKind::Outdated,
            }];

        let entries = crate::app::agent_profile_picker::agent_profile_picker_entries(&app.state);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].profile_id, "system:codex");
    }

    #[test]
    fn new_agent_uses_outdated_default_profile_without_picker() {
        let mut app = app_with_space();
        app.state.integration_recommendations =
            vec![crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Omp,
                label: "omp",
                command: "omp",
                available: true,
                path: std::path::PathBuf::from("/tmp/gardn-test-integration"),
                state: crate::integration::IntegrationStatusKind::Outdated,
            }];
        app.state.groups[app.state.active_group].default_agent_profile_id =
            Some("system:omp".to_string());

        open_new_agent_picker_for_workspace(&mut app.state, 0);

        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((0, "system:omp".to_string()))
        );
    }

    #[test]
    fn picker_alt_number_uses_workspace_group_favorites_when_active_group_differs() {
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
        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::ALT));

        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((1, "system:codex".to_string()))
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn picker_filter_shortcut_filters_by_agent_family() {
        let mut app = app_with_space();
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));

        assert_eq!(
            app.state.agent_profile_picker.kind_filter,
            Some(crate::agent_profiles::AgentKind::Pi)
        );
        let entries = agent_profile_picker_filtered_entries(&app.state);
        assert!(entries
            .iter()
            .all(|entry| entry.kind == crate::agent_profiles::AgentKind::Pi));
    }

    #[test]
    fn picker_unmodified_arrows_do_not_change_filter() {
        let mut app = app_with_space();
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));

        assert_eq!(app.state.agent_profile_picker.kind_filter, None);
    }

    #[test]
    fn picker_alt_number_launches_visible_family_favorite() {
        let mut app = app_with_space();
        app.state.groups[app.state.active_group].favorite_agent_profile_ids = vec![
            "system:pi".to_string(),
            "system:omp".to_string(),
            "system:claude".to_string(),
        ];
        open_new_agent_picker_for_workspace(&mut app.state, 0);
        app.state.agent_profile_picker.kind_filter = Some(crate::agent_profiles::AgentKind::Omp);

        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::ALT));

        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((0, "system:omp".to_string()))
        );
    }

    #[test]
    fn picker_overflow_chevron_selects_hidden_tab() {
        let mut app = app_with_space();
        app.state.view.terminal_area = Rect::new(0, 0, 60, 24);
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        let tab_row = agent_profile_picker_tab_row(&app.state).expect("tab row");
        let right = crate::ui::agent_profile_picker_tab_hit_areas(&app.state, tab_row)
            .last()
            .map(|(_, rect)| (rect.x.saturating_add(rect.width), rect.y))
            .expect("visible tab");

        assert!(select_agent_profile_picker_tab_at(
            &mut app.state,
            right.0,
            right.1
        ));
        assert!(app.state.agent_profile_picker.kind_filter.is_some());
        assert_ne!(
            app.state.agent_profile_picker.kind_filter,
            Some(crate::agent_profiles::AgentKind::Pi)
        );
    }

    #[test]
    fn picker_tabs_select_visible_rendered_labels_after_last_tab_selected() {
        let mut app = app_with_space();
        app.state.view.terminal_area = Rect::new(0, 0, 80, 24);
        open_new_agent_picker_for_workspace(&mut app.state, 0);
        app.state.agent_profile_picker.kind_filter =
            Some(crate::agent_profiles::AgentKind::Qodercli);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render agent picker");
        let buffer = terminal.backend().buffer();
        let filter_y = (0..24)
            .find(|&y| {
                (0..=80 - 6).any(|x| {
                    ["F", "i", "l", "t", "e", "r"]
                        .iter()
                        .enumerate()
                        .all(|(idx, ch)| buffer[(x + idx as u16, y)].symbol() == *ch)
                })
            })
            .expect("rendered filter row");

        let mut clicked_non_selected_tab = false;
        for tab in AGENT_PROFILE_PICKER_TABS {
            let label = crate::app::agent_profile_picker::agent_profile_picker_tab_label(tab);
            let rendered = format!(" {label} ");
            let symbols = rendered
                .chars()
                .map(|ch| ch.to_string())
                .collect::<Vec<_>>();
            let Some(x) = (0..=80 - symbols.len() as u16).find(|&x| {
                symbols
                    .iter()
                    .enumerate()
                    .all(|(idx, ch)| buffer[(x + idx as u16, filter_y)].symbol() == ch.as_str())
            }) else {
                continue;
            };

            app.state.agent_profile_picker.kind_filter =
                Some(crate::agent_profiles::AgentKind::Qodercli);
            app.handle_mouse(super::super::mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                x + 1,
                filter_y,
            ));
            assert_eq!(app.state.agent_profile_picker.kind_filter, tab);
            clicked_non_selected_tab |= tab != Some(crate::agent_profiles::AgentKind::Qodercli);
        }

        assert!(clicked_non_selected_tab);
    }

    #[test]
    fn picker_hover_highlights_rendered_profile_row() {
        let mut app = app_with_space();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 156, 48));
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        let backend = ratatui::backend::TestBackend::new(156, 48);
        let mut terminal = ratatui::Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render agent picker");

        let buffer = terminal.backend().buffer();
        let (codex_x, codex_y) = (0..48)
            .flat_map(|y| {
                (0..153).filter_map(move |x| {
                    ["C", "o", "d", "e", "x"]
                        .iter()
                        .enumerate()
                        .all(|(idx, ch)| buffer[(x + idx as u16, y)].symbol() == *ch)
                        .then_some((x, y))
                })
            })
            .last()
            .expect("codex profile");
        let codex_idx = agent_profile_picker_filtered_entries(&app.state)
            .iter()
            .position(|entry| entry.profile_id == "system:codex")
            .expect("codex profile index");

        app.handle_mouse(super::super::mouse(
            crossterm::event::MouseEventKind::Moved,
            codex_x,
            codex_y,
        ));

        assert_eq!(app.state.agent_profile_picker.list.selected, 0);
        assert_eq!(
            app.state.agent_profile_picker.list.visible(),
            Some(codex_idx)
        );
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render hovered agent picker");
        let hovered = terminal.backend().buffer();
        assert_eq!(
            hovered[(codex_x, codex_y)].style().bg,
            Some(app.state.palette_for_workspace(0).accent)
        );
        assert_ne!(
            hovered[(codex_x, codex_y.saturating_add(1))].style().bg,
            Some(app.state.palette_for_workspace(0).accent)
        );
        app.handle_mouse(super::super::mouse(
            crossterm::event::MouseEventKind::Moved,
            0,
            0,
        ));
        assert_eq!(app.state.agent_profile_picker.list.visible(), None);
        assert_eq!(app.state.agent_profile_picker.list.selected, 0);
        app.handle_agent_profile_picker_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));
        assert_eq!(app.state.agent_profile_picker.list.visible(), Some(1));
    }

    #[test]
    fn picker_click_uses_rendered_mobile_geometry() {
        let mut app = app_with_space();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 119, 24));
        open_new_agent_picker_for_workspace(&mut app.state, 0);

        let backend = ratatui::backend::TestBackend::new(119, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render agent picker");

        let buffer = terminal.backend().buffer();
        let (codex_x, codex_y) = (0..24)
            .flat_map(|y| {
                (0..114).filter_map(move |x| {
                    ["C", "o", "d", "e", "x"]
                        .iter()
                        .enumerate()
                        .all(|(idx, ch)| buffer[(x + idx as u16, y)].symbol() == *ch)
                        .then_some((x, y))
                })
            })
            .last()
            .expect("codex profile");
        let codex_idx = agent_profile_picker_filtered_entries(&app.state)
            .iter()
            .position(|entry| entry.profile_id == "system:codex")
            .expect("codex profile index");

        app.handle_mouse(super::super::mouse(
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            codex_x,
            codex_y,
        ));

        assert_eq!(app.state.agent_profile_picker.list.selected, codex_idx);
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
