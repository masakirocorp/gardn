use std::borrow::Cow;

use crate::{
    app::{
        state::{
            normalize_theme_name, theme_names_for_appearance, AppState, SettingsSection,
            SettingsState,
        },
        ClientViewState,
    },
    config::{NewTerminalCwdConfig, TerminalAccent, ThemeMode, ToastDelivery},
    terminal_theme::ThemeAppearance,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsListRow {
    Header(&'static str),
    Caption(Cow<'static, str>),
    Spacer,
    Toggle {
        index: usize,
        title: Cow<'static, str>,
        description: Cow<'static, str>,
        enabled: bool,
    },
    Value {
        index: usize,
        title: Cow<'static, str>,
        description: Cow<'static, str>,
        value: Cow<'static, str>,
    },
    TextInput {
        index: usize,
        title: Cow<'static, str>,
        value: Cow<'static, str>,
    },
    Choice {
        index: usize,
        label: Cow<'static, str>,
        checked: bool,
    },
    Action {
        index: usize,
        icon: Cow<'static, str>,
        label: Cow<'static, str>,
        tone: SettingsMarkerTone,
    },
    Status {
        index: usize,
        label: Cow<'static, str>,
        status: Cow<'static, str>,
        tone: SettingsMarkerTone,
    },
    Profile {
        index: usize,
        name: Cow<'static, str>,
        detail: Cow<'static, str>,
        badge: Option<Cow<'static, str>>,
        tone: SettingsMarkerTone,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsMarkerTone {
    Good,
    Warning,
    Accent,
    Danger,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettingsRowHit {
    pub index: usize,
    pub hoverable: bool,
}

pub(crate) fn option_hit_for_visual_row(
    rows: &[SettingsListRow],
    row: usize,
) -> Option<SettingsRowHit> {
    let mut visual_row = 0;
    for entry in rows {
        match entry {
            SettingsListRow::Header(_) | SettingsListRow::Caption(_) | SettingsListRow::Spacer => {
                if row == visual_row {
                    return None;
                }
                visual_row += 1;
            }
            SettingsListRow::Toggle { index, .. } | SettingsListRow::Value { index, .. } => {
                if row == visual_row || row == visual_row + 1 {
                    return Some(SettingsRowHit {
                        index: *index,
                        hoverable: true,
                    });
                }
                visual_row += 2;
            }
            SettingsListRow::TextInput { index, .. } => {
                if row == visual_row + 1 {
                    return Some(SettingsRowHit {
                        index: *index,
                        hoverable: false,
                    });
                }
                visual_row += 2;
            }
            SettingsListRow::Choice { index, .. }
            | SettingsListRow::Action { index, .. }
            | SettingsListRow::Status { index, .. } => {
                if row == visual_row {
                    return Some(SettingsRowHit {
                        index: *index,
                        hoverable: true,
                    });
                }
                visual_row += 1;
            }
            SettingsListRow::Profile { index, .. } => {
                if row == visual_row {
                    return Some(SettingsRowHit {
                        index: *index,
                        hoverable: true,
                    });
                }
                visual_row += 1;
            }
        }
    }
    None
}

pub(crate) fn option_index_for_visual_row(rows: &[SettingsListRow], row: usize) -> Option<usize> {
    option_hit_for_visual_row(rows, row).map(|hit| hit.index)
}

pub(crate) fn rows_for_section(
    app: &AppState,
    section: SettingsSection,
) -> Option<Vec<SettingsListRow>> {
    rows_for_section_with_settings(app, &app.settings, section)
}

fn rows_for_section_with_settings(
    app: &AppState,
    settings: &SettingsState,
    section: SettingsSection,
) -> Option<Vec<SettingsListRow>> {
    match section {
        SettingsSection::Theme => Some(appearance_rows(app, settings)),
        SettingsSection::Layout => Some(layout_rows(app, settings)),
        SettingsSection::Sound => Some(notification_rows(app, settings)),
        SettingsSection::Toast => Some(toast_rows(app, settings)),
        SettingsSection::PaneLabels => Some(behavior_rows(app, settings)),
        SettingsSection::Experiments => Some(experiment_rows(app, settings)),
        SettingsSection::Agents => Some(agent_profile_rows(app, settings)),
        SettingsSection::Integrations => Some(integration_rows(app)),
        SettingsSection::GroupGeneral => Some(group_general_rows(app, settings)),
        SettingsSection::GroupProfiles => Some(group_profile_rows(app, settings)),
        SettingsSection::WorkspaceGeneral => Some(workspace_general_rows(app, settings)),
    }
}

/// Builds settings rows for the requesting client's selected section.
///
/// Shared domain values remain derived from `AppState`; client-local drafts,
/// selection, and scrolling are consumed from the client's settings state.
pub(crate) fn rows_for_section_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> Option<Vec<SettingsListRow>> {
    rows_for_section_with_settings(app, &view.settings, view.settings.section)
}

pub(crate) fn selected_visual_row(rows: &[SettingsListRow], selected: usize) -> Option<usize> {
    let mut visual_row = 0;
    for entry in rows {
        match entry {
            SettingsListRow::Header(_) | SettingsListRow::Caption(_) | SettingsListRow::Spacer => {
                visual_row += 1;
            }
            SettingsListRow::Toggle { index, .. } | SettingsListRow::Value { index, .. } => {
                if *index == selected {
                    return Some(visual_row);
                }
                visual_row += 2;
            }
            SettingsListRow::TextInput { index, .. } => {
                if *index == selected {
                    return Some(visual_row + 1);
                }
                visual_row += 2;
            }
            SettingsListRow::Choice { index, .. }
            | SettingsListRow::Action { index, .. }
            | SettingsListRow::Status { index, .. } => {
                if *index == selected {
                    return Some(visual_row);
                }
                visual_row += 1;
            }
            SettingsListRow::Profile { index, .. } => {
                if *index == selected {
                    return Some(visual_row);
                }
                visual_row += 1;
            }
        }
    }
    None
}

pub(crate) fn visual_row_count(rows: &[SettingsListRow]) -> usize {
    rows.iter()
        .map(|row| match row {
            SettingsListRow::Header(_)
            | SettingsListRow::Caption(_)
            | SettingsListRow::Spacer
            | SettingsListRow::Choice { .. }
            | SettingsListRow::Action { .. }
            | SettingsListRow::Status { .. } => 1,
            SettingsListRow::Toggle { .. }
            | SettingsListRow::Value { .. }
            | SettingsListRow::TextInput { .. } => 2,
            SettingsListRow::Profile { .. } => 1,
        })
        .sum()
}

pub(crate) fn option_count(rows: &[SettingsListRow]) -> usize {
    rows.iter()
        .filter(|row| {
            matches!(
                row,
                SettingsListRow::Toggle { .. }
                    | SettingsListRow::Value { .. }
                    | SettingsListRow::TextInput { .. }
                    | SettingsListRow::Choice { .. }
                    | SettingsListRow::Action { .. }
                    | SettingsListRow::Status { .. }
                    | SettingsListRow::Profile { .. }
            )
        })
        .count()
}

fn theme_settings_choices_group_accent(
    app: &AppState,
    settings: &SettingsState,
) -> Option<TerminalAccent> {
    if let Some(pending) = settings.pending_group_accent_choice {
        return pending;
    }

    settings
        .group_settings_target
        .and_then(|group_idx| app.groups.get(group_idx))
        .and_then(|group| group.accent)
}

fn theme_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    if settings.group_settings_target.is_some() {
        let active = theme_settings_choices_group_accent(app, settings);
        let mut rows = Vec::new();
        rows.push(SettingsListRow::Header("accent"));
        rows.push(choice(0, "inherit", active.is_none()));
        for (offset, accent) in TerminalAccent::ALL.iter().copied().enumerate() {
            rows.push(choice(offset + 1, accent.as_str(), active == Some(accent)));
        }
        return rows;
    }

    let mode = settings.pending_theme_mode.unwrap_or(app.global_theme_mode);
    let pending_light_theme = settings
        .pending_light_theme_name
        .as_deref()
        .unwrap_or(&app.global_light_theme_name);
    let pending_dark_theme = settings
        .pending_dark_theme_name
        .as_deref()
        .unwrap_or(&app.global_dark_theme_name);
    let system_source = mode == ThemeMode::System
        && normalize_theme_name(pending_light_theme) == "system"
        && normalize_theme_name(pending_dark_theme) == "system";
    let show_terminal_accent = system_source;

    let mut rows = Vec::new();
    rows.push(SettingsListRow::Header("colors"));
    rows.push(choice(0, "terminal", system_source));
    rows.push(choice(1, "palettes", !system_source));

    if show_terminal_accent {
        rows.push(SettingsListRow::Spacer);
        rows.push(SettingsListRow::Header("light accent"));
        let pending_light_accent = settings
            .pending_terminal_light_accent
            .unwrap_or(app.global_terminal_light_accent);
        for (offset, accent) in TerminalAccent::ALL.iter().copied().enumerate() {
            rows.push(choice(
                2 + offset,
                accent.as_str(),
                pending_light_accent == accent,
            ));
        }

        rows.push(SettingsListRow::Spacer);
        rows.push(SettingsListRow::Header("dark accent"));
        let pending_dark_accent = settings
            .pending_terminal_dark_accent
            .unwrap_or(app.global_terminal_dark_accent);
        let dark_base = 2 + TerminalAccent::ALL.len();
        for (offset, accent) in TerminalAccent::ALL.iter().copied().enumerate() {
            rows.push(choice(
                dark_base + offset,
                accent.as_str(),
                pending_dark_accent == accent,
            ));
        }
    }

    if !system_source {
        rows.push(SettingsListRow::Spacer);
        rows.push(SettingsListRow::Header("appearance"));
        for (offset, candidate) in ThemeMode::ALL.iter().copied().enumerate() {
            rows.push(choice(
                2 + offset,
                theme_mode_display_name(candidate),
                mode == candidate,
            ));
        }

        let theme_base = 2 + ThemeMode::ALL.len();
        match mode {
            ThemeMode::System => {
                rows.push(SettingsListRow::Spacer);
                rows.push(SettingsListRow::Header("light appearance"));
                let mut option_idx = theme_base;
                for name in theme_names_for_appearance(ThemeAppearance::Light)
                    .iter()
                    .copied()
                {
                    rows.push(choice(
                        option_idx,
                        theme_display_name(name),
                        pending_light_theme == name,
                    ));
                    option_idx += 1;
                }

                rows.push(SettingsListRow::Spacer);
                rows.push(SettingsListRow::Header("dark appearance"));
                for name in theme_names_for_appearance(ThemeAppearance::Dark)
                    .iter()
                    .copied()
                {
                    rows.push(choice(
                        option_idx,
                        theme_display_name(name),
                        pending_dark_theme == name,
                    ));
                    option_idx += 1;
                }
            }
            ThemeMode::Light => {
                rows.push(SettingsListRow::Spacer);
                rows.push(SettingsListRow::Header("light appearance"));
                for (offset, name) in theme_names_for_appearance(ThemeAppearance::Light)
                    .iter()
                    .copied()
                    .enumerate()
                {
                    rows.push(choice(
                        theme_base + offset,
                        theme_display_name(name),
                        pending_light_theme == name,
                    ));
                }
            }
            ThemeMode::Dark => {
                rows.push(SettingsListRow::Spacer);
                rows.push(SettingsListRow::Header("dark appearance"));
                for (offset, name) in theme_names_for_appearance(ThemeAppearance::Dark)
                    .iter()
                    .copied()
                    .enumerate()
                {
                    rows.push(choice(
                        theme_base + offset,
                        theme_display_name(name),
                        pending_dark_theme == name,
                    ));
                }
            }
        }
    }

    rows
}

fn group_general_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    let group_name = settings
        .pending_group_name
        .clone()
        .or_else(|| {
            settings
                .group_settings_target
                .and_then(|group_idx| app.groups.get(group_idx))
                .map(|group| group.name.clone())
        })
        .unwrap_or_else(|| "group".to_string());
    let default_directory = settings
        .pending_group_default_directory
        .clone()
        .or_else(|| {
            settings
                .group_settings_target
                .and_then(|group_idx| app.groups.get(group_idx))
                .and_then(|group| {
                    group
                        .default_directory
                        .as_ref()
                        .map(|path| path.display().to_string())
                })
        })
        .unwrap_or_default();

    vec![
        SettingsListRow::TextInput {
            index: 0,
            title: "name".into(),
            value: group_name.into(),
        },
        SettingsListRow::Spacer,
        SettingsListRow::TextInput {
            index: 1,
            title: "default directory for new spaces".into(),
            value: default_directory.into(),
        },
        SettingsListRow::Spacer,
        SettingsListRow::Header("danger zone"),
        SettingsListRow::Action {
            index: 2,
            icon: "×".into(),
            label: "delete group".into(),
            tone: SettingsMarkerTone::Danger,
        },
    ]
}

fn workspace_general_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    let workspace = settings
        .workspace_settings_target
        .and_then(|ws_idx| app.workspaces.get(ws_idx));
    let name = settings
        .pending_workspace_name
        .clone()
        .or_else(|| workspace.map(|workspace| workspace.display_name()))
        .unwrap_or_else(|| "space".to_string());
    let default_cwd = settings
        .pending_workspace_default_cwd
        .clone()
        .or_else(|| workspace.map(|workspace| workspace.default_cwd.display().to_string()))
        .unwrap_or_default();

    vec![
        SettingsListRow::TextInput {
            index: 0,
            title: "name".into(),
            value: name.into(),
        },
        SettingsListRow::Spacer,
        SettingsListRow::TextInput {
            index: 1,
            title: "default directory".into(),
            value: default_cwd.into(),
        },
    ]
}

fn agent_profile_editor_open(settings: &SettingsState) -> bool {
    settings.pending_agent_profile_id.is_some()
        || settings.pending_agent_profile_name.is_some()
        || settings.pending_agent_profile_command.is_some()
}

fn agent_profile_detail(profile: &crate::agent_profiles::AgentProfile) -> String {
    if profile.is_system() {
        return String::new();
    }

    if profile.kind.is_supported() {
        if profile.command == profile.name {
            profile.kind.as_str().to_string()
        } else {
            format!("{} · {}", profile.kind.as_str(), profile.command)
        }
    } else {
        "custom · launch-only".to_string()
    }
}

fn agent_profile_badge(
    profile: &crate::agent_profiles::AgentProfile,
    is_favorite: bool,
    is_default: bool,
    integration_badge: Option<&str>,
) -> Option<Cow<'static, str>> {
    if let Some(badge) = integration_badge {
        Some(badge.to_string().into())
    } else if !profile.available() {
        Some("unavailable".into())
    } else if is_default {
        Some("default".into())
    } else if is_favorite {
        Some("favorite".into())
    } else if !profile.kind.is_supported() {
        Some("launch-only".into())
    } else {
        None
    }
}

fn agent_profile_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    if !agent_profile_editor_open(settings) {
        return agent_profile_browse_rows(app);
    }

    let mut rows = Vec::new();
    let name = settings
        .pending_agent_profile_name
        .clone()
        .unwrap_or_default();
    let command = settings
        .pending_agent_profile_command
        .clone()
        .unwrap_or_default();
    let mut kind = settings
        .pending_agent_profile_kind
        .unwrap_or_else(|| app.default_agent_profile_kind_choice());
    if !app.agent_profile_kind_available(kind) {
        kind = crate::agent_profiles::AgentKind::Custom;
    }
    let editing = settings.pending_agent_profile_id.is_some();

    rows.push(SettingsListRow::Header("1. name"));
    rows.push(SettingsListRow::Caption("label shown in menus".into()));
    rows.push(SettingsListRow::TextInput {
        index: 0,
        title: "profile name".into(),
        value: name.into(),
    });
    rows.push(SettingsListRow::Spacer);
    rows.push(SettingsListRow::Header("2. kind"));
    rows.push(SettingsListRow::Caption(
        "choose an installed integration family, or custom for launch-only commands".into(),
    ));
    let kind_choices = app.agent_profile_kind_choices().collect::<Vec<_>>();
    for (offset, agent_kind) in kind_choices.iter().copied().enumerate() {
        rows.push(SettingsListRow::Choice {
            index: 1 + offset,
            label: agent_kind.as_str().into(),
            checked: agent_kind == kind,
        });
    }
    if !kind.is_supported() {
        rows.push(SettingsListRow::Spacer);
        rows.push(SettingsListRow::Header("custom agents are launch-only"));
        rows.push(SettingsListRow::Caption(
            "status, restore, and integration install are unavailable".into(),
        ));
    }
    rows.push(SettingsListRow::Spacer);
    rows.push(SettingsListRow::Header("3. command"));
    rows.push(SettingsListRow::Caption("shell command to run".into()));
    let command_index = 1 + kind_choices.len();
    rows.push(SettingsListRow::TextInput {
        index: command_index,
        title: "command".into(),
        value: command.into(),
    });
    rows.push(SettingsListRow::Spacer);
    rows.push(SettingsListRow::Header("4. actions"));
    rows.push(SettingsListRow::Caption(
        "save the profile, discard changes, or delete a custom profile".into(),
    ));
    let save_index = command_index + 1;
    rows.push(SettingsListRow::Action {
        index: save_index,
        icon: "".into(),
        label: if editing {
            "save profile".into()
        } else {
            "create profile".into()
        },
        tone: SettingsMarkerTone::Accent,
    });
    rows.push(SettingsListRow::Action {
        index: save_index + 1,
        icon: "×".into(),
        label: "discard changes".into(),
        tone: SettingsMarkerTone::Disabled,
    });
    if editing {
        rows.push(SettingsListRow::Action {
            index: save_index + 2,
            icon: "×".into(),
            label: "delete custom profile".into(),
            tone: SettingsMarkerTone::Danger,
        });
    }
    rows
}

fn agent_profile_browse_rows(app: &AppState) -> Vec<SettingsListRow> {
    let mut rows = vec![
        SettingsListRow::Action {
            index: 0,
            icon: "".into(),
            label: "new custom profile".into(),
            tone: SettingsMarkerTone::Accent,
        },
        SettingsListRow::Spacer,
        SettingsListRow::Header("custom profiles"),
    ];
    let custom_profiles = app
        .agent_profiles
        .profiles()
        .iter()
        .filter(|profile| !profile.is_system());
    let mut has_custom_profiles = false;
    for (index, profile) in (1..).zip(custom_profiles) {
        has_custom_profiles = true;
        let tone = if profile.available() {
            SettingsMarkerTone::Good
        } else {
            SettingsMarkerTone::Disabled
        };
        rows.push(agent_profile_row(profile, index, false, false, tone));
    }
    if !has_custom_profiles {
        rows.push(SettingsListRow::Caption(
            "none yet — create one to add custom launch commands".into(),
        ));
    }
    rows
}

fn agent_profile_row(
    profile: &crate::agent_profiles::AgentProfile,
    index: usize,
    is_favorite: bool,
    is_default: bool,
    tone: SettingsMarkerTone,
) -> SettingsListRow {
    let integration_badge = crate::integration::agent_profile_integration_badge(profile);
    let tone = if integration_badge.is_some() {
        SettingsMarkerTone::Warning
    } else {
        tone
    };
    SettingsListRow::Profile {
        index,
        name: profile.name.clone().into(),
        detail: agent_profile_detail(profile).into(),
        badge: agent_profile_badge(profile, is_favorite, is_default, integration_badge),
        tone,
    }
}

fn profile_visible_in_group_settings(
    app: &AppState,
    profile: &crate::agent_profiles::AgentProfile,
) -> bool {
    profile.available()
        && (app.agent_profile_launchable(profile)
            || crate::integration::agent_profile_integration_warning(profile).is_some())
}

fn group_profile_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    let group = settings
        .group_settings_target
        .and_then(|idx| app.groups.get(idx));
    let favorites = group
        .map(|group| group.favorite_agent_profile_ids.as_slice())
        .unwrap_or(&[]);
    let default_profile_id = group.and_then(|group| group.default_agent_profile_id.as_deref());
    let (favorite, available) = app.agent_profiles.group_sections(favorites);
    let favorite: Vec<_> = favorite
        .into_iter()
        .filter(|profile| profile_visible_in_group_settings(app, profile))
        .collect();
    let available: Vec<_> = available
        .into_iter()
        .filter(|profile| profile_visible_in_group_settings(app, profile))
        .collect();
    let mut rows = Vec::new();
    let mut index = 0;
    rows.push(SettingsListRow::Header("favorites"));
    if favorite.is_empty() {
        rows.push(SettingsListRow::Caption("no favorites".into()));
    } else {
        for profile in favorite {
            let is_default = default_profile_id == Some(profile.id.as_str());
            rows.push(agent_profile_row(
                profile,
                index,
                false,
                is_default,
                SettingsMarkerTone::Accent,
            ));
            index += 1;
        }
    }
    rows.push(SettingsListRow::Spacer);
    rows.push(SettingsListRow::Header("available"));
    for profile in available {
        let is_default = default_profile_id == Some(profile.id.as_str());
        rows.push(agent_profile_row(
            profile,
            index,
            false,
            is_default,
            if is_default {
                SettingsMarkerTone::Accent
            } else {
                SettingsMarkerTone::Disabled
            },
        ));
        index += 1;
    }
    rows
}

