use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{
    command_palette::{command_palette_filtered_commands_for_view, CommandPaletteAction},
    state::{AppState, Mode},
    view_state::ClientViewState,
};

use super::{modal::modal_action_from_buttons, modal::ModalAction, MODAL_PAGE_SCROLL_ROWS};

#[cfg(test)]
pub(crate) fn open_command_palette_for_view(view: &mut ClientViewState) {
    view.command_palette.query.clear();
    view.command_palette.list.select(0);
    view.command_palette.list.hide();
    view.command_palette.scroll = 0;
    view.mode = Mode::CommandPalette;
}

pub(crate) fn handle_command_palette_key_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    key: KeyEvent,
) {
    match key.code {
        KeyCode::Esc => {
            view.return_to_active_workspace_mode();
        }
        KeyCode::Enter => {}
        KeyCode::Up => {
            move_command_palette_selection_for_view(state, view, false);
        }
        KeyCode::Down => {
            move_command_palette_selection_for_view(state, view, true);
        }
        KeyCode::PageUp => {
            scroll_command_palette_rows_for_view(state, view, -MODAL_PAGE_SCROLL_ROWS)
        }
        KeyCode::PageDown => {
            scroll_command_palette_rows_for_view(state, view, MODAL_PAGE_SCROLL_ROWS)
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_command_palette_selection_for_view(state, view, false);
        }
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_command_palette_selection_for_view(state, view, true);
        }
        KeyCode::Backspace => {
            view.command_palette.query.pop();
            clamp_command_palette_selection_for_view(state, view);
        }
        KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
            view.command_palette.query.push(c);
            clamp_command_palette_selection_for_view(state, view);
        }
        _ => {}
    }
}

pub(crate) fn selected_command_palette_action_for_view(
    state: &AppState,
    view: &ClientViewState,
) -> Option<CommandPaletteAction> {
    command_palette_filtered_commands_for_view(state, view)
        .get(view.command_palette.list.selected)
        .map(|command| command.action.clone())
}

fn command_palette_action_button_at_for_view(
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<ModalAction> {
    let inner = crate::ui::command_palette_inner_rect(view.screen_rect())?;
    let (run, close) = crate::ui::command_palette_button_rects(inner);
    modal_action_from_buttons(
        col,
        row,
        &[(run, ModalAction::Apply), (close, ModalAction::Close)],
    )
}

pub(crate) fn handle_command_palette_mouse_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    mouse: MouseEvent,
) -> bool {
    if view.mode != Mode::CommandPalette {
        return false;
    }

    match mouse.kind {
        MouseEventKind::Moved => {
            hover_command_palette_selection_for_view(state, view, mouse.column, mouse.row);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if matches!(
                command_palette_action_button_at_for_view(view, mouse.column, mouse.row),
                Some(ModalAction::Close)
            ) {
                view.return_to_active_workspace_mode();
                view.command_palette.query.clear();
                view.command_palette.list.select(0);
                view.command_palette.scroll = 0;
            } else if command_palette_contains_point_for_view(view, mouse.column, mouse.row) {
                select_command_palette_selection_for_view(state, view, mouse.column, mouse.row);
            } else {
                view.return_to_active_workspace_mode();
                view.command_palette.query.clear();
                view.command_palette.list.select(0);
                view.command_palette.scroll = 0;
            }
        }
        MouseEventKind::ScrollDown => {
            scroll_command_palette_rows_for_view(state, view, super::MODAL_WHEEL_SCROLL_ROWS);
        }
        MouseEventKind::ScrollUp => {
            scroll_command_palette_rows_for_view(state, view, -super::MODAL_WHEEL_SCROLL_ROWS);
        }
        MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {}
        _ => {}
    }

    true
}

fn clamp_command_palette_selection_for_view(state: &AppState, view: &mut ClientViewState) {
    let count = command_palette_filtered_commands_for_view(state, view).len();
    if count == 0 {
        view.command_palette.list.select(0);
        view.command_palette.scroll = 0;
        return;
    }

    view.command_palette
        .list
        .select(view.command_palette.list.selected.min(count - 1));
    ensure_command_palette_selection_visible_for_view(state, view);
}

