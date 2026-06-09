use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::scrollbar::render_scrollbar;
use super::widgets::{
    action_button_width, centered_popup_rect, modal_section_heading_style, modal_stack_areas,
    panel_contrast_fg, render_action_button, render_modal_description, render_modal_divider,
    render_modal_header_bar, render_modal_hint_line, render_panel_shell, secondary_action_style,
};
use crate::{
    app::{
        state::{normalize_theme_name, Palette, SettingsSection},
        AppState,
    },
    config::ThemeMode,
    settings_rows::{rows_for_section, visual_row_count, SettingsListRow, SettingsMarkerTone},
};
const GROUP_SETTINGS_SECTIONS: &[SettingsSection] = &[
    SettingsSection::Theme,
    SettingsSection::GroupGeneral,
    SettingsSection::GroupProfiles,
];

fn settings_title(app: &AppState) -> &'static str {
    if app.settings.group_settings_target.is_some() {
        "group settings"
    } else {
        "settings"
    }
}

fn settings_sections(app: &AppState) -> &'static [SettingsSection] {
    if app.settings.group_settings_target.is_some() {
        GROUP_SETTINGS_SECTIONS
    } else {
        SettingsSection::ALL
    }
}

fn settings_section_label(app: &AppState, section: SettingsSection) -> &'static str {
    if app.settings.group_settings_target.is_some() && section == SettingsSection::Theme {
        "appearance"
    } else {
        section.label()
    }
}

fn settings_tab_text(app: &AppState, section: SettingsSection) -> &'static str {
    settings_section_label(app, section)
}

fn settings_tab_width(app: &AppState, section: SettingsSection) -> u16 {
    let label_width = settings_tab_text(app, section).width() as u16;
    let badge_width = if app.settings_section_has_badge(section) {
        2
    } else {
        0
    };
    label_width + badge_width + 2
}

fn settings_tabs_width(
    app: &AppState,
    sections: &[SettingsSection],
    start: usize,
    end: usize,
) -> u16 {
    if start >= end {
        return 0;
    }

    let tab_width = sections[start..end]
        .iter()
        .copied()
        .map(|section| settings_tab_width(app, section))
        .sum::<u16>();
    let gaps = end.saturating_sub(start + 1) as u16;
    let edge_hints = u16::from(start > 0) * 2 + u16::from(end < sections.len()) * 2;
    tab_width + gaps + edge_hints
}

fn settings_visible_tab_range(app: &AppState, row_width: u16) -> (usize, usize) {
    let sections = settings_sections(app);
    if sections.is_empty() {
        return (0, 0);
    }
    let selected = sections
        .iter()
        .position(|section| *section == app.settings.section)
        .unwrap_or(0);
    let mut start = selected;
    let mut end = selected + 1;

    loop {
        let mut expanded = false;
        if start > 0 && settings_tabs_width(app, sections, start - 1, end) <= row_width {
            start -= 1;
            expanded = true;
        }
        if end < sections.len() && settings_tabs_width(app, sections, start, end + 1) <= row_width {
            end += 1;
            expanded = true;
        }
        if !expanded {
            break;
        }
    }

    (start, end)
}

pub(crate) fn settings_tab_hit_areas(app: &AppState, row: Rect) -> Vec<(SettingsSection, Rect)> {
    let sections = settings_sections(app);
    let (start, end) = settings_visible_tab_range(app, row.width);
    let mut x = row.x;
    if start > 0 {
        x = x.saturating_add(2);
    }

    let mut areas = Vec::new();
    for (visible_idx, section) in sections[start..end].iter().copied().enumerate() {
        if visible_idx > 0 {
            x = x.saturating_add(1);
        }
        let width = settings_tab_width(app, section);
        areas.push((section, Rect::new(x, row.y, width, 1)));
        x = x.saturating_add(width);
    }
    areas
}

pub(crate) fn settings_tab_chevron_at(
    app: &AppState,
    row: Rect,
    col: u16,
) -> Option<SettingsSection> {
    let sections = settings_sections(app);
    let (start, end) = settings_visible_tab_range(app, row.width);
    if start > 0 && col >= row.x && col < row.x.saturating_add(2) {
        return sections.get(start - 1).copied();
    }

    if end < sections.len() {
        let right_x = settings_tab_hit_areas(app, row)
            .last()
            .map(|(_, rect)| rect.x.saturating_add(rect.width))
            .unwrap_or(row.x);
        if col >= right_x && col < right_x.saturating_add(2) {
            return sections.get(end).copied();
        }
    }

    None
}

