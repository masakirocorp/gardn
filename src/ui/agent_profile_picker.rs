use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{
    agent_profile_picker::{agent_profile_picker_filtered_entries, AgentProfilePickerEntry},
    AppState,
};

use super::{
    scrollbar::render_scrollbar,
    widgets::{
        action_button_row_rects, modal_section_heading_style, panel_contrast_fg,
        primary_action_style, render_action_button, render_modal_header_bar,
        render_modal_hint_line, render_modal_shell, render_modal_subtitle, render_modal_text_input,
        ActionButtonSpec,
    },
};

const AGENT_PROFILE_PICKER_KEY_HINT_RIGHT_PADDING: usize = 1;

pub(crate) fn agent_profile_picker_button_rects(inner: Rect) -> (Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[ActionButtonSpec {
            hint: Some("↵"),
            label: "start",
        }],
        2,
        inner.height.saturating_sub(1),
    );
    let close =
        super::widgets::modal_close_button_rect(Rect::new(inner.x, inner.y, inner.width, 1));
    (rects[0], close)
}

pub(crate) fn agent_profile_picker_popup_rect(area: Rect) -> Option<Rect> {
    super::centered_popup_rect(area, 76, 20)
}

pub(crate) fn agent_profile_picker_inner_rect(area: Rect) -> Option<Rect> {
    let popup = agent_profile_picker_popup_rect(area)?;
    Some(Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    ))
}

pub(crate) fn agent_profile_picker_list_area(area: Rect) -> Option<Rect> {
    let inner = agent_profile_picker_inner_rect(area)?;
    if inner.height < 8 || inner.width < 20 {
        return None;
    }

    Some(Rect::new(
        inner.x,
        inner.y + 5,
        inner.width,
        inner.height.saturating_sub(7),
    ))
}

pub(super) fn render_agent_profile_picker_overlay(app: &AppState, frame: &mut Frame) {
    super::dim_background(frame, frame.area());

    let Some(inner) = render_modal_shell(frame, frame.area(), 76, 20, &app.palette) else {
        return;
    };
    if inner.height < 8 || inner.width < 20 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<8>(inner);

    render_modal_header_bar(frame, rows[0], "new agent", &app.palette, true);
    render_modal_subtitle(
        frame,
        rows[1],
        "choose an agent profile for this group",
        &app.palette,
    );

    let input = Rect::new(rows[3].x, rows[3].y, rows[3].width, 1);
    render_modal_text_input(frame, input, &app.agent_profile_picker.query, &app.palette);

    render_modal_hint_line(
        frame,
        rows[6],
        &app.palette,
        &[
            ("scroll", "wheel ↑↓"),
            ("jump", "pgup / pgdn"),
            ("favorite", "ctrl+f"),
        ],
    );

    let (start_rect, _) = agent_profile_picker_button_rects(inner);
    render_action_button(
        frame,
        start_rect,
        Some("↵"),
        "start",
        primary_action_style(&app.palette),
    );

    let entries = agent_profile_picker_filtered_entries(app);
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(" no agent profiles").style(Style::default().fg(app.palette.overlay1)),
            rows[5],
        );
        return;
    }

    let selected = app
        .agent_profile_picker
        .selected
        .min(entries.len().saturating_sub(1));
    let picker_rows = agent_profile_picker_rows(&entries);
    let viewport = crate::ui::ModalListViewport::new(
        picker_rows.len(),
        rows[5].height as usize,
        app.agent_profile_picker.scroll,
    );
    let visible_range = viewport.visible_range();
    let metrics = viewport.metrics();
    let scroll_area = viewport.scroll_area(rows[5]);
    let list_width = (scroll_area.body.width as usize)
        .saturating_sub(AGENT_PROFILE_PICKER_KEY_HINT_RIGHT_PADDING);

    let lines = picker_rows[visible_range]
        .iter()
        .map(|row| match row {
            AgentProfilePickerRow::Spacer => Line::raw(""),
            AgentProfilePickerRow::Header(section) => Line::from(Span::styled(
                format!(" {}", section),
                modal_section_heading_style(&app.palette),
            )),
            AgentProfilePickerRow::Entry(idx, entry) => {
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
                agent_profile_picker_entry_line(&entry.name, list_width, title_style, row_style)
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

enum AgentProfilePickerRow<'a> {
    Spacer,
    Header(&'static str),
    Entry(usize, &'a AgentProfilePickerEntry),
}

fn agent_profile_picker_rows(
    entries: &[AgentProfilePickerEntry],
) -> Vec<AgentProfilePickerRow<'_>> {
    let mut rows = Vec::new();
    let mut last_section = None;

    for (idx, entry) in entries.iter().enumerate() {
        if last_section != Some(entry.section) {
            if last_section.is_some() {
                rows.push(AgentProfilePickerRow::Spacer);
            }
            rows.push(AgentProfilePickerRow::Header(entry.section));
            last_section = Some(entry.section);
        }
        rows.push(AgentProfilePickerRow::Entry(idx, entry));
    }

    rows
}

fn agent_profile_picker_entry_line<'a>(
    title: &str,
    width: usize,
    title_style: Style,
    row_style: Style,
) -> Line<'a> {
    let text = format!("  {title}");
    Line::from(Span::styled(
        pad_right(text, width),
        title_style.patch(row_style),
    ))
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
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    #[test]
    fn agent_profile_picker_uses_picker_copy() {
        let mut app = AppState::test_new();
        app.mode = crate::app::state::Mode::AgentProfilePicker;

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_agent_profile_picker_overlay(&app, frame))
            .expect("render agent picker");

        let text = buffer_text(terminal.backend().buffer(), 100, 24);
        assert!(text.contains("new agent"));
        assert!(text.contains("choose an agent profile for this group"));
        assert!(text.contains("favorite ctrl+f"));
        assert!(text.contains("↵ start"));
        assert!(!text.contains("command palette"));
        assert!(!text.contains("type to filter commands"));
        assert!(!text.contains("↵ run"));
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer, width: u16, height: u16) -> String {
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
