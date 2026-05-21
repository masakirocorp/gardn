use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Direction, Rect};

use crate::app::{
    command_palette::{
        command_palette_filtered_commands, CommandPaletteAction, CommandPaletteCommand,
    },
    state::{AppState, Mode},
    App,
};

use super::{
    modal::modal_action_from_buttons, modal::ModalAction, ScrollbarClickTarget,
    MODAL_PAGE_SCROLL_ROWS,
};

pub(super) fn open_command_palette(state: &mut AppState) {
    state.command_palette.query.clear();
    state.command_palette.selected = 0;
    state.command_palette.scroll = 0;
    state.mode = Mode::CommandPalette;
}

pub(super) fn command_palette_visible_commands(state: &AppState) -> Vec<CommandPaletteCommand> {
    command_palette_filtered_commands(state)
}

impl App {
    pub(crate) fn handle_command_palette_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => leave_command_palette(&mut self.state),
            KeyCode::Enter => self.execute_selected_command_palette_command(),
            KeyCode::Up => {
                move_command_palette_selection(&mut self.state, false);
            }
            KeyCode::Down => {
                move_command_palette_selection(&mut self.state, true);
            }
            KeyCode::PageUp => {
                scroll_command_palette_rows(&mut self.state, -MODAL_PAGE_SCROLL_ROWS)
            }
            KeyCode::PageDown => {
                scroll_command_palette_rows(&mut self.state, MODAL_PAGE_SCROLL_ROWS)
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_command_palette_selection(&mut self.state, false);
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                move_command_palette_selection(&mut self.state, true);
            }
            KeyCode::Backspace => {
                self.state.command_palette.query.pop();
                clamp_command_palette_selection(&mut self.state);
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.state.command_palette.query.push(c);
                clamp_command_palette_selection(&mut self.state);
            }
            _ => {}
        }
    }

    pub(super) fn execute_selected_command_palette_command(&mut self) {
        let commands = command_palette_visible_commands(&self.state);
        let Some(command) = commands.get(self.state.command_palette.selected).cloned() else {
            return;
        };
        execute_command_palette_action(self, command.action);
    }
}

fn leave_command_palette(state: &mut AppState) {
    state.mode = if state.active.is_some() {
        Mode::Terminal
    } else {
        Mode::Navigate
    };
}

pub(super) fn close_command_palette(state: &mut AppState) {
    leave_command_palette(state);
}

pub(super) fn command_palette_action_button_at(
    state: &AppState,
    col: u16,
    row: u16,
) -> Option<ModalAction> {
    let inner = command_palette_inner_rect(state)?;
    let (run, close) = crate::ui::command_palette_button_rects(inner);
    modal_action_from_buttons(
        col,
        row,
        &[(run, ModalAction::Apply), (close, ModalAction::Close)],
    )
}

fn clamp_command_palette_selection(state: &mut AppState) {
    let count = command_palette_visible_commands(state).len();
    if count == 0 {
        state.command_palette.selected = 0;
        state.command_palette.scroll = 0;
        return;
    }

    state.command_palette.selected = state.command_palette.selected.min(count - 1);
    ensure_command_palette_selection_visible(state);
}

fn move_command_palette_selection(state: &mut AppState, down: bool) -> bool {
    let count = command_palette_visible_commands(state).len();
    if count == 0 {
        state.command_palette.selected = 0;
        state.command_palette.scroll = 0;
        return false;
    }

    let next = if down {
        (state.command_palette.selected + 1).min(count - 1)
    } else {
        state.command_palette.selected.saturating_sub(1)
    };
    let changed = next != state.command_palette.selected;
    state.command_palette.selected = next;
    ensure_command_palette_selection_visible(state);
    changed
}

pub(super) fn scroll_command_palette_rows(state: &mut AppState, delta: i16) {
    let max_scroll = command_palette_max_scroll(state);
    let next = if delta.is_negative() {
        state
            .command_palette
            .scroll
            .saturating_sub(delta.unsigned_abs() as usize)
    } else {
        state
            .command_palette
            .scroll
            .saturating_add(delta as usize)
            .min(max_scroll)
    };
    state.command_palette.scroll = next.min(max_scroll);
}

