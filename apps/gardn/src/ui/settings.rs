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
        state::{Palette, SettingsSection, SettingsState},
        AppState,
    },
    settings_rows::{
        rows_for_section_for_view, visual_row_count, SettingsListRow, SettingsMarkerTone,
    },
};

const GROUP_SETTINGS_SECTIONS: &[SettingsSection] = &[
    SettingsSection::GroupGeneral,
    SettingsSection::GroupDefaults,
    SettingsSection::Theme,
    SettingsSection::GroupProfiles,
    SettingsSection::GroupGithub,
];
const WORKSPACE_SETTINGS_SECTIONS: &[SettingsSection] = &[
    SettingsSection::WorkspaceGeneral,
    SettingsSection::WorkspaceGithub,
];

const SETTINGS_SIDEBAR_WIDTH: u16 = 21;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettingsSidebarEntry {
    pub(crate) section: SettingsSection,
    pub(crate) subsection: Option<usize>,
    pub(crate) label: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SettingsSidebarAreas {
    pub(crate) navigation: Rect,
    pub(crate) divider: Rect,
    pub(crate) content: Rect,
}

#[derive(Clone, Copy)]
struct SettingsSubsection {
    label: &'static str,
    anchor: Option<&'static str>,
}

const APPEARANCE_SUBSECTIONS: &[SettingsSubsection] = &[
    SettingsSubsection {
        label: "Colors",
        anchor: Some("Colors"),
    },
    SettingsSubsection {
        label: "Themes",
        anchor: Some("Appearance"),
    },
    SettingsSubsection {
        label: "Sidebar",
        anchor: Some("Sidebar"),
    },
    SettingsSubsection {
        label: "Panes",
        anchor: Some("Panes"),
    },
    SettingsSubsection {
        label: "Agent Status",
        anchor: Some("Agent Status"),
    },
    SettingsSubsection {
        label: "Window",
        anchor: Some("Window"),
    },
];
const NOTIFICATION_SUBSECTIONS: &[SettingsSubsection] = &[
    SettingsSubsection {
        label: "Sound alerts",
        anchor: Some("Sound Alerts"),
    },
    SettingsSubsection {
        label: "Popups",
        anchor: Some("Notification Popups"),
    },
    SettingsSubsection {
        label: "Clipboard",
        anchor: Some("Clipboard Feedback"),
    },
];
const BEHAVIOR_SUBSECTIONS: &[SettingsSubsection] = &[
    SettingsSubsection {
        label: "General",
        anchor: Some("General"),
    },
    SettingsSubsection {
        label: "Selection",
        anchor: Some("Selection"),
    },
    SettingsSubsection {
        label: "Terminal",
        anchor: Some("Terminal"),
    },
    SettingsSubsection {
        label: "Sessions",
        anchor: Some("Sessions"),
    },
];
const COMMAND_SUBSECTIONS: &[SettingsSubsection] = &[SettingsSubsection {
    label: "Project commands",
    anchor: Some("Project Commands"),
}];
const AGENT_SUBSECTIONS: &[SettingsSubsection] = &[SettingsSubsection {
    label: "Profiles",
    anchor: Some("Saved Profiles"),
}];
const INTEGRATION_SUBSECTIONS: &[SettingsSubsection] = &[SettingsSubsection {
    label: "Agent tools",
    anchor: None,
}];
const CONNECTION_SUBSECTIONS: &[SettingsSubsection] = &[SettingsSubsection {
    label: "SSH profiles",
    anchor: Some("Saved Profiles"),
}];
const ADVANCED_SUBSECTIONS: &[SettingsSubsection] = &[
    SettingsSubsection {
        label: "Input",
        anchor: Some("Input"),
    },
    SettingsSubsection {
        label: "Server",
        anchor: Some("Server"),
    },
    SettingsSubsection {
        label: "Updates",
        anchor: Some("Updates"),
    },
];

fn settings_sidebar_section_label(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Theme => "Appearance",
        SettingsSection::Sound => "Notifications",
        SettingsSection::PaneLabels => "Behavior",
        SettingsSection::Commands => "Commands",
        SettingsSection::Agents => "Agents",
        SettingsSection::Integrations => "Integrations",
        SettingsSection::Connections => "Connections",
        SettingsSection::Experiments => "Advanced",
        SettingsSection::About => "About",
        SettingsSection::Layout
        | SettingsSection::Toast
        | SettingsSection::GroupProfiles
        | SettingsSection::GroupGeneral
        | SettingsSection::GroupDefaults
        | SettingsSection::GroupGithub
        | SettingsSection::WorkspaceGeneral
        | SettingsSection::WorkspaceGithub => section.label(),
    }
}

