use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{
    agent_profile_picker::{
        agent_profile_picker_filtered_entries, agent_profile_picker_tab_label,
        AgentProfilePickerEntry, AGENT_PROFILE_PICKER_TABS,
    },
    AppState,
};

use super::{
    scrollbar::render_scrollbar,
    widgets::{
        action_button_row_rects, modal_hint_line_count, modal_section_heading_style,
        panel_contrast_fg, primary_action_style, render_action_button, render_modal_divider,
        render_modal_header_bar, render_modal_hint_lines, render_modal_shell,
        render_modal_subtitle, render_modal_text_input, ActionButtonSpec,
    },
};

const AGENT_PROFILE_PICKER_KEY_HINT_RIGHT_PADDING: usize = 1;
const AGENT_PROFILE_PICKER_HINTS: &[(&str, &str)] = &[
    ("quick start", "alt+1..9"),
    ("favorite", "ctrl+f"),
    ("default", "ctrl+d"),
    ("filter", "shift+←→"),
];

fn agent_profile_picker_hint_rows(inner_width: u16) -> u16 {
    modal_hint_line_count(inner_width, AGENT_PROFILE_PICKER_HINTS, 2)
}

fn agent_profile_picker_height(area: Rect) -> u16 {
    let popup_width = 60.min(area.width.saturating_sub(4));
    let inner_width = popup_width.saturating_sub(2);
    23 + agent_profile_picker_hint_rows(inner_width)
}
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
    super::centered_popup_rect(area, 60, agent_profile_picker_height(area))
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

pub(crate) fn agent_profile_picker_tab_hit_areas(app: &AppState, row: Rect) -> Vec<(usize, Rect)> {
    let (start, end) = agent_profile_picker_visible_tab_range(app, row.width);
    super::modal_tabs::tab_hit_areas(row, start, end, |idx| {
        agent_profile_picker_tab_width(AGENT_PROFILE_PICKER_TABS[idx])
    })
}

pub(crate) fn agent_profile_picker_tab_chevron_at(
    app: &AppState,
    row: Rect,
    col: u16,
) -> Option<usize> {
    let (start, end) = agent_profile_picker_visible_tab_range(app, row.width);
    super::modal_tabs::chevron_tab_at(
        AGENT_PROFILE_PICKER_TABS.len(),
        row,
        col,
        start,
        end,
        |idx| agent_profile_picker_tab_width(AGENT_PROFILE_PICKER_TABS[idx]),
    )
}

fn agent_profile_picker_visible_tab_range(app: &AppState, row_width: u16) -> (usize, usize) {
    let selected = AGENT_PROFILE_PICKER_TABS
        .iter()
        .position(|tab| *tab == app.agent_profile_picker.kind_filter)
        .unwrap_or(0);
    super::modal_tabs::visible_tab_range(
        AGENT_PROFILE_PICKER_TABS.len(),
        selected,
        row_width,
        |idx| agent_profile_picker_tab_width(AGENT_PROFILE_PICKER_TABS[idx]),
    )
}

fn agent_profile_picker_tab_width(tab: Option<crate::agent_profiles::AgentKind>) -> u16 {
    (agent_profile_picker_tab_label(tab).chars().count() as u16).saturating_add(2)
}

pub(crate) fn agent_profile_picker_list_area(area: Rect) -> Option<Rect> {
    let inner = agent_profile_picker_inner_rect(area)?;
    if inner.height < 13 || inner.width < 20 {
        return None;
    }

    let hint_rows = agent_profile_picker_hint_rows(inner.width);
    Some(Rect::new(
        inner.x,
        inner.y + 10,
        inner.width,
        inner.height.saturating_sub(12 + hint_rows),
    ))
}

