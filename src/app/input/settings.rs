use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};

use crate::{
    app::{
        state::{
            normalize_theme_name, theme_names_for_appearance, AppState, DragState, DragTarget,
            SettingsSection, THEME_NAMES,
        },
        App, Mode,
    },
    config::{NewTerminalCwdConfig, TerminalAccent, ThemeMode, ToastDelivery},
    settings_rows::{
        option_count, option_index_for_visual_row, rows_for_section, selected_visual_row,
        visual_row_count,
    },
    terminal_theme::ThemeAppearance,
};

use super::ScrollbarClickTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
// The shared `Save` verb is semantic: these actions persist settings.
#[allow(clippy::enum_variant_names)]
pub(super) enum SettingsAction {
    SaveSettings {
        light: String,
        dark: String,
        mode: ThemeMode,
        terminal_light_accent: TerminalAccent,
        terminal_dark_accent: TerminalAccent,
        sound_enabled: bool,
        toast_delivery: ToastDelivery,
        confirm_close: bool,
        prompt_new_tab_name: bool,
        new_terminal_cwd: NewTerminalCwdConfig,
        mouse_scroll_lines: usize,
        sidebar_width: u16,
        sidebar_min_width: u16,
        sidebar_max_width: u16,
        worktree_directory: Option<String>,
        agent_border_labels: bool,
    },
    SaveSwitchAsciiInputSourceInPrefix(bool),
    SaveGroupAccent {
        group_idx: usize,
        accent: Option<TerminalAccent>,
    },
    SaveGroupName {
        group_idx: usize,
        name: String,
    },
    DeleteGroup(usize),
    InstallRecommendedIntegrations,
    InstallIntegration(crate::api::schema::IntegrationTarget),
    UninstallIntegration(crate::api::schema::IntegrationTarget),
}

impl App {
    pub(crate) fn handle_settings_key(&mut self, key: KeyEvent) {
        let previous_section = self.state.settings.section;
        if let Some(action) = update_settings_state(&mut self.state, key) {
            match action {
                SettingsAction::SaveSettings {
                    light,
                    dark,
                    mode,
                    terminal_light_accent,
                    terminal_dark_accent,
                    sound_enabled,
                    toast_delivery,
                    confirm_close,
                    prompt_new_tab_name,
                    new_terminal_cwd,
                    mouse_scroll_lines,
                    sidebar_width,
                    sidebar_min_width,
                    sidebar_max_width,
                    agent_border_labels,
                    worktree_directory,
                } => {
                    self.save_theme(
                        &light,
                        &dark,
                        mode,
                        terminal_light_accent,
                        terminal_dark_accent,
                    );
                    self.save_sound(sound_enabled);
                    self.save_confirm_close(confirm_close);
                    self.save_prompt_new_tab_name(prompt_new_tab_name);
                    self.save_new_terminal_cwd(&new_terminal_cwd);
                    self.save_mouse_scroll_lines(mouse_scroll_lines);
                    self.save_sidebar_widths(sidebar_width, sidebar_min_width, sidebar_max_width);
                    if let Some(directory) = worktree_directory {
                        self.save_worktree_directory(&directory);
                    }
                    self.save_toast_delivery(toast_delivery);
                    self.save_agent_border_labels(agent_border_labels);
                }
                SettingsAction::SaveGroupAccent { group_idx, accent } => {
                    self.state.set_group_accent(group_idx, accent);
                    self.query_host_terminal_theme();
                }
                SettingsAction::SaveGroupName { group_idx, name } => {
                    self.state.rename_group(group_idx, name);
                }
                SettingsAction::DeleteGroup(group_idx) => {
                    super::modal::open_confirm_delete_group(&mut self.state, group_idx);
                }
                SettingsAction::SaveSwitchAsciiInputSourceInPrefix(enabled) => {
                    self.save_switch_ascii_input_source_in_prefix(enabled)
                }
                SettingsAction::InstallRecommendedIntegrations => {
                    self.install_recommended_integrations()
                }
                SettingsAction::InstallIntegration(target) => self.install_integration(target),
                SettingsAction::UninstallIntegration(target) => self.uninstall_integration(target),
            }
        }
        if previous_section != SettingsSection::Integrations
            && self.state.settings.section == SettingsSection::Integrations
        {
            self.refresh_integration_recommendations();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeChoiceTarget {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThemeChoice {
    name: &'static str,
    target: ThemeChoiceTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeSettingsChoice {
    SourceSystem,
    SourceCustom,
    GroupAccent(Option<TerminalAccent>),
    TerminalLightAccent(TerminalAccent),
    TerminalDarkAccent(TerminalAccent),
    Mode(ThemeMode),
    Theme(ThemeChoice),
}

fn pending_uses_system_theme_source(state: &AppState) -> bool {
    pending_theme_mode(state) == ThemeMode::System
        && normalize_theme_name(&pending_light_theme_name(state)) == "system"
        && normalize_theme_name(&pending_dark_theme_name(state)) == "system"
}

fn pending_shows_terminal_accent(state: &AppState) -> bool {
    state.settings.group_settings_target.is_none() && pending_uses_system_theme_source(state)
}

fn toast_delivery_index(delivery: ToastDelivery) -> usize {
    match delivery {
        ToastDelivery::Off => 0,
        ToastDelivery::Hako => 1,
        ToastDelivery::Terminal => 2,
        ToastDelivery::System => 3,
    }
}

fn toast_delivery_for_index(idx: usize) -> ToastDelivery {
    match idx {
        0 => ToastDelivery::Off,
        1 => ToastDelivery::Hako,
        2 => ToastDelivery::Terminal,
        _ => ToastDelivery::System,
    }
}

fn global_theme_choices(mode: ThemeMode) -> Vec<ThemeChoice> {
    match mode {
        ThemeMode::Light => theme_names_for_appearance(ThemeAppearance::Light)
            .iter()
            .copied()
            .map(|name| ThemeChoice {
                name,
                target: ThemeChoiceTarget::Light,
            })
            .collect(),
        ThemeMode::Dark => theme_names_for_appearance(ThemeAppearance::Dark)
            .iter()
            .copied()
            .map(|name| ThemeChoice {
                name,
                target: ThemeChoiceTarget::Dark,
            })
            .collect(),
        ThemeMode::System => {
            let mut choices = Vec::with_capacity(
                theme_names_for_appearance(ThemeAppearance::Light).len()
                    + theme_names_for_appearance(ThemeAppearance::Dark).len(),
            );
            choices.extend(
                theme_names_for_appearance(ThemeAppearance::Light)
                    .iter()
                    .copied()
                    .map(|name| ThemeChoice {
                        name,
                        target: ThemeChoiceTarget::Light,
                    }),
            );
            choices.extend(
                theme_names_for_appearance(ThemeAppearance::Dark)
                    .iter()
                    .copied()
                    .map(|name| ThemeChoice {
                        name,
                        target: ThemeChoiceTarget::Dark,
                    }),
            );
            choices
        }
    }
}
fn theme_settings_choices(state: &AppState) -> Vec<ThemeSettingsChoice> {
    if state.settings.group_settings_target.is_some() {
        let mut choices = Vec::with_capacity(1 + TerminalAccent::ALL.len());
        choices.push(ThemeSettingsChoice::GroupAccent(None));
        choices.extend(
            TerminalAccent::ALL
                .iter()
                .copied()
                .map(|accent| ThemeSettingsChoice::GroupAccent(Some(accent))),
        );
        return choices;
    }

    let mut choices = Vec::with_capacity(
        2 + (TerminalAccent::ALL.len() * 2) + ThemeMode::ALL.len() + THEME_NAMES.len(),
    );
    choices.push(ThemeSettingsChoice::SourceSystem);
    choices.push(ThemeSettingsChoice::SourceCustom);
    if pending_shows_terminal_accent(state) {
        choices.extend(
            TerminalAccent::ALL
                .iter()
                .copied()
                .map(ThemeSettingsChoice::TerminalLightAccent),
        );
        choices.extend(
            TerminalAccent::ALL
                .iter()
                .copied()
                .map(ThemeSettingsChoice::TerminalDarkAccent),
        );
    }
    if !pending_uses_system_theme_source(state) {
        let theme_choices = global_theme_choices(pending_theme_mode(state));
        choices.extend(
            ThemeMode::ALL
                .iter()
                .copied()
                .map(ThemeSettingsChoice::Mode),
        );
        choices.extend(theme_choices.into_iter().map(ThemeSettingsChoice::Theme));
    }
    choices
}

fn theme_choice_len(state: &AppState) -> usize {
    theme_settings_choices(state).len()
}

fn theme_rows(state: &AppState) -> Vec<crate::settings_rows::SettingsListRow> {
    rows_for_section(state, SettingsSection::Theme).unwrap_or_default()
}

fn theme_visual_len(state: &AppState) -> usize {
    visual_row_count(&theme_rows(state))
}

fn theme_visual_row_for_selection(state: &AppState, selected: usize) -> usize {
    selected_visual_row(&theme_rows(state), selected).unwrap_or(0)
}

fn theme_selection_for_visual_row(state: &AppState, row: usize) -> Option<usize> {
    option_index_for_visual_row(&theme_rows(state), row)
}

fn settings_section_choice_len(state: &AppState, section: SettingsSection) -> usize {
    rows_for_section(state, section)
        .map(|rows| option_count(&rows))
        .unwrap_or_else(|| match section {
            SettingsSection::Integrations => state.integration_recommendations.len(),
            SettingsSection::Theme => theme_choice_len(state),
            SettingsSection::Layout
            | SettingsSection::Sound
            | SettingsSection::Toast
            | SettingsSection::PaneLabels
            | SettingsSection::Experiments
            | SettingsSection::GroupGeneral => 0,
        })
}

fn settings_theme_list_rect(area: Rect) -> Rect {
    let [_, _, list_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(2),
    ])
    .areas::<3>(area);
    list_area
}

fn theme_scroll_len(state: &AppState) -> usize {
    theme_visual_len(state)
}

fn settings_theme_max_scroll(state: &AppState) -> usize {
    settings_theme_viewport(state).max_scroll()
}

fn settings_theme_viewport(state: &AppState) -> crate::ui::ModalListViewport {
    crate::ui::ModalListViewport::new(
        theme_scroll_len(state),
        settings_theme_list_rect(state.settings_content_rect()).height as usize,
        state.settings.scroll,
    )
}

fn ensure_settings_selection_visible(state: &mut AppState) {
    let viewport = settings_theme_viewport(state);
    state.settings.scroll = viewport.scroll();

    let selected_row = theme_visual_row_for_selection(state, state.settings.list.selected);
    state.settings.scroll = viewport.ensure_visible(selected_row, None);
}

fn set_settings_theme_offset_from_bottom(state: &mut AppState, offset_from_bottom: usize) {
    state.settings.scroll =
        settings_theme_viewport(state).scroll_from_offset_from_bottom(offset_from_bottom);
}

fn group_accent_choice_at_cursor(state: &AppState) -> Option<TerminalAccent> {
    theme_settings_choices(state)
        .get(state.settings.list.selected)
        .and_then(|choice| match choice {
            ThemeSettingsChoice::GroupAccent(accent) => Some(*accent),
            _ => None,
        })
        .unwrap_or(None)
}

fn group_accent_selection_index(state: &AppState) -> usize {
    let accent = state
        .settings
        .pending_group_accent_choice
        .unwrap_or_else(|| {
            state
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .and_then(|group| group.accent)
        });
    accent
        .and_then(|accent| {
            TerminalAccent::ALL
                .iter()
                .position(|candidate| *candidate == accent)
                .map(|idx| idx + 1)
        })
        .unwrap_or(0)
}

fn checked_group_accent_choice(state: &AppState) -> Option<TerminalAccent> {
    state
        .settings
        .pending_group_accent_choice
        .unwrap_or_else(|| group_accent_choice_at_cursor(state))
}

fn pending_group_name(state: &AppState) -> String {
    state
        .settings
        .pending_group_name
        .clone()
        .or_else(|| {
            state
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .map(|group| group.name.clone())
        })
        .unwrap_or_default()
}

fn set_pending_group_name(state: &mut AppState, name: String) {
    state.settings.pending_group_name = Some(name);
}

fn delete_pending_group_name_word(state: &mut AppState) {
    let mut name = pending_group_name(state);
    while name.chars().last().is_some_and(char::is_whitespace) {
        name.pop();
    }
    while name.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
        name.pop();
    }
    set_pending_group_name(state, name);
}

fn edit_pending_group_name(state: &mut AppState, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('u')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            set_pending_group_name(state, String::new());
            true
        }
        KeyCode::Backspace
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::SUPER) =>
        {
            set_pending_group_name(state, String::new());
            true
        }
        KeyCode::Backspace
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
                || key.modifiers.contains(crossterm::event::KeyModifiers::ALT) =>
        {
            delete_pending_group_name_word(state);
            true
        }
        KeyCode::Char('h' | 'w')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL) =>
        {
            delete_pending_group_name_word(state);
            true
        }
        KeyCode::Backspace => {
            let mut name = pending_group_name(state);
            name.pop();
            set_pending_group_name(state, name);
            true
        }
        KeyCode::Char(c)
            if key
                .modifiers
                .difference(crossterm::event::KeyModifiers::SHIFT)
                .is_empty() =>
        {
            let mut name = pending_group_name(state);
            name.push(c);
            set_pending_group_name(state, name);
            true
        }
        _ => false,
    }
}

