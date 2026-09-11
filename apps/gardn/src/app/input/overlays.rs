use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::Rect,
    widgets::{Block, Borders},
};

use crate::app::{
    state::{AppState, DragState, DragTarget, Mode},
    App, ClientViewState,
};

use super::{modal::keybind_help_back, ScrollbarClickTarget, MODAL_WHEEL_SCROLL_ROWS};

impl App {
    pub(crate) fn handle_client_view_overlay_mouse(
        &mut self,
        client_view: &mut ClientViewState,
        mouse: MouseEvent,
    ) -> bool {
        match client_view.mode {
            Mode::ReleaseNotes => {
                handle_release_notes_mouse(&self.state, client_view, mouse);
            }
            Mode::ProductAnnouncement => {
                handle_product_announcement_mouse(&self.state, client_view, mouse);
            }
            Mode::KeybindHelp => {
                handle_keybind_help_mouse(&self.state, client_view, mouse);
            }
            _ => return false,
        }
        true
    }
}

fn handle_release_notes_mouse(state: &AppState, view: &mut ClientViewState, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left)
            if release_notes_close_button_at(view, mouse.column, mouse.row) =>
        {
            view.return_to_active_workspace_mode();
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(target) =
                release_notes_scrollbar_target_at(state, view, mouse.column, mouse.row)
            {
                match target {
                    ScrollbarClickTarget::Thumb { grab_row_offset } => {
                        view.drag = Some(DragState {
                            target: DragTarget::ReleaseNotesScrollbar { grab_row_offset },
                        });
                    }
                    ScrollbarClickTarget::Track { offset_from_bottom } => {
                        set_release_notes_offset_from_bottom(state, view, offset_from_bottom);
                    }
                }
            } else if !rect_contains(release_notes_popup_rect(view), mouse.column, mouse.row) {
                view.return_to_active_workspace_mode();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(DragState {
                target: DragTarget::ReleaseNotesScrollbar { grab_row_offset },
            }) = &view.drag
            {
                if let Some(offset_from_bottom) =
                    release_notes_offset_for_drag_row(state, view, mouse.row, *grab_row_offset)
                {
                    set_release_notes_offset_from_bottom(state, view, offset_from_bottom);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            view.drag = None;
        }
        MouseEventKind::ScrollUp => scroll_release_notes(state, view, -MODAL_WHEEL_SCROLL_ROWS),
        MouseEventKind::ScrollDown => scroll_release_notes(state, view, MODAL_WHEEL_SCROLL_ROWS),
        _ => {}
    }
}

fn handle_product_announcement_mouse(
    state: &AppState,
    view: &mut ClientViewState,
    mouse: MouseEvent,
) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left)
            if product_announcement_close_button_at(view, mouse.column, mouse.row) =>
        {
            view.return_to_active_workspace_mode();
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(target) =
                product_announcement_scrollbar_target_at(state, view, mouse.column, mouse.row)
            {
                match target {
                    ScrollbarClickTarget::Thumb { grab_row_offset } => {
                        view.drag = Some(DragState {
                            target: DragTarget::ProductAnnouncementScrollbar { grab_row_offset },
                        });
                    }
                    ScrollbarClickTarget::Track { offset_from_bottom } => {
                        set_product_announcement_offset_from_bottom(
                            state,
                            view,
                            offset_from_bottom,
                        );
                    }
                }
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(DragState {
                target: DragTarget::ProductAnnouncementScrollbar { grab_row_offset },
            }) = &view.drag
            {
                if let Some(offset_from_bottom) = product_announcement_offset_for_drag_row(
                    state,
                    view,
                    mouse.row,
                    *grab_row_offset,
                ) {
                    set_product_announcement_offset_from_bottom(state, view, offset_from_bottom);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            view.drag = None;
        }
        MouseEventKind::ScrollUp => {
            scroll_product_announcement(state, view, -MODAL_WHEEL_SCROLL_ROWS)
        }
        MouseEventKind::ScrollDown => {
            scroll_product_announcement(state, view, MODAL_WHEEL_SCROLL_ROWS)
        }
        _ => {}
    }
}

fn handle_keybind_help_mouse(state: &AppState, view: &mut ClientViewState, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left)
            if keybind_help_close_button_at(view, mouse.column, mouse.row) =>
        {
            if keybind_help_back(&mut view.keybind_help) {
                view.return_to_active_workspace_mode();
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(target) =
                keybind_help_scrollbar_target_at(state, view, mouse.column, mouse.row)
            {
                match target {
                    ScrollbarClickTarget::Thumb { grab_row_offset } => {
                        view.drag = Some(DragState {
                            target: DragTarget::KeybindHelpScrollbar { grab_row_offset },
                        });
                    }
                    ScrollbarClickTarget::Track { offset_from_bottom } => {
                        set_keybind_help_offset_from_bottom(state, view, offset_from_bottom);
                    }
                }
            } else if !rect_contains(keybind_help_popup_rect(view), mouse.column, mouse.row) {
                view.return_to_active_workspace_mode();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some(DragState {
                target: DragTarget::KeybindHelpScrollbar { grab_row_offset },
            }) = &view.drag
            {
                if let Some(offset_from_bottom) =
                    keybind_help_offset_for_drag_row(state, view, mouse.row, *grab_row_offset)
                {
                    set_keybind_help_offset_from_bottom(state, view, offset_from_bottom);
                }
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            view.drag = None;
        }
        MouseEventKind::ScrollUp => scroll_keybind_help(state, view, -MODAL_WHEEL_SCROLL_ROWS),
        MouseEventKind::ScrollDown => scroll_keybind_help(state, view, MODAL_WHEEL_SCROLL_ROWS),
        _ => {}
    }
}

fn modal_inner(view: &ClientViewState, popup_w: u16, popup_h: u16) -> Option<Rect> {
    crate::ui::centered_popup_rect(view.screen_rect(), popup_w, popup_h)
        .map(|popup| Block::default().borders(Borders::ALL).inner(popup))
}

fn release_notes_modal_inner(view: &ClientViewState) -> Option<Rect> {
    modal_inner(
        view,
        crate::ui::RELEASE_NOTES_MODAL_SIZE.0,
        crate::ui::RELEASE_NOTES_MODAL_SIZE.1,
    )
}

fn release_notes_popup_rect(view: &ClientViewState) -> Rect {
    crate::ui::centered_popup_rect(
        view.screen_rect(),
        crate::ui::RELEASE_NOTES_MODAL_SIZE.0,
        crate::ui::RELEASE_NOTES_MODAL_SIZE.1,
    )
    .unwrap_or_default()
}

fn product_announcement_modal_inner(view: &ClientViewState) -> Option<Rect> {
    modal_inner(
        view,
        crate::ui::PRODUCT_ANNOUNCEMENT_MODAL_SIZE.0,
        crate::ui::PRODUCT_ANNOUNCEMENT_MODAL_SIZE.1,
    )
}

fn release_notes_close_button_at(view: &ClientViewState, col: u16, row: u16) -> bool {
    let Some(inner) = release_notes_modal_inner(view) else {
        return false;
    };
    if inner.height < 4 || inner.width < 12 {
        return false;
    }
    let button =
        crate::ui::release_notes_close_button_rect(Rect::new(inner.x, inner.y, inner.width, 1));
    rect_contains(button, col, row)
}

fn release_notes_body_rect(view: &ClientViewState) -> Option<Rect> {
    let inner = release_notes_modal_inner(view)?;
    if inner.height < 8 || inner.width < 4 {
        return None;
    }
    Some(crate::ui::modal_stack_areas(inner, 2, 1, 0, 1).content)
}

fn release_notes_scroll_metrics(
    state: &AppState,
    view: &ClientViewState,
) -> Option<crate::pane::ScrollMetrics> {
    let notes = view.release_notes.as_ref()?;
    let body = release_notes_body_rect(view)?;
    let viewport_rows = body.height.max(1) as usize;
    let lines = crate::ui::release_notes_display_lines(notes, state.update_install, &state.palette);
    let rows_for_width =
        |wrap_width: u16| crate::ui::release_notes_wrapped_line_count(&lines, wrap_width.max(1));
    let full_width = body.width.max(1);
    let mut total_rows = rows_for_width(full_width);
    let wrap_width = if total_rows > viewport_rows && full_width > 1 {
        body.width.saturating_sub(1).max(1)
    } else {
        full_width
    };
    total_rows = rows_for_width(wrap_width);
    Some(crate::ui::modal_scroll_metrics(
        total_rows,
        viewport_rows,
        notes.scroll as usize,
    ))
}

fn release_notes_max_scroll(state: &AppState, view: &ClientViewState) -> u16 {
    release_notes_scroll_metrics(state, view)
        .map(|metrics| metrics.max_offset_from_bottom as u16)
        .unwrap_or(0)
}

fn release_notes_scrollbar_target_at(
    state: &AppState,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<ScrollbarClickTarget> {
    let body = release_notes_body_rect(view)?;
    let metrics = release_notes_scroll_metrics(state, view)?;
    let track = crate::ui::release_notes_scrollbar_rect(body, metrics)?;
    if !rect_contains(track, col, row) {
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

fn release_notes_offset_for_drag_row(
    state: &AppState,
    view: &ClientViewState,
    row: u16,
    grab_row_offset: u16,
) -> Option<usize> {
    let body = release_notes_body_rect(view)?;
    let metrics = release_notes_scroll_metrics(state, view)?;
    let track = crate::ui::release_notes_scrollbar_rect(body, metrics)?;
    Some(crate::ui::scrollbar_offset_from_drag_row(
        metrics,
        track,
        row,
        grab_row_offset,
    ))
}

fn set_release_notes_offset_from_bottom(
    state: &AppState,
    view: &mut ClientViewState,
    offset_from_bottom: usize,
) {
    let max_scroll = release_notes_max_scroll(state, view) as usize;
    if let Some(notes) = &mut view.release_notes {
        notes.scroll = max_scroll.saturating_sub(offset_from_bottom) as u16;
    }
}

fn scroll_release_notes(state: &AppState, view: &mut ClientViewState, delta: i16) {
    let max_scroll = release_notes_max_scroll(state, view);
    if let Some(notes) = &mut view.release_notes {
        notes.scroll = if delta.is_negative() {
            notes.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            notes.scroll.saturating_add(delta as u16).min(max_scroll)
        };
    }
}

fn product_announcement_close_button_at(view: &ClientViewState, col: u16, row: u16) -> bool {
    let Some(inner) = product_announcement_modal_inner(view) else {
        return false;
    };
    if inner.height < 4 || inner.width < 12 {
        return false;
    }
    let button =
        crate::ui::release_notes_close_button_rect(Rect::new(inner.x, inner.y, inner.width, 1));
    rect_contains(button, col, row)
}

fn product_announcement_body_rect(view: &ClientViewState) -> Option<Rect> {
    let inner = product_announcement_modal_inner(view)?;
    if inner.height < 8 || inner.width < 4 {
        return None;
    }
    Some(crate::ui::modal_stack_areas(inner, 2, 1, 0, 1).content)
}

fn product_announcement_scroll_metrics(
    state: &AppState,
    view: &ClientViewState,
) -> Option<crate::pane::ScrollMetrics> {
    let announcement = view.product_announcement.as_ref()?;
    let body = product_announcement_body_rect(view)?;
    let viewport_rows = body.height.max(1) as usize;
    let lines = crate::ui::product_announcement_display_lines(announcement, &state.palette);
    let rows_for_width =
        |wrap_width: u16| crate::ui::release_notes_wrapped_line_count(&lines, wrap_width.max(1));
    let full_width = body.width.max(1);
    let mut total_rows = rows_for_width(full_width);
    let wrap_width = if total_rows > viewport_rows && full_width > 1 {
        body.width.saturating_sub(1).max(1)
    } else {
        full_width
    };
    total_rows = rows_for_width(wrap_width);
    let max_offset_from_bottom = total_rows.saturating_sub(viewport_rows);
    Some(crate::pane::ScrollMetrics {
        offset_from_bottom: max_offset_from_bottom.saturating_sub(announcement.scroll as usize),
        max_offset_from_bottom,
        viewport_rows,
    })
}

fn product_announcement_max_scroll(state: &AppState, view: &ClientViewState) -> u16 {
    product_announcement_scroll_metrics(state, view)
        .map(|metrics| metrics.max_offset_from_bottom as u16)
        .unwrap_or(0)
}

fn product_announcement_scrollbar_target_at(
    state: &AppState,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<ScrollbarClickTarget> {
    let body = product_announcement_body_rect(view)?;
    let metrics = product_announcement_scroll_metrics(state, view)?;
    let track = crate::ui::release_notes_scrollbar_rect(body, metrics)?;
    if !rect_contains(track, col, row) {
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

fn product_announcement_offset_for_drag_row(
    state: &AppState,
    view: &ClientViewState,
    row: u16,
    grab_row_offset: u16,
) -> Option<usize> {
    let body = product_announcement_body_rect(view)?;
    let metrics = product_announcement_scroll_metrics(state, view)?;
    let track = crate::ui::release_notes_scrollbar_rect(body, metrics)?;
    Some(crate::ui::scrollbar_offset_from_drag_row(
        metrics,
        track,
        row,
        grab_row_offset,
    ))
}

fn set_product_announcement_offset_from_bottom(
    state: &AppState,
    view: &mut ClientViewState,
    offset_from_bottom: usize,
) {
    let max_scroll = product_announcement_max_scroll(state, view) as usize;
    if let Some(announcement) = &mut view.product_announcement {
        announcement.scroll = max_scroll.saturating_sub(offset_from_bottom) as u16;
    }
}

fn scroll_product_announcement(state: &AppState, view: &mut ClientViewState, delta: i16) {
    let max_scroll = product_announcement_max_scroll(state, view);
    if let Some(announcement) = &mut view.product_announcement {
        announcement.scroll = if delta.is_negative() {
            announcement.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            announcement
                .scroll
                .saturating_add(delta as u16)
                .min(max_scroll)
        };
    }
}

fn keybind_help_popup_rect(view: &ClientViewState) -> Rect {
    crate::ui::keybind_help_layout(view.screen_rect(), view.keybind_help.search_focused)
        .map(|layout| layout.popup)
        .unwrap_or_default()
}

fn keybind_help_close_button_at(view: &ClientViewState, col: u16, row: u16) -> bool {
    crate::ui::keybind_help_layout(view.screen_rect(), view.keybind_help.search_focused)
        .is_some_and(|layout| rect_contains(layout.close, col, row))
}

fn keybind_help_body_rect(view: &ClientViewState) -> Option<Rect> {
    crate::ui::keybind_help_layout(view.screen_rect(), view.keybind_help.search_focused)
        .map(|layout| layout.body)
}

fn keybind_help_scroll_metrics(
    state: &AppState,
    view: &ClientViewState,
) -> Option<crate::pane::ScrollMetrics> {
    let body = keybind_help_body_rect(view)?;
    Some(crate::ui::keybind_help_scroll_metrics(
        state,
        body,
        view.keybind_help.scroll,
        &view.keybind_help.query,
    ))
}

fn keybind_help_max_scroll(state: &AppState, view: &ClientViewState) -> u16 {
    keybind_help_scroll_metrics(state, view)
        .map(|metrics| metrics.max_offset_from_bottom as u16)
        .unwrap_or(0)
}

fn keybind_help_scrollbar_target_at(
    state: &AppState,
    view: &ClientViewState,
    col: u16,
    row: u16,
) -> Option<ScrollbarClickTarget> {
    let body = keybind_help_body_rect(view)?;
    let metrics = keybind_help_scroll_metrics(state, view)?;
    let track = crate::ui::keybind_help_scrollbar_rect(body, metrics)?;
    if !rect_contains(track, col, row) {
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

fn keybind_help_offset_for_drag_row(
    state: &AppState,
    view: &ClientViewState,
    row: u16,
    grab_row_offset: u16,
) -> Option<usize> {
    let body = keybind_help_body_rect(view)?;
    let metrics = keybind_help_scroll_metrics(state, view)?;
    let track = crate::ui::keybind_help_scrollbar_rect(body, metrics)?;
    Some(crate::ui::scrollbar_offset_from_drag_row(
        metrics,
        track,
        row,
        grab_row_offset,
    ))
}

fn set_keybind_help_offset_from_bottom(
    state: &AppState,
    view: &mut ClientViewState,
    offset_from_bottom: usize,
) {
    let max_scroll = keybind_help_max_scroll(state, view) as usize;
    view.keybind_help.scroll = max_scroll.saturating_sub(offset_from_bottom) as u16;
}

fn scroll_keybind_help(state: &AppState, view: &mut ClientViewState, delta: i16) {
    let max_scroll = keybind_help_max_scroll(state, view);
    let current = view.keybind_help.scroll as i16;
    view.keybind_help.scroll = current.saturating_add(delta).clamp(0, max_scroll as i16) as u16;
}

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && col >= rect.x
        && col < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}
