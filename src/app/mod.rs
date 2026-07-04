//! Application orchestration.
//!
//! - `state.rs` — AppState, Mode, and pure data structs
//! - `actions.rs` — state mutations (testable without PTYs/async)
//! - `input.rs` — key/mouse → action translation

pub(crate) mod actions;
pub(crate) mod agent_profile_picker;
mod agent_resume;
mod agents;
mod api;
mod api_helpers;
pub(crate) mod command_palette;
mod config_io;
mod creation;
mod ids;
mod input;
mod runtime;
mod session;
pub mod state;
mod terminal_targets;
mod theme_sync;
pub(crate) mod view_state;

use std::collections::{HashMap, HashSet};
use std::future::pending;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MIN_RENDER_INTERVAL: Duration = Duration::from_millis(16);
pub(crate) const ANIMATION_INTERVAL: Duration = Duration::from_millis(16);
pub(crate) const HEADLESS_ANIMATION_INTERVAL: Duration = Duration::from_millis(128);
pub(crate) const HEADLESS_ANIMATION_TICK_STEP: u32 = 8;
pub(crate) const SELECTION_AUTOSCROLL_INTERVAL: Duration = Duration::from_millis(30);
const RESIZE_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const PORT_SCAN_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const COMMAND_SCAN_INTERVAL: Duration = Duration::from_secs(5);
pub(crate) const API_NOTIFICATION_RATE_LIMIT: Duration = Duration::from_millis(250);
const PORT_STALE_TTL: Duration = Duration::from_secs(5);
const GIT_REMOTE_STATUS_REFRESH_INTERVAL: Duration = Duration::from_millis(1500);
const AUTO_UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(30 * 60);
const PENDING_AGENT_RESUME_THEME_WAIT: Duration = Duration::from_millis(750);
const SESSION_SAVE_DEBOUNCE: Duration = Duration::from_secs(5);
const SIDEBAR_DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(350);
const COPY_FEEDBACK_DURATION: Duration = Duration::from_secs(2);
const PANE_DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(350);
const PANE_COPY_HIGHLIGHT_DURATION: Duration = Duration::from_millis(500);

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute, terminal,
};
use ratatui::layout::Rect;
use ratatui::DefaultTerminal;
use tokio::sync::{mpsc, Notify};
use tracing::{debug, info};

use crate::config::Config;
use crate::events::AppEvent;

pub use state::{AppState, Mode, ToastKind, ViewState};
pub(crate) use view_state::ClientViewState;

pub(crate) fn load_plugin_manifest(
    path: &str,
    enabled: bool,
) -> Result<crate::api::schema::InstalledPluginInfo, (&'static str, String)> {
    api::plugins::load_plugin_manifest(path, enabled)
}

/// Full application: AppState + runtime concerns (event channels, async I/O).
#[derive(Debug, Clone)]
pub(crate) struct OverlayPaneState {
    ws_idx: usize,
    tab_idx: usize,
    previous_focus: crate::layout::PaneId,
    previous_zoomed: bool,
    temp_files: Vec<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PaneClickState {
    pane_id: crate::layout::PaneId,
    viewport_row: u16,
    col: u16,
    at: Instant,
}

impl PaneClickState {
    fn is_double_click_for(self, next: Self) -> bool {
        self.pane_id == next.pane_id
            && next.at.duration_since(self.at) <= PANE_DOUBLE_CLICK_WINDOW
            && self.viewport_row.abs_diff(next.viewport_row) <= 1
            && self.col.abs_diff(next.col) <= 1
    }
}

pub struct App {
    pub state: AppState,
    pub(crate) default_client_view: ClientViewState,
    pub(crate) terminal_runtimes: crate::terminal::TerminalRuntimeRegistry,
    pub event_tx: mpsc::Sender<AppEvent>,
    pub(crate) event_rx: mpsc::Receiver<AppEvent>,
    pub(crate) api_rx: tokio::sync::mpsc::UnboundedReceiver<crate::api::ApiRequestMessage>,
    pub(crate) event_hub: crate::api::EventHub,
    pub(crate) last_focus: Option<(usize, crate::layout::PaneId)>,
    pub(crate) no_session: bool,
    pub(crate) input_rx: Option<mpsc::Receiver<crate::raw_input::RawInputEvent>>,
    pub(crate) last_terminal_size: Option<(u16, u16)>,
    pub(crate) config_diagnostic_deadline: Option<Instant>,
    pub(crate) toast_deadline: Option<Instant>,
    pub(crate) copy_feedback_deadline: Option<Instant>,
    pub(crate) last_git_remote_status_refresh: Instant,
    pub(crate) git_refresh_in_flight: bool,
    pub(crate) git_refresh_due_after_in_flight: bool,
    pub(crate) git_status_cache: HashMap<std::path::PathBuf, crate::workspace::GitStatusCacheEntry>,
    pub(crate) last_sidebar_divider_click: Option<Instant>,
    pub(crate) last_pane_click: Option<PaneClickState>,
    pub(crate) next_resize_poll: Instant,
    pub(crate) next_port_scan: Instant,
    pub(crate) next_command_scan: Instant,
    pub(crate) next_animation_tick: Option<Instant>,
    pub(crate) next_auto_update_check: Option<Instant>,
    pub(crate) next_agent_manifest_update_check: Option<Instant>,
    pub(crate) agent_metadata_deadline: Option<Instant>,
    pub(crate) last_api_notification_at: Option<Instant>,
    pub(crate) pending_agent_resume_deadline: Option<Instant>,
    pub(crate) selection_autoscroll_deadline: Option<Instant>,
    pub(crate) selection_highlight_clear_deadline: Option<Instant>,
    pub(crate) session_save_deadline: Option<Instant>,
    pub(crate) persist_pane_history: bool,
    pub(crate) last_render_at: Option<Instant>,
    pub(crate) suppressed_repeat_keys:
        HashSet<(crossterm::event::KeyCode, crossterm::event::KeyModifiers)>,
    pub render_notify: Arc<Notify>,
    pub render_dirty: Arc<AtomicBool>,
    pub(crate) full_redraw_pending: bool,
    pub(crate) overlay_panes: HashMap<crate::layout::PaneId, OverlayPaneState>,
    pub(crate) local_terminal_notifications: bool,
    pub(crate) config_reloaded_from_disk: bool,
    prefix_input_source: Box<dyn crate::platform::PrefixInputSource>,
    #[cfg(test)]
    pub(crate) host_terminal_theme_query_count: std::cell::Cell<usize>,
}

pub(crate) enum LoopEvent {
    Timer,
    Internal(AppEvent),
    Api(Box<crate::api::ApiRequestMessage>),
    RawInput(crate::raw_input::RawInputEvent),
    InputClosed,
    RenderRequested,
}

struct SyncOutputGuard;

impl SyncOutputGuard {
    fn begin() -> io::Result<Self> {
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"\x1b[?2026h")?;
        stdout.flush()?;
        Ok(Self)
    }
}

impl Drop for SyncOutputGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(b"\x1b[?2026l");
        let _ = stdout.flush();
    }
}

async fn recv_raw_input_or_pending(
    input_rx: Option<&mut mpsc::Receiver<crate::raw_input::RawInputEvent>>,
) -> Option<crate::raw_input::RawInputEvent> {
    match input_rx {
        Some(rx) => rx.recv().await,
        None => pending().await,
    }
}

async fn sleep_until_or_pending(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await,
        None => pending().await,
    }
}

fn repeat_key_identity(
    key: &crate::input::TerminalKey,
) -> (crossterm::event::KeyCode, crossterm::event::KeyModifiers) {
    (key.code, key.modifiers)
}

fn command_palette_accepts_repeat_key(key: &crate::input::TerminalKey) -> bool {
    matches!(
        key.code,
        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Down
    )
}

fn settings_accepts_repeat_key(key: &crate::input::TerminalKey) -> bool {
    matches!(
        key.code,
        crossterm::event::KeyCode::Up
            | crossterm::event::KeyCode::Down
            | crossterm::event::KeyCode::Char('j')
            | crossterm::event::KeyCode::Char('k')
    )
}

fn mode_accepts_repeat_key(mode: Mode, key: &crate::input::TerminalKey) -> bool {
    match mode {
        Mode::CommandPalette | Mode::AgentProfilePicker => command_palette_accepts_repeat_key(key),
        Mode::Settings => settings_accepts_repeat_key(key),
        _ => false,
    }
}

fn auto_updates_enabled(no_session: bool) -> bool {
    !no_session && !cfg!(debug_assertions)
}

fn load_plugin_registry(no_session: bool) -> crate::app::state::InstalledPluginRegistry {
    if no_session {
        return std::collections::HashMap::new();
    }
    let entries = crate::persist::plugin_registry::load();
    let entries = crate::persist::plugin_registry::reload_manifests(entries, |path, enabled| {
        crate::app::api::plugins::load_plugin_manifest(path, enabled).map_err(|(_, msg)| msg)
    });
    entries
        .into_iter()
        .map(|plugin| (plugin.plugin_id.clone(), plugin))
        .collect()
}

fn agent_panel_scope_from_config(
    scope: crate::config::AgentPanelScopeConfig,
) -> state::AgentPanelScope {
    match scope {
        crate::config::AgentPanelScopeConfig::Current => state::AgentPanelScope::CurrentWorkspace,
        crate::config::AgentPanelScopeConfig::Group => state::AgentPanelScope::CurrentGroup,
        crate::config::AgentPanelScopeConfig::All => state::AgentPanelScope::AllWorkspaces,
    }
}

/// Parse the configured agent name list into a deduplicated set of `Agent`
/// values. Unknown agent names are silently dropped so a typo cannot disable
/// other valid entries.
fn parse_cjk_ime_agents(names: &[String]) -> Vec<crate::detect::Agent> {
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        if let Some(agent) = crate::detect::parse_agent_label(name) {
            if !out.contains(&agent) {
                out.push(agent);
            }
        }
    }
    out
}

/// Resolve the palette from config: base theme + optional custom overrides.
fn resolve_palette(
    config: &crate::config::Config,
    host_theme: crate::terminal_theme::TerminalTheme,
) -> state::Palette {
    resolve_palette_with_legacy_accent(config, true, host_theme)
}

fn resolve_palette_with_legacy_accent(
    config: &crate::config::Config,
    use_legacy_ui_accent: bool,
    host_theme: crate::terminal_theme::TerminalTheme,
) -> state::Palette {
    let appearance = config.theme.mode.resolve(host_theme);
    let (light_theme_name, dark_theme_name) = state::theme_config_names(&config.theme);
    let base_name = match appearance {
        crate::terminal_theme::ThemeAppearance::Light => &light_theme_name,
        crate::terminal_theme::ThemeAppearance::Dark => &dark_theme_name,
    };
    let mut palette = state::Palette::from_theme_with_terminal_accent(
        base_name,
        appearance,
        host_theme,
        match appearance {
            crate::terminal_theme::ThemeAppearance::Light => {
                config.theme.resolved_terminal_light_accent()
            }
            crate::terminal_theme::ThemeAppearance::Dark => {
                config.theme.resolved_terminal_dark_accent()
            }
        },
    )
    .unwrap_or_else(|| {
        tracing::warn!(
            theme = base_name,
            "unknown theme, falling back to default appearance theme"
        );
        let fallback = state::default_theme_name_for_appearance(appearance);
        state::Palette::from_theme(fallback, appearance).unwrap_or_else(state::Palette::catppuccin)
    });

    // Apply custom overrides if present
    if let Some(custom) = &config.theme.custom {
        palette = palette.with_overrides(custom);
    }

    // Legacy: if ui.accent is set and no theme.custom.accent, use it for compat
    if use_legacy_ui_accent
        && config.ui.accent != "cyan"
        && config
            .theme
            .custom
            .as_ref()
            .and_then(|c| c.accent.as_ref())
            .is_none()
    {
        palette.accent = crate::config::parse_color(&config.ui.accent);
    }

    palette
}