fn appearance_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    if settings.group_settings_target.is_some() {
        return theme_rows(app, settings);
    }

    let mut rows = theme_rows(app, settings);
    let layout_base = option_count(&rows);
    rows.push(SettingsListRow::Spacer);
    rows.extend(layout_rows_with_base(app, settings, layout_base));
    rows.push(SettingsListRow::Spacer);
    rows.extend(setting_group(
        "panes",
        [option(
            layout_base + 4,
            "agent border labels",
            "show detected agent names in split pane borders",
            settings
                .pending_agent_border_labels
                .unwrap_or_else(|| app.agent_border_labels_enabled()),
        )],
    ));
    rows
}

fn layout_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    layout_rows_with_base(app, settings, 0)
}

fn layout_rows_with_base(
    app: &AppState,
    settings: &SettingsState,
    base: usize,
) -> Vec<SettingsListRow> {
    let width = settings
        .pending_sidebar_width
        .unwrap_or(app.default_sidebar_width);
    let min = settings
        .pending_sidebar_min_width
        .unwrap_or(app.sidebar_min_width);
    let max = settings
        .pending_sidebar_max_width
        .unwrap_or(app.sidebar_max_width);
    let arrangement = settings
        .pending_sidebar_arrangement
        .unwrap_or(app.sidebar_arrangement);
    setting_group(
        "sidebar",
        [
            value_option(
                base,
                "default sidebar width",
                "preferred desktop sidebar width",
                format!("{width} cols"),
            ),
            value_option(
                base + 1,
                "minimum sidebar width",
                "smallest allowed desktop sidebar width",
                format!("{min} cols"),
            ),
            value_option(
                base + 2,
                "maximum sidebar width",
                "largest allowed desktop sidebar width",
                format!("{max} cols"),
            ),
            value_option(
                base + 3,
                "sidebar arrangement",
                "where spaces and agents live on desktop",
                arrangement.label(),
            ),
        ],
    )
}

