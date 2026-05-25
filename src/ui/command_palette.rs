use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{
    command_palette::{command_palette_filtered_commands, CommandPaletteCommand},
    AppState,
};

use super::{
    scrollbar::render_scrollbar,
    widgets::{
        action_button_row_rects, modal_section_heading_style, panel_contrast_fg,
        primary_action_style, render_action_button, render_modal_header_bar,
        render_modal_scroll_hints, render_modal_shell, render_modal_subtitle,
        render_modal_text_input, ActionButtonSpec,
    },
};

const COMMAND_PALETTE_KEY_HINT_RIGHT_PADDING: usize = 1;

pub(crate) fn command_palette_button_rects(inner: Rect) -> (Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[ActionButtonSpec {
            hint: Some("↵"),
            label: "run",
        }],
        2,
        inner.height.saturating_sub(1),
    );
    let close =
        super::widgets::modal_close_button_rect(Rect::new(inner.x, inner.y, inner.width, 1));
    (rects[0], close)
}

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
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<7>(inner);

    render_modal_header_bar(frame, rows[0], "command palette", &app.palette, true);
    render_modal_subtitle(frame, rows[1], "type to filter commands", &app.palette);

    let input = Rect::new(rows[2].x, rows[2].y, rows[2].width, 1);
    render_modal_text_input(frame, input, &app.command_palette.query, &app.palette);

    render_modal_scroll_hints(frame, rows[5], &app.palette);

    let (run_rect, _) = command_palette_button_rects(inner);
    render_action_button(
        frame,
        run_rect,
        Some("↵"),
        "run",
        primary_action_style(&app.palette),
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
    let viewport = crate::ui::ModalListViewport::new(
        palette_rows.len(),
        rows[3].height as usize,
        app.command_palette.scroll,
    );
    let visible_range = viewport.visible_range();
    let metrics = viewport.metrics();
    let scroll_area = viewport.scroll_area(rows[3]);
    let list_width =
        (scroll_area.body.width as usize).saturating_sub(COMMAND_PALETTE_KEY_HINT_RIGHT_PADDING);

    let lines = palette_rows[visible_range]
        .iter()
        .map(|row| match row {
            CommandPaletteRow::Spacer => Line::raw(""),
            CommandPaletteRow::Header(group) => Line::from(Span::styled(
                format!(" {}", group),
                modal_section_heading_style(&app.palette),
            )),
            CommandPaletteRow::Command(idx, command) => {
                let selected = *idx == selected;
                let row_style = if selected {
                    Style::default().bg(app.palette.accent)
                } else {
                    Style::default()
                };
                let title_style = if selected {
                    Style::default()
                        .fg(panel_contrast_fg(&app.palette))
                        .bg(app.palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(app.palette.text)
                };
                let key_style = if selected {
                    Style::default()
                        .fg(panel_contrast_fg(&app.palette))
                        .bg(app.palette.accent)
                } else {
                    Style::default()
                        .fg(app.palette.mauve)
                        .add_modifier(Modifier::BOLD)
                };
                command_palette_command_line(
                    &command.title,
                    command.key_label.as_deref(),
                    list_width,
                    title_style,
                    row_style,
                    key_style,
                )
            }
        })
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(lines), scroll_area.body);

    if let Some(track) = scroll_area.track {
        render_scrollbar(
            frame,
            metrics,
            track,
            app.palette.surface_dim,
            app.palette.overlay0,
            "▐",
        );
    }
}

enum CommandPaletteRow<'a> {
    Spacer,
    Header(&'static str),
    Command(usize, &'a CommandPaletteCommand),
}

fn command_palette_rows(commands: &[CommandPaletteCommand]) -> Vec<CommandPaletteRow<'_>> {
    let mut rows = Vec::new();
    let mut last_group = None;

    for (idx, command) in commands.iter().enumerate() {
        if last_group != Some(command.group) {
            if last_group.is_some() {
                rows.push(CommandPaletteRow::Spacer);
            }
            rows.push(CommandPaletteRow::Header(command.group));
            last_group = Some(command.group);
        }
        rows.push(CommandPaletteRow::Command(idx, command));
    }

    rows
}

fn command_palette_command_line<'a>(
    title: &str,
    key_label: Option<&str>,
    width: usize,
    title_style: Style,
    row_style: Style,
    key_style: Style,
) -> Line<'a> {
    let text = format!("  {title}");
    let title_len = text.chars().count();
    let Some(key_label) = key_label else {
        return Line::from(Span::styled(pad_right(text, width), title_style));
    };

    let key_len = key_label.chars().count();
    if title_len + key_len + 1 >= width {
        return Line::from(vec![Span::styled(text, title_style)]);
    }

    let gap = width - title_len - key_len;
    Line::from(vec![
        Span::styled(text, title_style),
        Span::styled(" ".repeat(gap), row_style),
        Span::styled(key_label.to_string(), key_style),
    ])
}

fn pad_right(text: String, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text
    } else {
        format!("{text}{}", " ".repeat(width - len))
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

    use super::*;

    #[test]
    fn command_palette_renders_one_close_affordance_and_run_action() {
        let app = AppState::test_new();
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");

        terminal
            .draw(|frame| render_command_palette_overlay(&app, frame))
            .expect("render command palette");

        let text = buffer_text(terminal.backend().buffer(), 100, 24);
        assert_eq!(text.matches("esc close").count(), 1);
        assert!(text.contains("↵ run"));
        assert!(text.contains("scroll wheel ↑↓"));
        assert!(text.contains("jump pgup / pgdn"));

        let popup = crate::ui::centered_popup_rect(Rect::new(0, 0, 100, 24), 76, 18)
            .expect("command palette popup");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let gap_y = inner.y + inner.height.saturating_sub(3);
        assert!(buffer_row_text(terminal.backend().buffer(), inner, gap_y)
            .trim()
            .is_empty());
    }

    fn buffer_row_text(buffer: &Buffer, area: Rect, y: u16) -> String {
        let mut text = String::new();
        for x in area.x..area.x + area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text
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
}