fn settings_subsections(section: SettingsSection) -> &'static [SettingsSubsection] {
    match section {
        SettingsSection::Theme => APPEARANCE_SUBSECTIONS,
        SettingsSection::Sound => NOTIFICATION_SUBSECTIONS,
        SettingsSection::PaneLabels => BEHAVIOR_SUBSECTIONS,
        SettingsSection::Commands => COMMAND_SUBSECTIONS,
        SettingsSection::Agents => AGENT_SUBSECTIONS,
        SettingsSection::Integrations => INTEGRATION_SUBSECTIONS,
        SettingsSection::Connections => CONNECTION_SUBSECTIONS,
        SettingsSection::Experiments => ADVANCED_SUBSECTIONS,
        SettingsSection::About
        | SettingsSection::Layout
        | SettingsSection::Toast
        | SettingsSection::GroupProfiles
        | SettingsSection::GroupGeneral
        | SettingsSection::GroupDefaults
        | SettingsSection::GroupGithub
        | SettingsSection::WorkspaceGeneral
        | SettingsSection::WorkspaceGithub => &[],
    }
}

pub(crate) fn settings_subsection_anchor(
    section: SettingsSection,
    subsection: usize,
) -> Option<&'static str> {
    settings_subsections(section)
        .get(subsection)
        .and_then(|subsection| subsection.anchor)
}

pub(crate) fn settings_sidebar_entries(settings: &SettingsState) -> Vec<SettingsSidebarEntry> {
    let mut entries = Vec::with_capacity(SettingsSection::ALL.len() + 4);
    for &section in SettingsSection::ALL {
        entries.push(SettingsSidebarEntry {
            section,
            subsection: None,
            label: settings_sidebar_section_label(section),
        });
        if settings.sidebar_expanded == Some(section) {
            entries.extend(settings_subsections(section).iter().enumerate().map(
                |(subsection, item)| SettingsSidebarEntry {
                    section,
                    subsection: Some(subsection),
                    label: item.label,
                },
            ));
        }
    }
    entries
}

pub(crate) fn settings_sidebar_areas(area: Rect) -> SettingsSidebarAreas {
    let [navigation, divider, _, content] = Layout::horizontal([
        Constraint::Length(SETTINGS_SIDEBAR_WIDTH.min(area.width.saturating_sub(3))),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<4>(area);
    SettingsSidebarAreas {
        navigation,
        divider,
        content,
    }
}

pub(crate) fn settings_sidebar_hit_areas(
    settings: &SettingsState,
    area: Rect,
) -> Vec<(SettingsSidebarEntry, Rect)> {
    settings_sidebar_entries(settings)
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let y = area.y.saturating_add(2).saturating_add(index as u16);
            (y < area.y.saturating_add(area.height))
                .then_some((entry, Rect::new(area.x, y, area.width, 1)))
        })
        .collect()
}

fn settings_sections_for(
    settings: &crate::app::state::SettingsState,
) -> &'static [SettingsSection] {
    if settings.group_settings_target.is_some() {
        GROUP_SETTINGS_SECTIONS
    } else if settings.workspace_settings_target.is_some() {
        WORKSPACE_SETTINGS_SECTIONS
    } else {
        SettingsSection::ALL
    }
}

fn settings_tab_text_for(
    settings: &crate::app::state::SettingsState,
    section: SettingsSection,
) -> &'static str {
    if settings.group_settings_target.is_some() && section == SettingsSection::Theme {
        "Appearance"
    } else {
        section.label()
    }
}

fn settings_tab_width_for(
    settings: &crate::app::state::SettingsState,
    section: SettingsSection,
) -> u16 {
    settings_tab_text_for(settings, section).width() as u16 + 2
}

fn settings_visible_tab_range_for(
    settings: &crate::app::state::SettingsState,
    row_width: u16,
) -> (usize, usize) {
    let sections = settings_sections_for(settings);
    let selected = sections
        .iter()
        .position(|section| *section == settings.section)
        .unwrap_or(0);
    super::modal_tabs::visible_tab_range(sections.len(), selected, row_width, |idx| {
        settings_tab_width_for(settings, sections[idx])
    })
}