fn behavior_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    let cwd_label = new_terminal_cwd_label(
        &settings
            .pending_new_terminal_cwd
            .clone()
            .unwrap_or_else(|| app.new_terminal_cwd.clone()),
    );
    let scroll_label = format!(
        "{} lines per wheel notch",
        settings
            .pending_mouse_scroll_lines
            .unwrap_or(app.mouse_scroll_lines)
    );
    let worktree_directory = settings
        .pending_worktree_directory
        .clone()
        .unwrap_or_else(|| app.worktree_directory.display().to_string());

    let mut rows = setting_group(
        "workspace",
        [
            option(
                0,
                "confirm before closing workspaces",
                "ask before closing a workspace",
                settings
                    .pending_confirm_close
                    .unwrap_or_else(|| app.confirm_close_enabled()),
            ),
            option(
                1,
                "name new tabs",
                "ask for a tab name before creating a new tab",
                settings
                    .pending_prompt_new_tab_name
                    .unwrap_or_else(|| app.prompt_new_tab_name_enabled()),
            ),
            value_option(
                2,
                "worktree directory",
                "where task worktrees are created",
                worktree_directory,
            ),
        ],
    );
    rows.push(SettingsListRow::Spacer);
    rows.extend(setting_group(
        "terminal",
        [
            value_option(
                3,
                "new terminal cwd",
                "directory used by newly created terminal tabs",
                cwd_label,
            ),
            value_option(
                4,
                "mouse wheel speed",
                "terminal scroll amount per wheel notch",
                scroll_label,
            ),
        ],
    ));
    rows
}

