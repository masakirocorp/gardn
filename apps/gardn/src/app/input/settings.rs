use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{
        state::{
            normalize_theme_name, theme_names_for_appearance, AppState, DragState, DragTarget,
            SettingsSection, SettingsSidebarSelection, SettingsState, THEME_NAMES,
        },
        view_state::ClientViewState,
        App, Mode,
    },
    config::{
        AgentPanelScopeConfig, CommandsConfig, ContextBarVisibilityConfig, NewTerminalCwdConfig,
        PaneBorderAgentInfoConfig, RightClickPassthroughModifierConfig, ShellModeConfig,
        SidebarArrangementConfig, SidebarInitialStateConfig, StatusIndicatorStyle, TerminalAccent,
        ThemeMode, ToastClipboardPosition, ToastDelivery, ToastGardnPosition,
        MAX_TOAST_DELAY_SECONDS,
    },
    settings_rows::{
        connection_editor_open as settings_connection_editor_open, next_option_index, option_count,
        option_hit_for_visual_row, previous_option_index, selected_visual_row, visual_row_count,
        AdvancedRowId, BehaviorRowId, CommandAction, CommandField, CommandRowId, ConnectionField,
        ConnectionRowId, NotificationRowId, SettingsListRow, SettingsRowHit,
        GROUP_DEFAULTS_DIRECTORY, GROUP_DEFAULTS_HOST, GROUP_GENERAL_DELETE, GROUP_GENERAL_ICON,
        GROUP_GENERAL_NAME, GROUP_GITHUB_ORGANIZATION, WORKSPACE_GENERAL_DIRECTORY,
        WORKSPACE_GENERAL_HOST, WORKSPACE_GENERAL_NAME, WORKSPACE_GITHUB_AUTOMATIC,
        WORKSPACE_GITHUB_GROUP, WORKSPACE_GITHUB_REPOSITORIES, WORKSPACE_GITHUB_SELECTED,
    },
    terminal_theme::ThemeAppearance,
};

use super::ScrollbarClickTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
// The shared `Save` verb is semantic: these actions persist settings.
#[allow(clippy::enum_variant_names)]
pub(crate) enum SettingsAction {
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
        show_counters: bool,
        pane_borders: bool,
        pane_scrollbars: bool,
        pane_gaps: bool,
        hide_tab_bar_when_single_tab: bool,
        copy_on_select: bool,
        prompt_new_workspace_name: bool,
        right_click_passthrough_modifier: RightClickPassthroughModifierConfig,
        new_terminal_cwd: NewTerminalCwdConfig,
        mouse_scroll_lines: usize,
        browser_command: String,
        review_command: String,
        editor_command: String,
        sidebar_width: u16,
        sidebar_min_width: u16,
        sidebar_max_width: u16,
        sidebar_arrangement: SidebarArrangementConfig,
        context_bar_visibility: ContextBarVisibilityConfig,
        sidebar_initial_state: SidebarInitialStateConfig,
        sidebar_initial_agent_scope: AgentPanelScopeConfig,
        pane_border_agent_info: PaneBorderAgentInfoConfig,
        status_indicators: StatusIndicatorStyle,
    },
    SaveSwitchAsciiInputSourceInPrefix(bool),
    SaveKittyGraphics(bool),
    SaveResumeAgentsOnRestore(bool),
    SaveWindowTitle(String),
    SaveHeadlessSize {
        cols: u16,
        rows: u16,
    },
    SaveDefaultShell(String),
    SaveShellMode(ShellModeConfig),
    SaveVersionCheck(bool),
    SaveManifestCheck(bool),
    SaveToastDelay(u64),
    SaveToastGardnPosition(ToastGardnPosition),
    SaveClipboardToastEnabled(bool),
    SaveClipboardToastPosition(ToastClipboardPosition),
    SaveGroupAccent {
        group_idx: usize,
        accent: Option<TerminalAccent>,
    },
    SaveGroupName {
        group_idx: usize,
        name: String,
    },
    SaveGroupIcon {
        group_idx: usize,
        icon: String,
    },
    SaveGroupGithubOrganization {
        group_idx: usize,
        organization: Option<crate::app::state::GithubOrganization>,
    },
    SaveGroupDefaultLocation {
        group_idx: usize,
        default_location: Option<crate::execution_host::ResourceLocation>,
    },
    SaveWorkspaceName {
        ws_idx: usize,
        name: String,
    },
    SaveWorkspaceDefaultLocation {
        ws_idx: usize,
        location: crate::execution_host::ResourceLocation,
    },
    SaveWorkspaceGithubScope {
        ws_idx: usize,
        scope: crate::github::GithubRepositoryScope,
    },
    DeleteGroup(usize),
    CycleIntegrationHost,
    InstallIntegration(crate::api::schema::IntegrationTarget),
    UninstallIntegration(crate::api::schema::IntegrationTarget),
    SaveAgentProfile(crate::agent_profiles::UserAgentProfileConfig),
    DeleteAgentProfile(String),
    SaveSshConnectionProfile(crate::persist::ssh_profiles::SshConnectionProfile),
    PreviewSshConnectionRetirement(String),
    ConfirmSshConnectionRetirement {
        profile_id: String,
        preview: crate::app::state::ConnectionRetirementPreview,
    },
    ConfirmLocalConnectionForget {
        profile_id: String,
        plan: crate::execution_host::connection_retirement::ConnectionRetirementPlan,
    },
    TestSshConnection {
        profile_id: String,
    },
    ConnectSshConnection {
        profile_id: String,
    },
    LaunchSshWorkspace {
        profile_id: String,
    },
    DisconnectSshConnection {
        profile_id: String,
    },
    RequestForgetRemoteTermination {
        terminal_id: crate::terminal::TerminalId,
    },
    ConfirmForgetRemoteTermination {
        terminal_id: crate::terminal::TerminalId,
    },
}

impl App {
    pub(crate) fn apply_settings_action(&mut self, action: SettingsAction) {
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
                show_counters,
                pane_borders,
                pane_scrollbars,
                pane_gaps,
                hide_tab_bar_when_single_tab,
                copy_on_select,
                prompt_new_workspace_name,
                right_click_passthrough_modifier,
                new_terminal_cwd,
                mouse_scroll_lines,
                browser_command,
                review_command,
                editor_command,
                sidebar_width,
                sidebar_min_width,
                sidebar_max_width,
                sidebar_arrangement,
                context_bar_visibility,
                sidebar_initial_state,
                sidebar_initial_agent_scope,
                pane_border_agent_info,
                status_indicators,
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
                self.save_show_counters(show_counters);
                self.save_pane_appearance(
                    pane_borders,
                    pane_scrollbars,
                    pane_gaps,
                    hide_tab_bar_when_single_tab,
                );
                self.save_behavior_selection(
                    copy_on_select,
                    prompt_new_workspace_name,
                    right_click_passthrough_modifier,
                );
                self.save_new_terminal_cwd(&new_terminal_cwd);
                self.save_mouse_scroll_lines(mouse_scroll_lines);
                self.save_commands(&browser_command, &review_command, &editor_command);
                self.save_sidebar_widths(sidebar_width, sidebar_min_width, sidebar_max_width);
                self.save_sidebar_arrangement(sidebar_arrangement);
                self.save_context_bar_visibility(context_bar_visibility);
                self.save_sidebar_initial_view(sidebar_initial_state, sidebar_initial_agent_scope);
                self.save_toast_delivery(toast_delivery);
                self.save_pane_border_agent_info(pane_border_agent_info);
                self.save_status_indicators(status_indicators);
            }
            SettingsAction::SaveResumeAgentsOnRestore(enabled) => {
                self.save_resume_agents_on_restore(enabled);
            }
            SettingsAction::SaveWindowTitle(template) => {
                self.save_window_title(&template);
            }
            SettingsAction::SaveHeadlessSize { cols, rows } => {
                self.save_headless_size(cols, rows);
            }
            SettingsAction::SaveDefaultShell(shell) => {
                self.save_default_shell(&shell);
            }
            SettingsAction::SaveShellMode(mode) => {
                self.save_shell_mode(mode);
            }
            SettingsAction::SaveVersionCheck(enabled) => {
                self.save_version_check(enabled);
            }
            SettingsAction::SaveManifestCheck(enabled) => {
                self.save_manifest_check(enabled);
            }
            SettingsAction::SaveToastDelay(seconds) => {
                self.save_toast_delay(seconds);
            }
            SettingsAction::SaveToastGardnPosition(position) => {
                self.save_toast_gardn_position(position);
            }
            SettingsAction::SaveClipboardToastEnabled(enabled) => {
                self.save_clipboard_toast_enabled(enabled);
            }
            SettingsAction::SaveClipboardToastPosition(position) => {
                self.save_clipboard_toast_position(position);
            }
            SettingsAction::SaveWorkspaceName { ws_idx, name } => {
                self.state.rename_workspace(ws_idx, name);
            }
            SettingsAction::SaveWorkspaceDefaultLocation { ws_idx, location } => {
                self.state.set_workspace_default_location(ws_idx, location);
            }
            SettingsAction::SaveWorkspaceGithubScope { ws_idx, scope } => {
                self.state.set_workspace_github_scope(ws_idx, scope);
            }

            SettingsAction::SaveGroupName { group_idx, name } => {
                self.state.rename_group(group_idx, name);
            }
            SettingsAction::SaveGroupIcon { group_idx, icon } => {
                self.state.set_group_icon(group_idx, icon);
            }
            SettingsAction::SaveGroupGithubOrganization {
                group_idx,
                organization,
            } => {
                self.state
                    .set_group_github_organization(group_idx, organization);
            }
            SettingsAction::SaveGroupAccent { group_idx, accent } => {
                self.state.set_group_accent(group_idx, accent);
                self.query_host_terminal_theme();
            }

            SettingsAction::SaveGroupDefaultLocation {
                group_idx,
                default_location,
            } => {
                self.state
                    .set_group_default_location(group_idx, default_location);
            }
            SettingsAction::DeleteGroup(group_idx) => {
                self.default_client_view.confirm_delete_group = Some(group_idx);
                self.default_client_view.mode = Mode::ConfirmDeleteGroup;
            }
            SettingsAction::SaveSwitchAsciiInputSourceInPrefix(enabled) => {
                self.save_switch_ascii_input_source_in_prefix(enabled)
            }
            SettingsAction::SaveKittyGraphics(enabled) => self.save_kitty_graphics(enabled),
            SettingsAction::CycleIntegrationHost => self.cycle_integration_host(),
            SettingsAction::InstallIntegration(target) => self.apply_integration_operation(
                crate::integration::host::HostIntegrationOperation::EnsureCurrent { target },
            ),
            SettingsAction::UninstallIntegration(target) => self.apply_integration_operation(
                crate::integration::host::HostIntegrationOperation::UninstallOwned { target },
            ),
            SettingsAction::SaveAgentProfile(profile) => {
                if self.save_agent_profile(profile) {
                    close_agent_profile_editor_for_view(&self.state, &mut self.default_client_view);
                }
            }
            SettingsAction::DeleteAgentProfile(profile_id) => {
                self.delete_agent_profile(&profile_id);
                if self
                    .default_client_view
                    .settings
                    .pending_agent_profile_id
                    .as_deref()
                    == Some(profile_id.as_str())
                {
                    close_agent_profile_editor_for_view(&self.state, &mut self.default_client_view);
                }
            }
            SettingsAction::SaveSshConnectionProfile(profile) => {
                self.save_ssh_connection_profile(profile)
            }
            SettingsAction::PreviewSshConnectionRetirement(profile_id) => {
                let owner = crate::execution_host::auth::AuthenticationOwner::new(
                    self.default_client_view.id(),
                );
                self.preview_connection_retirement_for(owner, profile_id);
            }
            SettingsAction::ConfirmSshConnectionRetirement {
                profile_id,
                preview,
            } => {
                let owner = crate::execution_host::auth::AuthenticationOwner::new(
                    self.default_client_view.id(),
                );
                self.retire_connection_for(owner, profile_id, preview);
            }
            SettingsAction::ConfirmLocalConnectionForget { profile_id, plan } => {
                let owner = crate::execution_host::auth::AuthenticationOwner::new(
                    self.default_client_view.id(),
                );
                self.forget_connection_locally_for(owner, profile_id, plan);
            }
            SettingsAction::TestSshConnection { profile_id } => {
                let owner = crate::execution_host::auth::AuthenticationOwner::new(
                    self.default_client_view.id(),
                );
                self.request_connection_for(
                    owner,
                    &profile_id,
                    crate::execution_host::HostConnectionAction::Test,
                );
            }
            SettingsAction::ConnectSshConnection { profile_id } => {
                let owner = crate::execution_host::auth::AuthenticationOwner::new(
                    self.default_client_view.id(),
                );
                self.state.queue_ssh_connection_request(
                    profile_id,
                    crate::execution_host::HostConnectionAction::Connect,
                    owner,
                )
            }
            SettingsAction::LaunchSshWorkspace { profile_id } => {
                let Some(profile) = self
                    .state
                    .ssh_connection_profiles
                    .iter()
                    .find(|profile| profile.id() == profile_id)
                    .cloned()
                else {
                    return;
                };
                let path = profile.suggested_directory().cloned().unwrap_or_default();
                let location =
                    crate::execution_host::ResourceLocation::new(profile.execution_host_id(), path);
                let group_id = self
                    .default_client_view
                    .active_group_id(&self.state)
                    .to_string();
                match self.begin_remote_workspace(location, true, group_id, None, Vec::new()) {
                    Ok(_) => close_settings_for_view(&mut self.default_client_view),
                    Err(error) => {
                        self.state.toast = Some(crate::app::state::ToastNotification {
                            kind: crate::app::state::ToastKind::NeedsAttention,
                            title: "Could Not Open Workspace".to_string(),
                            context: error,
                            position: None,
                            target: None,
                        });
                    }
                }
            }
            SettingsAction::DisconnectSshConnection { profile_id } => {
                let owner = crate::execution_host::auth::AuthenticationOwner::new(
                    self.default_client_view.id(),
                );
                self.state.queue_ssh_connection_request(
                    profile_id,
                    crate::execution_host::HostConnectionAction::Disconnect,
                    owner,
                )
            }
            SettingsAction::RequestForgetRemoteTermination { terminal_id } => {
                if let Some(editor) = self.default_client_view.settings.connection_editor.as_mut() {
                    editor.pending_forget_remote_terminal = Some(terminal_id);
                }
            }
            SettingsAction::ConfirmForgetRemoteTermination { terminal_id } => {
                match self.forget_remote_termination(&terminal_id) {
                    Ok(true) => {
                        if let Some(editor) =
                            self.default_client_view.settings.connection_editor.as_mut()
                        {
                            editor.pending_forget_remote_terminal = None;
                        }
                    }
                    Ok(false) => {}
                    Err(err) => {
                        self.state.toast = Some(crate::app::state::ToastNotification {
                            kind: crate::app::state::ToastKind::NeedsAttention,
                            title: "Remote Termination Not Forgotten".to_string(),
                            context: err.to_string(),
                            position: None,
                            target: None,
                        });
                    }
                }
            }
        }
    }

    /// Persist a connection profile after shared reference guards pass.
    pub(super) fn save_ssh_connection_profile(
        &mut self,
        profile: crate::persist::ssh_profiles::SshConnectionProfile,
    ) {
        if let Err(err) = self.commit_ssh_connection_profile(profile) {
            self.state.toast = Some(crate::app::state::ToastNotification {
                kind: crate::app::state::ToastKind::NeedsAttention,
                title: "Connection Profile Not Saved".to_string(),
                context: err.to_string(),
                position: None,
                target: None,
            });
        }
    }

    fn cycle_integration_host(&mut self) {
        self.with_default_client_view(|app, view| app.cycle_integration_host_for_view(view));
    }

    pub(crate) fn cycle_integration_host_for_view(&mut self, view: &mut ClientViewState) {
        let current = view.settings.integration_host_profile_id.as_deref();
        let next_index = current
            .and_then(|profile_id| {
                self.state
                    .ssh_connection_profiles
                    .iter()
                    .position(|profile| profile.id() == profile_id)
            })
            .map_or(0, |index| index + 1);
        let next = if next_index < self.state.ssh_connection_profiles.len() {
            Some(
                self.state.ssh_connection_profiles[next_index]
                    .id()
                    .to_string(),
            )
        } else {
            None
        };
        view.settings.integration_host_profile_id = next.clone();
        view.settings.list.selected = 0;
        if next.is_some() {
            self.apply_integration_operation_for_view(
                view,
                crate::integration::host::HostIntegrationOperation::Inspect,
            );
        }
    }

    pub(super) fn apply_integration_operation(
        &mut self,
        operation: crate::integration::host::HostIntegrationOperation,
    ) {
        self.with_default_client_view(|app, view| {
            app.apply_integration_operation_for_view(view, operation);
        });
    }

    pub(crate) fn apply_integration_operation_for_view(
        &mut self,
        view: &mut ClientViewState,
        operation: crate::integration::host::HostIntegrationOperation,
    ) {
        let selected_profile_id = view.settings.integration_host_profile_id.clone();
        let host_id = crate::app::integration_host::resolve(&self.state, &view.settings)
            .host_id()
            .cloned();
        if selected_profile_id.is_some() && host_id.is_none() {
            view.settings.integration_host_profile_id = None;
        }
        let Some(host_id) = host_id else {
            match operation {
                crate::integration::host::HostIntegrationOperation::Inspect => {
                    self.refresh_integration_recommendations();
                }
                crate::integration::host::HostIntegrationOperation::EnsureCurrent { target } => {
                    self.install_integration(target);
                }
                crate::integration::host::HostIntegrationOperation::UninstallOwned { target } => {
                    self.uninstall_integration(target);
                }
            }
            return;
        };

        self.state
            .host_integration_install_messages
            .remove(&host_id);
        let request =
            crate::integration::host::request_for_catalog(operation, &self.state.agent_profiles);
        let outcome = self.execution_hosts.as_mut().map_or_else(
            || Err("execution host manager is unavailable".to_string()),
            |hosts| {
                hosts
                    .request_agent_integrations(host_id.clone(), request)
                    .map_err(|error| error.to_string())
            },
        );
        match outcome {
            Ok(request_id) => {
                self.state
                    .host_integration_request_ids
                    .insert(host_id.clone(), request_id);
                self.state.host_integration_observations.insert(
                    host_id,
                    crate::integration::host::HostIntegrationObservation::Pending,
                );
            }
            Err(message) => {
                self.state.host_integration_request_ids.remove(&host_id);
                self.state.host_integration_observations.insert(
                    host_id,
                    crate::integration::host::HostIntegrationObservation::Failed(message),
                );
            }
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

struct SettingsInput<'a> {
    shared: &'a mut AppState,
    client: &'a mut ClientViewState,
}

impl std::ops::Deref for SettingsInput<'_> {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        self.shared
    }
}

impl std::ops::DerefMut for SettingsInput<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.shared
    }
}

fn rows_for_section(
    state: &SettingsInput<'_>,
    section: SettingsSection,
) -> Option<Vec<SettingsListRow>> {
    if state.client.settings.section != section {
        return None;
    }
    crate::settings_rows::rows_for_section_for_view(state.shared, state.client)
}

