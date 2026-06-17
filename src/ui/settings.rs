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
    action_button_width, modal_hint_line_count, modal_section_heading_style, modal_stack_areas,
    panel_contrast_fg, render_action_button, render_modal_description, render_modal_divider,
    render_modal_frame, secondary_action_style, ModalFrameSpec,
};
use crate::{
    app::{
        state::{Palette, SettingsSection},
        AppState,
    },
    settings_rows::{rows_for_section, visual_row_count, SettingsListRow, SettingsMarkerTone},
};

#[cfg(test)]
use crate::config::ThemeMode;
const GROUP_SETTINGS_SECTIONS: &[SettingsSection] = &[
    SettingsSection::GroupGeneral,
    SettingsSection::Theme,
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
    settings_tab_text(app, section).width() as u16 + 2
}

fn settings_visible_tab_range(app: &AppState, row_width: u16) -> (usize, usize) {
    let sections = settings_sections(app);
    let selected = sections
        .iter()
        .position(|section| *section == app.settings.section)
        .unwrap_or(0);
    super::modal_tabs::visible_tab_range(sections.len(), selected, row_width, |idx| {
        settings_tab_width(app, sections[idx])
    })
}

pub(crate) fn settings_tab_hit_areas(app: &AppState, row: Rect) -> Vec<(SettingsSection, Rect)> {
    let sections = settings_sections(app);
    let (start, end) = settings_visible_tab_range(app, row.width);
    super::modal_tabs::tab_hit_areas(row, start, end, |idx| {
        settings_tab_width(app, sections[idx])
    })
    .into_iter()
    .map(|(idx, rect)| (sections[idx], rect))
    .collect()
}

pub(crate) fn settings_tab_chevron_at(
    app: &AppState,
    row: Rect,
    col: u16,
) -> Option<SettingsSection> {
    let sections = settings_sections(app);
    let (start, end) = settings_visible_tab_range(app, row.width);
    super::modal_tabs::chevron_tab_at(sections.len(), row, col, start, end, |idx| {
        settings_tab_width(app, sections[idx])
    })
    .and_then(|idx| sections.get(idx).copied())
}

fn settings_palette(app: &AppState) -> crate::app::state::Palette {
    app.settings
        .group_settings_target
        .map(|group_idx| app.palette_for_group(group_idx))
        .unwrap_or_else(|| app.palette.clone())
}