fn experiment_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    setting_group(
        "input",
        [option(
            0,
            "switch to ascii input source in prefix (macOS)",
            "temporarily use an ASCII-capable layout for prefix commands",
            settings
                .pending_switch_ascii_input_source_in_prefix
                .unwrap_or_else(|| app.switch_ascii_input_source_in_prefix_enabled()),
        )],
    )
}

fn notification_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    let sound_enabled = settings
        .pending_sound_enabled
        .unwrap_or_else(|| app.sound_enabled());
    let toast_delivery = settings
        .pending_toast_delivery
        .unwrap_or_else(|| app.toast_delivery());
    let mut rows = setting_group(
        "sound alerts",
        [option(
            0,
            "sound alerts",
            "play sound when a background agent needs attention",
            sound_enabled,
        )],
    );
    rows.push(SettingsListRow::Spacer);
    rows.extend(setting_group(
        "notification popups",
        [value_option(
            1,
            "toast delivery",
            "where command and agent notifications should appear",
            toast_delivery_label(toast_delivery),
        )],
    ));
    rows
}

fn integration_rows(app: &AppState) -> Vec<SettingsListRow> {
    app.integration_recommendations
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let missing_profile_hooks = crate::integration::missing_profile_hook_count_for_target(
                item.target,
                &app.agent_profiles,
            );
            let profile_hooks_missing = item.state
                == crate::integration::IntegrationStatusKind::Current
                && missing_profile_hooks > 0;
            let tone = if profile_hooks_missing {
                SettingsMarkerTone::Warning
            } else {
                match item.state {
                    crate::integration::IntegrationStatusKind::Current => SettingsMarkerTone::Good,
                    crate::integration::IntegrationStatusKind::Outdated => {
                        SettingsMarkerTone::Warning
                    }
                    crate::integration::IntegrationStatusKind::NotInstalled if item.available => {
                        SettingsMarkerTone::Accent
                    }
                    crate::integration::IntegrationStatusKind::NotInstalled => {
                        SettingsMarkerTone::Disabled
                    }
                }
            };
            let status = if profile_hooks_missing {
                if missing_profile_hooks == 1 {
                    "installed · 1 profile hook missing".to_string()
                } else {
                    format!("installed · {missing_profile_hooks} profile hooks missing")
                }
            } else {
                item.status_label().to_string()
            };
            SettingsListRow::Status {
                index,
                label: item.label.into(),
                status: status.into(),
                tone,
            }
        })
        .collect()
}