fn render_settings_tabs_for_view(
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    row: Rect,
    p: &crate::app::state::Palette,
) {
    let settings = &client_view.settings;
    let sections = settings_sections_for(settings);
    let (start, end) = settings_visible_tab_range_for(settings, row.width);
    let mut spans = Vec::new();

    if start > 0 {
        spans.push(Span::styled("‹ ", Style::default().fg(p.overlay0)));
    }

    for (visible_idx, section) in sections[start..end].iter().copied().enumerate() {
        if visible_idx > 0 {
            spans.push(Span::raw(" "));
        }

        let selected = section == settings.section;
        let tab_style = if selected {
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(p.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.overlay1)
        };
        spans.push(Span::styled(" ", tab_style));
        spans.push(Span::styled(
            settings_tab_text_for(settings, section),
            tab_style,
        ));
        spans.push(Span::styled(" ", tab_style));
    }

    if end < sections.len() {
        spans.push(Span::styled(" ›", Style::default().fg(p.overlay0)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), row);
}

fn render_settings_sidebar(settings: &SettingsState, frame: &mut Frame, area: Rect, p: &Palette) {
    if area.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new("SETTINGS")
            .style(Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD)),
        Rect::new(area.x, area.y, area.width, 1),
    );

    for (entry, row) in settings_sidebar_hit_areas(settings, area) {
        let selected = settings.sidebar_selection.section == entry.section
            && settings.sidebar_selection.subsection == entry.subsection;
        let active = if let Some(subsection) = entry.subsection {
            settings.section == entry.section
                && (settings.sidebar_selection.subsection == Some(subsection)
                    || (settings.sidebar_selection.subsection.is_none() && subsection == 0))
        } else {
            settings.section == entry.section
        };
        let mut style = if active {
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.overlay1)
        };
        if selected && settings.sidebar_focused {
            style = style.bg(p.surface1);
        }

        let text = if entry.subsection.is_some() {
            let marker = if active { "*" } else { " " };
            format!("  {marker} {}", entry.label)
        } else {
            let chevron = if settings.sidebar_expanded == Some(entry.section) {
                "▾"
            } else {
                "▸"
            };
            format!("{chevron} {}", entry.label)
        };
        frame.render_widget(Paragraph::new(text).style(style), row);
    }
}

fn render_settings_sidebar_divider(frame: &mut Frame, area: Rect, p: &Palette) {
    for y in area.y..area.y.saturating_add(area.height) {
        frame.render_widget(
            Paragraph::new("│").style(Style::default().fg(p.surface1)),
            Rect::new(area.x, y, area.width, 1),
        );
    }
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

const SETTINGS_INTEGRATIONS_HINTS: &[(&str, &str)] =
    &[("Move", "↑↓"), ("Action", "Space/↵"), ("Section", "←→/Tab")];
const SETTINGS_AGENTS_EDITOR_HINTS: &[(&str, &str)] =
    &[("Move", "↑↓"), ("Action", "Space/↵"), ("Section", "←→/Tab")];
const SETTINGS_AGENTS_HINTS: &[(&str, &str)] = &[
    ("Move", "↑↓"),
    ("New/Edit", "Space/↵"),
    ("Delete", "Ctrl+D"),
    ("Section", "←→/Tab"),
];
const SETTINGS_CONNECTIONS_HINTS: &[(&str, &str)] = &[
    ("Move", "↑↓"),
    ("New/Edit", "Space/↵"),
    ("Delete", "Ctrl+D"),
    ("Section", "←→/Tab"),
];
const SETTINGS_CONNECTIONS_EDITOR_HINTS: &[(&str, &str)] =
    &[("Move", "↑↓"), ("Action", "Space/↵"), ("Section", "←→/Tab")];
const SETTINGS_GROUP_PROFILES_HINTS: &[(&str, &str)] = &[
    ("Move", "↑↓"),
    ("Favorite", "Ctrl+F"),
    ("Default", "Ctrl+D"),
    ("Section", "←→/tab"),
];
const SETTINGS_GROUP_HINTS: &[(&str, &str)] =
    &[("Move", "↑↓"), ("Action", "Space/↵"), ("Section", "←→/Tab")];
const SETTINGS_DEFAULT_HINTS: &[(&str, &str)] =
    &[("Move", "↑↓"), ("Action", "Space/↵"), ("Section", "←→/Tab")];
const SETTINGS_SIDEBAR_HINTS: &[(&str, &str)] =
    &[("Move", "↑↓"), ("Action", "Space/↵"), ("Sidebar", "Tab")];
const SETTINGS_SIDEBAR_AGENTS_HINTS: &[(&str, &str)] = &[
    ("Move", "↑↓"),
    ("New/Edit", "Space/↵"),
    ("Delete", "Ctrl+D"),
    ("Sidebar", "Tab"),
];
const SETTINGS_SIDEBAR_CONNECTIONS_HINTS: &[(&str, &str)] = SETTINGS_SIDEBAR_AGENTS_HINTS;

fn general_settings_sidebar_visible(settings: &SettingsState) -> bool {
    settings.group_settings_target.is_none() && settings.workspace_settings_target.is_none()
}

fn settings_stack_areas_for(
    settings: &SettingsState,
    inner: Rect,
) -> super::widgets::ModalStackAreas {
    let footer_rows = modal_hint_line_count(inner.width, settings_footer_hints_for(settings), 2);
    let header_rows = if general_settings_sidebar_visible(settings) {
        1
    } else {
        4
    };
    modal_stack_areas(inner, header_rows, footer_rows, 0, 1)
}

pub(super) fn render_settings_overlay_for_view(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    render_settings_overlay_with(app, client_view, frame, area);
}

fn render_settings_overlay_with(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let settings = &client_view.settings;
    let palette = if let Some(group_idx) = settings.group_settings_target {
        app.palette_for_group(group_idx)
    } else if let Some(workspace_idx) = settings.workspace_settings_target {
        app.palette_for_workspace(workspace_idx)
    } else {
        app.palette.clone()
    };
    let title = if settings.group_settings_target.is_some() {
        "Group Settings"
    } else if settings.workspace_settings_target.is_some() {
        "Space Settings"
    } else {
        "Settings"
    };
    super::dim_background(frame, area);
    let Some(frame_areas) = render_modal_frame(
        frame,
        area,
        &palette,
        ModalFrameSpec {
            title,
            width: 92,
            height: 26,
            header_rows: if general_settings_sidebar_visible(settings) {
                1
            } else {
                4
            },
            footer_hints: settings_footer_hints_for(settings),
            footer_max_rows: 2,
            gap: 1,
            actions_rows: 0,
            show_close: true,
        },
    ) else {
        return;
    };
    let inner = frame_areas.inner;
    if inner.height < 4 || inner.width < 10 {
        return;
    }
    let stack = settings_stack_areas_for(settings, inner);
    if general_settings_sidebar_visible(settings) {
        let sidebar = settings_sidebar_areas(stack.content);
        render_settings_sidebar(settings, frame, sidebar.navigation, &palette);
        render_settings_sidebar_divider(frame, sidebar.divider, &palette);
        render_settings_content_for_view(app, client_view, frame, sidebar.content, &palette);
    } else {
        let header_rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<4>(stack.header);
        render_settings_tabs_for_view(client_view, frame, header_rows[2], &palette);
        render_modal_divider(frame, header_rows[3], &palette);
        render_settings_content_for_view(app, client_view, frame, stack.content, &palette);
    }
}

fn settings_agents_editor_open_for(settings: &crate::app::state::SettingsState) -> bool {
    settings.pending_agent_profile_id.is_some()
        || settings.pending_agent_profile_name.is_some()
        || settings.pending_agent_profile_command.is_some()
}

fn settings_footer_hints_for(
    settings: &crate::app::state::SettingsState,
) -> &'static [(&'static str, &'static str)] {
    if general_settings_sidebar_visible(settings) {
        return if settings.section == SettingsSection::Agents
            && !settings_agents_editor_open_for(settings)
        {
            SETTINGS_SIDEBAR_AGENTS_HINTS
        } else if settings.section == SettingsSection::Connections
            && !crate::settings_rows::connection_editor_open(settings)
        {
            SETTINGS_SIDEBAR_CONNECTIONS_HINTS
        } else {
            SETTINGS_SIDEBAR_HINTS
        };
    }
    if settings.section == SettingsSection::Integrations {
        SETTINGS_INTEGRATIONS_HINTS
    } else if settings.section == SettingsSection::Agents {
        if settings_agents_editor_open_for(settings) {
            SETTINGS_AGENTS_EDITOR_HINTS
        } else {
            SETTINGS_AGENTS_HINTS
        }
    } else if settings.section == SettingsSection::Connections {
        if crate::settings_rows::connection_editor_open(settings) {
            SETTINGS_CONNECTIONS_EDITOR_HINTS
        } else {
            SETTINGS_CONNECTIONS_HINTS
        }
    } else if settings.section == SettingsSection::GroupProfiles {
        SETTINGS_GROUP_PROFILES_HINTS
    } else if settings.group_settings_target.is_some() {
        SETTINGS_GROUP_HINTS
    } else {
        SETTINGS_DEFAULT_HINTS
    }
}

