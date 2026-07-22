use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{
    command_palette::{
        command_palette_filtered_commands, command_palette_filtered_commands_for_view,
        CommandPaletteAction, CommandPaletteCommand,
    },
    view_state::ClientViewState,
    AppState,
};

use super::text::display_width;

use super::{
    scrollbar::render_scrollbar,
    widgets::{
        action_button_row_rects, modal_frame_areas, modal_hint_line_count, modal_option_line,
        modal_section_heading_style, panel_contrast_fg, primary_action_style, render_action_button,
        render_modal_divider, render_modal_frame, render_modal_text_input, ActionButtonSpec,
        ModalFrameSpec, ModalListGeometry,
    },
};

const COMMAND_PALETTE_KEY_HINT_RIGHT_PADDING: usize = 1;
const COMMAND_PALETTE_HINTS: &[(&str, &str)] = &[("scroll", "wheel ↑↓"), ("jump", "pgup / pgdn")];

fn command_palette_frame_spec(area: Rect) -> ModalFrameSpec<'static> {
    let popup_width = 76.min(area.width.saturating_sub(4));
    let inner_width = popup_width.saturating_sub(2);
    ModalFrameSpec {
        title: "command palette",
        width: 76,
        height: 19 + modal_hint_line_count(inner_width, COMMAND_PALETTE_HINTS, 2),
        header_rows: 1,
        footer_hints: COMMAND_PALETTE_HINTS,
        footer_max_rows: 2,
        gap: 1,
        actions_rows: 1,
        show_close: true,
    }
}

pub(crate) fn command_palette_popup_rect(area: Rect) -> Option<Rect> {
    modal_frame_areas(area, command_palette_frame_spec(area)).map(|frame| frame.popup)
}

pub(crate) fn command_palette_inner_rect(area: Rect) -> Option<Rect> {
    modal_frame_areas(area, command_palette_frame_spec(area)).map(|frame| frame.inner)
}

fn command_palette_content_rows(content: Rect) -> Option<[Rect; 3]> {
    if content.height < 4 || content.width < 20 {
        return None;
    }
    Some(
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .areas::<3>(content),
    )
}

pub(crate) fn command_palette_list_geometry(
    area: Rect,
    total_rows: usize,
    scroll: usize,
) -> Option<ModalListGeometry> {
    let frame = modal_frame_areas(area, command_palette_frame_spec(area))?;
    let rows = command_palette_content_rows(frame.content)?;
    Some(ModalListGeometry::new(rows[2], total_rows, scroll))
}

pub(crate) fn command_palette_button_rects(inner: Rect) -> (Rect, Rect) {
    let footer_rows = modal_hint_line_count(inner.width, COMMAND_PALETTE_HINTS, 2);
    let stack = super::widgets::modal_stack_areas(inner, 1, footer_rows, 1, 1);
    let actions = stack.actions.unwrap_or_default();
    let rects = action_button_row_rects(
        actions,
        &[ActionButtonSpec {
            hint: Some("↵"),
            label: "run",
        }],
        2,
        0,
    );
    let close = super::widgets::modal_close_button_rect(stack.header);
    (rects[0], close)
}

pub(super) fn render_command_palette_overlay(app: &AppState, frame: &mut Frame) {
    let commands = command_palette_filtered_commands(app);
    render_command_palette_overlay_from(
        app,
        frame,
        app.screen_rect(),
        &app.command_palette,
        commands,
    );
}

pub(super) fn render_command_palette_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let commands = command_palette_filtered_commands_for_view(app, view);
    render_command_palette_overlay_from(
        app,
        frame,
        view.screen_rect(),
        &view.command_palette,
        commands,
    );
}

