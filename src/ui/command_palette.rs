use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::app::{
    command_palette::{command_palette_filtered_commands, CommandPaletteCommand},
    AppState,
};

use super::{
    scrollbar::{render_scrollbar, should_show_scrollbar},
    widgets::{panel_contrast_fg, render_modal_header, render_modal_shell},
};

const COMMAND_PALETTE_SCROLLBAR_WIDTH: u16 = 3;

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
    let palette_rows = command_palette_rows(&commands);
    let visible_rows = rows[3].height as usize;
    let max_start = palette_rows.len().saturating_sub(visible_rows);
    let start = app.command_palette.scroll.min(max_start);
    let end = (start + visible_rows).min(palette_rows.len());
    let metrics = crate::pane::ScrollMetrics {
        offset_from_bottom: palette_rows
            .len()
            .saturating_sub(start)
            .saturating_sub(visible_rows),
        max_offset_from_bottom: palette_rows.len().saturating_sub(visible_rows),
        viewport_rows: visible_rows,
    };
    let has_scrollbar =
        should_show_scrollbar(metrics) && rows[3].width > COMMAND_PALETTE_SCROLLBAR_WIDTH;

    let lines = palette_rows[start..end]
        .iter()
        .map(|row| match row {
            CommandPaletteRow::Header(group) => Line::from(Span::styled(
                format!(" {}", group),
                Style::default()
                    .fg(app.palette.overlay1)
                    .add_modifier(Modifier::DIM),
            )),
            CommandPaletteRow::Command(idx, command) => {
                let selected = *idx == selected;
                let style = if selected {
                    Style::default()
                        .fg(panel_contrast_fg(&app.palette))
                        .bg(app.palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.palette.text)
                };
                Line::from(Span::styled(format!("  {}", command.title), style))
            }
        })
        .collect::<Vec<_>>();

    let list_area = if has_scrollbar {
        Rect::new(
            rows[3].x,
            rows[3].y,
            rows[3]
                .width
                .saturating_sub(COMMAND_PALETTE_SCROLLBAR_WIDTH),
            rows[3].height,
        )
    } else {
        rows[3]
    };
    frame.render_widget(Paragraph::new(lines), list_area);

    if has_scrollbar {
        let track = Rect::new(
            rows[3].x
                + rows[3]
                    .width
                    .saturating_sub(COMMAND_PALETTE_SCROLLBAR_WIDTH),
            rows[3].y,
            COMMAND_PALETTE_SCROLLBAR_WIDTH,
            rows[3].height,
        );
        render_scrollbar(
            frame,
            metrics,
            track,
            app.palette.surface_dim,
            app.palette.overlay0,
            "█",
        );
    }
}

enum CommandPaletteRow<'a> {
    Header(&'static str),
    Command(usize, &'a CommandPaletteCommand),
}

fn command_palette_rows(commands: &[CommandPaletteCommand]) -> Vec<CommandPaletteRow<'_>> {
    let mut rows = Vec::new();
    let mut last_group = None;

    for (idx, command) in commands.iter().enumerate() {
        if last_group != Some(command.group) {
            rows.push(CommandPaletteRow::Header(command.group));
            last_group = Some(command.group);
        }
        rows.push(CommandPaletteRow::Command(idx, command));
    }

    rows
}