fn settings_section_title_for(
    settings: &crate::app::state::SettingsState,
    section: SettingsSection,
) -> &'static str {
    if section == SettingsSection::Agents && settings_agents_editor_open_for(settings) {
        if settings.pending_agent_profile_id.is_some() {
            "Edit Agent Profile"
        } else {
            "New Agent Profile"
        }
    } else if section == SettingsSection::Connections
        && crate::settings_rows::connection_editor_open(settings)
    {
        let editor = settings.connection_editor.as_ref();
        if editor.is_some_and(|editor| editor.is_detail()) {
            "Connection Details"
        } else if editor.is_some_and(|editor| editor.is_editing()) {
            "Edit Connection"
        } else {
            "Add Connection"
        }
    } else {
        settings_section_title_for_non_editor(section)
    }
}

fn settings_section_title_for_non_editor(section: SettingsSection) -> &'static str {
    match section {
        SettingsSection::Theme => "Appearance",
        SettingsSection::Layout => "Layout",
        SettingsSection::Sound => "Notifications",
        SettingsSection::Toast => "Toasts",
        SettingsSection::PaneLabels => "Behavior",
        SettingsSection::Commands => "Commands",
        SettingsSection::Experiments => "Advanced",
        SettingsSection::Agents => "Agents",
        SettingsSection::Integrations => "Agent Integrations",
        SettingsSection::Connections => "Connections",
        SettingsSection::GroupGeneral => "General",
        SettingsSection::GroupDefaults => "Space Defaults",
        SettingsSection::GroupProfiles => "Agents",
        SettingsSection::GroupGithub => "GitHub",
        SettingsSection::WorkspaceGeneral => "General",
        SettingsSection::WorkspaceGithub => "GitHub",
        SettingsSection::About => "About",
    }
}