fn toast_delivery_label(delivery: ToastDelivery) -> &'static str {
    match delivery {
        ToastDelivery::Off => "off",
        ToastDelivery::Hako => "inside hako",
        ToastDelivery::Terminal => "via terminal",
        ToastDelivery::System => "via system",
    }
}

fn toast_rows(app: &AppState, settings: &SettingsState) -> Vec<SettingsListRow> {
    let current = settings
        .pending_toast_delivery
        .unwrap_or_else(|| app.toast_delivery());
    setting_group(
        "notification popups",
        [value_option(
            0,
            "toast delivery",
            "where notification popups should appear",
            toast_delivery_label(current),
        )],
    )
}

fn setting_group(
    header: &'static str,
    settings: impl IntoIterator<Item = SettingsListRow>,
) -> Vec<SettingsListRow> {
    let mut rows = vec![SettingsListRow::Header(header)];
    push_spaced_settings(&mut rows, settings);
    rows
}

fn push_spaced_settings(
    rows: &mut Vec<SettingsListRow>,
    settings: impl IntoIterator<Item = SettingsListRow>,
) {
    let mut first = true;
    for setting in settings {
        if !first {
            rows.push(SettingsListRow::Spacer);
        }
        rows.push(setting);
        first = false;
    }
}

fn option(
    index: usize,
    title: impl Into<Cow<'static, str>>,
    description: impl Into<Cow<'static, str>>,
    enabled: bool,
) -> SettingsListRow {
    SettingsListRow::Toggle {
        index,
        title: title.into(),
        description: description.into(),
        enabled,
    }
}

