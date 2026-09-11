use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

use crate::app::{
    agent_profile_picker::{
        agent_profile_picker_filtered_entries_for_picker, AGENT_PROFILE_PICKER_TABS,
    },
    state::AppState,
    view_state::ClientViewState,
};

use super::{modal::modal_action_from_buttons, modal::ModalAction};

pub(crate) fn handle_agent_profile_picker_key_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    can_mutate_tab: bool,
    key: KeyEvent,
) {
    if let Some(index) = agent_profile_picker_favorite_shortcut_index(key) {
        if can_mutate_tab {
            launch_favorite_agent_profile_by_shortcut_for_view(state, view, index);
        } else {
            view.return_to_active_workspace_mode();
        }
        return;
    }

    match key.code {
        KeyCode::Esc => view.return_to_active_workspace_mode(),
        KeyCode::Enter if can_mutate_tab => launch_selected_agent_profile_for_view(state, view),
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
            .insert(ws.id.clone(), ws.next_remote_tab_number());
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

    let tab_idx = crate::ui::agent_profile_picker_tab_chevron_at_for_view(view, tab_row, col)
        .or_else(|| {
            crate::ui::agent_profile_picker_tab_hit_areas_for_view(view, tab_row)
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
