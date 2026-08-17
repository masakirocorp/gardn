use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{
    agent_profile_picker::{
        agent_profile_picker_filtered_entries, agent_profile_picker_filtered_entries_for_picker,
        agent_profile_picker_tab_label, AgentProfilePickerEntry, AGENT_PROFILE_PICKER_TABS,
    },
    view_state::ClientViewState,
    AppState,
};

use super::text::display_width_u16;

use super::{
    scrollbar::render_scrollbar,
    widgets::{
        action_button_row_rects, modal_frame_areas, modal_hint_line_count, modal_option_line,
        modal_section_heading_style, panel_contrast_fg, primary_action_style, render_action_button,
        render_modal_divider, render_modal_frame, render_modal_subtitle, render_modal_text_input,
        ActionButtonSpec, ModalFrameSpec, ModalListGeometry,
    },
};

const AGENT_PROFILE_PICKER_KEY_HINT_RIGHT_PADDING: usize = 1;
const AGENT_PROFILE_PICKER_HINTS: &[(&str, &str)] = &[
    ("Quick Start", "Alt+1..9"),
    ("Favorite", "Ctrl+F"),
    ("Default", "Ctrl+D"),
    ("Filter", "Shift+←→"),
];

fn agent_profile_picker_hint_rows(inner_width: u16) -> u16 {
    modal_hint_line_count(inner_width, AGENT_PROFILE_PICKER_HINTS, 2)
}

fn agent_profile_picker_frame_spec(area: Rect) -> ModalFrameSpec<'static> {
    let popup_width = 60.min(area.width.saturating_sub(4));
    let inner_width = popup_width.saturating_sub(2);
    ModalFrameSpec {
        title: "New Agent",
        width: 60,
        height: 23 + agent_profile_picker_hint_rows(inner_width),
        header_rows: 1,
        footer_hints: AGENT_PROFILE_PICKER_HINTS,
        footer_max_rows: 2,
        gap: 1,
        actions_rows: 1,
        show_close: true,
    }
}

pub(crate) fn agent_profile_picker_button_rects(inner: Rect) -> (Rect, Rect) {
    let hint_rows = agent_profile_picker_hint_rows(inner.width);
    let stack = super::widgets::modal_stack_areas(inner, 1, hint_rows, 1, 1);
    let actions = stack.actions.unwrap_or_default();
    let rects = action_button_row_rects(
        actions,
        &[ActionButtonSpec {
            hint: Some("↵"),
            label: "Start",
        }],
        2,
        0,
    );
    let close = super::widgets::modal_close_button_rect(stack.header);
    (rects[0], close)
}

pub(crate) fn agent_profile_picker_popup_rect(area: Rect) -> Option<Rect> {
    modal_frame_areas(area, agent_profile_picker_frame_spec(area)).map(|frame| frame.popup)
}

pub(crate) fn agent_profile_picker_inner_rect(area: Rect) -> Option<Rect> {
    modal_frame_areas(area, agent_profile_picker_frame_spec(area)).map(|frame| frame.inner)
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
    display_width_u16(agent_profile_picker_tab_label(tab)).saturating_add(2)
}

pub(crate) fn agent_profile_picker_list_geometry(
    area: Rect,
    total_rows: usize,
    scroll: usize,
) -> Option<ModalListGeometry> {
    let frame = modal_frame_areas(area, agent_profile_picker_frame_spec(area))?;
    if frame.inner.height < 13 || frame.inner.width < 20 {
        return None;
    }
    let rows = agent_profile_picker_content_rows(frame.content);
    Some(ModalListGeometry::new(rows[8], total_rows, scroll))
}

fn agent_profile_picker_content_rows(content: Rect) -> [Rect; 9] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas::<9>(content)
}

pub(super) fn render_agent_profile_picker_overlay(app: &AppState, frame: &mut Frame) {
    let entries = agent_profile_picker_filtered_entries(app);
    render_agent_profile_picker_overlay_from(
        app,
        frame,
        app.screen_rect(),
        &app.agent_profile_picker,
        entries,
    );
}

pub(super) fn render_agent_profile_picker_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let entries = agent_profile_picker_filtered_entries_for_picker(app, &view.agent_profile_picker);
    render_agent_profile_picker_overlay_from(
        app,
        frame,
        view.screen_rect(),
        &view.agent_profile_picker,
        entries,
    );
}

