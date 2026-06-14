use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::widgets::{
    action_button_row_rects, centered_popup_rect, danger_action_style, panel_contrast_fg,
    primary_action_style, render_action_button, render_modal_header_bar, render_modal_shell,
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
        ],
        2,
        inner.height.saturating_sub(1),
    );
    let close =
        super::widgets::modal_close_button_rect(Rect::new(inner.x, inner.y, inner.width, 1));
    (rects[0], rects[1], close)
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

fn rename_palette(app: &AppState) -> crate::app::state::Palette {
    match app.mode {
        Mode::RenameWorkspace | Mode::RenameTab | Mode::RenamePane => app
            .active
            .map(|ws_idx| app.palette_for_workspace(ws_idx))
            .unwrap_or_else(|| app.palette.clone()),
        Mode::RenameGroup if !app.creating_new_group => app
            .rename_group_target
            .map(|group_idx| app.palette_for_group(group_idx))
            .unwrap_or_else(|| app.palette_for_group(app.active_group)),
        _ => app.palette.clone(),
    }
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
        Mode::EditWorktreeDirectory => "worktree directory",
        _ => return,
    };

    let palette = rename_palette(app);
    let (popup_w, popup_h) = rename_modal_size(app);
    let Some(inner) = render_modal_shell(frame, area, popup_w, popup_h, &palette) else {
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

    render_modal_header_bar(frame, rows[0], title, &palette, true);
    if matches!(app.mode, Mode::RenameGroup) {
        render_modal_subtitle(frame, rows[1], " name + icon", &palette);
    }

    let input_rect = if matches!(app.mode, Mode::RenameGroup) {
        let icon_rect = group_icon_button_rect(inner);
        let icon_style = if app.group_icon_picker_open {
            Style::default()
                .fg(panel_contrast_fg(&palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(palette.text)
                .bg(palette.surface0)
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
    render_modal_text_input(frame, input_rect, &app.name_input, &palette);

    if matches!(app.mode, Mode::RenameGroup) && app.group_icon_picker_open {
        for (rect, icon) in group_icon_picker_rects(inner) {
            let selected = app.group_icon_input == icon;
            let style = if selected {
                Style::default()
                    .fg(panel_contrast_fg(&palette))
                    .bg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(palette.text)
                    .bg(palette.surface0)
            };
            frame.render_widget(
                Paragraph::new(format!(" {icon} "))
                    .style(style)
                    .alignment(Alignment::Center),
                rect,
            );
        }
    }

    let (save_rect, clear_rect, _) = rename_button_rects(inner);

    render_action_button(
        frame,
        save_rect,
        Some("↵"),
        "save",
        primary_action_style(&palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "clear",
        secondary_action_style(&palette),
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
        .map(|ws| ws.tabs.iter().map(|tab| tab.layout.pane_count()).sum())
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

    let palette = app.palette_for_workspace(app.selected);
    let warn = Style::default()
        .fg(palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" close workspace?", warn)]);

    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {ws_name}"),
            Style::default()
                .fg(palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — {pane_text}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, palette.red, palette.panel_bg)
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
            danger_action_style(&palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
            secondary_action_style(&palette),
        );
    }
}

pub(super) fn render_confirm_delete_group_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let group_idx = app.confirm_delete_group.unwrap_or(app.active_group);
    let group = app.groups.get(group_idx);
    let group_name = group.map(|group| group.name.as_str()).unwrap_or("?");
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

    let palette = app.palette_for_group(group_idx);
    let warn = Style::default()
        .fg(palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" delete group?", warn)]);
    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {group_name}"),
            Style::default()
                .fg(app.group_accent_color(group_idx))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — closes {spaces}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, palette.red, palette.panel_bg)
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
            danger_action_style(&palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
            secondary_action_style(&palette),
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

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    use super::*;
    use crate::workspace::Workspace;

    #[test]
    fn confirm_close_overlay_renders_empty_workspace() {
        let mut app = AppState::test_new();
        app.workspaces = vec![Workspace::test_new("empty")];
        app.workspaces[0].tabs.clear();
        app.selected = 0;
        app.active = Some(0);
        app.mode = Mode::ConfirmClose;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_confirm_close_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .unwrap();

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("close workspace?"));
        assert!(text.contains("empty"));
        assert!(text.contains("0 panes"));
        assert!(text.contains("↵"));
        assert!(text.contains("confirm"));
        assert!(text.contains("esc"));
        assert!(text.contains("cancel"));
    }

    #[test]
    fn confirm_delete_group_uses_group_accent_for_name() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Yellow));
        app.confirm_delete_group = Some(group_idx);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_confirm_delete_group_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_symbol(buffer, 80, 24, "w").expect("group name");
        assert_eq!(
            buffer[(x, y)].style().fg,
            Some(app.group_accent_color(group_idx))
        );
    }

    fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    fn first_cell_with_symbol(
        buffer: &Buffer,
        width: u16,
        height: u16,
        symbol: &str,
    ) -> Option<(u16, u16)> {
        for y in 0..height {
            for x in 0..width {
                if buffer[(x, y)].symbol() == symbol {
                    return Some((x, y));
                }
            }
        }
        None
    }
}