fn render_command_palette_overlay_from(
    app: &AppState,
    frame: &mut Frame,
    screen: Rect,
    palette_state: &crate::app::state::CommandPaletteState,
    commands: Vec<CommandPaletteCommand>,
) {
    super::dim_background(frame, screen);

    let area = if screen.width >= 4 && screen.height >= 4 {
        screen
    } else {
        frame.area()
    };
    let spec = command_palette_frame_spec(area);
    let Some(frame_areas) = render_modal_frame(frame, area, &app.palette, spec) else {
        return;
    };
    let run_rect = action_button_row_rects(
        frame_areas.actions.unwrap_or_default(),
        &[ActionButtonSpec {
            hint: Some("↵"),
            label: "run",
        }],
        2,
        0,
    )[0];
    render_action_button(
        frame,
        run_rect,
        Some("↵"),
        "run",
        primary_action_style(&app.palette),
    );

    let Some(rows) = command_palette_content_rows(frame_areas.content) else {
        return;
    };
    render_modal_text_input(frame, rows[0], &palette_state.query, &app.palette);
    render_modal_divider(frame, rows[1], &app.palette);

    if commands.is_empty() {
        frame.render_widget(
            Paragraph::new(" no commands").style(Style::default().fg(app.palette.overlay1)),
            rows[2],
        );
        return;
    }

    let selected = palette_state.list.visible();
    let palette_rows = command_palette_rows(&commands);
    let Some(list) = command_palette_list_geometry(area, palette_rows.len(), palette_state.scroll)
    else {
        return;
    };
    let visible_range = list.visible_range();
    let metrics = list.metrics();
    let scroll_area = list.scroll_area;
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
                let selected = Some(*idx) == selected;
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
                match &command.action {
                    CommandPaletteAction::SwitchGroup(group_idx) => {
                        command_palette_group_command_line(
                            app,
                            *group_idx,
                            &command.title,
                            command.key_label.as_deref(),
                            list_width,
                            title_style,
                            row_style,
                            key_style,
                            selected,
                        )
                    }
                    _ => command_palette_command_line(
                        &command.title,
                        command.key_label.as_deref(),
                        list_width,
                        title_style,
                        row_style,
                        key_style,
                    ),
                }
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
    let metadata = key_label
        .map(|key| vec![Span::styled(key.to_string(), key_style)])
        .unwrap_or_default();
    modal_option_line(title, metadata, width, title_style, row_style)
}

fn command_palette_group_command_line<'a>(
    app: &AppState,
    group_idx: usize,
    title: &str,
    key_label: Option<&str>,
    width: usize,
    title_style: Style,
    row_style: Style,
    key_style: Style,
    selected: bool,
) -> Line<'a> {
    let Some(group) = app.groups.get(group_idx) else {
        return command_palette_command_line(
            title,
            key_label,
            width,
            title_style,
            row_style,
            key_style,
        );
    };
    let prefix = "  switch to group: ";
    let group_label = format!("{} {}", group.icon, group.name);
    if title != format!("switch to group: {group_label}") {
        return command_palette_command_line(
            title,
            key_label,
            width,
            title_style,
            row_style,
            key_style,
        );
    }

    let full_len = display_width(prefix) + display_width(&group_label);
    let Some(key_label) = key_label else {
        if full_len >= width {
            return command_palette_command_line(
                title,
                None,
                width,
                title_style,
                row_style,
                key_style,
            );
        }
        let group_style = if selected {
            title_style
        } else {
            title_style
                .fg(app.group_accent_color(group_idx))
                .add_modifier(Modifier::BOLD)
        };
        return Line::from(vec![
            Span::styled(prefix.to_string(), title_style),
            Span::styled(group_label, group_style),
            Span::styled(" ".repeat(width - full_len), title_style),
        ]);
    };

    let key_len = display_width(key_label);
    if full_len + key_len + 1 >= width {
        return command_palette_command_line(
            title,
            Some(key_label),
            width,
            title_style,
            row_style,
            key_style,
        );
    }
    let group_style = if selected {
        title_style
    } else {
        title_style
            .fg(app.group_accent_color(group_idx))
            .add_modifier(Modifier::BOLD)
    };
    let gap = width - full_len - key_len;
    Line::from(vec![
        Span::styled(prefix.to_string(), title_style),
        Span::styled(group_label, group_style),
        Span::styled(" ".repeat(gap), row_style),
        Span::styled(key_label.to_string(), key_style),
    ])
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

    #[test]
    fn command_palette_keeps_close_and_run_actions_on_a_narrow_terminal() {
        let app = AppState::test_new();
        let backend = TestBackend::new(32, 12);
        let mut terminal = Terminal::new(backend).expect("test backend");

        terminal
            .draw(|frame| render_command_palette_overlay(&app, frame))
            .expect("render narrow command palette");

        let text = buffer_text(terminal.backend().buffer(), 32, 12);
        assert!(text.contains("close"));
        assert!(text.contains("run"));
    }

    #[test]
    fn command_palette_group_command_uses_group_accent() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Green));
        app.command_palette.query = "switch group".to_string();

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_command_palette_overlay(&app, frame))
            .expect("render command palette");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_symbol(buffer, 100, 24, "■").expect("group icon");
        assert_eq!(
            buffer[(x, y)].style().fg,
            Some(app.group_accent_color(group_idx))
        );
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