fn render_settings_tabs(app: &AppState, frame: &mut Frame, row: Rect) {
    let p = &app.palette;
    let sections = settings_sections(app);
    let (start, end) = settings_visible_tab_range(app, row.width);
    let mut spans = Vec::new();

    if start > 0 {
        spans.push(Span::styled("‹ ", Style::default().fg(p.overlay0)));
    }

    for (visible_idx, section) in sections[start..end].iter().copied().enumerate() {
        if visible_idx > 0 {
            spans.push(Span::raw(" "));
        }

        let selected = section == app.settings.section;
        let tab_style = if selected {
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.overlay1)
        };
        spans.push(Span::styled(" ", tab_style));
        if app.settings_section_has_badge(section) {
            let badge_style = if selected {
                tab_style
            } else {
                Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled("●", badge_style));
            spans.push(Span::styled(" ", tab_style));
        }
        spans.push(Span::styled(settings_tab_text(app, section), tab_style));
        spans.push(Span::styled(" ", tab_style));
    }

    if end < sections.len() {
        spans.push(Span::styled(" ›", Style::default().fg(p.overlay0)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn settings_section_title(app: &AppState, section: SettingsSection) -> &'static str {
    if app.settings.group_settings_target.is_some() && section == SettingsSection::Theme {
        "accent"
    } else if section == SettingsSection::Agents && settings_agents_editor_open(app) {
        if app.settings.pending_agent_profile_id.is_some() {
            "edit custom profile"
        } else {
            "add custom profile"
        }
    } else {
        match section {
            SettingsSection::Theme => "theme",
            SettingsSection::Layout => "layout",
            SettingsSection::Sound => "sound",
            SettingsSection::Toast => "toasts",
            SettingsSection::PaneLabels => "behavior",
            SettingsSection::Experiments => "experiments",
            SettingsSection::Agents => "agents",
            SettingsSection::Integrations => "agent integrations",
            SettingsSection::GroupGeneral => "general",
            SettingsSection::GroupProfiles => "profiles",
        }
    }
}

fn settings_section_description(app: &AppState, section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Theme if app.settings.group_settings_target.is_some() => {
            "choose an ANSI accent for this group, or inherit the global accent"
        }
        SettingsSection::Theme => {
            let mode = app
                .settings
                .pending_theme_mode
                .unwrap_or(app.global_theme_mode);
            let pending_light_theme = app
                .settings
                .pending_light_theme_name
                .as_deref()
                .unwrap_or(&app.global_light_theme_name);
            let pending_dark_theme = app
                .settings
                .pending_dark_theme_name
                .as_deref()
                .unwrap_or(&app.global_dark_theme_name);
            let system_source = mode == ThemeMode::System
                && normalize_theme_name(pending_light_theme) == "system"
                && normalize_theme_name(pending_dark_theme) == "system";
            if system_source {
                "follow terminal colors directly"
            } else {
                match mode {
                    ThemeMode::System => {
                        "choose custom palettes for automatic light and dark appearance"
                    }
                    ThemeMode::Light => "choose the palette hako uses in light appearance",
                    ThemeMode::Dark => "choose the palette hako uses in dark appearance",
                }
            }
        }
        SettingsSection::Layout => "set sidebar width bounds",
        SettingsSection::Sound => "choose whether hako plays terminal bell sounds",
        SettingsSection::Toast => "choose where command and agent notifications are delivered",
        SettingsSection::PaneLabels => {
            "control workspace prompts and terminal interaction defaults"
        }
        SettingsSection::Experiments => "enable behavior that is useful but still being proven",
        SettingsSection::Agents if settings_agents_editor_open(app) => {
            "name the profile and provide the command hako should launch"
        }
        SettingsSection::Agents => "create custom agent commands and manage global profile order",
        SettingsSection::Integrations => "install hooks so agents report state directly",
        SettingsSection::GroupGeneral => "rename this group or delete it",
        SettingsSection::GroupProfiles => {
            "choose which agent profiles are favorites for this group"
        }
    }
}

pub(crate) fn settings_agents_editor_back_button_rect(app: &AppState, area: Rect) -> Option<Rect> {
    (app.settings.section == SettingsSection::Agents && settings_agents_editor_open(app)).then(
        || {
            let width = action_button_width(None, "← back");
            Rect::new(area.x + area.width.saturating_sub(width), area.y, width, 1)
        },
    )
}

pub(crate) fn settings_section_list_rect(area: Rect) -> Rect {
    let [_, _, list_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(2),
    ])
    .areas::<3>(area);
    list_area
}

fn render_settings_section_intro(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    section: SettingsSection,
) -> Rect {
    let [desc_area, _, list_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(2),
    ])
    .areas::<3>(area);
    let [title_area, description_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas::<2>(desc_area);
    let title_width = settings_agents_editor_back_button_rect(app, title_area)
        .map(|back| back.x.saturating_sub(title_area.x).saturating_sub(1))
        .unwrap_or(title_area.width);
    render_modal_description(
        frame,
        Rect::new(title_area.x, title_area.y, title_width, title_area.height),
        settings_section_title(app, section),
        Style::default().fg(app.palette.accent),
    );
    if let Some(back) = settings_agents_editor_back_button_rect(app, title_area) {
        render_action_button(
            frame,
            back,
            None,
            "← back",
            secondary_action_style(&app.palette),
        );
    }
    render_modal_description(
        frame,
        description_area,
        settings_section_description(app, section),
        Style::default().fg(app.palette.overlay0),
    );
    list_area
}

