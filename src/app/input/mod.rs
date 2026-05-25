//! Input handling — translates crossterm key/mouse events into state mutations.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::input::TerminalKey;
use ratatui::layout::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScrollbarClickTarget {
    Thumb { grab_row_offset: u16 },
    Track { offset_from_bottom: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
enum WheelRouting {
    HostScroll,
    MouseReport,
    AlternateScroll,
}

const WORKSPACE_DRAG_THRESHOLD: u16 = 1;
const TAB_DRAG_THRESHOLD: u16 = 1;
const MODAL_WHEEL_SCROLL_ROWS: i16 = 3;
const MODAL_PAGE_SCROLL_ROWS: i16 = 8;

mod command_palette;
mod modal;
mod mouse;
mod navigate;
mod overlays;
mod selection;
mod settings;
mod sidebar;
mod terminal;

pub(crate) use self::{
    modal::{
        handle_agent_menu_key, handle_confirm_close_key, handle_confirm_delete_group_key,
        handle_context_menu_key, handle_global_menu_key, handle_group_menu_key,
        handle_keybind_help_key, handle_rename_key, handle_resize_key,
    },
    navigate::terminal_direct_navigation_action,
    settings::open_settings_at,
};
use self::{
    modal::{
        modal_action_from_key, ModalAction, ONBOARDING_WELCOME_ACTIONS, RELEASE_NOTES_ACTIONS,
    },
    settings::SettingsAction,
};
use super::state::{AppState, DragState, DragTarget, Mode};
use super::App;

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

impl App {
    pub(super) async fn handle_key(&mut self, key: TerminalKey) {
        let previous_agent_panel_scope = self.state.agent_panel_scope;
        match self.state.mode {
            Mode::Terminal => self.handle_terminal_key(key).await,
            Mode::Prefix => self.handle_prefix_key(key),
            Mode::Navigate => self.handle_navigate_key(key),
            _ => {
                let key_event = key.as_key_event();
                match self.state.mode {
                    Mode::Onboarding => self.handle_onboarding_key(key_event),
                    Mode::ReleaseNotes => self.handle_release_notes_key(key_event),
                    Mode::ProductAnnouncement => self.handle_product_announcement_key(key_event),
                    Mode::Prefix | Mode::Navigate => unreachable!(),
                    Mode::RenameWorkspace
                    | Mode::RenameGroup
                    | Mode::RenameTab
                    | Mode::RenamePane => handle_rename_key(&mut self.state, key_event),
                    Mode::Resize => handle_resize_key(&mut self.state, key),
                    Mode::ConfirmClose => handle_confirm_close_key(&mut self.state, key_event),
                    Mode::ConfirmDeleteGroup => {
                        handle_confirm_delete_group_key(&mut self.state, key_event)
                    }
                    Mode::ContextMenu => {
                        handle_context_menu_key(
                            &mut self.state,
                            &mut self.terminal_runtimes,
                            key_event,
                        );
                    }
                    Mode::Settings => self.handle_settings_key(key_event),
                    Mode::GlobalMenu => handle_global_menu_key(&mut self.state, key_event),
                    Mode::GroupMenu => handle_group_menu_key(&mut self.state, key_event),
                    Mode::AgentMenu => handle_agent_menu_key(&mut self.state, key_event),
                    Mode::KeybindHelp => handle_keybind_help_key(&mut self.state, key_event),
                    Mode::CommandPalette => self.handle_command_palette_key(key_event),
                    Mode::Terminal => unreachable!(),
                }
            }
        }
        if self.state.agent_panel_scope != previous_agent_panel_scope {
            self.save_agent_panel_scope(self.state.agent_panel_scope);
        }
    }

    pub(super) async fn handle_paste(&mut self, text: String) {
        if self.state.mode != Mode::Terminal {
            return;
        }
        if let Some(ws_idx) = self.state.active {
            if let Some(rt) = self
                .state
                .focused_runtime_in_workspace(&self.terminal_runtimes, ws_idx)
            {
                let _ = rt.send_paste(text).await;
            }
        }
    }

    pub(crate) fn handle_onboarding_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Right | KeyCode::Char('l') => self.open_settings_from_onboarding(),
            _ => {
                if let Some(ModalAction::Continue) =
                    modal_action_from_key(&key, ONBOARDING_WELCOME_ACTIONS)
                {
                    self.open_settings_from_onboarding();
                }
            }
        }
    }

    pub(crate) fn handle_release_notes_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_release_notes(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_release_notes(1),
            KeyCode::PageUp => self.scroll_release_notes(-MODAL_PAGE_SCROLL_ROWS),
            KeyCode::PageDown => self.scroll_release_notes(MODAL_PAGE_SCROLL_ROWS),
            KeyCode::Home => {
                if let Some(notes) = &mut self.state.release_notes {
                    notes.scroll = 0;
                }
            }
            KeyCode::End => {
                let max_scroll = self.state.release_notes_max_scroll();
                if let Some(notes) = &mut self.state.release_notes {
                    notes.scroll = max_scroll;
                }
            }
            _ => {
                if let Some(ModalAction::Close) = modal_action_from_key(&key, RELEASE_NOTES_ACTIONS)
                {
                    self.dismiss_release_notes();
                }
            }
        }
    }

    pub(crate) fn handle_product_announcement_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_product_announcement(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_product_announcement(1),
            KeyCode::PageUp => self.scroll_product_announcement(-8),
            KeyCode::PageDown => self.scroll_product_announcement(8),
            KeyCode::Home => {
                if let Some(announcement) = &mut self.state.product_announcement {
                    announcement.scroll = 0;
                }
            }
            KeyCode::End => {
                let max_scroll = self.state.product_announcement_max_scroll();
                if let Some(announcement) = &mut self.state.product_announcement {
                    announcement.scroll = max_scroll;
                }
            }
            _ => {
                if let Some(ModalAction::Close) = modal_action_from_key(&key, RELEASE_NOTES_ACTIONS)
                {
                    self.dismiss_product_announcement();
                }
            }
        }
    }

    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.handle_overlay_mouse(mouse) {
            return;
        }

        if self.state.mode == Mode::CommandPalette {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    match command_palette::command_palette_action_button_at(
                        &self.state,
                        mouse.column,
                        mouse.row,
                    ) {
                        Some(ModalAction::Apply) => {
                            self.execute_selected_command_palette_command();
                            return;
                        }
                        Some(ModalAction::Close) => {
                            command_palette::close_command_palette(&mut self.state);
                            return;
                        }
                        _ => {}
                    }

                    if let Some(target) = command_palette::command_palette_scrollbar_target_at(
                        &self.state,
                        mouse.column,
                        mouse.row,
                    ) {
                        match target {
                            ScrollbarClickTarget::Thumb { grab_row_offset } => {
                                self.state.drag = Some(DragState {
                                    target: DragTarget::CommandPaletteScrollbar { grab_row_offset },
                                });
                            }
                            ScrollbarClickTarget::Track { offset_from_bottom } => {
                                command_palette::set_command_palette_offset_from_bottom(
                                    &mut self.state,
                                    offset_from_bottom,
                                );
                            }
                        }
                        return;
                    }

                    if command_palette::command_palette_contains_point(
                        &self.state,
                        mouse.column,
                        mouse.row,
                    ) {
                        command_palette::hover_command_palette_selection(
                            &mut self.state,
                            mouse.column,
                            mouse.row,
                        );
                    } else {
                        self.state.drag = None;
                        command_palette::close_command_palette(&mut self.state);
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some(DragState {
                        target: DragTarget::CommandPaletteScrollbar { grab_row_offset },
                    }) = &self.state.drag
                    {
                        if let Some(offset_from_bottom) =
                            command_palette::command_palette_offset_for_drag_row(
                                &self.state,
                                mouse.row,
                                *grab_row_offset,
                            )
                        {
                            command_palette::set_command_palette_offset_from_bottom(
                                &mut self.state,
                                offset_from_bottom,
                            );
                        }
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => {
                    self.state.drag = None;
                    return;
                }
                MouseEventKind::ScrollDown => {
                    command_palette::scroll_command_palette_rows(
                        &mut self.state,
                        MODAL_WHEEL_SCROLL_ROWS,
                    );
                    return;
                }
                MouseEventKind::ScrollUp => {
                    command_palette::scroll_command_palette_rows(
                        &mut self.state,
                        -MODAL_WHEEL_SCROLL_ROWS,
                    );
                    return;
                }
                MouseEventKind::Moved => {
                    command_palette::hover_command_palette_selection(
                        &mut self.state,
                        mouse.column,
                        mouse.row,
                    );
                    return;
                }
                _ => {}
            }
        }

        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
            && self.state.on_sidebar_divider(mouse.column, mouse.row)
        {
            let now = std::time::Instant::now();
            let is_double_click = self
                .last_sidebar_divider_click
                .is_some_and(|last| now.duration_since(last) <= super::SIDEBAR_DOUBLE_CLICK_WINDOW);
            self.last_sidebar_divider_click = Some(now);

            if is_double_click {
                self.state.sidebar_width = self.state.default_sidebar_width;
                self.state.sidebar_width_source =
                    crate::app::state::SidebarWidthSource::ConfigDefault;
                self.state.sidebar_width_auto = false;
                self.state.mark_session_dirty();
                self.state.drag = None;
                return;
            }
        }

        let previous_agent_panel_scope = self.state.agent_panel_scope;
        let previous_settings_section = self.state.settings.section;
        if let Some(action) = self.state.handle_mouse(&mut self.terminal_runtimes, mouse) {
            match action {
                SettingsAction::SaveSettings {
                    light,
                    dark,
                    mode,
                    sound_enabled,
                    toast_delivery,
                    agent_border_labels,
                } => {
                    self.save_theme(&light, &dark, mode);
                    self.save_sound(sound_enabled);
                    self.save_toast_delivery(toast_delivery);
                    self.save_agent_border_labels(agent_border_labels);
                }
                SettingsAction::SaveGroupTheme { group_idx, name } => {
                    self.state.set_group_theme(group_idx, name);
                }
                SettingsAction::InstallRecommendedIntegrations => {
                    self.install_recommended_integrations()
                }
                SettingsAction::InstallIntegration(target) => self.install_integration(target),
                SettingsAction::UninstallIntegration(target) => self.uninstall_integration(target),
            }
        }
        if previous_settings_section != crate::app::state::SettingsSection::Integrations
            && self.state.settings.section == crate::app::state::SettingsSection::Integrations
        {
            self.refresh_integration_recommendations();
        }
        if self.state.agent_panel_scope != previous_agent_panel_scope {
            self.save_agent_panel_scope(self.state.agent_panel_scope);
        }

        if let Some(content) = self.state.request_clipboard_write.take() {
            if self
                .event_tx
                .try_send(crate::events::AppEvent::ClipboardWrite { content })
                .is_err()
            {
                tracing::warn!("failed to queue clipboard write event");
            }
        }

        if let Some(action) = self.state.request_command_action.take() {
            match action {
                crate::app::state::CommandPanelAction::RunOrFocus(command_id) => {
                    if let Err(err) = self
                        .state
                        .run_project_command(&mut self.terminal_runtimes, &command_id)
                    {
                        self.state.toast = Some(crate::app::state::ToastNotification {
                            kind: crate::app::state::ToastKind::NeedsAttention,
                            title: "command failed".to_string(),
                            context: err,
                            target: None,
                        });
                    }
                }
                crate::app::state::CommandPanelAction::Stop(command_id) => {
                    self.state
                        .stop_project_command(&mut self.terminal_runtimes, &command_id);
                }
            }
        }

        // Sync autoscroll deadline with state (mouse handler may have
        // set or cleared selection_autoscroll during handle_mouse).
        if self.state.selection_autoscroll.is_none() {
            self.selection_autoscroll_deadline = None;
        } else if self.selection_autoscroll_deadline.is_none() {
            self.selection_autoscroll_deadline =
                Some(std::time::Instant::now() + super::SELECTION_AUTOSCROLL_INTERVAL);
        }
    }
}