fn render_settings_tabs(
    app: &AppState,
    frame: &mut Frame,
    row: Rect,
    p: &crate::app::state::Palette,
) {
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
        spans.push(Span::styled(settings_tab_text(app, section), tab_style));
        spans.push(Span::styled(" ", tab_style));
    }

    if end < sections.len() {
        spans.push(Span::styled(" ›", Style::default().fg(p.overlay0)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn settings_section_title(app: &AppState, section: SettingsSection) -> &'static str {
    if section == SettingsSection::Agents && settings_agents_editor_open(app) {
        if app.settings.pending_agent_profile_id.is_some() {
            "edit custom profile"
        } else {
            "new custom profile"
        }
    } else {
        match section {
            SettingsSection::Theme => "appearance",
            SettingsSection::Layout => "layout",
            SettingsSection::Sound => "notifications",
            SettingsSection::Toast => "toasts",
            SettingsSection::PaneLabels => "behavior",
            SettingsSection::Experiments => "advanced",
            SettingsSection::Agents => "agents",
            SettingsSection::Integrations => "agent integrations",
            SettingsSection::GroupGeneral => "general",
            SettingsSection::GroupProfiles => "agents",
        }
    }
}

fn settings_section_description(app: &AppState, section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Theme if app.settings.group_settings_target.is_some() => {
            "choose an ANSI accent for this group, or inherit the global accent"
        }
        SettingsSection::Theme => "configure theme, sidebar layout, and pane appearance",
        SettingsSection::Layout => "set sidebar width bounds",
        SettingsSection::Sound => "choose sound and toast notification behavior",
        SettingsSection::Toast => "choose where command and agent notifications are delivered",
        SettingsSection::PaneLabels => {
            "control workspace prompts and terminal interaction defaults"
        }
        SettingsSection::Experiments => "configure advanced or platform-specific behavior",
        SettingsSection::Agents if settings_agents_editor_open(app) => {
            "name the profile and provide the command hako should launch"
        }
        SettingsSection::Agents => "create custom commands and manage agent profiles",
        SettingsSection::Integrations => "install hooks so agents report state directly",
        SettingsSection::GroupGeneral => "rename this group or delete it",
        SettingsSection::GroupProfiles => {
            "choose favorite and default agent profiles for this group"
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
    p: &crate::app::state::Palette,
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
        Style::default().fg(p.accent),
    );
    if let Some(back) = settings_agents_editor_back_button_rect(app, title_area) {
        render_action_button(frame, back, None, "← back", secondary_action_style(p));
    }
    render_modal_description(
        frame,
        description_area,
        settings_section_description(app, section),
        Style::default().fg(p.overlay0),
    );
    list_area
}

const SETTINGS_INTEGRATIONS_HINTS: &[(&str, &str)] =
    &[("move", "↑↓"), ("action", "space/↵"), ("section", "←→/tab")];
const SETTINGS_AGENTS_EDITOR_HINTS: &[(&str, &str)] =
    &[("move", "↑↓"), ("action", "space/↵"), ("section", "←→/tab")];
const SETTINGS_AGENTS_HINTS: &[(&str, &str)] = &[
    ("move", "↑↓"),
    ("new/edit", "space/↵"),
    ("delete", "ctrl+d"),
    ("section", "←→/tab"),
];
const SETTINGS_GROUP_PROFILES_HINTS: &[(&str, &str)] = &[
    ("move", "↑↓"),
    ("favorite", "ctrl+f"),
    ("default", "ctrl+d"),
    ("section", "←→/tab"),
];
const SETTINGS_GROUP_HINTS: &[(&str, &str)] =
    &[("move", "↑↓"), ("action", "space/↵"), ("section", "←→/tab")];
const SETTINGS_DEFAULT_HINTS: &[(&str, &str)] =
    &[("move", "↑↓"), ("action", "space/↵"), ("section", "←→/tab")];

fn settings_footer_hints(
    app: &AppState,
    group_settings: bool,
) -> &'static [(&'static str, &'static str)] {
    if app.settings.section == SettingsSection::Integrations {
        SETTINGS_INTEGRATIONS_HINTS
    } else if app.settings.section == SettingsSection::Agents {
        if settings_agents_editor_open(app) {
            SETTINGS_AGENTS_EDITOR_HINTS
        } else {
            SETTINGS_AGENTS_HINTS
        }
    } else if app.settings.section == SettingsSection::GroupProfiles {
        SETTINGS_GROUP_PROFILES_HINTS
    } else if group_settings {
        SETTINGS_GROUP_HINTS
    } else {
        SETTINGS_DEFAULT_HINTS
    }
}

pub(crate) fn settings_stack_areas(app: &AppState, inner: Rect) -> super::widgets::ModalStackAreas {
    let group_settings = app.settings.group_settings_target.is_some();
    let footer_rows =
        modal_hint_line_count(inner.width, settings_footer_hints(app, group_settings), 2);
    modal_stack_areas(inner, 4, footer_rows, 0, 1)
}

pub(super) fn render_settings_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    use crate::app::state::SettingsSection;

    let group_settings = app.settings.group_settings_target.is_some();

    let palette = settings_palette(app);
    let p = &palette;
    super::dim_background(frame, area);

    let Some(frame_areas) = render_modal_frame(
        frame,
        area,
        p,
        ModalFrameSpec {
            title: settings_title(app),
            width: 92,
            height: 26,
            header_rows: 4,
            footer_hints: settings_footer_hints(app, group_settings),
            footer_max_rows: 2,
            reserve_footer_gap: 1,
            show_close: true,
        },
    ) else {
        return;
    };
    let inner = frame_areas.inner;
    if inner.height < 4 || inner.width < 10 {
        return;
    }

    let stack = settings_stack_areas(app, inner);
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(stack.header);
    render_settings_tabs(app, frame, header_rows[2], p);
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
            render_settings_sectioned_toggle_list(app, frame, content_area, p);
        }
        SettingsSection::Integrations => render_settings_integrations(app, frame, content_area, p),
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
    let palette = settings_palette(app);
    render_settings_sectioned_toggle_list(app, frame, area, &palette);
}