fn render_agent_profile_picker_overlay_from(
    app: &AppState,
    frame: &mut Frame,
    screen: Rect,
    picker: &crate::app::state::AgentProfilePickerState,
    entries: Vec<AgentProfilePickerEntry>,
) {
    super::dim_background(frame, screen);

    let area = if screen.width >= 4 && screen.height >= 4 {
        screen
    } else {
        frame.area()
    };
    let palette = app.palette_for_workspace(picker.ws_idx);
    let spec = agent_profile_picker_frame_spec(area);
    let Some(frame_areas) = render_modal_frame(frame, area, &palette, spec) else {
        return;
    };
    let inner = frame_areas.inner;
    if inner.height < 13 || inner.width < 20 {
        return;
    }

    let rows = agent_profile_picker_content_rows(frame_areas.content);
    render_agent_profile_picker_filters_for_picker(picker, frame, rows[0], &palette);
    render_modal_divider(frame, rows[1], &palette);
    render_agent_profile_picker_group_line_for_picker(app, picker, frame, rows[2], &palette);
    render_modal_subtitle(
        frame,
        rows[3],
        "Choose an agent profile for this group",
        &palette,
    );

    frame.render_widget(
        Paragraph::new(Span::styled(
            " Search",
            modal_section_heading_style(&palette),
        )),
        rows[5],
    );

    render_modal_text_input(frame, rows[6], &picker.query, &palette);
    let start_rect = action_button_row_rects(
        frame_areas.actions.unwrap_or_default(),
        &[ActionButtonSpec {
            hint: Some("↵"),
            label: "Start",
        }],
        2,
        0,
    )[0];

    render_action_button(
        frame,
        start_rect,
        Some("↵"),
        "Start",
        primary_action_style(&palette),
    );

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(" No Agent Profiles").style(Style::default().fg(palette.overlay1)),
            rows[8],
        );
        return;
    }

    let selected = picker.list.visible().filter(|idx| *idx < entries.len());
    let picker_rows = agent_profile_picker_rows_for_picker(app, picker, &entries);
    let Some(list) = agent_profile_picker_list_geometry(area, picker_rows.len(), picker.scroll)
    else {
        return;
    };
    let visible_range = list.visible_range();
    let metrics = list.metrics();
    let scroll_area = list.scroll_area;
    let list_width = (scroll_area.body.width as usize)
        .saturating_sub(AGENT_PROFILE_PICKER_KEY_HINT_RIGHT_PADDING);
    let lines = picker_rows[visible_range]
        .iter()
        .map(|row| match row {
            AgentProfilePickerRow::Spacer => Line::raw(""),
            AgentProfilePickerRow::Header(section) => Line::from(Span::styled(
                format!(" {section}"),
                modal_section_heading_style(&palette),
            )),
            AgentProfilePickerRow::Entry(idx, entry, shortcut, default) => {
                let selected = selected == Some(*idx);
                let row_style = if selected {
                    Style::default().bg(palette.accent)
                } else {
                    Style::default()
                };
                let title_style = if selected {
                    Style::default()
                        .fg(panel_contrast_fg(&palette))
                        .bg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.text)
                };
                let metadata_style = if selected {
                    Style::default()
                        .fg(panel_contrast_fg(&palette))
                        .bg(palette.accent)
                } else if entry.integration_warning.is_some() {
                    Style::default().fg(palette.yellow)
                } else {
                    Style::default().fg(palette.overlay0)
                };
                let shortcut_style = if selected {
                    metadata_style
                } else {
                    Style::default()
                        .fg(palette.mauve)
                        .add_modifier(Modifier::BOLD)
                };
                agent_profile_picker_entry_line(
                    &entry.name,
                    entry.integration_badge,
                    *shortcut,
                    *default,
                    list_width,
                    title_style,
                    metadata_style,
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
            palette.surface_dim,
            palette.overlay0,
            "▐",
        );
    }
}