fn groups_from_snapshot(snap: &crate::persist::SessionSnapshot) -> Vec<state::Group> {
    if snap.groups.is_empty() {
        return vec![state::Group::default_group()];
    }

    let mut groups: Vec<state::Group> = snap
        .groups
        .iter()
        .map(|group| state::Group {
            id: group.id.clone(),
            name: group.name.clone(),
            icon: state::normalize_group_icon(&group.icon),
            accent: group.accent,
            default_directory: group.default_directory.clone(),
            favorite_agent_profile_ids: group.favorite_agent_profile_ids.clone(),
            default_agent_profile_id: group.default_agent_profile_id.clone(),
        })
        .collect();

    for workspace in &snap.workspaces {
        if groups.iter().any(|group| group.id == workspace.group_id) {
            continue;
        }
        groups.push(state::Group {
            id: workspace.group_id.clone(),
            name: format!("group {}", groups.len() + 1),
            icon: state::DEFAULT_GROUP_ICON.to_string(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
    }

    groups
}

impl App {
    pub fn new(
        config: &Config,
        no_session: bool,
        config_diagnostic: Option<String>,
        api_rx: tokio::sync::mpsc::UnboundedReceiver<crate::api::ApiRequestMessage>,
        event_hub: crate::api::EventHub,
    ) -> Self {
        let (prefix_code, prefix_mods) = config.prefix_key();
        crate::kitty_graphics::set_enabled(config.experimental.kitty_graphics);
        let (event_tx, event_rx) = mpsc::channel::<AppEvent>(64);
        let render_notify = Arc::new(Notify::new());
        let render_dirty = Arc::new(AtomicBool::new(false));

        // Try to restore previous session
        let mut restored_terminals = std::collections::HashMap::new();
        let mut restored_terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let (
            groups,
            active_group,
            group_filter_enabled,
            workspaces,
            active,
            selected,
            _restored_agent_panel_scope,
            sidebar_width,
            sidebar_width_source,
            sidebar_collapsed,
            sidebar_section_split,
            right_sidebar_width,
            right_sidebar_collapsed,
        ) = if no_session {
            (
                vec![state::Group::default_group()],
                0,
                true,
                Vec::new(),
                None,
                0,
                state::AgentPanelScope::CurrentWorkspace,
                config.ui.sidebar_width,
                state::SidebarWidthSource::ConfigDefault,
                false,
                0.5_f32,
                28,
                false,
            )
        } else if let Some(snap) = crate::persist::load() {
            let history = config
                .experimental
                .pane_history
                .then(crate::persist::load_history)
                .flatten();
            let (ws, terminals, terminal_runtimes) = crate::persist::restore(
                &snap,
                history.as_ref(),
                24,
                80,
                config.advanced.scrollback_limit_bytes,
                &config.terminal.default_shell,
                config.terminal.shell_mode,
                config.session.resume_agents_on_restore,
                event_tx.clone(),
                render_notify.clone(),
                render_dirty.clone(),
            );
            restored_terminals = terminals;
            restored_terminal_runtimes = terminal_runtimes.into();
            if ws.is_empty() {
                crate::logging::session_restored(0, "empty");
                (
                    groups_from_snapshot(&snap),
                    snap.active_group,
                    snap.group_filter_enabled,
                    Vec::new(),
                    None,
                    0,
                    snap.default_view.agent_panel_scope,
                    snap.default_view
                        .sidebar_width
                        .unwrap_or(config.ui.sidebar_width),
                    if snap.default_view.sidebar_width.is_some() {
                        state::SidebarWidthSource::Persisted
                    } else {
                        state::SidebarWidthSource::ConfigDefault
                    },
                    snap.default_view.sidebar_collapsed,
                    snap.default_view.sidebar_section_split.unwrap_or(0.5),
                    snap.default_view.right_sidebar_width.unwrap_or(28),
                    snap.default_view.right_sidebar_collapsed,
                )
            } else {
                crate::logging::session_restored(ws.len(), "ok");
                let active = snap.default_view.active.filter(|&i| i < ws.len());
                let selected = snap.default_view.selected.min(ws.len().saturating_sub(1));
                (
                    groups_from_snapshot(&snap),
                    snap.active_group,
                    snap.group_filter_enabled,
                    ws,
                    active,
                    selected,
                    snap.default_view.agent_panel_scope,
                    snap.default_view
                        .sidebar_width
                        .unwrap_or(config.ui.sidebar_width),
                    if snap.default_view.sidebar_width.is_some() {
                        state::SidebarWidthSource::Persisted
                    } else {
                        state::SidebarWidthSource::ConfigDefault
                    },
                    snap.default_view.sidebar_collapsed,
                    snap.default_view.sidebar_section_split.unwrap_or(0.5),
                    snap.default_view.right_sidebar_width.unwrap_or(28),
                    snap.default_view.right_sidebar_collapsed,
                )
            }
        } else {
            (
                vec![state::Group::default_group()],
                0,
                true,
                Vec::new(),
                None,
                0,
                state::AgentPanelScope::CurrentWorkspace,
                config.ui.sidebar_width,
                state::SidebarWidthSource::ConfigDefault,
                false,
                0.5_f32,
                28,
                false,
            )
        };

        let agent_panel_scope = agent_panel_scope_from_config(config.ui.agent_panel_scope);
        let active_group = active_group.min(groups.len().saturating_sub(1));
        let host_terminal_theme = crate::terminal_theme::TerminalTheme::default();
        let global_palette = resolve_palette(config, host_terminal_theme);
        let (global_light_theme_name, global_dark_theme_name) =
            state::theme_config_names(&config.theme);
        let global_theme_mode = config.theme.mode;
        let global_theme_name = match global_theme_mode.resolve(host_terminal_theme) {
            crate::terminal_theme::ThemeAppearance::Light => global_light_theme_name.clone(),
            crate::terminal_theme::ThemeAppearance::Dark => global_dark_theme_name.clone(),
        };

        // Validate sidebar bounds before they reach any `u16::clamp(min, max)`
        // call: `clamp` panics when `min > max`. On bad config, fall back to
        // the built-in defaults rather than crashing on the first render.
        let (sidebar_min_width, sidebar_max_width) = crate::config::validated_sidebar_bounds(
            config.ui.sidebar_min_width,
            config.ui.sidebar_max_width,
        )
        .unwrap_or_else(|| {
            tracing::warn!(
                min = config.ui.sidebar_min_width,
                max = config.ui.sidebar_max_width,
                "ui.sidebar_min_width is greater than sidebar_max_width; falling back to default bounds (18, 36)"
            );
            (18, 36)
        });

        let worktree_directory =
            crate::worktree::expand_tilde_absolute_path(&config.worktrees.directory);
        info!(
            pane_scrollback_limit_bytes = config.advanced.scrollback_limit_bytes,
            "using pane scrollback configuration"
        );

        let latest_release_notes = crate::release_notes::load_latest();
        let update_available = latest_release_notes
            .as_ref()
            .filter(|notes| notes.preview)
            .map(|notes| notes.version.clone());
        let latest_release_notes_available = latest_release_notes.is_some();
        let update_install_command = crate::update::update_install_command().to_string();
        let startup_product_announcement =
            crate::product_announcements::load_unseen_for_current_version();

        let mode = if config.should_show_onboarding() {
            state::Mode::Onboarding
        } else if startup_product_announcement.is_some() {
            state::Mode::ProductAnnouncement
        } else if active.is_some() {
            state::Mode::Terminal
        } else {
            state::Mode::Navigate
        };

        let agent_manifest_summaries = crate::detect::manifest::reload_manifests();

        let mut state = AppState {
            groups,
            active_group,
            group_filter_enabled,
            terminals: std::collections::HashMap::new(),
            git_repo_summaries: std::collections::HashMap::new(),
            next_agent_activity_seq: 0,
            direct_attach_resize_locks: std::collections::HashSet::new(),
            pane_id_aliases: std::collections::HashMap::new(),
            public_pane_id_aliases: std::collections::HashMap::new(),
            workspaces,
            active,
            previous_pane_focus: None,
            selected,
            mode,
            should_quit: false,
            detach_exits: no_session,
            detach_requested: false,
            request_new_workspace: false,
            request_new_tab: false,
            request_agent_profile_tab: None,
            request_reload_config: false,
            request_open_git_diff_command: false,
            git_repo_picker: state::GitRepoPickerState {
                ws_idx: 0,
                roots: Vec::new(),
                selected: 0,
                scroll: 0,
            },
            request_client_config_reload: false,
            request_clipboard_write: None,
            creating_new_tab: false,
            creating_new_group: false,
            group_icon_input: state::DEFAULT_GROUP_ICON.to_string(),
            group_default_directory_input: String::new(),
            group_modal_selected_field: 0,
            group_icon_picker_open: false,
            rename_group_target: None,
            requested_new_tab_name: None,
            rename_pane_target: None,
            confirm_delete_group: None,
            request_complete_onboarding: false,
            name_input: String::new(),
            name_input_replace_on_type: false,
            release_notes: None,
            product_announcement: startup_product_announcement.map(|announcement| {
                state::ProductAnnouncementState {
                    version: announcement.version,
                    id: announcement.id,
                    title: announcement.title,
                    body: announcement.body,
                    scroll: 0,
                    preview: announcement.preview,
                }
            }),
            keybind_help: state::KeybindHelpState { scroll: 0 },
            command_palette: state::CommandPaletteState {
                query: String::new(),
                selected: 0,
                scroll: 0,
            },
            agent_profile_picker: state::AgentProfilePickerState {
                ws_idx: 0,
                query: String::new(),
                kind_filter: None,
                selected: 0,
                scroll: 0,
            },
            navigator: state::NavigatorState::default(),
            command_catalog: Vec::new(),
            command_runs: HashMap::new(),
            port_registry: crate::ports::PortRegistry::default(),
            copy_mode: None,
            workspace_scroll: 0,
            agent_panel_scroll: 0,
            tab_scroll: 0,
            tab_scroll_follow_active: true,
            hovered_tab: None,
            mobile_switcher_scroll: 0,
            view: state::ViewState {
                layout: state::ViewLayout::Desktop,
                sidebar_rect: Rect::default(),
                right_sidebar_rect: Rect::default(),
                workspace_card_areas: Vec::new(),
                workspace_group_header_areas: Vec::new(),
                workspace_group_empty_areas: Vec::new(),
                tab_bar_rect: Rect::default(),
                tab_hit_areas: Vec::new(),
                tab_close_hit_areas: Vec::new(),
                tab_scroll_left_hit_area: Rect::default(),
                tab_scroll_right_hit_area: Rect::default(),
                new_tab_hit_area: Rect::default(),
                terminal_area: Rect::default(),
                mobile_header_rect: Rect::default(),
                mobile_menu_hit_area: Rect::default(),
                toast_hit_area: Rect::default(),
                pane_infos: Vec::new(),
                split_borders: Vec::new(),
            },
            drag: None,
            workspace_press: None,
            group_press: None,
            tab_press: None,
            selection: None,
            selection_autoscroll: None,
            context_menu: None,
            update_available,
            update_install_command,
            latest_release_notes_available,
            update_dismissed: false,
            config_diagnostic,
            toast: None,
            pending_agent_notifications: std::collections::HashMap::new(),
            copy_feedback: None,
            outer_terminal_focus: None,
            prefix_code,
            prefix_mods,
            default_sidebar_width: config.ui.sidebar_width,
            sidebar_width,
            sidebar_min_width,
            sidebar_max_width,
            mobile_width_threshold: config.ui.mobile_width_threshold,
            sidebar_width_source,
            sidebar_width_auto: false,
            sidebar_collapsed,
            right_sidebar_width,
            right_sidebar_collapsed,
            sidebar_arrangement: config.ui.sidebar_arrangement,
            sidebar_section_split,
            activity_agents_expanded: true,
            activity_commands_expanded: false,
            activity_ports_expanded: false,
            collapsed_agent_sections: Vec::new(),
            collapsed_command_groups: Vec::new(),
            collapsed_command_status_groups: Vec::new(),
            collapsed_workspace_groups: Vec::new(),
            agent_panel_scope,
            mouse_capture: config.ui.mouse_capture,
            right_click_passthrough_modifiers: config.ui.right_click_passthrough_modifiers(),
            right_click_passthrough: None,
            redraw_on_focus_gained: config.ui.redraw_on_focus_gained,
            mouse_scroll_lines: config.ui.mouse_scroll_lines(),
            confirm_close: config.ui.confirm_close,
            prompt_new_tab_name: config.ui.prompt_new_tab_name,
            git_diff_command: config.git.diff_command.clone(),
            show_agent_labels_on_pane_borders: config.ui.show_agent_labels_on_pane_borders,
            pane_history_persistence: config.experimental.pane_history,
            resume_agents_on_restore: config.session.resume_agents_on_restore,
            reveal_hidden_cursor_for_cjk_ime: config.experimental.reveal_hidden_cursor_for_cjk_ime,
            cjk_ime_agent_filter_configured: !config.experimental.cjk_ime_agents.is_empty(),
            cjk_ime_agents: parse_cjk_ime_agents(&config.experimental.cjk_ime_agents),
            cjk_ime_cursor_shape: config.experimental.cjk_ime_cursor_shape.to_decscusr(),
            switch_ascii_input_source_in_prefix: config
                .experimental
                .switch_ascii_input_source_in_prefix,
            kitty_graphics_enabled: config.experimental.kitty_graphics,
            default_shell: config.terminal.default_shell.clone(),
            shell_mode: config.terminal.shell_mode,
            new_terminal_cwd: config.terminal.new_cwd.clone(),
            pane_scrollback_limit_bytes: config.advanced.scrollback_limit_bytes,
            worktree_directory,
            accent: crate::config::parse_color(&config.ui.accent),
            sound: config.ui.sound.clone(),
            local_sound_playback: true,
            toast_config: config.ui.toast.clone(),
            agent_profiles: crate::agent_profiles::AgentProfileCatalog::from_config(
                &config.agent_profiles,
            ),
            keybinds: config.keybinds(),
            spinner_tick: 0,
            palette: global_palette.clone(),
            global_palette,
            theme_name: global_theme_name.clone(),
            global_theme_name,
            global_theme_mode,
            global_light_theme_name,
            global_dark_theme_name,
            global_terminal_light_accent: config.theme.resolved_terminal_light_accent(),
            global_terminal_dark_accent: config.theme.resolved_terminal_dark_accent(),
            global_theme_custom: config.theme.custom.clone(),
            global_theme_use_legacy_ui_accent: config.ui.accent != "cyan"
                && config
                    .theme
                    .custom
                    .as_ref()
                    .and_then(|custom| custom.accent.as_ref())
                    .is_none(),
            settings: state::SettingsState {
                section: state::SettingsSection::Theme,
                list: state::SelectionListState::new(0),
                selection_active: false,
                scroll: 0,
                original_palette: None,
                original_theme: None,
                pending_theme_name: None,
                pending_theme_mode: None,
                pending_light_theme_name: None,
                pending_dark_theme_name: None,
                pending_terminal_light_accent: None,
                pending_terminal_dark_accent: None,
                pending_sound_enabled: None,
                pending_toast_delivery: None,
                pending_confirm_close: None,
                pending_prompt_new_tab_name: None,
                pending_new_terminal_cwd: None,
                pending_mouse_scroll_lines: None,
                pending_sidebar_width: None,
                pending_sidebar_min_width: None,
                pending_sidebar_max_width: None,
                pending_sidebar_arrangement: None,
                pending_worktree_directory: None,
                pending_agent_border_labels: None,
                pending_switch_ascii_input_source_in_prefix: None,
                pending_group_accent_choice: None,
                pending_group_name: None,
                pending_group_default_directory: None,
                pending_workspace_name: None,
                pending_workspace_default_cwd: None,
                pending_agent_profile_id: None,
                pending_agent_profile_name: None,
                pending_agent_profile_kind: None,
                pending_agent_profile_command: None,
                agent_profile_kind_filter: None,
                group_settings_target: None,
                workspace_settings_target: None,
            },
            integration_recommendations: crate::integration::integration_recommendations(),
            agent_manifest_summaries,
            agent_manifest_update_status: crate::detect::manifest_update::load_status(),
            integration_install_messages: Vec::new(),
            installed_plugins: load_plugin_registry(no_session),
            plugin_panes: std::collections::HashMap::new(),
            plugin_command_logs: Vec::new(),
            next_plugin_command_log_id: 1,
            plugin_commands_in_flight: 0,
            global_menu: state::MenuListState::new(0),
            group_menu: state::MenuListState::new(0),
            agent_menu: state::MenuListState::new(0),
            host_terminal_theme,
            session_dirty: false,
            terminal_runtime_shutdowns: Vec::new(),
        };

        state.terminals = restored_terminals;

        for ws_idx in 0..state.workspaces.len() {
            let cwd = state.workspaces[ws_idx]
                .resolved_identity_cwd_from(&state.terminals, &restored_terminal_runtimes);
            state.workspaces[ws_idx].cached_git_branch =
                cwd.as_deref().and_then(crate::workspace::git_branch);
        }

        if state.group_filter_enabled
            && state
                .active
                .is_some_and(|idx| !state.workspace_in_active_group(idx))
        {
            state.active = state.first_visible_workspace();
            state.selected = state.active.unwrap_or(0);
            if state.active.is_none() && state.mode == state::Mode::Terminal {
                state.mode = state::Mode::Navigate;
            }
        }
        state.apply_effective_theme();

        // Background auto-update is disabled in monolithic no-session mode
        // and in debug/test builds so local development never mutates the
        // running binary out from under spawned test processes.
        if auto_updates_enabled(no_session) {
            let update_tx = event_tx.clone();
            std::thread::spawn(move || crate::update::auto_update(update_tx));
            let manifest_update_tx = event_tx.clone();
            std::thread::spawn(move || {
                crate::detect::manifest_update::auto_update(manifest_update_tx)
            });
        }

        let last_focus = state.active.and_then(|idx| {
            state
                .workspaces
                .get(idx)
                .and_then(|ws| ws.focused_pane_id().map(|pane_id| (idx, pane_id)))
        });

        let default_client_view = ClientViewState::from_app_state(&state);

        Self {
            config_diagnostic_deadline: None,
            toast_deadline: None,
            copy_feedback_deadline: None,
            state,
            default_client_view,
            terminal_runtimes: restored_terminal_runtimes,
            event_tx,
            event_rx,
            last_git_remote_status_refresh: Instant::now() - GIT_REMOTE_STATUS_REFRESH_INTERVAL,
            git_refresh_in_flight: false,
            git_refresh_due_after_in_flight: false,
            git_status_cache: HashMap::new(),
            last_sidebar_divider_click: None,
            last_pane_click: None,
            next_resize_poll: Instant::now() + RESIZE_POLL_INTERVAL,
            next_port_scan: Instant::now() + PORT_SCAN_INTERVAL,
            next_command_scan: Instant::now(),
            next_animation_tick: None,
            next_auto_update_check: auto_updates_enabled(no_session)
                .then_some(Instant::now() + AUTO_UPDATE_CHECK_INTERVAL),
            next_agent_manifest_update_check: auto_updates_enabled(no_session)
                .then_some(Instant::now() + AUTO_UPDATE_CHECK_INTERVAL),
            agent_metadata_deadline: None,
            last_api_notification_at: None,
            pending_agent_resume_deadline: None,
            session_save_deadline: None,
            selection_autoscroll_deadline: None,
            selection_highlight_clear_deadline: None,
            persist_pane_history: config.experimental.pane_history,
            last_render_at: None,
            suppressed_repeat_keys: HashSet::new(),
            api_rx,
            event_hub,
            last_focus,
            no_session,
            input_rx: None,
            last_terminal_size: terminal::size().ok(),
            render_notify,
            render_dirty,
            full_redraw_pending: false,
            overlay_panes: HashMap::new(),
            local_terminal_notifications: true,
            config_reloaded_from_disk: false,
            prefix_input_source: Box::new(crate::platform::RealPrefixInputSource::default()),
            #[cfg(test)]
            host_terminal_theme_query_count: std::cell::Cell::new(0),
        }
    }

    #[cfg(unix)]
    pub fn new_from_handoff(
        config: &Config,
        config_diagnostic: Option<String>,
        api_rx: tokio::sync::mpsc::UnboundedReceiver<crate::api::ApiRequestMessage>,
        event_hub: crate::api::EventHub,
        snapshot: &crate::persist::SessionSnapshot,
        imports: &mut std::collections::HashMap<
            u32,
            crate::handoff_runtime::ImportedHandoffRuntime,
        >,
    ) -> io::Result<Self> {
        let mut app = Self::new(config, true, config_diagnostic, api_rx, event_hub);
        let (workspaces, terminals, runtimes) = crate::persist::restore_handoff(
            snapshot,
            config.advanced.scrollback_limit_bytes,
            &config.terminal.default_shell,
            config.terminal.shell_mode,
            imports,
            app.event_tx.clone(),
            app.render_notify.clone(),
            app.render_dirty.clone(),
        )?;
        let pane_id_aliases = crate::persist::handoff_pane_aliases(snapshot, &workspaces);

        let groups = groups_from_snapshot(snapshot);
        app.no_session = false;
        if auto_updates_enabled(app.no_session) {
            let now = Instant::now();
            app.next_auto_update_check = app
                .state
                .update_available
                .is_none()
                .then_some(now + AUTO_UPDATE_CHECK_INTERVAL);
            app.next_agent_manifest_update_check = Some(now + AUTO_UPDATE_CHECK_INTERVAL);
        }
        app.state.detach_exits = false;
        app.state.pane_id_aliases = pane_id_aliases;
        app.state.public_pane_id_aliases.clear();
        app.state.groups = groups;
        app.state.active_group = snapshot
            .active_group
            .min(app.state.groups.len().saturating_sub(1));
        app.state.group_filter_enabled = snapshot.group_filter_enabled;
        app.state.workspaces = workspaces;
        app.state.terminals = terminals;
        app.terminal_runtimes = runtimes.into();
        app.state.active = snapshot
            .default_view
            .active
            .filter(|&idx| idx < app.state.workspaces.len());
        app.state.selected = snapshot
            .default_view
            .selected
            .min(app.state.workspaces.len().saturating_sub(1));
        app.state.agent_panel_scope = snapshot.default_view.agent_panel_scope;
        if let Some(width) = snapshot.default_view.sidebar_width {
            app.state.sidebar_width = width;
            app.state.sidebar_width_source = state::SidebarWidthSource::Persisted;
        }
        app.state.sidebar_collapsed = snapshot.default_view.sidebar_collapsed;
        if let Some(split) = snapshot.default_view.sidebar_section_split {
            app.state.sidebar_section_split = split;
        }
        if let Some(width) = snapshot.default_view.right_sidebar_width {
            app.state.right_sidebar_width = width;
        }
        app.state.right_sidebar_collapsed = snapshot.default_view.right_sidebar_collapsed;
        app.state.workspace_scroll = snapshot.default_view.ui.workspace_scroll;
        app.state.agent_panel_scroll = snapshot.default_view.ui.agent_panel_scroll;
        app.state.tab_scroll = snapshot.default_view.ui.tab_scroll;
        app.state.mobile_switcher_scroll = snapshot.default_view.ui.mobile_switcher_scroll;
        app.state.activity_agents_expanded = snapshot.default_view.ui.activity_agents_expanded;
        app.state.activity_commands_expanded = snapshot.default_view.ui.activity_commands_expanded;
        app.state.activity_ports_expanded = snapshot.default_view.ui.activity_ports_expanded;
        app.state.collapsed_agent_sections =
            snapshot.default_view.ui.collapsed_agent_sections.clone();
        app.state.collapsed_command_groups =
            snapshot.default_view.ui.collapsed_command_groups.clone();
        app.state.collapsed_command_status_groups = snapshot
            .default_view
            .ui
            .collapsed_command_status_groups
            .clone();
        app.state.collapsed_workspace_groups =
            snapshot.default_view.ui.collapsed_workspace_groups.clone();
        if app.state.group_filter_enabled
            && app
                .state
                .active
                .is_some_and(|idx| !app.state.workspace_in_active_group(idx))
        {
            app.state.active = app.state.first_visible_workspace();
            app.state.selected = app.state.active.unwrap_or(0);
        }
        app.state.mode = if app.state.active.is_some() {
            state::Mode::Terminal
        } else {
            state::Mode::Navigate
        };
        app.default_client_view = ClientViewState::from_app_state(&app.state);
        app.last_focus = app.state.active.and_then(|idx| {
            app.state
                .workspaces
                .get(idx)
                .and_then(|ws| ws.focused_pane_id().map(|pane_id| (idx, pane_id)))
        });
        Ok(app)
    }

    #[cfg(unix)]
    pub fn unpause_handoff_readers(&self) {
        self.terminal_runtimes.set_handoff_readers_paused(false);
    }

    #[cfg(unix)]
    pub fn assume_handoff_ownership(&mut self) {
        self.terminal_runtimes.assume_handoff_ownership();
    }

    fn request_full_redraw(&mut self) {
        self.full_redraw_pending = true;
    }

    pub(crate) fn sync_prefix_input_source(&mut self, previous_mode: Mode) {
        match (
            previous_mode == Mode::Prefix,
            self.state.mode == Mode::Prefix,
        ) {
            (false, true) if self.state.switch_ascii_input_source_in_prefix => {
                self.prefix_input_source.switch_to_ascii();
            }
            (true, false) => self.prefix_input_source.restore(),
            _ => {}
        }
    }

    pub(crate) fn handle_internal_event_with_prefix_sync(
        &mut self,
        event: crate::events::AppEvent,
    ) {
        let previous_mode = self.state.mode;
        self.handle_internal_event(event);
        self.sync_prefix_input_source(previous_mode);
    }

    pub(crate) fn process_deferred_workspace_requests(&mut self) -> bool {
        let mut changed = false;

        if self.state.request_complete_onboarding {
            self.state.request_complete_onboarding = false;
            self.open_settings_from_onboarding();
            changed = true;
        }

        if self.state.request_new_workspace {
            self.state.request_new_workspace = false;
            self.create_workspace();
            changed = true;
        }

        if self.state.request_new_tab {
            self.state.request_new_tab = false;
            self.create_tab();
            changed = true;
        }

        changed | self.process_agent_profile_tab_request()
    }

    fn process_agent_profile_tab_request(&mut self) -> bool {
        let Some((ws_idx, profile_id)) = self.state.request_agent_profile_tab.take() else {
            return false;
        };
        let previous_toast = self.state.toast.clone();

        match self.create_agent_profile_tab(ws_idx, &profile_id) {
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(profile = %profile_id, err = %err, "failed to launch agent profile");
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::NeedsAttention,
                    title: "agent launch failed".to_string(),
                    context: err.to_string(),
                    position: None,
                    target: None,
                });
                self.sync_toast_deadline(previous_toast);
            }
        }

        true
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        if self.input_rx.is_none() {
            self.input_rx = Some(crate::raw_input::spawn_input_reader());
        }
        self.query_host_terminal_theme();

        let mut needs_render = true;
        let mut host_mouse_capture_active = self.state.mouse_capture;

        while !self.state.should_quit {
            if self.render_dirty.load(Ordering::Acquire) {
                needs_render = true;
            }

            // Drain internal events first so API reads observe fresh pane state.
            if self.drain_internal_events() {
                needs_render = true;
            }
            if self.drain_api_requests() {
                needs_render = true;
            }

            self.sync_focus_events();
            self.sync_session_save_schedule();

            let now = Instant::now();
            if self.handle_scheduled_tasks(now, needs_render) {
                needs_render = true;
            }

            if self.process_deferred_workspace_requests() {
                needs_render = true;
            }

            if self.state.request_reload_config {
                self.state.request_reload_config = false;
                self.reload_config();
                needs_render = true;
            }

            if self.state.request_open_git_diff_command {
                self.state.request_open_git_diff_command = false;
                self.refresh_host_terminal_theme_for(Duration::from_millis(500))
                    .await;
                let previous_toast = self.state.toast.clone();
                if let Err(err) = self
                    .state
                    .open_git_diff_command(&mut self.terminal_runtimes)
                {
                    self.state.toast = Some(crate::app::state::ToastNotification {
                        kind: crate::app::state::ToastKind::NeedsAttention,
                        title: "git diff command failed".to_string(),
                        context: err,
                        position: None,
                        target: None,
                    });
                    self.sync_toast_deadline(previous_toast);
                }
                needs_render = true;
            }

            let now = Instant::now();
            self.sync_animation_timer(now);
            self.sync_host_mouse_capture(&mut host_mouse_capture_active)?;

            if needs_render && self.can_render_now(now) {
                self.render_dirty.swap(false, Ordering::AcqRel);
                let _sync_output = SyncOutputGuard::begin()?;
                let kitty_graphics_enabled = self.state.kitty_graphics_enabled;
                if self.full_redraw_pending {
                    if kitty_graphics_enabled {
                        crate::kitty_graphics::clear_all_host_graphics()?;
                    }
                    terminal.clear()?;
                    self.full_redraw_pending = false;
                }
                let mut cell_size = crate::kitty_graphics::HostCellSize::default();
                terminal.draw(|frame| {
                    let area = frame.area();
                    if kitty_graphics_enabled {
                        cell_size = crate::kitty_graphics::HostCellSize::from_terminal(area);
                        crate::ui::compute_view_with_cell_size(
                            &mut self.state,
                            &self.terminal_runtimes,
                            area,
                            cell_size,
                        );
                    } else {
                        crate::ui::compute_view_with_runtime_registry(
                            &mut self.state,
                            &self.terminal_runtimes,
                            area,
                        );
                    }
                    crate::ui::render_with_runtime_registry(
                        &self.state,
                        &self.terminal_runtimes,
                        frame,
                    );
                })?;
                if kitty_graphics_enabled {
                    crate::kitty_graphics::paint_local_pane_graphics(
                        &self.state,
                        &self.terminal_runtimes,
                        cell_size,
                    )?;
                }
                self.sync_pending_agent_resume_deadline(now);
                if self.start_pending_agent_resumes(self.pending_agent_resume_due(now)) {
                    self.render_dirty.store(true, Ordering::Release);
                    self.render_notify.notify_one();
                }
                self.last_render_at = Some(now);
                needs_render = false;
                continue;
            }

            let next_deadline = self.next_loop_deadline(now, needs_render);
            let event = {
                let input_rx = self.input_rx.as_mut();
                tokio::select! {
                    maybe_api = self.api_rx.recv() => match maybe_api {
                        Some(msg) => LoopEvent::Api(Box::new(msg)),
                        None => LoopEvent::Timer,
                    },
                    maybe_ev = self.event_rx.recv() => match maybe_ev {
                        Some(ev) => LoopEvent::Internal(ev),
                        None => LoopEvent::Timer,
                    },
                    maybe_input = recv_raw_input_or_pending(input_rx) => match maybe_input {
                        Some(input) => LoopEvent::RawInput(input),
                        None => LoopEvent::InputClosed,
                    },
                    _ = sleep_until_or_pending(next_deadline) => LoopEvent::Timer,
                    _ = self.render_notify.notified() => LoopEvent::RenderRequested,
                }
            };

            match event {
                LoopEvent::Timer => {}
                LoopEvent::Internal(ev) => {
                    self.handle_internal_event_with_prefix_sync(ev);
                    needs_render = true;
                }
                LoopEvent::Api(msg) => {
                    if self.handle_api_request_message(*msg) {
                        needs_render = true;
                    }
                }
                LoopEvent::RawInput(input) => {
                    if self.handle_raw_input_batch(input).await {
                        needs_render = true;
                    }
                }
                LoopEvent::InputClosed => {
                    self.input_rx = None;
                }
                LoopEvent::RenderRequested => {
                    if self.render_dirty.load(Ordering::Acquire) {
                        needs_render = true;
                    }
                }
            }
        }

        // Save session on exit (skip in --no-session mode)
        if !self.no_session {
            self.save_session_now();
        }

        Ok(())
    }

    fn sync_host_mouse_capture(&self, active: &mut bool) -> io::Result<()> {
        let view = self.default_client_view.clone_reconciled(&self.state);
        let desired = self
            .state
            .should_capture_host_mouse_from_view(&self.terminal_runtimes, &view);
        if desired == *active {
            return Ok(());
        }
        if desired {
            execute!(io::stdout(), EnableMouseCapture)?;
        } else {
            execute!(io::stdout(), DisableMouseCapture)?;
        }
        *active = desired;
        Ok(())
    }

    pub(crate) fn dismiss_release_notes(&mut self) {
        let preview = self
            .state
            .release_notes
            .as_ref()
            .is_some_and(|notes| notes.preview);

        self.state.release_notes = None;
        if !preview {
            if let Err(err) = crate::release_notes::mark_current_version_seen() {
                self.state.config_diagnostic =
                    Some(format!("failed to update release notes status: {err}"));
                self.config_diagnostic_deadline = Some(Instant::now() + Duration::from_secs(5));
            }
        }

        if self.state.product_announcement.is_some() {
            self.state.mode = Mode::ProductAnnouncement;
        } else {
            self.state.mode = if self.state.active.is_some() {
                Mode::Terminal
            } else {
                Mode::Navigate
            };
        }
    }

    pub(crate) fn dismiss_product_announcement(&mut self) {
        if let Some(announcement) = self.state.product_announcement.take() {
            if !announcement.preview {
                if let Err(err) =
                    crate::product_announcements::mark_seen(&announcement.version, &announcement.id)
                {
                    self.state.config_diagnostic =
                        Some(format!("failed to update announcement status: {err}"));
                    self.config_diagnostic_deadline = Some(Instant::now() + Duration::from_secs(5));
                }
            }
        }

        self.state.mode = if self.state.active.is_some() {
            Mode::Terminal
        } else {
            Mode::Navigate
        };
    }

    pub(crate) fn scroll_release_notes(&mut self, delta: i16) {
        let max_scroll = self.state.release_notes_max_scroll();
        if let Some(notes) = &mut self.state.release_notes {
            notes.scroll = if delta.is_negative() {
                notes.scroll.saturating_sub(delta.unsigned_abs())
            } else {
                notes.scroll.saturating_add(delta as u16)
            }
            .min(max_scroll);
        }
    }

    pub(crate) fn scroll_product_announcement(&mut self, delta: i16) {
        let max_scroll = self.state.product_announcement_max_scroll();
        if let Some(announcement) = &mut self.state.product_announcement {
            announcement.scroll = if delta.is_negative() {
                announcement.scroll.saturating_sub(delta.unsigned_abs())
            } else {
                announcement.scroll.saturating_add(delta as u16)
            }
            .min(max_scroll);
        }
    }

    pub(crate) fn open_settings_from_onboarding(&mut self) {
        self.mark_onboarding_complete();
        self.refresh_integration_recommendations();
        crate::app::input::open_settings_at(&mut self.state, state::SettingsSection::Integrations);
    }

    pub(crate) fn refresh_integration_recommendations(&mut self) {
        self.state.integration_recommendations = crate::integration::integration_recommendations();
    }

    pub(crate) fn install_integration(&mut self, target: crate::api::schema::IntegrationTarget) {
        let label = crate::integration::integration_target_label(target);
        self.state.integration_install_messages.clear();
        match crate::integration::install_target(target) {
            Ok(messages) => {
                self.state
                    .integration_install_messages
                    .push(format!("installed {label}"));
                self.state
                    .integration_install_messages
                    .extend(messages.into_iter().filter(|message| {
                        message.starts_with(crate::integration::INSTALL_WARNING_PREFIX)
                    }));
            }
            Err(err) => self
                .state
                .integration_install_messages
                .push(format!("{label}: {err}")),
        }
        self.state.integration_recommendations = crate::integration::integration_recommendations();
        self.state.mark_session_dirty();
    }

    pub(crate) fn uninstall_integration(&mut self, target: crate::api::schema::IntegrationTarget) {
        let label = crate::integration::integration_target_label(target);
        self.state.integration_install_messages.clear();
        match crate::integration::uninstall_target(target) {
            Ok(messages) => self.state.integration_install_messages.extend(messages),
            Err(err) => self
                .state
                .integration_install_messages
                .push(format!("{label}: {err}")),
        }
        self.state.integration_recommendations = crate::integration::integration_recommendations();
        self.state.mark_session_dirty();
    }

    pub(crate) fn reload_config(&mut self) -> crate::config::ConfigReloadReport {
        self.apply_config_from_disk(true)
    }

    pub(crate) fn take_config_reloaded_from_disk(&mut self) -> bool {
        let reloaded = self.config_reloaded_from_disk;
        self.config_reloaded_from_disk = false;
        reloaded
    }

    pub(crate) fn apply_config_from_disk(
        &mut self,
        notify_success: bool,
    ) -> crate::config::ConfigReloadReport {
        self.config_reloaded_from_disk = true;
        let previous_toast = self.state.toast.clone();
        let report = match crate::config::load_live_config() {
            Ok(loaded) => self.apply_live_config(
                &loaded.config,
                &loaded.diagnostics,
                &loaded.invalid_sections,
                notify_success,
            ),
            Err(diagnostics) => {
                self.state.toast = None;
                self.state.config_diagnostic =
                    crate::config::config_diagnostic_summary(&diagnostics);
                self.config_diagnostic_deadline = None;
                crate::config::ConfigReloadReport {
                    status: crate::config::ConfigReloadStatus::Failed,
                    diagnostics,
                }
            }
        };
        self.sync_toast_deadline(previous_toast);
        report
    }

    fn apply_live_config(
        &mut self,
        config: &crate::config::Config,
        load_diagnostics: &[String],
        invalid_sections: &[String],
        notify_success: bool,
    ) -> crate::config::ConfigReloadReport {
        let mut diagnostics = load_diagnostics.to_vec();
        let invalid_section =
            |section: &str| invalid_sections.iter().any(|invalid| invalid == section);

        if !invalid_section("keys") {
            match config.live_keybinds() {
                Ok(live) => {
                    self.state.prefix_code = live.prefix.0;
                    self.state.prefix_mods = live.prefix.1;
                    self.state.keybinds = live.keybinds;
                }
                Err(keybind_diagnostics) => {
                    diagnostics.extend(
                        keybind_diagnostics
                            .into_iter()
                            .map(|diagnostic| format!("{diagnostic}; kept current keybinds")),
                    );
                }
            }
        }

        if !invalid_section("ui") {
            // Validate sidebar bounds before they reach any `u16::clamp` call.
            // On `min > max`, treat the entire `[ui]` section as invalid: keep
            // the previous settings and skip the section so the re-clamp below
            // — and every subsequent render/drag — can never panic.
            if crate::config::validated_sidebar_bounds(
                config.ui.sidebar_min_width,
                config.ui.sidebar_max_width,
            )
            .is_none()
            {
                diagnostics.push(format!(
                    "ui.sidebar_min_width ({}) is greater than sidebar_max_width ({}); keeping previous [ui] settings",
                    config.ui.sidebar_min_width, config.ui.sidebar_max_width,
                ));
            } else {
                diagnostics.extend(config.ui.sound.diagnostics());

                self.state.default_sidebar_width = config.ui.sidebar_width;
                if self.state.sidebar_width_source == state::SidebarWidthSource::ConfigDefault {
                    self.state.sidebar_width = config.ui.sidebar_width;
                }
                self.state.sidebar_min_width = config.ui.sidebar_min_width;
                self.state.sidebar_max_width = config.ui.sidebar_max_width;
                self.state.mobile_width_threshold = config.ui.mobile_width_threshold;
                self.state.sidebar_arrangement = config.ui.sidebar_arrangement;
                // Re-clamp the live width to the new bounds. No source guard — bounds
                // always apply, including to widths owned by Persisted or Manual.
                self.state.sidebar_width = self
                    .state
                    .sidebar_width
                    .clamp(self.state.sidebar_min_width, self.state.sidebar_max_width);
                self.state.mouse_capture = config.ui.mouse_capture;
                self.state.right_click_passthrough_modifiers =
                    config.ui.right_click_passthrough_modifiers();
                if self.state.redraw_on_focus_gained != config.ui.redraw_on_focus_gained {
                    self.state.request_client_config_reload = true;
                }
                self.state.redraw_on_focus_gained = config.ui.redraw_on_focus_gained;
                self.state.mouse_scroll_lines = config.ui.mouse_scroll_lines();
                self.state.confirm_close = config.ui.confirm_close;
                self.state.prompt_new_tab_name = config.ui.prompt_new_tab_name;
                self.state.show_agent_labels_on_pane_borders =
                    config.ui.show_agent_labels_on_pane_borders;
                self.state.agent_panel_scope =
                    agent_panel_scope_from_config(config.ui.agent_panel_scope);
                self.state.sidebar_arrangement = config.ui.sidebar_arrangement;
                self.state.agent_panel_scroll = 0;
                self.state.accent = crate::config::parse_color(&config.ui.accent);
                if !self.state.local_sound_playback && self.state.sound != config.ui.sound {
                    self.state.request_client_config_reload = true;
                }
                self.state.sound = config.ui.sound.clone();
                self.state.toast_config = config.ui.toast.clone();
            }
        }

        if !invalid_section("experimental") {
            let was_kitty_graphics_enabled = self.state.kitty_graphics_enabled;
            self.state.kitty_graphics_enabled = config.experimental.kitty_graphics;
            crate::kitty_graphics::set_enabled(config.experimental.kitty_graphics);
            if was_kitty_graphics_enabled && !config.experimental.kitty_graphics {
                let _ = crate::kitty_graphics::clear_all_host_graphics();
            }
            self.state.reveal_hidden_cursor_for_cjk_ime =
                config.experimental.reveal_hidden_cursor_for_cjk_ime;
            self.state.cjk_ime_agent_filter_configured =
                !config.experimental.cjk_ime_agents.is_empty();
            self.state.cjk_ime_agents = parse_cjk_ime_agents(&config.experimental.cjk_ime_agents);
            self.state.cjk_ime_cursor_shape =
                config.experimental.cjk_ime_cursor_shape.to_decscusr();
            self.state.switch_ascii_input_source_in_prefix =
                config.experimental.switch_ascii_input_source_in_prefix;
            self.persist_pane_history = config.experimental.pane_history;
            self.state.pane_history_persistence = config.experimental.pane_history;
            self.state.resume_agents_on_restore = config.session.resume_agents_on_restore;
            if !self.persist_pane_history {
                crate::persist::clear_history();
            }
        }

        if !invalid_section("advanced") {
            self.state.pane_scrollback_limit_bytes = config.advanced.scrollback_limit_bytes;
        }

        if !invalid_section("agent_profiles") {
            self.state.agent_profiles =
                crate::agent_profiles::AgentProfileCatalog::from_config(&config.agent_profiles);
        }

        if !invalid_section("terminal") {
            self.state.default_shell = config.terminal.default_shell.clone();
            self.state.shell_mode = config.terminal.shell_mode;
            self.state.new_terminal_cwd = config.terminal.new_cwd.clone();
        }

        if !invalid_section("worktrees") {
            self.state.worktree_directory =
                crate::worktree::expand_tilde_absolute_path(&config.worktrees.directory);
        }
        if !invalid_section("theme") {
            let (global_light_theme_name, global_dark_theme_name) =
                state::theme_config_names(&config.theme);
            self.state.global_light_theme_name = global_light_theme_name;
            self.state.global_dark_theme_name = global_dark_theme_name;
            self.state.global_theme_mode = config.theme.mode;
            self.state.global_theme_name = self
                .state
                .global_theme_name_for_mode(self.state.global_theme_mode)
                .to_string();
            self.state.global_terminal_light_accent = config.theme.resolved_terminal_light_accent();
            self.state.global_terminal_dark_accent = config.theme.resolved_terminal_dark_accent();
            self.state.global_theme_custom = config.theme.custom.clone();
            self.state.global_theme_use_legacy_ui_accent = !invalid_section("ui")
                && config.ui.accent != "cyan"
                && config
                    .theme
                    .custom
                    .as_ref()
                    .and_then(|custom| custom.accent.as_ref())
                    .is_none();
            self.state.global_palette = resolve_palette_with_legacy_accent(
                config,
                !invalid_section("ui"),
                self.state.host_terminal_theme,
            );
            self.state.apply_effective_theme();
            self.query_host_terminal_theme();
        }

        let status = if diagnostics.is_empty() {
            crate::config::ConfigReloadStatus::Applied
        } else {
            crate::config::ConfigReloadStatus::Partial
        };

        if diagnostics.is_empty() {
            self.state.config_diagnostic = None;
            self.config_diagnostic_deadline = None;
            if notify_success {
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::UpdateInstalled,
                    title: "reloaded config".to_string(),
                    context: "using config.toml".to_string(),
                    position: None,
                    target: None,
                });
            }
        } else {
            self.state.config_diagnostic = crate::config::config_diagnostic_summary(&diagnostics);
            self.config_diagnostic_deadline = None;
            if notify_success {
                self.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::UpdateInstalled,
                    title: "reloaded config".to_string(),
                    context: "with warnings".to_string(),
                    position: None,
                    target: None,
                });
            }
        }

        crate::config::ConfigReloadReport {
            status,
            diagnostics,
        }
    }
}