pub(super) fn render_settings_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    use crate::app::state::SettingsSection;

    let group_settings = app.settings.group_settings_target.is_some();

    let p = &app.palette;
    let Some(popup) = centered_popup_rect(area, 92, 26) else {
        return;
    };

    super::dim_background(frame, area);

    let Some(inner) = render_panel_shell(frame, popup, p.accent, p.panel_bg) else {
        return;
    };
    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let stack = modal_stack_areas(inner, 4, 1, 0, 1);
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(stack.header);

    render_modal_header_bar(frame, header_rows[0], settings_title(app), p, true);
    render_settings_tabs(app, frame, header_rows[2]);
    render_modal_divider(frame, header_rows[3], p);

    let content_area = stack.content;

    match app.settings.section {
        SettingsSection::Theme
        | SettingsSection::Layout
        | SettingsSection::Sound
        | SettingsSection::Toast
        | SettingsSection::PaneLabels
        | SettingsSection::Experiments
        | SettingsSection::Agents
        | SettingsSection::GroupGeneral
        | SettingsSection::GroupProfiles => {
            render_settings_sectioned_toggle_list(app, frame, content_area);
        }
        SettingsSection::Integrations => render_settings_integrations(app, frame, content_area),
    }

    if let Some(footer_area) = stack.footer {
        let footer_rows = Layout::vertical([Constraint::Length(1)]).areas::<1>(footer_area);

        if app.settings.section == SettingsSection::Integrations {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("action", "space/↵"), ("section", "tab")],
            );
        } else if app.settings.section == SettingsSection::Agents {
            let hints = if settings_agents_editor_open(app) {
                &[("move", "↑↓"), ("action", "↵"), ("section", "tab")][..]
            } else {
                &[
                    ("move", "↑↓"),
                    ("edit/add", "↵"),
                    ("delete", "ctrl+d"),
                    ("reorder", "ctrl+↑↓"),
                    ("section", "tab"),
                ][..]
            };
            render_modal_hint_line(frame, footer_rows[0], p, hints);
        } else if app.settings.section == SettingsSection::GroupProfiles {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("favorite", "ctrl+f")],
            );
        } else if group_settings {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("select", "space")],
            );
        } else {
            render_modal_hint_line(
                frame,
                footer_rows[0],
                p,
                &[("move", "↑↓"), ("select", "space"), ("section", "tab")],
            );
        }
    }
}

fn settings_agents_editor_open(app: &AppState) -> bool {
    app.settings.pending_agent_profile_id.is_some()
        || app.settings.pending_agent_profile_name.is_some()
        || app.settings.pending_agent_profile_command.is_some()
}

pub(crate) fn settings_close_button_rect(inner: Rect) -> Rect {
    let header = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(modal_stack_areas(inner, 4, 1, 0, 1).header)[0];
    super::widgets::modal_close_button_rect(header)
}

#[cfg(test)]
fn render_settings_theme(app: &AppState, frame: &mut Frame, area: Rect) {
    render_settings_sectioned_toggle_list(app, frame, area);
}

fn render_settings_integrations(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let body_area = render_settings_section_intro(app, frame, area, SettingsSection::Integrations);
    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas::<2>(body_area);

    let model_rows = rows_for_section(app, SettingsSection::Integrations).unwrap_or_default();
    let mut lines = Vec::new();
    for row in &model_rows {
        let SettingsListRow::StatusChoice {
            index,
            marker,
            label,
            tone,
        } = row
        else {
            continue;
        };
        let selected = app.settings.list.selected == *index;
        let selected_style = modal_option_style(p, selected);
        let marker_style = if selected {
            selected_style
        } else {
            settings_marker_style(p, *tone)
        };
        let label_style = if selected {
            selected_style
        } else {
            Style::default().fg(p.subtext0)
        };
        if selected {
            let text = format!(" {marker} {label}");
            lines.push(Line::from(Span::styled(
                format!("{text:<width$}", width = list_area.width as usize),
                selected_style,
            )));
        } else {
            lines.push(Line::from(vec![
                Span::styled(format!(" {marker} "), marker_style),
                Span::styled(label.as_ref(), label_style),
            ]));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " no integration targets available",
            Style::default().fg(p.overlay1),
        )));
    }
    frame.render_widget(Paragraph::new(lines), list_area);

    let found_any = app.integration_recommendations.iter().any(|item| {
        item.available || item.state != crate::integration::IntegrationStatusKind::NotInstalled
    });
    let hint = if !app.integration_install_messages.is_empty() {
        app.integration_install_messages.join("\n ")
    } else if let Some(item) = app
        .integration_recommendations
        .get(app.settings.list.selected)
    {
        match item.state {
            crate::integration::IntegrationStatusKind::Current => {
                "press enter to uninstall selected integration".to_string()
            }
            crate::integration::IntegrationStatusKind::Outdated => {
                "press enter to update selected integration".to_string()
            }
            crate::integration::IntegrationStatusKind::NotInstalled if item.available => {
                "press enter to install selected integration".to_string()
            }
            crate::integration::IntegrationStatusKind::NotInstalled => {
                "selected integration is unavailable".to_string()
            }
        }
    } else if app
        .integration_recommendations
        .iter()
        .any(crate::integration::IntegrationRecommendation::needs_install)
    {
        "press enter to add available or outdated integrations".to_string()
    } else if found_any {
        "all detected integrations are installed".to_string()
    } else {
        "no supported agent CLIs found on PATH".to_string()
    };
    frame.render_widget(
        Paragraph::new(format!(" {hint}")).style(Style::default().fg(p.overlay1)),
        hint_area,
    );
}