fn pending_uses_system_theme_source(state: &SettingsInput<'_>) -> bool {
    pending_theme_mode(state) == ThemeMode::System
        && normalize_theme_name(&pending_light_theme_name(state)) == "system"
        && normalize_theme_name(&pending_dark_theme_name(state)) == "system"
}

fn pending_shows_terminal_accent(state: &SettingsInput<'_>) -> bool {
    state.client.settings.group_settings_target.is_none() && pending_uses_system_theme_source(state)
}

fn next_toast_delivery(delivery: ToastDelivery) -> ToastDelivery {
    match delivery {
        ToastDelivery::Off => ToastDelivery::Gardn,
        ToastDelivery::Gardn => ToastDelivery::Terminal,
        ToastDelivery::Terminal => ToastDelivery::System,
        ToastDelivery::System => ToastDelivery::Off,
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
fn theme_settings_choices(state: &SettingsInput<'_>) -> Vec<ThemeSettingsChoice> {
    if state.client.settings.group_settings_target.is_some() {
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

fn theme_choice_len(state: &SettingsInput<'_>) -> usize {
    theme_settings_choices(state).len()
}

fn theme_rows(state: &SettingsInput<'_>) -> Vec<crate::settings_rows::SettingsListRow> {
    rows_for_section(state, SettingsSection::Theme).unwrap_or_default()
}

fn theme_visual_len(state: &SettingsInput<'_>) -> usize {
    visual_row_count(&theme_rows(state))
}

fn theme_visual_row_for_selection(state: &SettingsInput<'_>, selected: usize) -> usize {
    selected_visual_row(&theme_rows(state), selected).unwrap_or(0)
}

fn settings_section_choice_len(state: &SettingsInput<'_>, section: SettingsSection) -> usize {
    rows_for_section(state, section)
        .map(|rows| option_count(&rows))
        .unwrap_or_else(|| match section {
            SettingsSection::Integrations => state.integration_recommendations.len(),
            SettingsSection::Theme => theme_choice_len(state),
            SettingsSection::Layout
            | SettingsSection::Sound
            | SettingsSection::Toast
            | SettingsSection::PaneLabels
            | SettingsSection::Commands
            | SettingsSection::Experiments
            | SettingsSection::Agents
            | SettingsSection::Connections
            | SettingsSection::GroupProfiles
            | SettingsSection::GroupGeneral
            | SettingsSection::GroupDefaults
            | SettingsSection::GroupGithub
            | SettingsSection::WorkspaceGeneral
            | SettingsSection::WorkspaceGithub
            | SettingsSection::About => 0,
        })
}

fn settings_section_scroll_len(state: &SettingsInput<'_>, section: SettingsSection) -> usize {
    rows_for_section(state, section)
        .map(|rows| visual_row_count(&rows))
        .unwrap_or_else(|| match section {
            SettingsSection::Theme => theme_visual_len(state),
            SettingsSection::Integrations => state.integration_recommendations.len(),
            _ => 0,
        })
}

fn settings_section_list_rect(state: &SettingsInput<'_>, section: SettingsSection) -> Rect {
    let body_area = crate::ui::settings_section_list_rect(state.settings_content_rect());
    if section == SettingsSection::Integrations {
        let [list_area, _] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas::<2>(body_area);
        list_area
    } else {
        body_area
    }
}

fn settings_section_list_geometry(
    state: &SettingsInput<'_>,
    section: SettingsSection,
) -> crate::ui::ModalListGeometry {
    crate::ui::ModalListGeometry::new(
        settings_section_list_rect(state, section),
        settings_section_scroll_len(state, section),
        state.client.settings.scroll,
    )
}

fn settings_section_viewport(
    state: &SettingsInput<'_>,
    section: SettingsSection,
) -> crate::ui::ModalListViewport {
    settings_section_list_geometry(state, section).viewport
}

fn settings_theme_viewport(state: &SettingsInput<'_>) -> crate::ui::ModalListViewport {
    settings_section_viewport(state, SettingsSection::Theme)
}

fn settings_section_max_scroll(state: &SettingsInput<'_>, section: SettingsSection) -> usize {
    settings_section_viewport(state, section).max_scroll()
}

fn settings_theme_max_scroll(state: &SettingsInput<'_>) -> usize {
    settings_section_max_scroll(state, SettingsSection::Theme)
}

fn ensure_settings_selection_visible(state: &mut SettingsInput<'_>) {
    let section = state.client.settings.section;
    let viewport = settings_section_viewport(state, section);
    state.client.settings.scroll = viewport.scroll();

    let selected_row = rows_for_section(state, section)
        .and_then(|rows| selected_visual_row(&rows, state.client.settings.list.selected))
        .unwrap_or_else(|| {
            if section == SettingsSection::Theme {
                theme_visual_row_for_selection(state, state.client.settings.list.selected)
            } else {
                state.client.settings.list.selected
            }
        });
    state.client.settings.scroll = viewport.ensure_visible(selected_row, None);
}

fn set_settings_theme_offset_from_bottom(state: &mut SettingsInput<'_>, offset_from_bottom: usize) {
    state.client.settings.scroll =
        settings_theme_viewport(state).scroll_from_offset_from_bottom(offset_from_bottom);
}

fn group_accent_choice_at_cursor(state: &SettingsInput<'_>) -> Option<TerminalAccent> {
    theme_settings_choices(state)
        .get(state.client.settings.list.selected)
        .and_then(|choice| match choice {
            ThemeSettingsChoice::GroupAccent(accent) => Some(*accent),
            _ => None,
        })
        .unwrap_or(None)
}

fn group_accent_selection_index(state: &SettingsInput<'_>) -> usize {
    let accent = state
        .client
        .settings
        .pending_group_accent_choice
        .unwrap_or_else(|| {
            state
                .client
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

fn checked_group_accent_choice(state: &SettingsInput<'_>) -> Option<TerminalAccent> {
    state
        .client
        .settings
        .pending_group_accent_choice
        .unwrap_or_else(|| group_accent_choice_at_cursor(state))
}

fn pending_group_name(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_group_name
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .map(|group| group.name.clone())
        })
        .unwrap_or_default()
}

fn set_pending_group_name(state: &mut SettingsInput<'_>, name: String) {
    state.client.settings.pending_group_name = Some(name);
}

fn pending_group_icon(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_group_icon
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .map(|group| group.icon.clone())
        })
        .unwrap_or_else(|| crate::app::state::DEFAULT_GROUP_ICON.to_string())
}

fn toggle_group_icon_picker(state: &mut SettingsInput<'_>) {
    state.client.settings.group_icon_picker_open = !state.client.settings.group_icon_picker_open;
}

fn group_settings_icon_picker_hit(
    state: &SettingsInput<'_>,
    col: u16,
    row: u16,
) -> Option<&'static str> {
    if !state.client.settings.group_icon_picker_open
        || state.client.settings.section != SettingsSection::GroupGeneral
    {
        return None;
    }
    let list = settings_section_list_geometry(state, SettingsSection::GroupGeneral);
    let rows = rows_for_section(state, SettingsSection::GroupGeneral)?;
    let icon_visual = selected_visual_row(&rows, GROUP_GENERAL_ICON)?;
    let picker_start = icon_visual + 1;
    let y = list.rect.y + picker_start.saturating_sub(state.client.settings.scroll) as u16;
    let origin = Rect::new(
        list.rect.x + 2,
        y,
        list.rect.width.saturating_sub(2).min(24),
        crate::ui::group_icon_picker_row_count(),
    );
    crate::ui::group_icon_picker_rects_at(origin)
        .into_iter()
        .find(|(rect, _)| {
            col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
        })
        .map(|(_, icon)| icon)
}

fn pending_group_default_directory(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_group_default_directory
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .and_then(|group| group.default_location.as_ref())
                .map(|location| location.path.as_path().display().to_string())
        })
        .unwrap_or_default()
}

fn pending_group_default_host(state: &SettingsInput<'_>) -> crate::execution_host::ExecutionHostId {
    state
        .client
        .settings
        .pending_group_default_execution_host_id
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .and_then(|group| group.default_location.as_ref())
                .map(|location| location.execution_host_id.clone())
        })
        .unwrap_or_else(crate::execution_host::ExecutionHostId::local)
}

fn pending_workspace_default_host(
    state: &SettingsInput<'_>,
) -> crate::execution_host::ExecutionHostId {
    state
        .client
        .settings
        .pending_workspace_default_execution_host_id
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .workspace_settings_target
                .and_then(|ws_idx| state.workspaces.get(ws_idx))
                .map(|workspace| workspace.default_location.execution_host_id.clone())
        })
        .unwrap_or_else(crate::execution_host::ExecutionHostId::local)
}

fn cycle_default_host(state: &mut SettingsInput<'_>, workspace: bool) {
    if workspace {
        let current = pending_workspace_default_host(state);
        let mut choices = vec![crate::execution_host::ExecutionHostId::local()];
        choices.extend(
            state
                .ssh_connection_profiles
                .iter()
                .map(|profile| profile.execution_host_id()),
        );
        let next = choices
            .iter()
            .position(|host| host == &current)
            .map_or(0, |index| (index + 1) % choices.len());
        state
            .client
            .settings
            .pending_workspace_default_execution_host_id = choices.get(next).cloned();
        return;
    }
    let mut host = pending_group_default_host(state);
    let mut directory = pending_group_default_directory(state);
    super::apply_group_host_cycle(&state.ssh_connection_profiles, &mut host, &mut directory);
    state
        .client
        .settings
        .pending_group_default_execution_host_id = Some(host);
    state.client.settings.pending_group_default_directory = Some(directory);
}

fn set_pending_group_default_directory(state: &mut SettingsInput<'_>, default_directory: String) {
    state.client.settings.pending_group_default_directory = Some(default_directory);
}

fn pending_group_github_organization(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_group_github_organization
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .group_settings_target
                .and_then(|group_idx| state.groups.get(group_idx))
                .and_then(|group| group.github_organization.as_ref())
                .map(|organization| organization.as_str().to_string())
        })
        .unwrap_or_default()
}

fn pending_group_field(
    state: &SettingsInput<'_>,
    section: SettingsSection,
    selected: usize,
) -> Option<String> {
    match section {
        SettingsSection::GroupGeneral if selected == GROUP_GENERAL_NAME => {
            Some(pending_group_name(state))
        }
        SettingsSection::GroupDefaults if selected == GROUP_DEFAULTS_DIRECTORY => {
            Some(pending_group_default_directory(state))
        }
        SettingsSection::GroupGithub if selected == GROUP_GITHUB_ORGANIZATION => {
            Some(pending_group_github_organization(state))
        }
        _ => None,
    }
}

fn set_pending_group_field(
    state: &mut SettingsInput<'_>,
    section: SettingsSection,
    selected: usize,
    value: String,
) {
    match section {
        SettingsSection::GroupGeneral if selected == GROUP_GENERAL_NAME => {
            set_pending_group_name(state, value)
        }
        SettingsSection::GroupDefaults if selected == GROUP_DEFAULTS_DIRECTORY => {
            set_pending_group_default_directory(state, value)
        }
        SettingsSection::GroupGithub if selected == GROUP_GITHUB_ORGANIZATION => {
            state.client.settings.pending_group_github_organization = Some(value)
        }
        _ => {}
    }
}

fn delete_pending_group_field_word(
    state: &mut SettingsInput<'_>,
    section: SettingsSection,
    selected: usize,
) {
    let Some(mut value) = pending_group_field(state, section, selected) else {
        return;
    };
    while value.chars().last().is_some_and(char::is_whitespace) {
        value.pop();
    }
    while value.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
        value.pop();
    }
    set_pending_group_field(state, section, selected, value);
}

fn edit_pending_group_field(state: &mut SettingsInput<'_>, key: KeyEvent) -> bool {
    let Some(selected) = state.client.settings.focused_input else {
        return false;
    };
    let section = state.client.settings.section;
    state.client.settings.list.select(selected);
    if !settings_row_accepts_text_input(state, selected) {
        return false;
    }

    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            set_pending_group_field(state, section, selected, String::new());
            true
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            set_pending_group_field(state, section, selected, String::new());
            true
        }
        KeyCode::Backspace
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            delete_pending_group_field_word(state, section, selected);
            true
        }
        KeyCode::Char('h' | 'w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_pending_group_field_word(state, section, selected);
            true
        }
        KeyCode::Backspace => {
            let mut value = pending_group_field(state, section, selected).unwrap_or_default();
            value.pop();
            set_pending_group_field(state, section, selected, value);
            true
        }
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            let mut value = pending_group_field(state, section, selected).unwrap_or_default();
            value.push(c);
            set_pending_group_field(state, section, selected, value);
            true
        }
        _ => false,
    }
}

fn pending_workspace_name(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_workspace_name
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .workspace_settings_target
                .and_then(|ws_idx| state.workspaces.get(ws_idx))
                .map(|workspace| workspace.display_name())
        })
        .unwrap_or_default()
}

fn pending_workspace_default_cwd(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_workspace_default_cwd
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .workspace_settings_target
                .and_then(|ws_idx| state.workspaces.get(ws_idx))
                .map(|workspace| {
                    workspace
                        .default_location
                        .path
                        .as_path()
                        .display()
                        .to_string()
                })
        })
        .unwrap_or_default()
}
fn pending_workspace_github_scope(
    state: &SettingsInput<'_>,
) -> crate::github::GithubRepositoryScope {
    state
        .client
        .settings
        .pending_workspace_github_scope
        .clone()
        .or_else(|| {
            state
                .client
                .settings
                .workspace_settings_target
                .and_then(|ws_idx| state.workspaces.get(ws_idx))
                .map(|workspace| workspace.github_scope.clone())
        })
        .unwrap_or_default()
}

fn pending_workspace_github_repositories(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_workspace_github_repositories
        .clone()
        .or_else(|| match pending_workspace_github_scope(state) {
            crate::github::GithubRepositoryScope::Selected(repositories) => Some(
                repositories
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            crate::github::GithubRepositoryScope::Automatic
            | crate::github::GithubRepositoryScope::GroupOrganization => Some(String::new()),
        })
        .unwrap_or_default()
}

fn set_pending_workspace_field(
    state: &mut SettingsInput<'_>,
    section: SettingsSection,
    selected: usize,
    value: String,
) {
    match section {
        SettingsSection::WorkspaceGeneral if selected == WORKSPACE_GENERAL_NAME => {
            state.client.settings.pending_workspace_name = Some(value)
        }
        SettingsSection::WorkspaceGeneral if selected == WORKSPACE_GENERAL_DIRECTORY => {
            state.client.settings.pending_workspace_default_cwd = Some(value)
        }
        SettingsSection::WorkspaceGithub if selected == WORKSPACE_GITHUB_REPOSITORIES => {
            state.client.settings.pending_workspace_github_repositories = Some(value)
        }
        _ => {}
    }
}

fn pending_workspace_field(
    state: &SettingsInput<'_>,
    section: SettingsSection,
    selected: usize,
) -> Option<String> {
    match section {
        SettingsSection::WorkspaceGeneral if selected == WORKSPACE_GENERAL_NAME => {
            Some(pending_workspace_name(state))
        }
        SettingsSection::WorkspaceGeneral if selected == WORKSPACE_GENERAL_DIRECTORY => {
            Some(pending_workspace_default_cwd(state))
        }
        SettingsSection::WorkspaceGithub if selected == WORKSPACE_GITHUB_REPOSITORIES => {
            Some(pending_workspace_github_repositories(state))
        }
        _ => None,
    }
}

fn edit_pending_workspace_field(state: &mut SettingsInput<'_>, key: KeyEvent) -> bool {
    let Some(selected) = state.client.settings.focused_input else {
        return false;
    };
    let section = state.client.settings.section;
    state.client.settings.list.select(selected);
    if !settings_row_accepts_text_input(state, selected) {
        return false;
    }
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            set_pending_workspace_field(state, section, selected, String::new());
            true
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            set_pending_workspace_field(state, section, selected, String::new());
            true
        }
        KeyCode::Backspace => {
            let Some(mut value) = pending_workspace_field(state, section, selected) else {
                return false;
            };
            value.pop();
            set_pending_workspace_field(state, section, selected, value);
            true
        }
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            let Some(mut value) = pending_workspace_field(state, section, selected) else {
                return false;
            };
            value.push(c);
            set_pending_workspace_field(state, section, selected, value);
            true
        }
        _ => false,
    }
}

const AGENT_PROFILE_NAME_INDEX: usize = 0;
const AGENT_PROFILE_KIND_START_INDEX: usize = 1;
fn agent_profile_command_index(state: &SettingsInput<'_>) -> usize {
    AGENT_PROFILE_KIND_START_INDEX + state.agent_profile_kind_choices().count()
}

fn agent_profile_enabled_index(state: &SettingsInput<'_>) -> usize {
    agent_profile_command_index(state) + 1
}

fn agent_profile_save_index(state: &SettingsInput<'_>) -> usize {
    agent_profile_command_index(state) + 2
}

fn agent_profile_discard_index(state: &SettingsInput<'_>) -> usize {
    agent_profile_command_index(state) + 3
}

fn agent_profile_delete_index(state: &SettingsInput<'_>) -> usize {
    agent_profile_command_index(state) + 4
}

fn pending_agent_profile_name(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_agent_profile_name
        .clone()
        .unwrap_or_default()
}

fn pending_agent_profile_command(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_agent_profile_command
        .clone()
        .unwrap_or_default()
}

fn set_pending_agent_profile_field(state: &mut SettingsInput<'_>, selected: usize, value: String) {
    match selected {
        AGENT_PROFILE_NAME_INDEX => state.client.settings.pending_agent_profile_name = Some(value),
        index if index == agent_profile_command_index(state) => {
            state.client.settings.pending_agent_profile_command = Some(value)
        }
        _ => {}
    }
}

fn delete_pending_agent_profile_word(state: &mut SettingsInput<'_>, selected: usize) {
    let mut value = if selected == AGENT_PROFILE_NAME_INDEX {
        pending_agent_profile_name(state)
    } else if selected == agent_profile_command_index(state) {
        pending_agent_profile_command(state)
    } else {
        return;
    };
    while value.chars().last().is_some_and(char::is_whitespace) {
        value.pop();
    }
    while value.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
        value.pop();
    }
    set_pending_agent_profile_field(state, selected, value);
}

fn edit_pending_agent_profile_text(state: &mut SettingsInput<'_>, key: KeyEvent) -> bool {
    let Some(selected) = state.client.settings.focused_input else {
        return false;
    };
    state.client.settings.list.select(selected);
    if selected != AGENT_PROFILE_NAME_INDEX && selected != agent_profile_command_index(state) {
        return false;
    }
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            set_pending_agent_profile_field(state, selected, String::new());
            true
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            set_pending_agent_profile_field(state, selected, String::new());
            true
        }
        KeyCode::Backspace
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            delete_pending_agent_profile_word(state, selected);
            true
        }
        KeyCode::Char('h' | 'w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_pending_agent_profile_word(state, selected);
            true
        }
        KeyCode::Backspace => {
            let mut value = if selected == AGENT_PROFILE_NAME_INDEX {
                pending_agent_profile_name(state)
            } else {
                pending_agent_profile_command(state)
            };
            value.pop();
            set_pending_agent_profile_field(state, selected, value);
            true
        }
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            let mut value = if selected == AGENT_PROFILE_NAME_INDEX {
                pending_agent_profile_name(state)
            } else {
                pending_agent_profile_command(state)
            };
            value.push(c);
            set_pending_agent_profile_field(state, selected, value);
            true
        }
        _ => false,
    }
}