// ---------------------------------------------------------------------------
// Input routing for headless server mode
// ---------------------------------------------------------------------------

impl App {
    /// Routes raw input bytes from a client through the existing input pipeline.
    ///
    /// The input bytes are parsed into `RawInputEvent`s and then processed.
    /// In terminal mode, keys are routed through the same semantic
    /// key-handling path as monolithic hako so they are re-encoded for the
    /// focused pane's negotiated keyboard protocol instead of passing host
    /// terminal escape sequences through unchanged.
    #[cfg(test)]
    pub(crate) fn route_client_input(&mut self, data: Vec<u8>) {
        let events = crate::raw_input::parse_raw_input_bytes_sync(&data);
        self.route_client_events(events, true);
    }

    pub(crate) fn route_client_events(
        &mut self,
        events: Vec<crate::raw_input::RawInputEvent>,
        apply_host_terminal_theme: bool,
    ) {
        for event in events {
            let previous_mode = self.state.mode;
            match event {
                crate::raw_input::RawInputEvent::Key(key) => {
                    let key_id = repeat_key_identity(&key);
                    match key.kind {
                        crossterm::event::KeyEventKind::Press => {
                            if self.state.mode == Mode::Terminal {
                                self.suppressed_repeat_keys.remove(&key_id);
                                self.handle_terminal_key_headless(key);
                            } else {
                                self.suppressed_repeat_keys.insert(key_id);
                                self.handle_non_terminal_key_headless(key);
                            }
                        }
                        crossterm::event::KeyEventKind::Repeat => {
                            if self.state.mode == Mode::Terminal
                                && !self.suppressed_repeat_keys.contains(&key_id)
                            {
                                self.handle_terminal_key_headless(key);
                            } else if mode_accepts_repeat_key(self.state.mode, &key) {
                                self.handle_non_terminal_key_headless(key);
                            }
                        }
                        crossterm::event::KeyEventKind::Release => {
                            self.suppressed_repeat_keys.remove(&key_id);
                        }
                    }
                }
                crate::raw_input::RawInputEvent::Mouse(mouse) => {
                    if self.state.mouse_capture {
                        self.handle_mouse_event_headless(mouse);
                    } else {
                        self.state
                            .handle_pane_mouse_only(&self.terminal_runtimes, mouse);
                    }
                }
                crate::raw_input::RawInputEvent::Paste(text) => {
                    if self.state.mode != Mode::Terminal {
                        self.paste_into_active_text_input(&text);
                    } else if let Some(ws_idx) = self.state.active {
                        if let Some(ws) = self.state.workspaces.get(ws_idx) {
                            if let Some(focused) = ws.focused_pane_id() {
                                if let Some(runtime) = self.state.runtime_for_pane_in_workspace(
                                    &self.terminal_runtimes,
                                    ws_idx,
                                    focused,
                                ) {
                                    let _ = runtime.try_send_bytes(bytes::Bytes::from(
                                        if runtime
                                            .input_state()
                                            .map(|s| s.bracketed_paste)
                                            .unwrap_or(false)
                                        {
                                            format!("\x1b[200~{text}\x1b[201~")
                                        } else {
                                            text
                                        },
                                    ));
                                }
                            }
                        }
                    }
                }
                crate::raw_input::RawInputEvent::OuterFocusGained => {
                    if apply_host_terminal_theme {
                        self.query_host_terminal_theme();
                    }
                }
                crate::raw_input::RawInputEvent::OuterFocusLost => {}
                crate::raw_input::RawInputEvent::HostDefaultColor { kind, color } => {
                    if apply_host_terminal_theme {
                        self.update_host_terminal_theme(kind, color);
                    }
                }
                crate::raw_input::RawInputEvent::HostPaletteColor { index, color } => {
                    if apply_host_terminal_theme {
                        self.update_host_terminal_palette_color(index, color);
                    }
                }
                crate::raw_input::RawInputEvent::HostCursorColor { color } => {
                    if apply_host_terminal_theme {
                        self.update_host_terminal_cursor_color(color);
                    }
                }
                crate::raw_input::RawInputEvent::Unsupported => {}
            }
            self.sync_prefix_input_source(previous_mode);
        }
    }

    pub(crate) fn route_client_events_for_view(
        &mut self,
        client_view: &mut ClientViewState,
        events: Vec<crate::raw_input::RawInputEvent>,
        apply_host_terminal_theme: bool,
    ) {
        client_view.reconcile(&self.state);
        for event in events {
            let previous_mode = client_view.mode;
            match event {
                crate::raw_input::RawInputEvent::Key(key) => {
                    self.route_client_key_for_view(client_view, key);
                }
                crate::raw_input::RawInputEvent::OuterFocusGained => {
                    if apply_host_terminal_theme {
                        self.query_host_terminal_theme();
                    }
                }
                crate::raw_input::RawInputEvent::HostDefaultColor { kind, color } => {
                    if apply_host_terminal_theme {
                        self.update_host_terminal_theme(kind, color);
                    }
                }
                crate::raw_input::RawInputEvent::HostPaletteColor { index, color } => {
                    if apply_host_terminal_theme {
                        self.update_host_terminal_palette_color(index, color);
                    }
                }
                crate::raw_input::RawInputEvent::HostCursorColor { color } => {
                    if apply_host_terminal_theme {
                        self.update_host_terminal_cursor_color(color);
                    }
                }
                crate::raw_input::RawInputEvent::OuterFocusLost
                | crate::raw_input::RawInputEvent::Unsupported => {}
                crate::raw_input::RawInputEvent::Paste(text) => {
                    self.paste_for_view(client_view, &text);
                }
                crate::raw_input::RawInputEvent::Mouse(mouse) => {
                    self.handle_mouse_for_view(client_view, mouse);
                }
            }
            self.sync_prefix_input_source_for_mode_transition(previous_mode, client_view.mode);
        }
    }

    fn route_client_key_for_view(
        &mut self,
        client_view: &mut ClientViewState,
        key: crate::input::TerminalKey,
    ) {
        let key_id = repeat_key_identity(&key);
        match key.kind {
            crossterm::event::KeyEventKind::Press => {
                if client_view.mode == Mode::Terminal {
                    self.suppressed_repeat_keys.remove(&key_id);
                    self.handle_terminal_key_for_view(client_view, key);
                } else {
                    self.suppressed_repeat_keys.insert(key_id);
                    self.handle_non_terminal_key_for_view(client_view, key);
                }
            }
            crossterm::event::KeyEventKind::Repeat => {
                if client_view.mode == Mode::Terminal
                    && !self.suppressed_repeat_keys.contains(&key_id)
                {
                    self.handle_terminal_key_for_view(client_view, key);
                } else if mode_accepts_repeat_key(client_view.mode, &key) {
                    self.handle_non_terminal_key_for_view(client_view, key);
                }
            }
            crossterm::event::KeyEventKind::Release => {
                self.suppressed_repeat_keys.remove(&key_id);
            }
        }
    }

    fn handle_terminal_key_for_view(
        &mut self,
        client_view: &mut ClientViewState,
        key: crate::input::TerminalKey,
    ) {
        if self.state.is_prefix_key(key) {
            client_view.mode = Mode::Prefix;
            return;
        }

        debug!(mode = ?client_view.mode, key = ?key, "client view terminal key routing");
        if let Some(action) = input::terminal_direct_navigation_action(&self.state, key) {
            self.execute_client_view_navigate_action(
                client_view,
                action,
                input::ActionContext::Direct,
            );
            return;
        }

        self.send_terminal_key_for_view(client_view, key);
    }

    fn handle_non_terminal_key_for_view(
        &mut self,
        client_view: &mut ClientViewState,
        raw_key: crate::input::TerminalKey,
    ) {
        if client_view.mode != Mode::Prefix {
            self.handle_client_view_modal_key(client_view, raw_key);
            return;
        }

        self.apply_client_view_local_app_action(client_view, |app| {
            app.handle_prefix_key(raw_key);
        });
    }