fn move_command_palette_selection_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    down: bool,
) -> bool {
    let count = command_palette_filtered_commands_for_view(state, view).len();
    if count == 0 {
        view.command_palette.list.select(0);
        view.command_palette.scroll = 0;
        return false;
    }

    let previous = view.command_palette.list.selected;
    let current = previous.min(count - 1);
    if current != previous {
        view.command_palette.list.select(current);
    }
    if down {
        view.command_palette.list.move_next(count);
    } else {
        view.command_palette.list.move_prev();
    }
    let changed = view.command_palette.list.selected != previous;
    ensure_command_palette_selection_visible_for_view(state, view);
    changed
}

fn scroll_command_palette_rows_for_view(state: &AppState, view: &mut ClientViewState, delta: i16) {
    let max_scroll = command_palette_max_scroll_for_view(state, view);
    let next = if delta.is_negative() {
        view.command_palette
            .scroll
            .saturating_sub(delta.unsigned_abs() as usize)
    } else {
        view.command_palette
            .scroll
            .saturating_add(delta as usize)
            .min(max_scroll)
    };
    view.command_palette.scroll = next.min(max_scroll);
}

fn hover_command_palette_selection_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    col: u16,
    row: u16,
) {
    let hovered = command_palette_selection_at_for_view(state, view, col, row);
    view.command_palette.list.hover(hovered);
}

fn select_command_palette_selection_for_view(
    state: &AppState,
    view: &mut ClientViewState,
    col: u16,
    row: u16,
) {
    match command_palette_selection_at_for_view(state, view, col, row) {
        Some(selected) => {
            view.command_palette.list.select(selected);
            ensure_command_palette_selection_visible_for_view(state, view);
        }
        None => view.command_palette.list.hover(None),
    }
}

fn command_palette_selection_at_for_view(
    state: &AppState,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<usize> {
    let (list, rows) = command_palette_viewport_for_view(state, view)?;
    let row_idx = list.hit_visual_row(col, row)?;
    rows.get(row_idx).copied().flatten()
}

fn command_palette_contains_point_for_view(view: &ClientViewState, col: u16, row: u16) -> bool {
    crate::ui::command_palette_popup_rect(view.screen_rect()).is_some_and(|popup| {
        col >= popup.x
            && col < popup.x + popup.width
            && row >= popup.y
            && row < popup.y + popup.height
    })
}

fn command_palette_max_scroll_for_view(state: &AppState, view: &ClientViewState) -> usize {
    command_palette_viewport_for_view(state, view)
        .map(|(list, _)| list.viewport.max_scroll())
        .unwrap_or(0)
}

fn command_palette_viewport_for_view(
    state: &AppState,
    view: &ClientViewState,
) -> Option<(crate::ui::ModalListGeometry, Vec<Option<usize>>)> {
    let rows = command_palette_rows_for_view(state, view)?;
    let list = crate::ui::command_palette_list_geometry(
        view.screen_rect(),
        rows.len(),
        view.command_palette.scroll,
    )?;
    Some((list, rows))
}

fn command_palette_rows_for_view(
    state: &AppState,
    view: &ClientViewState,
) -> Option<Vec<Option<usize>>> {
    let commands = command_palette_filtered_commands_for_view(state, view);
    if commands.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    let mut last_group = None;
    for (idx, command) in commands.iter().enumerate() {
        if last_group != Some(command.group) {
            if last_group.is_some() {
                rows.push(None);
            }
            rows.push(None);
            last_group = Some(command.group);
        }
        rows.push(Some(idx));
    }

    Some(rows)
}

fn ensure_command_palette_selection_visible_for_view(state: &AppState, view: &mut ClientViewState) {
    let Some((list, rows)) = command_palette_viewport_for_view(state, view) else {
        view.command_palette.scroll = 0;
        return;
    };

    let Some(selected_row) = rows
        .iter()
        .position(|row| *row == Some(view.command_palette.list.selected))
    else {
        view.command_palette.scroll = list.viewport.scroll();
        return;
    };

    let first_section_row = selected_row
        .checked_sub(1)
        .filter(|idx| rows.get(*idx).is_some_and(Option::is_none));
    view.command_palette.scroll = list
        .viewport
        .ensure_visible(selected_row, first_section_row);
}