pub(super) fn render_agent_profile_picker_overlay(app: &AppState, frame: &mut Frame) {
    super::dim_background(frame, frame.area());

    let Some(inner) = render_modal_shell(
        frame,
        frame.area(),
        60,
        agent_profile_picker_height(frame.area()),
        &app.palette,
    ) else {
        return;
    };
    if inner.height < 13 || inner.width < 20 {
        return;
    }

    let hint_rows = agent_profile_picker_hint_rows(inner.width);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(hint_rows),
        Constraint::Length(1),
    ])
    .areas::<14>(inner);

    render_modal_header_bar(frame, rows[0], "new agent", &app.palette, true);
    render_agent_profile_picker_filters(app, frame, rows[2]);
    render_modal_divider(frame, rows[3], &app.palette);
    render_agent_profile_picker_group_line(app, frame, rows[4]);
    render_modal_subtitle(
        frame,
        rows[5],
        "choose an agent profile for this group",
        &app.palette,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            " search",
            modal_section_heading_style(&app.palette),
        )),
        rows[7],
    );

    let input = Rect::new(rows[8].x, rows[8].y, rows[8].width, 1);
    render_modal_text_input(frame, input, &app.agent_profile_picker.query, &app.palette);

    render_modal_hint_lines(frame, rows[12], &app.palette, AGENT_PROFILE_PICKER_HINTS, 2);

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
            rows[10],
        );
        return;
    }

    let selected = app
        .agent_profile_picker
        .selected
        .min(entries.len().saturating_sub(1));
    let picker_rows = agent_profile_picker_rows(app, &entries);
    let viewport = crate::ui::ModalListViewport::new(
        picker_rows.len(),
        rows[10].height as usize,
        app.agent_profile_picker.scroll,
    );
    let visible_range = viewport.visible_range();
    let metrics = viewport.metrics();
    let scroll_area = viewport.scroll_area(rows[10]);
    let list_width = (scroll_area.body.width as usize)
        .saturating_sub(AGENT_PROFILE_PICKER_KEY_HINT_RIGHT_PADDING);
    let lines = picker_rows[visible_range]
        .iter()
        .map(|row| match row {
            AgentProfilePickerRow::Spacer => Line::raw(""),
            AgentProfilePickerRow::Header(section) => Line::from(Span::styled(
                format!(" {section}"),
                modal_section_heading_style(&app.palette),
            )),
            AgentProfilePickerRow::Entry(idx, entry, shortcut, default) => {
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
                let shortcut_style = if selected {
                    Style::default()
                        .fg(panel_contrast_fg(&app.palette))
                        .bg(app.palette.accent)
                } else {
                    Style::default().fg(app.palette.text)
                };
                agent_profile_picker_entry_line(
                    &entry.name,
                    *shortcut,
                    *default,
                    list_width,
                    title_style,
                    shortcut_style,
                    row_style,
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

fn render_agent_profile_picker_filters(app: &AppState, frame: &mut Frame, row: Rect) {
    let label_width = 7;
    frame.render_widget(
        Paragraph::new(Span::styled(
            "filter ",
            Style::default().fg(app.palette.overlay0),
        )),
        row,
    );
    let chip_row = Rect::new(
        row.x.saturating_add(label_width),
        row.y,
        row.width.saturating_sub(label_width),
        row.height,
    );
    let p = &app.palette;
    let (start, end) = agent_profile_picker_visible_tab_range(app, chip_row.width);
    let mut spans = Vec::new();

    if start > 0 {
        spans.push(Span::styled("‹ ", Style::default().fg(p.overlay0)));
    }

    for (visible_idx, tab) in AGENT_PROFILE_PICKER_TABS[start..end]
        .iter()
        .copied()
        .enumerate()
    {
        if visible_idx > 0 {
            spans.push(Span::raw(" "));
        }
        let selected = tab == app.agent_profile_picker.kind_filter;
        let style = if selected {
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.overlay1)
        };
        spans.push(Span::styled(" ", style));
        spans.push(Span::styled(agent_profile_picker_tab_label(tab), style));
        spans.push(Span::styled(" ", style));
    }

    if end < AGENT_PROFILE_PICKER_TABS.len() {
        spans.push(Span::styled(" ›", Style::default().fg(p.overlay0)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), chip_row);
}

fn agent_profile_picker_group_idx(app: &AppState) -> Option<usize> {
    app.workspaces
        .get(app.agent_profile_picker.ws_idx)
        .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
}

fn render_agent_profile_picker_group_line(app: &AppState, frame: &mut Frame, area: Rect) {
    let (icon, name, color) = agent_profile_picker_group_idx(app)
        .and_then(|group_idx| {
            app.groups.get(group_idx).map(|group| {
                (
                    group.icon.as_str(),
                    group.name.as_str(),
                    app.group_accent_color(group_idx),
                )
            })
        })
        .unwrap_or(("•", "current", app.palette.accent));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " group: ",
                Style::default()
                    .fg(app.palette.overlay1)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{icon} {name}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ])),
        area,
    );
}