// ---------------------------------------------------------------------------
// Mouse handling
// ---------------------------------------------------------------------------

// Note: split_pane needs runtime (event_tx for PTY spawn), so it lives on App
impl AppState {
    pub(crate) fn split_pane(
        &mut self,
        terminal_runtimes: &mut crate::terminal::TerminalRuntimeRegistry,
        direction: Direction,
    ) {
        // Actual PTY spawning happens in Workspace::split_focused
        // which needs events channel — this is called from navigate_key
        // where we don't have async context, so the workspace handles it
        let (rows, cols) = self.estimate_pane_size();
        let new_rows = (rows / 2).max(4);
        let new_cols = (cols / 2).max(10);

        let follow_cwd = self
            .active
            .and_then(|i| self.workspaces.get(i))
            .and_then(|ws| {
                let tab = ws.active_tab()?;
                tab.cwd_for_pane(tab.layout.focused(), &self.terminals, terminal_runtimes)
            });
        let cwd = Some(super::creation::resolve_new_terminal_cwd(
            &self.new_terminal_cwd,
            follow_cwd,
        ));

        if let Some(ws) = self.active.and_then(|i| self.workspaces.get_mut(i)) {
            if let Ok(new_pane) = ws.split_focused(
                direction,
                new_rows,
                new_cols,
                cwd,
                self.pane_scrollback_limit_bytes,
                self.host_terminal_theme,
                &self.default_shell,
            ) {
                let new_id = new_pane.pane_id;
                terminal_runtimes.insert(new_pane.terminal.id.clone(), new_pane.runtime);
                self.terminals
                    .insert(new_pane.terminal.id.clone(), new_pane.terminal);
                ws.layout.focus_pane(new_id);
                self.mark_session_dirty();
                self.mode = Mode::Terminal;
            }
        }
    }
}