fn agent_kind_for_settings_index(
    state: &SettingsInput<'_>,
    index: usize,
) -> Option<crate::agent_profiles::AgentKind> {
    let kinds = state.agent_profile_kind_choices().collect::<Vec<_>>();
    let end = AGENT_PROFILE_KIND_START_INDEX + kinds.len();
    (AGENT_PROFILE_KIND_START_INDEX..end)
        .contains(&index)
        .then(|| kinds[index - AGENT_PROFILE_KIND_START_INDEX])
}

fn agent_profile_editor_open(state: &SettingsInput<'_>) -> bool {
    state.client.settings.pending_agent_profile_id.is_some()
        || state.client.settings.pending_agent_profile_name.is_some()
        || state
            .client
            .settings
            .pending_agent_profile_command
            .is_some()
}

fn browse_agent_profile_id_for_settings_index(
    state: &SettingsInput<'_>,
    selected: usize,
) -> Option<String> {
    if selected == 0 || agent_profile_editor_open(state) {
        return None;
    }
    state
        .agent_profiles
        .profiles()
        .iter()
        .filter(|profile| !profile.is_system())
        .nth(selected - 1)
        .map(|profile| profile.id.clone())
}

fn custom_profile_id_for_settings_index(
    state: &SettingsInput<'_>,
    selected: usize,
) -> Option<String> {
    let profile_id = browse_agent_profile_id_for_settings_index(state, selected)?;
    state
        .agent_profiles
        .get(&profile_id)
        .is_some_and(|profile| !profile.is_system())
        .then_some(profile_id)
}

fn open_blank_agent_profile_editor(state: &mut SettingsInput<'_>) {
    state.client.settings.pending_agent_profile_id = None;
    state.client.settings.pending_agent_profile_name = Some(String::new());
    let filtered_kind = state.client.settings.agent_profile_kind_filter;
    let kind = filtered_kind
        .filter(|kind| state.agent_profile_kind_available(*kind))
        .unwrap_or_else(|| state.default_agent_profile_kind_choice());
    state.client.settings.pending_agent_profile_kind = Some(kind);
    state.client.settings.pending_agent_profile_command = Some(String::new());
    state.client.settings.pending_agent_profile_enabled = Some(true);
    state.client.settings.list.select(AGENT_PROFILE_NAME_INDEX);
    state.client.settings.focused_input = Some(AGENT_PROFILE_NAME_INDEX);
    state.client.settings.scroll = 0;
}

fn reset_agent_profile_editor(
    settings: &mut SettingsState,
    default_kind: crate::agent_profiles::AgentKind,
) {
    settings.pending_agent_profile_id = None;
    settings.pending_agent_profile_name = None;
    settings.pending_agent_profile_kind = Some(default_kind);
    settings.pending_agent_profile_command = None;
    settings.pending_agent_profile_enabled = None;
    settings.list.selected = 0;
    settings.focused_input = None;
    settings.list.hide();
    settings.scroll = 0;
}

fn close_agent_profile_editor(state: &mut SettingsInput<'_>) {
    let default_kind = state.default_agent_profile_kind_choice();
    reset_agent_profile_editor(&mut state.client.settings, default_kind);
}

pub(crate) fn close_agent_profile_editor_for_view(state: &AppState, view: &mut ClientViewState) {
    reset_agent_profile_editor(
        &mut view.settings,
        state.default_agent_profile_kind_choice(),
    );
}

fn load_custom_agent_profile_editor(state: &mut SettingsInput<'_>, profile_id: &str) -> bool {
    let Some(profile) = state.agent_profiles.get(profile_id).cloned() else {
        return false;
    };
    if profile.is_system() {
        return false;
    }
    state.client.settings.pending_agent_profile_id = Some(profile.id.clone());
    state.client.settings.pending_agent_profile_name = Some(profile.name.clone());
    let kind = if state.agent_profile_kind_available(profile.kind) {
        profile.kind
    } else {
        crate::agent_profiles::AgentKind::Custom
    };
    state.client.settings.pending_agent_profile_kind = Some(kind);
    state.client.settings.pending_agent_profile_command = Some(profile.command.clone());
    state.client.settings.pending_agent_profile_enabled = Some(profile.enabled);
    state.client.settings.list.select(AGENT_PROFILE_NAME_INDEX);
    state.client.settings.focused_input = Some(AGENT_PROFILE_NAME_INDEX);
    true
}

fn slugify_agent_profile_id(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "custom-agent".to_string()
    } else {
        trimmed.to_string()
    }
}

fn next_custom_agent_profile_id(state: &SettingsInput<'_>, name: &str) -> String {
    let base = slugify_agent_profile_id(name);
    let mut candidate = base.clone();
    let mut suffix = 2;
    while state
        .agent_profiles
        .get(&format!("user:{candidate}"))
        .is_some()
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn save_pending_agent_profile(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    let name = pending_agent_profile_name(state).trim().to_string();
    let command = pending_agent_profile_command(state).trim().to_string();
    if name.is_empty() || command.is_empty() {
        return None;
    }
    let id = state
        .client
        .settings
        .pending_agent_profile_id
        .clone()
        .map(|id| id.trim_start_matches("user:").to_string())
        .unwrap_or_else(|| next_custom_agent_profile_id(state, &name));
    let kind = state
        .client
        .settings
        .pending_agent_profile_kind
        .filter(|kind| state.agent_profile_kind_available(*kind))
        .unwrap_or_else(|| state.default_agent_profile_kind_choice());
    let existing_id = format!("user:{id}");
    let env = state
        .agent_profiles
        .get(&existing_id)
        .map(|profile| profile.env.iter().cloned().collect())
        .unwrap_or_default();
    let enabled = state
        .client
        .settings
        .pending_agent_profile_enabled
        .unwrap_or(true);
    Some(SettingsAction::SaveAgentProfile(
        crate::agent_profiles::UserAgentProfileConfig {
            id,
            name,
            kind,
            command,
            env,
            enabled,
        },
    ))
}

fn selected_agent_profile_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let selected = state.client.settings.list.selected;
    if agent_profile_editor_open(state) {
        if let Some(kind) = agent_kind_for_settings_index(state, selected) {
            state.client.settings.pending_agent_profile_kind = Some(kind);
            return None;
        }
        return match selected {
            index if index == agent_profile_enabled_index(state) => {
                let enabled = state
                    .client
                    .settings
                    .pending_agent_profile_enabled
                    .unwrap_or(true);
                state.client.settings.pending_agent_profile_enabled = Some(!enabled);
                None
            }
            index if index == agent_profile_discard_index(state) => {
                close_agent_profile_editor(state);
                None
            }
            index if index == agent_profile_save_index(state) => save_pending_agent_profile(state),
            index if index == agent_profile_delete_index(state) => state
                .client
                .settings
                .pending_agent_profile_id
                .clone()
                .map(SettingsAction::DeleteAgentProfile),
            _ => None,
        };
    }

    if selected == 0 {
        open_blank_agent_profile_editor(state);
        return None;
    }
    let profile_id = browse_agent_profile_id_for_settings_index(state, selected)?;
    if load_custom_agent_profile_editor(state, &profile_id) {
        return None;
    }
    None
}

fn connection_editor_open(state: &SettingsInput<'_>) -> bool {
    settings_connection_editor_open(&state.client.settings)
}

fn connection_editor<'a>(
    state: &'a SettingsInput<'_>,
) -> Option<&'a crate::app::state::ConnectionEditorState> {
    state.client.settings.connection_editor.as_ref()
}

fn connection_editor_mut<'a>(
    state: &'a mut SettingsInput<'_>,
) -> Option<&'a mut crate::app::state::ConnectionEditorState> {
    state.client.settings.connection_editor.as_mut()
}

fn pending_connection_name(state: &SettingsInput<'_>) -> String {
    connection_editor(state)
        .map(|editor| editor.draft.name.clone())
        .unwrap_or_default()
}

fn pending_connection_target(state: &SettingsInput<'_>) -> String {
    connection_editor(state)
        .map(|editor| editor.draft.target.clone())
        .unwrap_or_default()
}

fn pending_connection_directory(state: &SettingsInput<'_>) -> String {
    connection_editor(state)
        .map(|editor| editor.draft.directory.clone())
        .unwrap_or_default()
}
fn pending_connection_accent(state: &SettingsInput<'_>) -> Option<TerminalAccent> {
    connection_editor(state).and_then(|editor| editor.draft.accent)
}

fn next_unused_connection_accent(state: &SettingsInput<'_>) -> TerminalAccent {
    TerminalAccent::ALL
        .iter()
        .copied()
        .find(|accent| {
            !state
                .ssh_connection_profiles
                .iter()
                .any(|profile| profile.accent() == Some(*accent))
        })
        .unwrap_or_else(|| {
            TerminalAccent::ALL[state.ssh_connection_profiles.len() % TerminalAccent::ALL.len()]
        })
}

fn cycle_pending_connection_accent(state: &mut SettingsInput<'_>) {
    let next = match pending_connection_accent(state) {
        None => Some(next_unused_connection_accent(state)),
        Some(current) => {
            let index = TerminalAccent::ALL
                .iter()
                .position(|accent| *accent == current)
                .unwrap_or(0);
            TerminalAccent::ALL.get(index + 1).copied()
        }
    };
    if let Some(editor) = connection_editor_mut(state) {
        editor.draft.accent = next;
    }
}

fn set_pending_connection_field(state: &mut SettingsInput<'_>, selected: usize, value: String) {
    let Some(editor) = connection_editor_mut(state) else {
        return;
    };
    match crate::settings_rows::ConnectionRowId::from_selection_index(selected) {
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Name,
        )) => editor.draft.name = value,
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Target,
        )) => editor.draft.target = value,
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Directory,
        )) => editor.draft.directory = value,
        _ => {}
    }
}

fn delete_pending_connection_word(state: &mut SettingsInput<'_>, selected: usize) {
    let mut value = match crate::settings_rows::ConnectionRowId::from_selection_index(selected) {
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Name,
        )) => pending_connection_name(state),
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Target,
        )) => pending_connection_target(state),
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Directory,
        )) => pending_connection_directory(state),
        _ => return,
    };
    while value.chars().last().is_some_and(char::is_whitespace) {
        value.pop();
    }
    while value.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
        value.pop();
    }
    set_pending_connection_field(state, selected, value);
}

fn edit_pending_connection_text(state: &mut SettingsInput<'_>, key: KeyEvent) -> bool {
    let Some(selected) = state.client.settings.focused_input else {
        return false;
    };
    if !matches!(
        crate::settings_rows::ConnectionRowId::from_selection_index(selected),
        Some(crate::settings_rows::ConnectionRowId::Field(
            crate::settings_rows::ConnectionField::Name
                | crate::settings_rows::ConnectionField::Target
                | crate::settings_rows::ConnectionField::Directory
        ))
    ) {
        return false;
    }
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            set_pending_connection_field(state, selected, String::new());
            true
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            set_pending_connection_field(state, selected, String::new());
            true
        }
        KeyCode::Backspace
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::META) =>
        {
            delete_pending_connection_word(state, selected);
            true
        }
        KeyCode::Char('h' | 'w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_pending_connection_word(state, selected);
            true
        }
        KeyCode::Backspace => {
            let mut value =
                match crate::settings_rows::ConnectionRowId::from_selection_index(selected) {
                    Some(crate::settings_rows::ConnectionRowId::Field(
                        crate::settings_rows::ConnectionField::Name,
                    )) => pending_connection_name(state),
                    Some(crate::settings_rows::ConnectionRowId::Field(
                        crate::settings_rows::ConnectionField::Target,
                    )) => pending_connection_target(state),
                    _ => pending_connection_directory(state),
                };
            value.pop();
            set_pending_connection_field(state, selected, value);
            true
        }
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            let mut value =
                match crate::settings_rows::ConnectionRowId::from_selection_index(selected) {
                    Some(crate::settings_rows::ConnectionRowId::Field(
                        crate::settings_rows::ConnectionField::Name,
                    )) => pending_connection_name(state),
                    Some(crate::settings_rows::ConnectionRowId::Field(
                        crate::settings_rows::ConnectionField::Target,
                    )) => pending_connection_target(state),
                    _ => pending_connection_directory(state),
                };
            value.push(c);
            set_pending_connection_field(state, selected, value);
            true
        }
        _ => false,
    }
}

fn browse_connection_profile_id_for_index(
    state: &SettingsInput<'_>,
    selected: usize,
) -> Option<String> {
    if selected == 0 || connection_editor_open(state) {
        return None;
    }
    state
        .ssh_connection_profiles
        .get(selected - 1)
        .map(|profile| profile.id().to_string())
}

fn open_blank_connection_editor(state: &mut SettingsInput<'_>) {
    let mut editor = crate::app::state::ConnectionEditorState::new_draft();
    editor.draft.accent = Some(next_unused_connection_accent(state));
    state.client.settings.connection_editor = Some(editor);
    let target_index = ConnectionRowId::Field(ConnectionField::Target).selection_index();
    state.client.settings.list.select(target_index);
    state.client.settings.focused_input = Some(target_index);
    state.client.settings.scroll = 0;
}

fn close_connection_editor(state: &mut SettingsInput<'_>) {
    state.client.settings.connection_editor = None;
    state.client.settings.list.selected = 0;
    state.client.settings.focused_input = None;
    clear_settings_selection(state);
    state.client.settings.scroll = 0;
}
fn back_from_connection_screen(state: &mut SettingsInput<'_>) {
    let persisted_draft = state
        .client
        .settings
        .connection_editor
        .as_ref()
        .filter(|editor| editor.is_editing())
        .and_then(|editor| editor.profile_id())
        .and_then(|profile_id| {
            state
                .ssh_connection_profiles
                .iter()
                .find(|profile| profile.id() == profile_id)
        })
        .map(|profile| crate::app::state::ConnectionDraft {
            name: profile.name().to_string(),
            target: profile.target().to_string(),
            directory: profile
                .suggested_directory()
                .map(ToString::to_string)
                .unwrap_or_default(),
            accent: profile.accent(),
        });
    if let (Some(editor), Some(draft)) = (
        state.client.settings.connection_editor.as_mut(),
        persisted_draft,
    ) {
        if editor.show_detail() {
            editor.draft = draft;
            let edit_index =
                ConnectionRowId::Action(crate::settings_rows::ConnectionAction::EditDetails)
                    .selection_index();
            state.client.settings.list.select(edit_index);
            state.client.settings.focused_input = None;
            state.client.settings.scroll = 0;
            return;
        }
    }
    close_connection_editor(state);
}

fn load_connection_profile_detail(state: &mut SettingsInput<'_>, profile_id: &str) -> bool {
    let Some(profile) = state
        .ssh_connection_profiles
        .iter()
        .find(|profile| profile.id() == profile_id)
    else {
        return false;
    };
    state.client.settings.connection_editor =
        Some(crate::app::state::ConnectionEditorState::detail_profile(
            profile.id(),
            profile.name(),
            profile.target(),
            profile
                .suggested_directory()
                .map(|directory| directory.to_string())
                .unwrap_or_default(),
            profile.accent(),
        ));
    let toggle_index =
        ConnectionRowId::Action(crate::settings_rows::ConnectionAction::Toggle).selection_index();
    state.client.settings.list.select(toggle_index);
    state.client.settings.focused_input = None;
    state.client.settings.scroll = 0;
    true
}

/// Readable, id-safe slug for a connection profile display name.
fn slugify_connection_profile_id(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    // Leave room for the `ssh:` prefix, generation suffix, and numeric id suffix.
    let capped: String = trimmed.chars().take(48).collect();
    let capped = capped.trim_matches('-');
    if capped.is_empty() {
        "connection".to_string()
    } else {
        capped.to_string()
    }
}

/// Stable collision-free profile id: readable slug plus a deterministic
/// numeric suffix probed against the current catalog.
fn next_connection_profile_id(state: &SettingsInput<'_>, name: &str) -> String {
    let base = slugify_connection_profile_id(name);
    let mut candidate = base.clone();
    let mut suffix = 2;
    while state
        .ssh_connection_profiles
        .iter()
        .any(|profile| profile.id() == candidate)
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn save_pending_connection_profile(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    let target = pending_connection_target(state).trim().to_string();
    if target.is_empty() {
        return None;
    }
    let name_input = pending_connection_name(state).trim().to_string();
    let name = if name_input.is_empty() {
        target.clone()
    } else {
        name_input
    };
    let directory_input = pending_connection_directory(state).trim().to_string();
    let suggested_directory = if directory_input.is_empty() {
        None
    } else {
        crate::execution_host::HostPath::new(directory_input).ok()
    };
    let accent = pending_connection_accent(state);
    let profile_id =
        connection_editor(state).and_then(|editor| editor.profile_id().map(str::to_string));
    let profile = if let Some(id) = profile_id {
        // Editing preserves the stable id; a target change bumps the binding generation.
        let mut profile = match state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == id)
            .cloned()
        {
            Some(profile) => profile,
            None => crate::persist::ssh_profiles::SshConnectionProfile::new_with_accent(
                id,
                name.clone(),
                target.clone(),
                suggested_directory.clone(),
                accent,
            )
            .ok()?,
        };
        profile.rename(name).ok()?;
        profile.set_suggested_directory(suggested_directory);
        profile.set_accent(accent);
        profile.set_target(target).ok()?;
        profile
    } else {
        let id = next_connection_profile_id(state, &name);
        crate::persist::ssh_profiles::SshConnectionProfile::new_with_accent(
            id,
            name,
            target,
            suggested_directory,
            accent,
        )
        .ok()?
    };
    close_connection_editor(state);
    Some(SettingsAction::SaveSshConnectionProfile(profile))
}