    fn handle_client_view_modal_key(
        &mut self,
        client_view: &mut ClientViewState,
        raw_key: crate::input::TerminalKey,
    ) {
        let key = raw_key.as_key_event();
        match client_view.mode {
            Mode::RenameWorkspace | Mode::RenameGroup | Mode::RenameTab | Mode::RenamePane => {
                self.apply_client_view_local_key(client_view, |state| {
                    input::handle_rename_key(state, key);
                });
            }
            Mode::EditWorktreeDirectory => {
                self.apply_client_view_local_key(client_view, |state| {
                    input::handle_worktree_directory_key(state, key);
                });
            }
            Mode::KeybindHelp => {
                self.apply_client_view_local_key(client_view, |state| {
                    input::handle_keybind_help_key(state, key);
                });
            }
            Mode::Navigator => {
                if key.code == crossterm::event::KeyCode::Enter {
                    self.accept_client_view_navigator_selection(client_view);
                } else {
                    self.apply_client_view_local_key(client_view, |state| {
                        input::handle_navigator_key(state, key);
                    });
                }
            }
            Mode::CommandPalette => {
                if key.code == crossterm::event::KeyCode::Enter {
                    self.accept_client_view_command_palette_selection(client_view);
                } else {
                    input::handle_command_palette_key_for_view(&self.state, client_view, key);
                }
            }
            Mode::Settings => {
                if let Some(action) =
                    input::update_settings_state_for_view(&self.state, client_view, key)
                {
                    self.apply_settings_action(action);
                }
            }
            Mode::GlobalMenu => {
                if key.code == crossterm::event::KeyCode::Enter {
                    self.accept_client_view_global_menu_selection(client_view);
                } else {
                    self.apply_client_view_local_key(client_view, |state| {
                        input::handle_global_menu_key(state, key);
                    });
                }
            }
            Mode::GroupMenu => {
                if key.code == crossterm::event::KeyCode::Enter {
                    self.accept_client_view_group_menu_selection(client_view);
                } else {
                    self.apply_client_view_local_key(client_view, |state| {
                        input::handle_group_menu_key(state, key);
                    });
                }
            }
            Mode::AgentMenu => {
                if key.code == crossterm::event::KeyCode::Enter {
                    self.accept_client_view_agent_menu_selection(client_view);
                } else {
                    self.apply_client_view_local_key(client_view, |state| {
                        input::handle_agent_menu_key(state, key);
                    });
                }
            }
            Mode::ReleaseNotes => {
                self.apply_client_view_local_key(client_view, |state| match key.code {
                    crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                        let max_scroll = state.release_notes_max_scroll();
                        if let Some(notes) = &mut state.release_notes {
                            notes.scroll = notes.scroll.saturating_sub(1).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                        let max_scroll = state.release_notes_max_scroll();
                        if let Some(notes) = &mut state.release_notes {
                            notes.scroll = notes.scroll.saturating_add(1).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::PageUp => {
                        let max_scroll = state.release_notes_max_scroll();
                        if let Some(notes) = &mut state.release_notes {
                            notes.scroll = notes.scroll.saturating_sub(8).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::PageDown => {
                        let max_scroll = state.release_notes_max_scroll();
                        if let Some(notes) = &mut state.release_notes {
                            notes.scroll = notes.scroll.saturating_add(8).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::Home => {
                        if let Some(notes) = &mut state.release_notes {
                            notes.scroll = 0;
                        }
                    }
                    crossterm::event::KeyCode::End => {
                        let max_scroll = state.release_notes_max_scroll();
                        if let Some(notes) = &mut state.release_notes {
                            notes.scroll = max_scroll;
                        }
                    }
                    _ => {}
                });
            }
            Mode::ProductAnnouncement => {
                self.apply_client_view_local_key(client_view, |state| match key.code {
                    crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                        let max_scroll = state.product_announcement_max_scroll();
                        if let Some(announcement) = &mut state.product_announcement {
                            announcement.scroll =
                                announcement.scroll.saturating_sub(1).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                        let max_scroll = state.product_announcement_max_scroll();
                        if let Some(announcement) = &mut state.product_announcement {
                            announcement.scroll =
                                announcement.scroll.saturating_add(1).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::PageUp => {
                        let max_scroll = state.product_announcement_max_scroll();
                        if let Some(announcement) = &mut state.product_announcement {
                            announcement.scroll =
                                announcement.scroll.saturating_sub(8).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::PageDown => {
                        let max_scroll = state.product_announcement_max_scroll();
                        if let Some(announcement) = &mut state.product_announcement {
                            announcement.scroll =
                                announcement.scroll.saturating_add(8).min(max_scroll);
                        }
                    }
                    crossterm::event::KeyCode::Home => {
                        if let Some(announcement) = &mut state.product_announcement {
                            announcement.scroll = 0;
                        }
                    }
                    crossterm::event::KeyCode::End => {
                        let max_scroll = state.product_announcement_max_scroll();
                        if let Some(announcement) = &mut state.product_announcement {
                            announcement.scroll = max_scroll;
                        }
                    }
                    _ => {}
                });
            }
            Mode::AgentProfilePicker => {
                self.apply_client_view_local_key(client_view, |state| {
                    input::agent_profile_picker::handle_agent_profile_picker_key_for_view(
                        state, key,
                    );
                });
            }
            Mode::ConfirmClose
            | Mode::ConfirmDeleteGroup
            | Mode::ContextMenu
            | Mode::Onboarding
            | Mode::GitRepoPicker
            | Mode::Copy
            | Mode::Resize
            | Mode::Navigate
            | Mode::Prefix
            | Mode::Terminal => {}
        }
    }

    fn apply_client_view_local_key(
        &mut self,
        client_view: &mut ClientViewState,
        handle: impl FnOnce(&mut AppState),
    ) {
        let before_shared = self.state.clone();
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        handle(&mut local_state);
        let pending_tab_focus =
            Self::pending_client_tab_focus_after_deferred_request(&before_shared, &local_state);
        self.apply_shared_client_view_effects_from_local(&before_shared, local_state.clone());
        Self::copy_client_view_local_modal_state(client_view, local_state);
        client_view.reconcile(&self.state);
        if let Some((workspace_id, tab_idx)) = pending_tab_focus {
            client_view
                .pending_active_tabs
                .insert(workspace_id, tab_idx);
        }
    }

    fn apply_client_view_local_app_action(
        &mut self,
        client_view: &mut ClientViewState,
        handle: impl FnOnce(&mut Self),
    ) {
        let before_shared = self.state.clone();
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);

        let shared_state = std::mem::replace(&mut self.state, local_state);
        handle(self);
        let local_state = std::mem::replace(&mut self.state, shared_state);

        let pending_tab_focus =
            Self::pending_client_tab_focus_after_deferred_request(&before_shared, &local_state);
        self.apply_shared_client_view_effects_from_local(&before_shared, local_state.clone());
        *client_view = ClientViewState::from_app_state(&local_state);
        client_view.reconcile(&self.state);
        if let Some((workspace_id, tab_idx)) = pending_tab_focus {
            client_view
                .pending_active_tabs
                .insert(workspace_id, tab_idx);
        }
    }

    fn copy_client_view_local_modal_state(client_view: &mut ClientViewState, state: AppState) {
        client_view.mode = state.mode;
        client_view.settings = state.settings;
        client_view.command_palette = state.command_palette;
        client_view.navigator = state.navigator;
        client_view.agent_profile_picker = state.agent_profile_picker;
        client_view.keybind_help = state.keybind_help;
        client_view.global_menu = state.global_menu;
        client_view.group_menu = state.group_menu;
        client_view.agent_menu = state.agent_menu;
        client_view.creating_new_tab = state.creating_new_tab;
        client_view.creating_new_group = state.creating_new_group;
        client_view.group_icon_input = state.group_icon_input;
        client_view.group_default_directory_input = state.group_default_directory_input;
        client_view.group_modal_selected_field = state.group_modal_selected_field;
        client_view.group_icon_picker_open = state.group_icon_picker_open;
        client_view.rename_group_target = state.rename_group_target;
        client_view.requested_new_tab_name = state.requested_new_tab_name;
        client_view.rename_pane_target = state.rename_pane_target;
        client_view.confirm_delete_group = state.confirm_delete_group;
        client_view.name_input = state.name_input;
        client_view.name_input_replace_on_type = state.name_input_replace_on_type;
        client_view.release_notes = state.release_notes;
        client_view.product_announcement = state.product_announcement;
        client_view.context_menu = state.context_menu;
    }

    fn accept_client_view_navigator_selection(&mut self, client_view: &mut ClientViewState) {
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        let Some(row) = local_state
            .navigator_rows()
            .get(local_state.navigator.selected)
            .cloned()
        else {
            return;
        };

        if self.state.focus_navigator_target(row.target) {
            client_view.mode = Mode::Terminal;
        }
    }

    fn accept_client_view_command_palette_selection(&mut self, client_view: &mut ClientViewState) {
        let Some(action) =
            input::selected_command_palette_action_for_view(&self.state, client_view)
        else {
            return;
        };

        self.apply_client_view_local_app_action(client_view, |app| {
            input::execute_command_palette_action(app, action);
        });
    }

    fn accept_client_view_global_menu_selection(&mut self, client_view: &mut ClientViewState) {
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        let Some(action) = input::global_menu_actions(&local_state)
            .get(local_state.global_menu.highlighted)
            .copied()
        else {
            return;
        };

        match action {
            input::GlobalMenuAction::Detach => {
                input::request_detach(&mut self.state);
                Self::leave_client_view_command_mode(client_view);
            }
            input::GlobalMenuAction::ReloadConfig => {
                self.state.request_reload_config = true;
                Self::leave_client_view_command_mode(client_view);
            }
            input::GlobalMenuAction::Settings
            | input::GlobalMenuAction::Keybinds
            | input::GlobalMenuAction::WhatsNew => {
                self.apply_client_view_local_key(client_view, |state| {
                    input::handle_global_menu_key(
                        state,
                        crossterm::event::KeyEvent::new(
                            crossterm::event::KeyCode::Enter,
                            crossterm::event::KeyModifiers::empty(),
                        ),
                    );
                });
            }
        }
    }

    fn accept_client_view_group_menu_selection(&mut self, client_view: &mut ClientViewState) {
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        let Some(action) =
            local_state.group_menu_action_for_row(local_state.group_menu.highlighted)
        else {
            return;
        };

        match action {
            input::GroupMenuAction::AllSpaces => {
                self.state.show_all_groups();
                Self::leave_client_view_command_mode(client_view);
            }
            input::GroupMenuAction::Group(idx) => {
                self.state.switch_group(idx);
                Self::leave_client_view_command_mode(client_view);
            }
            input::GroupMenuAction::NewWorkspace => {
                self.state.request_new_workspace = true;
                Self::leave_client_view_command_mode(client_view);
            }
            input::GroupMenuAction::NewGroup => {
                self.apply_client_view_local_key(client_view, |state| {
                    input::handle_group_menu_key(
                        state,
                        crossterm::event::KeyEvent::new(
                            crossterm::event::KeyCode::Enter,
                            crossterm::event::KeyModifiers::empty(),
                        ),
                    );
                });
            }
        }
    }

    fn accept_client_view_agent_menu_selection(&mut self, client_view: &mut ClientViewState) {
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        let Some(action) =
            local_state.agent_menu_action_for_row(local_state.agent_menu.highlighted)
        else {
            return;
        };

        self.state.agent_panel_scope = match action {
            input::AgentMenuAction::ThisSpace => state::AgentPanelScope::CurrentWorkspace,
            input::AgentMenuAction::ThisGroup => state::AgentPanelScope::CurrentGroup,
            input::AgentMenuAction::AllAgents => state::AgentPanelScope::AllWorkspaces,
        };
        self.state.agent_panel_scroll = 0;
        self.state.mark_session_dirty();
        Self::leave_client_view_command_mode(client_view);
    }

    fn execute_client_view_navigate_action(
        &mut self,
        client_view: &mut ClientViewState,
        action: input::NavigateAction,
        context: input::ActionContext,
    ) {
        self.apply_client_view_local_app_action(client_view, |app| {
            let previous_mode = app.state.mode;
            if action == input::NavigateAction::EditScrollback {
                app.launch_focused_scrollback_editor();
                if matches!(
                    context,
                    input::ActionContext::Direct | input::ActionContext::Prefix
                ) && app.state.mode == previous_mode
                {
                    app.state.return_to_active_workspace_mode();
                }
                return;
            }

            input::execute_navigate_action_in_context(
                &mut app.state,
                &mut app.terminal_runtimes,
                action,
                context,
            );
        });
    }

    fn send_terminal_key_for_view(
        &self,
        client_view: &ClientViewState,
        key: crate::input::TerminalKey,
    ) -> bool {
        let Some(ws_idx) = client_view.active_workspace else {
            debug!("client view terminal key dropped: no active workspace");
            return false;
        };
        let Some((_, pane_id)) = client_view.focused_pane_for_workspace(&self.state, ws_idx) else {
            debug!(ws_idx, "client view terminal key dropped: no focused pane");
            return false;
        };
        let Some(runtime) =
            self.state
                .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
        else {
            debug!(
                ws_idx,
                pane_id = pane_id.raw(),
                "client view terminal key dropped: no runtime"
            );
            return false;
        };

        let bytes = runtime.encode_terminal_key(key);
        if bytes.is_empty() {
            debug!(
                ws_idx,
                pane_id = pane_id.raw(),
                "client view terminal key dropped: empty encoding"
            );
            return false;
        }
        if let Err(err) = runtime.try_send_bytes(bytes::Bytes::from(bytes)) {
            debug!(ws_idx, pane_id = pane_id.raw(), err = %err, "client view terminal key dropped: send failed");
            return false;
        }
        true
    }

    fn paste_for_view(&self, client_view: &mut ClientViewState, text: &str) -> bool {
        if client_view.mode != Mode::Terminal {
            return Self::paste_into_client_view_text_input(client_view, text);
        }
        let Some(ws_idx) = client_view.active_workspace else {
            return false;
        };
        let Some((_, pane_id)) = client_view.focused_pane_for_workspace(&self.state, ws_idx) else {
            return false;
        };
        let Some(runtime) =
            self.state
                .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
        else {
            return false;
        };
        let payload = if runtime
            .input_state()
            .map(|state| state.bracketed_paste)
            .unwrap_or(false)
        {
            format!("\x1b[200~{text}\x1b[201~")
        } else {
            text.to_string()
        };
        runtime.try_send_bytes(bytes::Bytes::from(payload)).is_ok()
    }

    fn paste_into_client_view_text_input(client_view: &mut ClientViewState, text: &str) -> bool {
        match client_view.mode {
            Mode::RenameWorkspace
            | Mode::RenameGroup
            | Mode::RenameTab
            | Mode::RenamePane
            | Mode::EditWorktreeDirectory => {
                if client_view.name_input_replace_on_type
                    && !(client_view.mode == Mode::RenameGroup
                        && client_view.creating_new_group
                        && client_view.group_modal_selected_field == 1)
                {
                    client_view.name_input.clear();
                    client_view.name_input_replace_on_type = false;
                }
                if client_view.mode == Mode::RenameGroup
                    && client_view.creating_new_group
                    && client_view.group_modal_selected_field == 1
                {
                    client_view.group_default_directory_input.push_str(text);
                } else {
                    client_view.name_input.push_str(text);
                }
                true
            }
            Mode::Navigator => {
                if !client_view.navigator.search_focused {
                    return false;
                }
                client_view.navigator.state_filter = None;
                client_view.navigator.query.push_str(text);
                true
            }
            _ => false,
        }
    }

    fn handle_mouse_for_view(
        &mut self,
        client_view: &mut ClientViewState,
        mouse: crossterm::event::MouseEvent,
    ) {
        if !self.state.mouse_capture {
            self.state
                .handle_pane_mouse_only_for_view(&self.terminal_runtimes, client_view, mouse);
            return;
        }

        let before_shared = self.state.clone();
        let mut local_state = self.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        if !Self::handle_client_view_picker_mouse(&mut local_state, mouse) {
            local_state.handle_mouse(&mut self.terminal_runtimes, mouse);
        }
        let pending_tab_focus =
            Self::pending_client_tab_focus_after_deferred_request(&before_shared, &local_state);
        self.apply_shared_client_view_effects_from_local(&before_shared, local_state.clone());
        *client_view = ClientViewState::from_app_state(&local_state);
        client_view.reconcile(&self.state);
        if let Some((workspace_id, tab_idx)) = pending_tab_focus {
            client_view
                .pending_active_tabs
                .insert(workspace_id, tab_idx);
        }
    }
    fn handle_client_view_picker_mouse(
        state: &mut AppState,
        mouse: crossterm::event::MouseEvent,
    ) -> bool {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

        match state.mode {
            Mode::CommandPalette => match mouse.kind {
                MouseEventKind::Moved => {
                    input::hover_command_palette_selection(state, mouse.column, mouse.row);
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if input::command_palette_contains_point(state, mouse.column, mouse.row) {
                        input::hover_command_palette_selection(state, mouse.column, mouse.row);
                    } else {
                        input::close_command_palette(state);
                    }
                }
                MouseEventKind::ScrollDown => {
                    input::scroll_command_palette_rows(state, input::MODAL_WHEEL_SCROLL_ROWS);
                }
                MouseEventKind::ScrollUp => {
                    input::scroll_command_palette_rows(state, -input::MODAL_WHEEL_SCROLL_ROWS);
                }
                MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                }
                _ => {}
            },
            Mode::GitRepoPicker => match mouse.kind {
                MouseEventKind::Moved | MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(idx) = crate::ui::git_repo_picker::git_repo_picker_index_at(
                        state,
                        mouse.column,
                        mouse.row,
                    ) {
                        state.git_repo_picker.selected = idx;
                    } else if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                        state.return_to_active_workspace_mode();
                    }
                }
                MouseEventKind::ScrollDown => {
                    let visible_repos =
                        crate::ui::git_repo_picker::git_repo_picker_list_geometry(state)
                            .map(|list| (list.scroll_area.body.height as usize).div_ceil(2).max(1))
                            .unwrap_or(1);
                    let max_scroll = state
                        .git_repo_picker
                        .roots
                        .len()
                        .saturating_sub(visible_repos);
                    state.git_repo_picker.scroll = state
                        .git_repo_picker
                        .scroll
                        .saturating_add((input::MODAL_WHEEL_SCROLL_ROWS as usize).div_ceil(2))
                        .min(max_scroll);
                }
                MouseEventKind::ScrollUp => {
                    state.git_repo_picker.scroll = state
                        .git_repo_picker
                        .scroll
                        .saturating_sub((input::MODAL_WHEEL_SCROLL_ROWS as usize).div_ceil(2));
                }
                _ => {}
            },
            Mode::AgentProfilePicker => match mouse.kind {
                MouseEventKind::Moved => {
                    input::agent_profile_picker::hover_agent_profile_picker_selection(
                        state,
                        mouse.column,
                        mouse.row,
                    );
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(inner) =
                        crate::ui::agent_profile_picker_inner_rect(state.screen_rect())
                    {
                        let (start, close) = crate::ui::agent_profile_picker_button_rects(inner);
                        if mouse.column >= start.x
                            && mouse.column < start.x + start.width
                            && mouse.row >= start.y
                            && mouse.row < start.y + start.height
                        {
                            input::agent_profile_picker::handle_agent_profile_picker_key_for_view(
                                state,
                                KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
                            );
                            return true;
                        }
                        if mouse.column >= close.x
                            && mouse.column < close.x + close.width
                            && mouse.row >= close.y
                            && mouse.row < close.y + close.height
                        {
                            input::agent_profile_picker::close_agent_profile_picker(state);
                            return true;
                        }
                    }

                    if input::agent_profile_picker::select_agent_profile_picker_tab_at(
                        state,
                        mouse.column,
                        mouse.row,
                    ) {
                        return true;
                    }

                    if input::agent_profile_picker::agent_profile_picker_contains_point(
                        state,
                        mouse.column,
                        mouse.row,
                    ) {
                        input::agent_profile_picker::hover_agent_profile_picker_selection(
                            state,
                            mouse.column,
                            mouse.row,
                        );
                    } else {
                        input::agent_profile_picker::close_agent_profile_picker(state);
                    }
                }
                MouseEventKind::ScrollDown => {
                    input::agent_profile_picker::scroll_agent_profile_picker_rows(
                        state,
                        input::MODAL_WHEEL_SCROLL_ROWS,
                    );
                }
                MouseEventKind::ScrollUp => {
                    input::agent_profile_picker::scroll_agent_profile_picker_rows(
                        state,
                        -input::MODAL_WHEEL_SCROLL_ROWS,
                    );
                }
                MouseEventKind::Up(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                }
                _ => {}
            },
            _ => return false,
        }

        true
    }

    fn pending_client_tab_focus_after_deferred_request(
        before: &AppState,
        local: &AppState,
    ) -> Option<(String, usize)> {
        let ws_idx = local
            .request_agent_profile_tab
            .as_ref()
            .map(|(ws_idx, _)| *ws_idx)
            .or_else(|| {
                (local.request_open_git_diff_command && !before.request_open_git_diff_command)
                    .then_some(local.active.unwrap_or(local.selected))
            })
            .or_else(|| {
                (local.request_new_tab && !before.request_new_tab)
                    .then_some(local.active.unwrap_or(local.selected))
            })?;
        let workspace = local.workspaces.get(ws_idx)?;
        Some((workspace.id.clone(), workspace.tabs.len()))
    }

    fn apply_shared_client_view_effects_from_local(
        &mut self,
        before: &AppState,
        mut local: AppState,
    ) {
        if local.sidebar_collapsed != before.sidebar_collapsed {
            self.state.sidebar_collapsed = local.sidebar_collapsed;
            self.state.mark_session_dirty();
        }

        if local.right_sidebar_collapsed != before.right_sidebar_collapsed {
            self.state.right_sidebar_collapsed = local.right_sidebar_collapsed;
            self.state.mark_session_dirty();
        }

        if local.group_filter_enabled != before.group_filter_enabled
            || local.active_group != before.active_group
        {
            self.state.group_filter_enabled = local.group_filter_enabled;
            self.state.active_group = local
                .active_group
                .min(self.state.groups.len().saturating_sub(1));
            self.state.mark_session_dirty();
        }

        if local.agent_panel_scope != before.agent_panel_scope
            || local.agent_panel_scroll != before.agent_panel_scroll
        {
            self.state.agent_panel_scope = local.agent_panel_scope;
            self.state.agent_panel_scroll = local.agent_panel_scroll;
            self.state.mark_session_dirty();
        }

        if local.groups != before.groups {
            self.state.groups = local.groups;
            self.state.active_group = local
                .active_group
                .min(self.state.groups.len().saturating_sub(1));
        }

        if !same_workspace_structure(&local.workspaces, &before.workspaces) {
            preserve_shared_workspace_view_state(&before.workspaces, &mut local.workspaces);
            let workspace_count_changed = local.workspaces.len() != before.workspaces.len();
            self.state.workspaces = local.workspaces;
            if workspace_count_changed {
                self.state.active = local.active;
                self.state.selected = local.selected;
                self.state.mode = local.mode;
            } else if self
                .state
                .active
                .is_some_and(|idx| idx >= self.state.workspaces.len())
            {
                self.state.active = self.state.workspaces.len().checked_sub(1);
            }
            self.state.selected = self
                .state
                .selected
                .min(self.state.workspaces.len().saturating_sub(1));
            self.state.mark_session_dirty();
        }

        if local.request_new_tab && !before.request_new_tab {
            self.state.active = local.active;
            self.state.selected = local.selected;
            self.state.requested_new_tab_name = local.requested_new_tab_name.take();
            self.state.request_new_tab = true;
        }

        if local.request_agent_profile_tab != before.request_agent_profile_tab {
            self.state.request_agent_profile_tab = local.request_agent_profile_tab.take();
        }

        if local.request_new_workspace && !before.request_new_workspace {
            self.state.active_group = local
                .active_group
                .min(self.state.groups.len().saturating_sub(1));
            self.state.request_new_workspace = true;
        }

        if local.request_open_git_diff_command && !before.request_open_git_diff_command {
            self.state.active = local.active;
            self.state.selected = local.selected;
            self.state.request_open_git_diff_command = true;
        }

        if local.request_reload_config && !before.request_reload_config {
            self.state.request_reload_config = true;
        }

        if local.request_complete_onboarding && !before.request_complete_onboarding {
            self.state.request_complete_onboarding = true;
        }

        if local.request_clipboard_write != before.request_clipboard_write {
            self.state.request_clipboard_write = local.request_clipboard_write.take();
        }

        if local.detach_requested && !before.detach_requested {
            self.state.detach_requested = true;
        }

        if local.should_quit && !before.should_quit {
            self.state.should_quit = true;
        }
    }

    fn leave_client_view_command_mode(client_view: &mut ClientViewState) {
        client_view.mode = if client_view.active_workspace.is_some() {
            Mode::Terminal
        } else {
            Mode::Navigate
        };
    }

    fn sync_prefix_input_source_for_mode_transition(
        &mut self,
        previous_mode: Mode,
        next_mode: Mode,
    ) {
        match (previous_mode == Mode::Prefix, next_mode == Mode::Prefix) {
            (false, true) if self.state.switch_ascii_input_source_in_prefix => {
                self.prefix_input_source.switch_to_ascii();
            }
            (true, false) => self.prefix_input_source.restore(),
            _ => {}
        }
    }

    /// Handles a key event in non-terminal mode for the headless server.
    ///
    /// Uses the standalone handler functions that work on `&mut AppState`
    /// since the server doesn't have the async context of the monolithic App.
    fn handle_non_terminal_key_headless(&mut self, key: crate::input::TerminalKey) {
        let key_event = key.as_key_event();
        if input::modal_paste_target_active(&self.state)
            && input::is_modal_paste_shortcut(&key_event)
        {
            if let Some(text) = crate::platform::read_clipboard_text() {
                self.paste_into_active_text_input(&text);
            }
            return;
        }

        match self.state.mode {
            Mode::Prefix => {
                self.handle_prefix_key(key);
            }
            Mode::Navigate => {
                self.handle_navigate_key(key);
            }
            Mode::Copy => {
                self.handle_copy_mode_key(key);
            }
            Mode::RenameWorkspace | Mode::RenameGroup | Mode::RenameTab | Mode::RenamePane => {
                input::handle_rename_key(&mut self.state, key_event);
            }
            Mode::EditWorktreeDirectory => {
                input::handle_worktree_directory_key(&mut self.state, key_event);
            }
            Mode::Resize => {
                input::handle_resize_key(&mut self.state, key);
            }
            Mode::ConfirmClose => {
                input::handle_confirm_close_key(&mut self.state, key_event);
            }
            Mode::ConfirmDeleteGroup => {
                input::handle_confirm_delete_group_key(&mut self.state, key_event);
            }
            Mode::ContextMenu => {
                input::handle_context_menu_key(
                    &mut self.state,
                    &mut self.terminal_runtimes,
                    key_event,
                );
            }
            Mode::KeybindHelp => {
                input::handle_keybind_help_key(&mut self.state, key_event);
            }
            Mode::Navigator => {
                input::handle_navigator_key(&mut self.state, key_event);
            }
            Mode::CommandPalette => {
                self.handle_command_palette_key(key_event);
            }
            Mode::AgentProfilePicker => {
                self.handle_agent_profile_picker_key(key_event);
            }
            Mode::GitRepoPicker => {
                self.handle_git_repo_picker_key(key_event);
            }
            Mode::GlobalMenu => {
                input::handle_global_menu_key(&mut self.state, key_event);
            }
            Mode::GroupMenu => {
                input::handle_group_menu_key(&mut self.state, key_event);
            }
            Mode::AgentMenu => {
                input::handle_agent_menu_key(&mut self.state, key_event);
            }
            Mode::Onboarding => {
                self.handle_onboarding_key(key_event);
            }
            Mode::ReleaseNotes => {
                self.handle_release_notes_key(key_event);
            }
            Mode::ProductAnnouncement => {
                self.handle_product_announcement_key(key_event);
            }
            Mode::Settings => {
                self.handle_settings_key(key_event);
            }
            Mode::Terminal => {
                // Should not be called in terminal mode.
            }
        }
    }

    /// Handles a mouse event for the headless server.
    ///
    /// Delegates to the same mouse handling logic used in the monolithic
    /// mode (hit-testing against the rendered UI), which works because
    /// the server's AppState maintains view geometry from virtual rendering.
    fn handle_mouse_event_headless(&mut self, mouse: crossterm::event::MouseEvent) {
        self.handle_mouse(mouse);
    }
}

fn same_workspace_structure(
    left: &[crate::workspace::Workspace],
    right: &[crate::workspace::Workspace],
) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right).all(|(left_ws, right_ws)| {
        left_ws.id == right_ws.id
            && left_ws.custom_name == right_ws.custom_name
            && left_ws.group_id == right_ws.group_id
            && left_ws.default_cwd == right_ws.default_cwd
            && left_ws.tabs.len() == right_ws.tabs.len()
            && left_ws
                .tabs
                .iter()
                .zip(&right_ws.tabs)
                .all(|(left_tab, right_tab)| {
                    left_tab.number == right_tab.number
                        && left_tab.custom_name == right_tab.custom_name
                        && left_tab.root_pane == right_tab.root_pane
                        && left_tab.panes.keys().collect::<HashSet<_>>()
                            == right_tab.panes.keys().collect::<HashSet<_>>()
                })
    })
}

fn preserve_shared_workspace_view_state(
    before: &[crate::workspace::Workspace],
    after: &mut [crate::workspace::Workspace],
) {
    let before_by_id: HashMap<&str, &crate::workspace::Workspace> = before
        .iter()
        .map(|workspace| (workspace.id.as_str(), workspace))
        .collect();

    for workspace in after {
        let Some(before_workspace) = before_by_id.get(workspace.id.as_str()) else {
            continue;
        };
        workspace.active_tab = before_workspace
            .active_tab
            .min(workspace.tabs.len().saturating_sub(1));

        for tab in &mut workspace.tabs {
            let Some(before_tab) = before_workspace
                .tabs
                .iter()
                .find(|before_tab| before_tab.number == tab.number)
            else {
                continue;
            };
            tab.zoomed = before_tab.zoomed;
            let before_focused = before_tab.layout.focused();
            if tab.panes.contains_key(&before_focused) {
                tab.layout.focus_pane(before_focused);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::detect::{Agent, AgentState};
    use crate::terminal::TerminalRuntime;
    use crate::workspace::Workspace;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use std::sync::Mutex;

    fn raw_key(
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: KeyEventKind,
    ) -> crate::raw_input::RawInputEvent {
        crate::raw_input::RawInputEvent::Key(
            crate::input::TerminalKey::new(code, modifiers).with_kind(kind),
        )
    }

    fn raw_mouse(
        kind: crossterm::event::MouseEventKind,
        column: u16,
        row: u16,
    ) -> crate::raw_input::RawInputEvent {
        crate::raw_input::RawInputEvent::Mouse(crossterm::event::MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        })
    }

    fn install_two_agent_profiles(state: &mut state::AppState) {
        state.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:first".to_string(), "user:second".to_string()],
                custom: vec![
                    crate::agent_profiles::UserAgentProfileConfig {
                        id: "first".to_string(),
                        name: "first".to_string(),
                        kind: crate::agent_profiles::AgentKind::Custom,
                        command: "first-agent".to_string(),
                        env: std::collections::BTreeMap::new(),
                        enabled: true,
                    },
                    crate::agent_profiles::UserAgentProfileConfig {
                        id: "second".to_string(),
                        name: "second".to_string(),
                        kind: crate::agent_profiles::AgentKind::Custom,
                        command: "second-agent".to_string(),
                        env: std::collections::BTreeMap::new(),
                        enabled: true,
                    },
                ],
            },
        );
    }

    fn compute_client_view(
        app: &App,
        client_view: &mut ClientViewState,
        area: ratatui::layout::Rect,
    ) {
        let mut local_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        crate::ui::compute_view(&mut local_state, area);
        client_view.computed = local_state.view;
    }

    fn confirm_button_for_client_view(
        app: &App,
        client_view: &ClientViewState,
    ) -> ratatui::layout::Rect {
        let mut local_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        let popup = local_state.confirm_close_rect();
        let inner = ratatui::layout::Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        crate::ui::confirm_close_button_rects(inner).0
    }

    fn context_menu_rect_for_client_view(
        app: &App,
        client_view: &ClientViewState,
    ) -> ratatui::layout::Rect {
        let mut local_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut local_state, client_view);
        local_state.context_menu_rect().expect("context menu rect")
    }

    #[tokio::test]
    async fn raw_input_dispatch_enters_and_leaves_prefix_mode() {
        let mut app = test_app();
        app.state.switch_ascii_input_source_in_prefix = true;
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.handle_raw_input_event(raw_key(
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        ))
        .await;
        assert_eq!(app.state.mode, Mode::Prefix);

        app.handle_raw_input_event(raw_key(
            KeyCode::Esc,
            KeyModifiers::empty(),
            KeyEventKind::Press,
        ))
        .await;
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    fn release_notes_state() -> state::ReleaseNotesState {
        state::ReleaseNotesState {
            version: "0.1.0".into(),
            body: "notes".into(),
            scroll: 0,
            preview: true,
        }
    }

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    fn config_env_lock() -> &'static Mutex<()> {
        crate::config::test_config_env_lock()
    }

    fn temp_config_path(name: &str) -> std::path::PathBuf {
        let unique = format!(
            "hako-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("config.toml")
    }

    fn test_snapshot(
        groups: Vec<crate::persist::GroupSnapshot>,
        workspaces: Vec<crate::persist::WorkspaceSnapshot>,
    ) -> crate::persist::SessionSnapshot {
        crate::persist::SessionSnapshot {
            version: 3,
            groups,
            active_group: 0,
            group_filter_enabled: true,
            default_view: crate::persist::SessionDefaultViewSnapshot::default(),
            workspaces,
            active: None,
            selected: 0,
            agent_panel_scope: state::AgentPanelScope::CurrentWorkspace,
            sidebar_width: None,
            sidebar_collapsed: false,
            sidebar_section_split: None,
            right_sidebar_width: None,
            right_sidebar_collapsed: false,
            ui: crate::persist::SessionUiSnapshot::default(),
            pane_id_aliases: std::collections::HashMap::new(),
        }
    }

    fn test_group_snapshot(id: &str, name: &str) -> crate::persist::GroupSnapshot {
        crate::persist::GroupSnapshot {
            id: id.to_string(),
            name: name.to_string(),
            icon: "■".to_string(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        }
    }

    fn test_workspace_snapshot(id: &str, group_id: &str) -> crate::persist::WorkspaceSnapshot {
        crate::persist::WorkspaceSnapshot {
            id: Some(id.to_string()),
            custom_name: Some(id.to_string()),
            group_id: group_id.to_string(),
            identity_cwd: std::path::PathBuf::from("/tmp"),
            default_cwd: std::path::PathBuf::from("/tmp"),
            public_pane_numbers: std::collections::HashMap::new(),
            next_public_pane_number: 0,
            public_tab_numbers: Vec::new(),
            next_public_tab_number: 0,
            tabs: Vec::new(),
            active_tab: 0,
        }
    }

    #[test]
    fn snapshot_groups_include_orphan_workspace_groups() {
        let snap = test_snapshot(
            vec![test_group_snapshot(
                crate::workspace::DEFAULT_GROUP_ID,
                "group 1",
            )],
            vec![test_workspace_snapshot("work", "orphan-group")],
        );

        let groups = groups_from_snapshot(&snap);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, crate::workspace::DEFAULT_GROUP_ID);
        assert_eq!(groups[1].id, "orphan-group");
        assert_eq!(groups[1].name, "group 2");
    }

    #[cfg(unix)]
    #[test]
    fn handoff_restores_snapshot_groups_and_sidebar_state() {
        let mut snap = test_snapshot(
            vec![
                test_group_snapshot(crate::workspace::DEFAULT_GROUP_ID, "group 1"),
                test_group_snapshot("work", "Work"),
            ],
            Vec::new(),
        );
        snap.active_group = 1;
        snap.group_filter_enabled = false;
        snap.sidebar_width = Some(32);
        snap.sidebar_collapsed = true;
        snap.sidebar_section_split = Some(0.25);
        snap.right_sidebar_width = Some(41);
        snap.right_sidebar_collapsed = true;
        snap.default_view.sidebar_width = Some(32);
        snap.default_view.sidebar_collapsed = true;
        snap.default_view.sidebar_section_split = Some(0.25);
        snap.default_view.right_sidebar_width = Some(41);
        snap.default_view.right_sidebar_collapsed = true;

        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut imports = std::collections::HashMap::new();
        let app = App::new_from_handoff(
            &Config::default(),
            None,
            api_rx,
            crate::api::EventHub::default(),
            &snap,
            &mut imports,
        )
        .expect("empty handoff restores app state");

        assert_eq!(app.state.groups.len(), 2);
        assert_eq!(app.state.groups[1].id, "work");
        assert_eq!(app.state.groups[1].name, "Work");
        assert_eq!(app.state.active_group, 1);
        assert!(!app.state.group_filter_enabled);
        assert_eq!(app.state.sidebar_width, 32);
        assert_eq!(app.state.sidebar_section_split, 0.25);
        assert!(app.state.sidebar_collapsed);
        assert_eq!(app.state.right_sidebar_width, 41);
        assert!(app.state.right_sidebar_collapsed);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn handoff_restores_agent_panel_semantics_before_hooks_report_again() {
        let mut state = state::AppState::test_new();
        state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = state::AgentPanelScope::AllWorkspaces;
        state.workspace_scroll = 3;
        state.agent_panel_scroll = 4;
        state.tab_scroll = 2;
        state.mobile_switcher_scroll = 5;
        state.activity_agents_expanded = false;
        state.activity_commands_expanded = true;
        state.activity_ports_expanded = true;
        state.collapsed_agent_sections = vec!["Work".to_string()];
        state.collapsed_command_groups = vec!["build".to_string()];
        state.collapsed_command_status_groups = vec!["running".to_string()];
        state.collapsed_workspace_groups = vec!["g1".to_string()];

        seed_handoff_agent(
            &mut state,
            0,
            AgentState::Working,
            true,
            7,
            Some("thinking"),
            Some("busy"),
        );
        let _second_pane = seed_handoff_agent(
            &mut state,
            1,
            AgentState::Idle,
            false,
            11,
            Some("idle custom"),
            Some("done label"),
        );

        let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
        let mut snap = crate::persist::capture_handoff(
            &state.groups,
            state.active_group,
            state.group_filter_enabled,
            &state.workspaces,
            &state.terminals,
            &terminal_runtimes,
            state.active,
            state.selected,
            state.agent_panel_scope,
            state.sidebar_width,
            state.sidebar_collapsed,
            state.sidebar_section_split,
            state.right_sidebar_width,
            state.right_sidebar_collapsed,
        );
        snap.ui = crate::persist::SessionUiSnapshot::from_app_state(&state);
        snap.default_view.ui = snap.ui.clone();

        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut imports = std::collections::HashMap::new();
        let mut app = App::new_from_handoff(
            &Config::default(),
            None,
            api_rx,
            crate::api::EventHub::default(),
            &snap,
            &mut imports,
        )
        .expect("handoff should restore semantic agent state");

        let agents = app.collect_agent_infos();
        assert_eq!(agents.len(), 2);
        assert!(agents.iter().any(|agent| {
            agent.workspace_id == app.state.workspaces[0].id
                && agent.agent_status == crate::api::schema::AgentStatus::Working
                && agent.custom_status.as_deref() == Some("thinking")
                && agent.state_labels.get("working").map(String::as_str) == Some("busy")
        }));
        assert!(agents.iter().any(|agent| {
            agent.workspace_id == app.state.workspaces[1].id
                && agent.agent_status == crate::api::schema::AgentStatus::Done
                && agent.custom_status.as_deref() == Some("idle custom")
                && agent.state_labels.get("idle").map(String::as_str) == Some("done label")
        }));
        assert_eq!(app.state.workspace_scroll, 3);
        assert_eq!(app.state.agent_panel_scroll, 4);
        assert_eq!(app.state.tab_scroll, 2);
        assert_eq!(app.state.mobile_switcher_scroll, 5);
        assert!(!app.state.activity_agents_expanded);
        assert!(app.state.activity_commands_expanded);
        assert!(app.state.activity_ports_expanded);
        assert_eq!(app.state.collapsed_agent_sections, vec!["Work"]);
        assert_eq!(app.state.collapsed_command_groups, vec!["build"]);
        assert_eq!(app.state.collapsed_command_status_groups, vec!["running"]);
        assert_eq!(app.state.collapsed_workspace_groups, vec!["g1"]);

        let restored_first_pane = app.state.workspaces[0].tabs[0].root_pane;
        app.handle_internal_event(AppEvent::HookStateReported {
            pane_id: restored_first_pane,
            source: "hako:omp".to_string(),
            agent_label: "omp".to_string(),
            state: AgentState::Idle,
            message: None,
            custom_status: None,
            seq: Some(6),
            session_ref: None,
            launch_env: Vec::new(),
        });
        let agents = app.collect_agent_infos();
        assert!(agents.iter().any(|agent| {
            agent.workspace_id == app.state.workspaces[0].id
                && agent.agent_status == crate::api::schema::AgentStatus::Working
                && agent.custom_status.as_deref() == Some("thinking")
        }));
    }

    #[cfg(unix)]
    fn seed_handoff_agent(
        state: &mut state::AppState,
        ws_idx: usize,
        agent_state: AgentState,
        seen: bool,
        seq: u64,
        custom_status: Option<&str>,
        state_label: Option<&str>,
    ) -> crate::layout::PaneId {
        let pane_id = state.workspaces[ws_idx].tabs[0].root_pane;
        let terminal_id = state.workspaces[ws_idx]
            .terminal_id(pane_id)
            .expect("test pane should have terminal")
            .clone();
        let terminal = state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal should exist");
        let _ = terminal.set_hook_authority_with_session_ref(
            "hako:omp".to_string(),
            "omp".to_string(),
            agent_state,
            Some("reported".to_string()),
            custom_status.map(str::to_string),
            Some(crate::agent_resume::AgentSessionRef {
                kind: crate::agent_resume::AgentSessionRefKind::Id,
                value: format!("session-{ws_idx}"),
            }),
            Some(seq),
        );
        let _ = terminal.set_agent_metadata(crate::terminal::AgentMetadataReport {
            source: format!("hako:omp:metadata:{ws_idx}"),
            agent_label: Some("omp".to_string()),
            applies_to_source: Some("hako:omp".to_string()),
            title: None,
            display_agent: Some("OMP".to_string()),
            custom_status: custom_status.map(str::to_string),
            state_labels: state_label
                .map(|label| {
                    std::collections::HashMap::from([(
                        match agent_state {
                            AgentState::Idle => "idle".to_string(),
                            AgentState::Working => "working".to_string(),
                            AgentState::Blocked => "blocked".to_string(),
                            AgentState::Unknown => "unknown".to_string(),
                        },
                        label.to_string(),
                    )])
                })
                .unwrap_or_default(),
            clear_title: false,
            clear_display_agent: false,
            clear_custom_status: false,
            clear_state_labels: false,
            ttl: None,
            seq: Some(seq),
        });
        state.workspaces[ws_idx].tabs[0]
            .panes
            .get_mut(&pane_id)
            .expect("test pane should exist")
            .seen = seen;
        pane_id
    }

    #[test]
    fn git_refresh_deadline_is_suppressed_while_in_flight() {
        let mut app = test_app();
        app.state.workspaces.push(Workspace::test_new("one"));
        app.git_refresh_in_flight = true;

        assert_eq!(app.git_refresh_deadline(), None);
    }

    #[test]
    fn git_status_event_clears_in_flight_refresh() {
        let mut app = test_app();
        app.git_refresh_in_flight = true;
        let previous_refresh = Instant::now() - Duration::from_secs(10);
        app.last_git_remote_status_refresh = previous_refresh;

        app.handle_internal_event(AppEvent::GitStatusRefreshed {
            results: Vec::new(),
            cache_updates: Vec::new(),
            repo_summaries: Vec::new(),
        });

        assert!(!app.git_refresh_in_flight);
        assert!(app.last_git_remote_status_refresh > previous_refresh);
    }

    #[test]
    fn git_status_event_marks_render_dirty_when_status_changes() {
        let mut app = test_app();
        app.state.workspaces.push(Workspace::test_new("one"));
        app.render_dirty.store(false, Ordering::Release);
        let workspace_id = app.state.workspaces[0].id.clone();
        let resolved_identity_cwd = app.state.workspaces[0].resolved_identity_cwd().unwrap();
        let cwd_fingerprint = app.state.workspaces[0].git_status_cwds();

        app.handle_internal_event(AppEvent::GitStatusRefreshed {
            results: vec![crate::workspace::WorkspaceGitStatus {
                workspace_id,
                resolved_identity_cwd,
                cwd_fingerprint,
                branch: Some("render-dirty-test".into()),
                ahead_behind: Some((1, 0)),
                work_summary: Some(crate::workspace::GitWorkSummary {
                    repo_count: 1,
                    modified: 1,
                    ..crate::workspace::GitWorkSummary::default()
                }),
                space: None,
            }],
            cache_updates: Vec::new(),
            repo_summaries: Vec::new(),
        });

        assert!(app.render_dirty.load(Ordering::Acquire));
    }

    #[test]
    fn startup_uses_configured_agent_panel_scope() {
        let mut config = Config::default();
        config.ui.agent_panel_scope = crate::config::AgentPanelScopeConfig::Current;
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();

        let app = App::new(&config, true, None, api_rx, crate::api::EventHub::default());

        assert_eq!(
            app.state.agent_panel_scope,
            state::AgentPanelScope::CurrentWorkspace
        );
    }

    #[test]
    fn startup_uses_redraw_on_focus_gained_config() {
        let mut config = Config::default();
        config.ui.redraw_on_focus_gained = false;
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();

        let app = App::new(&config, true, None, api_rx, crate::api::EventHub::default());

        assert!(!app.state.redraw_on_focus_gained);
    }

    #[test]
    fn startup_restores_preview_update_available_from_saved_notes() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("startup-preview-update-available");
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        // Use a bogus far-future version so preview=true regardless of current binary version.
        crate::release_notes::save_pending("99.99.99", "### Changed\n- One").unwrap();

        let app = test_app();

        assert_eq!(app.state.update_available.as_deref(), Some("99.99.99"));
        assert!(app.state.latest_release_notes_available);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn startup_does_not_restore_update_available_from_older_saved_notes() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("startup-stale-update-notes");
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        crate::release_notes::save_pending("0.0.9", "### Changed\n- One").unwrap();

        let app = test_app();

        assert_eq!(app.state.update_available, None);
        assert!(app.state.latest_release_notes_available);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn startup_keeps_pending_release_notes_available_without_auto_opening() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("startup-pending-release-notes-no-auto-open");
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        crate::release_notes::save_pending(env!("CARGO_PKG_VERSION"), "### Changed\n- One")
            .unwrap();
        let config = Config {
            onboarding: Some(false),
            ..Default::default()
        };
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();

        let app = App::new(&config, true, None, api_rx, crate::api::EventHub::default());

        assert_eq!(app.state.mode, Mode::Navigate);
        assert!(app.state.release_notes.is_none());
        assert!(app.state.latest_release_notes_available);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn startup_still_auto_opens_unseen_product_announcement() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("startup-product-announcement-auto-open");
        let state_home = path.parent().unwrap().join("state");
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);
        let _xdg_state_home_env = crate::config::TestEnvVar::set("XDG_STATE_HOME", &state_home);

        crate::release_notes::save_pending(env!("CARGO_PKG_VERSION"), "### Changed\n- One")
            .unwrap();
        let _fake_announcement_body_env = crate::config::TestEnvVar::set(
            "HAKO_FAKE_PRODUCT_ANNOUNCEMENT_BODY",
            "### Announcement\n- One",
        );
        let _fake_announcement_title_env = crate::config::TestEnvVar::set(
            "HAKO_FAKE_PRODUCT_ANNOUNCEMENT_TITLE",
            "Startup announcement",
        );
        let _fake_announcement_id_env = crate::config::TestEnvVar::set(
            "HAKO_FAKE_PRODUCT_ANNOUNCEMENT_ID",
            "startup-announcement",
        );

        let config = Config {
            onboarding: Some(false),
            ..Default::default()
        };
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();

        let app = App::new(&config, true, None, api_rx, crate::api::EventHub::default());

        assert_eq!(app.state.mode, Mode::ProductAnnouncement);
        assert_eq!(
            app.state
                .product_announcement
                .as_ref()
                .map(|announcement| announcement.id.as_str()),
            Some("startup-announcement")
        );
        assert!(app.state.release_notes.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_updates_live_state() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-success");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[terminal]\ndefault_shell = \"nu\"\nshell_mode = \"non_login\"\nnew_cwd = \"home\"\n[keys]\nnew_workspace = \"prefix+g\"\nprefix = \"ctrl+a\"\n[ui]\nagent_panel_scope = \"current\"\nredraw_on_focus_gained = false\nright_click_passthrough_modifier = \"ctrl\"\n[ui.toast]\ndelivery = \"hako\"\n",
        )
        .unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        let report = app.reload_config();

        assert_eq!(report.status, crate::config::ConfigReloadStatus::Applied);
        assert_eq!(app.state.prefix_code, KeyCode::Char('a'));
        assert_eq!(app.state.prefix_mods, KeyModifiers::CONTROL);
        assert!(app
            .state
            .keybinds
            .new_workspace
            .matches_prefix(&KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty())));
        assert_eq!(
            app.state.toast_config.delivery,
            crate::config::ToastDelivery::Hako
        );
        assert_eq!(
            app.state.agent_panel_scope,
            state::AgentPanelScope::CurrentWorkspace
        );
        assert!(!app.state.redraw_on_focus_gained);
        assert_eq!(
            app.state.right_click_passthrough_modifiers,
            Some(KeyModifiers::CONTROL)
        );
        assert!(app.state.request_client_config_reload);
        assert_eq!(app.state.default_shell, "nu");
        assert_eq!(
            app.state.shell_mode,
            crate::config::ShellModeConfig::NonLogin
        );
        assert_eq!(
            app.state.new_terminal_cwd,
            crate::config::NewTerminalCwdConfig::Home
        );
        assert!(app.state.config_diagnostic.is_none());
        let toast = app.state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, crate::app::state::ToastKind::UpdateInstalled);
        assert_eq!(toast.title, "reloaded config");
        assert_eq!(toast.context, "using config.toml");

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_updates_sidebar_width_only_when_config_owned() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-sidebar-width");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        assert_eq!(
            app.state.sidebar_width_source,
            state::SidebarWidthSource::ConfigDefault
        );

        std::fs::write(&path, "[ui]\nsidebar_width = 34\n").unwrap();
        let report = app.reload_config();
        assert_eq!(report.status, crate::config::ConfigReloadStatus::Applied);
        assert_eq!(app.state.default_sidebar_width, 34);
        assert_eq!(app.state.sidebar_width, 34);

        app.state.sidebar_width = 31;
        app.state.sidebar_width_source = state::SidebarWidthSource::Manual;
        std::fs::write(&path, "[ui]\nsidebar_width = 35\n").unwrap();
        let report = app.reload_config();
        assert_eq!(report.status, crate::config::ConfigReloadStatus::Applied);
        assert_eq!(app.state.default_sidebar_width, 35);
        assert_eq!(app.state.sidebar_width, 31);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_updates_sidebar_bounds_and_reclamps() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-sidebar-bounds");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        // Default bounds.
        assert_eq!(app.state.sidebar_min_width, 18);
        assert_eq!(app.state.sidebar_max_width, 36);

        // Manually set a width and flip the source so the existing
        // sidebar_width-only-when-config-owned guard does NOT update it.
        app.state.sidebar_width = 30;
        app.state.sidebar_width_source = state::SidebarWidthSource::Manual;

        // Tightening max below the current width must re-clamp the live width
        // even when source is Manual — bounds always apply.
        std::fs::write(&path, "[ui]\nsidebar_max_width = 24\n").unwrap();
        let report = app.reload_config();
        assert_eq!(report.status, crate::config::ConfigReloadStatus::Applied);
        assert_eq!(app.state.sidebar_max_width, 24);
        assert_eq!(
            app.state.sidebar_width, 24,
            "manual width must re-clamp to new max"
        );

        // Loosening max leaves the live width alone (it's already within bounds).
        app.state.sidebar_width = 24;
        std::fs::write(&path, "[ui]\nsidebar_max_width = 60\n").unwrap();
        let report = app.reload_config();
        assert_eq!(report.status, crate::config::ConfigReloadStatus::Applied);
        assert_eq!(app.state.sidebar_max_width, 60);
        assert_eq!(app.state.sidebar_width, 24);

        // Raising min above the current width re-clamps upward.
        std::fs::write(&path, "[ui]\nsidebar_min_width = 30\n").unwrap();
        let report = app.reload_config();
        assert_eq!(report.status, crate::config::ConfigReloadStatus::Applied);
        assert_eq!(app.state.sidebar_min_width, 30);
        assert_eq!(
            app.state.sidebar_width, 30,
            "manual width must re-clamp up to new min"
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn app_new_falls_back_to_default_bounds_on_inverted_config() {
        let mut config = Config::default();
        config.ui.sidebar_min_width = 50;
        config.ui.sidebar_max_width = 30;

        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let app = App::new(&config, true, None, api_rx, crate::api::EventHub::default());

        assert_eq!(
            app.state.sidebar_min_width, 18,
            "App::new must fall back to default min when bounds are inverted"
        );
        assert_eq!(
            app.state.sidebar_max_width, 36,
            "App::new must fall back to default max when bounds are inverted"
        );
    }

    #[test]
    fn reload_config_invalid_sidebar_bounds_keeps_previous_ui_and_returns_partial() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-invalid-sidebar-bounds");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        let original_min = app.state.sidebar_min_width;
        let original_max = app.state.sidebar_max_width;
        let original_mouse_capture = app.state.mouse_capture;
        // Pair the bad bounds with another `[ui]` field change to confirm the
        // entire section is treated as invalid (not just the bounds).
        let target_mouse_capture = !original_mouse_capture;
        std::fs::write(
            &path,
            format!(
                "[ui]\nsidebar_min_width = 50\nsidebar_max_width = 30\nmouse_capture = {}\n",
                target_mouse_capture
            ),
        )
        .unwrap();

        let report = app.reload_config();
        assert_eq!(report.status, crate::config::ConfigReloadStatus::Partial);
        assert_eq!(app.state.sidebar_min_width, original_min);
        assert_eq!(app.state.sidebar_max_width, original_max);
        assert_eq!(
            app.state.mouse_capture, original_mouse_capture,
            "[ui] is treated as invalid on bad bounds; mouse_capture must not apply"
        );
        assert!(app
            .state
            .config_diagnostic
            .as_deref()
            .is_some_and(|message| {
                message.contains("sidebar_min_width")
                    && message.contains("sidebar_max_width")
                    && message.contains("greater")
            }));

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_keeps_current_keybinds_on_invalid_binding_but_applies_other_sections() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-invalid-keybind");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[keys]\nnew_workspace = \"wat\"\n[ui.toast]\ndelivery = \"terminal\"\n",
        )
        .unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        let original_prefix = (app.state.prefix_code, app.state.prefix_mods);
        let original_keybinds = app.state.keybinds.new_workspace.clone();
        let report = app.reload_config();

        assert_eq!(report.status, crate::config::ConfigReloadStatus::Partial);
        assert_eq!(
            (app.state.prefix_code, app.state.prefix_mods),
            original_prefix
        );
        assert_eq!(app.state.keybinds.new_workspace, original_keybinds);
        assert_eq!(
            app.state.toast_config.delivery,
            crate::config::ToastDelivery::Terminal
        );
        assert!(app
            .state
            .config_diagnostic
            .as_deref()
            .is_some_and(|message| {
                message.contains("keys.new_workspace") && message.contains("kept current keybinds")
            }));

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_preserves_invalid_ui_section_but_applies_valid_keys() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-invalid-ui-section");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[keys]\nnew_workspace = \"prefix+g\"\n[ui.toast]\ndelivery = \"desktop\"\n",
        )
        .unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        app.state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let report = app.reload_config();