fn render_settings_integrations(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
) {
    let body_area =
        render_settings_section_intro(app, frame, area, SettingsSection::Integrations, p);
    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas::<2>(body_area);

    if app.integration_recommendations.is_empty() {
        frame.render_widget(
            Paragraph::new(settings_description_line(
                "no integration targets available",
                list_area.width as usize,
                Style::default().fg(p.overlay1),
                false,
            )),
            list_area,
        );
    } else {
        render_settings_rows(app, frame, list_area, p);
    }

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
        Paragraph::new(format!(" {hint}")).style(Style::default().fg(p.overlay0)),
        hint_area,
    );
}

fn render_settings_sectioned_toggle_list(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
) {
    let body_area = render_settings_section_intro(app, frame, area, app.settings.section, p);
    render_settings_rows(app, frame, body_area, p);
}

const SETTINGS_BODY_INDENT: usize = 2;
const SETTINGS_DESCRIPTION_INDENT: usize = 4;

fn settings_body_width(width: usize) -> usize {
    width.saturating_sub(SETTINGS_BODY_INDENT)
}

fn settings_description_width(width: usize) -> usize {
    width.saturating_sub(SETTINGS_DESCRIPTION_INDENT)
}

fn settings_padded_text(text: &str, width: usize) -> String {
    format!("{text:<width$}")
}

fn settings_title_value_line(
    title: &str,
    value: &str,
    width: usize,
    title_style: Style,
    value_style: Style,
    selected: bool,
) -> Line<'static> {
    let body_width = settings_body_width(width);
    let title_width = title.width();
    let value_width = value.width();
    let gap = if body_width > title_width + value_width {
        body_width - title_width - value_width
    } else {
        1
    };
    let filler_style = if selected {
        title_style
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(" ".repeat(SETTINGS_BODY_INDENT), filler_style),
        Span::styled(title.to_string(), title_style),
        Span::styled(" ".repeat(gap), filler_style),
        Span::styled(value.to_string(), value_style),
    ])
}

fn settings_description_line(
    text: &str,
    width: usize,
    style: Style,
    selected: bool,
) -> Line<'static> {
    let body_width = settings_body_width(width);
    let content = settings_padded_text(text, body_width);
    let filler_style = if selected { style } else { Style::default() };
    Line::from(vec![
        Span::styled(" ".repeat(SETTINGS_BODY_INDENT), filler_style),
        Span::styled(content, style),
    ])
}

fn settings_setting_description_line(
    text: &str,
    width: usize,
    style: Style,
    selected: bool,
) -> Line<'static> {
    let body_width = settings_description_width(width);
    let content = settings_padded_text(text, body_width);
    let filler_style = if selected { style } else { Style::default() };
    Line::from(vec![
        Span::styled(" ".repeat(SETTINGS_DESCRIPTION_INDENT), filler_style),
        Span::styled(content, style),
    ])
}

fn settings_status_line(
    label: &str,
    status: &str,
    width: usize,
    label_style: Style,
    status_style: Style,
    selected: bool,
) -> Line<'static> {
    settings_title_value_line(label, status, width, label_style, status_style, selected)
}

fn settings_action_line(
    icon: &str,
    label: &str,
    width: usize,
    style: Style,
    selected: bool,
) -> Line<'static> {
    let body_width = settings_body_width(width);
    let text = if icon.is_empty() {
        label.to_string()
    } else {
        format!("{icon} {label}")
    };
    let content = settings_padded_text(&text, body_width);
    let filler_style = if selected { style } else { Style::default() };
    Line::from(vec![
        Span::styled(" ".repeat(SETTINGS_BODY_INDENT), filler_style),
        Span::styled(content, style),
    ])
}

fn settings_choice_line(
    label: &str,
    checked: bool,
    width: usize,
    label_style: Style,
    check_style: Style,
    selected: bool,
) -> Line<'static> {
    let body_width = settings_body_width(width);
    let marker = if checked { "✓" } else { " " };
    let label_width = label.width();
    let padding = body_width.saturating_sub(2 + label_width);
    let filler_style = if selected {
        label_style
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(" ".repeat(SETTINGS_BODY_INDENT), filler_style),
        Span::styled(marker.to_string(), check_style),
        Span::styled(" ", filler_style),
        Span::styled(label.to_string(), label_style),
        Span::styled(" ".repeat(padding), filler_style),
    ])
}