enum AgentProfilePickerRow<'a> {
    Spacer,
    Header(&'static str),
    Entry(usize, &'a AgentProfilePickerEntry, Option<usize>, bool),
}

fn agent_profile_picker_rows<'a>(
    app: &AppState,
    entries: &'a [AgentProfilePickerEntry],
) -> Vec<AgentProfilePickerRow<'a>> {
    let mut rows = Vec::new();
    let mut last_section = None;
    let default_profile_id = agent_profile_picker_group_idx(app)
        .and_then(|group_idx| app.groups.get(group_idx))
        .and_then(|group| group.default_agent_profile_id.as_deref());
    let mut favorite_shortcut = 1;

    for (idx, entry) in entries.iter().enumerate() {
        if last_section != Some(entry.section) {
            if last_section.is_some() {
                rows.push(AgentProfilePickerRow::Spacer);
            }
            rows.push(AgentProfilePickerRow::Header(entry.section));
            last_section = Some(entry.section);
        }
        let shortcut = if entry.section == "favorites" && favorite_shortcut <= 9 {
            let shortcut = Some(favorite_shortcut);
            favorite_shortcut += 1;
            shortcut
        } else {
            None
        };
        rows.push(AgentProfilePickerRow::Entry(
            idx,
            entry,
            shortcut,
            default_profile_id == Some(entry.profile_id.as_str()),
        ));
    }

    rows
}

fn agent_profile_picker_entry_line<'a>(
    title: &str,
    shortcut: Option<usize>,
    is_default: bool,
    width: usize,
    title_style: Style,
    shortcut_style: Style,
    row_style: Style,
) -> Line<'a> {
    let title_text = format!("  {title}");
    let meta_text = match (is_default, shortcut) {
        (true, Some(shortcut)) => Some(format!("default  alt+{shortcut}")),
        (true, None) => Some("default".to_string()),
        (false, Some(shortcut)) => Some(format!("alt+{shortcut}")),
        (false, None) => None,
    };
    let Some(meta_text) = meta_text else {
        return Line::from(Span::styled(
            pad_right(title_text, width),
            title_style.patch(row_style),
        ));
    };

    let title_len = title_text.chars().count();
    let meta_len = meta_text.chars().count();
    if title_len + meta_len >= width {
        return Line::from(vec![
            Span::styled(title_text, title_style.patch(row_style)),
            Span::styled(meta_text, shortcut_style.patch(row_style)),
        ]);
    }

    let gap = width - title_len - meta_len;
    Line::from(vec![
        Span::styled(
            format!("{title_text}{}", " ".repeat(gap)),
            title_style.patch(row_style),
        ),
        Span::styled(meta_text, shortcut_style.patch(row_style)),
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
    use ratatui::{backend::TestBackend, Terminal};

    use super::*;

    #[test]
    fn agent_profile_picker_uses_picker_copy() {
        let mut app = AppState::test_new();
        app.mode = crate::app::state::Mode::AgentProfilePicker;
        app.groups[0].name = "Work".to_string();
        app.groups[0].icon = "■".to_string();
        app.set_group_accent(0, Some(crate::config::TerminalAccent::Red));
        app.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:shell-builtin".to_string()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "shell-builtin".to_string(),
                    name: "shell builtin".to_string(),
                    kind: crate::agent_profiles::AgentKind::Omp,
                    command: "sh".to_string(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        app.groups[0]
            .favorite_agent_profile_ids
            .push("user:shell-builtin".to_string());
        app.workspaces = vec![crate::workspace::Workspace::test_new("test")];

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_agent_profile_picker_overlay(&app, frame))
            .expect("render agent picker");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 100, 24);
        assert!(text.contains("new agent"));
        assert!(text.contains("all"));
        assert!(text.contains("pi"));
        assert!(text.contains("omp"));
        assert!(text.contains("group:"));
        assert!(text.contains("Work"));
        assert!(text.contains("choose an agent profile for this group"));
        assert!(text.contains("quick start alt+1..9"));
        assert!(text.contains("favorite ctrl+f"));
        assert!(text.contains("filter shift+←→"));
        assert!(text.contains("search"));
        assert!(text.contains("shell builtin"));
        assert!(text.contains("alt+1"));
        assert!(text.contains("↵ start"));
        let (group_icon_y, group_icon_x) = find_text_cell(&text, "■").expect("group icon");
        assert_eq!(
            buffer[(group_icon_x, group_icon_y)].style().fg,
            Some(app.group_accent_color(0))
        );
        assert!(!text.contains("command palette"));
        assert!(!text.contains("type to filter commands"));
        assert!(!text.contains("↵ run"));
    }

    fn find_text_cell(text: &str, needle: &str) -> Option<(u16, u16)> {
        text.lines().enumerate().find_map(|(y, line)| {
            let byte_x = line.find(needle)?;
            let cell_x = line[..byte_x].chars().count();
            Some((y as u16, cell_x as u16))
        })
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