fn selected_connection_profile_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let selected = state.client.settings.list.selected;
    if connection_editor_open(state) {
        let row = crate::settings_rows::ConnectionRowId::from_selection_index(selected)?;
        return match row {
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::Discard,
            ) => {
                back_from_connection_screen(state);
                None
            }
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::Save,
            ) => save_pending_connection_profile(state),
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::EditDetails,
            ) => {
                let editor = connection_editor_mut(state)?;
                if editor.start_editing() {
                    let target_index =
                        ConnectionRowId::Field(ConnectionField::Target).selection_index();
                    state.client.settings.list.select(target_index);
                    state.client.settings.focused_input = Some(target_index);
                    state.client.settings.scroll = 0;
                }
                None
            }
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::Delete,
            ) => {
                let editor = connection_editor(state)?;
                let profile_id = editor.profile_id()?.to_string();
                match editor.connection_retirement.as_ref() {
                    None
                    | Some(crate::app::state::ConnectionRetirementState::InventoryPending)
                    | Some(crate::app::state::ConnectionRetirementState::Failed) => {
                        Some(SettingsAction::PreviewSshConnectionRetirement(profile_id))
                    }
                    Some(crate::app::state::ConnectionRetirementState::Review(preview)) => {
                        Some(SettingsAction::ConfirmSshConnectionRetirement {
                            profile_id,
                            preview: preview.clone(),
                        })
                    }
                    Some(crate::app::state::ConnectionRetirementState::LocalForgetRunning)
                    | Some(crate::app::state::ConnectionRetirementState::Running(_)) => None,
                }
            }
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::ForgetConnection,
            ) => {
                let editor = connection_editor(state)?;
                let profile_id = editor.profile_id()?.to_string();
                let plan = state
                    .ssh_connection_profiles
                    .iter()
                    .find(|profile| profile.id() == profile_id)
                    .and_then(|profile| {
                        crate::execution_host::connection_retirement::plan_connection_retirement(
                            &profile.execution_host_id(),
                        )
                        .ok()
                    })?;
                matches!(
                    editor.connection_retirement,
                    Some(crate::app::state::ConnectionRetirementState::Failed)
                )
                .then_some(SettingsAction::ConfirmLocalConnectionForget { profile_id, plan })
            }
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::Test,
            ) => connection_editor(state)
                .and_then(|editor| editor.profile_id().map(str::to_string))
                .map(|profile_id| SettingsAction::TestSshConnection { profile_id }),
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::Toggle,
            ) => {
                let profile_id = connection_editor(state)?.profile_id()?.to_string();
                let profile = state
                    .ssh_connection_profiles
                    .iter()
                    .find(|profile| profile.id() == profile_id)?;
                use crate::execution_host::ConnectionStatus;
                match state.ssh_connection_status(profile) {
                    ConnectionStatus::Disconnected | ConnectionStatus::AuthenticationRequired => {
                        Some(SettingsAction::ConnectSshConnection { profile_id })
                    }
                    ConnectionStatus::Connecting
                    | ConnectionStatus::Connected
                    | ConnectionStatus::Reconnecting { .. } => {
                        Some(SettingsAction::DisconnectSshConnection { profile_id })
                    }
                    ConnectionStatus::Disconnecting => None,
                }
            }
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::LaunchWorkspace,
            ) => {
                let profile_id = connection_editor(state)?.profile_id()?.to_string();
                let profile = state
                    .ssh_connection_profiles
                    .iter()
                    .find(|profile| profile.id() == profile_id)?;
                matches!(
                    state.ssh_connection_status(profile),
                    crate::execution_host::ConnectionStatus::Connected
                )
                .then_some(SettingsAction::LaunchSshWorkspace { profile_id })
            }
            crate::settings_rows::ConnectionRowId::Action(
                crate::settings_rows::ConnectionAction::ForgetTermination { offset },
            ) => {
                let profile_id = connection_editor(state)?.profile_id()?.to_string();
                let tombstone = state
                    .remote_termination_tombstones_for_profile(&profile_id)
                    .into_iter()
                    .nth(offset)?;
                if connection_editor(state)
                    .and_then(|editor| editor.pending_forget_remote_terminal.as_ref())
                    == Some(&tombstone.terminal_id)
                {
                    Some(SettingsAction::ConfirmForgetRemoteTermination {
                        terminal_id: tombstone.terminal_id,
                    })
                } else {
                    Some(SettingsAction::RequestForgetRemoteTermination {
                        terminal_id: tombstone.terminal_id,
                    })
                }
            }
            crate::settings_rows::ConnectionRowId::Field(ConnectionField::Color) => {
                cycle_pending_connection_accent(state);
                None
            }
            crate::settings_rows::ConnectionRowId::Field(_) => None,
        };
    }

    if selected == 0 {
        open_blank_connection_editor(state);
        return None;
    }
    let profile_id = browse_connection_profile_id_for_index(state, selected)?;
    if load_connection_profile_detail(state, &profile_id) {
        return None;
    }
    None
}

fn pending_light_theme_name(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_light_theme_name
        .clone()
        .unwrap_or_else(|| state.global_light_theme_name.clone())
}

fn pending_dark_theme_name(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_dark_theme_name
        .clone()
        .unwrap_or_else(|| state.global_dark_theme_name.clone())
}

fn pending_terminal_light_accent(state: &SettingsInput<'_>) -> TerminalAccent {
    state
        .client
        .settings
        .pending_terminal_light_accent
        .unwrap_or(state.global_terminal_light_accent)
}

fn pending_terminal_dark_accent(state: &SettingsInput<'_>) -> TerminalAccent {
    state
        .client
        .settings
        .pending_terminal_dark_accent
        .unwrap_or(state.global_terminal_dark_accent)
}

fn selected_theme_settings_choice(state: &SettingsInput<'_>) -> Option<ThemeSettingsChoice> {
    theme_settings_choices(state)
        .get(state.client.settings.list.selected)
        .copied()
}

fn pending_sound_enabled(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_sound_enabled
        .unwrap_or_else(|| state.sound_enabled())
}

fn pending_toast_delivery(state: &SettingsInput<'_>) -> ToastDelivery {
    state
        .client
        .settings
        .pending_toast_delivery
        .unwrap_or_else(|| state.toast_delivery())
}

fn pending_default_shell(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_default_shell
        .clone()
        .unwrap_or_else(|| state.default_shell.clone())
}

fn pending_shell_mode(state: &SettingsInput<'_>) -> ShellModeConfig {
    state
        .client
        .settings
        .pending_shell_mode
        .unwrap_or(state.shell_mode)
}

fn pending_version_check(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_version_check
        .unwrap_or(state.update_version_check)
}

fn pending_manifest_check(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_manifest_check
        .unwrap_or(state.update_manifest_check)
}

fn pending_toast_delay(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_toast_delay
        .clone()
        .unwrap_or_else(|| state.toast_config.delay_seconds.to_string())
}

fn pending_toast_gardn_position(state: &SettingsInput<'_>) -> ToastGardnPosition {
    state
        .client
        .settings
        .pending_toast_gardn_position
        .unwrap_or(state.toast_config.gardn.position)
}

fn pending_clipboard_toast_enabled(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_clipboard_toast_enabled
        .unwrap_or(state.toast_config.clipboard.enabled)
}

fn pending_clipboard_toast_position(state: &SettingsInput<'_>) -> ToastClipboardPosition {
    state
        .client
        .settings
        .pending_clipboard_toast_position
        .unwrap_or(state.toast_config.clipboard.position)
}

fn pending_confirm_close(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_confirm_close
        .unwrap_or_else(|| state.confirm_close_enabled())
}

fn pending_prompt_new_tab_name(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_prompt_new_tab_name
        .unwrap_or_else(|| state.prompt_new_tab_name_enabled())
}
fn pending_show_counters(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_show_counters
        .unwrap_or(state.show_counters)
}

fn pending_pane_borders(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_pane_borders
        .unwrap_or(state.pane_borders)
}

fn pending_pane_scrollbars(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_pane_scrollbars
        .unwrap_or(state.pane_scrollbars)
}

fn pending_pane_gaps(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_pane_gaps
        .unwrap_or(state.pane_gaps)
}

fn pending_hide_tab_bar_when_single_tab(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_hide_tab_bar_when_single_tab
        .unwrap_or(state.hide_tab_bar_when_single_tab)
}

fn pending_copy_on_select(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_copy_on_select
        .unwrap_or(state.copy_on_select)
}

fn pending_prompt_new_workspace_name(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_prompt_new_workspace_name
        .unwrap_or(state.prompt_new_workspace_name)
}

fn pending_right_click_passthrough_modifier(
    state: &SettingsInput<'_>,
) -> RightClickPassthroughModifierConfig {
    state
        .client
        .settings
        .pending_right_click_passthrough_modifier
        .unwrap_or_else(|| {
            RightClickPassthroughModifierConfig::from_modifiers(
                state.right_click_passthrough_modifiers,
            )
        })
}

fn pending_new_terminal_cwd(state: &SettingsInput<'_>) -> NewTerminalCwdConfig {
    state
        .client
        .settings
        .pending_new_terminal_cwd
        .clone()
        .unwrap_or_else(|| state.new_terminal_cwd.clone())
}

fn pending_mouse_scroll_lines(state: &SettingsInput<'_>) -> usize {
    state
        .client
        .settings
        .pending_mouse_scroll_lines
        .unwrap_or(state.mouse_scroll_lines)
}
fn pending_resume_agents_on_restore(state: &SettingsInput<'_>) -> bool {
    state
        .client
        .settings
        .pending_resume_agents_on_restore
        .unwrap_or(state.resume_agents_on_restore)
}

fn pending_window_title(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_window_title
        .clone()
        .unwrap_or_else(|| state.window_title_template.clone())
}

fn pending_headless_cols(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_headless_cols
        .clone()
        .unwrap_or_else(|| state.headless_size.0.to_string())
}

fn pending_headless_rows(state: &SettingsInput<'_>) -> String {
    state
        .client
        .settings
        .pending_headless_rows
        .clone()
        .unwrap_or_else(|| state.headless_size.1.to_string())
}

fn headless_size_action(state: &SettingsInput<'_>) -> Option<SettingsAction> {
    let cols = pending_headless_cols(state).parse::<u16>().ok()?;
    let rows = pending_headless_rows(state).parse::<u16>().ok()?;
    (cols > 0 && rows > 0).then_some(SettingsAction::SaveHeadlessSize { cols, rows })
}

#[derive(Clone, Copy)]
enum GeneralTextField {
    WindowTitle,
    HeadlessCols,
    HeadlessRows,
    DefaultShell,
    ToastDelay,
}

fn focused_general_text_field(state: &SettingsInput<'_>) -> Option<GeneralTextField> {
    let focused = state.client.settings.focused_input?;
    let window_title_index = theme_choice_len(state) + 13;
    match state.client.settings.section {
        SettingsSection::Theme if focused == window_title_index => {
            Some(GeneralTextField::WindowTitle)
        }
        SettingsSection::Experiments
            if focused == AdvancedRowId::HeadlessCols.selection_index() =>
        {
            Some(GeneralTextField::HeadlessCols)
        }
        SettingsSection::Experiments
            if focused == AdvancedRowId::HeadlessRows.selection_index() =>
        {
            Some(GeneralTextField::HeadlessRows)
        }
        SettingsSection::PaneLabels if focused == BehaviorRowId::DefaultShell.selection_index() => {
            Some(GeneralTextField::DefaultShell)
        }
        SettingsSection::Sound if focused == NotificationRowId::ToastDelay.selection_index() => {
            Some(GeneralTextField::ToastDelay)
        }
        _ => None,
    }
}

fn pending_general_text(state: &SettingsInput<'_>, field: GeneralTextField) -> String {
    match field {
        GeneralTextField::WindowTitle => pending_window_title(state),
        GeneralTextField::HeadlessCols => pending_headless_cols(state),
        GeneralTextField::HeadlessRows => pending_headless_rows(state),
        GeneralTextField::DefaultShell => pending_default_shell(state),
        GeneralTextField::ToastDelay => pending_toast_delay(state),
    }
}

fn set_pending_general_text(
    state: &mut SettingsInput<'_>,
    field: GeneralTextField,
    value: String,
) -> Option<SettingsAction> {
    match field {
        GeneralTextField::WindowTitle => {
            state.client.settings.pending_window_title = Some(value.clone());
            crate::config::window_title_diagnostics(&value)
                .is_none()
                .then_some(SettingsAction::SaveWindowTitle(value))
        }
        GeneralTextField::HeadlessCols => {
            state.client.settings.pending_headless_cols = Some(value);
            headless_size_action(state)
        }
        GeneralTextField::HeadlessRows => {
            state.client.settings.pending_headless_rows = Some(value);
            headless_size_action(state)
        }
        GeneralTextField::DefaultShell => {
            state.client.settings.pending_default_shell = Some(value.clone());
            Some(SettingsAction::SaveDefaultShell(value))
        }
        GeneralTextField::ToastDelay => {
            state.client.settings.pending_toast_delay = Some(value.clone());
            value
                .parse::<u64>()
                .ok()
                .filter(|seconds| *seconds <= MAX_TOAST_DELAY_SECONDS)
                .map(SettingsAction::SaveToastDelay)
        }
    }
}

fn edit_pending_general_text(
    state: &mut SettingsInput<'_>,
    key: KeyEvent,
) -> Option<Option<SettingsAction>> {
    let field = focused_general_text_field(state)?;
    let mut value = pending_general_text(state, field);
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => value.clear(),
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => value.clear(),
        KeyCode::Backspace => {
            value.pop();
        }
        KeyCode::Char(c)
            if key.modifiers.difference(KeyModifiers::SHIFT).is_empty()
                && (matches!(
                    field,
                    GeneralTextField::WindowTitle | GeneralTextField::DefaultShell
                ) || c.is_ascii_digit()) =>
        {
            value.push(c);
        }
        _ => return None,
    }
    Some(set_pending_general_text(state, field, value))
}

fn paste_settings_text(
    state: &mut SettingsInput<'_>,
    text: &str,
) -> Option<Option<SettingsAction>> {
    if matches!(
        state.client.settings.section,
        SettingsSection::GroupGeneral
            | SettingsSection::GroupDefaults
            | SettingsSection::GroupGithub
            | SettingsSection::WorkspaceGeneral
            | SettingsSection::WorkspaceGithub
    ) {
        let section = state.client.settings.section;
        let field = state.client.settings.focused_input?;
        let mut value = if matches!(
            section,
            SettingsSection::GroupGeneral
                | SettingsSection::GroupDefaults
                | SettingsSection::GroupGithub
        ) {
            pending_group_field(state, section, field)?
        } else {
            pending_workspace_field(state, section, field)?
        };
        value.extend(text.chars().filter(|character| !character.is_control()));
        if matches!(
            section,
            SettingsSection::GroupGeneral
                | SettingsSection::GroupDefaults
                | SettingsSection::WorkspaceGeneral
        ) {
            if matches!(
                section,
                SettingsSection::GroupGeneral | SettingsSection::GroupDefaults
            ) {
                set_pending_group_field(state, section, field, value);
            } else {
                set_pending_workspace_field(state, section, field, value);
            }
        } else if section == SettingsSection::GroupGithub {
            set_pending_group_field(state, section, field, value);
        } else {
            set_pending_workspace_field(state, section, field, value);
        }
        return Some(match section {
            SettingsSection::GroupGeneral => selected_group_general_action(state),
            SettingsSection::GroupDefaults => selected_group_defaults_action(state),
            SettingsSection::WorkspaceGeneral => selected_workspace_general_action(state),
            SettingsSection::GroupGithub | SettingsSection::WorkspaceGithub => None,
            _ => None,
        });
    }
    let field = focused_general_text_field(state)?;
    let mut value = pending_general_text(state, field);
    let original_len = value.len();
    value.extend(text.chars().filter(|ch| {
        if matches!(
            field,
            GeneralTextField::WindowTitle | GeneralTextField::DefaultShell
        ) {
            !ch.is_control()
        } else {
            ch.is_ascii_digit()
        }
    }));
    if value.len() == original_len {
        return Some(None);
    }
    Some(set_pending_general_text(state, field, value))
}

fn pending_command(state: &SettingsInput<'_>, field: CommandField) -> String {
    match field {
        CommandField::Browser => state
            .client
            .settings
            .pending_browser_command
            .clone()
            .unwrap_or_else(|| state.browser_command.clone()),
        CommandField::Review => state
            .client
            .settings
            .pending_review_command
            .clone()
            .unwrap_or_else(|| state.review_command.clone()),
        CommandField::Editor => state
            .client
            .settings
            .pending_editor_command
            .clone()
            .unwrap_or_else(|| state.editor_command.clone()),
    }
}

fn set_pending_command(state: &mut SettingsInput<'_>, field: CommandField, value: String) {
    match field {
        CommandField::Browser => state.client.settings.pending_browser_command = Some(value),
        CommandField::Review => state.client.settings.pending_review_command = Some(value),
        CommandField::Editor => state.client.settings.pending_editor_command = Some(value),
    }
}

fn reset_pending_command(state: &mut SettingsInput<'_>, field: CommandField) {
    set_pending_command(state, field, field.default_value().to_owned());
}

fn reset_all_pending_commands(state: &mut SettingsInput<'_>) {
    let defaults = CommandsConfig::default();
    state.client.settings.pending_browser_command = Some(defaults.browser);
    state.client.settings.pending_review_command = Some(defaults.review);
    state.client.settings.pending_editor_command = Some(defaults.editor);
}

fn command_field_from_index(index: usize) -> Option<CommandField> {
    match CommandRowId::from_selection_index(index) {
        Some(CommandRowId::Field(field)) => Some(field),
        _ => None,
    }
}

fn delete_pending_command_word(state: &mut SettingsInput<'_>, field: CommandField) {
    let mut value = pending_command(state, field);
    while value.chars().last().is_some_and(char::is_whitespace) {
        value.pop();
    }
    while value.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
        value.pop();
    }
    set_pending_command(state, field, value);
}

fn edit_pending_command(state: &mut SettingsInput<'_>, key: KeyEvent) -> bool {
    let Some(field) = state
        .client
        .settings
        .focused_input
        .and_then(command_field_from_index)
    else {
        return false;
    };
    state
        .client
        .settings
        .list
        .select(CommandRowId::Field(field).selection_index());
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            set_pending_command(state, field, String::new());
            true
        }
        KeyCode::Backspace if key.modifiers.contains(KeyModifiers::SUPER) => {
            set_pending_command(state, field, String::new());
            true
        }
        KeyCode::Backspace
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            delete_pending_command_word(state, field);
            true
        }
        KeyCode::Char('h' | 'w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_pending_command_word(state, field);
            true
        }
        KeyCode::Backspace => {
            let mut value = pending_command(state, field);
            value.pop();
            set_pending_command(state, field, value);
            true
        }
        KeyCode::Char(c) if key.modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            let mut value = pending_command(state, field);
            value.push(c);
            set_pending_command(state, field, value);
            true
        }
        _ => false,
    }
}

fn pending_sidebar_width(state: &SettingsInput<'_>) -> u16 {
    state
        .client
        .settings
        .pending_sidebar_width
        .unwrap_or(state.default_sidebar_width)
}

fn pending_sidebar_min_width(state: &SettingsInput<'_>) -> u16 {
    state
        .client
        .settings
        .pending_sidebar_min_width
        .unwrap_or(state.sidebar_min_width)
}

fn pending_sidebar_max_width(state: &SettingsInput<'_>) -> u16 {
    state
        .client
        .settings
        .pending_sidebar_max_width
        .unwrap_or(state.sidebar_max_width)
}

fn pending_sidebar_arrangement(state: &SettingsInput<'_>) -> SidebarArrangementConfig {
    state
        .client
        .settings
        .pending_sidebar_arrangement
        .unwrap_or(state.sidebar_arrangement)
}
fn pending_context_bar_visibility(state: &SettingsInput<'_>) -> ContextBarVisibilityConfig {
    state
        .client
        .settings
        .pending_context_bar_visibility
        .unwrap_or(state.context_bar_visibility)
}

fn pending_sidebar_initial_state(state: &SettingsInput<'_>) -> SidebarInitialStateConfig {
    state
        .client
        .settings
        .pending_sidebar_initial_state
        .unwrap_or(state.sidebar_config.initial_state)
}

fn pending_sidebar_initial_agent_scope(state: &SettingsInput<'_>) -> AgentPanelScopeConfig {
    state
        .client
        .settings
        .pending_sidebar_initial_agent_scope
        .unwrap_or(state.sidebar_config.initial_agent_scope)
}

