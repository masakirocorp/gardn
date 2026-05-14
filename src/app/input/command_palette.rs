use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Direction;

use crate::app::{
    command_palette::{
        command_palette_filtered_commands, CommandPaletteAction, CommandPaletteCommand,
    },
    state::{AppState, CommandPaletteWheelGate, Mode},
    App,
};

const WHEEL_EVENTS_PER_SELECTION_STEP: u8 = 16;

pub(super) fn open_command_palette(state: &mut AppState) {
    state.command_palette.query.clear();
    state.command_palette.selected = 0;
    state.command_palette.scroll = 0;
    state.command_palette.wheel_gate = None;
    state.mode = Mode::CommandPalette;
}

pub(super) fn command_palette_visible_commands(state: &AppState) -> Vec<CommandPaletteCommand> {
    command_palette_filtered_commands(state)
}

impl App {
    pub(crate) fn handle_command_palette_key(&mut self, key: KeyEvent) {
        self.state.command_palette.wheel_gate = None;
        match key.code {
            KeyCode::Esc => leave_command_palette(&mut self.state),
            KeyCode::Enter => self.execute_selected_command_palette_command(),
            KeyCode::Up => {
                move_command_palette_selection(&mut self.state, false);
            }
            KeyCode::Down => {
                move_command_palette_selection(&mut self.state, true);
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

    fn execute_selected_command_palette_command(&mut self) {
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

fn clamp_command_palette_selection(state: &mut AppState) {
    let count = command_palette_visible_commands(state).len();
    if count == 0 {
        state.command_palette.selected = 0;
        state.command_palette.scroll = 0;
        return;
    }

    state.command_palette.selected = state.command_palette.selected.min(count - 1);
    if state.command_palette.selected < state.command_palette.scroll {
        state.command_palette.scroll = state.command_palette.selected;
    }
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
    changed
}

pub(super) fn scroll_command_palette_selection(state: &mut AppState, down: bool) {
    if let Some(gate) = state.command_palette.wheel_gate {
        if gate.down == down && gate.remaining_events > 0 {
            state.command_palette.wheel_gate = Some(CommandPaletteWheelGate {
                down,
                remaining_events: gate.remaining_events - 1,
            });
            return;
        }
    }

    move_command_palette_selection(state, down);
    state.command_palette.wheel_gate = Some(CommandPaletteWheelGate {
        down,
        remaining_events: WHEEL_EVENTS_PER_SELECTION_STEP.saturating_sub(1),
    });
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
        CommandPaletteAction::Fullscreen => app.state.toggle_fullscreen(),
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
}