fn pending_light_theme_name(state: &AppState) -> String {
    state
        .settings
        .pending_light_theme_name
        .clone()
        .unwrap_or_else(|| state.global_light_theme_name.clone())
}

fn pending_dark_theme_name(state: &AppState) -> String {
    state
        .settings
        .pending_dark_theme_name
        .clone()
        .unwrap_or_else(|| state.global_dark_theme_name.clone())
}

fn pending_terminal_light_accent(state: &AppState) -> TerminalAccent {
    state
        .settings
        .pending_terminal_light_accent
        .unwrap_or(state.global_terminal_light_accent)
}

fn pending_terminal_dark_accent(state: &AppState) -> TerminalAccent {
    state
        .settings
        .pending_terminal_dark_accent
        .unwrap_or(state.global_terminal_dark_accent)
}

fn selected_theme_settings_choice(state: &AppState) -> Option<ThemeSettingsChoice> {
    theme_settings_choices(state)
        .get(state.settings.list.selected)
        .copied()
}

fn pending_sound_enabled(state: &AppState) -> bool {
    state
        .settings
        .pending_sound_enabled
        .unwrap_or_else(|| state.sound_enabled())
}

fn pending_toast_delivery(state: &AppState) -> ToastDelivery {
    state
        .settings
        .pending_toast_delivery
        .unwrap_or_else(|| state.toast_delivery())
}

fn pending_confirm_close(state: &AppState) -> bool {
    state
        .settings
        .pending_confirm_close
        .unwrap_or_else(|| state.confirm_close_enabled())
}

fn pending_prompt_new_tab_name(state: &AppState) -> bool {
    state
        .settings
        .pending_prompt_new_tab_name
        .unwrap_or_else(|| state.prompt_new_tab_name_enabled())
}

fn pending_new_terminal_cwd(state: &AppState) -> NewTerminalCwdConfig {
    state
        .settings
        .pending_new_terminal_cwd
        .clone()
        .unwrap_or_else(|| state.new_terminal_cwd.clone())
}

fn pending_mouse_scroll_lines(state: &AppState) -> usize {
    state
        .settings
        .pending_mouse_scroll_lines
        .unwrap_or(state.mouse_scroll_lines)
}

fn pending_sidebar_width(state: &AppState) -> u16 {
    state
        .settings
        .pending_sidebar_width
        .unwrap_or(state.default_sidebar_width)
}

fn pending_sidebar_min_width(state: &AppState) -> u16 {
    state
        .settings
        .pending_sidebar_min_width
        .unwrap_or(state.sidebar_min_width)
}

fn pending_sidebar_max_width(state: &AppState) -> u16 {
    state
        .settings
        .pending_sidebar_max_width
        .unwrap_or(state.sidebar_max_width)
}

fn pending_agent_border_labels(state: &AppState) -> bool {
    state
        .settings
        .pending_agent_border_labels
        .unwrap_or_else(|| state.agent_border_labels_enabled())
}

fn selected_global_theme_name_for_mode(state: &AppState) -> String {
    match pending_theme_mode(state) {
        ThemeMode::Light => pending_light_theme_name(state),
        ThemeMode::Dark => pending_dark_theme_name(state),
        ThemeMode::System => match state.theme_appearance_for_mode(ThemeMode::System) {
            ThemeAppearance::Light => pending_light_theme_name(state),
            ThemeAppearance::Dark => pending_dark_theme_name(state),
        },
    }
}
fn target_theme_index(state: &AppState) -> usize {
    if pending_uses_system_theme_source(state) {
        0
    } else {
        2 + current_theme_mode_index(pending_theme_mode(state))
    }
}

fn current_theme_mode_index(mode: ThemeMode) -> usize {
    ThemeMode::ALL
        .iter()
        .position(|candidate| *candidate == mode)
        .unwrap_or(0)
}

