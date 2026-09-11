//! Input handling — translates crossterm key/mouse events into state mutations.

#[cfg(test)]
use crossterm::event::MouseEvent;

#[cfg(test)]
use crate::input::TerminalKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollbarClickTarget {
    Thumb { grab_row_offset: u16 },
    Track { offset_from_bottom: usize },
}

pub(super) const WORKSPACE_DRAG_THRESHOLD: u16 = 1;
pub(super) const TAB_DRAG_THRESHOLD: u16 = 1;
pub(super) const AGENT_DRAG_THRESHOLD: u16 = 1;
pub(super) const MODAL_WHEEL_SCROLL_ROWS: i16 = 3;
pub(super) const MODAL_PAGE_SCROLL_ROWS: i16 = 8;

pub(crate) fn rendering_client_may_open_url(url: &str, execution_host_is_local: bool) -> bool {
    if execution_host_is_local {
        return true;
    }
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("file:") {
        return false;
    }
    let Some((scheme, rest)) = lower.split_once("://") else {
        return true;
    };
    if !matches!(scheme, "http" | "https") {
        return true;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = if let Some(bracketed) = host_port.strip_prefix('[') {
        bracketed.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    !(host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host == "0.0.0.0"
        || host == "127.0.0.1"
        || host.starts_with("127."))
}

pub(crate) fn next_group_execution_host(
    profiles: &[crate::persist::ssh_profiles::SshConnectionProfile],
    current: &crate::execution_host::ExecutionHostId,
) -> crate::execution_host::ExecutionHostId {
    let mut hosts = vec![crate::execution_host::ExecutionHostId::local()];
    hosts.extend(profiles.iter().map(|profile| profile.execution_host_id()));
    let next = hosts
        .iter()
        .position(|host| host == current)
        .map_or(0, |index| (index + 1) % hosts.len());
    hosts
        .get(next)
        .cloned()
        .unwrap_or_else(crate::execution_host::ExecutionHostId::local)
}

pub(crate) fn apply_group_host_cycle(
    profiles: &[crate::persist::ssh_profiles::SshConnectionProfile],
    host: &mut crate::execution_host::ExecutionHostId,
    directory: &mut String,
) {
    *host = next_group_execution_host(profiles, host);
    if !directory.trim().is_empty() || host.is_local() {
        return;
    }
    *directory = profiles
        .iter()
        .find(|profile| profile.execution_host_id() == *host)
        .and_then(|profile| profile.suggested_directory())
        .map(|path| path.as_path().display().to_string())
        .unwrap_or_else(|| ".".to_string());
}

pub(crate) fn group_default_location_for(
    profiles: &[crate::persist::ssh_profiles::SshConnectionProfile],
    host: &crate::execution_host::ExecutionHostId,
    directory: &str,
) -> Option<crate::execution_host::ResourceLocation> {
    let trimmed = directory.trim();
    let path = if trimmed.is_empty() {
        if host.is_local() {
            return None;
        }
        profiles
            .iter()
            .find(|profile| &profile.execution_host_id() == host)
            .and_then(|profile| profile.suggested_directory().cloned())
            .or_else(|| crate::execution_host::HostPath::new(".").ok())?
    } else {
        crate::execution_host::HostPath::new(trimmed).ok()?
    };
    Some(crate::execution_host::ResourceLocation::new(
        host.clone(),
        path,
    ))
}

#[cfg(test)]
#[test]
fn remote_execution_urls_never_become_rendering_client_local_targets() {
    for url in [
        "file:///srv/private/report.html",
        "http://localhost:3000",
        "https://api.localhost/status",
        "http://127.0.0.1:8080",
        "http://[::1]:9000",
    ] {
        assert!(!rendering_client_may_open_url(url, false), "{url}");
        assert!(rendering_client_may_open_url(url, true), "{url}");
    }
    assert!(rendering_client_may_open_url(
        "https://example.com/report",
        false
    ));
}

#[cfg(test)]
#[test]
fn group_default_location_keeps_ssh_host_without_typed_directory() {
    let profile = crate::persist::ssh_profiles::SshConnectionProfile::new(
        "workbox",
        "Work box",
        "alice@workbox",
        Some(crate::execution_host::HostPath::new("/srv/work").expect("valid path")),
    )
    .expect("valid profile");
    let host = profile.execution_host_id();
    let location = group_default_location_for(std::slice::from_ref(&profile), &host, " ")
        .expect("remote default");
    assert_eq!(location.execution_host_id, host);
    assert_eq!(location.path.as_path(), std::path::Path::new("/srv/work"));
    assert!(group_default_location_for(
        std::slice::from_ref(&profile),
        &crate::execution_host::ExecutionHostId::local(),
        " "
    )
    .is_none());
}

#[cfg(test)]
#[test]
fn group_host_cycle_fills_suggested_directory_when_empty() {
    let profile = crate::persist::ssh_profiles::SshConnectionProfile::new(
        "workbox",
        "Work box",
        "alice@workbox",
        Some(crate::execution_host::HostPath::new("/srv/work").expect("valid path")),
    )
    .expect("valid profile");
    let expected_host = profile.execution_host_id();
    let mut host = crate::execution_host::ExecutionHostId::local();
    let mut directory = String::new();
    apply_group_host_cycle(std::slice::from_ref(&profile), &mut host, &mut directory);
    assert_eq!(host, expected_host);
    assert_eq!(directory, "/srv/work");
}

pub(super) mod agent_profile_picker;
mod clipboard;
mod command_palette;
mod copy_mode;
mod lease;
mod modal;
mod mouse;
mod navigate;
mod overlays;
mod selection;
mod settings;
mod sidebar;
mod terminal;

pub(crate) use self::{
    command_palette::{
        handle_command_palette_key_for_view, handle_command_palette_mouse_for_view,
        selected_command_palette_action_for_view,
    },
    modal::{
        apply_keybind_help_key, context_menu_state_for_pane, global_menu_actions,
        insert_keybind_help_query_text, modal_action_from_buttons, pane_context_menu_state,
        request_detach, GlobalMenuAction, KeybindHelpKeyResult, ModalAction,
    },
    navigate::{
        command_for_key, indexed_navigation_action, non_indexed_action_for_key,
        terminal_direct_indexed_navigation_action, terminal_direct_non_indexed_navigation_action,
        ActionContext, BindingDispatch, NavigateAction,
    },
    settings::{
        close_agent_profile_editor_for_view, paste_settings_text_for_view,
        prepare_general_settings_state, prepare_group_settings_state,
        prepare_workspace_settings_state, update_settings_mouse_for_view,
        update_settings_state_for_view, SettingsAction,
    },
    sidebar::{
        agent_menu_rows, group_menu_rows, AgentMenuAction, FilterMenuRow, GroupDropTarget,
        GroupMenuAction, WorkspaceDropTarget,
    },
};

#[cfg(test)]
pub(crate) use self::command_palette::open_command_palette_for_view;
pub(crate) use self::lease::{InputLeaseKey, InputLeaseTable, RepeatPlan, TerminalInputContext};
pub(crate) use self::terminal::TerminalKeyTarget;
#[cfg(test)]
use super::state::AppState;
use super::state::Mode;
use super::App;

// ---------------------------------------------------------------------------
// Key handling
// ---------------------------------------------------------------------------

impl App {
    #[cfg(test)]
    pub(super) async fn handle_key(&mut self, key: TerminalKey) -> Option<TerminalKeyTarget> {
        self.route_default_client_key(key);
        None
    }

    pub(crate) fn handle_text_commit_for_view(
        &mut self,
        client_view: &mut super::ClientViewState,
        text: &str,
    ) {
        if text.is_empty() {
            return;
        }
        if client_view.popup_pane.is_some() {
            let _ = self.send_popup_text_for_view(client_view, text);
            return;
        }
        if client_view.mode != Mode::Terminal {
            self.paste_for_view(client_view, text);
            return;
        }

        client_view.selection = None;
        client_view.selection_autoscroll = None;
        self.selection_autoscroll_deadline = None;
        self.state.update_dismissed = true;
        let Some(ws_idx) = client_view.active_workspace else {
            return;
        };
        let Some((_, pane_id)) = client_view.focused_pane_for_workspace(&self.state, ws_idx) else {
            return;
        };
        if let Some(runtime) =
            self.state
                .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
        {
            let _ = runtime.try_send_bytes(bytes::Bytes::copy_from_slice(text.as_bytes()));
        }
    }

    #[cfg(test)]
    pub(super) async fn handle_text_commit(&mut self, text: String) {
        self.with_default_client_view(|app, view| app.handle_text_commit_for_view(view, &text));
    }

    #[cfg(test)]
    pub(super) async fn handle_paste(&mut self, text: String) {
        self.with_default_client_view(|app, view| {
            app.paste_for_view(view, &text);
        });
    }

    #[cfg(test)]
    pub(super) fn handle_mouse(&mut self, mouse: MouseEvent) {
        self.with_default_client_view(|app, view| app.handle_mouse_for_view(view, mouse));
    }
}

// ---------------------------------------------------------------------------
// Mouse handling
// ---------------------------------------------------------------------------

#[cfg(test)]
fn state_with_workspaces(names: &[&str]) -> AppState {
    let mut state = AppState::test_new();
    state.workspaces = names
        .iter()
        .map(|name| crate::workspace::Workspace::test_new(name))
        .collect();
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
    app.state.host_display =
        crate::app::host_label::HostDisplayNameOverlay::from_config_or_hostname("test-host", None);
    app.default_client_view.mode = Mode::Terminal;
    app.state.sidebar_arrangement = crate::config::SidebarArrangementConfig::CombinedLeft;
    app.state.update_available = None;
    app.state.latest_release_notes_available = false;
    app.state.toast_config.delay_seconds = 0;
    app.default_client_view.computed.sidebar_rect = ratatui::layout::Rect::new(0, 0, 26, 20);
    app.default_client_view.computed.terminal_area = ratatui::layout::Rect::new(26, 0, 80, 20);
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
fn unique_temp_path(name: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("gardn-{name}-{}-{nanos}", std::process::id()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        App::new(
            &crate::config::Config::default(),
            true,
            None,
            tokio::sync::mpsc::unbounded_channel().1,
            crate::api::EventHub::default(),
        )
    }

    #[tokio::test]
    async fn paste_routes_to_rename_modal_input() {
        let mut app = test_app();
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("test")];
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.mode = Mode::RenameTab;
        app.default_client_view.name_input = "2".into();
        app.default_client_view.name_input_replace_on_type = true;

        app.handle_paste("feature/logs".into()).await;

        assert_eq!(app.default_client_view.name_input, "feature/logs");
        assert!(!app.default_client_view.name_input_replace_on_type);
    }

    #[tokio::test]
    async fn paste_routes_to_keybind_help_query_only_when_searching() {
        let mut app = test_app();
        app.default_client_view.mode = Mode::KeybindHelp;
        app.handle_paste("ignored".into()).await;
        assert!(app.default_client_view.keybind_help.query.is_empty());

        app.default_client_view.keybind_help.search_focused = true;
        app.default_client_view.keybind_help.scroll = 3;
        app.handle_paste("work\nspace".into()).await;

        assert_eq!(app.default_client_view.keybind_help.query, "workspace");
        assert_eq!(app.default_client_view.keybind_help.scroll, 0);
    }

    #[tokio::test]
    async fn text_commit_slash_focuses_keybind_help_search() {
        let mut app = test_app();
        app.default_client_view.mode = Mode::KeybindHelp;

        app.handle_text_commit("/".into()).await;

        assert!(app.default_client_view.keybind_help.search_focused);
        assert!(app.default_client_view.keybind_help.query.is_empty());
    }
}