pub(super) fn hover_command_palette_selection(state: &mut AppState, col: u16, row: u16) {
    let Some((list_area, rows, start)) = command_palette_visible_rows(state) else {
        return;
    };
    if col < list_area.x
        || col >= list_area.x + list_area.width
        || row < list_area.y
        || row >= list_area.y + list_area.height
    {
        return;
    }
    if command_palette_scrollbar_track(state).is_some_and(|track| {
        col >= track.x
            && col < track.x + track.width
            && row >= track.y
            && row < track.y + track.height
    }) {
        return;
    }

    let row_idx = start + row.saturating_sub(list_area.y) as usize;
    if let Some(Some(command_idx)) = rows.get(row_idx) {
        state.command_palette.selected = *command_idx;
    }
}

pub(super) fn command_palette_contains_point(state: &AppState, col: u16, row: u16) -> bool {
    command_palette_popup_rect(state).is_some_and(|popup| {
        col >= popup.x
            && col < popup.x + popup.width
            && row >= popup.y
            && row < popup.y + popup.height
    })
}

pub(super) fn command_palette_scrollbar_target_at(
    state: &AppState,
    col: u16,
    row: u16,
) -> Option<ScrollbarClickTarget> {
    let metrics = command_palette_scroll_metrics(state)?;
    let track = command_palette_scrollbar_track(state)?;
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

pub(super) fn command_palette_offset_for_drag_row(
    state: &AppState,
    row: u16,
    grab_row_offset: u16,
) -> Option<usize> {
    let metrics = command_palette_scroll_metrics(state)?;
    let track = command_palette_scrollbar_track(state)?;
    Some(crate::ui::scrollbar_offset_from_drag_row(
        metrics,
        track,
        row,
        grab_row_offset,
    ))
}

pub(super) fn set_command_palette_offset_from_bottom(
    state: &mut AppState,
    offset_from_bottom: usize,
) {
    let Some((list_area, rows)) = command_palette_rows_for_input(state) else {
        state.command_palette.scroll = 0;
        return;
    };
    state.command_palette.scroll = crate::ui::modal_scroll_from_offset_from_bottom(
        rows.len(),
        list_area.height as usize,
        offset_from_bottom,
    );
}

fn command_palette_visible_rows(state: &AppState) -> Option<(Rect, Vec<Option<usize>>, usize)> {
    let (list_area, rows) = command_palette_rows_for_input(state)?;
    let visible_rows = list_area.height as usize;
    let max_start = rows.len().saturating_sub(visible_rows);
    let start = state.command_palette.scroll.min(max_start);
    Some((list_area, rows, start))
}

fn command_palette_scroll_metrics(state: &AppState) -> Option<crate::pane::ScrollMetrics> {
    let (list_area, rows) = command_palette_rows_for_input(state)?;
    Some(crate::ui::modal_scroll_metrics(
        rows.len(),
        list_area.height as usize,
        state.command_palette.scroll,
    ))
}

fn command_palette_max_scroll(state: &AppState) -> usize {
    let Some((list_area, rows)) = command_palette_rows_for_input(state) else {
        return 0;
    };
    rows.len().saturating_sub(list_area.height as usize)
}

fn command_palette_scrollbar_track(state: &AppState) -> Option<Rect> {
    let metrics = command_palette_scroll_metrics(state)?;
    if !crate::ui::should_show_scrollbar(metrics) {
        return None;
    }
    let list_area = command_palette_list_area(state)?;
    crate::ui::modal_scrollbar_rect(list_area, metrics)
}

fn ensure_command_palette_selection_visible(state: &mut AppState) {
    let Some((list_area, rows)) = command_palette_rows_for_input(state) else {
        state.command_palette.scroll = 0;
        return;
    };
    let visible_rows = list_area.height as usize;
    if visible_rows == 0 {
        state.command_palette.scroll = 0;
        return;
    }

    let max_start = rows.len().saturating_sub(visible_rows);
    let Some(selected_row) = rows
        .iter()
        .position(|row| *row == Some(state.command_palette.selected))
    else {
        state.command_palette.scroll = state.command_palette.scroll.min(max_start);
        return;
    };

    let start = state.command_palette.scroll.min(max_start);
    let first_section_row = selected_row
        .checked_sub(1)
        .filter(|idx| rows.get(*idx).is_some_and(Option::is_none))
        .unwrap_or(selected_row);
    state.command_palette.scroll = if first_section_row < start {
        first_section_row
    } else if selected_row >= start + visible_rows {
        selected_row + 1 - visible_rows
    } else {
        start
    };
}

fn command_palette_rows_for_input(state: &AppState) -> Option<(Rect, Vec<Option<usize>>)> {
    let list_area = command_palette_list_area(state)?;
    if list_area.height == 0 {
        return None;
    }

    let commands = command_palette_visible_commands(state);
    if commands.is_empty() {
        return None;
    }
    let mut rows = Vec::new();
    let mut last_group = None;
    for (idx, command) in commands.iter().enumerate() {
        if last_group != Some(command.group) {
            if last_group.is_some() {
                rows.push(None);
            }
            rows.push(None);
            last_group = Some(command.group);
        }
        rows.push(Some(idx));
    }

    Some((list_area, rows))
}

fn command_palette_list_area(state: &AppState) -> Option<Rect> {
    let inner = command_palette_inner_rect(state)?;
    if inner.height < 6 || inner.width < 20 {
        return None;
    }

    Some(Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner.height.saturating_sub(5),
    ))
}