fn preview_selected_theme(state: &mut AppState) {
    let Some(choice) = selected_theme_settings_choice(state) else {
        return;
    };
    match choice {
        ThemeSettingsChoice::GroupAccent(accent) => {
            state.settings.pending_group_accent_choice = Some(accent);
            preview_group_accent(state, accent);
        }
        ThemeSettingsChoice::SourceSystem => {
            state.settings.pending_theme_mode = Some(ThemeMode::System);
            state.settings.pending_light_theme_name = Some("system".to_string());
            state.settings.pending_dark_theme_name = Some("system".to_string());
            state.settings.list.selected = 0;
            ensure_settings_selection_visible(state);
            let mode = ThemeMode::System;
            let accent = state.terminal_accent_for_mode(mode);
            state.preview_theme_with_mode_and_terminal_accent("system", mode, accent);
        }
        ThemeSettingsChoice::TerminalLightAccent(accent) => {
            state.settings.pending_terminal_light_accent = Some(accent);
            state.preview_theme_with_mode_and_terminal_accent("system", ThemeMode::Light, accent);
        }
        ThemeSettingsChoice::TerminalDarkAccent(accent) => {
            state.settings.pending_terminal_dark_accent = Some(accent);
            state.preview_theme_with_mode_and_terminal_accent("system", ThemeMode::Dark, accent);
        }
        ThemeSettingsChoice::SourceCustom => {
            if normalize_theme_name(&pending_light_theme_name(state)) == "system" {
                state.settings.pending_light_theme_name =
                    Some(crate::app::state::DEFAULT_LIGHT_THEME_NAME.to_string());
            }
            if normalize_theme_name(&pending_dark_theme_name(state)) == "system" {
                state.settings.pending_dark_theme_name =
                    Some(crate::app::state::DEFAULT_DARK_THEME_NAME.to_string());
            }
            state.settings.list.selected = 1;
            ensure_settings_selection_visible(state);
            let mode = pending_theme_mode(state);
            let name = selected_global_theme_name_for_mode(state);
            state.preview_theme_with_mode(&name, mode);
        }
        ThemeSettingsChoice::Mode(mode) => {
            state.settings.pending_theme_mode = Some(mode);
            let next_selected = 2 + current_theme_mode_index(mode);
            state.settings.list.selected = next_selected;
            ensure_settings_selection_visible(state);
            let name = selected_global_theme_name_for_mode(state);
            state.preview_theme_with_mode(&name, mode);
        }
        ThemeSettingsChoice::Theme(choice) => match choice.target {
            ThemeChoiceTarget::Light => {
                state.settings.pending_light_theme_name = Some(choice.name.to_string());
                state.preview_theme_with_mode(choice.name, ThemeMode::Light);
            }
            ThemeChoiceTarget::Dark => {
                state.settings.pending_dark_theme_name = Some(choice.name.to_string());
                state.preview_theme_with_mode(choice.name, ThemeMode::Dark);
            }
        },
    }
}

fn pending_theme_mode(state: &AppState) -> ThemeMode {
    state
        .settings
        .pending_theme_mode
        .unwrap_or(state.global_theme_mode)
}

fn preview_group_accent(state: &mut AppState, accent: Option<TerminalAccent>) {
    state.palette = state.global_palette.clone();
    state.theme_name = state.global_theme_name.clone();
    if let Some(accent) = accent {
        state.palette.accent =
            crate::app::state::Palette::terminal_accent_color(state.host_terminal_theme, accent);
    }
}

fn cancel_settings(state: &mut AppState) {
    if let Some(palette) = state.settings.original_palette.take() {
        state.palette = palette;
    }
    if let Some(theme_name) = state.settings.original_theme.take() {
        state.theme_name = theme_name;
    }
    state.settings.pending_theme_name = None;
    state.settings.pending_theme_mode = None;
    state.settings.pending_light_theme_name = None;
    state.settings.pending_dark_theme_name = None;
    state.settings.pending_terminal_light_accent = None;
    state.settings.pending_terminal_dark_accent = None;
    state.settings.pending_sound_enabled = None;
    state.settings.pending_toast_delivery = None;
    state.settings.pending_confirm_close = None;
    state.settings.pending_prompt_new_tab_name = None;
    state.settings.pending_new_terminal_cwd = None;
    state.settings.pending_worktree_directory = None;
    state.settings.pending_mouse_scroll_lines = None;
    state.settings.pending_sidebar_width = None;
    state.settings.pending_sidebar_min_width = None;
    state.settings.pending_sidebar_max_width = None;
    state.settings.pending_agent_border_labels = None;
    state.settings.pending_switch_ascii_input_source_in_prefix = None;
    state.settings.pending_group_accent_choice = None;
    state.settings.pending_group_name = None;
    state.settings.group_settings_target = None;
    super::modal::leave_modal(state);
}

fn integrations_need_install(state: &AppState) -> bool {
    state
        .integration_recommendations
        .iter()
        .any(crate::integration::IntegrationRecommendation::needs_install)
}

fn selected_integration_action(state: &AppState) -> Option<SettingsAction> {
    let recommendation = state
        .integration_recommendations
        .get(state.settings.list.selected)?;

    match recommendation.state {
        crate::integration::IntegrationStatusKind::Current => {
            Some(SettingsAction::UninstallIntegration(recommendation.target))
        }
        crate::integration::IntegrationStatusKind::Outdated => {
            Some(SettingsAction::InstallIntegration(recommendation.target))
        }
        crate::integration::IntegrationStatusKind::NotInstalled if recommendation.available => {
            Some(SettingsAction::InstallIntegration(recommendation.target))
        }
        crate::integration::IntegrationStatusKind::NotInstalled => None,
    }
}

fn selected_group_general_action(state: &mut AppState) -> Option<SettingsAction> {
    let group_idx = state.settings.group_settings_target?;
    match state.settings.list.selected {
        0 => apply_settings(state),
        1 => {
            cancel_settings(state);
            Some(SettingsAction::DeleteGroup(group_idx))
        }
        _ => None,
    }
}

fn clear_settings_pending(state: &mut AppState) {
    state.settings.pending_theme_name = None;
    state.settings.pending_theme_mode = None;
    state.settings.pending_light_theme_name = None;
    state.settings.pending_dark_theme_name = None;
    state.settings.pending_terminal_light_accent = None;
    state.settings.pending_terminal_dark_accent = None;
    state.settings.pending_sound_enabled = None;
    state.settings.pending_toast_delivery = None;
    state.settings.pending_confirm_close = None;
    state.settings.pending_prompt_new_tab_name = None;
    state.settings.pending_new_terminal_cwd = None;
    state.settings.pending_mouse_scroll_lines = None;
    state.settings.pending_sidebar_width = None;
    state.settings.pending_sidebar_min_width = None;
    state.settings.pending_sidebar_max_width = None;
    state.settings.pending_worktree_directory = None;
    state.settings.pending_agent_border_labels = None;
    state.settings.pending_switch_ascii_input_source_in_prefix = None;
    state.settings.pending_group_accent_choice = None;
    state.settings.pending_group_name = None;
    state.settings.group_settings_target = None;
}

fn apply_settings(state: &mut AppState) -> Option<SettingsAction> {
    if state.settings.section == SettingsSection::Integrations {
        return integrations_need_install(state)
            .then_some(SettingsAction::InstallRecommendedIntegrations);
    }
    if state.settings.section == SettingsSection::GroupGeneral {
        let group_idx = state.settings.group_settings_target;
        let name = pending_group_name(state).trim().to_string();
        state.settings.original_palette = None;
        state.settings.original_theme = None;
        clear_settings_pending(state);
        super::modal::leave_modal(state);
        return group_idx
            .zip((!name.is_empty()).then_some(name))
            .map(|(group_idx, name)| SettingsAction::SaveGroupName { group_idx, name });
    }

    let light = pending_light_theme_name(state);
    let dark = pending_dark_theme_name(state);
    let theme_mode = pending_theme_mode(state);
    let terminal_light_accent = pending_terminal_light_accent(state);
    let terminal_dark_accent = pending_terminal_dark_accent(state);
    let sound_enabled = pending_sound_enabled(state);
    let toast_delivery = pending_toast_delivery(state);
    let confirm_close = pending_confirm_close(state);
    let prompt_new_tab_name = pending_prompt_new_tab_name(state);
    let new_terminal_cwd = pending_new_terminal_cwd(state);
    let mouse_scroll_lines = pending_mouse_scroll_lines(state);
    let sidebar_width = pending_sidebar_width(state);
    let sidebar_min_width = pending_sidebar_min_width(state);
    let sidebar_max_width = pending_sidebar_max_width(state);
    let worktree_directory = state.settings.pending_worktree_directory.clone();
    let agent_border_labels = pending_agent_border_labels(state);
    let group_accent = state
        .settings
        .group_settings_target
        .map(|_| checked_group_accent_choice(state));
    let group_settings_target = state.settings.group_settings_target.take();

    state.settings.original_palette = None;
    state.settings.original_theme = None;
    clear_settings_pending(state);
    super::modal::leave_modal(state);

    if let (Some(group_idx), Some(accent)) = (group_settings_target, group_accent) {
        return Some(SettingsAction::SaveGroupAccent { group_idx, accent });
    }

    Some(SettingsAction::SaveSettings {
        light,
        dark,
        mode: theme_mode,
        terminal_light_accent,
        terminal_dark_accent,
        sound_enabled,
        toast_delivery,
        confirm_close,
        prompt_new_tab_name,
        new_terminal_cwd,
        mouse_scroll_lines,
        sidebar_width,
        sidebar_min_width,
        sidebar_max_width,
        worktree_directory,
        agent_border_labels,
    })
}

fn next_terminal_cwd_policy(policy: NewTerminalCwdConfig) -> NewTerminalCwdConfig {
    match policy {
        NewTerminalCwdConfig::Follow => NewTerminalCwdConfig::Home,
        NewTerminalCwdConfig::Home => NewTerminalCwdConfig::Current,
        NewTerminalCwdConfig::Current | NewTerminalCwdConfig::Path(_) => {
            NewTerminalCwdConfig::Follow
        }
    }
}

fn next_mouse_scroll_lines(lines: usize) -> usize {
    match lines {
        0 | 1 => 3,
        2 | 3 => 5,
        4 | 5 => 10,
        _ => 1,
    }
}