fn pending_pane_border_agent_info(state: &SettingsInput<'_>) -> PaneBorderAgentInfoConfig {
    state
        .client
        .settings
        .pending_pane_border_agent_info
        .unwrap_or_else(|| state.pane_border_agent_info())
}

fn pending_status_indicators(state: &SettingsInput<'_>) -> StatusIndicatorStyle {
    state
        .client
        .settings
        .pending_status_indicators
        .unwrap_or_else(|| state.status_indicators())
}

fn target_theme_index(state: &SettingsInput<'_>) -> usize {
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

fn preview_selected_theme(state: &mut SettingsInput<'_>) {
    let Some(choice) = selected_theme_settings_choice(state) else {
        return;
    };
    match choice {
        ThemeSettingsChoice::GroupAccent(accent) => {
            state.client.settings.pending_group_accent_choice = Some(accent);
        }
        ThemeSettingsChoice::SourceSystem => {
            state.client.settings.pending_theme_mode = Some(ThemeMode::System);
            state.client.settings.pending_light_theme_name = Some("system".to_string());
            state.client.settings.pending_dark_theme_name = Some("system".to_string());
            state.client.settings.list.selected = 0;
            ensure_settings_selection_visible(state);
        }
        ThemeSettingsChoice::TerminalLightAccent(accent) => {
            state.client.settings.pending_terminal_light_accent = Some(accent);
        }
        ThemeSettingsChoice::TerminalDarkAccent(accent) => {
            state.client.settings.pending_terminal_dark_accent = Some(accent);
        }
        ThemeSettingsChoice::SourceCustom => {
            if normalize_theme_name(&pending_light_theme_name(state)) == "system" {
                state.client.settings.pending_light_theme_name =
                    Some(crate::app::state::DEFAULT_LIGHT_THEME_NAME.to_string());
            }
            if normalize_theme_name(&pending_dark_theme_name(state)) == "system" {
                state.client.settings.pending_dark_theme_name =
                    Some(crate::app::state::DEFAULT_DARK_THEME_NAME.to_string());
            }
            state.client.settings.list.selected = 1;
            ensure_settings_selection_visible(state);
        }
        ThemeSettingsChoice::Mode(mode) => {
            state.client.settings.pending_theme_mode = Some(mode);
            state.client.settings.list.selected = 2 + current_theme_mode_index(mode);
            ensure_settings_selection_visible(state);
        }
        ThemeSettingsChoice::Theme(choice) => match choice.target {
            ThemeChoiceTarget::Light => {
                state.client.settings.pending_light_theme_name = Some(choice.name.to_string());
            }
            ThemeChoiceTarget::Dark => {
                state.client.settings.pending_dark_theme_name = Some(choice.name.to_string());
            }
        },
    }
}

fn selected_integration_action(state: &SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let selected = state.client.settings.list.selected;
    let has_host_selector = !state.ssh_connection_profiles.is_empty();
    if has_host_selector && selected == 0 {
        return Some(SettingsAction::CycleIntegrationHost);
    }
    let entry_index = selected.saturating_sub(usize::from(has_host_selector));

    if let Some(host_id) =
        crate::app::integration_host::resolve(state, &state.client.settings).host_id()
    {
        let crate::integration::host::HostIntegrationObservation::Ready(snapshot) =
            state.host_integration_observations.get(host_id)?
        else {
            return None;
        };
        let entry = snapshot.entries.get(entry_index)?;
        return integration_action_for_status(
            entry.target,
            entry.state,
            entry.available,
            entry.missing_profile_hooks,
        );
    }

    let recommendation = state.integration_recommendations.get(entry_index)?;
    let missing_profile_hooks = crate::integration::missing_profile_hook_count_for_target(
        recommendation.target,
        &state.agent_profiles,
    );
    integration_action_for_status(
        recommendation.target,
        recommendation.state,
        recommendation.available,
        missing_profile_hooks,
    )
}

fn integration_action_for_status(
    target: crate::api::schema::IntegrationTarget,
    state: crate::integration::IntegrationStatusKind,
    available: bool,
    missing_profile_hooks: usize,
) -> Option<SettingsAction> {
    if state == crate::integration::IntegrationStatusKind::Current {
        return if missing_profile_hooks > 0 {
            Some(SettingsAction::InstallIntegration(target))
        } else {
            Some(SettingsAction::UninstallIntegration(target))
        };
    }
    match state {
        crate::integration::IntegrationStatusKind::Outdated => {
            Some(SettingsAction::InstallIntegration(target))
        }
        crate::integration::IntegrationStatusKind::NotInstalled if available => {
            Some(SettingsAction::InstallIntegration(target))
        }
        crate::integration::IntegrationStatusKind::NotInstalled
        | crate::integration::IntegrationStatusKind::Current => None,
    }
}

fn pending_theme_mode(state: &SettingsInput<'_>) -> ThemeMode {
    state
        .client
        .settings
        .pending_theme_mode
        .unwrap_or(state.global_theme_mode)
}

fn close_settings(state: &mut SettingsInput<'_>) {
    close_settings_for_view(state.client);
}

pub(crate) fn close_settings_for_view(view: &mut ClientViewState) {
    view.settings.original_palette = None;
    view.settings.original_theme = None;
    clear_settings_pending(&mut view.settings);
    view.return_to_active_workspace_mode();
}

fn selected_group_general_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let group_idx = state.client.settings.group_settings_target?;
    match state.client.settings.list.selected {
        GROUP_GENERAL_NAME => {
            let name = pending_group_name(state).trim().to_string();
            (!name.is_empty()).then_some(SettingsAction::SaveGroupName { group_idx, name })
        }
        GROUP_GENERAL_ICON => Some(SettingsAction::SaveGroupIcon {
            group_idx,
            icon: pending_group_icon(state),
        }),
        GROUP_GENERAL_DELETE => {
            close_settings(state);
            Some(SettingsAction::DeleteGroup(group_idx))
        }
        _ => None,
    }
}

fn selected_group_defaults_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let group_idx = state.client.settings.group_settings_target?;
    match state.client.settings.list.selected {
        GROUP_DEFAULTS_HOST | GROUP_DEFAULTS_DIRECTORY => {
            Some(SettingsAction::SaveGroupDefaultLocation {
                group_idx,
                default_location: super::group_default_location_for(
                    &state.ssh_connection_profiles,
                    &pending_group_default_host(state),
                    &pending_group_default_directory(state),
                ),
            })
        }
        _ => None,
    }
}

fn selected_group_github_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state)
        || state.client.settings.list.selected != GROUP_GITHUB_ORGANIZATION
    {
        return None;
    }
    let group_idx = state.client.settings.group_settings_target?;
    let value = pending_group_github_organization(state);
    let organization = match crate::app::state::GithubOrganization::parse(&value) {
        Ok(organization) => organization,
        Err(context) => {
            state.toast = Some(crate::app::state::ToastNotification {
                kind: crate::app::state::ToastKind::NeedsAttention,
                title: "GitHub Organization Not Saved".to_string(),
                context,
                position: None,
                target: None,
            });
            return None;
        }
    };
    Some(SettingsAction::SaveGroupGithubOrganization {
        group_idx,
        organization,
    })
}

fn selected_workspace_general_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let ws_idx = state.client.settings.workspace_settings_target?;
    match state.client.settings.list.selected {
        WORKSPACE_GENERAL_NAME => {
            let name = pending_workspace_name(state).trim().to_string();
            (!name.is_empty()).then_some(SettingsAction::SaveWorkspaceName { ws_idx, name })
        }
        WORKSPACE_GENERAL_HOST | WORKSPACE_GENERAL_DIRECTORY => {
            let cwd = pending_workspace_default_cwd(state).trim().to_string();
            let path = crate::execution_host::HostPath::new(cwd).ok()?;
            Some(SettingsAction::SaveWorkspaceDefaultLocation {
                ws_idx,
                location: crate::execution_host::ResourceLocation::new(
                    pending_workspace_default_host(state),
                    path,
                ),
            })
        }
        _ => None,
    }
}

fn selected_workspace_github_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    let ws_idx = state.client.settings.workspace_settings_target?;
    match state.client.settings.list.selected {
        WORKSPACE_GITHUB_AUTOMATIC => {
            let scope = crate::github::GithubRepositoryScope::Automatic;
            state.client.settings.pending_workspace_github_scope = Some(scope.clone());
            Some(SettingsAction::SaveWorkspaceGithubScope { ws_idx, scope })
        }
        WORKSPACE_GITHUB_SELECTED | WORKSPACE_GITHUB_REPOSITORIES => {
            let value = pending_workspace_github_repositories(state);
            let scope = match crate::github::GithubRepositoryScope::selected_from_input(&value) {
                Ok(scope) => scope,
                Err(context) => {
                    state.toast = Some(crate::app::state::ToastNotification {
                        kind: crate::app::state::ToastKind::NeedsAttention,
                        title: "GitHub Repositories Not Saved".to_string(),
                        context,
                        position: None,
                        target: None,
                    });
                    return None;
                }
            };
            state.client.settings.pending_workspace_github_scope = Some(scope.clone());
            state.client.settings.pending_workspace_github_repositories = Some(
                scope
                    .selected_repositories()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            Some(SettingsAction::SaveWorkspaceGithubScope { ws_idx, scope })
        }
        WORKSPACE_GITHUB_GROUP => {
            let scope = crate::github::GithubRepositoryScope::GroupOrganization;
            state.client.settings.pending_workspace_github_scope = Some(scope.clone());
            Some(SettingsAction::SaveWorkspaceGithubScope { ws_idx, scope })
        }
        _ => None,
    }
}

fn group_profile_id_for_index(state: &SettingsInput<'_>, selected: usize) -> Option<String> {
    let group_idx = state.client.settings.group_settings_target?;
    let favorites = state
        .groups
        .get(group_idx)?
        .favorite_agent_profile_ids
        .as_slice();
    let (favorite, available) = state.agent_profiles.group_sections(favorites);
    favorite
        .into_iter()
        .chain(available)
        .filter(|profile| state.agent_profile_launchable(profile))
        .nth(selected)
        .map(|profile| profile.id.clone())
}

fn toggle_selected_group_profile_favorite(state: &mut SettingsInput<'_>) {
    if !settings_selection_active(state) {
        return;
    }
    let Some(group_idx) = state.client.settings.group_settings_target else {
        return;
    };
    let Some(profile_id) = group_profile_id_for_index(state, state.client.settings.list.selected)
    else {
        return;
    };
    state.toggle_group_agent_profile_favorite(group_idx, &profile_id);
}

fn toggle_selected_group_profile_default(state: &mut SettingsInput<'_>) {
    if !settings_selection_active(state) {
        return;
    }
    let Some(group_idx) = state.client.settings.group_settings_target else {
        return;
    };
    let Some(profile_id) = group_profile_id_for_index(state, state.client.settings.list.selected)
    else {
        return;
    };
    state.toggle_group_default_agent_profile(group_idx, &profile_id);
}
fn clear_settings_pending(settings: &mut SettingsState) {
    settings.pending_theme_name = None;
    settings.pending_theme_mode = None;
    settings.pending_light_theme_name = None;
    settings.pending_dark_theme_name = None;
    settings.pending_terminal_light_accent = None;
    settings.pending_terminal_dark_accent = None;
    settings.pending_sound_enabled = None;
    settings.pending_toast_delivery = None;
    settings.pending_default_shell = None;
    settings.pending_shell_mode = None;
    settings.pending_version_check = None;
    settings.pending_manifest_check = None;
    settings.pending_toast_delay = None;
    settings.pending_toast_gardn_position = None;
    settings.pending_clipboard_toast_enabled = None;
    settings.pending_clipboard_toast_position = None;
    settings.pending_confirm_close = None;
    settings.pending_prompt_new_tab_name = None;
    settings.pending_show_counters = None;
    settings.pending_pane_borders = None;
    settings.pending_pane_scrollbars = None;
    settings.pending_pane_gaps = None;
    settings.pending_hide_tab_bar_when_single_tab = None;
    settings.pending_copy_on_select = None;
    settings.pending_prompt_new_workspace_name = None;
    settings.pending_right_click_passthrough_modifier = None;
    settings.pending_new_terminal_cwd = None;
    settings.pending_mouse_scroll_lines = None;
    settings.pending_resume_agents_on_restore = None;
    settings.pending_window_title = None;
    settings.pending_headless_cols = None;
    settings.pending_headless_rows = None;
    settings.pending_browser_command = None;
    settings.pending_review_command = None;
    settings.pending_editor_command = None;
    settings.pending_sidebar_width = None;
    settings.pending_sidebar_min_width = None;
    settings.pending_sidebar_max_width = None;
    settings.pending_sidebar_arrangement = None;
    settings.pending_context_bar_visibility = None;
    settings.pending_sidebar_initial_state = None;
    settings.pending_sidebar_initial_agent_scope = None;
    settings.pending_pane_border_agent_info = None;
    settings.pending_status_indicators = None;
    settings.pending_switch_ascii_input_source_in_prefix = None;
    settings.pending_group_accent_choice = None;
    settings.pending_group_name = None;
    settings.pending_group_icon = None;
    settings.pending_group_github_organization = None;
    settings.group_icon_picker_open = false;
    settings.pending_group_default_directory = None;

    settings.pending_workspace_name = None;
    settings.pending_workspace_default_cwd = None;
    settings.pending_workspace_github_scope = None;
    settings.pending_workspace_github_repositories = None;
    settings.pending_agent_profile_id = None;
    settings.pending_agent_profile_name = None;
    settings.pending_agent_profile_kind = None;
    settings.pending_agent_profile_command = None;
    settings.pending_agent_profile_enabled = None;
    settings.connection_editor = None;
    settings.group_settings_target = None;
    settings.workspace_settings_target = None;
}
fn current_settings_action(state: &SettingsInput<'_>) -> SettingsAction {
    SettingsAction::SaveSettings {
        light: pending_light_theme_name(state),
        dark: pending_dark_theme_name(state),
        mode: pending_theme_mode(state),
        terminal_light_accent: pending_terminal_light_accent(state),
        terminal_dark_accent: pending_terminal_dark_accent(state),
        sound_enabled: pending_sound_enabled(state),
        toast_delivery: pending_toast_delivery(state),
        confirm_close: pending_confirm_close(state),
        prompt_new_tab_name: pending_prompt_new_tab_name(state),
        show_counters: pending_show_counters(state),
        pane_borders: pending_pane_borders(state),
        pane_scrollbars: pending_pane_scrollbars(state),
        pane_gaps: pending_pane_gaps(state),
        hide_tab_bar_when_single_tab: pending_hide_tab_bar_when_single_tab(state),
        copy_on_select: pending_copy_on_select(state),
        prompt_new_workspace_name: pending_prompt_new_workspace_name(state),
        right_click_passthrough_modifier: pending_right_click_passthrough_modifier(state),
        new_terminal_cwd: pending_new_terminal_cwd(state),
        mouse_scroll_lines: pending_mouse_scroll_lines(state),
        browser_command: pending_command(state, CommandField::Browser),
        review_command: pending_command(state, CommandField::Review),
        editor_command: pending_command(state, CommandField::Editor),
        sidebar_width: pending_sidebar_width(state),
        sidebar_min_width: pending_sidebar_min_width(state),
        sidebar_max_width: pending_sidebar_max_width(state),
        sidebar_arrangement: pending_sidebar_arrangement(state),
        context_bar_visibility: pending_context_bar_visibility(state),
        sidebar_initial_state: pending_sidebar_initial_state(state),
        sidebar_initial_agent_scope: pending_sidebar_initial_agent_scope(state),
        pane_border_agent_info: pending_pane_border_agent_info(state),
        status_indicators: pending_status_indicators(state),
    }
}