fn settings_section_description_for(
    settings: &crate::app::state::SettingsState,
    section: SettingsSection,
) -> &'static str {
    match section {
        SettingsSection::Theme if settings.group_settings_target.is_some() => {
            "Choose a theme accent for this group, or inherit the global accent"
        }
        SettingsSection::Theme => "Configure theme, sidebar layout, and pane appearance",
        SettingsSection::Layout => "Set sidebar width bounds",
        SettingsSection::Sound => "Choose sound and toast notification behavior",
        SettingsSection::Toast => "Choose where command and agent notifications are delivered",
        SettingsSection::PaneLabels => {
            "Control workspace prompts and terminal interaction defaults"
        }
        SettingsSection::Commands => "Edit launch commands; clear one to disable and hide it",
        SettingsSection::Experiments => "Configure advanced or platform-specific behavior",
        SettingsSection::Agents if settings_agents_editor_open_for(settings) => {
            "Configure the label, agent type, and launch command"
        }
        SettingsSection::Agents => "Create and manage agent launch profiles",
        SettingsSection::Integrations => "Install hooks so agents report state directly",
        SettingsSection::Connections
            if settings
                .connection_editor
                .as_ref()
                .is_some_and(|editor| editor.is_detail()) =>
        {
            "Connect, test, or open a workspace on this SSH host"
        }
        SettingsSection::Connections if crate::settings_rows::connection_editor_open(settings) => {
            "Credentials and host keys stay with OpenSSH; Gardn never stores them"
        }
        SettingsSection::Connections => "Add SSH hosts and manage their connections",
        SettingsSection::GroupGeneral => "Rename this group, change its icon, or delete it",
        SettingsSection::GroupDefaults => "Choose where new Spaces start",
        SettingsSection::GroupProfiles => {
            "Choose favorite and default agent profiles for this group"
        }
        SettingsSection::GroupGithub => "Spaces inherit this organization. Enter saves.",
        SettingsSection::WorkspaceGeneral => "Set this Space's name, host, and directory",
        SettingsSection::WorkspaceGithub => "Set repository scope. Enter saves the list.",
        SettingsSection::About => "Open-source projects and contributors behind Gardn",
    }
}

fn render_settings_section_intro_for_view(
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
) -> Rect {
    let settings = &client_view.settings;
    let [desc_area, divider_area, list_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(2),
    ])
    .areas::<3>(area);
    let [title_area, description_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas::<2>(desc_area);
    let editor_open = match settings.section {
        SettingsSection::Agents => settings_agents_editor_open_for(settings),
        SettingsSection::Connections => crate::settings_rows::connection_editor_open(settings),
        _ => false,
    };
    let back = editor_open.then(|| {
        let width = action_button_width(None, "← back");
        Rect::new(
            title_area.x + title_area.width.saturating_sub(width),
            title_area.y,
            width,
            1,
        )
    });
    let title_width = back
        .map(|back| back.x.saturating_sub(title_area.x).saturating_sub(1))
        .unwrap_or(title_area.width);
    render_modal_description(
        frame,
        Rect::new(title_area.x, title_area.y, title_width, title_area.height),
        settings_section_title_for(settings, settings.section),
        Style::default().fg(p.accent),
    );
    if let Some(back) = back {
        render_action_button(frame, back, None, "← Back", secondary_action_style(p));
    }
    render_modal_description(
        frame,
        description_area,
        settings_section_description_for(settings, settings.section),
        Style::default().fg(p.overlay0),
    );
    render_modal_divider(frame, divider_area, p);
    list_area
}

fn render_settings_content_for_view(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
) {
    let settings = &client_view.settings;
    let body_area = render_settings_section_intro_for_view(client_view, frame, area, p);
    if settings.section != SettingsSection::Integrations {
        render_settings_rows_for_view(
            rows_for_section_for_view(app, client_view),
            settings,
            frame,
            body_area,
            p,
        );
        return;
    }

    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas::<2>(body_area);
    if app.integration_recommendations.is_empty() && app.ssh_connection_profiles.is_empty() {
        frame.render_widget(
            Paragraph::new(settings_description_line(
                "No integration targets available",
                list_area.width as usize,
                Style::default().fg(p.overlay1),
                false,
            )),
            list_area,
        );
    } else {
        render_settings_rows_for_view(
            rows_for_section_for_view(app, client_view),
            settings,
            frame,
            list_area,
            p,
        );
    }

    let feedback = integration_feedback_for_settings(app, settings);
    if !feedback.is_empty() {
        render_settings_integration_feedback(frame, hint_area, p, feedback);
        return;
    }

    let hint = integration_hint_for_selection(app, settings);
    frame.render_widget(
        Paragraph::new(format!(" {hint}")).style(Style::default().fg(p.overlay0)),
        settings_integration_hint_row(hint_area),
    );
}