fn select_pending_layout_setting(state: &mut AppState) {
    match state.settings.list.selected {
        0 => {
            let min = pending_sidebar_min_width(state);
            let max = pending_sidebar_max_width(state);
            let current = pending_sidebar_width(state).clamp(min, max);
            let next = current.saturating_add(2);
            state.settings.pending_sidebar_width = Some(if next > max { min } else { next });
        }
        1 => {
            let max = pending_sidebar_max_width(state);
            let current = pending_sidebar_min_width(state);
            let next = current.saturating_add(2);
            let next = if next >= max {
                10.min(max)
            } else {
                next.max(10)
            };
            state.settings.pending_sidebar_min_width = Some(next);
            state.settings.pending_sidebar_width = Some(pending_sidebar_width(state).max(next));
        }
        2 => {
            let min = pending_sidebar_min_width(state);
            let current = pending_sidebar_max_width(state);
            let next = current.saturating_add(2);
            let next = if next > 48 {
                min.max(24)
            } else {
                next.max(min)
            };
            state.settings.pending_sidebar_max_width = Some(next);
            state.settings.pending_sidebar_width = Some(pending_sidebar_width(state).min(next));
        }
        3 => super::modal::open_worktree_directory_editor(state),
        _ => {}
    }
}
fn select_pending_setting(state: &mut AppState) -> Option<SettingsAction> {
    match state.settings.section {
        SettingsSection::Theme => {
            preview_selected_theme(state);
            None
        }
        SettingsSection::Layout => {
            select_pending_layout_setting(state);
            None
        }
        SettingsSection::Sound => {
            state.settings.pending_sound_enabled = Some(state.settings.list.selected == 0);
            None
        }
        SettingsSection::Toast => {
            state.settings.pending_toast_delivery =
                Some(toast_delivery_for_index(state.settings.list.selected));
            None
        }
        SettingsSection::PaneLabels => {
            match state.settings.list.selected {
                0 => state.settings.pending_confirm_close = Some(!pending_confirm_close(state)),
                1 => {
                    state.settings.pending_prompt_new_tab_name =
                        Some(!pending_prompt_new_tab_name(state))
                }
                2 => {
                    let next = next_terminal_cwd_policy(pending_new_terminal_cwd(state));
                    state.settings.pending_new_terminal_cwd = Some(next);
                }
                3 => {
                    let next = next_mouse_scroll_lines(pending_mouse_scroll_lines(state));
                    state.settings.pending_mouse_scroll_lines = Some(next);
                }
                4 => {
                    state.settings.pending_agent_border_labels =
                        Some(!pending_agent_border_labels(state))
                }
                _ => {}
            }
            None
        }
        SettingsSection::Experiments => selected_experiment_action(state),
        SettingsSection::Integrations => selected_integration_action(state),
        SettingsSection::GroupGeneral => selected_group_general_action(state),
    }
}

fn selected_experiment_action(state: &mut AppState) -> Option<SettingsAction> {
    match state.settings.list.selected {
        0 => Some(SettingsAction::SaveSwitchAsciiInputSourceInPrefix(
            !state.switch_ascii_input_source_in_prefix_enabled(),
        )),
        _ => None,
    }
}
fn select_previous_setting(state: &mut AppState, item_count: usize) {
    if item_count == 0 {
        return;
    }

    let selected = state.settings.list.selected.min(item_count - 1);
    state.settings.list.selected = if selected == 0 {
        item_count - 1
    } else {
        selected - 1
    };
}

fn select_next_setting(state: &mut AppState, item_count: usize) {
    if item_count == 0 {
        return;
    }

    let selected = state.settings.list.selected.min(item_count - 1);
    state.settings.list.selected = if selected + 1 == item_count {
        0
    } else {
        selected + 1
    };
}

fn handle_settings_modal_action(state: &mut AppState, key: &KeyEvent) -> Option<SettingsAction> {
    match super::modal::modal_action_from_key(key, super::modal::SETTINGS_ACTIONS) {
        Some(super::modal::ModalAction::Apply) => apply_settings(state),
        Some(super::modal::ModalAction::Close) => {
            cancel_settings(state);
            None
        }
        _ => None,
    }
}

pub(super) fn update_settings_state(state: &mut AppState, key: KeyEvent) -> Option<SettingsAction> {
    if state.settings.group_settings_target.is_some()
        && !matches!(
            state.settings.section,
            SettingsSection::Theme | SettingsSection::GroupGeneral
        )
    {
        state.settings.section = SettingsSection::Theme;
    }
    match state.settings.section {
        SettingsSection::Theme => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(state, theme_choice_len(state));
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(state, theme_choice_len(state));
                ensure_settings_selection_visible(state);
            }
            KeyCode::PageUp => {
                state.settings.scroll = state
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_PAGE_SCROLL_ROWS as usize);
            }
            KeyCode::PageDown => {
                state.settings.scroll = state
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_PAGE_SCROLL_ROWS as usize)
                    .min(settings_theme_max_scroll(state));
            }
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                if state.settings.group_settings_target.is_some() {
                    state.settings.section = SettingsSection::GroupGeneral;
                    state.settings.list.selected = 0;
                } else {
                    state.settings.section = SettingsSection::Layout;
                    state.settings.list.selected = 0;
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                if state.settings.group_settings_target.is_some() {
                    state.settings.section = SettingsSection::GroupGeneral;
                    state.settings.list.selected = 0;
                } else {
                    state.settings.section = SettingsSection::Experiments;
                    state.settings.list.selected = 0;
                }
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Layout => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Layout),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Layout),
                );
            }
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Theme;
                state.settings.list.selected = target_theme_index(state);
                ensure_settings_selection_visible(state);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Sound;
                state.settings.list.selected = usize::from(!pending_sound_enabled(state));
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Sound => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Sound),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Sound),
                );
            }
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Toast;
                state.settings.list.selected = toast_delivery_index(pending_toast_delivery(state));
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Layout;
                state.settings.list.selected = 0;
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Toast => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Toast),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Toast),
                );
            }
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Sound;
                state.settings.list.selected = usize::from(!pending_sound_enabled(state));
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::PaneLabels;
                state.settings.list.selected = 0;
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::PaneLabels => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::PaneLabels),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::PaneLabels),
                );
            }
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Toast;
                state.settings.list.selected = toast_delivery_index(pending_toast_delivery(state));
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Integrations;
                state.settings.list.selected = 0;
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Experiments => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Experiments),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Experiments),
                );
            }
            KeyCode::Enter | KeyCode::Char(' ') => return selected_experiment_action(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Integrations;
                state.settings.list.selected = 0;
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Theme;
                state.settings.list.selected = target_theme_index(state);
                ensure_settings_selection_visible(state);
            }
            _ => {
                if let Some(super::modal::ModalAction::Close) =
                    super::modal::modal_action_from_key(&key, super::modal::SETTINGS_ACTIONS)
                {
                    cancel_settings(state);
                }
            }
        },
        SettingsSection::Integrations => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Integrations),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Integrations),
                );
            }
            KeyCode::Enter | KeyCode::Char(' ') => return selected_integration_action(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::PaneLabels;
                state.settings.list.selected = 0;
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Experiments;
                state.settings.list.selected = 0;
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::GroupGeneral => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::GroupGeneral),
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::GroupGeneral),
                );
            }
            KeyCode::Enter => return selected_group_general_action(state),
            KeyCode::Char(' ') if state.settings.list.selected == 1 => {
                return selected_group_general_action(state);
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Theme;
                state.settings.list.selected = group_accent_selection_index(state);
                ensure_settings_selection_visible(state);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Theme;
                state.settings.list.selected = group_accent_selection_index(state);
                ensure_settings_selection_visible(state);
            }
            _ => {
                if state.settings.list.selected == 0 && edit_pending_group_name(state, key) {
                    return None;
                }
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
    }

    None
}

pub(crate) fn open_settings(state: &mut AppState) {
    open_settings_at(state, SettingsSection::Theme);
}

pub(crate) fn open_settings_at(state: &mut AppState, section: SettingsSection) {
    state.settings.original_palette = Some(state.palette.clone());
    state.settings.original_theme = Some(state.theme_name.clone());
    state.settings.pending_theme_name = Some(state.global_theme_name.clone());
    state.settings.pending_theme_mode = Some(state.global_theme_mode);
    state.settings.pending_light_theme_name = Some(state.global_light_theme_name.clone());
    state.settings.pending_dark_theme_name = Some(state.global_dark_theme_name.clone());
    state.settings.pending_terminal_light_accent = Some(state.global_terminal_light_accent);
    state.settings.pending_terminal_dark_accent = Some(state.global_terminal_dark_accent);
    state.settings.pending_sound_enabled = Some(state.sound_enabled());
    state.settings.pending_toast_delivery = Some(state.toast_delivery());
    state.settings.pending_confirm_close = Some(state.confirm_close_enabled());
    state.settings.pending_prompt_new_tab_name = Some(state.prompt_new_tab_name_enabled());
    state.settings.pending_new_terminal_cwd = Some(state.new_terminal_cwd.clone());
    state.settings.pending_mouse_scroll_lines = Some(state.mouse_scroll_lines);
    state.settings.pending_sidebar_width = Some(state.default_sidebar_width);
    state.settings.pending_sidebar_min_width = Some(state.sidebar_min_width);
    state.settings.pending_sidebar_max_width = Some(state.sidebar_max_width);
    state.settings.pending_worktree_directory = None;
    state.settings.pending_agent_border_labels = Some(state.agent_border_labels_enabled());
    state.settings.group_settings_target = None;
    state.settings.section = section;
    state.settings.list.selected = match section {
        SettingsSection::Theme => target_theme_index(state),
        SettingsSection::Layout => 0,
        SettingsSection::Sound => usize::from(!pending_sound_enabled(state)),
        SettingsSection::Toast => toast_delivery_index(pending_toast_delivery(state)),
        SettingsSection::PaneLabels => 0,
        SettingsSection::Experiments => 0,
        SettingsSection::Integrations => 0,
        SettingsSection::GroupGeneral => 0,
    };
    state.settings.scroll = 0;
    ensure_settings_selection_visible(state);
    if section == SettingsSection::Theme {
        let theme_name = state
            .global_theme_name_for_mode(state.global_theme_mode)
            .to_string();
        state.preview_theme_with_mode(&theme_name, state.global_theme_mode);
    }
    state.mode = Mode::Settings;
}

