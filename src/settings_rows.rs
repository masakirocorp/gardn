use std::borrow::Cow;

use crate::{
    app::state::{normalize_theme_name, theme_names_for_appearance, AppState, SettingsSection},
    config::{NewTerminalCwdConfig, TerminalAccent, ThemeMode, ToastDelivery},
    terminal_theme::ThemeAppearance,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsListRow {
    Header(&'static str),
    Spacer,
    Option {
        index: usize,
        title: Cow<'static, str>,
        description: Cow<'static, str>,
        enabled: bool,
    },
    Choice {
        index: usize,
        label: Cow<'static, str>,
        checked: bool,
    },
    StatusChoice {
        index: usize,
        marker: Cow<'static, str>,
        label: Cow<'static, str>,
        tone: SettingsMarkerTone,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsMarkerTone {
    Good,
    Warning,
    Accent,
    Disabled,
}

pub(crate) fn option_index_for_visual_row(rows: &[SettingsListRow], row: usize) -> Option<usize> {
    let mut visual_row = 0;
    for entry in rows {
        match entry {
            SettingsListRow::Header(_) | SettingsListRow::Spacer => {
                if row == visual_row {
                    return None;
                }
                visual_row += 1;
            }
            SettingsListRow::Option { index, .. } => {
                if row == visual_row || row == visual_row + 1 {
                    return Some(*index);
                }
                visual_row += 2;
            }
            SettingsListRow::Choice { index, .. } | SettingsListRow::StatusChoice { index, .. } => {
                if row == visual_row {
                    return Some(*index);
                }
                visual_row += 1;
            }
        }
    }
    None
}

pub(crate) fn rows_for_section(
    app: &AppState,
    section: SettingsSection,
) -> Option<Vec<SettingsListRow>> {
    match section {
        SettingsSection::Theme => Some(theme_rows(app)),
        SettingsSection::Layout => Some(layout_rows(app)),
        SettingsSection::Sound => Some(sound_rows(app)),
        SettingsSection::Toast => Some(toast_rows(app)),
        SettingsSection::PaneLabels => Some(behavior_rows(app)),
        SettingsSection::Experiments => Some(experiment_rows(app)),
        SettingsSection::Integrations => Some(integration_rows(app)),
    }
}

pub(crate) fn selected_visual_row(rows: &[SettingsListRow], selected: usize) -> Option<usize> {
    let mut visual_row = 0;
    for entry in rows {
        match entry {
            SettingsListRow::Header(_) | SettingsListRow::Spacer => visual_row += 1,
            SettingsListRow::Option { index, .. } => {
                if *index == selected {
                    return Some(visual_row);
                }
                visual_row += 2;
            }
            SettingsListRow::Choice { index, .. } | SettingsListRow::StatusChoice { index, .. } => {
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
            | SettingsListRow::Spacer
            | SettingsListRow::Choice { .. }
            | SettingsListRow::StatusChoice { .. } => 1,
            SettingsListRow::Option { .. } => 2,
        })
        .sum()
}

pub(crate) fn option_count(rows: &[SettingsListRow]) -> usize {
    rows.iter()
        .filter(|row| {
            matches!(
                row,
                SettingsListRow::Option { .. }
                    | SettingsListRow::Choice { .. }
                    | SettingsListRow::StatusChoice { .. }
            )
        })
        .count()
}

fn theme_rows(app: &AppState) -> Vec<SettingsListRow> {
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
    let show_terminal_accent = app.settings.group_theme_target.is_none() && system_source;

    let mut rows = Vec::new();
    rows.push(SettingsListRow::Header("colors"));
    rows.push(choice(0, "terminal", system_source));
    rows.push(choice(1, "palettes", !system_source));

    if show_terminal_accent {
        rows.push(SettingsListRow::Spacer);
        rows.push(SettingsListRow::Header("light accent"));
        let pending_light_accent = app
            .settings
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
        let pending_dark_accent = app
            .settings
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

fn layout_rows(app: &AppState) -> Vec<SettingsListRow> {
    let width = app
        .settings
        .pending_sidebar_width
        .unwrap_or(app.default_sidebar_width);
    let min = app
        .settings
        .pending_sidebar_min_width
        .unwrap_or(app.sidebar_min_width);
    let max = app
        .settings
        .pending_sidebar_max_width
        .unwrap_or(app.sidebar_max_width);
    let worktree_directory = app
        .settings
        .pending_worktree_directory
        .clone()
        .unwrap_or_else(|| app.worktree_directory.display().to_string());

    vec![
        SettingsListRow::Header("sidebar"),
        option(0, "default sidebar width", format!("{width} columns"), true),
        option(1, "minimum sidebar width", format!("{min} columns"), true),
        option(2, "maximum sidebar width", format!("{max} columns"), true),
        SettingsListRow::Spacer,
        SettingsListRow::Header("worktrees"),
        option(3, "worktree directory", worktree_directory, true),
    ]
}

fn behavior_rows(app: &AppState) -> Vec<SettingsListRow> {
    let cwd_label = new_terminal_cwd_label(
        &app.settings
            .pending_new_terminal_cwd
            .clone()
            .unwrap_or_else(|| app.new_terminal_cwd.clone()),
    );
    let scroll_label = format!(
        "{} lines per wheel notch",
        app.settings
            .pending_mouse_scroll_lines
            .unwrap_or(app.mouse_scroll_lines)
    );

    vec![
        SettingsListRow::Header("workspace"),
        option(
            0,
            "confirm before closing workspaces",
            "ask before closing a workspace",
            app.settings
                .pending_confirm_close
                .unwrap_or_else(|| app.confirm_close_enabled()),
        ),
        option(
            1,
            "name new tabs",
            "ask for a tab name before creating a new tab",
            app.settings
                .pending_prompt_new_tab_name
                .unwrap_or_else(|| app.prompt_new_tab_name_enabled()),
        ),
        SettingsListRow::Spacer,
        SettingsListRow::Header("terminal"),
        option(2, "new terminal cwd", cwd_label, true),
        option(3, "mouse wheel speed", scroll_label, true),
        option(
            4,
            "agent border labels",
            "show detected agent names in split pane borders",
            app.settings
                .pending_agent_border_labels
                .unwrap_or_else(|| app.agent_border_labels_enabled()),
        ),
    ]
}

fn experiment_rows(app: &AppState) -> Vec<SettingsListRow> {
    vec![
        SettingsListRow::Header("restore"),
        option(
            0,
            "resume agent sessions",
            "restart supported agents in their native session after restore",
            app.resume_agents_on_restore_enabled(),
        ),
        SettingsListRow::Spacer,
        SettingsListRow::Header("history"),
        option(
            1,
            "pane screen history",
            "save recent pane output across server restarts",
            app.pane_history_persistence_enabled(),
        ),
        SettingsListRow::Spacer,
        SettingsListRow::Header("input"),
        option(
            2,
            "switch to ascii input source in prefix (macOS)",
            "temporarily use an ASCII-capable layout for prefix commands",
            app.switch_ascii_input_source_in_prefix_enabled(),
        ),
    ]
}

fn sound_rows(app: &AppState) -> Vec<SettingsListRow> {
    let current = app
        .settings
        .pending_sound_enabled
        .unwrap_or_else(|| app.sound_enabled());
    vec![
        SettingsListRow::Header("sound alerts"),
        choice(0, "on", current),
        choice(1, "off", !current),
    ]
}

fn integration_rows(app: &AppState) -> Vec<SettingsListRow> {
    app.integration_recommendations
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let (marker, tone) = match item.state {
                crate::integration::IntegrationStatusKind::Current => {
                    ("✓", SettingsMarkerTone::Good)
                }
                crate::integration::IntegrationStatusKind::Outdated => {
                    ("↻", SettingsMarkerTone::Warning)
                }
                crate::integration::IntegrationStatusKind::NotInstalled if item.available => {
                    ("+", SettingsMarkerTone::Accent)
                }
                crate::integration::IntegrationStatusKind::NotInstalled => {
                    ("–", SettingsMarkerTone::Disabled)
                }
            };
            SettingsListRow::StatusChoice {
                index,
                marker: marker.into(),
                label: format!("{:<9}{}", item.label, item.status_label()).into(),
                tone,
            }
        })
        .collect()
}
fn toast_rows(app: &AppState) -> Vec<SettingsListRow> {
    let current = app
        .settings
        .pending_toast_delivery
        .unwrap_or_else(|| app.toast_delivery());
    vec![
        SettingsListRow::Header("notification popups"),
        choice(0, "off", current == ToastDelivery::Off),
        choice(1, "inside hako", current == ToastDelivery::Hako),
        choice(2, "via terminal", current == ToastDelivery::Terminal),
        choice(3, "via system", current == ToastDelivery::System),
    ]
}

fn option(
    index: usize,
    title: impl Into<Cow<'static, str>>,
    description: impl Into<Cow<'static, str>>,
    enabled: bool,
) -> SettingsListRow {
    SettingsListRow::Option {
        index,
        title: title.into(),
        description: description.into(),
        enabled,
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
        let app = AppState::test_new();
        let rows = rows_for_section(&app, SettingsSection::PaneLabels).unwrap();

        assert_eq!(option_index_for_visual_row(&rows, 0), None);
        assert_eq!(option_index_for_visual_row(&rows, 1), Some(0));
        assert_eq!(option_index_for_visual_row(&rows, 2), Some(0));
        assert_eq!(option_index_for_visual_row(&rows, 3), Some(1));
        assert_eq!(option_index_for_visual_row(&rows, 4), Some(1));
        assert_eq!(option_index_for_visual_row(&rows, 5), None);
        assert_eq!(option_index_for_visual_row(&rows, 6), None);
        assert_eq!(option_index_for_visual_row(&rows, 7), Some(2));
        assert_eq!(option_index_for_visual_row(&rows, 11), Some(4));
    }
}