fn current_settings_or_group_accent_action(state: &SettingsInput<'_>) -> SettingsAction {
    if let Some(group_idx) = state.client.settings.group_settings_target {
        SettingsAction::SaveGroupAccent {
            group_idx,
            accent: checked_group_accent_choice(state),
        }
    } else {
        current_settings_action(state)
    }
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

fn next_shell_mode(mode: ShellModeConfig) -> ShellModeConfig {
    match mode {
        ShellModeConfig::Auto => ShellModeConfig::Login,
        ShellModeConfig::Login => ShellModeConfig::NonLogin,
        ShellModeConfig::NonLogin => ShellModeConfig::Auto,
    }
}

fn next_toast_gardn_position(position: ToastGardnPosition) -> ToastGardnPosition {
    match position {
        ToastGardnPosition::TopLeft => ToastGardnPosition::TopRight,
        ToastGardnPosition::TopRight => ToastGardnPosition::BottomLeft,
        ToastGardnPosition::BottomLeft => ToastGardnPosition::BottomRight,
        ToastGardnPosition::BottomRight => ToastGardnPosition::TopLeft,
    }
}

fn next_toast_clipboard_position(position: ToastClipboardPosition) -> ToastClipboardPosition {
    match position {
        ToastClipboardPosition::TopLeft => ToastClipboardPosition::TopCenter,
        ToastClipboardPosition::TopCenter => ToastClipboardPosition::TopRight,
        ToastClipboardPosition::TopRight => ToastClipboardPosition::BottomLeft,
        ToastClipboardPosition::BottomLeft => ToastClipboardPosition::BottomCenter,
        ToastClipboardPosition::BottomCenter => ToastClipboardPosition::BottomRight,
        ToastClipboardPosition::BottomRight => ToastClipboardPosition::TopLeft,
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

fn select_pending_layout_setting(state: &mut SettingsInput<'_>) {
    select_pending_layout_setting_at(state, state.client.settings.list.selected);
}

fn select_pending_layout_setting_at(state: &mut SettingsInput<'_>, selected: usize) {
    match selected {
        0 => {
            let min = pending_sidebar_min_width(state);
            let max = pending_sidebar_max_width(state);
            let current = pending_sidebar_width(state).clamp(min, max);
            let next = current.saturating_add(2);
            state.client.settings.pending_sidebar_width = Some(if next > max { min } else { next });
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
            state.client.settings.pending_sidebar_min_width = Some(next);
            state.client.settings.pending_sidebar_width =
                Some(pending_sidebar_width(state).max(next));
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
            state.client.settings.pending_sidebar_max_width = Some(next);
            state.client.settings.pending_sidebar_width =
                Some(pending_sidebar_width(state).min(next));
        }
        3 => {
            state.client.settings.pending_sidebar_arrangement =
                Some(pending_sidebar_arrangement(state).next());
        }
        4 => {
            state.client.settings.pending_context_bar_visibility =
                Some(pending_context_bar_visibility(state).next());
        }
        5 => {
            state.client.settings.pending_sidebar_initial_state =
                Some(pending_sidebar_initial_state(state).next());
        }
        6 => {
            state.client.settings.pending_sidebar_initial_agent_scope =
                Some(pending_sidebar_initial_agent_scope(state).next());
        }
        _ => {}
    }
}

fn select_pending_appearance_setting(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    let selected = state.client.settings.list.selected;
    let theme_count = theme_choice_len(state);
    if state.client.settings.group_settings_target.is_some() || selected < theme_count {
        preview_selected_theme(state);
        return Some(current_settings_or_group_accent_action(state));
    }

    let appearance_selected = selected - theme_count;
    match appearance_selected {
        0..=6 => select_pending_layout_setting_at(state, appearance_selected),
        7 => state.client.settings.pending_pane_borders = Some(!pending_pane_borders(state)),
        8 => state.client.settings.pending_pane_scrollbars = Some(!pending_pane_scrollbars(state)),
        9 => state.client.settings.pending_pane_gaps = Some(!pending_pane_gaps(state)),
        10 => {
            state.client.settings.pending_hide_tab_bar_when_single_tab =
                Some(!pending_hide_tab_bar_when_single_tab(state))
        }
        11 => {
            state.client.settings.pending_pane_border_agent_info =
                Some(pending_pane_border_agent_info(state).next())
        }
        12 => {
            state.client.settings.pending_status_indicators =
                Some(pending_status_indicators(state).next())
        }
        13 => {}
        _ => {}
    }
    Some(current_settings_action(state))
}

fn select_pending_notification_setting(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    match NotificationRowId::from_selection_index(state.client.settings.list.selected) {
        Some(NotificationRowId::SoundAlerts) => {
            state.client.settings.pending_sound_enabled = Some(!pending_sound_enabled(state));
            Some(current_settings_action(state))
        }
        Some(NotificationRowId::ToastDelivery) => {
            state.client.settings.pending_toast_delivery =
                Some(next_toast_delivery(pending_toast_delivery(state)));
            Some(current_settings_action(state))
        }
        Some(NotificationRowId::ToastDelay) => None,
        Some(NotificationRowId::ToastGardnPosition) => {
            let next = next_toast_gardn_position(pending_toast_gardn_position(state));
            state.client.settings.pending_toast_gardn_position = Some(next);
            Some(SettingsAction::SaveToastGardnPosition(next))
        }
        Some(NotificationRowId::ClipboardEnabled) => {
            let next = !pending_clipboard_toast_enabled(state);
            state.client.settings.pending_clipboard_toast_enabled = Some(next);
            Some(SettingsAction::SaveClipboardToastEnabled(next))
        }
        Some(NotificationRowId::ClipboardPosition) => {
            let next = next_toast_clipboard_position(pending_clipboard_toast_position(state));
            state.client.settings.pending_clipboard_toast_position = Some(next);
            Some(SettingsAction::SaveClipboardToastPosition(next))
        }
        None => None,
    }
}

fn settings_selection_active(state: &SettingsInput<'_>) -> bool {
    state.client.settings.list.is_engaged()
}

fn clear_settings_selection(state: &mut SettingsInput<'_>) {
    state.client.settings.list.hide();
    state.client.settings.focused_input = None;
}

fn switch_settings_section(
    state: &mut SettingsInput<'_>,
    section: SettingsSection,
    selected: usize,
) {
    state.client.settings.section = section;
    state.client.settings.list.selected = selected;
    state.client.settings.scroll = 0;
    state.client.settings.group_icon_picker_open = false;
    clear_settings_selection(state);
}

fn general_settings_section_selection(
    state: &SettingsInput<'_>,
    section: SettingsSection,
) -> usize {
    match section {
        SettingsSection::Theme => target_theme_index(state),
        SettingsSection::Layout
        | SettingsSection::Sound
        | SettingsSection::Toast
        | SettingsSection::PaneLabels
        | SettingsSection::Commands
        | SettingsSection::Experiments
        | SettingsSection::Agents
        | SettingsSection::Integrations
        | SettingsSection::Connections
        | SettingsSection::GroupProfiles
        | SettingsSection::GroupGeneral
        | SettingsSection::GroupDefaults
        | SettingsSection::GroupGithub
        | SettingsSection::WorkspaceGeneral
        | SettingsSection::WorkspaceGithub
        | SettingsSection::About => 0,
    }
}

fn select_general_settings_section(state: &mut SettingsInput<'_>, section: SettingsSection) {
    let selected = general_settings_section_selection(state, section);
    switch_settings_section(state, section, selected);
    if section == SettingsSection::Theme {
        ensure_settings_selection_visible(state);
    }
}

fn settings_row_option_index(row: &SettingsListRow) -> Option<usize> {
    match row {
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
    }
}

fn select_general_settings_subsection(
    state: &mut SettingsInput<'_>,
    section: SettingsSection,
    subsection: usize,
) {
    select_general_settings_section(state, section);
    let anchor = crate::ui::settings_subsection_anchor(section, subsection);
    let Some(rows) = rows_for_section(state, section) else {
        return;
    };
    let header_index = anchor
        .and_then(|anchor| {
            rows.iter()
                .position(|row| matches!(row, SettingsListRow::Header(label) if *label == anchor))
        })
        .unwrap_or(0);
    state.client.settings.scroll = visual_row_count(&rows[..header_index]);
    if let Some(selected) = rows[header_index..]
        .iter()
        .find_map(settings_row_option_index)
    {
        state.client.settings.list.select(selected);
        focus_selected_settings_input(state);
    }
}

fn activate_settings_sidebar_entry(
    state: &mut SettingsInput<'_>,
    entry: crate::ui::SettingsSidebarEntry,
) {
    state.client.settings.sidebar_selection = SettingsSidebarSelection {
        section: entry.section,
        subsection: entry.subsection,
    };
    if let Some(subsection) = entry.subsection {
        state.client.settings.sidebar_expanded = Some(entry.section);
        select_general_settings_subsection(state, entry.section, subsection);
        return;
    }

    let collapse = state.client.settings.sidebar_expanded == Some(entry.section)
        && state.client.settings.section == entry.section;
    state.client.settings.sidebar_expanded = (!collapse).then_some(entry.section);
    select_general_settings_section(state, entry.section);
}

fn move_settings_sidebar_selection(state: &mut SettingsInput<'_>, forward: bool) {
    let entries = crate::ui::settings_sidebar_entries(&state.client.settings);
    let selected = entries
        .iter()
        .position(|entry| {
            entry.section == state.client.settings.sidebar_selection.section
                && entry.subsection == state.client.settings.sidebar_selection.subsection
        })
        .unwrap_or(0);
    let next = if forward {
        (selected + 1).min(entries.len().saturating_sub(1))
    } else {
        selected.saturating_sub(1)
    };
    if let Some(entry) = entries.get(next) {
        state.client.settings.sidebar_selection = SettingsSidebarSelection {
            section: entry.section,
            subsection: entry.subsection,
        };
    }
}

fn update_settings_sidebar_state(
    state: &mut SettingsInput<'_>,
    key: KeyEvent,
) -> Option<SettingsAction> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => move_settings_sidebar_selection(state, false),
        KeyCode::Down | KeyCode::Char('j') => move_settings_sidebar_selection(state, true),
        KeyCode::Left | KeyCode::Char('h') => {
            if state.client.settings.sidebar_selection.subsection.is_some() {
                state.client.settings.sidebar_selection.subsection = None;
            } else if state.client.settings.sidebar_expanded
                == Some(state.client.settings.sidebar_selection.section)
            {
                state.client.settings.sidebar_expanded = None;
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            let section = state.client.settings.sidebar_selection.section;
            state.client.settings.sidebar_expanded = Some(section);
            let entries = crate::ui::settings_sidebar_entries(&state.client.settings);
            state.client.settings.sidebar_selection.subsection = entries
                .iter()
                .find(|entry| entry.section == section && entry.subsection.is_some())
                .and_then(|entry| entry.subsection);
            select_general_settings_section(state, section);
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            let selection = state.client.settings.sidebar_selection;
            activate_settings_sidebar_entry(
                state,
                crate::ui::SettingsSidebarEntry {
                    section: selection.section,
                    subsection: selection.subsection,
                    label: "",
                },
            );
        }
        KeyCode::Esc => return handle_settings_modal_action(state, &key),
        _ => {}
    }
    None
}

fn select_pending_setting(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    match state.client.settings.section {
        SettingsSection::Theme => select_pending_appearance_setting(state),
        SettingsSection::Layout => {
            select_pending_layout_setting(state);
            Some(current_settings_action(state))
        }
        SettingsSection::Sound => select_pending_notification_setting(state),
        SettingsSection::Toast => {
            state.client.settings.pending_toast_delivery =
                Some(next_toast_delivery(pending_toast_delivery(state)));
            Some(current_settings_action(state))
        }
        SettingsSection::PaneLabels => {
            match BehaviorRowId::from_selection_index(state.client.settings.list.selected) {
                Some(BehaviorRowId::ConfirmClose) => {
                    state.client.settings.pending_confirm_close =
                        Some(!pending_confirm_close(state));
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::NameNewTabs) => {
                    state.client.settings.pending_prompt_new_tab_name =
                        Some(!pending_prompt_new_tab_name(state));
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::NameNewWorkspaces) => {
                    state.client.settings.pending_prompt_new_workspace_name =
                        Some(!pending_prompt_new_workspace_name(state));
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::ShowCounters) => {
                    state.client.settings.pending_show_counters =
                        Some(!pending_show_counters(state));
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::CopyOnSelect) => {
                    state.client.settings.pending_copy_on_select =
                        Some(!pending_copy_on_select(state));
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::RightClickPassthrough) => {
                    state
                        .client
                        .settings
                        .pending_right_click_passthrough_modifier =
                        Some(pending_right_click_passthrough_modifier(state).next());
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::DefaultShell) => None,
                Some(BehaviorRowId::ShellMode) => {
                    let next = next_shell_mode(pending_shell_mode(state));
                    state.client.settings.pending_shell_mode = Some(next);
                    Some(SettingsAction::SaveShellMode(next))
                }
                Some(BehaviorRowId::NewTerminalCwd) => {
                    let next = next_terminal_cwd_policy(pending_new_terminal_cwd(state));
                    state.client.settings.pending_new_terminal_cwd = Some(next);
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::MouseWheelSpeed) => {
                    let next = next_mouse_scroll_lines(pending_mouse_scroll_lines(state));
                    state.client.settings.pending_mouse_scroll_lines = Some(next);
                    Some(current_settings_action(state))
                }
                Some(BehaviorRowId::ResumeAgents) => {
                    state.client.settings.pending_resume_agents_on_restore =
                        Some(!pending_resume_agents_on_restore(state));
                    Some(SettingsAction::SaveResumeAgentsOnRestore(
                        pending_resume_agents_on_restore(state),
                    ))
                }
                None => None,
            }
        }
        SettingsSection::Commands => selected_command_action(state),
        SettingsSection::Experiments => selected_experiment_action(state),
        SettingsSection::Agents => selected_agent_profile_action(state),
        SettingsSection::Integrations => selected_integration_action(state),
        SettingsSection::Connections => selected_connection_profile_action(state),
        SettingsSection::GroupGeneral => selected_group_general_action(state),
        SettingsSection::GroupDefaults => selected_group_defaults_action(state),
        SettingsSection::GroupGithub => selected_group_github_action(state),
        SettingsSection::GroupProfiles => None,
        SettingsSection::WorkspaceGeneral => selected_workspace_general_action(state),
        SettingsSection::WorkspaceGithub => selected_workspace_github_action(state),
        SettingsSection::About => None,
    }
}

fn selected_experiment_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    if !settings_selection_active(state) {
        return None;
    }
    match AdvancedRowId::from_selection_index(state.client.settings.list.selected) {
        Some(AdvancedRowId::SwitchAscii) => {
            Some(SettingsAction::SaveSwitchAsciiInputSourceInPrefix(
                !state.switch_ascii_input_source_in_prefix_enabled(),
            ))
        }
        Some(AdvancedRowId::KittyGraphics) => Some(SettingsAction::SaveKittyGraphics(
            !state.kitty_graphics_enabled,
        )),
        Some(AdvancedRowId::HeadlessCols | AdvancedRowId::HeadlessRows) => {
            headless_size_action(state)
        }
        Some(AdvancedRowId::VersionCheck) => {
            let next = !pending_version_check(state);
            state.client.settings.pending_version_check = Some(next);
            Some(SettingsAction::SaveVersionCheck(next))
        }
        Some(AdvancedRowId::ManifestCheck) => {
            let next = !pending_manifest_check(state);
            state.client.settings.pending_manifest_check = Some(next);
            Some(SettingsAction::SaveManifestCheck(next))
        }
        None => None,
    }
}
fn selected_command_action(state: &mut SettingsInput<'_>) -> Option<SettingsAction> {
    match CommandRowId::from_selection_index(state.client.settings.list.selected) {
        Some(CommandRowId::Action(CommandAction::Reset(field))) => {
            reset_pending_command(state, field);
        }
        Some(CommandRowId::Action(CommandAction::ResetAll)) => {
            reset_all_pending_commands(state);
        }
        Some(CommandRowId::Field(_)) | None => {}
    }
    Some(current_settings_action(state))
}

fn settings_row_accepts_text_input(state: &SettingsInput<'_>, selected: usize) -> bool {
    match state.client.settings.section {
        SettingsSection::Theme => selected == theme_choice_len(state) + 13,
        SettingsSection::Commands => command_field_from_index(selected).is_some(),
        SettingsSection::Experiments => matches!(
            AdvancedRowId::from_selection_index(selected),
            Some(AdvancedRowId::HeadlessCols | AdvancedRowId::HeadlessRows)
        ),
        SettingsSection::PaneLabels => {
            BehaviorRowId::from_selection_index(selected) == Some(BehaviorRowId::DefaultShell)
        }
        SettingsSection::Sound => {
            NotificationRowId::from_selection_index(selected) == Some(NotificationRowId::ToastDelay)
        }
        SettingsSection::GroupGeneral => selected == GROUP_GENERAL_NAME,
        SettingsSection::GroupDefaults => selected == GROUP_DEFAULTS_DIRECTORY,
        SettingsSection::GroupGithub => selected == GROUP_GITHUB_ORGANIZATION,
        SettingsSection::WorkspaceGeneral => matches!(
            selected,
            WORKSPACE_GENERAL_NAME | WORKSPACE_GENERAL_DIRECTORY
        ),
        SettingsSection::WorkspaceGithub => selected == WORKSPACE_GITHUB_REPOSITORIES,
        SettingsSection::Agents if agent_profile_editor_open(state) => {
            selected == AGENT_PROFILE_NAME_INDEX || selected == agent_profile_command_index(state)
        }
        SettingsSection::Connections if connection_editor_open(state) => {
            matches!(
                crate::settings_rows::ConnectionRowId::from_selection_index(selected),
                Some(crate::settings_rows::ConnectionRowId::Field(_))
            )
        }
        _ => false,
    }
}

fn focus_selected_settings_input(state: &mut SettingsInput<'_>) {
    let selected = state.client.settings.list.selected;
    state.client.settings.focused_input =
        settings_row_accepts_text_input(state, selected).then_some(selected);
}

fn select_previous_setting(state: &mut SettingsInput<'_>, item_count: usize) {
    if item_count == 0 {
        return;
    }

    let had_selection = state.client.settings.list.restore();
    if let Some(rows) = rows_for_section(state, state.client.settings.section) {
        let selected = if had_selection {
            previous_option_index(&rows, state.client.settings.list.selected)
        } else {
            previous_option_index(&rows, usize::MAX)
        };
        if let Some(selected) = selected {
            state.client.settings.list.select(selected);
        }
        focus_selected_settings_input(state);
        return;
    }

    let selected = state.client.settings.list.selected.min(item_count - 1);
    state
        .client
        .settings
        .list
        .select(if !had_selection || selected == 0 {
            item_count - 1
        } else {
            selected - 1
        });
    focus_selected_settings_input(state);
}

fn select_next_setting(state: &mut SettingsInput<'_>, item_count: usize) {
    if item_count == 0 {
        return;
    }

    let had_selection = state.client.settings.list.restore();
    if let Some(rows) = rows_for_section(state, state.client.settings.section) {
        let selected = if had_selection {
            next_option_index(&rows, state.client.settings.list.selected)
        } else {
            next_option_index(&rows, usize::MAX)
        };
        if let Some(selected) = selected {
            state.client.settings.list.select(selected);
        }
        focus_selected_settings_input(state);
        return;
    }

    let selected = state.client.settings.list.selected.min(item_count - 1);
    state
        .client
        .settings
        .list
        .select(if !had_selection || selected + 1 == item_count {
            0
        } else {
            selected + 1
        });
    focus_selected_settings_input(state);
}

fn handle_settings_modal_action(
    state: &mut SettingsInput<'_>,
    key: &KeyEvent,
) -> Option<SettingsAction> {
    match super::modal::modal_action_from_key(key, super::modal::SETTINGS_ACTIONS) {
        Some(super::modal::ModalAction::Close) => {
            close_settings(state);
            None
        }
        _ => None,
    }
}

fn update_settings_state(state: &mut SettingsInput<'_>, key: KeyEvent) -> Option<SettingsAction> {
    if state.client.settings.group_settings_target.is_some()
        && !matches!(
            state.client.settings.section,
            SettingsSection::Theme
                | SettingsSection::GroupGeneral
                | SettingsSection::GroupDefaults
                | SettingsSection::GroupProfiles
                | SettingsSection::GroupGithub
        )
    {
        switch_settings_section(state, SettingsSection::GroupGeneral, 0);
    }
    if state.client.settings.workspace_settings_target.is_some()
        && !matches!(
            state.client.settings.section,
            SettingsSection::WorkspaceGeneral | SettingsSection::WorkspaceGithub
        )
    {
        switch_settings_section(state, SettingsSection::WorkspaceGeneral, 0);
    }
    let general_settings = state.client.settings.group_settings_target.is_none()
        && state.client.settings.workspace_settings_target.is_none();
    if general_settings && matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
        state.client.settings.sidebar_focused = !state.client.settings.sidebar_focused;
        if state.client.settings.sidebar_focused {
            clear_settings_selection(state);
        } else {
            state.client.settings.list.restore();
            focus_selected_settings_input(state);
        }
        return None;
    }
    if general_settings && state.client.settings.sidebar_focused {
        return update_settings_sidebar_state(state, key);
    }

    let section_before_key = state.client.settings.section;
    if state.client.settings.section == SettingsSection::Agents
        && agent_profile_editor_open(state)
        && edit_pending_agent_profile_text(state, key)
    {
        return None;
    }
    if state.client.settings.section == SettingsSection::Commands
        && edit_pending_command(state, key)
    {
        return None;
    }
    if state.client.settings.section == SettingsSection::Connections
        && connection_editor_open(state)
        && edit_pending_connection_text(state, key)
    {
        return None;
    }
    if let Some(action) = edit_pending_general_text(state, key) {
        return action;
    }
    state.client.settings.list.restore();
    match state.client.settings.section {
        SettingsSection::Theme => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Theme),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Theme),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::PageUp => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_PAGE_SCROLL_ROWS as usize);
            }
            KeyCode::PageDown => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_PAGE_SCROLL_ROWS as usize)
                    .min(settings_theme_max_scroll(state));
            }
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                if state.client.settings.group_settings_target.is_some() {
                    switch_settings_section(state, SettingsSection::GroupProfiles, 0);
                } else {
                    switch_settings_section(state, SettingsSection::Sound, 0);
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                if state.client.settings.group_settings_target.is_some() {
                    switch_settings_section(state, SettingsSection::GroupDefaults, 0);
                } else {
                    switch_settings_section(state, SettingsSection::About, 0);
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
                state.client.settings.section = SettingsSection::Theme;
                state.client.settings.list.selected = target_theme_index(state);
                ensure_settings_selection_visible(state);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.client.settings.section = SettingsSection::Sound;
                state.client.settings.list.selected = usize::from(!pending_sound_enabled(state));
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
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Sound),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Enter | KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::PaneLabels, 0);
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::Theme, target_theme_index(state));
                ensure_settings_selection_visible(state);
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
                state.client.settings.section = SettingsSection::Sound;
                state.client.settings.list.selected = usize::from(!pending_sound_enabled(state));
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.client.settings.section = SettingsSection::PaneLabels;
                state.client.settings.list.selected = 0;
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
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::PaneLabels),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Enter => return select_pending_setting(state),
            KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::Sound, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::Commands, 0);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Commands => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Commands),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Commands),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Enter | KeyCode::Char(' ') => return select_pending_setting(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::PaneLabels, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::Agents, 0);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Agents => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Agents),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Agents),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::PageUp => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_PAGE_SCROLL_ROWS as usize);
            }
            KeyCode::PageDown => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_PAGE_SCROLL_ROWS as usize)
                    .min(settings_section_max_scroll(state, SettingsSection::Agents));
            }
            KeyCode::Enter => {
                return selected_agent_profile_action(state);
            }
            KeyCode::Char(' ') => {
                return selected_agent_profile_action(state);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(profile_id) =
                    custom_profile_id_for_settings_index(state, state.client.settings.list.selected)
                {
                    return Some(SettingsAction::DeleteAgentProfile(profile_id));
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::Commands, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::Integrations, 0);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::Connections => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Connections),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Connections),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::PageUp => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_PAGE_SCROLL_ROWS as usize);
            }
            KeyCode::PageDown => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_PAGE_SCROLL_ROWS as usize)
                    .min(settings_section_max_scroll(
                        state,
                        SettingsSection::Connections,
                    ));
            }
            KeyCode::Enter => {
                return selected_connection_profile_action(state);
            }
            KeyCode::Char(' ') => {
                return selected_connection_profile_action(state);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(profile_id) = browse_connection_profile_id_for_index(
                    state,
                    state.client.settings.list.selected,
                ) {
                    let _ = load_connection_profile_detail(state, &profile_id);
                    state.client.settings.list.select(
                        crate::settings_rows::ConnectionRowId::Action(
                            crate::settings_rows::ConnectionAction::Delete,
                        )
                        .selection_index(),
                    );
                    return Some(SettingsAction::PreviewSshConnectionRetirement(profile_id));
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::Integrations, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::Experiments, 0);
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
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::Experiments),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Enter | KeyCode::Char(' ') => return selected_experiment_action(state),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::Connections, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::About, 0);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::About => match key.code {
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(state, SettingsSection::Experiments, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::Theme, target_theme_index(state));
                ensure_settings_selection_visible(state);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
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
                switch_settings_section(state, SettingsSection::Agents, 0);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::Connections, 0);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::GroupGeneral => {
            if state.client.settings.focused_input.is_some() && edit_pending_group_field(state, key)
            {
                return selected_group_general_action(state);
            }
            match key.code {
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
                KeyCode::Enter if state.client.settings.list.selected == GROUP_GENERAL_ICON => {
                    toggle_group_icon_picker(state);
                    return None;
                }
                KeyCode::Enter => return selected_group_general_action(state),
                KeyCode::Char(' ') if state.client.settings.list.selected == GROUP_GENERAL_ICON => {
                    toggle_group_icon_picker(state);
                    return None;
                }
                KeyCode::Char(' ')
                    if state.client.settings.list.selected == GROUP_GENERAL_DELETE =>
                {
                    return selected_group_general_action(state);
                }
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    switch_settings_section(state, SettingsSection::GroupGithub, 0);
                }
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    switch_settings_section(state, SettingsSection::GroupDefaults, 0);
                }
                _ => {
                    if let Some(action) = handle_settings_modal_action(state, &key) {
                        return Some(action);
                    }
                }
            }
        }
        SettingsSection::GroupDefaults => {
            if state.client.settings.focused_input.is_some() && edit_pending_group_field(state, key)
            {
                return selected_group_defaults_action(state);
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    select_previous_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::GroupDefaults),
                    );
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    select_next_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::GroupDefaults),
                    );
                }
                KeyCode::Enter | KeyCode::Char(' ')
                    if state.client.settings.list.selected == GROUP_DEFAULTS_HOST =>
                {
                    cycle_default_host(state, false);
                    return selected_group_defaults_action(state);
                }
                KeyCode::Enter => return selected_group_defaults_action(state),
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    switch_settings_section(state, SettingsSection::GroupGeneral, 0);
                }
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    switch_settings_section(
                        state,
                        SettingsSection::Theme,
                        group_accent_selection_index(state),
                    );
                    ensure_settings_selection_visible(state);
                }
                _ => {
                    if let Some(action) = handle_settings_modal_action(state, &key) {
                        return Some(action);
                    }
                }
            }
        }
        SettingsSection::GroupGithub => {
            if state.client.settings.focused_input.is_some() && edit_pending_group_field(state, key)
            {
                return None;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    select_previous_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::GroupGithub),
                    );
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    select_next_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::GroupGithub),
                    );
                }
                KeyCode::Enter => return selected_group_github_action(state),
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    switch_settings_section(state, SettingsSection::GroupProfiles, 0);
                }
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    switch_settings_section(state, SettingsSection::GroupGeneral, 0);
                }
                _ => {
                    if let Some(action) = handle_settings_modal_action(state, &key) {
                        return Some(action);
                    }
                }
            }
        }
        SettingsSection::GroupProfiles => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                select_previous_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::GroupProfiles),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                select_next_setting(
                    state,
                    settings_section_choice_len(state, SettingsSection::GroupProfiles),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::PageUp => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_PAGE_SCROLL_ROWS as usize);
            }
            KeyCode::PageDown => {
                state.client.settings.scroll = state
                    .client
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_PAGE_SCROLL_ROWS as usize)
                    .min(settings_section_max_scroll(
                        state,
                        SettingsSection::GroupProfiles,
                    ));
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                toggle_selected_group_profile_favorite(state);
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                toggle_selected_group_profile_default(state);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {}
            KeyCode::Left | KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {}
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                switch_settings_section(
                    state,
                    SettingsSection::Theme,
                    group_accent_selection_index(state),
                );
                ensure_settings_selection_visible(state);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                switch_settings_section(state, SettingsSection::GroupGithub, 0);
            }
            _ => {
                if let Some(action) = handle_settings_modal_action(state, &key) {
                    return Some(action);
                }
            }
        },
        SettingsSection::WorkspaceGeneral => {
            if edit_pending_workspace_field(state, key) {
                return selected_workspace_general_action(state);
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    select_previous_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::WorkspaceGeneral),
                    );
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    select_next_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::WorkspaceGeneral),
                    );
                }
                KeyCode::Enter | KeyCode::Char(' ')
                    if state.client.settings.list.selected == WORKSPACE_GENERAL_HOST =>
                {
                    cycle_default_host(state, true);
                    return selected_workspace_general_action(state);
                }
                KeyCode::Enter => return selected_workspace_general_action(state),
                KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                    switch_settings_section(state, SettingsSection::WorkspaceGithub, 0);
                }
                KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                    switch_settings_section(state, SettingsSection::WorkspaceGithub, 0);
                }
                _ => {
                    if let Some(action) = handle_settings_modal_action(state, &key) {
                        return Some(action);
                    }
                }
            }
        }
        SettingsSection::WorkspaceGithub => {
            if edit_pending_workspace_field(state, key) {
                return None;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    select_previous_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::WorkspaceGithub),
                    );
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    select_next_setting(
                        state,
                        settings_section_choice_len(state, SettingsSection::WorkspaceGithub),
                    );
                }
                KeyCode::Enter | KeyCode::Char(' ')
                    if matches!(
                        state.client.settings.list.selected,
                        WORKSPACE_GITHUB_AUTOMATIC
                            | WORKSPACE_GITHUB_SELECTED
                            | WORKSPACE_GITHUB_GROUP
                    ) =>
                {
                    return selected_workspace_github_action(state);
                }
                KeyCode::Enter
                    if state.client.settings.list.selected == WORKSPACE_GITHUB_REPOSITORIES =>
                {
                    return selected_workspace_github_action(state);
                }
                KeyCode::Tab
                | KeyCode::Right
                | KeyCode::Char('l')
                | KeyCode::BackTab
                | KeyCode::Left
                | KeyCode::Char('h') => {
                    switch_settings_section(state, SettingsSection::WorkspaceGeneral, 0);
                }
                _ => {
                    if let Some(action) = handle_settings_modal_action(state, &key) {
                        return Some(action);
                    }
                }
            }
        }
    }

    if state.client.settings.section != section_before_key {
        clear_settings_selection(state);
    }

    None
}