pub(crate) fn open_group_settings(state: &mut AppState, group_idx: usize) {
    let Some(group) = state.groups.get(group_idx) else {
        return;
    };
    let group_accent = group.accent;
    let group_name = group.name.clone();
    state.settings.original_palette = Some(state.palette.clone());
    state.settings.original_theme = Some(state.theme_name.clone());
    state.settings.pending_theme_name = None;
    state.settings.pending_theme_mode = None;
    state.settings.pending_light_theme_name = None;
    state.settings.pending_dark_theme_name = None;
    state.settings.pending_terminal_light_accent = None;
    state.settings.pending_terminal_dark_accent = None;
    state.settings.pending_group_accent_choice = None;
    state.settings.pending_group_name = Some(group_name);
    state.settings.pending_sound_enabled = None;
    state.settings.pending_toast_delivery = None;
    state.settings.pending_confirm_close = None;
    state.settings.pending_prompt_new_tab_name = None;
    state.settings.pending_new_terminal_cwd = None;
    state.settings.pending_mouse_scroll_lines = None;
    state.settings.pending_sidebar_width = None;
    state.settings.pending_sidebar_min_width = None;
    state.settings.pending_sidebar_max_width = None;
    state.settings.pending_worktree_directory = None;
    state.settings.pending_agent_border_labels = None;
    state.settings.pending_switch_ascii_input_source_in_prefix = None;
    state.settings.group_settings_target = Some(group_idx);
    state.settings.section = SettingsSection::Theme;
    state.settings.list.selected = group_accent_selection_index(state);
    state.settings.scroll = 0;
    ensure_settings_selection_visible(state);
    preview_group_accent(state, group_accent);
    state.mode = Mode::Settings;
}
impl AppState {
    fn settings_popup_rect(&self) -> Rect {
        crate::ui::centered_popup_rect(self.screen_rect(), 76, 22).unwrap_or_default()
    }