fn command_palette_inner_rect(state: &AppState) -> Option<Rect> {
    let popup = command_palette_popup_rect(state)?;
    Some(Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    ))
}

fn command_palette_popup_rect(state: &AppState) -> Option<Rect> {
    crate::ui::centered_popup_rect(command_palette_screen_rect(state), 76, 18)
}

fn command_palette_screen_rect(state: &AppState) -> Rect {
    let sidebar = state.view.sidebar_rect;
    let right_sidebar = state.view.right_sidebar_rect;
    let terminal = state.view.terminal_area;
    let x = sidebar.x.min(terminal.x).min(right_sidebar.x);
    let y = sidebar.y.min(terminal.y).min(right_sidebar.y);
    let right = (sidebar.x + sidebar.width)
        .max(terminal.x + terminal.width)
        .max(right_sidebar.x + right_sidebar.width);
    let bottom = (sidebar.y + sidebar.height)
        .max(terminal.y + terminal.height)
        .max(right_sidebar.y + right_sidebar.height);
    Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}

fn execute_command_palette_action(app: &mut App, action: CommandPaletteAction) {
    match action {
        CommandPaletteAction::NewWorkspace => app.state.request_new_workspace = true,
        CommandPaletteAction::RenameWorkspace => {
            let selected = app.state.selected;
            if app.state.workspace_in_active_group(selected) {
                super::modal::open_rename_workspace(&mut app.state, selected);
                return;
            }
        }
        CommandPaletteAction::CloseWorkspace => {
            if app.state.workspace_in_active_group(app.state.selected) {
                if app.state.confirm_close {
                    super::modal::open_confirm_close(&mut app.state);
                    return;
                }
                app.state.close_selected_workspace();
            }
        }
        CommandPaletteAction::PreviousWorkspace => app.state.previous_workspace(),
        CommandPaletteAction::NextWorkspace => app.state.next_workspace(),
        CommandPaletteAction::SwitchWorkspace(idx) => app.state.switch_workspace(idx),
        CommandPaletteAction::NewTab => {
            super::modal::open_new_tab_dialog(&mut app.state);
            return;
        }
        CommandPaletteAction::RenameTab => {
            super::modal::open_rename_active_tab(&mut app.state, false);
            return;
        }
        CommandPaletteAction::PreviousTab => app.state.previous_tab(),
        CommandPaletteAction::NextTab => app.state.next_tab(),
        CommandPaletteAction::CloseTab => {
            app.state.close_tab();
            if app.state.mode == Mode::ConfirmClose {
                return;
            }
        }
        CommandPaletteAction::SplitVertical => app.state.split_pane(Direction::Horizontal),
        CommandPaletteAction::SplitHorizontal => app.state.split_pane(Direction::Vertical),
        CommandPaletteAction::ClosePane => app.state.close_pane(),
        CommandPaletteAction::RenamePane => {
            if let Some(pane_id) = app
                .state
                .active
                .and_then(|ws_idx| app.state.workspaces.get(ws_idx))
                .and_then(|ws| ws.focused_pane_id())
            {
                super::modal::open_rename_pane(&mut app.state, pane_id);
                return;
            }
        }
        CommandPaletteAction::Fullscreen => app.state.toggle_zoom(),
        CommandPaletteAction::ResizeMode => {
            app.state.mode = Mode::Resize;
            return;
        }
        CommandPaletteAction::FocusPane(direction) => app.state.navigate_pane(direction),
        CommandPaletteAction::CyclePaneNext => app.state.cycle_pane(false),
        CommandPaletteAction::CyclePanePrevious => app.state.cycle_pane(true),
        CommandPaletteAction::OpenGroupMenu => {
            super::modal::open_group_menu(&mut app.state);
            return;
        }
        CommandPaletteAction::ShowAllGroups => app.state.show_all_groups(),
        CommandPaletteAction::NewGroup => {
            super::modal::open_new_group_dialog(&mut app.state);
            return;
        }
        CommandPaletteAction::RenameGroup => {
            super::modal::open_rename_group(&mut app.state);
            return;
        }
        CommandPaletteAction::DeleteGroup => {
            let active_group = app.state.active_group;
            super::modal::open_confirm_delete_group(&mut app.state, active_group);
            return;
        }
        CommandPaletteAction::ToggleGroupFilter => app.state.toggle_group_filter(),
        CommandPaletteAction::PreviousGroup => app.state.previous_group(),
        CommandPaletteAction::NextGroup => app.state.next_group(),
        CommandPaletteAction::SwitchGroup(idx) => app.state.switch_group(idx),
        CommandPaletteAction::OpenAgentMenu => {
            super::modal::open_agent_menu(&mut app.state);
            return;
        }
        CommandPaletteAction::SetAgentScope(scope) => {
            app.state.agent_panel_scope = scope;
            app.state.agent_panel_scroll = 0;
            app.state.mark_session_dirty();
        }
        CommandPaletteAction::PreviousAgent => app.state.previous_agent(),
        CommandPaletteAction::NextAgent => app.state.next_agent(),
        CommandPaletteAction::OpenGitDiff => {
            let previous_toast = app.state.toast.clone();
            if let Err(err) = app.state.open_git_diff_panel() {
                app.state.toast = Some(crate::app::state::ToastNotification {
                    kind: crate::app::state::ToastKind::NeedsAttention,
                    title: "git diff failed".to_string(),
                    context: err,
                    target: None,
                });
                app.sync_toast_deadline(previous_toast);
            }
            return;
        }
        CommandPaletteAction::ToggleSidebar => {
            app.state.sidebar_collapsed = !app.state.sidebar_collapsed;
            app.state.mark_session_dirty();
        }
        CommandPaletteAction::ToggleRightSidebar => {
            if app.state.view.right_sidebar_rect != ratatui::layout::Rect::default() {
                app.state.right_sidebar_collapsed = !app.state.right_sidebar_collapsed;
                app.state.mark_session_dirty();
            }
        }
        CommandPaletteAction::OpenGlobalMenu => {
            super::modal::open_global_menu(&mut app.state);
            return;
        }
        CommandPaletteAction::OpenSettings => {
            super::settings::open_settings(&mut app.state);
            return;
        }
        CommandPaletteAction::OpenKeybinds => {
            super::modal::open_keybind_help(&mut app.state);
            return;
        }
        CommandPaletteAction::ReloadConfig => app.state.request_reload_config = true,
        CommandPaletteAction::OpenNotificationTarget => {
            app.state.focus_toast_target();
            if app.state.mode != Mode::CommandPalette {
                return;
            }
        }
        CommandPaletteAction::DetachOrQuit => super::modal::request_quit_or_detach(&mut app.state),
        CommandPaletteAction::CustomCommand(idx) => {
            let Some(binding) = app.state.keybinds.custom_commands.get(idx).cloned() else {
                return;
            };
            app.launch_custom_command(binding);
            return;
        }
    }

    leave_command_palette(&mut app.state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, workspace::Workspace};

    fn app_with_space() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::CommandPalette;
        app
    }

    #[test]
    fn command_palette_filters_commands_by_query() {
        let mut app = app_with_space();
        app.state.command_palette.query = "right side".to_string();

        let commands = command_palette_visible_commands(&app.state);

        assert!(commands
            .iter()
            .any(|command| command.title == "toggle right sidebar"));
        assert!(commands
            .iter()
            .all(|command| command.title.contains("right") || command.group.contains("right")));
    }

    #[test]
    fn command_palette_enter_executes_selected_command() {
        let mut app = app_with_space();
        app.state.command_palette.query = "new tab".to_string();

        app.handle_command_palette_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));

        assert_eq!(app.state.mode, Mode::RenameTab);
        assert!(app.state.creating_new_tab);
    }

    #[test]
    fn command_palette_selection_clamps() {
        let mut app = app_with_space();

        app.handle_command_palette_key(KeyEvent::new(KeyCode::Up, KeyModifiers::empty()));
        assert_eq!(app.state.command_palette.selected, 0);

        app.handle_command_palette_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));
        assert_eq!(app.state.command_palette.selected, 1);
    }

    #[test]
    fn command_palette_page_keys_scroll_rows_without_changing_selection() {
        let mut app = app_with_space();
        app.state.view.sidebar_rect = ratatui::layout::Rect::new(0, 0, 26, 20);
        app.state.view.terminal_area = ratatui::layout::Rect::new(26, 0, 80, 20);

        app.handle_command_palette_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::empty()));
        assert_eq!(app.state.command_palette.selected, 0);
        assert_eq!(
            app.state.command_palette.scroll,
            MODAL_PAGE_SCROLL_ROWS as usize
        );

        app.handle_command_palette_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty()));
        assert_eq!(app.state.command_palette.selected, 0);
        assert_eq!(app.state.command_palette.scroll, 0);
    }

    #[test]
    fn command_palette_keeps_section_header_reachable_at_top() {
        let mut app = app_with_space();
        app.state.view.sidebar_rect = ratatui::layout::Rect::new(0, 0, 26, 20);
        app.state.view.terminal_area = ratatui::layout::Rect::new(26, 0, 80, 20);

        for _ in 0..12 {
            app.handle_command_palette_key(KeyEvent::new(KeyCode::Down, KeyModifiers::empty()));
        }
        assert!(app.state.command_palette.scroll > 0);

        for _ in 0..12 {
            app.handle_command_palette_key(KeyEvent::new(KeyCode::Up, KeyModifiers::empty()));
        }

        assert_eq!(app.state.command_palette.selected, 0);
        assert_eq!(app.state.command_palette.scroll, 0);
    }

    #[test]
    fn command_palette_commands_include_keybind_labels() {
        let app = app_with_space();

        let commands = command_palette_visible_commands(&app.state);

        assert!(commands.iter().any(|command| {
            command.title == "new space"
                && command.key_label.as_deref()
                    == Some(app.state.keybinds.new_workspace_label.as_str())
        }));
    }

    #[test]
    fn command_palette_includes_git_diff_launcher() {
        let app = app_with_space();

        let commands = command_palette_visible_commands(&app.state);

        assert!(commands.iter().any(|command| {
            command.title == "open git diff"
                && command.group == "git"
                && command.action == CommandPaletteAction::OpenGitDiff
        }));
    }
}