pub(crate) fn update_settings_state_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    key: KeyEvent,
) -> Option<SettingsAction> {
    update_settings_state(
        &mut SettingsInput {
            shared: state,
            client: view,
        },
        key,
    )
}

pub(crate) fn paste_settings_text_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    text: &str,
) -> Option<Option<SettingsAction>> {
    paste_settings_text(
        &mut SettingsInput {
            shared: state,
            client: view,
        },
        text,
    )
}

pub(crate) fn update_settings_mouse_for_view(
    state: &mut AppState,
    view: &mut ClientViewState,
    mouse: MouseEvent,
) -> Option<SettingsAction> {
    SettingsInput {
        shared: state,
        client: view,
    }
    .handle_settings_mouse(mouse)
}

pub(crate) fn prepare_general_settings_state(
    state: &AppState,
    settings: &mut SettingsState,
    section: SettingsSection,
) {
    settings.original_palette = Some(state.palette.clone());
    settings.original_theme = Some(state.theme_name.clone());
    settings.pending_theme_name = Some(state.global_theme_name.clone());
    settings.pending_theme_mode = Some(state.global_theme_mode);
    settings.pending_light_theme_name = Some(state.global_light_theme_name.clone());
    settings.pending_dark_theme_name = Some(state.global_dark_theme_name.clone());
    settings.pending_terminal_light_accent = Some(state.global_terminal_light_accent);
    settings.pending_terminal_dark_accent = Some(state.global_terminal_dark_accent);
    settings.pending_sound_enabled = Some(state.sound_enabled());
    settings.pending_toast_delivery = Some(state.toast_delivery());
    settings.pending_default_shell = Some(state.default_shell.clone());
    settings.pending_shell_mode = Some(state.shell_mode);
    settings.pending_version_check = Some(state.update_version_check);
    settings.pending_manifest_check = Some(state.update_manifest_check);
    settings.pending_toast_delay = Some(state.toast_config.delay_seconds.to_string());
    settings.pending_toast_gardn_position = Some(state.toast_config.gardn.position);
    settings.pending_clipboard_toast_enabled = Some(state.toast_config.clipboard.enabled);
    settings.pending_clipboard_toast_position = Some(state.toast_config.clipboard.position);
    settings.pending_confirm_close = Some(state.confirm_close_enabled());
    settings.pending_prompt_new_tab_name = Some(state.prompt_new_tab_name_enabled());
    settings.pending_show_counters = Some(state.show_counters);
    settings.pending_pane_borders = Some(state.pane_borders);
    settings.pending_pane_scrollbars = Some(state.pane_scrollbars);
    settings.pending_pane_gaps = Some(state.pane_gaps);
    settings.pending_hide_tab_bar_when_single_tab = Some(state.hide_tab_bar_when_single_tab);
    settings.pending_copy_on_select = Some(state.copy_on_select);
    settings.pending_prompt_new_workspace_name = Some(state.prompt_new_workspace_name);
    settings.pending_right_click_passthrough_modifier =
        Some(RightClickPassthroughModifierConfig::from_modifiers(
            state.right_click_passthrough_modifiers,
        ));
    settings.pending_new_terminal_cwd = Some(state.new_terminal_cwd.clone());
    settings.pending_mouse_scroll_lines = Some(state.mouse_scroll_lines);
    settings.pending_resume_agents_on_restore = Some(state.resume_agents_on_restore);
    settings.pending_window_title = Some(state.window_title_template.clone());
    settings.pending_headless_cols = Some(state.headless_size.0.to_string());
    settings.pending_headless_rows = Some(state.headless_size.1.to_string());
    settings.pending_browser_command = Some(state.browser_command.clone());
    settings.pending_review_command = Some(state.review_command.clone());
    settings.pending_editor_command = Some(state.editor_command.clone());
    settings.pending_sidebar_width = Some(state.default_sidebar_width);
    settings.pending_sidebar_min_width = Some(state.sidebar_min_width);
    settings.pending_sidebar_max_width = Some(state.sidebar_max_width);
    settings.pending_sidebar_arrangement = Some(state.sidebar_arrangement);
    settings.pending_context_bar_visibility = Some(state.context_bar_visibility);
    settings.pending_sidebar_initial_state = Some(state.sidebar_config.initial_state);
    settings.pending_sidebar_initial_agent_scope = Some(state.sidebar_config.initial_agent_scope);
    settings.pending_pane_border_agent_info = Some(state.pane_border_agent_info());
    settings.pending_status_indicators = Some(state.status_indicators());
    settings.pending_agent_profile_id = None;
    settings.pending_agent_profile_name = None;
    settings.pending_agent_profile_kind = Some(state.default_agent_profile_kind_choice());
    settings.pending_agent_profile_command = None;
    settings.pending_agent_profile_enabled = None;
    settings.connection_editor = None;
    settings.pending_workspace_name = None;
    settings.pending_workspace_default_cwd = None;
    settings.pending_workspace_github_scope = None;
    settings.pending_workspace_github_repositories = None;
    settings.group_settings_target = None;
    settings.workspace_settings_target = None;
    settings.section = section;
    settings.sidebar_expanded = Some(section);
    settings.sidebar_selection = SettingsSidebarSelection::section(section);
    settings.sidebar_focused = false;
    settings.list.selected = match section {
        SettingsSection::Theme => {
            if state.global_theme_mode == ThemeMode::System
                && normalize_theme_name(&state.global_light_theme_name) == "system"
                && normalize_theme_name(&state.global_dark_theme_name) == "system"
            {
                0
            } else {
                2 + current_theme_mode_index(state.global_theme_mode)
            }
        }
        SettingsSection::Layout => 0,
        SettingsSection::Sound => 0,
        SettingsSection::Toast => 0,
        SettingsSection::PaneLabels => 0,
        SettingsSection::Commands => 0,
        SettingsSection::Experiments => 0,
        SettingsSection::Agents => 0,
        SettingsSection::Integrations => 0,
        SettingsSection::Connections => 0,
        SettingsSection::GroupGeneral => 0,
        SettingsSection::GroupDefaults => 0,
        SettingsSection::GroupGithub => 0,
        SettingsSection::GroupProfiles => 0,
        SettingsSection::WorkspaceGeneral => 0,
        SettingsSection::WorkspaceGithub => 0,
        SettingsSection::About => 0,
    };
    settings.scroll = 0;
    settings.list = crate::app::state::ModalListState::hidden(settings.list.selected);
    settings.focused_input = None;
}

fn reset_settings_for_scoped_editor(state: &AppState, settings: &mut SettingsState) {
    settings.original_palette = Some(state.palette.clone());
    settings.original_theme = Some(state.theme_name.clone());
    settings.pending_theme_name = None;
    settings.pending_theme_mode = None;
    settings.pending_light_theme_name = None;
    settings.pending_dark_theme_name = None;
    settings.pending_terminal_light_accent = None;
    settings.pending_terminal_dark_accent = None;
    settings.pending_group_accent_choice = None;
    settings.pending_sound_enabled = None;
    settings.pending_toast_delivery = None;
    settings.pending_default_shell = None;
    settings.pending_shell_mode = None;
    settings.pending_version_check = None;
    settings.pending_manifest_check = None;
    settings.pending_toast_delay = None;
    settings.pending_toast_gardn_position = None;
    settings.pending_clipboard_toast_enabled = None;
    settings.pending_clipboard_toast_position = None;
    settings.pending_confirm_close = None;
    settings.pending_prompt_new_tab_name = None;
    settings.pending_show_counters = None;
    settings.pending_pane_borders = None;
    settings.pending_pane_scrollbars = None;
    settings.pending_pane_gaps = None;
    settings.pending_hide_tab_bar_when_single_tab = None;
    settings.pending_copy_on_select = None;
    settings.pending_prompt_new_workspace_name = None;
    settings.pending_right_click_passthrough_modifier = None;
    settings.pending_new_terminal_cwd = None;
    settings.pending_mouse_scroll_lines = None;
    settings.pending_resume_agents_on_restore = None;
    settings.pending_window_title = None;
    settings.pending_headless_cols = None;
    settings.pending_browser_command = None;
    settings.pending_review_command = None;
    settings.pending_editor_command = None;
    settings.pending_group_github_organization = None;
    settings.pending_workspace_github_scope = None;
    settings.pending_workspace_github_repositories = None;
    settings.pending_sidebar_width = None;
    settings.pending_sidebar_min_width = None;
    settings.pending_sidebar_max_width = None;
    settings.pending_sidebar_arrangement = None;
    settings.pending_context_bar_visibility = None;
    settings.pending_sidebar_initial_state = None;
    settings.pending_sidebar_initial_agent_scope = None;
    settings.pending_pane_border_agent_info = None;
    settings.pending_status_indicators = None;
    settings.pending_switch_ascii_input_source_in_prefix = None;
    settings.sidebar_expanded = None;
    settings.sidebar_selection = SettingsSidebarSelection::section(settings.section);
    settings.sidebar_focused = false;
}

pub(crate) fn prepare_group_settings_state(
    state: &AppState,
    settings: &mut SettingsState,
    group_idx: usize,
) -> bool {
    let Some(group) = state.groups.get(group_idx) else {
        return false;
    };
    reset_settings_for_scoped_editor(state, settings);
    settings.pending_group_name = Some(group.name.clone());
    settings.pending_group_icon = Some(group.icon.clone());
    settings.pending_group_github_organization = group
        .github_organization
        .as_ref()
        .map(|organization| organization.as_str().to_string());
    settings.pending_group_default_directory = None;

    settings.pending_group_default_execution_host_id = group
        .default_location
        .as_ref()
        .map(|location| location.execution_host_id.clone());
    settings.pending_workspace_name = None;
    settings.pending_workspace_default_cwd = None;
    settings.pending_workspace_github_scope = None;
    settings.pending_workspace_github_repositories = None;
    settings.pending_workspace_default_execution_host_id = None;
    settings.group_settings_target = Some(group_idx);
    settings.workspace_settings_target = None;
    settings.section = SettingsSection::GroupGeneral;
    settings.list = crate::app::state::ModalListState::hidden(0);
    settings.focused_input = None;
    settings.scroll = 0;
    true
}

pub(crate) fn prepare_workspace_settings_state(
    state: &AppState,
    settings: &mut SettingsState,
    ws_idx: usize,
) -> bool {
    let Some(workspace) = state.workspaces.get(ws_idx) else {
        return false;
    };
    reset_settings_for_scoped_editor(state, settings);
    settings.pending_group_name = None;
    settings.pending_group_icon = None;
    settings.pending_group_github_organization = None;
    settings.pending_group_default_directory = None;

    settings.pending_workspace_name = Some(workspace.display_name());
    settings.pending_workspace_default_cwd = Some(
        workspace
            .default_location
            .path
            .as_path()
            .display()
            .to_string(),
    );
    settings.pending_workspace_github_scope = Some(workspace.github_scope.clone());
    settings.pending_workspace_github_repositories = Some(
        workspace
            .github_scope
            .selected_repositories()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", "),
    );
    settings.pending_workspace_default_execution_host_id =
        Some(workspace.default_location.execution_host_id.clone());
    settings.group_settings_target = None;
    settings.workspace_settings_target = Some(ws_idx);
    settings.section = SettingsSection::WorkspaceGeneral;
    settings.list = crate::app::state::ModalListState::hidden(0);
    settings.focused_input = None;
    settings.scroll = 0;
    true
}

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