    fn settings_inner_rect(&self) -> Rect {
        let popup = self.settings_popup_rect();
        Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        )
    }

    fn settings_tab_at(&self, col: u16, row: u16) -> Option<SettingsSection> {
        let sections = if self.settings.group_settings_target.is_some() {
            &[SettingsSection::Theme, SettingsSection::GroupGeneral][..]
        } else {
            SettingsSection::ALL
        };

        let inner = self.settings_inner_rect();
        let tab_y = inner.y + 1;
        if row != tab_y {
            return None;
        }
        let mut x = inner.x;
        for section in sections {
            let badge_width = if self.settings_section_has_badge(*section) {
                2
            } else {
                0
            };
            let label = if self.settings.group_settings_target.is_some()
                && *section == SettingsSection::Theme
            {
                "appearance"
            } else {
                section.label()
            };
            let width = label.len() as u16 + 2 + badge_width;
            if col >= x && col < x + width {
                return Some(*section);
            }
            x += width + 1;
        }
        None
    }

    pub(crate) fn settings_content_rect(&self) -> Rect {
        let inner = self.settings_inner_rect();
        crate::ui::modal_stack_areas(inner, 3, 2, 0, 1).content
    }

    fn settings_list_index_at(&self, col: u16, row: u16) -> Option<usize> {
        let area = self.settings_content_rect();
        if row < area.y || row >= area.y + area.height || col < area.x || col >= area.x + area.width
        {
            return None;
        }

        match self.settings.section {
            SettingsSection::Theme => {
                let list_area = settings_theme_list_rect(area);
                let visual_row =
                    settings_theme_viewport(self).hit_visual_row(list_area, col, row)?;
                theme_selection_for_visual_row(self, visual_row)
            }
            SettingsSection::Layout
            | SettingsSection::Sound
            | SettingsSection::Toast
            | SettingsSection::PaneLabels
            | SettingsSection::Experiments
            | SettingsSection::GroupGeneral => {
                let rows = rows_for_section(self, self.settings.section)?;
                option_index_for_visual_row(&rows, (row - area.y) as usize)
            }
            SettingsSection::Integrations => {
                let list_y = area.y + 4;
                if row < list_y {
                    return None;
                }
                let rows = rows_for_section(self, self.settings.section)?;
                option_index_for_visual_row(&rows, (row - list_y) as usize)
            }
        }
    }

    fn settings_theme_scrollbar_target_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<ScrollbarClickTarget> {
        if self.settings.section != SettingsSection::Theme {
            return None;
        }
        let list_area = settings_theme_list_rect(self.settings_content_rect());
        let viewport = settings_theme_viewport(self);
        let metrics = viewport.metrics();
        let track = viewport.scroll_area(list_area).track?;
        if !(col >= track.x
            && col < track.x + track.width
            && row >= track.y
            && row < track.y + track.height)
        {
            return None;
        }
        if let Some(grab_row_offset) = crate::ui::scrollbar_thumb_grab_offset(metrics, track, row) {
            Some(ScrollbarClickTarget::Thumb { grab_row_offset })
        } else {
            Some(ScrollbarClickTarget::Track {
                offset_from_bottom: crate::ui::scrollbar_offset_from_row(metrics, track, row),
            })
        }
    }

    fn settings_theme_offset_for_drag_row(&self, row: u16, grab_row_offset: u16) -> Option<usize> {
        if self.settings.section != SettingsSection::Theme {
            return None;
        }
        let list_area = settings_theme_list_rect(self.settings_content_rect());
        let viewport = settings_theme_viewport(self);
        let metrics = viewport.metrics();
        let track = viewport.scroll_area(list_area).track?;
        Some(crate::ui::scrollbar_offset_from_drag_row(
            metrics,
            track,
            row,
            grab_row_offset,
        ))
    }

    pub(super) fn handle_settings_mouse(&mut self, mouse: MouseEvent) -> Option<SettingsAction> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(target) =
                    self.settings_theme_scrollbar_target_at(mouse.column, mouse.row)
                {
                    match target {
                        ScrollbarClickTarget::Thumb { grab_row_offset } => {
                            self.drag = Some(DragState {
                                target: DragTarget::SettingsThemeScrollbar { grab_row_offset },
                            });
                        }
                        ScrollbarClickTarget::Track { offset_from_bottom } => {
                            set_settings_theme_offset_from_bottom(self, offset_from_bottom);
                        }
                    }
                    return None;
                }

                if let Some(section) = self.settings_tab_at(mouse.column, mouse.row) {
                    self.settings.section = section;
                    self.settings.list.select(match section {
                        SettingsSection::Theme => {
                            if self.settings.group_settings_target.is_some() {
                                group_accent_selection_index(self)
                            } else {
                                target_theme_index(self)
                            }
                        }
                        SettingsSection::Layout => 0,
                        SettingsSection::Sound => usize::from(!pending_sound_enabled(self)),
                        SettingsSection::Toast => {
                            toast_delivery_index(pending_toast_delivery(self))
                        }
                        SettingsSection::PaneLabels => 0,
                        SettingsSection::Experiments => 0,
                        SettingsSection::Integrations => 0,
                        SettingsSection::GroupGeneral => 0,
                    });
                    if section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                    return None;
                }
                if let Some(idx) = self.settings_list_index_at(mouse.column, mouse.row) {
                    self.settings.list.select(idx);
                    if self.settings.section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                    return match self.settings.section {
                        SettingsSection::Theme
                        | SettingsSection::Layout
                        | SettingsSection::Sound
                        | SettingsSection::Toast
                        | SettingsSection::PaneLabels => select_pending_setting(self),
                        SettingsSection::Experiments => selected_experiment_action(self),
                        SettingsSection::Integrations => None,
                        SettingsSection::GroupGeneral => selected_group_general_action(self),
                    };
                }

                let inner = self.settings_inner_rect();
                let show_primary = crate::ui::settings_show_primary_action(self);
                let (apply, close) =
                    crate::ui::settings_button_rects(inner, self.settings.section, show_primary);
                let mut buttons = vec![(close, super::modal::ModalAction::Close)];
                if let Some(apply) = apply {
                    buttons.insert(0, (apply, super::modal::ModalAction::Apply));
                }
                match super::modal::modal_action_from_buttons(mouse.column, mouse.row, &buttons) {
                    Some(super::modal::ModalAction::Apply) => apply_settings(self),
                    Some(super::modal::ModalAction::Close) => {
                        cancel_settings(self);
                        None
                    }
                    _ => {
                        let popup = self.settings_popup_rect();
                        let inside = popup.width > 0
                            && popup.height > 0
                            && mouse.column >= popup.x
                            && mouse.column < popup.x + popup.width
                            && mouse.row >= popup.y
                            && mouse.row < popup.y + popup.height;
                        if !inside {
                            cancel_settings(self);
                        }
                        None
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(DragState {
                    target: DragTarget::SettingsThemeScrollbar { grab_row_offset },
                }) = &self.drag
                {
                    if let Some(offset_from_bottom) =
                        self.settings_theme_offset_for_drag_row(mouse.row, *grab_row_offset)
                    {
                        set_settings_theme_offset_from_bottom(self, offset_from_bottom);
                    }
                }
                None
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.drag.as_ref().is_some_and(|drag| {
                    matches!(drag.target, DragTarget::SettingsThemeScrollbar { .. })
                }) {
                    self.drag = None;
                }
                None
            }
            MouseEventKind::Moved => {
                if let Some(idx) = self.settings_list_index_at(mouse.column, mouse.row) {
                    self.settings.list.select(idx);
                    if self.settings.section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                }
                None
            }
            MouseEventKind::ScrollUp if self.settings.section == SettingsSection::Theme => {
                self.settings.scroll = self
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_WHEEL_SCROLL_ROWS as usize);
                None
            }
            MouseEventKind::ScrollDown if self.settings.section == SettingsSection::Theme => {
                self.settings.scroll = self
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_WHEEL_SCROLL_ROWS as usize)
                    .min(settings_theme_max_scroll(self));
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

    use super::super::{app_for_mouse_test, mouse, state_with_workspaces};
    use super::*;

    #[test]
    fn settings_cancel_restores_previewed_theme_from_other_sections() {
        let mut state = state_with_workspaces(&["test"]);
        let original_palette = state.palette.clone();
        let original_theme = state.theme_name.clone();

        open_settings(&mut state);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_ne!(state.theme_name, original_theme);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(
            state.settings.section,
            crate::app::state::SettingsSection::Layout
        );

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.theme_name, original_theme);
        assert_eq!(state.palette.accent, original_palette.accent);
        assert_eq!(state.palette.panel_bg, original_palette.panel_bg);
    }

    #[test]
    fn group_accent_settings_apply_returns_group_accent_action() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_settings(&mut state, group_idx);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveGroupAccent {
                group_idx,
                accent: Some(TerminalAccent::Blue),
            })
        );
        assert_eq!(state.settings.group_settings_target, None);
    }

    #[test]
    fn group_accent_settings_default_keeps_group_on_global_accent() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_settings(&mut state, group_idx);
        assert_eq!(state.settings.list.selected, 0);
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveGroupAccent {
                group_idx,
                accent: None,
            })
        );
    }

    #[test]
    fn group_settings_inherit_previews_global_theme_accent() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());
        state.global_palette = crate::app::state::Palette::dracula();
        state.palette = state.global_palette.clone();
        state.active_group = group_idx;
        assert!(state.set_group_accent(group_idx, Some(TerminalAccent::Blue)));
        assert_ne!(
            state.group_accent_color(group_idx),
            state.global_palette.accent
        );

        open_group_settings(&mut state, group_idx);
        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );

        assert_eq!(state.palette.accent, state.global_palette.accent);
    }

    #[test]
    fn group_accent_hover_does_not_move_checkmark() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());
        assert!(state.set_group_accent(group_idx, Some(TerminalAccent::Blue)));

        open_group_settings(&mut state, group_idx);
        assert_eq!(state.settings.list.selected, 1);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );

        let rows = rows_for_section(&state, SettingsSection::Theme).expect("theme rows");
        let checked = rows
            .iter()
            .find_map(|row| match row {
                crate::settings_rows::SettingsListRow::Choice {
                    label,
                    checked: true,
                    ..
                } => Some(label.as_ref().to_string()),
                _ => None,
            })
            .expect("checked accent");
        assert_eq!(state.settings.list.selected, 2);
        assert_eq!(checked, TerminalAccent::Blue.as_str());
    }

    #[test]
    fn group_settings_switches_between_appearance_and_general() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_settings(&mut state, group_idx);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );

        assert_eq!(state.settings.section, SettingsSection::GroupGeneral);
        assert_eq!(state.settings.group_settings_target, Some(group_idx));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );

        assert_eq!(state.settings.section, SettingsSection::Theme);
        assert_eq!(state.settings.group_settings_target, Some(group_idx));
    }

    #[test]
    fn group_general_settings_edits_name_inline_and_opens_delete_action() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_settings(&mut state, group_idx);
        state.settings.section = SettingsSection::GroupGeneral;
        state.settings.list.selected = 0;
        for ch in [' ', 'A'] {
            assert_eq!(
                update_settings_state(
                    &mut state,
                    KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()),
                ),
                None
            );
        }
        assert_eq!(state.settings.pending_group_name.as_deref(), Some("Side A"));
        let rename_action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(
            rename_action,
            Some(SettingsAction::SaveGroupName {
                group_idx,
                name: "Side A".to_string(),
            })
        );
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.settings.group_settings_target, None);

        open_group_settings(&mut state, group_idx);
        state.settings.section = SettingsSection::GroupGeneral;
        state.settings.list.selected = 1;
        let delete_action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(delete_action, Some(SettingsAction::DeleteGroup(group_idx)));
        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.settings.group_settings_target, None);
    }

    #[test]
    fn group_settings_uses_full_settings_modal() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());
        state.view.terminal_area = Rect::new(0, 0, 100, 40);

        open_group_settings(&mut state, group_idx);
        let group_rect = state.settings_popup_rect();

        open_settings(&mut state);
        let settings_rect = state.settings_popup_rect();

        assert_eq!(group_rect, settings_rect);
    }

    #[test]
    fn group_accent_apply_uses_full_screen_geometry_with_right_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.view.right_sidebar_rect = Rect::new(106, 0, 34, 20);
        let group_idx = app.state.create_group("Side".to_string());

        open_group_settings(&mut app.state, group_idx);
        app.state.settings.list.selected = 1;
        let inner = app.state.settings_inner_rect();
        let (apply, _) = crate::ui::settings_button_rects(
            inner,
            crate::app::state::SettingsSection::Theme,
            true,
        );
        let apply = apply.expect("group accent apply button");
        app.handle_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            apply.x,
            apply.y,
        ));

        assert_eq!(
            app.state.groups[group_idx].accent,
            Some(TerminalAccent::Blue)
        );
    }

    #[test]
    fn global_theme_settings_apply_returns_pending_settings() {
        let mut state = state_with_workspaces(&["test"]);

        open_settings(&mut state);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveSettings {
                light: "catppuccin-latte".to_string(),
                dark: "catppuccin".to_string(),
                mode: ThemeMode::Light,
                terminal_light_accent: TerminalAccent::Blue,
                terminal_dark_accent: TerminalAccent::Blue,
                sound_enabled: false,
                toast_delivery: ToastDelivery::Off,
                confirm_close: true,
                prompt_new_tab_name: true,
                new_terminal_cwd: NewTerminalCwdConfig::Follow,
                mouse_scroll_lines: crate::config::DEFAULT_MOUSE_SCROLL_LINES,
                sidebar_width: 26,
                sidebar_min_width: 18,
                sidebar_max_width: 36,
                worktree_directory: None,
                agent_border_labels: false,
            })
        );
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn terminal_theme_accent_selection_is_saved() {
        let mut state = state_with_workspaces(&["test"]);
        state.global_theme_mode = ThemeMode::System;
        state.global_light_theme_name = "system".to_string();
        state.global_dark_theme_name = "system".to_string();
        state.global_terminal_light_accent = TerminalAccent::Blue;
        state.global_terminal_dark_accent = TerminalAccent::Red;

        open_settings(&mut state);
        for _ in 0..3 {
            update_settings_state(
                &mut state,
                KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
            );
        }
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert!(matches!(
            action,
            Some(SettingsAction::SaveSettings {
                light,
                dark,
                mode: ThemeMode::System,
                terminal_light_accent: TerminalAccent::Magenta,
                terminal_dark_accent: TerminalAccent::Red,
                ..
            }) if light == "system" && dark == "system"
        ));
    }

    #[test]
    fn dark_accent_rows_skip_heading_before_options() {
        let mut state = state_with_workspaces(&["test"]);
        state.global_theme_mode = ThemeMode::System;
        state.global_light_theme_name = "system".to_string();
        state.global_dark_theme_name = "system".to_string();
        open_settings(&mut state);

        let dark_heading_row = 6 + TerminalAccent::ALL.len();
        assert_eq!(
            theme_selection_for_visual_row(&state, dark_heading_row),
            None
        );

        let dark_blue_row = dark_heading_row + 1;
        let selected =
            theme_selection_for_visual_row(&state, dark_blue_row).expect("dark blue row");
        assert_eq!(
            theme_settings_choices(&state).get(selected),
            Some(&ThemeSettingsChoice::TerminalDarkAccent(
                TerminalAccent::Blue
            ))
        );
        assert_eq!(
            theme_visual_row_for_selection(&state, selected),
            dark_blue_row
        );

        let dark_red_selected = theme_settings_choices(&state)
            .iter()
            .position(|choice| {
                *choice == ThemeSettingsChoice::TerminalDarkAccent(TerminalAccent::Red)
            })
            .expect("dark red option");
        let dark_red_row = theme_visual_row_for_selection(&state, dark_red_selected);
        assert_eq!(dark_red_row + 1, theme_visual_len(&state));
        assert_eq!(
            theme_selection_for_visual_row(&state, dark_red_row),
            Some(dark_red_selected)
        );
    }

    #[test]
    fn space_selects_theme_without_closing_settings() {
        let mut state = state_with_workspaces(&["test"]);

        open_settings(&mut state);
        state.settings.list.selected = 2 + ThemeMode::ALL.len() + 1;
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );

        assert_eq!(action, None);
        assert_eq!(
            state.settings.pending_light_theme_name.as_deref(),
            theme_names_for_appearance(ThemeAppearance::Light)
                .get(1)
                .copied()
        );
        assert_eq!(state.mode, Mode::Settings);
    }

    #[test]
    fn space_switches_between_system_source_and_custom_themes() {
        let mut state = state_with_workspaces(&["test"]);

        open_settings(&mut state);
        state.settings.pending_theme_mode = Some(ThemeMode::System);
        state.settings.pending_light_theme_name = Some("system".to_string());
        state.settings.pending_dark_theme_name = Some("system".to_string());
        state.settings.list.selected = 1;

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );

        assert_eq!(action, None);
        assert_eq!(state.settings.pending_theme_mode, Some(ThemeMode::System));
        assert_eq!(
            state.settings.pending_light_theme_name.as_deref(),
            Some(crate::app::state::DEFAULT_LIGHT_THEME_NAME)
        );
        assert_eq!(
            state.settings.pending_dark_theme_name.as_deref(),
            Some(crate::app::state::DEFAULT_DARK_THEME_NAME)
        );

        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );

        assert_eq!(state.settings.pending_theme_mode, Some(ThemeMode::System));
        assert_eq!(
            state.settings.pending_light_theme_name.as_deref(),
            Some("system")
        );
        assert_eq!(
            state.settings.pending_dark_theme_name.as_deref(),
            Some("system")
        );
    }
    #[test]
    fn settings_sound_space_updates_pending_value_without_closing() {
        let mut state = state_with_workspaces(&["test"]);
        open_settings(&mut state);
        state.settings.section = crate::app::state::SettingsSection::Sound;
        state.settings.list.selected = 0;

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );

        assert_eq!(action, None);
        assert_eq!(state.settings.pending_sound_enabled, Some(true));
        assert!(!state.sound.enabled);
        assert_eq!(state.mode, Mode::Settings);
    }

    #[test]
    fn settings_layout_cycles_sidebar_widths() {
        let mut state = state_with_workspaces(&["test"]);
        state.default_sidebar_width = 26;
        state.sidebar_min_width = 18;
        state.sidebar_max_width = 36;
        open_settings_at(&mut state, SettingsSection::Layout);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.settings.pending_sidebar_width, Some(28));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.settings.pending_sidebar_min_width, Some(20));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.settings.pending_sidebar_max_width, Some(38));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.mode, Mode::EditWorktreeDirectory);
        assert_eq!(state.name_input, "/tmp/hako-worktrees");
        state.mode = Mode::Settings;

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert!(matches!(
            action,
            Some(SettingsAction::SaveSettings {
                sidebar_width: 28,
                sidebar_min_width: 20,
                sidebar_max_width: 38,
                ..
            })
        ));
    }
    #[test]
    fn settings_behavior_toggles_close_prompt_and_border_labels() {
        let mut state = state_with_workspaces(&["test"]);
        state.confirm_close = true;
        state.prompt_new_tab_name = true;
        state.show_agent_labels_on_pane_borders = false;
        state.new_terminal_cwd = NewTerminalCwdConfig::Follow;
        state.mouse_scroll_lines = 3;
        open_settings_at(&mut state, SettingsSection::PaneLabels);

        assert_eq!(
            update_settings_state(
                &mut state,
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
            ),
            None
        );
        assert_eq!(state.settings.pending_confirm_close, Some(false));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.settings.pending_prompt_new_tab_name, Some(false));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(
            state.settings.pending_new_terminal_cwd,
            Some(NewTerminalCwdConfig::Home)
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.settings.pending_mouse_scroll_lines, Some(5));

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(state.settings.pending_agent_border_labels, Some(true));

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert!(matches!(
            action,
            Some(SettingsAction::SaveSettings {
                confirm_close: false,
                prompt_new_tab_name: false,
                new_terminal_cwd: NewTerminalCwdConfig::Home,
                mouse_scroll_lines: 5,
                agent_border_labels: true,
                ..
            })
        ));
        assert_eq!(state.mode, Mode::Terminal);
    }

    #[test]
    fn settings_experiments_toggles_input_source() {
        let mut state = state_with_workspaces(&["test"]);
        state.switch_ascii_input_source_in_prefix = false;
        open_settings_at(&mut state, SettingsSection::Experiments);

        let input_source_action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            input_source_action,
            Some(SettingsAction::SaveSwitchAsciiInputSourceInPrefix(true))
        );
        assert_eq!(state.mode, Mode::Settings);
    }

    #[test]
    fn settings_tab_cycle_places_experiments_last() {
        let mut state = state_with_workspaces(&["test"]);
        open_settings_at(&mut state, SettingsSection::PaneLabels);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Integrations);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Experiments);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Theme);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Experiments);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Integrations);
    }

    #[test]
    fn integrations_enter_does_nothing_when_nothing_needs_install() {
        let mut state = state_with_workspaces(&["test"]);
        open_settings_at(&mut state, SettingsSection::Integrations);

        let enter_action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(enter_action, None);

        let space_action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
        );
        assert_eq!(space_action, None);
    }

    #[test]
    fn integrations_arrows_select_rows() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![
            integration_recommendation_for(
                crate::api::schema::IntegrationTarget::Pi,
                crate::integration::IntegrationStatusKind::Current,
                true,
            ),
            integration_recommendation_for(
                crate::api::schema::IntegrationTarget::Omp,
                crate::integration::IntegrationStatusKind::NotInstalled,
                true,
            ),
        ];
        open_settings_at(&mut state, SettingsSection::Integrations);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 1);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);
    }

    #[test]
    fn settings_arrows_wrap_top_and_bottom_in_each_section() {
        let mut state = state_with_workspaces(&["test"]);
        open_settings_at(&mut state, SettingsSection::Theme);
        state.settings.pending_theme_mode = Some(ThemeMode::Light);
        state.settings.pending_light_theme_name =
            Some(crate::app::state::DEFAULT_LIGHT_THEME_NAME.to_string());
        state.settings.pending_dark_theme_name =
            Some(crate::app::state::DEFAULT_DARK_THEME_NAME.to_string());
        state.settings.list.selected = 0;
        let theme_len = theme_choice_len(&state);
        assert!(theme_len > 2);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, theme_len - 1);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);

        open_settings_at(&mut state, SettingsSection::Layout);
        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 3);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);

        open_settings_at(&mut state, SettingsSection::Sound);
        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 1);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);

        open_settings_at(&mut state, SettingsSection::Toast);
        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 3);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);

        open_settings_at(&mut state, SettingsSection::PaneLabels);
        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 4);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);

        state.integration_recommendations = vec![
            integration_recommendation_for(
                crate::api::schema::IntegrationTarget::Pi,
                crate::integration::IntegrationStatusKind::Current,
                true,
            ),
            integration_recommendation_for(
                crate::api::schema::IntegrationTarget::Omp,
                crate::integration::IntegrationStatusKind::NotInstalled,
                true,
            ),
        ];
        open_settings_at(&mut state, SettingsSection::Integrations);
        state.settings.list.selected = 0;
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 1);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.list.selected, 0);
    }

    #[test]
    fn settings_tabs_wrap_between_theme_and_experiments() {
        let mut state = state_with_workspaces(&["test"]);
        open_settings_at(&mut state, SettingsSection::Theme);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Left, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Experiments);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Right, KeyModifiers::empty()),
        );
        assert_eq!(state.settings.section, SettingsSection::Theme);
    }

    #[test]
    fn integrations_enter_installs_selected_available_row() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![integration_recommendation_for(
            crate::api::schema::IntegrationTarget::Omp,
            crate::integration::IntegrationStatusKind::NotInstalled,
            true,
        )];
        open_settings_at(&mut state, SettingsSection::Integrations);

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::InstallIntegration(
                crate::api::schema::IntegrationTarget::Omp
            ))
        );
    }

    #[test]
    fn integrations_enter_uninstalls_selected_current_row() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![integration_recommendation_for(
            crate::api::schema::IntegrationTarget::Omp,
            crate::integration::IntegrationStatusKind::Current,
            true,
        )];
        open_settings_at(&mut state, SettingsSection::Integrations);

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::UninstallIntegration(
                crate::api::schema::IntegrationTarget::Omp
            ))
        );
    }

    #[test]
    fn integrations_mouse_click_selects_row() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations = vec![
            integration_recommendation_for(
                crate::api::schema::IntegrationTarget::Pi,
                crate::integration::IntegrationStatusKind::Current,
                true,
            ),
            integration_recommendation_for(
                crate::api::schema::IntegrationTarget::Omp,
                crate::integration::IntegrationStatusKind::NotInstalled,
                true,
            ),
        ];
        open_settings_at(&mut app.state, SettingsSection::Integrations);
        let area = app.state.settings_content_rect();

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            area.x + 2,
            area.y + 5,
        ));

        assert_eq!(app.state.settings.list.selected, 1);
    }

    #[test]
    fn settings_hover_moves_cursor_without_selecting_pending_value() {
        let mut app = app_for_mouse_test();
        open_settings(&mut app.state);
        app.state.settings.list.select(0);
        app.state.settings.pending_theme_mode = Some(ThemeMode::System);

        let list_area = settings_theme_list_rect(app.state.settings_content_rect());
        app.handle_mouse(mouse(
            MouseEventKind::Moved,
            list_area.x + 2,
            list_area.y + 2,
        ));

        assert_eq!(app.state.settings.list.selected, 1);
        assert_eq!(
            app.state.settings.pending_theme_mode,
            Some(ThemeMode::System)
        );
    }

    #[test]
    fn settings_theme_hover_ignores_scrollbar_column() {
        let mut app = app_for_mouse_test();
        open_settings(&mut app.state);
        app.state.global_theme_mode = ThemeMode::System;
        app.state.settings.pending_theme_mode = Some(ThemeMode::System);
        app.state.settings.list.select(0);
        let list_area = settings_theme_list_rect(app.state.settings_content_rect());
        let track = settings_theme_viewport(&app.state)
            .scroll_area(list_area)
            .track
            .expect("scrollbar track");

        app.handle_mouse(mouse(MouseEventKind::Moved, track.x, track.y + 1));

        assert_eq!(app.state.settings.list.selected, 0);
    }

    #[test]
    fn settings_theme_wheel_scrolls_without_changing_selection() {
        let mut app = app_for_mouse_test();
        open_settings(&mut app.state);
        app.state.settings.list.select(0);

        let area = app.state.settings_content_rect();
        let expected_scroll = settings_theme_max_scroll(&app.state)
            .min(super::super::MODAL_WHEEL_SCROLL_ROWS as usize);
        app.handle_mouse(mouse(MouseEventKind::ScrollDown, area.x + 2, area.y + 2));

        assert_eq!(app.state.settings.list.selected, 0);
        assert_eq!(app.state.settings.scroll, expected_scroll);

        app.handle_mouse(mouse(MouseEventKind::ScrollUp, area.x + 2, area.y + 2));

        assert_eq!(app.state.settings.list.selected, 0);
        assert_eq!(app.state.settings.scroll, 0);
    }

    #[test]
    fn settings_theme_system_scrollbar_drag_reveals_dark_options() {
        let mut app = app_for_mouse_test();
        app.state.global_theme_mode = ThemeMode::System;
        open_settings(&mut app.state);

        let list_area = settings_theme_list_rect(app.state.settings_content_rect());
        let track = settings_theme_viewport(&app.state)
            .scroll_area(list_area)
            .track
            .expect("scrollbar track");
        assert_eq!(app.state.settings.scroll, 0);

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            track.x,
            track.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            track.x,
            track.y + track.height.saturating_sub(1),
        ));

        assert!(
            app.state.settings.scroll > theme_names_for_appearance(ThemeAppearance::Light).len()
        );
    }

    #[test]
    fn settings_mouse_click_toggles_behavior_options() {
        let mut app = app_for_mouse_test();
        app.state.confirm_close = true;
        app.state.prompt_new_tab_name = true;
        app.state.show_agent_labels_on_pane_borders = false;
        app.state.new_terminal_cwd = NewTerminalCwdConfig::Follow;
        app.state.mouse_scroll_lines = 3;
        app.state.view.terminal_area = Rect::new(26, 0, 100, 30);
        open_settings_at(&mut app.state, SettingsSection::PaneLabels);

        let area = app.state.settings_content_rect();
        assert_eq!(
            app.state.handle_settings_mouse(mouse(
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                area.x + 2,
                area.y + 1,
            )),
            None
        );
        assert_eq!(app.state.settings.pending_confirm_close, Some(false));
        assert_eq!(app.state.settings.list.selected, 0);

        app.state.handle_settings_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            area.x + 2,
            area.y + 3,
        ));
        assert_eq!(app.state.settings.pending_prompt_new_tab_name, Some(false));
        assert_eq!(app.state.settings.list.selected, 1);

        app.state.handle_settings_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            area.x + 2,
            area.y + 7,
        ));
        assert_eq!(
            app.state.settings.pending_new_terminal_cwd,
            Some(NewTerminalCwdConfig::Home)
        );
        assert_eq!(app.state.settings.list.selected, 2);
        app.state.handle_settings_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            area.x + 2,
            area.y + 9,
        ));
        assert_eq!(app.state.settings.pending_mouse_scroll_lines, Some(5));
        assert_eq!(app.state.settings.list.selected, 3);

        app.state.handle_settings_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            area.x + 2,
            area.y + 11,
        ));
        assert_eq!(app.state.settings.pending_agent_border_labels, Some(true));
        assert_eq!(app.state.settings.list.selected, 4);
    }

    #[test]
    fn settings_mouse_ignores_section_headers_and_separators() {
        let mut app = app_for_mouse_test();
        open_settings_at(&mut app.state, SettingsSection::PaneLabels);

        let area = app.state.settings_content_rect();
        app.state.settings.list.selected = 4;
        assert_eq!(
            app.state
                .handle_settings_mouse(mouse(MouseEventKind::Moved, area.x + 2, area.y)),
            None
        );
        assert_eq!(app.state.settings.list.selected, 4);

        app.state
            .handle_settings_mouse(mouse(MouseEventKind::Moved, area.x + 2, area.y + 5));
        assert_eq!(app.state.settings.list.selected, 4);

        app.state
            .handle_settings_mouse(mouse(MouseEventKind::Moved, area.x + 2, area.y + 7));
        assert_eq!(app.state.settings.list.selected, 2);
    }

    #[test]
    fn settings_mouse_click_toggles_experiment_rows() {
        let mut app = app_for_mouse_test();
        app.state.switch_ascii_input_source_in_prefix = false;
        app.state.view.sidebar_rect = Rect::new(0, 0, 26, 30);
        app.state.view.terminal_area = Rect::new(26, 0, 80, 30);
        open_settings_at(&mut app.state, SettingsSection::Experiments);

        let area = app.state.settings_content_rect();
        let input_source_action = app.state.handle_settings_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            area.x + 2,
            area.y + 1,
        ));

        assert_eq!(
            input_source_action,
            Some(SettingsAction::SaveSwitchAsciiInputSourceInPrefix(true))
        );
        assert_eq!(app.state.settings.list.selected, 0);
    }

    #[test]
    fn integration_update_badge_only_tracks_outdated_recommendations() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![integration_recommendation(
            crate::integration::IntegrationStatusKind::NotInstalled,
            true,
        )];
        assert!(!state.integration_updates_available());

        state.integration_recommendations = vec![integration_recommendation(
            crate::integration::IntegrationStatusKind::NotInstalled,
            false,
        )];
        assert!(!state.integration_updates_available());

        state.integration_recommendations = vec![integration_recommendation(
            crate::integration::IntegrationStatusKind::Current,
            true,
        )];
        assert!(!state.integration_updates_available());

        state.integration_recommendations = vec![integration_recommendation(
            crate::integration::IntegrationStatusKind::Outdated,
            true,
        )];
        assert!(state.integration_updates_available());
    }

    #[test]
    fn settings_tab_hit_area_includes_integration_update_badge() {
        let mut state = state_with_workspaces(&["test"]);
        state.integration_recommendations = vec![integration_recommendation(
            crate::integration::IntegrationStatusKind::Outdated,
            true,
        )];
        open_settings(&mut state);

        let inner = state.settings_inner_rect();
        let tab_y = inner.y + 1;
        let integrations_idx = SettingsSection::ALL
            .iter()
            .position(|section| *section == SettingsSection::Integrations)
            .expect("integrations section should be present");
        let integrations_x = inner.x
            + SettingsSection::ALL[..integrations_idx]
                .iter()
                .map(|section| {
                    let badge_width = if state.settings_section_has_badge(*section) {
                        2
                    } else {
                        0
                    };
                    section.label().len() as u16 + 3 + badge_width
                })
                .sum::<u16>();
        let dotted_width = SettingsSection::Integrations.label().len() as u16 + 4;

        assert_eq!(
            state.settings_tab_at(integrations_x + dotted_width - 1, tab_y),
            Some(SettingsSection::Integrations)
        );
    }

    fn integration_recommendation_for(
        target: crate::api::schema::IntegrationTarget,
        state: crate::integration::IntegrationStatusKind,
        available: bool,
    ) -> crate::integration::IntegrationRecommendation {
        crate::integration::IntegrationRecommendation {
            target,
            label: crate::integration::integration_target_label(target),
            command: crate::integration::integration_target_command(target),
            available,
            path: std::path::PathBuf::from("/tmp/hako-test-integration"),
            state,
        }
    }
    fn integration_recommendation(
        state: crate::integration::IntegrationStatusKind,
        available: bool,
    ) -> crate::integration::IntegrationRecommendation {
        integration_recommendation_for(
            crate::api::schema::IntegrationTarget::Claude,
            state,
            available,
        )
    }
}
