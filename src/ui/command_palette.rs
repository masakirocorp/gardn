use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::app::{command_palette::command_palette_filtered_commands, AppState};

use super::widgets::{panel_contrast_fg, render_modal_header, render_modal_shell};

pub(super) fn render_command_palette_overlay(app: &AppState, frame: &mut Frame) {
    super::dim_background(frame, frame.area());

    let Some(inner) = render_modal_shell(frame, frame.area(), 76, 18, &app.palette) else {
        return;
    };
    if inner.height < 6 || inner.width < 20 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas::<4>(inner);

    render_modal_header(frame, rows[0], "command palette", &app.palette);
    frame.render_widget(
        Paragraph::new(" type to filter, enter to run, esc to close")
            .style(Style::default().fg(app.palette.overlay1)),
        rows[1],
    );

    let input = Rect::new(rows[2].x, rows[2].y, rows[2].width, 1);
    frame.render_widget(Clear, input);
    frame.render_widget(
        Paragraph::new(format!(" {}█", app.command_palette.query)).style(
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.surface0),
        ),
        input,
    );

    let commands = command_palette_filtered_commands(app);
    if commands.is_empty() {
        frame.render_widget(
            Paragraph::new(" no commands").style(Style::default().fg(app.palette.overlay1)),
            rows[3],
        );
        return;
    }

    let selected = app
        .command_palette
        .selected
        .min(commands.len().saturating_sub(1));
    let visible_rows = rows[3].height as usize;
    let start = if visible_rows == 0 {
        0
    } else if selected >= visible_rows {
        selected + 1 - visible_rows
    } else {
        0
    };
    let end = (start + visible_rows).min(commands.len());

    let lines = commands[start..end]
        .iter()
        .enumerate()
        .map(|(offset, command)| {
            let idx = start + offset;
            let selected = idx == selected;
            let style = if selected {
                Style::default()
                    .fg(panel_contrast_fg(&app.palette))
                    .bg(app.palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.palette.text)
            };
            let group_style = if selected {
                style
            } else {
                Style::default().fg(app.palette.overlay1)
            };
            Line::from(vec![
                Span::styled(format!(" {}", command.title), style),
                Span::styled(format!("  {}", command.group), group_style),
            ])
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(lines), rows[3]);
}