        assert_eq!(report.status, crate::config::ConfigReloadStatus::Partial);
        assert!(app
            .state
            .keybinds
            .new_workspace
            .matches_prefix(&KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty())));
        assert_eq!(
            app.state.toast_config.delivery,
            crate::config::ToastDelivery::Hako
        );
        assert!(app
            .state
            .config_diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("invalid ui config")));

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_preserves_invalid_terminal_section_but_applies_valid_ui() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-invalid-terminal-section");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "[terminal]\ndefault_shell = \"nu\"\nshell_mode = \"sideways\"\nnew_cwd = \"home\"\n[ui.toast]\ndelivery = \"terminal\"\n",
        )
        .unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        let original_default_shell = app.state.default_shell.clone();
        let original_shell_mode = app.state.shell_mode;
        let original_new_cwd = app.state.new_terminal_cwd.clone();
        let report = app.reload_config();

        assert_eq!(report.status, crate::config::ConfigReloadStatus::Partial);
        assert_eq!(app.state.default_shell, original_default_shell);
        assert_eq!(app.state.shell_mode, original_shell_mode);
        assert_eq!(app.state.new_terminal_cwd, original_new_cwd);
        assert_eq!(
            app.state.toast_config.delivery,
            crate::config::ToastDelivery::Terminal
        );
        assert!(app
            .state
            .config_diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("invalid terminal config")));

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn settings_save_toast_delivery_persists_then_applies_live_config() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-toast-delivery");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "onboarding = false\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        assert_eq!(
            app.state.toast_config.delivery,
            crate::config::ToastDelivery::Off
        );

        app.save_toast_delivery(crate::config::ToastDelivery::Terminal);

        assert_eq!(
            app.state.toast_config.delivery,
            crate::config::ToastDelivery::Terminal
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("delivery = \"terminal\""));
        assert!(app.state.config_diagnostic.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn settings_save_theme_persists_family_and_mode() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-theme-mode");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "onboarding = false\n[theme]\nname = \"nord\"\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        app.save_theme(
            "solarized-light",
            "rose-pine",
            crate::config::ThemeMode::System,
            crate::config::TerminalAccent::Blue,
            crate::config::TerminalAccent::Magenta,
        );

        assert_eq!(app.state.global_light_theme_name, "solarized-light");
        assert_eq!(app.state.global_dark_theme_name, "rose-pine");
        assert_eq!(
            app.state.global_theme_mode,
            crate::config::ThemeMode::System
        );
        assert_eq!(
            app.state.global_terminal_light_accent,
            crate::config::TerminalAccent::Blue
        );
        assert_eq!(
            app.state.global_terminal_dark_accent,
            crate::config::TerminalAccent::Magenta
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("name = \""));
        assert!(content.contains("light = \"solarized-light\""));
        assert!(content.contains("dark = \"rose-pine\""));
        assert!(content.contains("mode = \"system\""));
        assert!(content.contains("terminal_light_accent = \"blue\""));
        assert!(content.contains("terminal_dark_accent = \"magenta\""));
        assert_eq!(app.host_terminal_theme_query_count.get(), 1);
    }

    #[test]
    fn settings_save_new_terminal_cwd_persists_then_applies_live_config() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-new-terminal-cwd");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "onboarding = false\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        assert_eq!(
            app.state.new_terminal_cwd,
            crate::config::NewTerminalCwdConfig::Follow
        );

        app.save_new_terminal_cwd(&crate::config::NewTerminalCwdConfig::Home);

        assert_eq!(
            app.state.new_terminal_cwd,
            crate::config::NewTerminalCwdConfig::Home
        );
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[terminal]"));
        assert!(content.contains("new_cwd = \"home\""));
        assert!(app.state.config_diagnostic.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
    #[test]
    fn settings_save_mouse_scroll_lines_persists_then_applies_live_config() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-mouse-scroll-lines");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "onboarding = false\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        assert_eq!(
            app.state.mouse_scroll_lines,
            crate::config::DEFAULT_MOUSE_SCROLL_LINES
        );

        app.save_mouse_scroll_lines(5);

        assert_eq!(app.state.mouse_scroll_lines, 5);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[ui]"));
        assert!(content.contains("mouse_scroll_lines = 5"));
        assert!(app.state.config_diagnostic.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
    #[test]
    fn settings_save_sidebar_widths_persists_then_applies_live_config() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-sidebar-widths");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "onboarding = false\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        app.save_sidebar_widths(30, 20, 40);

        assert_eq!(app.state.default_sidebar_width, 30);
        assert_eq!(app.state.sidebar_min_width, 20);
        assert_eq!(app.state.sidebar_max_width, 40);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("sidebar_width = 30"));
        assert!(content.contains("sidebar_min_width = 20"));
        assert!(content.contains("sidebar_max_width = 40"));
        assert!(app.state.config_diagnostic.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn settings_save_worktree_directory_persists_then_applies_live_config() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-worktree-directory");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "onboarding = false\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        app.save_worktree_directory("~/Projects/hako-worktrees");

        assert!(app
            .state
            .worktree_directory
            .ends_with("Projects/hako-worktrees"));
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[worktrees]"));
        assert!(content.contains("directory = \"~/Projects/hako-worktrees\""));
        assert!(app.state.config_diagnostic.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
    #[test]
    fn settings_save_close_and_tab_prompts_persist_then_apply_live_config() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("settings-save-close-tab-prompts");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "onboarding = false\n[ui]\nconfirm_close = true\nprompt_new_tab_name = true\n",
        )
        .unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        assert!(app.state.confirm_close);
        assert!(app.state.prompt_new_tab_name);

        app.save_confirm_close(false);
        app.save_prompt_new_tab_name(false);

        assert!(!app.state.confirm_close);
        assert!(!app.state.prompt_new_tab_name);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("confirm_close = false"));
        assert!(content.contains("prompt_new_tab_name = false"));
        assert!(app.state.config_diagnostic.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn reload_config_keeps_current_state_on_invalid_toml() {
        let _guard = config_env_lock().lock().unwrap();
        let path = temp_config_path("reload-config-invalid-toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[keys\nnew_workspace = \"g\"\n").unwrap();
        let _config_path_env =
            crate::config::TestEnvVar::set(crate::config::CONFIG_PATH_ENV_VAR, &path);

        let mut app = test_app();
        let original_prefix = (app.state.prefix_code, app.state.prefix_mods);
        let original_keybinds = app.state.keybinds.new_workspace.clone();
        let original_toast_delivery = app.state.toast_config.delivery;
        let report = app.reload_config();

        assert_eq!(report.status, crate::config::ConfigReloadStatus::Failed);
        assert_eq!(
            (app.state.prefix_code, app.state.prefix_mods),
            original_prefix
        );
        assert_eq!(app.state.keybinds.new_workspace, original_keybinds);
        assert_eq!(app.state.toast_config.delivery, original_toast_delivery);
        assert!(app
            .state
            .config_diagnostic
            .as_deref()
            .is_some_and(|message| {
                message.contains("config parse error") && message.contains("keeping current config")
            }));
        assert!(app.state.toast.is_none());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn raw_input_waits_when_reader_is_gone() {
        let result =
            tokio::time::timeout(Duration::from_millis(20), recv_raw_input_or_pending(None)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn terminal_mode_handles_repeat_key_events() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Backspace,
                KeyModifiers::empty(),
                KeyEventKind::Repeat,
            ))
            .await;

        assert!(handled);
    }

    #[tokio::test]
    async fn outer_focus_gained_marks_visible_done_panes_seen() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let root_pane = workspace.tabs[0].root_pane;
        let split_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let background_tab = workspace.test_add_tab(Some("background"));
        let background_pane = workspace.tabs[background_tab].root_pane;

        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        let root_terminal_id = app.state.workspaces[0].tabs[0].panes[&root_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&root_terminal_id)
            .unwrap()
            .state = AgentState::Idle;
        app.state.workspaces[0].tabs[0]
            .panes
            .get_mut(&root_pane)
            .unwrap()
            .seen = false;
        let split_terminal_id = app.state.workspaces[0].tabs[0].panes[&split_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&split_terminal_id)
            .unwrap()
            .state = AgentState::Idle;
        app.state.workspaces[0].tabs[0]
            .panes
            .get_mut(&split_pane)
            .unwrap()
            .seen = false;
        let bg_terminal_id = app.state.workspaces[0].tabs[background_tab].panes[&background_pane]
            .attached_terminal_id
            .clone();
        app.state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Idle;
        app.state.workspaces[0].tabs[background_tab]
            .panes
            .get_mut(&background_pane)
            .unwrap()
            .seen = false;

        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.outer_terminal_focus = Some(false);

        let handled = app
            .handle_raw_input_event(crate::raw_input::RawInputEvent::OuterFocusGained)
            .await;

        assert!(handled);
        assert_eq!(app.state.outer_terminal_focus, Some(true));
        assert!(app.state.workspaces[0].tabs[0].panes[&root_pane].seen);
        assert!(app.state.workspaces[0].tabs[0].panes[&split_pane].seen);
        assert!(!app.state.workspaces[0].tabs[background_tab].panes[&background_pane].seen);
    }

    #[tokio::test]
    async fn outer_focus_gained_does_not_require_full_redraw_when_disabled() {
        let mut app = test_app();
        app.state.redraw_on_focus_gained = false;

        let handled = app
            .handle_raw_input_event(crate::raw_input::RawInputEvent::OuterFocusGained)
            .await;

        assert!(handled);
        assert_eq!(app.state.outer_terminal_focus, Some(true));
        assert!(!app.full_redraw_pending);
        assert_eq!(app.host_terminal_theme_query_count.get(), 1);
    }

    #[tokio::test]
    async fn repeat_key_events_are_ignored_outside_terminal_mode() {
        let mut app = test_app();
        app.state.mode = Mode::ReleaseNotes;
        app.state.release_notes = Some(release_notes_state());

        let handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Repeat,
            ))
            .await;

        assert!(!handled);
        assert_eq!(app.state.mode, Mode::ReleaseNotes);
        assert!(app.state.release_notes.is_some());
    }

    #[tokio::test]
    async fn command_palette_handles_repeated_arrow_keys() {
        let mut app = test_app();
        app.state.mode = Mode::CommandPalette;

        let press_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Down,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ))
            .await;
        let repeat_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Down,
                KeyModifiers::empty(),
                KeyEventKind::Repeat,
            ))
            .await;

        assert!(press_handled);
        assert!(repeat_handled);
        assert_eq!(app.state.command_palette.selected, 2);
    }

    #[tokio::test]
    async fn settings_handles_repeated_navigation_keys() {
        let mut app = test_app();
        app.state.mode = Mode::Settings;
        app.state.settings.section = state::SettingsSection::Theme;

        let press_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Down,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ))
            .await;
        let repeat_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Down,
                KeyModifiers::empty(),
                KeyEventKind::Repeat,
            ))
            .await;

        assert!(press_handled);
        assert!(repeat_handled);
        assert_eq!(app.state.settings.list.selected, 1);
    }

    #[tokio::test]
    async fn settings_ignores_repeated_confirm_keys() {
        let mut app = test_app();
        app.state.mode = Mode::Settings;
        app.state.settings.section = state::SettingsSection::Sound;

        let press_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ))
            .await;
        let repeat_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Repeat,
            ))
            .await;

        assert!(press_handled);
        assert!(!repeat_handled);
    }

    #[tokio::test]
    async fn modal_press_does_not_leak_repeat_into_terminal_mode() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::ReleaseNotes;
        app.state.release_notes = Some(release_notes_state());

        let press_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ))
            .await;
        let repeat_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Repeat,
            ))
            .await;
        let release_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Release,
            ))
            .await;
        let next_press_handled = app
            .handle_raw_input_event(raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ))
            .await;

        assert!(press_handled);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(!repeat_handled);
        assert!(!release_handled);
        assert!(next_press_handled);
    }

    #[test]
    fn read_only_api_requests_do_not_force_rerender() {
        let read_only = crate::api::schema::Request {
            id: "req_1".into(),
            method: crate::api::schema::Method::WorkspaceList(
                crate::api::schema::EmptyParams::default(),
            ),
        };
        let mutating = crate::api::schema::Request {
            id: "req_2".into(),
            method: crate::api::schema::Method::WorkspaceFocus(
                crate::api::schema::WorkspaceTarget {
                    workspace_id: "w_1".into(),
                },
            ),
        };
        let pane_rename = crate::api::schema::Request {
            id: "req_3".into(),
            method: crate::api::schema::Method::PaneRename(crate::api::schema::PaneRenameParams {
                pane_id: "w_1-1".into(),
                label: Some("logs".into()),
            }),
        };
        let worktree_list = crate::api::schema::Request {
            id: "req_4".into(),
            method: crate::api::schema::Method::WorktreeList(
                crate::api::schema::WorktreeListParams::default(),
            ),
        };
        let worktree_create = crate::api::schema::Request {
            id: "req_5".into(),
            method: crate::api::schema::Method::WorktreeCreate(
                crate::api::schema::WorktreeCreateParams::default(),
            ),
        };

        assert!(!crate::api::request_changes_ui(&read_only));
        assert!(!crate::api::request_changes_ui(&worktree_list));
        assert!(crate::api::request_changes_ui(&mutating));
        assert!(crate::api::request_changes_ui(&pane_rename));
        assert!(crate::api::request_changes_ui(&worktree_create));
    }

    #[test]
    fn workspace_create_response_includes_initial_tab_and_root_pane() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("api-root-pane")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let crate::api::schema::ResponseResult::WorkspaceCreated {
            workspace,
            tab,
            root_pane,
        } = app.workspace_created_result(0).unwrap()
        else {
            panic!("expected workspace_created response");
        };

        assert_eq!(workspace.label, "api-root-pane");
        assert_eq!(tab.workspace_id, workspace.workspace_id);
        assert_eq!(root_pane.workspace_id, workspace.workspace_id);
        assert_eq!(root_pane.tab_id, tab.tab_id);
        assert!(root_pane.terminal_id.starts_with("term_"));
        assert_ne!(root_pane.terminal_id, root_pane.pane_id);
    }

    #[test]
    fn tab_create_response_includes_root_pane() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("api-tab-root-pane");
        workspace.test_add_tab(None);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let crate::api::schema::ResponseResult::TabCreated { tab, root_pane } =
            app.tab_created_result(0, 1).unwrap()
        else {
            panic!("expected tab_created response");
        };

        assert_eq!(tab.workspace_id, root_pane.workspace_id);
        assert_eq!(root_pane.tab_id, tab.tab_id);
        assert_eq!(tab.pane_count, 1);
    }

    #[test]
    fn workspace_creation_in_navigate_mode_uses_selected_workspace_seed_cwd() {
        let mut app = test_app();
        let mut first = Workspace::test_new("hako");
        first.identity_cwd = std::path::PathBuf::from("/tmp/hako");
        let mut second = Workspace::test_new("pion");
        second.identity_cwd = std::path::PathBuf::from("/tmp/pion");

        app.state.workspaces = vec![first, second];
        app.state.ensure_test_terminals();
        let selected_root = app.state.workspaces[1].tabs[0].root_pane;
        let selected_terminal_id = app.state.workspaces[1]
            .terminal_id(selected_root)
            .unwrap()
            .clone();
        app.state
            .terminals
            .get_mut(&selected_terminal_id)
            .unwrap()
            .cwd = std::path::PathBuf::from("/tmp/pion-runtime");
        app.state.active = Some(0);
        app.state.selected = 1;
        app.state.mode = Mode::Navigate;

        let ws_idx = app.workspace_creation_source().unwrap();
        let seed_cwd = app.seed_cwd_from_workspace(ws_idx).unwrap();

        assert_eq!(ws_idx, 1);
        assert_eq!(seed_cwd, std::path::PathBuf::from("/tmp/pion-runtime"));
    }

    #[test]
    fn workspace_creation_in_all_mode_uses_selected_workspace_group() {
        let mut app = test_app();
        let work_group = app.state.create_group("Work".to_string());
        let mut first = Workspace::test_new("home");
        first.group_id = app.state.groups[0].id.clone();
        let mut second = Workspace::test_new("api");
        second.group_id = app.state.groups[work_group].id.clone();
        app.state.workspaces = vec![first, second];
        app.state.active = Some(0);
        app.state.selected = 1;
        app.state.active_group = 0;
        app.state.group_filter_enabled = false;
        app.state.mode = Mode::Navigate;

        let source = app.workspace_creation_source();
        let group_id = app.workspace_creation_group_id(source);

        assert_eq!(source, Some(1));
        assert_eq!(group_id, app.state.groups[work_group].id);
    }

    #[test]
    fn workspace_creation_in_empty_group_uses_active_group() {
        let mut app = test_app();
        let group_two = app.state.create_group("two".to_string());
        let group_three = app.state.create_group("three".to_string());
        let mut first = Workspace::test_new("first");
        first.group_id = app.state.groups[group_two].id.clone();
        let mut second = Workspace::test_new("second");
        second.group_id = app.state.groups[group_two].id.clone();
        app.state.workspaces = vec![first, second];
        app.state.active_group = group_three;
        app.state.group_filter_enabled = true;
        app.state.active = None;
        app.state.selected = 0;
        app.state.mode = Mode::Navigate;

        let source = app.workspace_creation_source();
        let group_id = app.workspace_creation_group_id(source);

        assert_eq!(source, None);
        assert_eq!(group_id, app.state.groups[group_three].id);
    }

    #[test]
    fn workspace_creation_prefers_group_default_directory_over_source_space() {
        let mut app = test_app();
        let group_idx = app.state.create_group("Work".to_string());
        let group_id = app.state.groups[group_idx].id.clone();
        app.state
            .set_group_default_directory(group_idx, Some(std::path::PathBuf::from("/tmp/group")));
        let mut source = Workspace::test_new("source");
        source.group_id = group_id.clone();
        source.default_cwd = std::path::PathBuf::from("/tmp/source");
        app.state.workspaces = vec![source];
        app.state.active_group = group_idx;
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Navigate;

        let source = app.workspace_creation_source();
        let group_id = app.workspace_creation_group_id(source);
        let cwd = app.group_default_directory(&group_id).unwrap_or_else(|| {
            let follow_cwd = source.and_then(|ws_idx| app.seed_cwd_from_workspace(ws_idx));
            app.resolve_new_terminal_cwd(follow_cwd)
        });

        assert_eq!(cwd, std::path::PathBuf::from("/tmp/group"));
    }

    #[test]
    fn workspace_creation_names_duplicate_cwd_labels_with_suffix() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("hako"), Workspace::test_new("hako 2")];

        let name = app.collision_free_workspace_name(
            std::path::Path::new("/tmp/hako"),
            app.state.active_group_id(),
        );

        assert_eq!(name.as_deref(), Some("hako 3"));
    }

    #[test]
    fn workspace_creation_suffixes_only_within_active_group() {
        let mut app = test_app();
        let work_group = app.state.create_group("Work".to_string());
        let mut existing = Workspace::test_new("hako");
        existing.group_id = app.state.groups[work_group].id.clone();
        app.state.workspaces = vec![existing];
        app.state.active_group = 0;

        let name = app.collision_free_workspace_name(
            std::path::Path::new("/tmp/hako"),
            app.state.active_group_id(),
        );

        assert_eq!(name, None);
    }

    #[test]
    fn new_terminal_cwd_follow_uses_source_cwd() {
        let cwd = creation::resolve_new_terminal_cwd(
            &crate::config::NewTerminalCwdConfig::Follow,
            Some(std::path::PathBuf::from("/tmp/hako-source")),
        );

        assert_eq!(cwd, std::path::PathBuf::from("/tmp/hako-source"));
    }

    #[test]
    fn new_terminal_cwd_path_uses_configured_path() {
        let cwd = creation::resolve_new_terminal_cwd(
            &crate::config::NewTerminalCwdConfig::Path("/tmp/hako-fixed".into()),
            Some(std::path::PathBuf::from("/tmp/hako-source")),
        );

        assert_eq!(cwd, std::path::PathBuf::from("/tmp/hako-fixed"));
    }

    #[test]
    fn server_stop_request_sets_should_quit_flag() {
        let mut app = test_app();

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_server_stop".into(),
            method: crate::api::schema::Method::ServerStop(
                crate::api::schema::EmptyParams::default(),
            ),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "ok");
        assert!(app.state.should_quit);
    }

    #[test]
    fn pane_rename_request_sets_and_clears_manual_label() {
        let mut app = test_app();
        let workspace = Workspace::test_new("api-pane-rename");
        let pane = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let pane_id = app.pane_info(0, pane).unwrap().pane_id;
        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_rename".into(),
            method: crate::api::schema::Method::PaneRename(crate::api::schema::PaneRenameParams {
                pane_id: pane_id.clone(),
                label: Some("reviewer".into()),
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "pane_info");
        assert_eq!(response["result"]["pane"]["label"], "reviewer");
        let terminal_id = app.state.workspaces[0]
            .pane_state(pane)
            .unwrap()
            .attached_terminal_id
            .clone();
        assert_eq!(
            app.state
                .terminals
                .get(&terminal_id)
                .unwrap()
                .manual_label
                .as_deref(),
            Some("reviewer")
        );

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_rename_clear".into(),
            method: crate::api::schema::Method::PaneRename(crate::api::schema::PaneRenameParams {
                pane_id,
                label: None,
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "pane_info");
        assert!(response["result"]["pane"].get("label").is_none());
        assert!(app
            .state
            .terminals
            .get(&terminal_id)
            .unwrap()
            .manual_label
            .is_none());
    }

    #[test]
    fn terminal_target_resolves_terminal_id() {
        let mut app = test_app();
        let workspace = Workspace::test_new("terminal-target-id");
        let pane = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane).unwrap().to_string();
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;

        let resolved = app.resolve_terminal_target(&terminal_id).unwrap();

        assert_eq!(resolved.ws_idx, 0);
        assert_eq!(resolved.pane_id, pane);
        assert_eq!(resolved.terminal_id, terminal_id);
    }

    #[test]
    fn terminal_target_resolves_legacy_pane_id() {
        let mut app = test_app();
        let workspace = Workspace::test_new("terminal-target-pane");
        let pane = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane).unwrap().to_string();
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        let pane_id = app.public_pane_id(0, pane).unwrap();

        let resolved = app.resolve_terminal_target(&pane_id).unwrap();

        assert_eq!(resolved.pane_id, pane);
        assert_eq!(resolved.terminal_id, terminal_id);
    }

    #[test]
    fn terminal_target_resolves_unique_agent_name() {
        let mut app = test_app();
        let workspace = Workspace::test_new("terminal-target-name");
        let pane = workspace.tabs[0].root_pane;
        let terminal_id = workspace.terminal_id(pane).unwrap().to_string();
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        let attached_terminal_id = app.state.workspaces[0]
            .pane_state(pane)
            .unwrap()
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&attached_terminal_id)
            .unwrap()
            .set_agent_name("reviewer".into());
        app.state.active = Some(0);
        app.state.selected = 0;

        let resolved = app.resolve_terminal_target("reviewer").unwrap();

        assert_eq!(resolved.pane_id, pane);
        assert_eq!(resolved.terminal_id, terminal_id);
    }

    #[test]
    fn terminal_target_reports_missing_target() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("terminal-target-missing")];
        app.state.active = Some(0);
        app.state.selected = 0;

        let err = app.resolve_terminal_target("missing-agent").unwrap_err();

        assert_eq!(
            err,
            crate::app::terminal_targets::TerminalTargetError::NotFound {
                target: "missing-agent".into()
            }
        );
    }

    #[test]
    fn terminal_target_reports_ambiguous_duplicate_agent_name() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("terminal-target-ambiguous");
        let first = workspace.tabs[0].root_pane;
        let second = workspace.test_split(ratatui::layout::Direction::Horizontal);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0]
            .pane_state(first)
            .unwrap()
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .set_agent_name("worker".into());
        let second_terminal_id = app.state.workspaces[0]
            .pane_state(second)
            .unwrap()
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .set_agent_name("worker".into());
        app.state.active = Some(0);
        app.state.selected = 0;

        let err = app.resolve_terminal_target("worker").unwrap_err();

        let crate::app::terminal_targets::TerminalTargetError::Ambiguous { target, candidates } =
            err
        else {
            panic!("expected ambiguous terminal target");
        };
        assert_eq!(target, "worker");
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(|candidate| {
            candidate.terminal_id.starts_with("term_")
                && candidate.pane_id.starts_with(&app.state.workspaces[0].id)
                && candidate.workspace_id == app.state.workspaces[0].id
                && candidate.cwd.is_some()
        }));
    }

    #[tokio::test]
    async fn pane_split_request_targets_pane_in_background_tab() {
        let _guard = config_env_lock().lock().unwrap();
        let _shell_env = crate::config::TestEnvVar::set("SHELL", "/usr/bin/true");

        let mut app = test_app();
        let mut workspace = Workspace::test_new("api-pane-split-background-tab");
        let active_pane = workspace.tabs[0].root_pane;
        let background_tab = workspace.test_add_tab(Some("worker"));
        let target_pane = workspace.tabs[background_tab].root_pane;
        workspace.switch_tab(background_tab);
        let background_previous_focus =
            workspace.test_split(ratatui::layout::Direction::Horizontal);
        workspace.switch_tab(0);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        let split_cwd = std::env::temp_dir();
        let target_terminal_id = app.state.workspaces[0]
            .pane_state(target_pane)
            .unwrap()
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&target_terminal_id)
            .unwrap()
            .cwd = split_cwd.clone();
        app.state.active = Some(0);
        app.state.selected = 0;

        let target_pane_id = app.pane_info(0, target_pane).unwrap().pane_id;
        let target_tab_id = app.public_tab_id(0, background_tab).unwrap();

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_split_background_tab".into(),
            method: crate::api::schema::Method::PaneSplit(crate::api::schema::PaneSplitParams {
                workspace_id: None,
                target_pane_id: Some(target_pane_id),
                direction: crate::api::schema::SplitDirection::Right,
                ratio: None,
                cwd: None,
                focus: false,
                env: Default::default(),
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "pane_info");
        assert_eq!(response["result"]["pane"]["tab_id"], target_tab_id);
        let response_cwd =
            std::path::PathBuf::from(response["result"]["pane"]["cwd"].as_str().unwrap());
        assert_eq!(
            crate::worktree::canonical_or_original(&response_cwd),
            crate::worktree::canonical_or_original(&split_cwd)
        );
        assert_eq!(response["result"]["pane"]["focused"], false);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.workspaces[0].active_tab, 0);
        assert_eq!(
            app.state.workspaces[0].tabs[0].layout.focused(),
            active_pane
        );
        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 1);
        assert_eq!(
            app.state.workspaces[0].tabs[background_tab]
                .layout
                .focused(),
            background_previous_focus
        );
        assert_eq!(
            app.state.workspaces[0].tabs[background_tab]
                .layout
                .pane_count(),
            3
        );

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pane_split_request_focuses_new_pane_when_requested() {
        let _guard = config_env_lock().lock().unwrap();
        let _shell_env = crate::config::TestEnvVar::set("SHELL", "/usr/bin/true");

        let mut app = test_app();
        let mut workspace = Workspace::test_new("api-pane-split-focus-background-tab");
        let background_tab = workspace.test_add_tab(Some("worker"));
        workspace.switch_tab(0);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let target_pane = app.state.workspaces[0].tabs[background_tab].root_pane;
        let target_pane_id = app.pane_info(0, target_pane).unwrap().pane_id;
        let target_tab_id = app.public_tab_id(0, background_tab).unwrap();

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_split_focus_background_tab".into(),
            method: crate::api::schema::Method::PaneSplit(crate::api::schema::PaneSplitParams {
                workspace_id: None,
                target_pane_id: Some(target_pane_id),
                direction: crate::api::schema::SplitDirection::Right,
                ratio: None,
                cwd: None,
                focus: true,
                env: Default::default(),
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "pane_info");
        assert_eq!(response["result"]["pane"]["tab_id"], target_tab_id);
        assert_eq!(response["result"]["pane"]["focused"], true);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.workspaces[0].active_tab, background_tab);

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pane_split_request_applies_ratio() {
        let _guard = config_env_lock().lock().unwrap();
        let _shell_env = crate::config::TestEnvVar::set("SHELL", "/usr/bin/true");

        let mut app = test_app();
        let workspace = Workspace::test_new("api-pane-split-ratio");
        let target_pane = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let target_pane_id = app.pane_info(0, target_pane).unwrap().pane_id;

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_split_ratio".into(),
            method: crate::api::schema::Method::PaneSplit(crate::api::schema::PaneSplitParams {
                workspace_id: None,
                target_pane_id: Some(target_pane_id),
                direction: crate::api::schema::SplitDirection::Right,
                ratio: Some(0.333),
                cwd: None,
                focus: false,
                env: Default::default(),
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "pane_info");
        let splits = app.state.workspaces[0].tabs[0]
            .layout
            .splits(ratatui::layout::Rect::new(0, 0, 100, 20));
        assert_eq!(splits.len(), 1);
        assert_eq!(splits[0].pos, 33);

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn pane_split_request_uses_active_focused_pane_when_target_is_omitted() {
        let _guard = config_env_lock().lock().unwrap();
        let _shell_env = crate::config::TestEnvVar::set("SHELL", "/usr/bin/true");

        let mut app = test_app();
        let workspace = Workspace::test_new("api-pane-split-current");
        let target_pane = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.focus_pane_in_workspace(0, target_pane);

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_split_current".into(),
            method: crate::api::schema::Method::PaneSplit(crate::api::schema::PaneSplitParams {
                workspace_id: None,
                target_pane_id: None,
                direction: crate::api::schema::SplitDirection::Right,
                ratio: None,
                cwd: None,
                focus: false,
                env: Default::default(),
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "pane_info");
        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
        assert_eq!(
            app.state.workspaces[0].tabs[0].layout.focused(),
            target_pane
        );

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[tokio::test]
    async fn focused_agent_start_records_previous_pane() {
        let mut app = test_app();
        let workspace = Workspace::test_new("agent-start-focus");
        let root = workspace.tabs[0].root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_agent_start_focus".into(),
            method: crate::api::schema::Method::AgentStart(crate::api::schema::AgentStartParams {
                name: "worker".into(),
                cwd: None,
                workspace_id: None,
                tab_id: None,
                split: Some(crate::api::schema::SplitDirection::Right),
                focus: true,
                argv: vec!["/usr/bin/true".into()],
                env: Default::default(),
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "agent_started");
        assert_ne!(app.state.workspaces[0].focused_pane_id(), Some(root));

        app.state.last_pane();

        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.workspaces[0].focused_pane_id(), Some(root));

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[test]
    fn pane_close_request_closes_only_the_target_tab_when_other_tabs_exist() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("api-pane-close");
        let second_tab = workspace.test_add_tab(Some("logs"));
        workspace.switch_tab(second_tab);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let target_pane = app.state.workspaces[0].tabs[second_tab].root_pane;
        let target_pane_id = app.pane_info(0, target_pane).unwrap().pane_id;

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_close".into(),
            method: crate::api::schema::Method::PaneClose(crate::api::schema::PaneTarget {
                pane_id: target_pane_id,
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "ok");
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
        assert_eq!(app.state.workspaces[0].display_name(), "api-pane-close");
    }

    #[test]
    fn pane_close_request_deletes_workspace_when_it_removes_the_last_pane() {
        let mut app = test_app();
        let workspace = Workspace::test_new("api-pane-close-last");
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;

        let target_pane = app.state.workspaces[0].tabs[0].root_pane;
        let target_pane_id = app.pane_info(0, target_pane).unwrap().pane_id;

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_close_last".into(),
            method: crate::api::schema::Method::PaneClose(crate::api::schema::PaneTarget {
                pane_id: target_pane_id,
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["result"]["type"], "ok");
        assert!(app.state.workspaces.is_empty());
        assert_eq!(app.state.active, None);
    }

    #[test]
    fn pane_close_request_requires_confirmation_before_closing_parent_worktree_group() {
        let mut app = test_app();
        let mut parent = Workspace::test_new("api-pane-close-parent");
        parent.worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: "/repo/herdr".into(),
            is_linked_worktree: false,
        });
        let mut child = Workspace::test_new("api-pane-close-child");
        child.worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "herdr".into(),
            repo_root: "/repo/herdr".into(),
            checkout_path: "/repo/herdr-child".into(),
            is_linked_worktree: true,
        });
        app.state.workspaces = vec![parent, child];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 1;

        let target_pane = app.state.workspaces[0].tabs[0].root_pane;
        let target_pane_id = app.pane_info(0, target_pane).unwrap().pane_id;

        let response = app.handle_api_request(crate::api::schema::Request {
            id: "req_pane_close_parent_group".into(),
            method: crate::api::schema::Method::PaneClose(crate::api::schema::PaneTarget {
                pane_id: target_pane_id,
            }),
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();

        assert_eq!(response["error"]["code"], "confirmation_required");
        assert_eq!(app.state.mode, Mode::ConfirmClose);
        assert_eq!(app.state.selected, 0);
        assert_eq!(app.state.workspaces.len(), 2);
    }

    #[test]
    fn session_dirty_flag_schedules_debounced_save() {
        let mut app = test_app();
        app.no_session = false;
        app.state.session_dirty = true;

        app.sync_session_save_schedule();

        assert!(!app.state.session_dirty);
        assert!(app.session_save_deadline.is_some());
    }

    #[test]
    fn next_loop_deadline_includes_session_save_deadline() {
        let mut app = test_app();
        let now = Instant::now();
        app.session_save_deadline = Some(now + Duration::from_secs(2));
        app.next_resize_poll = now + Duration::from_secs(5);
        app.next_command_scan = now + Duration::from_secs(5);
        app.next_auto_update_check = Some(now + Duration::from_secs(6));

        assert_eq!(
            app.next_loop_deadline(now, false),
            app.session_save_deadline
        );
    }

    #[test]
    fn headless_next_loop_deadline_ignores_resize_poll() {
        let mut app = test_app();
        let now = Instant::now();
        app.next_resize_poll = now + Duration::from_millis(100);
        app.session_save_deadline = Some(now + Duration::from_secs(2));
        app.next_command_scan = now + Duration::from_secs(5);
        app.next_auto_update_check = Some(now + Duration::from_secs(6));

        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, true),
            app.session_save_deadline
        );
    }

    #[test]
    fn headless_next_loop_deadline_returns_none_when_resize_poll_is_only_deadline() {
        let mut app = test_app();
        let now = Instant::now();
        app.next_resize_poll = now - Duration::from_millis(1);
        app.config_diagnostic_deadline = None;
        app.toast_deadline = None;
        app.next_animation_tick = None;
        app.next_auto_update_check = None;
        app.session_save_deadline = None;
        app.state.workspaces.clear();

        assert_eq!(
            app.next_headless_loop_deadline_with_git_refresh(now, false, true),
            None
        );
    }

    #[test]
    fn due_session_save_deadline_is_cleared() {
        let mut app = test_app();
        app.session_save_deadline = Some(Instant::now() - Duration::from_secs(1));

        app.handle_scheduled_tasks(Instant::now(), false);

        assert!(app.session_save_deadline.is_none());
    }

    #[test]
    fn next_loop_deadline_includes_selection_autoscroll_deadline() {
        let mut app = test_app();
        let now = Instant::now();
        app.selection_autoscroll_deadline = Some(now + Duration::from_millis(5));
        app.next_animation_tick = Some(now + Duration::from_millis(100));
        app.session_save_deadline = Some(now + Duration::from_millis(200));
        app.next_resize_poll = now + Duration::from_secs(5);
        app.next_command_scan = now + Duration::from_secs(5);
        app.next_auto_update_check = Some(now + Duration::from_secs(6));
        assert_eq!(
            app.next_loop_deadline(now, false),
            app.selection_autoscroll_deadline
        );
    }

    #[test]
    fn tick_selection_autoscroll_self_heals_when_state_cleared() {
        let mut app = test_app();
        let now = Instant::now();
        app.state.selection_autoscroll = None;
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[test]
    fn tick_selection_autoscroll_stops_on_rect_change() {
        let mut app = test_app();
        let now = Instant::now();
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        app.state.workspaces.push(ws);
        app.state.active = Some(0);
        app.state.selection = Some(crate::selection::Selection::anchor(pane_id, 0, 0, None));
        // Set autoscroll with a stale inner_rect that doesn't match pane_infos
        app.state.selection_autoscroll = Some(state::SelectionAutoscroll {
            direction: state::SelectionAutoscrollDirection::Down,
            last_mouse_screen_col: 0,
            last_mouse_screen_row: 999,
            inner_rect: ratatui::layout::Rect::new(0, 0, 1, 1), // wrong rect
        });
        app.selection_autoscroll_deadline = Some(now);
        app.tick_selection_autoscroll(now);
        assert!(app.state.selection_autoscroll.is_none());
        assert!(app.selection_autoscroll_deadline.is_none());
    }

    #[tokio::test]
    async fn full_internal_event_queue_eventually_applies_working_to_idle_transition() {
        let mut app = test_app();
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;

        app.state.workspaces = vec![ws];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let terminal_id = app.state.workspaces[0]
            .pane_state(pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        app.handle_internal_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        assert_eq!(
            app.state.terminals.get(&terminal_id).unwrap().state,
            AgentState::Working
        );

        for i in 0..64 {
            app.event_tx
                .try_send(AppEvent::UpdateReady {
                    version: format!("9.9.{i}"),
                    install_command: "hako update".into(),
                })
                .unwrap();
        }

        let tx = app.event_tx.clone();
        let send = tx.send(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            observed_at: std::time::Instant::now(),
        });
        tokio::pin!(send);

        let blocked =
            tokio::time::timeout(Duration::from_millis(20), async { (&mut send).await }).await;
        assert!(
            blocked.is_err(),
            "state change sender should wait for queue space instead of failing"
        );

        app.drain_internal_events();

        tokio::time::timeout(Duration::from_millis(50), async { (&mut send).await })
            .await
            .expect("state change should enqueue once queue space is available")
            .expect("app event receiver should still be alive");

        app.drain_internal_events();

        assert_eq!(
            app.state.terminals.get(&terminal_id).unwrap().state,
            AgentState::Idle,
            "Working→Idle should still apply after temporary queue pressure"
        );
    }

    #[test]
    fn route_client_input_dispatches_navigate_mode_keybinds() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;

        // Start in navigate mode.
        app.state.mode = Mode::Navigate;

        // Send Ctrl+B then Esc (prefix → leave navigate mode).
        // Ctrl+B is 0x02 in raw terminal input.
        // After entering navigate mode and pressing Esc, we should leave navigate mode.
        let esc_bytes = vec![0x1b]; // Esc
        app.route_client_input(esc_bytes);
        // Esc in navigate mode should leave navigate mode.
        assert_eq!(
            app.state.mode,
            Mode::Terminal,
            "Esc should leave navigate mode and return to Terminal mode"
        );
    }

    #[test]
    fn route_client_input_q_detaches_in_persistence_mode() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.detach_exits = false;

        // Start in navigate mode.
        app.state.mode = Mode::Navigate;
        assert!(!app.state.detach_requested);

        let q_bytes = b"q".to_vec();
        app.route_client_input(q_bytes);

        assert!(
            app.state.detach_requested,
            "q should detach in persistence mode"
        );
        assert_eq!(
            app.state.mode,
            Mode::Terminal,
            "q should leave navigate mode"
        );
    }

    #[test]
    fn route_client_input_prefix_then_q_detaches_in_persistence_mode() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.detach_exits = false;

        // Start in terminal mode (default after workspace creation).
        app.state.mode = Mode::Terminal;
        assert!(!app.state.detach_requested);

        // Send Ctrl+B (prefix key, raw byte 0x02).
        let prefix_bytes = vec![0x02];
        app.route_client_input(prefix_bytes);

        assert_eq!(
            app.state.mode,
            Mode::Prefix,
            "prefix key should enter prefix mode"
        );
        assert!(
            !app.state.detach_requested,
            "prefix key should not set detach flag"
        );

        let q_bytes = b"q".to_vec();
        app.route_client_input(q_bytes);

        assert!(
            app.state.detach_requested,
            "q should detach in persistence mode"
        );
        assert_eq!(
            app.state.mode,
            Mode::Terminal,
            "q should leave navigate mode"
        );
    }

    #[tokio::test]
    async fn route_client_input_double_prefix_passes_prefix_through_to_focused_pane() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let focused = workspace.focused_pane_id().unwrap();
        let (runtime, mut rx) = TerminalRuntime::test_with_channel(80, 24);
        workspace.tabs[0].runtimes.insert(focused, runtime);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.prefix_code = KeyCode::Char('l');
        app.state.prefix_mods = KeyModifiers::CONTROL;

        app.route_client_input(vec![0x0c]);
        assert_eq!(app.state.mode, Mode::Prefix);

        app.route_client_input(vec![0x0c]);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(rx.recv().await.unwrap(), bytes::Bytes::from(vec![0x0c]));
    }

    #[tokio::test]
    async fn route_client_input_reencodes_terminal_keys_for_focused_pane_protocol() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let focused = workspace.focused_pane_id().unwrap();
        let (runtime, mut rx) = TerminalRuntime::test_with_channel(80, 24);
        workspace.tabs[0].runtimes.insert(focused, runtime);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        // Ghostty/kitty-style Ctrl-C should be normalized back to the pane's
        // negotiated encoding instead of being forwarded verbatim.
        app.route_client_input(b"\x1b[99;5u".to_vec());

        assert_eq!(rx.recv().await.unwrap(), bytes::Bytes::from(vec![3]));

        // iTerm2 and rxvt-style hosts may send F4 as CSI 14~. Normalize it
        // through the same semantic key path instead of leaking host bytes.
        app.route_client_input(b"\x1b[14~".to_vec());

        assert_eq!(
            rx.recv().await.unwrap(),
            bytes::Bytes::from_static(b"\x1bOS")
        );
    }

    #[tokio::test]
    async fn route_client_input_preserves_shift_enter_for_modify_other_keys_pane() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let focused = workspace.focused_pane_id().unwrap();
        let (runtime, mut rx) =
            TerminalRuntime::test_with_channel_and_scrollback_bytes(80, 24, 0, b"\x1b[>4;1m", 4);
        workspace.tabs[0].runtimes.insert(focused, runtime);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.route_client_input(b"\x1b[13;2u".to_vec());

        assert_eq!(
            rx.recv().await.unwrap(),
            bytes::Bytes::from_static(b"\x1b[27;2;13~")
        );
    }

    #[tokio::test]
    async fn route_client_input_splits_multi_event_payloads_before_forwarding() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let focused = workspace.focused_pane_id().unwrap();
        let (runtime, mut rx) = TerminalRuntime::test_with_channel(80, 24);
        workspace.tabs[0].runtimes.insert(focused, runtime);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.route_client_input(b"ab".to_vec());

        assert_eq!(rx.recv().await.unwrap(), bytes::Bytes::from_static(b"a"));
        assert_eq!(rx.recv().await.unwrap(), bytes::Bytes::from_static(b"b"));
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn route_client_input_forwards_multilingual_ime_text_to_focused_pane() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let focused = workspace.focused_pane_id().unwrap();
        let text = "中日한🙂";
        let (runtime, mut rx) =
            TerminalRuntime::test_with_channel_capacity(80, 24, text.chars().count());
        workspace.tabs[0].runtimes.insert(focused, runtime);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.route_client_input(text.as_bytes().to_vec());

        let mut forwarded = Vec::new();
        for _ in text.chars() {
            let chunk = rx.recv().await.unwrap();
            forwarded.extend_from_slice(&chunk);
        }
        assert_eq!(forwarded, text.as_bytes());
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn route_client_input_forwards_long_voice_like_cjk_text_without_truncation() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        let focused = workspace.focused_pane_id().unwrap();
        let text = "你好，今天我们测试一段比较长的语音输入。こんにちは。안녕하세요.🙂".repeat(64);
        let char_count = text.chars().count();
        let (runtime, mut rx) = TerminalRuntime::test_with_channel_capacity(80, 24, char_count);
        workspace.tabs[0].runtimes.insert(focused, runtime);
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.route_client_input(text.as_bytes().to_vec());

        let mut forwarded = Vec::new();
        for _ in 0..char_count {
            let chunk = rx.recv().await.unwrap();
            forwarded.extend_from_slice(&chunk);
        }
        assert_eq!(forwarded, text.as_bytes());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn route_client_input_advances_onboarding_modal() {
        let mut app = test_app();
        app.state.mode = Mode::Onboarding;

        app.route_client_input(b"\r".to_vec());

        assert_eq!(app.state.mode, Mode::Settings);
        assert_eq!(
            app.state.settings.section,
            state::SettingsSection::Integrations
        );
    }

    #[test]
    fn route_client_input_pastes_bracketed_text_into_rename_modal() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::RenameTab;
        app.state.name_input = "2".into();
        app.state.name_input_replace_on_type = true;

        app.route_client_input(b"\x1b[200~feature/logs\x1b[201~".to_vec());

        assert_eq!(app.state.name_input, "feature/logs");
        assert!(!app.state.name_input_replace_on_type);
    }

    #[test]
    fn raw_ctrl_v_decodes_as_modal_paste_shortcut() {
        let events = crate::raw_input::parse_raw_input_bytes_sync(&[0x16]);
        let Some(crate::raw_input::RawInputEvent::Key(key)) = events.first() else {
            panic!("expected ctrl-v key event");
        };

        assert!(input::is_modal_paste_shortcut(&key.as_key_event()));
    }

    #[test]
    fn route_client_events_pastes_text_into_worktree_directory_modal() {
        let mut app = test_app();
        app.state.mode = Mode::EditWorktreeDirectory;
        app.state.name_input = "/tmp/hako".into();
        app.state.name_input_replace_on_type = true;

        app.route_client_events(
            vec![crate::raw_input::RawInputEvent::Paste(
                "/tmp/hako-worktrees".into(),
            )],
            true,
        );

        assert_eq!(app.state.name_input, "/tmp/hako-worktrees");
        assert!(!app.state.name_input_replace_on_type);
    }

    #[test]
    fn route_client_events_for_view_keeps_tab_navigation_client_local() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        workspace.test_add_tab(Some("logs"));
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        let second_client = ClientViewState::from_app_state(&app.state);

        app.route_client_events_for_view(
            &mut first_client,
            vec![
                raw_key(
                    KeyCode::Char('b'),
                    KeyModifiers::CONTROL,
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char('2'),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
            ],
            true,
        );

        assert_eq!(
            first_client.active_tab_for_workspace(&app.state.workspaces[0].id),
            Some(1)
        );
        assert_eq!(
            second_client.active_tab_for_workspace(&app.state.workspaces[0].id),
            Some(0)
        );
        assert_eq!(app.state.workspaces[0].active_tab_index(), 0);
    }

    #[test]
    fn route_client_events_for_view_keeps_open_settings_client_local() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        let second_client = ClientViewState::from_app_state(&app.state);

        app.route_client_events_for_view(
            &mut first_client,
            vec![
                raw_key(
                    KeyCode::Char('b'),
                    KeyModifiers::CONTROL,
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char('s'),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
            ],
            true,
        );

        assert_eq!(first_client.mode, Mode::Settings);
        assert_eq!(second_client.mode, Mode::Terminal);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn route_client_events_for_view_keeps_command_palette_client_local() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        let second_client = ClientViewState::from_app_state(&app.state);

        app.route_client_events_for_view(
            &mut first_client,
            vec![
                raw_key(
                    KeyCode::Char('b'),
                    KeyModifiers::CONTROL,
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char(' '),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
            ],
            true,
        );

        assert_eq!(first_client.mode, Mode::CommandPalette);
        assert_eq!(second_client.mode, Mode::Terminal);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn route_client_events_for_view_prefix_new_tab_queues_shared_tab() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.prompt_new_tab_name = false;

        let mut client = ClientViewState::from_app_state(&app.state);
        let other_client = ClientViewState::from_app_state(&app.state);
        let workspace_id = app.state.workspaces[0].id.clone();

        app.route_client_events_for_view(
            &mut client,
            vec![
                raw_key(
                    KeyCode::Char('b'),
                    KeyModifiers::CONTROL,
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char('c'),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
            ],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert!(app.state.request_new_tab);
        assert_eq!(
            app.state.workspaces[0].tabs.len(),
            1,
            "prefix+c queues creation for the shared app cycle instead of mutating tabs inline"
        );
        assert_eq!(client.active_tab_for_workspace(&workspace_id), Some(0));
        assert_eq!(
            client.pending_active_tabs.get(&workspace_id),
            Some(&1),
            "invoking client tracks the tab that will become active when the server creates it"
        );
        assert_eq!(
            other_client.pending_active_tabs.get(&workspace_id),
            None,
            "other clients must not inherit the invoking client's pending tab focus"
        );
    }

    #[tokio::test]
    async fn route_client_events_for_view_prefix_split_vertical_updates_shared_layout() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);

        app.route_client_events_for_view(
            &mut client,
            vec![
                raw_key(
                    KeyCode::Char('b'),
                    KeyModifiers::CONTROL,
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char('v'),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
            ],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(
            app.state.workspaces[0].tabs[0].layout.pane_count(),
            2,
            "prefix+v through a client view must perform the same shared split-pane action as the normal route"
        );
    }

    #[test]
    fn route_client_events_for_view_updates_command_palette_query_and_selection_locally() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        let second_client = ClientViewState::from_app_state(&app.state);

        app.route_client_events_for_view(
            &mut first_client,
            vec![
                raw_key(
                    KeyCode::Char('b'),
                    KeyModifiers::CONTROL,
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char(' '),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
                raw_key(
                    KeyCode::Char('n'),
                    KeyModifiers::empty(),
                    KeyEventKind::Press,
                ),
                raw_key(KeyCode::Down, KeyModifiers::empty(), KeyEventKind::Press),
            ],
            true,
        );

        assert_eq!(first_client.mode, Mode::CommandPalette);
        assert_eq!(first_client.command_palette.query, "n");
        assert_eq!(first_client.command_palette.selected, 1);
        assert_eq!(second_client.mode, Mode::Terminal);
        assert!(second_client.command_palette.query.is_empty());
        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(app.state.command_palette.query.is_empty());
        assert_eq!(app.state.command_palette.selected, 0);
    }

    #[test]
    fn route_client_events_for_view_accepts_command_palette_shared_toggle_sidebar() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = false;

        let mut client = ClientViewState::from_app_state(&app.state);

        let mut events = vec![
            raw_key(
                KeyCode::Char('b'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            raw_key(
                KeyCode::Char(' '),
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ),
        ];
        for ch in "toggle sidebar".chars() {
            events.push(raw_key(
                KeyCode::Char(ch),
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ));
        }
        events.push(raw_key(
            KeyCode::Enter,
            KeyModifiers::empty(),
            KeyEventKind::Press,
        ));

        app.route_client_events_for_view(&mut client, events, true);

        assert!(app.state.sidebar_collapsed);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(client.mode, Mode::Terminal);
    }

    #[test]
    fn route_client_events_for_view_command_palette_diff_key_sequence_queues_shared_git_diff() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.request_open_git_diff_command = false;

        let mut client = ClientViewState::from_app_state(&app.state);

        let mut events = vec![
            raw_key(
                KeyCode::Char('b'),
                KeyModifiers::CONTROL,
                KeyEventKind::Press,
            ),
            raw_key(
                KeyCode::Char(' '),
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ),
        ];
        for ch in "git diff".chars() {
            events.push(raw_key(
                KeyCode::Char(ch),
                KeyModifiers::empty(),
                KeyEventKind::Press,
            ));
        }
        events.push(raw_key(
            KeyCode::Enter,
            KeyModifiers::empty(),
            KeyEventKind::Press,
        ));

        app.route_client_events_for_view(&mut client, events, true);

        let workspace_id = app.state.workspaces[0].id.clone();
        assert!(app.state.request_open_git_diff_command);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(client.pending_active_tabs.get(&workspace_id), Some(&1));
    }

    fn route_client_command_palette_enter(
        app: &mut App,
        client: &mut ClientViewState,
        query: &str,
    ) {
        input::open_command_palette_for_view(client);
        client.command_palette.query = query.to_string();

        app.route_client_events_for_view(
            client,
            vec![raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );
    }

    #[test]
    fn route_client_events_for_view_command_palette_next_tab_stays_client_local() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("shell");
        workspace.test_add_tab(Some("logs"));
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        let other_client = ClientViewState::from_app_state(&app.state);
        let workspace_id = app.state.workspaces[0].id.clone();

        route_client_command_palette_enter(&mut app, &mut client, "next tab");

        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(client.active_tab_for_workspace(&workspace_id), Some(1));
        assert_eq!(
            other_client.active_tab_for_workspace(&workspace_id),
            Some(0)
        );
        assert_eq!(
            app.state.workspaces[0].active_tab_index(),
            0,
            "command palette tab navigation in one client must not move the shared server focus"
        );
    }

    #[test]
    fn route_client_events_for_view_command_palette_new_tab_opens_client_local_dialog() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);

        route_client_command_palette_enter(&mut app, &mut client, "new tab");

        assert_eq!(client.mode, Mode::RenameTab);
        assert!(client.creating_new_tab);
        assert!(!app.state.request_new_tab);
        assert_eq!(
            app.state.mode,
            Mode::Terminal,
            "new-tab naming stays local to the invoking client until the name is accepted"
        );
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
    }

    #[tokio::test]
    async fn route_client_events_for_view_command_palette_split_pane_updates_shared_layout() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);

        route_client_command_palette_enter(&mut app, &mut client, "split pane vertical");

        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
    }

    #[test]
    fn route_client_events_for_view_accepts_navigator_selection_against_shared_state() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.mode = Mode::Navigator;
        client.navigator.selected = 1;

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.selected, 1);
        assert_eq!(client.mode, Mode::Terminal);
    }

    #[test]
    fn route_client_events_for_view_updates_settings_row_selection_locally() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        first_client.mode = Mode::Settings;
        first_client.settings.section = state::SettingsSection::Theme;
        first_client.settings.list.selected = 0;
        first_client.settings.selection_active = false;
        let second_client = ClientViewState::from_app_state(&app.state);

        app.route_client_events_for_view(
            &mut first_client,
            vec![
                raw_key(KeyCode::Down, KeyModifiers::empty(), KeyEventKind::Press),
                raw_key(KeyCode::Down, KeyModifiers::empty(), KeyEventKind::Press),
            ],
            true,
        );

        assert_eq!(first_client.mode, Mode::Settings);
        assert_eq!(first_client.settings.section, state::SettingsSection::Theme);
        assert_eq!(first_client.settings.list.selected, 1);
        assert_eq!(second_client.mode, Mode::Terminal);
        assert_eq!(second_client.settings.list.selected, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(app.state.settings.list.selected, 0);
    }

    #[test]
    fn route_client_events_for_view_pastes_rename_text_only_into_invoking_client_view() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.name_input = "shared-tab-name".into();
        app.state.name_input_replace_on_type = true;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        first_client.mode = Mode::RenameTab;
        first_client.name_input = "replace-me".into();
        first_client.name_input_replace_on_type = true;
        let mut second_client = ClientViewState::from_app_state(&app.state);
        second_client.mode = Mode::RenameTab;
        second_client.name_input = "other-client-tab".into();
        second_client.name_input_replace_on_type = true;

        app.route_client_events_for_view(
            &mut first_client,
            vec![crate::raw_input::RawInputEvent::Paste(
                "feature/logs".into(),
            )],
            true,
        );

        assert_eq!(first_client.name_input, "feature/logs");
        assert!(!first_client.name_input_replace_on_type);
        assert_eq!(second_client.name_input, "other-client-tab");
        assert!(second_client.name_input_replace_on_type);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(app.state.name_input, "shared-tab-name");
        assert!(app.state.name_input_replace_on_type);
    }

    #[test]
    fn route_client_events_for_view_pastes_worktree_directory_only_into_invoking_client_view() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.name_input = "/shared/worktrees".into();
        app.state.name_input_replace_on_type = true;

        let mut first_client = ClientViewState::from_app_state(&app.state);
        first_client.mode = Mode::EditWorktreeDirectory;
        first_client.name_input = "/tmp/old".into();
        first_client.name_input_replace_on_type = true;
        let mut second_client = ClientViewState::from_app_state(&app.state);
        second_client.mode = Mode::EditWorktreeDirectory;
        second_client.name_input = "/tmp/other".into();
        second_client.name_input_replace_on_type = true;

        app.route_client_events_for_view(
            &mut first_client,
            vec![crate::raw_input::RawInputEvent::Paste(
                "/tmp/hako-worktrees".into(),
            )],
            true,
        );

        assert_eq!(first_client.name_input, "/tmp/hako-worktrees");
        assert!(!first_client.name_input_replace_on_type);
        assert_eq!(second_client.name_input, "/tmp/other");
        assert!(second_client.name_input_replace_on_type);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(app.state.name_input, "/shared/worktrees");
        assert!(app.state.name_input_replace_on_type);
    }

    #[test]
    fn route_client_events_for_view_pastes_navigator_search_only_into_invoking_client_view() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.navigator.query = "shared".into();
        app.state.navigator.search_focused = true;
        app.state.navigator.state_filter = Some(state::NavigatorStateFilter::Working);

        let mut first_client = ClientViewState::from_app_state(&app.state);
        first_client.mode = Mode::Navigator;
        first_client.navigator.query = "fea".into();
        first_client.navigator.search_focused = true;
        first_client.navigator.state_filter = Some(state::NavigatorStateFilter::Blocked);
        let mut second_client = ClientViewState::from_app_state(&app.state);
        second_client.mode = Mode::Navigator;
        second_client.navigator.query = "other".into();
        second_client.navigator.search_focused = true;
        second_client.navigator.state_filter = Some(state::NavigatorStateFilter::Idle);

        app.route_client_events_for_view(
            &mut first_client,
            vec![crate::raw_input::RawInputEvent::Paste("ture".into())],
            true,
        );

        assert_eq!(first_client.navigator.query, "feature");
        assert_eq!(first_client.navigator.state_filter, None);
        assert_eq!(second_client.navigator.query, "other");
        assert_eq!(
            second_client.navigator.state_filter,
            Some(state::NavigatorStateFilter::Idle)
        );
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(app.state.navigator.query, "shared");
        assert_eq!(
            app.state.navigator.state_filter,
            Some(state::NavigatorStateFilter::Working)
        );
    }

    #[tokio::test]
    async fn route_client_events_for_view_mouse_uses_invoking_client_workspace() {
        let mut app = test_app();
        let first_workspace = Workspace::test_new("one");
        let second_workspace = Workspace::test_new("two");
        let first_pane = first_workspace.tabs[0].root_pane;
        let second_pane = second_workspace.tabs[0].root_pane;
        app.state.workspaces = vec![first_workspace, second_workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = false;
        crate::ui::compute_view(&mut app.state, ratatui::layout::Rect::new(0, 0, 106, 20));
        let info = app.state.view.pane_infos[0].clone();

        let (first_runtime, mut first_rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_screen_bytes(
                info.inner_rect.width.max(1),
                info.inner_rect.height.max(1),
                b"\x1b[?1002h\x1b[?1006h",
            );
        let (second_runtime, mut second_rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_screen_bytes(
                info.inner_rect.width.max(1),
                info.inner_rect.height.max(1),
                b"\x1b[?1002h\x1b[?1006h",
            );
        app.state.workspaces[0].insert_test_runtime(first_pane, first_runtime);
        app.state.workspaces[1].insert_test_runtime(second_pane, second_runtime);

        let first_client = ClientViewState::from_app_state(&app.state);
        let mut second_client = ClientViewState::from_app_state(&app.state);
        second_client.active_workspace = Some(1);
        second_client.selected_workspace = 1;
        second_client.reconcile(&app.state);
        second_client.computed.pane_infos[0].id = second_pane;

        app.route_client_events_for_view(
            &mut second_client,
            vec![crate::raw_input::RawInputEvent::Mouse(
                crossterm::event::MouseEvent {
                    kind: crossterm::event::MouseEventKind::Down(
                        crossterm::event::MouseButton::Left,
                    ),
                    column: info.inner_rect.x + 2,
                    row: info.inner_rect.y + 3,
                    modifiers: KeyModifiers::empty(),
                },
            )],
            true,
        );

        assert!(first_rx.try_recv().is_err());
        assert_eq!(
            second_rx
                .try_recv()
                .expect("mouse forwarded to second client pane"),
            bytes::Bytes::from_static(b"\x1b[<0;3;4M")
        );
        assert!(second_rx.try_recv().is_err());
        assert_eq!(app.state.active, Some(0));
        assert_eq!(first_client.active_workspace, Some(0));
        assert_eq!(second_client.active_workspace, Some(1));
    }

    #[test]
    fn route_client_events_for_view_sidebar_click_is_client_local() {
        let mut app = test_app();
        app.state.workspaces = vec![
            Workspace::test_new("one"),
            Workspace::test_new("two"),
            Workspace::test_new("three"),
        ];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        crate::ui::compute_view(&mut app.state, ratatui::layout::Rect::new(0, 0, 120, 30));

        let mut first_client = ClientViewState::from_app_state(&app.state);
        let second_client = ClientViewState::from_app_state(&app.state);
        let card = first_client
            .computed
            .workspace_card_areas
            .iter()
            .find(|card| card.ws_idx == 1)
            .expect("second workspace card")
            .rect;

        app.route_client_events_for_view(
            &mut first_client,
            vec![
                crate::raw_input::RawInputEvent::Mouse(crossterm::event::MouseEvent {
                    kind: crossterm::event::MouseEventKind::Down(
                        crossterm::event::MouseButton::Left,
                    ),
                    column: card.x + 1,
                    row: card.y,
                    modifiers: KeyModifiers::empty(),
                }),
                crate::raw_input::RawInputEvent::Mouse(crossterm::event::MouseEvent {
                    kind: crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                    column: card.x + 1,
                    row: card.y,
                    modifiers: KeyModifiers::empty(),
                }),
            ],
            true,
        );

        assert_eq!(first_client.active_workspace, Some(1));
        assert_eq!(first_client.selected_workspace, 1);
        assert_eq!(second_client.active_workspace, Some(0));
        assert_eq!(second_client.selected_workspace, 0);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
    }

    #[test]
    fn route_client_events_for_view_tab_click_targets_invoking_client_workspace() {
        let mut app = test_app();
        let mut first_workspace = Workspace::test_new("one");
        first_workspace.test_add_tab(Some("one-logs"));
        let mut second_workspace = Workspace::test_new("two");
        second_workspace.test_add_tab(Some("two-logs"));
        app.state.workspaces = vec![first_workspace, second_workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        crate::ui::compute_view(&mut app.state, ratatui::layout::Rect::new(0, 0, 120, 30));

        let first_client = ClientViewState::from_app_state(&app.state);
        let mut second_client = ClientViewState::from_app_state(&app.state);
        second_client.active_workspace = Some(1);
        second_client.selected_workspace = 1;
        second_client.reconcile(&app.state);
        let mut second_local_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(
            &mut second_local_state,
            &second_client,
        );
        crate::ui::compute_view(
            &mut second_local_state,
            ratatui::layout::Rect::new(0, 0, 120, 30),
        );
        second_client.computed = second_local_state.view.clone();

        let tab = second_client.computed.tab_hit_areas[1];
        app.route_client_events_for_view(
            &mut second_client,
            vec![
                crate::raw_input::RawInputEvent::Mouse(crossterm::event::MouseEvent {
                    kind: crossterm::event::MouseEventKind::Down(
                        crossterm::event::MouseButton::Left,
                    ),
                    column: tab.x + 1,
                    row: tab.y,
                    modifiers: KeyModifiers::empty(),
                }),
                crate::raw_input::RawInputEvent::Mouse(crossterm::event::MouseEvent {
                    kind: crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                    column: tab.x + 1,
                    row: tab.y,
                    modifiers: KeyModifiers::empty(),
                }),
            ],
            true,
        );

        let second_workspace_id = app.state.workspaces[1].id.clone();
        assert_eq!(
            second_client.active_tab_for_workspace(&second_workspace_id),
            Some(1)
        );
        assert_eq!(first_client.active_workspace, Some(0));
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.workspaces[0].active_tab_index(), 0);
        assert_eq!(app.state.workspaces[1].active_tab_index(), 0);
    }

    #[test]
    fn route_client_events_for_view_tab_close_click_closes_shared_tab() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("shell");
        workspace.test_add_tab(Some("logs"));
        workspace.test_add_tab(Some("db"));
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.hovered_tab = Some(1);
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let close = client.computed.tab_close_hit_areas[1];
        assert!(close.width > 0, "second tab close affordance is visible");

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                close.x,
                close.y,
            )],
            true,
        );

        let tab_names = app.state.workspaces[0]
            .tabs
            .iter()
            .map(|tab| tab.display_name().to_string())
            .collect::<Vec<_>>();
        assert_eq!(tab_names, vec!["1", "db"]);
    }

    #[tokio::test]
    async fn route_client_events_for_view_new_tab_button_click_creates_shared_tab() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        app.state.prompt_new_tab_name = false;

        let mut client = ClientViewState::from_app_state(&app.state);
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let plus = client.computed.new_tab_hit_area;
        assert!(plus.width > 0, "new-tab plus hit area is visible");

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                plus.x,
                plus.y,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::ContextMenu);
        let menu = context_menu_rect_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 2,
            )],
            true,
        );

        assert!(app.process_deferred_workspace_requests());
        assert_eq!(app.state.workspaces[0].tabs.len(), 2);
        assert_eq!(app.state.workspaces[0].active_tab_index(), 1);
        client.reconcile(&app.state);
        assert_eq!(
            client.active_tabs.get(&app.state.workspaces[0].id),
            Some(&1),
            "invoking client follows the tab created by its plus-menu click"
        );
    }

    #[test]
    fn route_client_events_for_view_new_agent_button_click_opens_keyboard_picker() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        install_two_agent_profiles(&mut app.state);

        let mut client = ClientViewState::from_app_state(&app.state);
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let plus = client.computed.new_tab_hit_area;
        assert!(plus.width > 0, "new-tab plus hit area is visible");

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                plus.x,
                plus.y,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::ContextMenu);
        let menu = context_menu_rect_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 3,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::AgentProfilePicker);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Down,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::AgentProfilePicker);
        assert_eq!(client.agent_profile_picker.selected, 1);
    }

    #[tokio::test]
    async fn route_client_events_for_view_context_new_tab_creates_shared_tab() {
        for (case, kind) in [
            (
                "new tab button",
                state::ContextMenuKind::NewTabButton {
                    ws_idx: 0,
                    can_diff: false,
                },
            ),
            (
                "workspace",
                state::ContextMenuKind::Workspace {
                    ws_idx: 0,
                    can_diff: false,
                },
            ),
        ] {
            let mut app = test_app();
            app.state.workspaces = vec![Workspace::test_new("shell")];
            app.state.ensure_test_terminals();
            app.state.active = Some(0);
            app.state.selected = 0;
            app.state.mode = Mode::Terminal;
            app.state.mouse_capture = true;
            app.state.prompt_new_tab_name = false;

            let mut client = ClientViewState::from_app_state(&app.state);
            client.context_menu = Some(state::ContextMenuState {
                kind,
                x: 4,
                y: 4,
                list: state::MenuListState::new(1),
            });
            client.mode = Mode::ContextMenu;
            compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
            let menu = context_menu_rect_for_client_view(&app, &client);

            app.route_client_events_for_view(
                &mut client,
                vec![raw_mouse(
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                    menu.x + 2,
                    menu.y + 2,
                )],
                true,
            );

            assert!(
                app.state.request_new_tab,
                "{case} queued a shared tab request"
            );
            assert_eq!(
                app.state.workspaces[0].tabs.len(),
                1,
                "{case} defers creation to the app cycle"
            );

            assert!(
                app.process_deferred_workspace_requests(),
                "{case} app cycle processes queued tab request"
            );
            assert!(
                !app.state.request_new_tab,
                "{case} consumes the tab request"
            );
            assert_eq!(
                app.state.workspaces[0].tabs.len(),
                2,
                "{case} creates a tab through the normal app cycle"
            );
            assert_eq!(
                app.state.workspaces[0].active_tab_index(),
                1,
                "{case} focuses the created shared tab"
            );
        }
    }

    #[test]
    fn route_client_events_for_view_command_palette_mouse_move_selects_command() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::CommandPalette;
        app.state.mouse_capture = true;

        let mut client = ClientViewState::from_app_state(&app.state);
        let area = ratatui::layout::Rect::new(0, 0, 120, 30);
        compute_client_view(&app, &mut client, area);
        let mut rendered_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut rendered_state, &client);
        crate::ui::compute_view(&mut rendered_state, area);
        let list = crate::ui::command_palette_list_geometry(
            rendered_state.screen_rect(),
            64,
            rendered_state.command_palette.scroll,
        )
        .expect("command palette list is visible");

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Moved,
                list.rect.x + 1,
                list.rect.y + 2,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::CommandPalette);
        assert_eq!(client.command_palette.selected, 1);
        assert_eq!(
            app.state.command_palette.selected, 0,
            "command palette hover remains local to the invoking client"
        );
    }

    #[test]
    fn route_client_events_for_view_command_palette_diff_queues_shared_git_diff() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;

        let mut client = ClientViewState::from_app_state(&app.state);
        input::open_command_palette_for_view(&mut client);
        client.command_palette.query = "open git diff".to_string();

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        let workspace_id = app.state.workspaces[0].id.clone();
        assert_eq!(client.mode, Mode::Terminal);
        assert!(app.state.request_open_git_diff_command);
        assert_eq!(client.active_tabs.get(&workspace_id), Some(&0));
        assert_eq!(client.pending_active_tabs.get(&workspace_id), Some(&1));
    }

    #[test]
    fn route_client_events_for_view_git_picker_mouse_move_selects_repo() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::GitRepoPicker;
        app.state.mouse_capture = true;
        app.state.git_repo_picker = state::GitRepoPickerState {
            ws_idx: 0,
            roots: vec!["/tmp/one".into(), "/tmp/two".into()],
            selected: 0,
            scroll: 0,
        };

        let mut client = ClientViewState::from_app_state(&app.state);
        let area = ratatui::layout::Rect::new(0, 0, 120, 30);
        compute_client_view(&app, &mut client, area);
        let mut rendered_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut rendered_state, &client);
        crate::ui::compute_view(&mut rendered_state, area);
        let list = crate::ui::git_repo_picker::git_repo_picker_list_geometry(&rendered_state)
            .expect("git picker list is visible");

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Moved,
                list.rect.x + 1,
                list.rect.y + 2,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::GitRepoPicker);
        assert_eq!(client.git_repo_picker.selected, 1);
        assert_eq!(
            app.state.git_repo_picker.selected, 0,
            "git picker hover remains local to the invoking client"
        );
    }

    #[test]
    fn route_client_events_for_view_context_new_agent_picker_handles_client_keys() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        install_two_agent_profiles(&mut app.state);

        let mut client = ClientViewState::from_app_state(&app.state);
        client.context_menu = Some(state::ContextMenuState {
            kind: state::ContextMenuKind::NewTabButton {
                ws_idx: 0,
                can_diff: false,
            },
            x: 4,
            y: 4,
            list: state::MenuListState::new(2),
        });
        client.mode = Mode::ContextMenu;
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let menu = context_menu_rect_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 3,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::AgentProfilePicker);
        assert_eq!(client.agent_profile_picker.ws_idx, 0);
        assert_eq!(client.agent_profile_picker.selected, 0);
        assert_eq!(app.state.mode, Mode::Terminal);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Down,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::AgentProfilePicker);
        assert_eq!(client.agent_profile_picker.selected, 1);
        assert_eq!(app.state.mode, Mode::Terminal);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Esc,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn route_client_events_for_view_agent_picker_mouse_move_selects_profile() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        install_two_agent_profiles(&mut app.state);
        input::agent_profile_picker::open_new_agent_picker_for_workspace(&mut app.state, 0);

        let mut client = ClientViewState::from_app_state(&app.state);
        let area = ratatui::layout::Rect::new(0, 0, 120, 30);
        compute_client_view(&app, &mut client, area);

        let mut rendered_state = app.state.clone();
        crate::app::view_state::apply_client_view_to_app_state(&mut rendered_state, &client);
        crate::ui::compute_view(&mut rendered_state, area);
        let row_count = crate::app::agent_profile_picker::agent_profile_picker_filtered_entries(
            &rendered_state,
        )
        .len();
        assert!(row_count >= 2, "test profile list has a second row");
        let list = crate::ui::agent_profile_picker_list_geometry(
            rendered_state.screen_rect(),
            row_count,
            rendered_state.agent_profile_picker.scroll,
        )
        .expect("agent picker list is visible");

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Moved,
                list.rect.x + 1,
                list.rect.y + 2,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::AgentProfilePicker);
        assert_eq!(client.agent_profile_picker.selected, 1);
        assert_eq!(
            app.state.agent_profile_picker.selected, 0,
            "picker hover remains local to the invoking client"
        );
    }

    #[test]
    fn route_client_events_for_view_agent_picker_enter_queues_shared_agent_tab() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        install_two_agent_profiles(&mut app.state);

        let mut client = ClientViewState::from_app_state(&app.state);
        input::agent_profile_picker::open_new_agent_picker_for_workspace(&mut app.state, 0);
        client.mode = app.state.mode;
        client.agent_profile_picker = app.state.agent_profile_picker.clone();
        let selected_profile_id =
            crate::app::agent_profile_picker::agent_profile_picker_filtered_entries(&app.state)
                .get(client.agent_profile_picker.selected)
                .expect("selected agent profile")
                .profile_id
                .clone();
        app.state.mode = Mode::Terminal;

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(
            app.state.request_agent_profile_tab,
            Some((0, selected_profile_id))
        );
        let workspace_id = app.state.workspaces[0].id.clone();
        assert_eq!(client.active_tabs.get(&workspace_id), Some(&0));
        assert_eq!(
            client.pending_active_tabs.get(&workspace_id),
            Some(&1),
            "invoking client keeps a pending focus target until the server creates the agent tab"
        );
    }

    #[test]
    fn route_client_events_for_view_prompted_new_tab_enter_queues_shared_tab() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        app.state.prompt_new_tab_name = true;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.context_menu = Some(state::ContextMenuState {
            kind: state::ContextMenuKind::NewTabButton {
                ws_idx: 0,
                can_diff: false,
            },
            x: 4,
            y: 4,
            list: state::MenuListState::new(1),
        });
        client.mode = Mode::ContextMenu;
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let menu = context_menu_rect_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 2,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::RenameTab);
        assert!(client.creating_new_tab);
        assert!(!app.state.request_new_tab);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_key(
                KeyCode::Enter,
                KeyModifiers::empty(),
                KeyEventKind::Press,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert!(app.state.request_new_tab);
    }

    #[test]
    fn route_client_events_for_view_context_diff_queues_shared_git_diff() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.context_menu = Some(state::ContextMenuState {
            kind: state::ContextMenuKind::NewTabButton {
                ws_idx: 0,
                can_diff: true,
            },
            x: 4,
            y: 4,
            list: state::MenuListState::new(3),
        });
        client.mode = Mode::ContextMenu;
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let menu = context_menu_rect_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 4,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert!(app.state.request_open_git_diff_command);
        let workspace_id = app.state.workspaces[0].id.clone();
        assert_eq!(client.active_tabs.get(&workspace_id), Some(&0));
        assert_eq!(
            client.pending_active_tabs.get(&workspace_id),
            Some(&1),
            "invoking client keeps a pending focus target until the server creates the diff tab"
        );

        client.reconcile(&app.state);
        assert_eq!(client.pending_active_tabs.get(&workspace_id), Some(&1));
        assert_eq!(client.active_tabs.get(&workspace_id), Some(&0));

        app.state.workspaces[0].test_add_tab(Some("diff"));
        app.state.workspaces[0].active_tab = 1;
        client.reconcile(&app.state);

        assert_eq!(
            client.active_tabs.get(&workspace_id),
            Some(&1),
            "invoking client follows the diff tab after an intervening render"
        );
        assert!(!client.pending_active_tabs.contains_key(&workspace_id));
    }

    #[test]
    fn route_client_events_for_view_group_context_space_queues_shared_workspace() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("shell")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.context_menu = Some(state::ContextMenuState {
            kind: state::ContextMenuKind::Group {
                group_idx: 0,
                can_delete: false,
            },
            x: 4,
            y: 4,
            list: state::MenuListState::new(1),
        });
        client.mode = Mode::ContextMenu;
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let menu = context_menu_rect_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 2,
            )],
            true,
        );

        assert_eq!(client.mode, Mode::Terminal);
        assert!(app.state.request_new_workspace);
        assert_eq!(app.state.active_group, 0);
    }

    #[test]
    fn route_client_events_for_view_pane_context_close_removes_shared_pane() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("work");
        let root_pane = workspace.tabs[0].root_pane;
        let closed_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        workspace.tabs[0].layout.focus_pane(closed_pane);
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        crate::ui::compute_view(&mut app.state, ratatui::layout::Rect::new(0, 0, 120, 30));

        let mut client = ClientViewState::from_app_state(&app.state);
        client.focus_pane_in_workspace(&app.state, 0, 0, closed_pane);
        client.context_menu = Some(state::ContextMenuState {
            kind: state::ContextMenuKind::Pane {
                ws_idx: 0,
                pane_id: closed_pane,
                has_manual_label: false,
            },
            x: 4,
            y: 4,
            list: state::MenuListState::new(5),
        });
        client.mode = Mode::ContextMenu;
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let menu = context_menu_rect_for_client_view(&app, &client);
        let close_pane_row = 4;

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                menu.x + 2,
                menu.y + 1 + close_pane_row,
            )],
            true,
        );

        let panes = &app.state.workspaces[0].tabs[0].panes;
        assert_eq!(panes.len(), 1);
        assert!(panes.contains_key(&root_pane));
        assert!(!panes.contains_key(&closed_pane));
    }

    #[test]
    fn route_client_events_for_view_confirm_delete_group_click_deletes_group() {
        let mut app = test_app();
        let work_group = app.state.create_group("Work".to_string());
        app.state.workspaces = vec![Workspace::test_new("keep"), Workspace::test_new("drop")];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();
        app.state.active = Some(1);
        app.state.selected = 1;
        app.state.active_group = work_group;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        crate::ui::compute_view(&mut app.state, ratatui::layout::Rect::new(0, 0, 120, 30));

        let mut client = ClientViewState::from_app_state(&app.state);
        client.mode = Mode::ConfirmDeleteGroup;
        client.confirm_delete_group = Some(work_group);
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let confirm = confirm_button_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                confirm.x + 1,
                confirm.y,
            )],
            true,
        );

        assert_eq!(
            app.state
                .groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            vec!["group 1"]
        );
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].display_name(), "keep");
        assert_eq!(app.state.active_group, 0);
    }

    #[test]
    fn route_client_events_for_view_confirm_close_last_space_click_deletes_space() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("only")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        crate::ui::compute_view(&mut app.state, ratatui::layout::Rect::new(0, 0, 120, 30));

        let mut client = ClientViewState::from_app_state(&app.state);
        client.mode = Mode::ConfirmClose;
        client.selected_workspace = 0;
        compute_client_view(&app, &mut client, ratatui::layout::Rect::new(0, 0, 120, 30));
        let confirm = confirm_button_for_client_view(&app, &client);

        app.route_client_events_for_view(
            &mut client,
            vec![raw_mouse(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                confirm.x + 1,
                confirm.y,
            )],
            true,
        );

        assert!(app.state.workspaces.is_empty());
        assert_eq!(app.state.active, None);
        assert_eq!(app.state.selected, 0);
        assert_eq!(client.mode, Mode::Navigate);
    }

    #[tokio::test]
    async fn api_request_for_view_uses_invoking_client_view_for_no_target_pane_split() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        let shared_focus_before = app.state.workspaces[1].tabs[0].layout.focused();

        let mut client = ClientViewState::from_app_state(&app.state);
        client.active_workspace = Some(1);
        client.selected_workspace = 1;
        client.reconcile(&app.state);
        let workspace_id = app.state.workspaces[1].id.clone();

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "split".into(),
                method: crate::api::schema::Method::PaneSplit(
                    crate::api::schema::PaneSplitParams {
                        workspace_id: None,
                        target_pane_id: None,
                        direction: crate::api::schema::SplitDirection::Right,
                        ratio: None,
                        cwd: None,
                        focus: true,
                        env: std::collections::HashMap::new(),
                    },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "pane_info");
        assert_eq!(body["result"]["pane"]["focused"], true);
        let response_pane_id = body["result"]["pane"]["pane_id"].as_str().unwrap();
        let (_, new_pane_id) = app.parse_pane_id(response_pane_id).unwrap();

        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(app.state.workspaces[0].tabs[0].panes.len(), 1);
        assert_eq!(app.state.workspaces[1].tabs[0].panes.len(), 2);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            shared_focus_before
        );
        assert_eq!(client.active_workspace, Some(1));
        assert_eq!(client.active_tab_for_workspace(&workspace_id), Some(0));
        assert_eq!(
            client.focused_pane_for_tab(&workspace_id, 1),
            Some(new_pane_id)
        );
    }

    #[tokio::test]
    async fn api_request_for_view_pane_split_without_focus_preserves_client_focus() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        let workspace_id = app.state.workspaces[0].id.clone();
        let original_focus = app.state.workspaces[0].tabs[0].root_pane;

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "split-no-focus".into(),
                method: crate::api::schema::Method::PaneSplit(
                    crate::api::schema::PaneSplitParams {
                        workspace_id: None,
                        target_pane_id: None,
                        direction: crate::api::schema::SplitDirection::Right,
                        ratio: None,
                        cwd: None,
                        focus: false,
                        env: std::collections::HashMap::new(),
                    },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "pane_info");
        assert_eq!(body["result"]["pane"]["focused"], false);
        assert_eq!(app.state.workspaces[0].tabs[0].panes.len(), 2);
        assert_eq!(
            app.state.workspaces[0].tabs[0].layout.focused(),
            original_focus
        );
        assert_eq!(
            client.focused_pane_for_tab(&workspace_id, 1),
            Some(original_focus)
        );
    }

    #[tokio::test]
    async fn api_request_for_view_pane_split_with_workspace_id_uses_that_workspace_view_focus() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.active_workspace = Some(0);
        client.selected_workspace = 0;
        client.reconcile(&app.state);
        let workspace_id = app.state.workspaces[1].id.clone();
        let workspace_public_id = app.public_workspace_id(1);
        let shared_focus_before = app.state.workspaces[1].tabs[0].layout.focused();

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "split-workspace-id".into(),
                method: crate::api::schema::Method::PaneSplit(
                    crate::api::schema::PaneSplitParams {
                        workspace_id: Some(workspace_public_id.clone()),
                        target_pane_id: None,
                        direction: crate::api::schema::SplitDirection::Right,
                        ratio: None,
                        cwd: None,
                        focus: true,
                        env: std::collections::HashMap::new(),
                    },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "pane_info");
        assert_eq!(body["result"]["pane"]["focused"], true);
        assert_eq!(body["result"]["pane"]["workspace_id"], workspace_public_id);
        let response_pane_id = body["result"]["pane"]["pane_id"].as_str().unwrap();
        let (_, new_pane_id) = app.parse_pane_id(response_pane_id).unwrap();

        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert_eq!(app.state.workspaces[0].tabs[0].panes.len(), 1);
        assert_eq!(app.state.workspaces[1].tabs[0].panes.len(), 2);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            shared_focus_before
        );
        assert_eq!(client.active_workspace, Some(1));
        assert_eq!(
            client.focused_pane_for_tab(&workspace_id, 1),
            Some(new_pane_id)
        );
    }

    #[tokio::test]
    async fn api_request_for_view_pane_zoom_updates_invoking_client_only() {
        let mut app = test_app();
        let mut active = Workspace::test_new("one");
        active.test_split(ratatui::layout::Direction::Horizontal);
        let mut background = Workspace::test_new("two");
        let background_root = background.tabs[0].root_pane;
        let background_second = background.test_split(ratatui::layout::Direction::Horizontal);
        background.tabs[0].layout.focus_pane(background_root);
        app.state.workspaces = vec![active, background];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.active_workspace = Some(1);
        client.selected_workspace = 1;
        client.reconcile(&app.state);
        let workspace_id = app.state.workspaces[1].id.clone();
        client.focus_pane_in_workspace(&app.state, 1, 0, background_second);

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "zoom-client-view".into(),
                method: crate::api::schema::Method::PaneZoom(crate::api::schema::PaneZoomParams {
                    pane_id: None,
                    mode: crate::api::schema::PaneZoomMode::On,
                }),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "pane_zoom");
        assert_eq!(body["result"]["zoom"]["zoomed"], true);
        assert_eq!(body["result"]["zoom"]["focus_changed"], false);
        assert_eq!(
            body["result"]["zoom"]["focused_pane_id"],
            app.public_pane_id(1, background_second).unwrap()
        );
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert!(!app.state.workspaces[1].tabs[0].zoomed);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            background_root
        );
        assert!(client.tab_is_zoomed(&workspace_id, 1));
        assert_eq!(
            client.focused_pane_for_tab(&workspace_id, 1),
            Some(background_second)
        );
    }

    #[tokio::test]
    async fn api_request_for_view_pane_resize_uses_invoking_client_focus() {
        let mut app = test_app();
        let active = Workspace::test_new("one");
        let mut background = Workspace::test_new("two");
        let background_root = background.tabs[0].root_pane;
        let background_second = background.test_split(ratatui::layout::Direction::Horizontal);
        background.tabs[0].layout.focus_pane(background_root);
        app.state.workspaces = vec![active, background];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.active_workspace = Some(1);
        client.selected_workspace = 1;
        client.reconcile(&app.state);
        let workspace_id = app.state.workspaces[1].id.clone();
        client.focus_pane_in_workspace(&app.state, 1, 0, background_second);

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "resize-client-view".into(),
                method: crate::api::schema::Method::PaneResize(
                    crate::api::schema::PaneResizeParams {
                        pane_id: None,
                        direction: crate::api::schema::PaneDirection::Left,
                        amount: Some(0.10),
                    },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "pane_resize");
        assert_eq!(body["result"]["resize"]["changed"], true);
        assert_eq!(
            body["result"]["resize"]["focused_pane_id"],
            app.public_pane_id(1, background_second).unwrap()
        );
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            background_root
        );
        assert_eq!(
            client.focused_pane_for_tab(&workspace_id, 1),
            Some(background_second)
        );
    }

    #[tokio::test]
    async fn api_request_for_view_workspace_focus_updates_client_view_only() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        let workspace_id = app.public_workspace_id(1);

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "workspace-focus".into(),
                method: crate::api::schema::Method::WorkspaceFocus(
                    crate::api::schema::WorkspaceTarget { workspace_id },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "workspace_info");
        assert_eq!(body["result"]["workspace"]["focused"], true);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(client.active_workspace, Some(1));
        assert_eq!(client.selected_workspace, 1);
        assert_eq!(client.mode, Mode::Terminal);
    }

    #[tokio::test]
    async fn api_request_for_view_workspace_create_focuses_client_not_ambient_state() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "workspace-create".into(),
                method: crate::api::schema::Method::WorkspaceCreate(
                    crate::api::schema::WorkspaceCreateParams {
                        cwd: None,
                        label: Some("client workspace".into()),
                        focus: true,
                        env: std::collections::HashMap::new(),
                    },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "workspace_created");
        assert_eq!(body["result"]["workspace"]["focused"], true);
        assert_eq!(app.state.workspaces.len(), 2);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(client.active_workspace, Some(1));
        assert_eq!(client.selected_workspace, 1);
    }

    #[tokio::test]
    async fn api_request_for_view_workspace_close_preserves_ambient_focus() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.active_workspace = Some(1);
        client.selected_workspace = 1;
        client.reconcile(&app.state);
        let workspace_id = app.public_workspace_id(1);

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "workspace-close".into(),
                method: crate::api::schema::Method::WorkspaceClose(
                    crate::api::schema::WorkspaceTarget { workspace_id },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "ok");
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(client.active_workspace, Some(0));
        assert_eq!(client.selected_workspace, 0);
    }

    #[tokio::test]
    async fn api_request_for_view_tab_focus_updates_only_invoking_client_view() {
        let mut app = test_app();
        let mut workspace = Workspace::test_new("test");
        workspace.test_add_tab(Some("logs"));
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        let workspace_id = app.state.workspaces[0].id.clone();
        let tab_id = app.public_tab_id(0, 1).expect("tab id");

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "tab-focus".into(),
                method: crate::api::schema::Method::TabFocus(crate::api::schema::TabTarget {
                    tab_id,
                }),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "tab_info");
        assert_eq!(body["result"]["tab"]["focused"], true);
        assert_eq!(body["result"]["tab"]["number"], 2);
        assert_eq!(client.active_tab_for_workspace(&workspace_id), Some(1));
        assert_eq!(client.mode, Mode::Terminal);
        assert_eq!(app.state.workspaces[0].active_tab_index(), 0);
        assert_eq!(app.state.active, Some(0));
    }

    #[tokio::test]
    async fn api_request_for_view_tab_create_defaults_to_invoking_client_workspace() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let mut client = ClientViewState::from_app_state(&app.state);
        client.active_workspace = Some(1);
        client.selected_workspace = 1;
        client.reconcile(&app.state);
        let workspace_id = app.state.workspaces[1].id.clone();

        let response = app.handle_api_request_for_view(
            &mut client,
            crate::api::schema::Request {
                id: "tab-create".into(),
                method: crate::api::schema::Method::TabCreate(
                    crate::api::schema::TabCreateParams {
                        workspace_id: None,
                        cwd: None,
                        focus: true,
                        label: Some("client tab".into()),
                        env: std::collections::HashMap::new(),
                    },
                ),
            },
        );

        let body: serde_json::Value = serde_json::from_str(&response).expect("response json");
        assert_eq!(body["result"]["type"], "tab_created");
        assert_eq!(
            body["result"]["tab"]["workspace_id"],
            app.public_workspace_id(1)
        );
        assert_eq!(body["result"]["tab"]["focused"], true);
        assert_eq!(body["result"]["tab"]["label"], "client tab");
        assert_eq!(app.state.workspaces[0].tabs.len(), 1);
        assert_eq!(app.state.workspaces[1].tabs.len(), 2);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.workspaces[1].active_tab_index(), 0);
        assert_eq!(client.active_workspace, Some(1));
        assert_eq!(client.active_tab_for_workspace(&workspace_id), Some(1));
    }
    #[test]
    fn route_client_input_closes_release_notes_modal() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::ReleaseNotes;
        app.state.release_notes = Some(release_notes_state());

        app.route_client_input(b"\x1b".to_vec());

        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(app.state.release_notes.is_none());
    }

    #[test]
    fn route_client_input_closes_settings_modal() {
        let mut app = test_app();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Settings;
        app.state.settings.original_theme = Some(app.state.theme_name.clone());
        app.state.settings.original_palette = Some(app.state.palette.clone());

        app.route_client_input(b"\x1b".to_vec());

        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn route_client_input_updates_host_terminal_theme_from_osc_response() {
        let mut app = test_app();
        app.state.global_light_theme_name = "gruvbox-light".to_string();
        app.state.global_dark_theme_name = "gruvbox".to_string();
        app.state.global_theme_mode = crate::config::ThemeMode::System;

        app.route_client_input(b"\x1b]11;#f5f5f5\x07".to_vec());

        assert_eq!(
            app.state.host_terminal_theme.background,
            Some(crate::terminal_theme::RgbColor {
                r: 0xf5,
                g: 0xf5,
                b: 0xf5,
            })
        );
        assert_eq!(
            app.state.palette.panel_bg,
            state::Palette::gruvbox_light().panel_bg
        );
    }

    #[test]
    fn route_client_input_requeries_host_terminal_theme_on_focus_gained() {
        let mut app = test_app();

        app.route_client_input(b"\x1b[I".to_vec());

        assert_eq!(app.host_terminal_theme_query_count.get(), 1);
    }

    #[test]
    fn route_client_input_updates_host_terminal_palette_from_osc_response() {
        let mut app = test_app();

        app.route_client_input(b"\x1b]4;2;#112233\x07\x1b]12;#445566\x07".to_vec());

        assert_eq!(
            app.state.host_terminal_theme.palette[2],
            Some(crate::terminal_theme::RgbColor {
                r: 0x11,
                g: 0x22,
                b: 0x33,
            })
        );
        assert_eq!(
            app.state.host_terminal_theme.cursor,
            Some(crate::terminal_theme::RgbColor {
                r: 0x44,
                g: 0x55,
                b: 0x66,
            })
        );
    }

    #[test]
    fn parse_raw_input_bytes_with_ranges_tracks_offsets() {
        // Verify that the range-aware parser correctly tracks byte offsets
        // for events within a multi-event input buffer.
        let input = b"\x1b[Aa".to_vec(); // Up arrow + 'a'
        let events = crate::raw_input::parse_raw_input_bytes_with_ranges(&input);

        assert_eq!(events.len(), 2, "should parse Up arrow and 'a'");
        // Up arrow: \x1b[A = 3 bytes starting at offset 0
        assert_eq!(events[0].start, 0);
        assert_eq!(events[0].len, 3);
        // 'a': 1 byte starting at offset 3
        assert_eq!(events[1].start, 3);
        assert_eq!(events[1].len, 1);

        // Verify the raw bytes for each event are correct.
        assert_eq!(
            &input[events[0].start..events[0].start + events[0].len],
            b"\x1b[A"
        );
        assert_eq!(
            &input[events[1].start..events[1].start + events[1].len],
            b"a"
        );
    }
}