fn integration_feedback_for_settings<'a>(
    app: &'a AppState,
    settings: &crate::app::state::SettingsState,
) -> &'a [String] {
    let selection = crate::app::integration_host::resolve(app, settings);
    let Some(host_id) = selection.host_id() else {
        return &app.integration_install_messages;
    };
    app.host_integration_install_messages
        .get(host_id)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn integration_hint_for_selection(
    app: &AppState,
    settings: &crate::app::state::SettingsState,
) -> String {
    let has_host_selector = !app.ssh_connection_profiles.is_empty();
    let selected = settings.list.selected;
    if has_host_selector && selected == 0 {
        return "Press enter to change the integration host".to_string();
    }
    let entry_index = selected.saturating_sub(usize::from(has_host_selector));

    let selection = crate::app::integration_host::resolve(app, settings);
    if let Some(host_id) = selection.host_id() {
        return match app.host_integration_observations.get(host_id) {
            Some(crate::integration::host::HostIntegrationObservation::Ready(snapshot)) => {
                let selected_entry = snapshot.entries.get(entry_index);
                let needs_install = snapshot.entries.iter().any(|entry| {
                    integration_status_needs_install(
                        entry.state,
                        entry.available,
                        entry.missing_profile_hooks,
                    )
                });
                let found_any = snapshot.entries.iter().any(|entry| {
                    entry.available
                        || entry.state != crate::integration::IntegrationStatusKind::NotInstalled
                });
                integration_hint_for_status(
                    selected_entry
                        .map(|entry| (entry.state, entry.available, entry.missing_profile_hooks)),
                    needs_install,
                    found_any,
                )
            }
            Some(crate::integration::host::HostIntegrationObservation::Failed(_)) => {
                "Integration status check failed on the selected host".to_string()
            }
            Some(crate::integration::host::HostIntegrationObservation::Pending) | None => {
                "Waiting for integration status from the selected host".to_string()
            }
        };
    }

    let selected_entry = app
        .integration_recommendations
        .get(entry_index)
        .map(|item| {
            (
                item.state,
                item.available,
                crate::integration::missing_profile_hook_count_for_target(
                    item.target,
                    &app.agent_profiles,
                ),
            )
        });
    let needs_install = app
        .integration_recommendations
        .iter()
        .any(crate::integration::IntegrationRecommendation::needs_install);
    let found_any = app.integration_recommendations.iter().any(|item| {
        item.available || item.state != crate::integration::IntegrationStatusKind::NotInstalled
    });
    integration_hint_for_status(selected_entry, needs_install, found_any)
}

fn integration_hint_for_status(
    selected: Option<(crate::integration::IntegrationStatusKind, bool, usize)>,
    needs_install: bool,
    found_any: bool,
) -> String {
    if let Some((state, available, missing_profile_hooks)) = selected {
        match state {
            crate::integration::IntegrationStatusKind::Current if missing_profile_hooks > 0 => {
                "Press enter to repair profile hooks".to_string()
            }
            crate::integration::IntegrationStatusKind::Current => {
                "Press enter to uninstall selected integration (affects configured profiles)"
                    .to_string()
            }
            crate::integration::IntegrationStatusKind::Outdated => {
                "Press enter to update selected integration".to_string()
            }
            crate::integration::IntegrationStatusKind::NotInstalled if available => {
                "Press enter to install selected integration".to_string()
            }
            crate::integration::IntegrationStatusKind::NotInstalled => {
                "Selected integration is unavailable".to_string()
            }
        }
    } else if needs_install {
        "Press enter to add available or outdated integrations".to_string()
    } else if found_any {
        "All detected integrations are installed".to_string()
    } else {
        "No supported agent CLIs found on PATH".to_string()
    }
}