fn settings_footer_hints(settings: &SettingsState) -> &'static [(&'static str, &'static str)] {
    const DEFAULT: &[(&str, &str)] =
        &[("Move", "↑↓"), ("Action", "Space/↵"), ("Section", "←→/Tab")];
    const SIDEBAR: &[(&str, &str)] = &[("Move", "↑↓"), ("Action", "Space/↵"), ("Sidebar", "Tab")];
    const EDITABLE_LIST: &[(&str, &str)] = &[
        ("Move", "↑↓"),
        ("New/Edit", "Space/↵"),
        ("Delete", "Ctrl+D"),
        ("Section", "←→/Tab"),
    ];
    const SIDEBAR_EDITABLE_LIST: &[(&str, &str)] = &[
        ("Move", "↑↓"),
        ("New/Edit", "Space/↵"),
        ("Delete", "Ctrl+D"),
        ("Sidebar", "Tab"),
    ];
    const GROUP_PROFILES: &[(&str, &str)] = &[
        ("Move", "↑↓"),
        ("Favorite", "Ctrl+F"),
        ("Default", "Ctrl+D"),
        ("Section", "←→/tab"),
    ];

    let general =
        settings.group_settings_target.is_none() && settings.workspace_settings_target.is_none();
    let agent_editor = settings.pending_agent_profile_id.is_some()
        || settings.pending_agent_profile_name.is_some()
        || settings.pending_agent_profile_command.is_some();
    let connection_editor = settings_connection_editor_open(settings);
    if general {
        return match settings.section {
            SettingsSection::Agents if !agent_editor => SIDEBAR_EDITABLE_LIST,
            SettingsSection::Connections if !connection_editor => SIDEBAR_EDITABLE_LIST,
            _ => SIDEBAR,
        };
    }
    match settings.section {
        SettingsSection::Agents if !agent_editor => EDITABLE_LIST,
        SettingsSection::Connections if !connection_editor => EDITABLE_LIST,
        SettingsSection::GroupProfiles => GROUP_PROFILES,
        _ => DEFAULT,
    }
}

fn settings_footer_rows(settings: &SettingsState, width: u16) -> u16 {
    let mut rows = 1u16;
    let mut current_width = 0usize;
    for (label, key) in settings_footer_hints(settings) {
        let prefix = if current_width == 0 { 1 } else { 5 };
        let hint_width = prefix + label.width() + 1 + key.width();
        if current_width != 0 && current_width + hint_width > width as usize && rows < 2 {
            rows += 1;
            current_width = 0;
        }
        let prefix = if current_width == 0 { 1 } else { 5 };
        current_width += prefix + label.width() + 1 + key.width();
    }
    rows
}

fn settings_stack_content(settings: &SettingsState, inner: Rect) -> Rect {
    let header_rows = if settings.group_settings_target.is_none()
        && settings.workspace_settings_target.is_none()
    {
        1
    } else {
        4
    };
    crate::ui::modal_stack_areas(
        inner,
        header_rows,
        settings_footer_rows(settings, inner.width),
        0,
        1,
    )
    .content
}

fn settings_stack_header(settings: &SettingsState, inner: Rect) -> Rect {
    let header_rows = if settings.group_settings_target.is_none()
        && settings.workspace_settings_target.is_none()
    {
        1
    } else {
        4
    };
    crate::ui::modal_stack_areas(
        inner,
        header_rows,
        settings_footer_rows(settings, inner.width),
        0,
        1,
    )
    .header
}

fn settings_sections(settings: &SettingsState) -> &'static [SettingsSection] {
    if settings.group_settings_target.is_some() {
        GROUP_SETTINGS_SECTIONS
    } else if settings.workspace_settings_target.is_some() {
        WORKSPACE_SETTINGS_SECTIONS
    } else {
        SettingsSection::ALL
    }
}

fn settings_tab_width(settings: &SettingsState, section: SettingsSection) -> u16 {
    let label = if settings.group_settings_target.is_some() && section == SettingsSection::Theme {
        "Appearance"
    } else {
        section.label()
    };
    label.width() as u16 + 2
}

fn settings_visible_tab_range(settings: &SettingsState, row_width: u16) -> (usize, usize) {
    let sections = settings_sections(settings);
    if sections.is_empty() {
        return (0, 0);
    }
    let selected = sections
        .iter()
        .position(|section| *section == settings.section)
        .unwrap_or(0);
    let mut start = selected;
    let mut end = selected + 1;
    let tabs_width = |start: usize, end: usize| {
        let widths = (start..end)
            .map(|idx| settings_tab_width(settings, sections[idx]))
            .sum::<u16>();
        let gaps = end.saturating_sub(start + 1) as u16;
        let edges = u16::from(start > 0) * 2 + u16::from(end < sections.len()) * 2;
        widths.saturating_add(gaps).saturating_add(edges)
    };
    loop {
        let mut expanded = false;
        if start > 0 && tabs_width(start - 1, end) <= row_width {
            start -= 1;
            expanded = true;
        }
        if end < sections.len() && tabs_width(start, end + 1) <= row_width {
            end += 1;
            expanded = true;
        }
        if !expanded {
            break;
        }
    }
    (start, end)
}

fn settings_tab_at(settings: &SettingsState, row: Rect, col: u16) -> Option<SettingsSection> {
    let sections = settings_sections(settings);
    let (start, end) = settings_visible_tab_range(settings, row.width);
    if start > 0 && col >= row.x && col < row.x.saturating_add(2) {
        return sections.get(start - 1).copied();
    }
    let mut x = row.x.saturating_add(u16::from(start > 0) * 2);
    for (visible, idx) in (start..end).enumerate() {
        if visible > 0 {
            x = x.saturating_add(1);
        }
        let width = settings_tab_width(settings, sections[idx]);
        if col >= x && col < x.saturating_add(width) {
            return Some(sections[idx]);
        }
        x = x.saturating_add(width);
    }
    if end < sections.len() && col >= x && col < x.saturating_add(2) {
        sections.get(end).copied()
    } else {
        None
    }
}

impl SettingsInput<'_> {
    fn settings_popup_rect(&self) -> Rect {
        crate::ui::centered_popup_rect(self.settings_overlay_rect(), 92, 26).unwrap_or_default()
    }

    fn settings_sidebar_entry_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<crate::ui::SettingsSidebarEntry> {
        if self.client.settings.group_settings_target.is_some()
            || self.client.settings.workspace_settings_target.is_some()
        {
            return None;
        }
        let inner = self.settings_inner_rect();
        let content = settings_stack_content(&self.client.settings, inner);
        let navigation = crate::ui::settings_sidebar_areas(content).navigation;
        crate::ui::settings_sidebar_hit_areas(&self.client.settings, navigation)
            .into_iter()
            .find_map(|(entry, rect)| {
                (col >= rect.x && col < rect.x.saturating_add(rect.width) && row == rect.y)
                    .then_some(entry)
            })
    }

    fn settings_overlay_rect(&self) -> Rect {
        self.client.screen_rect()
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
        if self.client.settings.group_settings_target.is_none()
            && self.client.settings.workspace_settings_target.is_none()
        {
            return None;
        }
        let inner = self.settings_inner_rect();
        let header_rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<4>(settings_stack_header(&self.client.settings, inner));
        let tab_row = header_rows[2];
        if row != tab_row.y {
            return None;
        }
        settings_tab_at(&self.client.settings, tab_row, col)
    }

    fn settings_editor_back_at(&self, col: u16, row: u16) -> bool {
        let area = self.settings_content_rect();
        let editor_open = match self.client.settings.section {
            SettingsSection::Agents => {
                self.client.settings.pending_agent_profile_id.is_some()
                    || self.client.settings.pending_agent_profile_name.is_some()
                    || self.client.settings.pending_agent_profile_command.is_some()
            }
            SettingsSection::Connections => settings_connection_editor_open(&self.client.settings),
            _ => false,
        };
        let rect = Rect::new(area.x + area.width.saturating_sub(8), area.y, 8, 1);
        editor_open && col >= rect.x && col < rect.x + rect.width && row == rect.y
    }

    pub(crate) fn settings_content_rect(&self) -> Rect {
        let inner = self.settings_inner_rect();
        let content = settings_stack_content(&self.client.settings, inner);
        if self.client.settings.group_settings_target.is_none()
            && self.client.settings.workspace_settings_target.is_none()
        {
            crate::ui::settings_sidebar_areas(content).content
        } else {
            content
        }
    }

    fn settings_list_hit_at(&self, col: u16, row: u16) -> Option<SettingsRowHit> {
        let area = self.settings_content_rect();
        if row < area.y || row >= area.y + area.height || col < area.x || col >= area.x + area.width
        {
            return None;
        }

        match self.client.settings.section {
            SettingsSection::Theme => {
                let list = settings_section_list_geometry(self, SettingsSection::Theme);
                let visual_row = list.hit_visual_row(col, row)?;
                option_hit_for_visual_row(&theme_rows(self), visual_row)
            }
            SettingsSection::Layout
            | SettingsSection::Sound
            | SettingsSection::Toast
            | SettingsSection::PaneLabels
            | SettingsSection::Commands
            | SettingsSection::Experiments
            | SettingsSection::Agents
            | SettingsSection::Connections
            | SettingsSection::GroupGeneral
            | SettingsSection::GroupDefaults
            | SettingsSection::GroupGithub
            | SettingsSection::GroupProfiles
            | SettingsSection::WorkspaceGeneral
            | SettingsSection::WorkspaceGithub
            | SettingsSection::About
            | SettingsSection::Integrations => {
                let list = settings_section_list_geometry(self, self.client.settings.section);
                let visual_row = list.hit_visual_row(col, row)?;
                let rows = rows_for_section(self, self.client.settings.section)?;
                option_hit_for_visual_row(&rows, visual_row)
            }
        }
    }

    fn settings_theme_scrollbar_target_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<ScrollbarClickTarget> {
        if self.client.settings.section != SettingsSection::Theme {
            return None;
        }
        let list = settings_section_list_geometry(self, SettingsSection::Theme);
        let metrics = list.metrics();
        let track = list.scroll_area.track?;
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
        if self.client.settings.section != SettingsSection::Theme {
            return None;
        }
        let list = settings_section_list_geometry(self, SettingsSection::Theme);
        let metrics = list.metrics();
        let track = list.scroll_area.track?;
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
                            self.client.drag = Some(DragState {
                                target: DragTarget::SettingsThemeScrollbar { grab_row_offset },
                            });
                        }
                        ScrollbarClickTarget::Track { offset_from_bottom } => {
                            set_settings_theme_offset_from_bottom(self, offset_from_bottom);
                        }
                    }
                    return None;
                }

                if let Some(entry) = self.settings_sidebar_entry_at(mouse.column, mouse.row) {
                    self.client.settings.sidebar_focused = true;
                    activate_settings_sidebar_entry(self, entry);
                    return None;
                }

                if let Some(section) = self.settings_tab_at(mouse.column, mouse.row) {
                    let selected = match section {
                        SettingsSection::Theme => {
                            if self.client.settings.group_settings_target.is_some() {
                                group_accent_selection_index(self)
                            } else {
                                target_theme_index(self)
                            }
                        }
                        SettingsSection::Layout
                        | SettingsSection::Sound
                        | SettingsSection::Toast
                        | SettingsSection::PaneLabels
                        | SettingsSection::Commands
                        | SettingsSection::Experiments
                        | SettingsSection::Agents
                        | SettingsSection::Integrations
                        | SettingsSection::Connections
                        | SettingsSection::GroupGeneral
                        | SettingsSection::GroupDefaults
                        | SettingsSection::GroupGithub
                        | SettingsSection::GroupProfiles
                        | SettingsSection::WorkspaceGeneral
                        | SettingsSection::WorkspaceGithub
                        | SettingsSection::About => 0,
                    };
                    switch_settings_section(self, section, selected);
                    self.client.settings.group_icon_picker_open = false;
                    if section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                    return None;
                }

                self.client.settings.sidebar_focused = false;
                if self.settings_editor_back_at(mouse.column, mouse.row) {
                    match self.client.settings.section {
                        SettingsSection::Agents => close_agent_profile_editor(self),
                        SettingsSection::Connections => back_from_connection_screen(self),
                        _ => {}
                    }
                    return None;
                }
                if let Some(icon) = group_settings_icon_picker_hit(self, mouse.column, mouse.row) {
                    self.client.settings.pending_group_icon = Some(icon.to_string());
                    self.client.settings.group_icon_picker_open = false;
                    self.client.settings.list.select(GROUP_GENERAL_ICON);
                    return selected_group_general_action(self);
                }
                if let Some(target) = self.settings_list_hit_at(mouse.column, mouse.row) {
                    let idx = target.index;
                    self.client.settings.list.select(idx);
                    self.client.settings.focused_input = (!target.hoverable).then_some(idx);
                    if !target.hoverable {
                        return None;
                    }
                    if self.client.settings.section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                    return match self.client.settings.section {
                        SettingsSection::Theme
                        | SettingsSection::Layout
                        | SettingsSection::Sound
                        | SettingsSection::Toast
                        | SettingsSection::PaneLabels
                        | SettingsSection::Commands => select_pending_setting(self),
                        SettingsSection::Experiments => selected_experiment_action(self),
                        SettingsSection::Agents => selected_agent_profile_action(self),
                        SettingsSection::Connections => selected_connection_profile_action(self),
                        SettingsSection::Integrations => selected_integration_action(self),
                        SettingsSection::GroupGeneral => match idx {
                            GROUP_GENERAL_ICON => {
                                toggle_group_icon_picker(self);
                                None
                            }
                            GROUP_GENERAL_DELETE => {
                                self.client.settings.group_icon_picker_open = false;
                                selected_group_general_action(self)
                            }
                            _ => {
                                self.client.settings.group_icon_picker_open = false;
                                None
                            }
                        },
                        SettingsSection::GroupDefaults => match idx {
                            GROUP_DEFAULTS_HOST => {
                                self.client.settings.group_icon_picker_open = false;
                                cycle_default_host(self, false);
                                selected_group_defaults_action(self)
                            }
                            _ => None,
                        },
                        SettingsSection::GroupGithub => selected_group_github_action(self),
                        SettingsSection::GroupProfiles => None,
                        SettingsSection::WorkspaceGeneral => match idx {
                            WORKSPACE_GENERAL_HOST => {
                                cycle_default_host(self, true);
                                selected_workspace_general_action(self)
                            }
                            _ => None,
                        },
                        SettingsSection::WorkspaceGithub => selected_workspace_github_action(self),
                        SettingsSection::About => None,
                    };
                }

                let inner = self.settings_inner_rect();
                let close = crate::ui::settings_close_button_rect(inner);
                match super::modal::modal_action_from_buttons(
                    mouse.column,
                    mouse.row,
                    &[(close, super::modal::ModalAction::Close)],
                ) {
                    Some(super::modal::ModalAction::Close) => {
                        close_settings(self);
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
                            close_settings(self);
                        }
                        None
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(DragState {
                    target: DragTarget::SettingsThemeScrollbar { grab_row_offset },
                }) = &self.client.drag
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
                if self.client.drag.as_ref().is_some_and(|drag| {
                    matches!(drag.target, DragTarget::SettingsThemeScrollbar { .. })
                }) {
                    self.client.drag = None;
                }
                None
            }
            MouseEventKind::Moved => {
                let hovered = self
                    .settings_list_hit_at(mouse.column, mouse.row)
                    .filter(|target| target.hoverable)
                    .map(|target| target.index);
                self.client.settings.list.hover(hovered);
                None
            }
            MouseEventKind::ScrollUp => {
                self.client.settings.scroll = self
                    .client
                    .settings
                    .scroll
                    .saturating_sub(super::MODAL_WHEEL_SCROLL_ROWS as usize);
                None
            }
            MouseEventKind::ScrollDown => {
                self.client.settings.scroll = self
                    .client
                    .settings
                    .scroll
                    .saturating_add(super::MODAL_WHEEL_SCROLL_ROWS as usize)
                    .min(settings_section_max_scroll(
                        self,
                        self.client.settings.section,
                    ));
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::super::state_with_workspaces;
    use super::*;

    #[test]
    fn connection_drafts_are_client_local_across_views() {
        let mut state = state_with_workspaces(&["test"]);
        let mut first = ClientViewState::from_default_client_state(&state);
        let mut second = ClientViewState::from_default_client_state(&state);
        prepare_general_settings_state(&state, &mut first.settings, SettingsSection::Connections);
        prepare_general_settings_state(&state, &mut second.settings, SettingsSection::Connections);
        first.mode = Mode::Settings;
        second.mode = Mode::Settings;

        for view in [&mut first, &mut second] {
            update_settings_state_for_view(
                &mut state,
                view,
                KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
            );
            update_settings_state_for_view(
                &mut state,
                view,
                KeyEvent::new(KeyCode::Char(' '), KeyModifiers::empty()),
            );
        }
        for ch in "alpha".chars() {
            update_settings_state_for_view(
                &mut state,
                &mut first,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()),
            );
        }
        for ch in "beta".chars() {
            update_settings_state_for_view(
                &mut state,
                &mut second,
                KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()),
            );
        }

        assert_eq!(
            first
                .settings
                .connection_editor
                .as_ref()
                .map(|editor| editor.draft.target.as_str()),
            Some("alpha")
        );
        assert_eq!(
            second
                .settings
                .connection_editor
                .as_ref()
                .map(|editor| editor.draft.target.as_str()),
            Some("beta")
        );
    }

    #[test]
    fn settings_text_commit_updates_client_owned_shell_and_delay_drafts() {
        let mut state = state_with_workspaces(&["test"]);
        let mut view = ClientViewState::from_default_client_state(&state);
        prepare_general_settings_state(&state, &mut view.settings, SettingsSection::PaneLabels);
        view.mode = Mode::Settings;
        let shell_index = BehaviorRowId::DefaultShell.selection_index();
        view.settings.list.select(shell_index);
        view.settings.focused_input = Some(shell_index);

        assert_eq!(
            paste_settings_text_for_view(&mut state, &mut view, "/bin/zsh"),
            Some(Some(SettingsAction::SaveDefaultShell(
                "/bin/zsh".to_string()
            )))
        );
        assert_eq!(
            view.settings.pending_default_shell.as_deref(),
            Some("/bin/zsh")
        );

        view.settings.section = SettingsSection::Sound;
        let delay_index = NotificationRowId::ToastDelay.selection_index();
        view.settings.list.select(delay_index);
        view.settings.focused_input = Some(delay_index);
        view.settings.pending_toast_delay = Some(String::new());
        assert_eq!(
            paste_settings_text_for_view(&mut state, &mut view, "3601x"),
            Some(None)
        );
        assert_eq!(view.settings.pending_toast_delay.as_deref(), Some("3601"));
    }
}
