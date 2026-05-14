use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::widgets::{
    action_button_row_rects, centered_popup_rect, danger_action_style, panel_contrast_fg,
    primary_action_style, render_action_button, render_modal_header, render_modal_shell,
    render_modal_subtitle, render_modal_text_input, render_panel_shell, secondary_action_style,
    ActionButtonSpec,
};
use crate::app::{AppState, Mode};

pub(crate) fn rename_button_rects(inner: Rect) -> (Rect, Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: "save",
            },
            ActionButtonSpec {
                hint: Some("^c"),
                label: "clear",
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
            },
        ],
        2,
        inner.height.saturating_sub(1),
    );
    (rects[0], rects[1], rects[2])
}

pub(crate) fn rename_modal_size(app: &AppState) -> (u16, u16) {
    if matches!(app.mode, Mode::RenameGroup) {
        (56, 12)
    } else {
        (56, 7)
    }
}

pub(crate) fn group_icon_button_rect(inner: Rect) -> Rect {
    if inner.width < 5 || inner.height < 4 {
        return Rect::default();
    }
    Rect::new(inner.x, inner.y + 2, 3, 1)
}

pub(crate) fn group_name_input_rect(inner: Rect) -> Rect {
    let icon = group_icon_button_rect(inner);
    if icon == Rect::default() {
        return Rect::new(inner.x, inner.y + 2, inner.width, 1);
    }
    Rect::new(
        icon.x + icon.width + 1,
        icon.y,
        inner.width.saturating_sub(icon.width + 1),
        1,
    )
}

pub(crate) fn group_icon_picker_rects(inner: Rect) -> Vec<(Rect, &'static str)> {
    let start = Rect::new(inner.x, inner.y + 4, inner.width.min(24), 4);
    if start.width < 3 || start.height == 0 {
        return Vec::new();
    }

    crate::app::state::GROUP_ICONS
        .iter()
        .enumerate()
        .filter_map(|(idx, icon)| {
            let col = (idx % 5) as u16;
            let row = (idx / 5) as u16;
            if row >= start.height {
                return None;
            }
            let x = start.x + col * 4;
            if x >= start.x + start.width {
                return None;
            }
            Some((Rect::new(x, start.y + row, 3, 1), *icon))
        })
        .collect()
}