fn integration_status_needs_install(
    state: crate::integration::IntegrationStatusKind,
    available: bool,
    missing_profile_hooks: usize,
) -> bool {
    state == crate::integration::IntegrationStatusKind::Outdated
        || (state == crate::integration::IntegrationStatusKind::NotInstalled && available)
        || (state == crate::integration::IntegrationStatusKind::Current
            && missing_profile_hooks > 0)
}
fn render_settings_rows_for_view(
    model_rows: Option<Vec<SettingsListRow>>,
    settings: &crate::app::state::SettingsState,
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
) {
    let Some(model_rows) = model_rows else {
        return;
    };
    let total_items = visual_row_count(&model_rows);
    let viewport =
        crate::ui::ModalListViewport::new(total_items, area.height as usize, settings.scroll);
    let scroll = viewport.scroll();
    let scroll_area = viewport.scroll_area(area);
    let list_width = scroll_area.body.width as usize;
    let mut selected_row = None;
    let mut rows = Vec::with_capacity(total_items);
    for row in &model_rows {
        let selected_index = match row {
            SettingsListRow::Toggle { index, .. }
            | SettingsListRow::Value { index, .. }
            | SettingsListRow::TextInput { index, .. }
            | SettingsListRow::Choice { index, .. }
            | SettingsListRow::Action { index, .. }
            | SettingsListRow::Status { index, .. }
            | SettingsListRow::Profile { index, .. } => Some(*index),
            SettingsListRow::Header(_)
            | SettingsListRow::Caption(_)
            | SettingsListRow::Spacer
            | SettingsListRow::GroupIconPicker => None,
        };
        let selected =
            settings.list.visible() == selected_index || settings.focused_input == selected_index;
        if selected {
            selected_row = Some(rows.len());
        }
        let style = if selected {
            modal_option_style(p, true)
        } else {
            Style::default().fg(p.text)
        };
        match row {
            SettingsListRow::Header(title) => rows.push(ListItem::new(Line::from(Span::styled(
                format!(" {title}"),
                modal_section_heading_style(p),
            )))),
            SettingsListRow::Caption(text) => rows.push(ListItem::new(Line::from(Span::styled(
                format!(" {text}"),
                Style::default().fg(p.subtext0),
            )))),
            SettingsListRow::Spacer => rows.push(ListItem::new(Line::from(""))),
            SettingsListRow::GroupIconPicker => {
                rows.extend(group_icon_picker_list_items(
                    group_settings_picker_icon(settings),
                    p,
                ));
            }

            SettingsListRow::Toggle {
                title,
                description,
                enabled,
                ..
            } => {
                let value_style = if selected {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.accent)
                };
                rows.push(ListItem::new(settings_title_value_line(
                    title,
                    if *enabled { "On" } else { "Off" },
                    list_width,
                    style,
                    value_style,
                    selected,
                )));
                rows.push(ListItem::new(settings_setting_description_line(
                    description,
                    list_width,
                    if selected {
                        style
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
                editable,
            } => {
                let value_style = if selected {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.accent)
                };
                let edited_value = (*editable && settings.focused_input == Some(*index))
                    .then(|| format!("{value}█"));
                let displayed_value = edited_value.as_deref().unwrap_or(value.as_ref());
                rows.push(ListItem::new(settings_title_value_line(
                    title,
                    displayed_value,
                    list_width,
                    style,
                    value_style,
                    selected,
                )));
                rows.push(ListItem::new(settings_setting_description_line(
                    description,
                    list_width,
                    if selected {
                        style
                    } else {
                        Style::default().fg(p.subtext0)
                    },
                    selected,
                )));
            }
            SettingsListRow::TextInput { title, value, .. } => {
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
                rows.push(ListItem::new(settings_description_line(
                    &input_value,
                    list_width,
                    Style::default().fg(p.text).bg(p.surface0),
                    false,
                )));
            }
            SettingsListRow::Choice { label, checked, .. } => {
                let check_style = if selected {
                    style
                } else if *checked {
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(p.overlay0)
                };
                rows.push(ListItem::new(settings_choice_line(
                    label,
                    *checked,
                    list_width,
                    style,
                    check_style,
                    selected,
                )))
            }
            SettingsListRow::Action {
                icon, label, tone, ..
            } => {
                let action_style = if selected && *tone == SettingsMarkerTone::Danger {
                    settings_danger_selected_style(p)
                } else if selected {
                    style
                } else if *tone == SettingsMarkerTone::Danger {
                    settings_marker_style(p, *tone)
                } else {
                    Style::default().fg(p.text)
                };
                rows.push(ListItem::new(settings_action_line(
                    icon,
                    label,
                    list_width,
                    action_style,
                    selected,
                )));
            }
            SettingsListRow::Status {
                label,
                status,
                tone,
                ..
            } => {
                let status_style = if selected {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    settings_marker_style(p, *tone)
                };
                rows.push(ListItem::new(settings_status_line(
                    label,
                    status,
                    list_width,
                    style,
                    status_style,
                    selected,
                )))
            }
            SettingsListRow::Profile {
                name,
                detail,
                badge,
                tone,
                ..
            } => {
                let detail_style = if selected {
                    style
                } else {
                    Style::default().fg(p.subtext0)
                };
                let badge_style = if selected {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    settings_marker_style(p, *tone).add_modifier(Modifier::BOLD)
                };
                rows.push(ListItem::new(settings_profile_name_line(
                    name,
                    detail,
                    badge.as_deref(),
                    list_width,
                    style,
                    detail_style,
                    badge_style,
                    selected,
                )));
            }
        }
    }
    let selected = selected_row
        .and_then(|row| (row >= scroll && row < scroll + area.height as usize).then_some(row));
    let mut state = ListState::default()
        .with_selected(selected)
        .with_offset(scroll);
    frame.render_stateful_widget(List::new(rows), scroll_area.body, &mut state);
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

fn settings_integration_hint_row(area: Rect) -> Rect {
    if area.height > 1 {
        Rect::new(area.x, area.y + 1, area.width, 1)
    } else {
        area
    }
}

fn render_settings_integration_feedback(
    frame: &mut Frame,
    area: Rect,
    p: &crate::app::state::Palette,
    messages: &[String],
) {
    let Some(first) = messages.first() else {
        return;
    };
    let (label, accent, text_style, text) =
        if let Some(warning) = first.strip_prefix(crate::integration::INSTALL_WARNING_PREFIX) {
            (
                " Warning ",
                p.yellow,
                Style::default().fg(p.text),
                warning.trim_start().to_string(),
            )
        } else if first.contains(": ") {
            (
                " Error ",
                p.red,
                Style::default().fg(p.text),
                first.to_string(),
            )
        } else {
            (
                " Hint ",
                p.green,
                Style::default().fg(p.subtext0),
                first.to_string(),
            )
        };
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            label,
            Style::default()
                .fg(panel_contrast_fg(p))
                .bg(accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(text, text_style),
    ];
    if messages.len() > 1 {
        spans.push(Span::styled(
            format!(" · {} More", messages.len() - 1),
            Style::default().fg(p.overlay1),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        settings_integration_hint_row(area),
    );
}

const SETTINGS_BODY_INDENT: usize = 2;

fn group_icon_picker_list_items(
    selected_icon: &str,
    p: &crate::app::state::Palette,
) -> Vec<ListItem<'static>> {
    let row_count = crate::ui::group_icon_picker_row_count() as usize;
    (0..row_count)
        .map(|row| {
            let mut spans = vec![Span::raw(" ".repeat(SETTINGS_BODY_INDENT))];
            for col in 0..5 {
                let idx = row * 5 + col;
                let Some(icon) = crate::app::state::GROUP_ICONS.get(idx) else {
                    break;
                };
                let selected = *icon == selected_icon;
                spans.push(Span::styled(
                    format!(" {icon} "),
                    if selected {
                        Style::default().fg(panel_contrast_fg(p)).bg(p.accent)
                    } else {
                        Style::default().fg(p.text).bg(p.surface0)
                    },
                ));
                spans.push(Span::raw(" "));
            }
            ListItem::new(Line::from(spans))
        })
        .collect()
}

fn group_settings_picker_icon(settings: &SettingsState) -> &str {
    settings
        .pending_group_icon
        .as_deref()
        .unwrap_or(crate::app::state::DEFAULT_GROUP_ICON)
}

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
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn group_settings_render_from_client_view() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("Work".to_string());
        let mut view = crate::app::ClientViewState::from_default_client_state(&app);
        view.settings.group_settings_target = Some(group_idx);
        view.settings.section = SettingsSection::Theme;

        let rendered = render_settings(&app, &view, Rect::new(0, 0, 80, 24));

        assert!(rendered.contains("Group Settings"), "{rendered:?}");
        assert!(rendered.contains("Appearance"), "{rendered:?}");
        assert!(
            rendered.contains("Choose a theme accent for this group"),
            "{rendered:?}"
        );
    }

    #[test]
    fn command_draft_renders_from_client_view() {
        let app = AppState::test_new();
        let mut view = crate::app::ClientViewState::from_default_client_state(&app);
        view.settings.section = SettingsSection::Commands;
        view.settings.pending_review_command = Some("hunk diff --watch".to_string());
        view.settings.list.select(1);
        view.settings.list.show();
        view.settings.focused_input = Some(1);

        let rendered = render_settings(&app, &view, Rect::new(0, 0, 100, 40));

        assert!(rendered.contains("hunk diff --watch"), "{rendered:?}");
    }

    #[test]
    fn expanded_settings_sidebar_exposes_subsections() {
        let settings = SettingsState {
            sidebar_expanded: Some(SettingsSection::PaneLabels),
            ..Default::default()
        };
        let labels = settings_sidebar_entries(&settings)
            .into_iter()
            .filter(|entry| entry.section == SettingsSection::PaneLabels)
            .map(|entry| entry.label)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec!["Behavior", "General", "Selection", "Terminal", "Sessions"]
        );
    }

    fn render_settings(app: &AppState, view: &crate::app::ClientViewState, area: Rect) -> String {
        let mut terminal =
            Terminal::new(TestBackend::new(area.width, area.height)).expect("test backend");
        terminal
            .draw(|frame| render_settings_overlay_for_view(app, view, frame, area))
            .expect("render settings");
        buffer_text(terminal.backend().buffer(), area)
    }

    fn buffer_text(buffer: &Buffer, area: Rect) -> String {
        let mut text = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }
}