#[cfg(test)]
fn state_with_workspaces(names: &[&str]) -> AppState {
    let mut state = AppState::test_new();
    state.workspaces = names
        .iter()
        .map(|name| crate::workspace::Workspace::test_new(name))
        .collect();
    if !state.workspaces.is_empty() {
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Navigate;
    }
    state
}

#[cfg(test)]
fn app_for_mouse_test() -> App {
    let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(
        &crate::config::Config::default(),
        true,
        None,
        api_rx,
        crate::api::EventHub::default(),
    );
    app.state.mode = Mode::Terminal;
    app.state.update_available = None;
    app.state.latest_release_notes_available = false;
    app.state.view.sidebar_rect = ratatui::layout::Rect::new(0, 0, 26, 20);
    app.state.view.terminal_area = ratatui::layout::Rect::new(26, 0, 80, 20);
    app
}

#[cfg(test)]
fn mouse(
    kind: crossterm::event::MouseEventKind,
    col: u16,
    row: u16,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column: col,
        row,
        modifiers: crossterm::event::KeyModifiers::empty(),
    }
}

#[cfg(test)]
fn numbered_lines_bytes(count: usize) -> Vec<u8> {
    (0..count)
        .map(|i| format!("{i:06}\r\n"))
        .collect::<String>()
        .into_bytes()
}

#[cfg(test)]
fn capture_snapshot(state: &AppState) -> crate::persist::SessionSnapshot {
    let terminal_runtimes = crate::terminal::TerminalRuntimeRegistry::new();
    crate::persist::capture(
        &state.groups,
        state.active_group,
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
    )
}

#[cfg(test)]
fn root_layout_ratio(snapshot: &crate::persist::SessionSnapshot) -> Option<f32> {
    match &snapshot.workspaces.first()?.tabs.first()?.layout {
        crate::persist::LayoutSnapshot::Split { ratio, .. } => Some(*ratio),
        crate::persist::LayoutSnapshot::Pane(_) => None,
    }
}

#[cfg(test)]
fn unique_temp_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("hako-{name}-{}-{nanos}", std::process::id()))
}

#[cfg(test)]
fn wait_for_file(path: &std::path::Path) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.is_empty() {
                return content;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("timed out waiting for {}", path.display());
}