fn settings_profile_name_line(
    name: &str,
    detail: &str,
    badge: Option<&str>,
    width: usize,
    name_style: Style,
    detail_style: Style,
    badge_style: Style,
    selected: bool,
) -> Line<'static> {
    let badge = badge.unwrap_or("");
    let detail_text = if detail.is_empty() {
        String::new()
    } else {
        format!(" · {detail}")
    };
    let body_width = settings_body_width(width);
    let used_width = name.width() + detail_text.width() + badge.width();
    let gap = if body_width > used_width {
        body_width - used_width
    } else {
        1
    };
    let filler_style = if selected {
        name_style
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(" ".repeat(SETTINGS_BODY_INDENT), filler_style),
        Span::styled(name.to_string(), name_style),
        Span::styled(detail_text, detail_style),
        Span::styled(" ".repeat(gap), filler_style),
        Span::styled(badge.to_string(), badge_style),
    ])
}

fn render_settings_rows(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
) {
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
            SettingsListRow::Toggle {
                index,
                title,
                description,
                enabled,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let value = if *enabled { "on" } else { "off" };
                let label_style = if selected {
                    selected_style
                } else {
                    Style::default().fg(p.text)
                };
                let value_style = if selected {
                    selected_style.add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.accent)
                };
                rows.push(ListItem::new(settings_title_value_line(
                    title,
                    value,
                    list_width,
                    label_style,
                    value_style,
                    selected,
                )));
                rows.push(ListItem::new(settings_setting_description_line(
                    description,
                    list_width,
                    if selected {
                        selected_style
                    } else {
                        Style::default().fg(p.subtext0)
                    },
                    selected,
                )));
            }
            SettingsListRow::Value {
                index,
                title,
                description,
                value,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let label_style = if selected {
                    selected_style
                } else {
                    Style::default().fg(p.text)
                };
                let value_style = if selected {
                    selected_style.add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.accent)
                };
                rows.push(ListItem::new(settings_title_value_line(
                    title,
                    value,
                    list_width,
                    label_style,
                    value_style,
                    selected,
                )));
                rows.push(ListItem::new(settings_setting_description_line(
                    description,
                    list_width,
                    if selected {
                        selected_style
                    } else {
                        Style::default().fg(p.subtext0)
                    },
                    selected,
                )));
            }
            SettingsListRow::TextInput {
                index,
                title,
                value,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len() + 1);
                }
                rows.push(ListItem::new(settings_description_line(
                    title,
                    list_width,
                    Style::default().fg(p.text),
                    false,
                )));
                let input_value = if selected {
                    format!("{value}█")
                } else {
                    value.to_string()
                };
                let input_style = Style::default().fg(p.text).bg(p.surface0);
                rows.push(ListItem::new(settings_description_line(
                    &input_value,
                    list_width,
                    input_style,
                    false,
                )));
            }
            SettingsListRow::Choice {
                index,
                label,
                checked,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let label_style = if selected {
                    selected_style
                } else {
                    Style::default().fg(p.text)
                };
                let check_style = if selected {
                    selected_style
                } else if *checked {
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.overlay0)
                };
                rows.push(ListItem::new(settings_choice_line(
                    label,
                    *checked,
                    list_width,
                    label_style,
                    check_style,
                    selected,
                )));
            }
            SettingsListRow::Action {
                index,
                icon,
                label,
                tone,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let style = if selected && *tone == SettingsMarkerTone::Danger {
                    settings_danger_selected_style(p)
                } else if selected {
                    selected_style
                } else if *tone == SettingsMarkerTone::Danger {
                    settings_marker_style(p, *tone)
                } else {
                    Style::default().fg(p.text)
                };
                rows.push(ListItem::new(settings_action_line(
                    icon, label, list_width, style, selected,
                )));
            }
            SettingsListRow::Status {
                index,
                label,
                status,
                tone,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let label_style = if selected {
                    selected_style
                } else {
                    Style::default().fg(p.text)
                };
                let status_style = if selected {
                    selected_style.add_modifier(Modifier::BOLD)
                } else {
                    settings_marker_style(p, *tone)
                };
                rows.push(ListItem::new(settings_status_line(
                    label,
                    status,
                    list_width,
                    label_style,
                    status_style,
                    selected,
                )));
            }
            SettingsListRow::Profile {
                index,
                name,
                detail,
                badge,
                tone,
            } => {
                let selected =
                    app.settings.selection_active && app.settings.list.selected == *index;
                if selected {
                    selected_row = Some(rows.len());
                }
                let name_style = if selected {
                    selected_style
                } else {
                    Style::default().fg(p.text)
                };
                let detail_style = if selected {
                    selected_style
                } else {
                    Style::default().fg(p.subtext0)
                };
                let badge_style = if selected {
                    selected_style.add_modifier(Modifier::BOLD)
                } else {
                    settings_marker_style(p, *tone).add_modifier(Modifier::BOLD)
                };
                rows.push(ListItem::new(settings_profile_name_line(
                    name,
                    detail,
                    badge.as_deref(),
                    list_width,
                    name_style,
                    detail_style,
                    badge_style,
                    selected,
                )));
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
        assert!(text.contains("× delete group"));
        assert!(!text.contains("●"));
        assert!(!text.contains("○"));
        assert!(!text.contains("Work█"));
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
        let (y, x) = find_text_cell(&text, "× delete group").expect("delete action row");
        assert_eq!(buffer[(x, y)].style().fg, Some(app.palette.red));
        assert_eq!(buffer[(x + 2, y)].style().fg, Some(app.palette.red));
        assert_ne!(buffer[(x, y)].style().bg, Some(app.palette.red));

        app.settings.list.selected = 1;
        app.settings.selection_active = true;
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .expect("render group settings overlay");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 80, 24);
        let (y, x) = find_text_cell(&text, "× delete group").expect("delete action row");
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
        assert!(text.contains("✓ terminal"));
        assert!(text.contains("palettes"));
        assert!(text.contains("accent"));
        assert!(text.contains("✓ blue"));
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
        assert!(text.contains("✓ palettes"));
        assert!(text.contains("appearance"));
        assert!(text.contains("✓ automatic"));
        assert!(!text.contains("system terminal"));
        assert!(!text.contains(" mode"));
        assert!(text.contains("light appearance"));
        assert!(text.contains("solarized"));
        assert_no_option_line(&text, "terminal");

        app.settings.list.selected = theme_names_for_appearance(ThemeAppearance::Light).len();
        app.settings.selection_active = true;
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
        app.settings.selection_active = true;
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
        app.settings.selection_active = true;

        let backend = TestBackend::new(100, 50);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, Rect::new(0, 0, 100, 50)))
            .expect("render theme settings");

        let text = buffer_text(terminal.backend().buffer(), 100, 50);
        assert!(!text.contains("✓ automatic"));
        assert!(text.contains("✓ light"));
        assert!(!text.contains("✓ catppuccin latte"));
        assert!(text.contains("✓ solarized"));
    }

    #[test]
    fn theme_settings_selected_row_highlight_extends_to_row_end() {
        let mut app = AppState::test_new();
        app.global_theme_mode = ThemeMode::System;
        app.settings.section = SettingsSection::Theme;
        app.settings.pending_theme_mode = Some(ThemeMode::Light);
        app.settings.list.selected = 3;
        app.settings.selection_active = true;

        let area = Rect::new(0, 0, 100, 80);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, area.width, area.height);
        let (selected_y, selected_x) =
            find_text_cell(&text, "✓ light").expect("selected light row");
        let selected_row_end = area.x + area.width.saturating_sub(1);
        assert_eq!(
            buffer[(selected_x, selected_y)].style().bg,
            Some(app.palette.accent)
        );
        assert_eq!(
            buffer[(selected_row_end, selected_y)].style().bg,
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
        app.settings.selection_active = true;

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
        let selected_row_end = area.x + area.width.saturating_sub(2);

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
        app.settings.selection_active = true;

        let area = Rect::new(0, 0, 100, 50);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_theme(&app, frame, area))
            .expect("render theme settings");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, area.width, area.height);
        let (y, x) = find_text_cell(&text, "✓ light").expect("selected light row");
        assert_eq!(buffer[(x, y)].symbol(), "✓");
        assert_eq!(buffer[(x.saturating_add(1), y)].symbol(), " ");
        assert_eq!(buffer[(x.saturating_add(2), y)].symbol(), "l");
    }

    #[test]
    fn agent_settings_renders_custom_profile_management() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Agents;

        let area = Rect::new(0, 0, 100, 40);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("agents"));
        assert!(text.contains("new custom profile"));
        assert!(text.contains("custom profiles"));
        assert!(!text.contains("show shift+←→"));
    }

    #[test]
    fn group_profile_settings_renders_profile_sections_without_filters() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        app.settings.group_settings_target = Some(group_idx);
        app.settings.section = SettingsSection::GroupProfiles;
        app.settings.agent_profile_kind_filter = Some(crate::agent_profiles::AgentKind::Omp);

        let area = Rect::new(0, 0, 100, 40);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render group profile settings");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("group settings"));
        assert!(text.contains("favorites"));
        assert!(text.contains("available"));
        assert!(!text.contains("show shift+←→"));
    }
    #[test]
    fn agent_profile_editor_renders_numbered_steps() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Agents;
        app.settings.pending_agent_profile_name = Some("omp mk".to_string());
        app.settings.pending_agent_profile_command = Some("omp-mk".to_string());
        app.settings.pending_agent_profile_kind = Some(crate::agent_profiles::AgentKind::Omp);
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Omp,
            label: "omp",
            command: "omp",
            available: true,
            path: std::path::PathBuf::from("/tmp/hako-test-omp"),
            state: crate::integration::IntegrationStatusKind::Current,
        }];

        let area = Rect::new(0, 0, 100, 40);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("← back"));
        assert!(text.contains("1. name"));
        assert!(text.contains("label shown in menus"));
        assert!(text.contains("2. kind"));
        assert!(text.contains("choose an installed integration family"));
        app.settings.scroll = 12;
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render scrolled settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        assert!(text.contains("3. command"));
        assert!(text.contains("shell command to run"));
        assert!(text.contains("4. actions"));
    }

    #[test]
    fn agent_profile_editor_selected_choice_does_not_shift_text() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Agents;
        app.settings.pending_agent_profile_name = Some("omp mk".to_string());
        app.settings.pending_agent_profile_command = Some("omp-mk".to_string());
        app.settings.pending_agent_profile_kind = Some(crate::agent_profiles::AgentKind::Codex);
        app.integration_recommendations = vec![
            crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Claude,
                label: "claude",
                command: "claude",
                available: true,
                path: std::path::PathBuf::from("/tmp/hako-test-claude"),
                state: crate::integration::IntegrationStatusKind::Current,
            },
            crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Codex,
                label: "codex",
                command: "codex",
                available: true,
                path: std::path::PathBuf::from("/tmp/hako-test-codex"),
                state: crate::integration::IntegrationStatusKind::Current,
            },
        ];
        app.settings.list.selected = 1;
        app.settings.selection_active = true;

        let area = Rect::new(0, 0, 100, 40);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, area.width, area.height);
        let (selected_y, selected_x) =
            find_text_cell(&text, "✓ codex").expect("selected codex kind");
        let (_, claude_x) = find_text_cell(&text, "  claude").expect("unselected claude kind");
        assert_eq!(selected_x, claude_x);
        assert_eq!(buffer[(selected_x, selected_y)].symbol(), "✓");
        assert_eq!(
            buffer[(selected_x, selected_y)].style().bg,
            Some(app.palette.accent)
        );
    }

    #[test]
    fn toast_settings_render_as_value_row() {
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
        let delivery_row = lines
            .iter()
            .position(|line| line.contains("toast delivery") && line.contains("off"))
            .expect("toast delivery row");
        let description_row = lines
            .iter()
            .position(|line| line.contains("where notification popups should appear"))
            .expect("toast delivery description row");

        assert_eq!(delivery_row, header_row + 1);
        assert_eq!(description_row, delivery_row + 1);
        assert!(!text.contains("inside hako"));
        assert!(!text.contains("via terminal"));
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
            .find(|line| line.contains("appearance") && line.contains("notifications"))
            .expect("tab line");
        assert!(tab_line.contains("advanced"));
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
            .position(|line| line.contains("appearance") && line.contains("notifications"))
            .expect("settings tab row");

        assert_eq!(tab_row, title_row + 2);
    }

    #[test]
    fn settings_tabs_render_descriptions_consistently() {
        let expected = [
            (
                SettingsSection::Theme,
                "appearance",
                "configure theme, sidebar layout, and pane appearance",
            ),
            (
                SettingsSection::Sound,
                "notifications",
                "choose sound and toast notification behavior",
            ),
            (
                SettingsSection::PaneLabels,
                "behavior",
                "control workspace prompts and terminal interaction defaults",
            ),
            (
                SettingsSection::Agents,
                "agents",
                "create custom commands and manage agent profiles",
            ),
            (
                SettingsSection::Integrations,
                "agent integrations",
                "install hooks so agents report state directly",
            ),
            (
                SettingsSection::Experiments,
                "advanced",
                "configure advanced or platform-specific behavior",
            ),
        ];

        assert_eq!(expected.len(), SettingsSection::ALL.len());
        for (&(section, title, description), expected_section) in
            expected.iter().zip(SettingsSection::ALL)
        {
            assert_eq!(section, *expected_section);

            let mut app = AppState::test_new();
            app.settings.section = section;

            let area = Rect::new(0, 0, 100, 30);
            let backend = TestBackend::new(area.width, area.height);
            let mut terminal = Terminal::new(backend).expect("test backend");
            terminal
                .draw(|frame| render_settings_overlay(&app, frame, area))
                .expect("render settings overlay");

            let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
            assert!(text.contains(title), "missing title for {section:?}");
            assert!(
                text.contains(description),
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
        assert!(text.contains("sidebar arrangement"));
        assert!(text.contains("auto"));
        assert!(text.contains("default sidebar width"));
        assert!(text.contains("26 cols"));
        assert!(text.contains("minimum sidebar width"));
        assert!(text.contains("18 cols"));
        assert!(text.contains("maximum sidebar width"));
        assert!(text.contains("36 cols"));
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
        assert!(text.contains("worktree directory"));
        assert!(text.contains("/tmp/hako-worktrees"));
        assert!(!text.contains("agent border labels"));
    }
    #[test]
    fn sectioned_settings_selected_text_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 1;
        app.settings.selection_active = true;

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
    fn settings_rows_open_without_selected_highlight() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 0;
        app.settings.selection_active = false;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (row, col) = find_text_cell(&text, "sidebar arrangement").expect("layout setting row");

        assert_ne!(
            terminal.backend().buffer()[(col, row)].style().fg,
            Some(panel_contrast_fg(&app.palette))
        );
        assert_ne!(
            terminal.backend().buffer()[(col, row)].style().bg,
            Some(app.palette.accent)
        );
    }

    #[test]
    fn selected_value_row_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Layout;
        app.settings.list.selected = 0;
        app.settings.selection_active = true;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, selected_x) =
            find_text_cell(&text, "sidebar arrangement").expect("selected layout row");

        assert_eq!(
            terminal.backend().buffer()[(selected_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
        assert!(!text.contains("combined"));
        assert!(text.contains("auto"));
    }

    #[test]
    fn selected_toggle_row_uses_selected_foreground() {
        let mut app = AppState::test_new();
        app.switch_ascii_input_source_in_prefix = false;
        app.settings.section = SettingsSection::Experiments;
        app.settings.list.selected = 0;
        app.settings.selection_active = true;

        let area = Rect::new(0, 0, 100, 30);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay(&app, frame, area))
            .expect("render settings overlay");

        let text = buffer_text(terminal.backend().buffer(), area.width, area.height);
        let (selected_y, selected_x) =
            find_text_cell(&text, "switch to ascii input source in prefix (macOS)")
                .expect("selected experiment row");

        assert_eq!(
            terminal.backend().buffer()[(selected_x, selected_y)].symbol(),
            "s"
        );
        assert_eq!(
            terminal.backend().buffer()[(selected_x, selected_y)]
                .style()
                .fg,
            Some(panel_contrast_fg(&app.palette))
        );
    }

    #[test]
    fn settings_tabs_do_not_show_integration_badges() {
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
        assert!(text.contains("integrations"));
        assert!(!text.contains("● integrations"));
    }
    #[test]
    fn integrations_selected_row_highlight_extends_to_row_end() {
        let mut app = AppState::test_new();
        app.settings.section = SettingsSection::Integrations;
        app.settings.list.selected = 0;
        app.settings.selection_active = true;
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

        let popup = crate::ui::widgets::centered_popup_rect(area, 92, 26).expect("popup");
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
        app.settings.selection_active = true;

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
        assert!(text.contains("switch to ascii input source in prefix (macOS)"));
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
                !in_appearance_section
                    || (line != option
                        && line != format!("{option} ✓")
                        && line != format!("✓ {option}")),
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