fn render_settings_sectioned_toggle_list(app: &AppState, frame: &mut Frame, area: Rect) {
    let list_area = render_settings_section_intro(app, frame, area, app.settings.section);
    render_settings_rows(app, frame, list_area);
}

fn render_settings_rows(app: &AppState, frame: &mut Frame, area: Rect) {
    let p = &app.palette;
    let selected_style = modal_option_style(p, true);

    let Some(model_rows) = rows_for_section(app, app.settings.section) else {
        return;
    };

    let total_items = visual_row_count(&model_rows);
    let viewport =
        crate::ui::ModalListViewport::new(total_items, area.height as usize, app.settings.scroll);
    let scroll = viewport.scroll();
    let scroll_area = viewport.scroll_area(area);
    let list_width = scroll_area.body.width as usize;
    let viewport_rows = area.height as usize;
    let mut selected_row = None;
    let mut rows = Vec::with_capacity(total_items);

    for row in &model_rows {
        match row {
            SettingsListRow::Header(title) => {
                rows.push(ListItem::new(Line::from(Span::styled(
                    format!(" {title}"),
                    modal_section_heading_style(p),
                ))));
            }
            SettingsListRow::Caption(text) => {
                rows.push(ListItem::new(Line::from(Span::styled(
                    format!(" {text}"),
                    Style::default().fg(p.subtext0),
                ))));
            }
            SettingsListRow::Spacer => rows.push(ListItem::new(Line::from(""))),
            SettingsListRow::Option {
                index,
                title,
                description,
                enabled,
            } => {
                let selected = app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let marker = if *enabled { "●" } else { "○" };
                let marker_style = settings_toggle_marker_style(p, *enabled, selected);
                if selected {
                    rows.push(ListItem::new(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::styled(" ", selected_style),
                        Span::styled(
                            format!("{title:<width$}", width = list_width.saturating_sub(2)),
                            selected_style,
                        ),
                    ])));
                    rows.push(ListItem::new(Line::from(Span::styled(
                        format!(
                            "  {description:<width$}",
                            width = list_width.saturating_sub(2)
                        ),
                        selected_style,
                    ))));
                } else {
                    rows.push(ListItem::new(Line::from(vec![
                        Span::styled(marker, marker_style),
                        Span::raw(" "),
                        Span::styled(title.as_ref(), Style::default().fg(p.text)),
                    ])));
                    rows.push(ListItem::new(Line::from(Span::styled(
                        description.as_ref(),
                        Style::default().fg(p.subtext0),
                    ))));
                }
            }
            SettingsListRow::TextInput {
                index,
                title,
                value,
            } => {
                let selected = app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len() + 1);
                }
                rows.push(ListItem::new(Line::from(Span::styled(
                    format!(" {title:<width$}", width = list_width.saturating_sub(1)),
                    Style::default().fg(p.text).add_modifier(Modifier::BOLD),
                ))));
                let input_value = if selected {
                    format!(" {value}█")
                } else {
                    format!(" {value}")
                };
                let input_style = Style::default().fg(p.text).bg(p.surface0);
                rows.push(ListItem::new(Line::from(Span::styled(
                    format!("{input_value:<list_width$}"),
                    input_style,
                ))));
            }
            SettingsListRow::Choice {
                index,
                label,
                checked,
            } => {
                let selected = app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let marker = if *checked { " ✓" } else { "" };
                if selected {
                    let text = format!("  {label}{marker}");
                    rows.push(ListItem::new(Line::from(Span::styled(
                        format!("{text:<list_width$}"),
                        selected_style,
                    ))));
                } else {
                    rows.push(ListItem::new(Line::from(vec![
                        Span::styled(format!("  {label}"), modal_option_style(p, false)),
                        Span::styled(marker.to_string(), modal_option_marker_style(p, false)),
                    ])));
                }
            }
            SettingsListRow::StatusChoice {
                index,
                marker,
                label,
                tone,
            } => {
                let selected = app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let selected_style = if selected && *tone == SettingsMarkerTone::Danger {
                    settings_danger_selected_style(p)
                } else {
                    modal_option_style(p, selected)
                };
                let marker_style = if selected {
                    selected_style
                } else {
                    settings_marker_style(p, *tone)
                };
                let label_style = if *tone == SettingsMarkerTone::Danger {
                    marker_style
                } else {
                    Style::default().fg(p.text)
                };
                if selected {
                    let text = format!(" {marker} {label}");
                    rows.push(ListItem::new(Line::from(Span::styled(
                        format!("{text:<list_width$}"),
                        selected_style,
                    ))));
                } else {
                    rows.push(ListItem::new(Line::from(vec![
                        Span::styled(format!(" {marker} "), marker_style),
                        Span::styled(label.as_ref(), label_style),
                    ])));
                }
            }
        }
    }

    let selected =
        selected_row.and_then(|row| (row >= scroll && row < scroll + viewport_rows).then_some(row));

    let list = List::new(rows);
    let mut state = ListState::default()
        .with_selected(selected)
        .with_offset(scroll);
    frame.render_stateful_widget(list, scroll_area.body, &mut state);
    if let Some(track) = scroll_area.track {
        render_scrollbar(
            frame,
            viewport.metrics(),
            track,
            p.surface_dim,
            p.overlay0,
            "▐",
        );
    }
}