fn render_agent_profile_picker_filters_for_picker(
    picker: &crate::app::state::AgentProfilePickerState,
    frame: &mut Frame,
    row: Rect,
    p: &crate::app::state::Palette,
) {
    let label_width = 7;
    frame.render_widget(
        Paragraph::new(Span::styled("Filter ", Style::default().fg(p.overlay0))),
        row,
    );
    let chip_row = Rect::new(
        row.x.saturating_add(label_width),
        row.y,
        row.width.saturating_sub(label_width),
        row.height,
    );

    let (start, end) = agent_profile_picker_visible_tab_range_for_picker(picker, chip_row.width);
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
        let selected = tab == picker.kind_filter;
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

fn agent_profile_picker_visible_tab_range_for_picker(
    picker: &crate::app::state::AgentProfilePickerState,
    row_width: u16,
) -> (usize, usize) {
    let selected = AGENT_PROFILE_PICKER_TABS
        .iter()
        .position(|tab| *tab == picker.kind_filter)
        .unwrap_or(0);
    super::modal_tabs::visible_tab_range(
        AGENT_PROFILE_PICKER_TABS.len(),
        selected,
        row_width,
        |idx| agent_profile_picker_tab_width(AGENT_PROFILE_PICKER_TABS[idx]),
    )
}

fn agent_profile_picker_group_idx_for_picker(
    app: &AppState,
    picker: &crate::app::state::AgentProfilePickerState,
) -> Option<usize> {
    app.workspaces
        .get(picker.ws_idx)
        .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
}

fn render_agent_profile_picker_group_line_for_picker(
    app: &AppState,
    picker: &crate::app::state::AgentProfilePickerState,
    frame: &mut Frame,
    area: Rect,
    palette: &crate::app::state::Palette,
) {
    let (icon, name, color) = agent_profile_picker_group_idx_for_picker(app, picker)
        .and_then(|group_idx| {
            app.groups.get(group_idx).map(|group| {
                (
                    group.icon.as_str(),
                    group.name.as_str(),
                    app.group_accent_color(group_idx),
                )
            })
        })
        .unwrap_or(("•", "Current", palette.accent));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " Group: ",
                Style::default()
                    .fg(palette.overlay1)
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

fn agent_profile_picker_rows_for_picker<'a>(
    app: &AppState,
    picker: &crate::app::state::AgentProfilePickerState,
    entries: &'a [AgentProfilePickerEntry],
) -> Vec<AgentProfilePickerRow<'a>> {
    let mut rows = Vec::new();
    let mut last_section = None;
    let default_profile_id = agent_profile_picker_group_idx_for_picker(app, picker)
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
        let shortcut = if entry.section == "Favorites" && favorite_shortcut <= 9 {
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
    integration_badge: Option<&str>,
    shortcut: Option<usize>,
    is_default: bool,
    width: usize,
    title_style: Style,
    metadata_style: Style,
    shortcut_style: Style,
    row_style: Style,
) -> Line<'a> {
    let mut metadata = Vec::new();
    if let Some(badge) = integration_badge {
        metadata.push(Span::styled(badge.to_string(), metadata_style));
    } else if is_default {
        metadata.push(Span::styled("Default", metadata_style));
    }
    if let Some(shortcut) = shortcut {
        if !metadata.is_empty() {
            metadata.push(Span::styled("  ", row_style));
        }
        metadata.push(Span::styled(format!("Alt+{shortcut}"), shortcut_style));
    }
    modal_option_line(title, metadata, width, title_style, row_style)
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
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Omp,
            label: "omp",
            command: "omp",
            available: true,
            path: std::path::PathBuf::from("/tmp/omh-test-omp"),
            state: crate::integration::IntegrationStatusKind::Current,
        }];
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
        assert!(text.contains("New Agent"));
        assert!(text.contains("All"));
        assert!(text.contains("Pi"));
        assert!(text.contains("Group:"));
        assert!(text.contains("Work"));
        assert!(text.contains("Choose an agent profile for this group"));
        assert!(text.contains("Quick Start Alt+1..9"));
        assert!(text.contains("Favorite Ctrl+F"));
        assert!(text.contains("Filter Shift+←→"));
        assert!(text.contains("Search"));
        assert!(text.contains("shell builtin"));
        assert!(text.contains("Alt+1"));
        assert!(text.contains("↵ Start"));
        let (group_icon_y, group_icon_x) = find_text_cell(&text, "■").expect("group icon");
        assert_eq!(
            buffer[(group_icon_x, group_icon_y)].style().fg,
            Some(app.group_accent_color(0))
        );
        assert!(!text.contains("Command Palette"));
        assert!(!text.contains("Type to Filter Commands"));
        assert!(!text.contains("↵ Run"));
    }

    #[test]
    fn agent_profile_row_preserves_unicode_title_and_right_metadata() {
        let line = agent_profile_picker_entry_line(
            "開発チーム向けの非常に長いエージェント名",
            Some("omp"),
            Some(1),
            false,
            28,
            Style::default(),
            Style::default(),
            Style::default(),
            Style::default(),
        );
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(super::super::text::display_width(&rendered), 28);
        assert!(rendered.ends_with("omp  Alt+1"));
        assert!(rendered.contains('…'));
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