fn value_option(
    index: usize,
    title: impl Into<Cow<'static, str>>,
    description: impl Into<Cow<'static, str>>,
    value: impl Into<Cow<'static, str>>,
) -> SettingsListRow {
    SettingsListRow::Value {
        index,
        title: title.into(),
        description: description.into(),
        value: value.into(),
    }
}

fn choice(index: usize, label: impl Into<Cow<'static, str>>, checked: bool) -> SettingsListRow {
    SettingsListRow::Choice {
        index,
        label: label.into(),
        checked,
    }
}

fn theme_display_name(name: &'static str) -> &'static str {
    match name {
        "catppuccin-latte" => "catppuccin latte",
        "catppuccin" => "catppuccin mocha",
        "catppuccin-frappe" => "catppuccin frappe",
        "catppuccin-macchiato" => "catppuccin macchiato",
        "tokyo-night-day" => "tokyo night day",
        "gruvbox-light" => "gruvbox",
        "one-light" => "one",
        "solarized-light" => "solarized",
        "kanagawa-lotus" => "kanagawa lotus",
        "rose-pine-dawn" => "rose pine dawn",
        "tokyo-night" => "tokyo night",
        "one-dark" => "one dark",
        "rose-pine" => "rose pine",
        "monokai-pro" => "monokai pro",
        "monokai-pro-light" => "monokai pro light",
        "monokai-pro-light-sun" => "monokai pro sun",
        "monokai-pro-spectrum" => "monokai pro spectrum",
        "monokai-pro-ristretto" => "monokai pro ristretto",
        "monokai-pro-octagon" => "monokai pro octagon",
        "monokai-pro-machine" => "monokai pro machine",
        "monokai-classic" => "monokai classic",
        "ethereal" => "ethereal",
        "everforest" => "everforest",
        "flexoki-light" => "flexoki light",
        "hackerman" => "hackerman",
        "last-horizon" => "last horizon",
        "lumon" => "lumon",
        "matte-black" => "matte black",
        "miasma" => "miasma",
        "osaka-jade" => "osaka jade",
        "retro-82" => "retro 82",
        "solitude" => "solitude",
        "vantablack" => "vantablack",
        "white" => "white",
        "flexoki-dark" => "flexoki dark",
        "omarchy" => "omarchy",
        other => other,
    }
}

fn theme_mode_display_name(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "automatic",
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    }
}