fn modal_option_style(p: &Palette, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.text)
    }
}

fn settings_marker_style(p: &Palette, tone: SettingsMarkerTone) -> Style {
    match tone {
        SettingsMarkerTone::Good => Style::default().fg(p.green),
        SettingsMarkerTone::Warning => Style::default().fg(p.yellow),
        SettingsMarkerTone::Accent => Style::default().fg(p.accent),
        SettingsMarkerTone::Danger => Style::default().fg(p.red).add_modifier(Modifier::BOLD),
        SettingsMarkerTone::Disabled => Style::default().fg(p.overlay0),
    }
}

fn settings_danger_selected_style(p: &Palette) -> Style {
    Style::default()
        .fg(panel_contrast_fg(p))
        .bg(p.red)
        .add_modifier(Modifier::BOLD)
}

fn settings_toggle_marker_style(p: &Palette, enabled: bool, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else if enabled {
        Style::default().fg(p.green)
    } else {
        Style::default().fg(p.overlay0)
    }
}

fn modal_option_marker_style(p: &Palette, selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(panel_contrast_fg(p))
            .bg(p.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.green)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    use super::*;
    use crate::app::state::{AppState, SettingsSection};
    use crate::{
        app::state::theme_names_for_appearance,
        config::{TerminalAccent, ToastDelivery},
        terminal_theme::ThemeAppearance,
    };

    #[test]
    fn group_settings_overlay_uses_main_settings_layout_with_group_tabs() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        app.settings.group_settings_target = Some(group_idx);
        app.settings.section = SettingsSection::Theme;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group settings overlay");

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("group settings"));
        assert!(text.contains("appearance"));
        assert!(text.contains("general"));
        assert!(text.contains("accent"));
        assert!(!text.contains("sound"));
    }

    #[test]
    fn group_general_settings_lists_name_and_danger_actions() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        app.settings.group_settings_target = Some(group_idx);
        app.settings.section = SettingsSection::GroupGeneral;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group settings overlay");

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("name"));
        assert!(text.contains("Work"));
        assert!(text.contains("danger zone"));
        assert!(text.contains("! delete group"));
        assert!(!text.contains("●"));
        assert!(!text.contains("○"));
        assert!(text.contains("Work█"));
    }

    #[test]
    fn group_general_delete_action_uses_red_text_and_hover_background() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        app.settings.group_settings_target = Some(group_idx);
        app.settings.section = SettingsSection::GroupGeneral;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group settings overlay");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 80, 24);
        let (y, x) = find_text_cell(&text, "! delete group").expect("delete action row");
        assert_eq!(buffer[(x, y)].style().fg, Some(app.palette.red));
        assert_eq!(buffer[(x + 2, y)].style().fg, Some(app.palette.red));
        assert_ne!(buffer[(x, y)].style().bg, Some(app.palette.red));

        app.settings.list.selected = 1;
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group settings overlay");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 80, 24);
        let (y, x) = find_text_cell(&text, "! delete group").expect("delete action row");
        assert_eq!(buffer[(x, y)].style().bg, Some(app.palette.red));
        assert_eq!(buffer[(x + 2, y)].style().bg, Some(app.palette.red));
        assert_eq!(
            buffer[(x, y)].style().fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn theme_settings_light_mode_only_lists_light_themes() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::Light;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.pending_light_theme_name = Some("catppuccin-latte".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("catppuccin latte"));
        assert!(text.contains("solarized"));
        assert!(!text.contains("dracula"));
        assert!(!text.contains("nord"));
        assert!(!text.contains("vesper"));
        assert_no_option_line(&text, "terminal");
    }

    #[test]
    fn theme_settings_dark_mode_only_lists_dark_themes() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::Dark;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Dark);
        app.settings.pending_dark_theme_name = Some("catppuccin".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("catppuccin"));
        assert!(text.contains("dracula"));
        assert!(!text.contains("catppuccin latte"));
        assert_no_option_line(&text, "terminal");
    }

    #[test]
    fn theme_settings_system_source_hides_appearance_sections() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("system".to_string());
        app.settings.pending_dark_theme_name = Some("system".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("terminal ✓"));
        assert!(text.contains("palettes"));
        assert!(text.contains("accent"));
        assert!(text.contains("blue ✓"));
        assert!(text.contains("magenta"));
        assert!(text.contains("colors"));
        assert!(!text.contains("light appearance"));
        assert!(!text.contains("dark appearance"));
    }

    #[test]
    fn theme_settings_system_mode_lists_light_and_dark_selections() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("solarized-light".to_string());
        app.settings.pending_dark_theme_name = Some("rose-pine".to_string());

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("terminal"));
        assert!(text.contains("palettes ✓"));
        assert!(text.contains("appearance"));
        assert!(text.contains("automatic ✓"));
        assert!(!text.contains("system terminal"));
        assert!(!text.contains(" mode"));
        assert!(text.contains("light appearance"));
        assert!(text.contains("solarized"));
        assert_no_option_line(&text, "terminal");

        app.settings.list.selected = theme_names_for_appearance(ThemeAppearance::Light).len();
        app.settings.scroll = theme_names_for_appearance(ThemeAppearance::Light).len() + 3;
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("dark appearance"));
        assert!(text.contains("rose pine"));
    }

    #[test]
    fn theme_settings_system_scroll_can_reveal_dark_section_while_mode_selected() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("solarized-light".to_string());
        app.settings.pending_dark_theme_name = Some("rose-pine".to_string());
        app.settings.list.selected = 0;
        app.settings.scroll = theme_names_for_appearance(ThemeAppearance::Light).len() + 2;

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(text.contains("dark appearance"));
        assert!(text.contains("rose pine"));
    }

    #[test]
    fn theme_settings_marks_pending_values_not_hovered_cursor_values() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.global_light_theme_name = "catppuccin-latte".to_string();
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.pending_light_theme_name = Some("solarized-light".to_string());
        app.settings.list.selected = 1;

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(!text.contains("automatic ✓"));
        assert!(text.contains("light ✓"));
        assert!(!text.contains("catppuccin latte ✓"));
        assert!(text.contains("solarized ✓"));
    }

    #[test]
    fn theme_settings_selected_row_highlight_extends_to_row_end() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.list.selected = 3;

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let selected_row_y = 9;
        let selected_row_end = area.x + area.width.saturating_sub(1);
        assert_eq!(
            terminal.backend().buffer()[(selected_row_end, selected_row_y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
    }

    #[test]
    fn terminal_dark_accent_highlight_starts_on_first_option() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::System);
        app.settings.pending_light_theme_name = Some("system".to_string());
        app.settings.pending_dark_theme_name = Some("system".to_string());
        app.settings.list.selected = 2 + TerminalAccent::ALL.len();

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let dark_heading_y = text
            .lines()
            .position(|line| line.contains("dark accent"))
            .expect("dark accent heading") as u16;
        let dark_blue_y = text
            .lines()
            .enumerate()
            .skip(dark_heading_y as usize + 1)
            .find_map(|(y, line)| line.contains("blue").then_some(y as u16))
            .expect("dark blue option");
        let selected_row_end = area.x + area.width.saturating_sub(1);

        assert_ne!(
            terminal.backend().buffer()[(selected_row_end, dark_heading_y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
        assert_eq!(
            terminal.backend().buffer()[(selected_row_end, dark_blue_y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
    }

    #[test]
    fn theme_settings_selected_row_does_not_shift_text() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.list.selected = 3;

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, area.width, area.height);
        let (y, x) = find_text_cell(&text, "light ✓").expect("selected light row");
        assert_eq!(buffer[(x.saturating_sub(2), y)].symbol(), " ");
        assert_eq!(buffer[(x.saturating_sub(1), y)].symbol(), " ");
        assert_eq!(buffer[(x, y)].symbol(), "l");
    }

    #[test]
    fn agent_profile_editor_renders_numbered_steps() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Agents;
        app.settings.pending_agent_profile_name = Some("omp mk".to_string());
        app.settings.pending_agent_profile_command = Some("omp-mk".to_string());
        app.settings.pending_agent_profile_kind = Some(crate::agent_profiles::AgentKind::Omp);

        let area = Rect::new(0, 0, 100, 40);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("← back"));
        assert!(text.contains("1. name"));
        assert!(text.contains("enter the short label shown in menus and pickers"));
        assert!(text.contains("2. kind"));
        assert!(text.contains("select the agent family this command should restore as"));
        app.settings.scroll = 12;
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render scrolled settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("3. command"));
        assert!(text.contains("enter the exact shell command hako should launch"));
        assert!(text.contains("4. actions"));
    }

    #[test]
    fn agent_profile_editor_selected_choice_does_not_shift_text() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Agents;
        app.settings.pending_agent_profile_name = Some("omp mk".to_string());
        app.settings.pending_agent_profile_command = Some("omp-mk".to_string());
        app.settings.pending_agent_profile_kind = Some(crate::agent_profiles::AgentKind::Omp);
        app.settings.list.selected = 2;

        let area = Rect::new(0, 0, 100, 40);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, area.width, area.height);
        let (y, x) = find_text_cell(&text, "omp ✓").expect("selected omp kind");
        let popup = centered_popup_rect(area, 92, 26).expect("settings popup");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let list_x = settings_section_list_rect(modal_stack_areas(inner, 4, 1, 0, 1).content).x;
        assert_eq!(x, list_x + 2);
        assert_eq!(buffer[(x, y)].symbol(), "o");
    }

    #[test]
    fn settings_choice_tabs_use_single_row_options() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Toast;
        app.settings.pending_toast_delivery = Some(ToastDelivery::Off);

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let lines: Vec<&str> = text.lines().collect();
        let header_row = lines
            .iter()
            .position(|line| line.contains("notification popups"))
            .expect("notification header row");
        let off_row = lines
            .iter()
            .position(|line| line.contains("off ✓"))
            .expect("off choice row");
        let hako_row = lines
            .iter()
            .position(|line| line.contains("inside hako"))
            .expect("hako choice row");
        let terminal_row = lines
            .iter()
            .position(|line| line.contains("via terminal"))
            .expect("terminal choice row");
        let system_row = lines
            .iter()
            .position(|line| line.contains("via system"))
            .expect("system choice row");

        assert_eq!(off_row, header_row + 1);
        assert_eq!(hako_row, off_row + 1);
        assert_eq!(terminal_row, hako_row + 1);
        assert_eq!(system_row, terminal_row + 1);
    }

    #[test]
    fn settings_renders_single_escape_close_label() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Theme;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert_eq!(text.matches("esc close").count(), 1);
        assert!(!text.contains("esc cancel"));
    }

    #[test]
    fn settings_has_no_footer_save_button() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Theme;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(!text.contains("↵ save"));
        assert!(!text.contains("↵ apply"));
    }

    #[test]
    fn settings_tabs_fit_modal_width() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Theme;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let tab_line = text
            .lines()
            .find(|line| line.contains("theme") && line.contains("layout"))
            .expect("tab line");
        assert!(tab_line.contains("experiments"));
    }

    #[test]
    fn settings_header_keeps_blank_row_between_title_and_tabs() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Theme;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let lines: Vec<&str> = text.lines().collect();
        let title_row = lines
            .iter()
            .position(|line| line.contains("settings") && line.contains("esc close"))
            .expect("settings title row");
        let tab_row = lines
            .iter()
            .position(|line| line.contains("theme") && line.contains("layout"))
            .expect("settings tab row");

        assert_eq!(tab_row, title_row + 2);
    }

    #[test]
    fn settings_tabs_render_descriptions_consistently() {
        for section in SettingsSection::ALL {
            let mut app = AppState::test_new();
            app.settings.section = *section;

            let area = Rect::new(0, 0, 100, 30);
            let backend = TestBackend::new(area.width, area.height);
            let mut terminal = Terminal::new(backend).expect("test backend");
            terminal
                .draw(|frame| render_settings_overlay(&app, frame, area))
                .expect("render settings overlay");

            let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
            assert!(
                text.contains(settings_section_title(&app, *section)),
                "missing title for {section:?}"
            );
            assert!(
                text.contains(settings_section_description(&app, *section)),
                "missing description for {section:?}"
            );
        }
    }

    #[test]
    fn layout_settings_render_sidebar_widths() {
        let mut app = AppState::test_new();
        app.default_sidebar_width = 26;
        app.sidebar_min_width = 18;
        app.sidebar_max_width = 36;
        app.settings.section = SettingsSection::Layout;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("sidebar"));
        assert!(text.contains("● default sidebar width"));
        assert!(text.contains("26 columns"));
        assert!(text.contains("● minimum sidebar width"));
        assert!(text.contains("18 columns"));
        assert!(text.contains("● maximum sidebar width"));
        assert!(text.contains("36 columns"));
        assert!(!text.contains("worktrees"));
        assert!(!text.contains("worktree directory"));
        assert!(!text.contains("/tmp/hako-worktrees"));
    }

    #[test]
    fn behavior_settings_render_workspace_terminal_and_worktree_options() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::PaneLabels;
        app.worktree_directory = std::path::PathBuf::from("/tmp/hako-worktrees");

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("workspace"));
        assert!(text.contains("terminal"));
        assert!(text.contains("worktrees"));
        assert!(text.contains("● worktree directory"));
        assert!(text.contains("/tmp/hako-worktrees"));
    }
    #[test]
    fn sectioned_settings_selected_text_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, selected_x) = text
            .lines()
            .enumerate()
            .find_map(|(y, line)| {
                line.find("default sidebar width")
                    .map(|x| (y as u16, x as u16))
            })
            .expect("selected layout row");

        assert_eq!(
            terminal.backend().buffer()[(selected_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn behavior_settings_render_close_prompt_and_agent_labels() {
        let mut app = AppState::test_new();
        app.confirm_close = true;
        app.prompt_new_tab_name = true;
        app.show_agent_labels_on_pane_borders = false;
        app.settings.section = SettingsSection::PaneLabels;
        app.settings.list.selected = 5;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("● name new tabs"));
        assert!(text.contains("● worktree directory"));
        assert!(text.contains("● new terminal cwd"));
        assert!(text.contains("follow focused pane"));
        assert!(text.contains("● mouse wheel speed"));
        assert!(text.contains("3 lines per wheel notch"));
        assert!(text.contains("○ agent border labels"));
    }

    #[test]
    fn selected_section_markers_use_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, marker_x) =
            find_text_cell(&text, "● default sidebar width").expect("selected layout row");

        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)].symbol(),
            "●"
        );
        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn selected_disabled_section_markers_use_selected_foreground() {
        let mut app = AppState::test_new();
        app.switch_ascii_input_source_in_prefix = false;
        app.settings.section = SettingsSection::Experiments;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, marker_x) =
            find_text_cell(&text, "○ switch to ascii input source in prefix (macOS)")
                .expect("selected experiment row");

        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)].symbol(),
            "○"
        );
        assert_eq!(
            terminal.backend().buffer()[(marker_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn selected_settings_tab_badge_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Integrations;
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Omp,
            label: "omp",
            command: "omp",
            available: true,
            path: std::path::PathBuf::from("/tmp/hako-test-omp"),
            state: crate::integration::IntegrationStatusKind::Outdated,
        }];

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (badge_y, badge_x) =
            find_text_cell(&text, "● integrations").expect("selected integrations badge");
        assert_eq!(
            terminal.backend().buffer()[(badge_x, badge_y)].style().fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }
    #[test]
    fn integrations_selected_row_highlight_extends_to_row_end() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Integrations;
        app.settings.list.selected = 0;
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Omp,
            label: "omp",
            command: "omp",
            available: true,
            path: std::path::PathBuf::from("/tmp/hako-test-omp"),
            state: crate::integration::IntegrationStatusKind::NotInstalled,
        }];

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let popup = centered_popup_rect(area, 92, 26).expect("popup");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let content = modal_stack_areas(inner, 4, 1, 0, 1).content;
        let body = settings_section_list_rect(content);
        let [list_area, _] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas::<2>(body);
        let selected_row_end = list_area.x + list_area.width.saturating_sub(1);

        assert_eq!(
            terminal.backend().buffer()[(selected_row_end, list_area.y)]
                .style()
                .bg,
            Some(app.palette.accent)
        );
    }

    #[test]
    fn experiments_render_input_source_only() {
        let mut app = AppState::test_new();
        app.switch_ascii_input_source_in_prefix = true;
        app.settings.section = SettingsSection::Experiments;
        app.settings.list.selected = 0;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(!text.contains("restore"));
        assert!(!text.contains("history"));
        assert!(!text.contains("resume agent sessions"));
        assert!(!text.contains("pane screen history"));
        assert!(text.contains("input"));
        assert!(text.contains("● switch to ascii input source in prefix (macOS)"));
    }
    fn assert_no_option_line(text: &str, option: &str) {
        let mut in_appearance_section = false;
        for line in text.lines() {
            let line = line.trim();
            if line == "light appearance" || line == "dark appearance" {
                in_appearance_section = true;
                continue;
            }
            if in_appearance_section && line.is_empty() {
                in_appearance_section = false;
                continue;
            }
            assert!(
                !in_appearance_section || (line != option && line != format!("{option} ✓")),
                "unexpected appearance option line {option:?} in:\n{text}"
            );
        }
    }
    fn find_text_cell(text: &str, needle: &str) -> Option<(u16, u16)> {
        text.lines().enumerate().find_map(|(y, line)| {
            let byte_x = line.find(needle)?;
            let cell_x = line[..byte_x].chars().count();
            Some((y as u16, cell_x as u16))
        })
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