pub(super) fn render_rename_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    super::dim_background(frame, area);

    let title = match app.mode {
        Mode::RenameWorkspace => "rename workspace",
        Mode::RenameGroup if app.creating_new_group => "new group",
        Mode::RenameGroup => "rename group",
        Mode::RenameTab if app.creating_new_tab => "new tab",
        Mode::RenameTab => "rename tab",
        Mode::RenamePane => "rename pane",
        _ => return,
    };

    let (popup_w, popup_h) = rename_modal_size(app);
    let Some(inner) = render_modal_shell(frame, area, popup_w, popup_h, &app.palette) else {
        return;
    };
    if inner.height < 4 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<5>(inner);

    render_modal_header(frame, rows[0], title, &app.palette);
    if matches!(app.mode, Mode::RenameGroup) {
        render_modal_subtitle(frame, rows[1], " name + icon", &app.palette);
    }

    let input_rect = if matches!(app.mode, Mode::RenameGroup) {
        let icon_rect = group_icon_button_rect(inner);
        let icon_style = if app.group_icon_picker_open {
            Style::default()
                .fg(panel_contrast_fg(&app.palette))
                .bg(app.palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.surface0)
                .add_modifier(Modifier::BOLD)
        };
        frame.render_widget(
            Paragraph::new(format!(" {} ", app.group_icon_input))
                .style(icon_style)
                .alignment(ratatui::layout::Alignment::Center),
            icon_rect,
        );
        group_name_input_rect(inner)
    } else {
        Rect::new(rows[2].x, rows[2].y, rows[2].width, 1)
    };
    render_modal_text_input(frame, input_rect, &app.name_input, &app.palette);

    if matches!(app.mode, Mode::RenameGroup) && app.group_icon_picker_open {
        for (rect, icon) in group_icon_picker_rects(inner) {
            let selected = app.group_icon_input == icon;
            let style = if selected {
                Style::default()
                    .fg(panel_contrast_fg(&app.palette))
                    .bg(app.palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(app.palette.text)
                    .bg(app.palette.surface0)
            };
            frame.render_widget(
                Paragraph::new(format!(" {icon} "))
                    .style(style)
                    .alignment(Alignment::Center),
                rect,
            );
        }
    }

    let (save_rect, clear_rect, cancel_rect) = rename_button_rects(inner);

    render_action_button(
        frame,
        save_rect,
        Some("↵"),
        "save",
        primary_action_style(&app.palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "clear",
        secondary_action_style(&app.palette),
    );
    render_action_button(
        frame,
        cancel_rect,
        Some("esc"),
        "cancel",
        secondary_action_style(&app.palette),
    );
}

pub(super) fn render_confirm_close_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let ws_name = app
        .workspaces
        .get(app.selected)
        .map(|ws| ws.display_name())
        .unwrap_or_else(|| "?".to_string());
    let pane_count = app
        .workspaces
        .get(app.selected)
        .map(|ws| ws.layout.pane_count())
        .unwrap_or(0);

    let pane_text = if pane_count == 1 {
        "1 pane".to_string()
    } else {
        format!("{pane_count} panes")
    };

    super::dim_background(frame, area);

    let Some(popup) = confirm_close_popup_rect(area) else {
        return;
    };

    let warn = Style::default()
        .fg(app.palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" close workspace?", warn)]);

    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {ws_name}"),
            Style::default()
                .fg(app.palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — {pane_text}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, app.palette.red, app.palette.panel_bg)
    else {
        return;
    };

    if inner.height >= 3 {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<3>(inner);

        frame.render_widget(Paragraph::new(title_line), rows[0]);
        frame.render_widget(Paragraph::new(detail_line), rows[1]);

        let (confirm_rect, cancel_rect) = confirm_close_button_rects(inner);
        render_action_button(
            frame,
            confirm_rect,
            Some("↵"),
            "confirm",
            danger_action_style(&app.palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
            secondary_action_style(&app.palette),
        );
    }
}

pub(super) fn render_confirm_delete_group_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let group_idx = app.confirm_delete_group.unwrap_or(app.active_group);
    let group_name = app
        .groups
        .get(group_idx)
        .map(|group| group.name.as_str())
        .unwrap_or("?");
    let space_count = app
        .groups
        .get(group_idx)
        .map(|group| {
            app.workspaces
                .iter()
                .filter(|workspace| workspace.group_id == group.id)
                .count()
        })
        .unwrap_or(0);
    let spaces = if space_count == 1 {
        "1 space".to_string()
    } else {
        format!("{space_count} spaces")
    };

    super::dim_background(frame, area);

    let Some(popup) = confirm_close_popup_rect(area) else {
        return;
    };

    let warn = Style::default()
        .fg(app.palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" delete group?", warn)]);
    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {group_name}"),
            Style::default()
                .fg(app.palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — closes {spaces}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, app.palette.red, app.palette.panel_bg)
    else {
        return;
    };

    if inner.height >= 3 {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<3>(inner);

        frame.render_widget(Paragraph::new(title_line), rows[0]);
        frame.render_widget(Paragraph::new(detail_line), rows[1]);

        let (confirm_rect, cancel_rect) = confirm_close_button_rects(inner);
        render_action_button(
            frame,
            confirm_rect,
            Some("↵"),
            "confirm",
            danger_action_style(&app.palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
            secondary_action_style(&app.palette),
        );
    }
}

pub(crate) fn confirm_close_popup_rect(area: Rect) -> Option<Rect> {
    centered_popup_rect(area, 44, 5)
}

pub(crate) fn confirm_close_button_rects(inner: Rect) -> (Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: "confirm",
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
            },
        ],
        2,
        2,
    );
    (rects[0], rects[1])
}