fn new_terminal_cwd_label(policy: &NewTerminalCwdConfig) -> String {
    match policy {
        NewTerminalCwdConfig::Follow => "follow focused pane".to_string(),
        NewTerminalCwdConfig::Home => "home directory".to_string(),
        NewTerminalCwdConfig::Current => "hako process directory".to_string(),
        NewTerminalCwdConfig::Path(path) => format!("custom path: {path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_headers_and_spacers_are_not_selectable() {
        let rows = [
            SettingsListRow::Header("section"),
            SettingsListRow::Spacer,
            SettingsListRow::Caption("caption".into()),
            SettingsListRow::Toggle {
                index: 7,
                title: "option".into(),
                description: "description".into(),
                enabled: true,
            },
            SettingsListRow::Choice {
                index: 11,
                label: "choice".into(),
                checked: false,
            },
        ];

        assert_eq!(option_index_for_visual_row(&rows, 0), None);
        assert_eq!(option_index_for_visual_row(&rows, 1), None);
        assert_eq!(option_index_for_visual_row(&rows, 2), None);
        assert_eq!(option_index_for_visual_row(&rows, 3), Some(7));
        assert_eq!(option_index_for_visual_row(&rows, 4), Some(7));
        assert_eq!(option_index_for_visual_row(&rows, 5), Some(11));
        assert_eq!(option_index_for_visual_row(&rows, 6), None);
    }

    #[test]
    fn system_profile_rows_do_not_repeat_builtin_details() {
        let profile = crate::agent_profiles::AgentProfile {
            id: "system:cursor".to_string(),
            name: "cursor".to_string(),
            kind: crate::agent_profiles::AgentKind::Cursor,
            command: "cursor-agent".to_string(),
            argv: vec!["cursor-agent".to_string()],
            env: Vec::new(),
            enabled: true,
            source: crate::agent_profiles::AgentProfileSource::System,
            parse_error: None,
        };

        assert_eq!(agent_profile_detail(&profile), "");
    }

    #[test]
    fn profile_rows_take_one_visual_row() {
        let rows = [
            SettingsListRow::Header("profiles"),
            SettingsListRow::Profile {
                index: 3,
                name: "cursor".into(),
                detail: "".into(),
                badge: None,
                tone: SettingsMarkerTone::Good,
            },
            SettingsListRow::Profile {
                index: 4,
                name: "omp-mk".into(),
                detail: "omp".into(),
                badge: None,
                tone: SettingsMarkerTone::Good,
            },
        ];

        assert_eq!(visual_row_count(&rows), 3);
        assert_eq!(option_index_for_visual_row(&rows, 1), Some(3));
        assert_eq!(option_index_for_visual_row(&rows, 2), Some(4));
        assert_eq!(option_index_for_visual_row(&rows, 3), None);
    }

    #[test]
    fn custom_codex_profile_rows_mark_missing_profile_hook_as_warning() {
        let _lock = crate::integration::integration_env_lock();
        let base = std::env::temp_dir().join(format!(
            "hako-settings-codex-profile-warning-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = base.join("home");
        std::fs::create_dir_all(home.join(".codex-mk")).unwrap();
        let _codex_home_env = crate::config::TestEnvVar::remove("CODEX_HOME");
        let _home_env = crate::config::TestEnvVar::set("HOME", &home);
        let mut app = AppState::test_new();
        app.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-mk".to_string()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-mk".to_string(),
                    name: "codex mk".to_string(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-mk".to_string(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Codex,
            label: "codex",
            command: "codex",
            available: true,
            path: std::path::PathBuf::from("/tmp/hako-test-codex"),
            state: crate::integration::IntegrationStatusKind::Current,
        }];

        let rows = rows_for_section(&app, SettingsSection::Agents).expect("agent rows");
        let row = rows
            .iter()
            .find(|row| {
                matches!(
                    row,
                    SettingsListRow::Profile { name, .. } if name.as_ref() == "codex mk"
                )
            })
            .expect("custom codex profile row remains visible");

        match row {
            SettingsListRow::Profile { badge, tone, .. } => {
                assert_eq!(*tone, SettingsMarkerTone::Warning);
                assert_eq!(
                    badge
                        .as_ref()
                        .expect("profile row should expose missing hook badge")
                        .as_ref(),
                    "hook missing"
                );
            }
            _ => unreachable!("matched profile row"),
        }

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn integrations_row_warns_when_custom_codex_profile_home_missing_hook() {
        let _lock = crate::integration::integration_env_lock();
        let base = std::env::temp_dir().join(format!(
            "hako-settings-integrations-codex-profile-hook-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let home = base.join("home");
        let default_codex_dir = home.join(".codex");
        let custom_codex_dir = home.join(".codex-frs");
        std::fs::create_dir_all(&default_codex_dir).unwrap();
        std::fs::create_dir_all(&custom_codex_dir).unwrap();
        std::fs::write(
            default_codex_dir.join("config.toml"),
            "model = \"gpt-5.4\"\n",
        )
        .unwrap();
        let _codex_home_env = crate::config::TestEnvVar::remove("CODEX_HOME");
        let _home_env = crate::config::TestEnvVar::set("HOME", &home);

        crate::integration::install_target(crate::api::schema::IntegrationTarget::Codex)
            .expect("install default codex integration");
        assert!(default_codex_dir.join("hako-agent-state.sh").is_file());
        assert!(!custom_codex_dir.join("hako-agent-state.sh").exists());

        let mut app = AppState::test_new();
        app.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-frs".to_string()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-frs".to_string(),
                    name: "codex frs".to_string(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-frs".to_string(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        app.integration_recommendations = vec![crate::integration::IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Codex,
            label: "codex",
            command: "codex",
            available: true,
            path: default_codex_dir.join("hako-agent-state.sh"),
            state: crate::integration::IntegrationStatusKind::Current,
        }];

        let rows = rows_for_section(&app, SettingsSection::Integrations).expect("integration rows");
        let codex_row = rows
            .iter()
            .find(|row| {
                matches!(
                    row,
                    SettingsListRow::Status { label, .. } if label.as_ref() == "codex"
                )
            })
            .expect("codex integration row");

        match codex_row {
            SettingsListRow::Status { status, tone, .. } => {
                assert_eq!(status.as_ref(), "installed · 1 profile hook missing");
                assert_eq!(*tone, SettingsMarkerTone::Warning);
            }
            _ => unreachable!("matched status row"),
        }

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn client_rows_use_the_client_pending_sidebar_width() {
        let mut app = AppState::test_new();
        app.settings.pending_sidebar_width = Some(22);
        let mut view = ClientViewState::from_default_client_state(&app);
        view.settings.section = SettingsSection::Layout;
        view.settings.pending_sidebar_width = Some(77);

        let rows = rows_for_section_for_view(&app, &view).expect("layout rows");
        let width = rows
            .iter()
            .find_map(|row| match row {
                SettingsListRow::Value { title, value, .. }
                    if title.as_ref() == "default sidebar width" =>
                {
                    Some(value.as_ref())
                }
                _ => None,
            })
            .expect("default sidebar width row");

        assert_eq!(width, "77 cols");
    }

    #[test]
    fn appearance_rows_keep_blank_line_between_sidebar_and_panes() {
        let app = AppState::test_new();
        let rows = appearance_rows(&app, &app.settings);
        let arrangement = rows
            .iter()
            .position(|row| {
                matches!(
                    row,
                    SettingsListRow::Value { title, .. } if title.as_ref() == "sidebar arrangement"
                )
            })
            .expect("sidebar arrangement row");
        assert!(matches!(rows[arrangement + 1], SettingsListRow::Spacer));
        assert!(matches!(
            rows[arrangement + 2],
            SettingsListRow::Header("panes")
        ));
    }
}
