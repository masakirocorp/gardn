use std::collections::{HashMap, HashSet};

use crate::app::state::{
    AgentProfilePickerState, AppState, CommandPaletteState, ContextMenuState, CopyModeState,
    DiffAgentPickerState, DragState, GitRepoPickerState, GroupPressState, KeybindHelpState,
    MenuListState, Mode, NavigatorState, PaneFocusTarget, ProductAnnouncementState,
    ReleaseNotesState, RightClickPassthroughGesture, SelectionAutoscroll, SettingsState,
    TabPressState, ViewState, WorkspacePressState,
};
use crate::layout::PaneId;
use crate::native_diff::NativeDiffPaneViewState;
use crate::terminal::{TerminalId, TerminalRuntimeRegistry};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ClientTabViewKey {
    pub(crate) workspace_id: String,
    pub(crate) tab_number: usize,
}

impl ClientTabViewKey {
    fn new(workspace_id: &str, tab_number: usize) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            tab_number,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ClientPaneViewKey {
    pub(crate) workspace_id: String,
    pub(crate) tab_number: usize,
    pub(crate) pane_id: PaneId,
}

impl ClientPaneViewKey {
    fn new(workspace_id: &str, tab_number: usize, pane_id: PaneId) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            tab_number,
            pane_id,
        }
    }
}

/// Per-normal-app-client view/navigation state.
///
/// This type stores fields that describe what one attached app client is
/// looking at. Shared session structures remain in `AppState`; callers must
/// explicitly run view-sensitive work through the client's state instead of
/// implicitly reading whichever client last touched the server.
#[derive(Clone)]
pub(crate) struct ClientViewState {
    pub(crate) active_workspace: Option<usize>,
    pub(crate) selected_workspace: usize,
    pub(crate) active_group: usize,
    pub(crate) group_filter_enabled: bool,
    pub(crate) agent_panel_scope: crate::app::state::AgentPanelScope,
    pub(crate) workspace_scroll: usize,
    pub(crate) agent_panel_scroll: usize,
    pub(crate) tab_scroll: usize,
    pub(crate) tab_scroll_follow_active: bool,
    pub(crate) hovered_tab: Option<usize>,
    pub(crate) mobile_switcher_scroll: usize,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) right_sidebar_collapsed: bool,
    pub(crate) activity_agents_expanded: bool,
    pub(crate) activity_commands_expanded: bool,
    pub(crate) activity_ports_expanded: bool,
    pub(crate) collapsed_agent_sections: Vec<String>,
    pub(crate) collapsed_command_groups: Vec<String>,
    pub(crate) collapsed_command_status_groups: Vec<String>,
    pub(crate) collapsed_workspace_groups: Vec<String>,
    pub(crate) mode: Mode,
    pub(crate) active_tabs: HashMap<String, usize>,
    pub(crate) focused_panes: HashMap<ClientTabViewKey, PaneId>,
    pub(crate) zoomed_tabs: HashSet<ClientTabViewKey>,
    pub(crate) native_diff_panes: HashMap<ClientPaneViewKey, NativeDiffPaneViewState>,
    pub(crate) terminal_offsets_from_bottom: HashMap<TerminalId, usize>,
    pub(crate) settings: SettingsState,
    pub(crate) command_palette: CommandPaletteState,
    pub(crate) navigator: NavigatorState,
    pub(crate) agent_profile_picker: AgentProfilePickerState,
    pub(crate) diff_agent_picker: Option<DiffAgentPickerState>,
    pub(crate) git_repo_picker: GitRepoPickerState,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) copy_mode: Option<CopyModeState>,
    pub(crate) selection: Option<crate::selection::Selection>,
    pub(crate) selection_autoscroll: Option<SelectionAutoscroll>,
    pub(crate) drag: Option<DragState>,
    pub(crate) workspace_press: Option<WorkspacePressState>,
    pub(crate) group_press: Option<GroupPressState>,
    pub(crate) tab_press: Option<TabPressState>,
    pub(crate) previous_pane_focus: Option<PaneFocusTarget>,
    pub(crate) right_click_passthrough: Option<RightClickPassthroughGesture>,
    pub(crate) keybind_help: KeybindHelpState,
    pub(crate) global_menu: MenuListState,
    pub(crate) group_menu: MenuListState,
    pub(crate) agent_menu: MenuListState,
    pub(crate) creating_new_tab: bool,
    pub(crate) creating_new_group: bool,
    pub(crate) group_icon_input: String,
    pub(crate) group_default_directory_input: String,
    pub(crate) group_modal_selected_field: usize,
    pub(crate) group_icon_picker_open: bool,
    pub(crate) rename_group_target: Option<usize>,
    pub(crate) requested_new_tab_name: Option<String>,
    pub(crate) rename_pane_target: Option<PaneId>,
    pub(crate) confirm_delete_group: Option<usize>,
    pub(crate) name_input: String,
    pub(crate) name_input_replace_on_type: bool,
    pub(crate) release_notes: Option<ReleaseNotesState>,
    pub(crate) product_announcement: Option<ProductAnnouncementState>,
    pub(crate) computed: ViewState,
}

impl ClientViewState {
    pub(crate) fn from_app_state(state: &AppState) -> Self {
        let mut view = Self {
            active_workspace: state.active,
            selected_workspace: state.selected,
            active_group: state.active_group,
            group_filter_enabled: state.group_filter_enabled,
            agent_panel_scope: state.agent_panel_scope,
            workspace_scroll: state.workspace_scroll,
            agent_panel_scroll: state.agent_panel_scroll,
            tab_scroll: state.tab_scroll,
            tab_scroll_follow_active: state.tab_scroll_follow_active,
            hovered_tab: state.hovered_tab,
            mobile_switcher_scroll: state.mobile_switcher_scroll,
            sidebar_collapsed: state.sidebar_collapsed,
            right_sidebar_collapsed: state.right_sidebar_collapsed,
            activity_agents_expanded: state.activity_agents_expanded,
            activity_commands_expanded: state.activity_commands_expanded,
            activity_ports_expanded: state.activity_ports_expanded,
            collapsed_agent_sections: state.collapsed_agent_sections.clone(),
            collapsed_command_groups: state.collapsed_command_groups.clone(),
            collapsed_command_status_groups: state.collapsed_command_status_groups.clone(),
            collapsed_workspace_groups: state.collapsed_workspace_groups.clone(),
            mode: state.mode,
            active_tabs: HashMap::new(),
            focused_panes: HashMap::new(),
            zoomed_tabs: HashSet::new(),
            native_diff_panes: HashMap::new(),
            terminal_offsets_from_bottom: HashMap::new(),
            settings: state.settings.clone(),
            command_palette: state.command_palette.clone(),
            navigator: state.navigator.clone(),
            agent_profile_picker: state.agent_profile_picker.clone(),
            diff_agent_picker: state.diff_agent_picker.clone(),
            git_repo_picker: state.git_repo_picker.clone(),
            context_menu: state.context_menu.clone(),
            copy_mode: state.copy_mode,
            selection: state.selection.clone(),
            selection_autoscroll: state.selection_autoscroll.clone(),
            drag: state.drag.clone(),
            workspace_press: state.workspace_press.clone(),
            group_press: state.group_press.clone(),
            tab_press: state.tab_press.clone(),
            previous_pane_focus: state.previous_pane_focus.clone(),
            right_click_passthrough: state.right_click_passthrough.clone(),
            keybind_help: state.keybind_help.clone(),
            global_menu: state.global_menu.clone(),
            group_menu: state.group_menu.clone(),
            agent_menu: state.agent_menu.clone(),
            creating_new_tab: state.creating_new_tab,
            creating_new_group: state.creating_new_group,
            group_icon_input: state.group_icon_input.clone(),
            group_default_directory_input: state.group_default_directory_input.clone(),
            group_modal_selected_field: state.group_modal_selected_field,
            group_icon_picker_open: state.group_icon_picker_open,
            rename_group_target: state.rename_group_target,
            requested_new_tab_name: state.requested_new_tab_name.clone(),
            rename_pane_target: state.rename_pane_target,
            confirm_delete_group: state.confirm_delete_group,
            name_input: state.name_input.clone(),
            name_input_replace_on_type: state.name_input_replace_on_type,
            release_notes: state.release_notes.clone(),
            product_announcement: state.product_announcement.clone(),
            computed: state.view.clone(),
        };
        view.reconcile(state);
        view
    }

    pub(crate) fn reconcile(&mut self, state: &AppState) {
        if state.groups.is_empty() {
            self.active_group = 0;
            self.group_filter_enabled = false;
        } else {
            self.active_group = self.active_group.min(state.groups.len() - 1);
        }

        if state.workspaces.is_empty() {
            self.active_workspace = None;
            self.selected_workspace = 0;
            self.active_tabs.clear();
            self.focused_panes.clear();
            self.zoomed_tabs.clear();
            self.terminal_offsets_from_bottom.clear();
            self.native_diff_panes.clear();
            return;
        }

        let visible_workspace = |idx: usize| {
            if !self.group_filter_enabled {
                return state.workspaces.get(idx).is_some();
            }

            let active_group_id = state
                .groups
                .get(self.active_group)
                .map(|group| group.id.as_str())
                .unwrap_or(crate::workspace::DEFAULT_GROUP_ID);
            state
                .workspaces
                .get(idx)
                .is_some_and(|workspace| workspace.group_id == active_group_id)
        };
        let first_visible_workspace = || {
            state
                .workspaces
                .iter()
                .enumerate()
                .find_map(|(idx, _)| visible_workspace(idx).then_some(idx))
        };

        if !self
            .active_workspace
            .is_some_and(|idx| idx < state.workspaces.len() && visible_workspace(idx))
        {
            self.active_workspace = if self.group_filter_enabled {
                first_visible_workspace()
            } else {
                state.active.filter(|idx| *idx < state.workspaces.len())
            };
        }
        if self.selected_workspace >= state.workspaces.len()
            || !visible_workspace(self.selected_workspace)
        {
            self.selected_workspace = self
                .active_workspace
                .or_else(first_visible_workspace)
                .unwrap_or(0);
        }

        let valid_workspace_ids: HashSet<&str> = state
            .workspaces
            .iter()
            .map(|workspace| workspace.id.as_str())
            .collect();
        self.active_tabs
            .retain(|workspace_id, _| valid_workspace_ids.contains(workspace_id.as_str()));
        self.focused_panes
            .retain(|key, _| valid_workspace_ids.contains(key.workspace_id.as_str()));
        self.zoomed_tabs
            .retain(|key| valid_workspace_ids.contains(key.workspace_id.as_str()));
        self.native_diff_panes
            .retain(|key, _| valid_workspace_ids.contains(key.workspace_id.as_str()));

        for workspace in &state.workspaces {
            if workspace.tabs.is_empty() {
                self.active_tabs.remove(&workspace.id);
                self.focused_panes
                    .retain(|key, _| key.workspace_id != workspace.id);
                self.zoomed_tabs
                    .retain(|key| key.workspace_id != workspace.id);
                self.native_diff_panes
                    .retain(|key, _| key.workspace_id != workspace.id);
                continue;
            }

            let active_tab = self
                .active_tabs
                .get(&workspace.id)
                .copied()
                .filter(|idx| *idx < workspace.tabs.len())
                .unwrap_or_else(|| workspace.active_tab.min(workspace.tabs.len() - 1));
            self.active_tabs.insert(workspace.id.clone(), active_tab);

            for (tab_idx, tab) in workspace.tabs.iter().enumerate() {
                let tab_number = tab_idx + 1;
                let tab_key = ClientTabViewKey::new(&workspace.id, tab_number);
                if !tab.panes.contains_key(
                    self.focused_panes
                        .get(&tab_key)
                        .unwrap_or(&tab.layout.focused()),
                ) {
                    self.focused_panes
                        .insert(tab_key.clone(), tab.layout.focused());
                } else {
                    self.focused_panes
                        .entry(tab_key.clone())
                        .or_insert_with(|| tab.layout.focused());
                }

                if tab.zoomed {
                    self.zoomed_tabs.insert(tab_key);
                }

                for (&pane_id, pane) in &tab.panes {
                    if let Some(diff) = pane.native_diff() {
                        self.native_diff_panes
                            .entry(ClientPaneViewKey::new(&workspace.id, tab_number, pane_id))
                            .or_insert_with(|| diff.view_state());
                    }
                }
            }

            let tab_count = workspace.tabs.len();
            self.focused_panes.retain(|key, _| {
                key.workspace_id != workspace.id || (1..=tab_count).contains(&key.tab_number)
            });
            self.zoomed_tabs.retain(|key| {
                key.workspace_id != workspace.id || (1..=tab_count).contains(&key.tab_number)
            });
            self.native_diff_panes.retain(|key, _| {
                key.workspace_id != workspace.id || (1..=tab_count).contains(&key.tab_number)
            });
            self.native_diff_panes.retain(|key, _| {
                if key.workspace_id != workspace.id {
                    return true;
                }
                workspace
                    .tabs
                    .get(key.tab_number.saturating_sub(1))
                    .is_some_and(|tab| {
                        tab.panes
                            .get(&key.pane_id)
                            .and_then(|pane| pane.native_diff())
                            .is_some()
                    })
            });
        }
    }

    pub(crate) fn active_tab_for_workspace(&self, workspace_id: &str) -> Option<usize> {
        self.active_tabs.get(workspace_id).copied()
    }

    pub(crate) fn focused_pane_for_tab(
        &self,
        workspace_id: &str,
        tab_number: usize,
    ) -> Option<PaneId> {
        self.focused_panes
            .get(&ClientTabViewKey::new(workspace_id, tab_number))
            .copied()
    }

    pub(crate) fn tab_is_zoomed(&self, workspace_id: &str, tab_number: usize) -> bool {
        self.zoomed_tabs
            .contains(&ClientTabViewKey::new(workspace_id, tab_number))
    }

    pub(crate) fn native_diff_view_for_pane(
        &self,
        workspace_id: &str,
        tab_number: usize,
        pane_id: PaneId,
    ) -> Option<&NativeDiffPaneViewState> {
        self.native_diff_panes
            .get(&ClientPaneViewKey::new(workspace_id, tab_number, pane_id))
    }
}
pub(crate) fn with_client_view_app_state<R>(
    state: &mut AppState,
    runtimes: &TerminalRuntimeRegistry,
    view: &mut ClientViewState,
    f: impl FnOnce(&mut AppState) -> R,
) -> R {
    let mut shared_view = ClientViewState::from_app_state(state);
    capture_terminal_offsets_from_app_state(state, runtimes, &mut shared_view);

    view.reconcile(state);
    apply_client_view_to_app_state(state, view);
    apply_terminal_offsets_to_runtimes(state, runtimes, view);
    let result = f(state);
    *view = ClientViewState::from_app_state(state);
    capture_terminal_offsets_from_app_state(state, runtimes, view);

    apply_client_view_to_app_state(state, &shared_view);
    apply_terminal_offsets_to_runtimes(state, runtimes, &shared_view);
    result
}

pub(crate) fn apply_client_view_to_app_state(state: &mut AppState, view: &ClientViewState) {
    state.active = view.active_workspace;
    state.selected = view.selected_workspace;
    state.active_group = view.active_group;
    state.group_filter_enabled = view.group_filter_enabled;
    state.agent_panel_scope = view.agent_panel_scope;
    state.workspace_scroll = view.workspace_scroll;
    state.agent_panel_scroll = view.agent_panel_scroll;
    state.tab_scroll = view.tab_scroll;
    state.tab_scroll_follow_active = view.tab_scroll_follow_active;
    state.hovered_tab = view.hovered_tab;
    state.mobile_switcher_scroll = view.mobile_switcher_scroll;
    state.sidebar_collapsed = view.sidebar_collapsed;
    state.right_sidebar_collapsed = view.right_sidebar_collapsed;
    state.activity_agents_expanded = view.activity_agents_expanded;
    state.activity_commands_expanded = view.activity_commands_expanded;
    state.activity_ports_expanded = view.activity_ports_expanded;
    state.collapsed_agent_sections = view.collapsed_agent_sections.clone();
    state.collapsed_command_groups = view.collapsed_command_groups.clone();
    state.collapsed_command_status_groups = view.collapsed_command_status_groups.clone();
    state.collapsed_workspace_groups = view.collapsed_workspace_groups.clone();
    state.mode = view.mode;
    state.view = view.computed.clone();
    state.settings = view.settings.clone();
    state.command_palette = view.command_palette.clone();
    state.navigator = view.navigator.clone();
    state.agent_profile_picker = view.agent_profile_picker.clone();
    state.diff_agent_picker = view.diff_agent_picker.clone();
    state.git_repo_picker = view.git_repo_picker.clone();
    state.context_menu = view.context_menu.clone();
    state.copy_mode = view.copy_mode;
    state.selection = view.selection.clone();
    state.selection_autoscroll = view.selection_autoscroll.clone();
    state.drag = view.drag.clone();
    state.workspace_press = view.workspace_press.clone();
    state.group_press = view.group_press.clone();
    state.tab_press = view.tab_press.clone();
    state.previous_pane_focus = view.previous_pane_focus.clone();
    state.right_click_passthrough = view.right_click_passthrough.clone();
    state.keybind_help = view.keybind_help.clone();
    state.global_menu = view.global_menu.clone();
    state.group_menu = view.group_menu.clone();
    state.agent_menu = view.agent_menu.clone();
    state.creating_new_tab = view.creating_new_tab;
    state.creating_new_group = view.creating_new_group;
    state.group_icon_input = view.group_icon_input.clone();
    state.group_default_directory_input = view.group_default_directory_input.clone();
    state.group_modal_selected_field = view.group_modal_selected_field;
    state.group_icon_picker_open = view.group_icon_picker_open;
    state.rename_group_target = view.rename_group_target;
    state.requested_new_tab_name = view.requested_new_tab_name.clone();
    state.rename_pane_target = view.rename_pane_target;
    state.confirm_delete_group = view.confirm_delete_group;
    state.name_input = view.name_input.clone();
    state.name_input_replace_on_type = view.name_input_replace_on_type;
    state.release_notes = view.release_notes.clone();
    state.product_announcement = view.product_announcement.clone();

    for workspace in &mut state.workspaces {
        if let Some(active_tab) = view.active_tab_for_workspace(&workspace.id) {
            workspace.active_tab = active_tab.min(workspace.tabs.len().saturating_sub(1));
        }

        for (tab_idx, tab) in workspace.tabs.iter_mut().enumerate() {
            let tab_number = tab_idx + 1;
            if let Some(focused_pane) = view.focused_pane_for_tab(&workspace.id, tab_number) {
                tab.layout.focus_pane(focused_pane);
            }
            tab.zoomed = view.tab_is_zoomed(&workspace.id, tab_number);

            for (&pane_id, pane) in &mut tab.panes {
                let Some(native_diff_view) =
                    view.native_diff_view_for_pane(&workspace.id, tab_number, pane_id)
                else {
                    continue;
                };
                let Some(diff) = pane.native_diff_mut() else {
                    continue;
                };
                diff.apply_view_state(native_diff_view);
            }
        }
    }
}

pub(crate) fn capture_terminal_offsets_from_app_state(
    state: &AppState,
    runtimes: &TerminalRuntimeRegistry,
    view: &mut ClientViewState,
) {
    let mut live_terminal_ids = HashSet::new();
    for workspace in &state.workspaces {
        for tab in &workspace.tabs {
            for pane in tab.panes.values() {
                let Some(terminal_id) = pane.terminal_id() else {
                    continue;
                };
                live_terminal_ids.insert(terminal_id.clone());
                let Some(metrics) = runtimes
                    .get(terminal_id)
                    .and_then(|runtime| runtime.scroll_metrics())
                else {
                    continue;
                };
                view.terminal_offsets_from_bottom
                    .insert(terminal_id.clone(), metrics.offset_from_bottom);
            }
        }
    }
    view.terminal_offsets_from_bottom
        .retain(|terminal_id, _| live_terminal_ids.contains(terminal_id));
}

pub(crate) fn apply_terminal_offsets_to_runtimes(
    state: &AppState,
    runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) {
    for workspace in &state.workspaces {
        for tab in &workspace.tabs {
            for pane in tab.panes.values() {
                let Some(terminal_id) = pane.terminal_id() else {
                    continue;
                };
                let Some(offset) = view.terminal_offsets_from_bottom.get(terminal_id) else {
                    continue;
                };
                let Some(runtime) = runtimes.get(terminal_id) else {
                    continue;
                };
                runtime.set_scroll_offset_from_bottom(*offset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    #[test]
    fn default_view_matches_current_empty_app_state() {
        let state = AppState::test_new();

        let view = ClientViewState::from_app_state(&state);

        assert_eq!(view.active_workspace, None);
        assert_eq!(view.selected_workspace, 0);
        assert_eq!(view.active_group, 0);
        assert!(view.group_filter_enabled);
        assert_eq!(
            view.agent_panel_scope,
            crate::app::state::AgentPanelScope::CurrentWorkspace
        );
        assert_eq!(view.agent_panel_scroll, 0);
        assert_eq!(view.mode, Mode::Navigate);
        assert!(view.active_tabs.is_empty());
        assert!(view.focused_panes.is_empty());
    }

    #[test]
    fn default_view_captures_workspace_tab_focus_and_zoom() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        state.active = Some(1);
        state.selected = 1;
        state.mode = Mode::Terminal;
        state.workspaces[0].tabs[0].zoomed = true;

        let first_workspace_id = state.workspaces[0].id.clone();
        let second_workspace_id = state.workspaces[1].id.clone();
        let first_focused = state.workspaces[0].tabs[0].layout.focused();
        let second_focused = state.workspaces[1].tabs[0].layout.focused();

        let view = ClientViewState::from_app_state(&state);

        assert_eq!(view.active_workspace, Some(1));
        assert_eq!(view.selected_workspace, 1);
        assert_eq!(view.mode, Mode::Terminal);
        assert_eq!(view.active_tab_for_workspace(&first_workspace_id), Some(0));
        assert_eq!(view.active_tab_for_workspace(&second_workspace_id), Some(0));
        assert_eq!(
            view.focused_pane_for_tab(&first_workspace_id, 1),
            Some(first_focused)
        );
        assert_eq!(
            view.focused_pane_for_tab(&second_workspace_id, 1),
            Some(second_focused)
        );
        assert!(view.tab_is_zoomed(&first_workspace_id, 1));
        assert!(!view.tab_is_zoomed(&second_workspace_id, 1));
    }

    #[test]
    fn reconcile_discards_deleted_workspaces_and_clamps_selection() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        state.active = Some(0);
        state.selected = 0;
        let removed_workspace_id = state.workspaces[1].id.clone();
        let removed_pane = state.workspaces[1].tabs[0].layout.focused();

        let mut view = ClientViewState::from_app_state(&state);
        view.active_workspace = Some(9);
        view.selected_workspace = 9;
        view.active_tabs.insert(removed_workspace_id.clone(), 7);
        view.focused_panes.insert(
            ClientTabViewKey::new(&removed_workspace_id, 1),
            removed_pane,
        );
        view.zoomed_tabs
            .insert(ClientTabViewKey::new(&removed_workspace_id, 1));

        state.workspaces.pop();
        view.reconcile(&state);

        assert_eq!(view.active_workspace, Some(0));
        assert_eq!(view.selected_workspace, 0);
        assert!(!view.active_tabs.contains_key(&removed_workspace_id));
        assert!(view
            .focused_panes
            .keys()
            .all(|key| key.workspace_id != removed_workspace_id));
        assert!(view
            .zoomed_tabs
            .iter()
            .all(|key| key.workspace_id != removed_workspace_id));
    }

    #[test]
    fn reconcile_keeps_empty_filtered_group_without_active_workspace() {
        let mut state = AppState::test_new();
        let mut workspace_group = crate::app::state::Group::default_group();
        workspace_group.id = "with-space".to_string();
        let mut empty_group = crate::app::state::Group::default_group();
        empty_group.id = "empty".to_string();
        state.groups = vec![workspace_group.clone(), empty_group];
        state.workspaces = vec![Workspace::test_new("one")];
        state.workspaces[0].group_id = workspace_group.id;
        state.active = Some(0);
        state.selected = 0;
        state.active_group = 0;
        state.group_filter_enabled = true;

        let mut view = ClientViewState::from_app_state(&state);
        view.active_group = 1;
        view.active_workspace = None;
        view.selected_workspace = 0;
        view.reconcile(&state);

        assert_eq!(view.active_group, 1);
        assert_eq!(view.active_workspace, None);
        assert_eq!(view.selected_workspace, 0);
    }

    #[test]
    fn native_diff_view_state_keeps_selection_client_local() {
        let session = crate::native_diff::parse_native_diff_session(
            "/repo",
            b"--- a/src/first.rs\n+++ b/src/first.rs\n@@ -1 +1 @@\n-old\n+new\n--- a/src/second.rs\n+++ b/src/second.rs\n@@ -1 +1 @@\n-old\n+new\n",
            b"",
        )
        .expect("parse native diff");
        let mut state = AppState::test_new();
        let mut workspace = Workspace::test_new("repo");
        workspace
            .create_native_diff_tab(session)
            .expect("create native diff tab");
        state.workspaces = vec![workspace];
        state.active = Some(0);

        let mut first_client = ClientViewState::from_app_state(&state);
        let second_client = ClientViewState::from_app_state(&state);
        apply_client_view_to_app_state(&mut state, &first_client);
        let pane_id = state.workspaces[0].focused_pane_id().expect("focused pane");
        let diff = state.workspaces[0]
            .pane_state_mut(pane_id)
            .and_then(|pane| pane.native_diff_mut())
            .expect("native diff pane");
        assert_eq!(
            diff.selected_path().as_deref(),
            Some(std::path::Path::new("src/first.rs"))
        );

        assert!(diff.select_visible_file_row(2));
        first_client = ClientViewState::from_app_state(&state);

        apply_client_view_to_app_state(&mut state, &second_client);
        let diff = state.workspaces[0]
            .pane_state(pane_id)
            .and_then(|pane| pane.native_diff())
            .expect("native diff pane");
        assert_eq!(
            diff.selected_path().as_deref(),
            Some(std::path::Path::new("src/first.rs"))
        );

        apply_client_view_to_app_state(&mut state, &first_client);
        let diff = state.workspaces[0]
            .pane_state(pane_id)
            .and_then(|pane| pane.native_diff())
            .expect("native diff pane");
        assert_eq!(
            diff.selected_path().as_deref(),
            Some(std::path::Path::new("src/second.rs"))
        );
    }

    #[test]
    fn settings_draft_state_is_client_local() {
        let mut state = AppState::test_new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let second_client = ClientViewState::from_app_state(&state);

        first_client.mode = Mode::Settings;
        first_client.settings.pending_sound_enabled = Some(false);
        first_client.settings.section = crate::app::state::SettingsSection::Sound;

        apply_client_view_to_app_state(&mut state, &first_client);
        assert_eq!(state.mode, Mode::Settings);
        assert_eq!(state.settings.pending_sound_enabled, Some(false));

        apply_client_view_to_app_state(&mut state, &second_client);
        assert_ne!(state.mode, Mode::Settings);
        assert_eq!(state.settings.pending_sound_enabled, None);
    }

    #[test]
    fn command_palette_state_is_client_local() {
        let mut state = AppState::test_new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let second_client = ClientViewState::from_app_state(&state);

        first_client.mode = Mode::CommandPalette;
        first_client.command_palette.query = "git".to_string();
        first_client.command_palette.selected = 3;

        apply_client_view_to_app_state(&mut state, &first_client);
        assert_eq!(state.mode, Mode::CommandPalette);
        assert_eq!(state.command_palette.query, "git");
        assert_eq!(state.command_palette.selected, 3);

        apply_client_view_to_app_state(&mut state, &second_client);
        assert_ne!(state.mode, Mode::CommandPalette);
        assert!(state.command_palette.query.is_empty());
        assert_eq!(state.command_palette.selected, 0);
    }

    #[test]
    fn agent_panel_scope_state_is_client_local() {
        let mut state = AppState::test_new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let second_client = ClientViewState::from_app_state(&state);

        first_client.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        apply_client_view_to_app_state(&mut state, &first_client);
        assert_eq!(
            state.agent_panel_scope,
            crate::app::state::AgentPanelScope::AllWorkspaces
        );

        apply_client_view_to_app_state(&mut state, &second_client);
        assert_eq!(
            state.agent_panel_scope,
            crate::app::state::AgentPanelScope::CurrentWorkspace
        );

        apply_client_view_to_app_state(&mut state, &first_client);
        assert_eq!(
            state.agent_panel_scope,
            crate::app::state::AgentPanelScope::AllWorkspaces
        );
    }

    #[test]
    fn scoped_client_agent_panel_scope_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
            state.agent_panel_scroll = 9;
        });

        assert_eq!(
            first_client.agent_panel_scope,
            crate::app::state::AgentPanelScope::AllWorkspaces
        );
        assert_eq!(
            state.agent_panel_scope,
            crate::app::state::AgentPanelScope::CurrentWorkspace
        );

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert_eq!(
                state.agent_panel_scope,
                crate::app::state::AgentPanelScope::CurrentWorkspace
            );
            assert_eq!(state.agent_panel_scroll, 0);
        });
    }

    #[test]
    fn scoped_client_collapse_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.sidebar_collapsed = true;
            state.right_sidebar_collapsed = true;
            state.activity_agents_expanded = false;
            state.activity_commands_expanded = true;
            state.activity_ports_expanded = true;
            state.collapsed_agent_sections = vec!["agent:build".to_string()];
            state.collapsed_command_groups = vec!["commands".to_string()];
            state.collapsed_command_status_groups = vec!["running".to_string()];
            state.collapsed_workspace_groups = vec!["group-1".to_string()];
            state.workspace_scroll = 4;
            state.tab_scroll = 5;
            state.tab_scroll_follow_active = false;
            state.hovered_tab = Some(2);
            state.mobile_switcher_scroll = 6;
        });

        assert!(!state.sidebar_collapsed);
        assert!(!state.right_sidebar_collapsed);
        assert!(state.activity_agents_expanded);
        assert!(!state.activity_commands_expanded);
        assert!(!state.activity_ports_expanded);
        assert!(state.collapsed_agent_sections.is_empty());
        assert!(state.collapsed_command_groups.is_empty());
        assert!(state.collapsed_command_status_groups.is_empty());
        assert!(state.collapsed_workspace_groups.is_empty());
        assert_eq!(state.workspace_scroll, 0);
        assert_eq!(state.tab_scroll, 0);
        assert!(state.tab_scroll_follow_active);
        assert_eq!(state.hovered_tab, None);
        assert_eq!(state.mobile_switcher_scroll, 0);

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert!(!state.sidebar_collapsed);
            assert!(!state.right_sidebar_collapsed);
            assert!(state.activity_agents_expanded);
            assert!(!state.activity_commands_expanded);
            assert!(!state.activity_ports_expanded);
            assert!(state.collapsed_agent_sections.is_empty());
            assert!(state.collapsed_command_groups.is_empty());
            assert!(state.collapsed_command_status_groups.is_empty());
            assert!(state.collapsed_workspace_groups.is_empty());
            assert_eq!(state.workspace_scroll, 0);
            assert_eq!(state.tab_scroll, 0);
            assert!(state.tab_scroll_follow_active);
            assert_eq!(state.hovered_tab, None);
            assert_eq!(state.mobile_switcher_scroll, 0);
        });
    }

    #[test]
    fn scoped_client_menu_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.keybind_help.scroll = 7;
            state.global_menu.highlighted = 1;
            state.group_menu.highlighted = 2;
            state.agent_menu.highlighted = 4;
        });

        assert_eq!(first_client.keybind_help.scroll, 7);
        assert_eq!(first_client.global_menu.highlighted, 1);
        assert_eq!(first_client.group_menu.highlighted, 2);
        assert_eq!(first_client.agent_menu.highlighted, 4);
        assert_eq!(state.keybind_help.scroll, 0);
        assert_eq!(state.global_menu.highlighted, 0);
        assert_eq!(state.group_menu.highlighted, 0);
        assert_eq!(state.agent_menu.highlighted, 0);

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert_eq!(state.keybind_help.scroll, 0);
            assert_eq!(state.global_menu.highlighted, 0);
            assert_eq!(state.group_menu.highlighted, 0);
            assert_eq!(state.agent_menu.highlighted, 0);
        });
    }

    #[test]
    fn scoped_client_navigator_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.navigator.query = "db".to_string();
            state.navigator.selected = 3;
            state.navigator.scroll = 2;
            state.navigator.search_focused = true;
            state.navigator.state_filter = Some(crate::app::state::NavigatorStateFilter::Blocked);
            state
                .navigator
                .expanded_workspaces
                .insert("workspace-a".to_string());
        });

        assert_eq!(first_client.navigator.query, "db");
        assert_eq!(first_client.navigator.selected, 3);
        assert_eq!(first_client.navigator.scroll, 2);
        assert!(first_client.navigator.search_focused);
        assert_eq!(
            first_client.navigator.state_filter,
            Some(crate::app::state::NavigatorStateFilter::Blocked)
        );
        assert!(first_client
            .navigator
            .expanded_workspaces
            .contains("workspace-a"));
        assert!(state.navigator.query.is_empty());
        assert_eq!(state.navigator.selected, 0);
        assert_eq!(state.navigator.scroll, 0);
        assert!(!state.navigator.search_focused);
        assert_eq!(state.navigator.state_filter, None);
        assert!(state.navigator.expanded_workspaces.is_empty());

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert!(state.navigator.query.is_empty());
            assert_eq!(state.navigator.selected, 0);
            assert_eq!(state.navigator.scroll, 0);
            assert!(!state.navigator.search_focused);
            assert_eq!(state.navigator.state_filter, None);
            assert!(state.navigator.expanded_workspaces.is_empty());
        });
    }

    #[test]
    fn scoped_client_picker_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.agent_profile_picker.ws_idx = 2;
            state.agent_profile_picker.query = "cod".to_string();
            state.agent_profile_picker.selected = 3;
            state.agent_profile_picker.kind_filter = Some(crate::agent_profiles::AgentKind::Codex);
            state.agent_profile_picker.scroll = 4;
            state.diff_agent_picker = Some(crate::app::state::DiffAgentPickerState {
                ws_idx: 1,
                source_pane_id: PaneId::from_raw(7),
                payload: "diff payload".to_string(),
                selected: 2,
            });
            state.git_repo_picker.ws_idx = 3;
            state.git_repo_picker.roots = vec![std::path::PathBuf::from("/repo")];
            state.git_repo_picker.selected = 1;
            state.git_repo_picker.scroll = 5;
        });

        assert_eq!(first_client.agent_profile_picker.ws_idx, 2);
        assert_eq!(first_client.agent_profile_picker.query, "cod");
        assert_eq!(first_client.agent_profile_picker.selected, 3);
        assert_eq!(
            first_client.agent_profile_picker.kind_filter,
            Some(crate::agent_profiles::AgentKind::Codex)
        );
        assert_eq!(first_client.agent_profile_picker.scroll, 4);
        assert_eq!(
            first_client
                .diff_agent_picker
                .as_ref()
                .expect("diff agent picker")
                .selected,
            2
        );
        assert_eq!(first_client.git_repo_picker.ws_idx, 3);
        assert_eq!(
            first_client.git_repo_picker.roots,
            vec![std::path::PathBuf::from("/repo")]
        );
        assert_eq!(first_client.git_repo_picker.selected, 1);
        assert_eq!(first_client.git_repo_picker.scroll, 5);

        assert_eq!(state.agent_profile_picker.ws_idx, 0);
        assert!(state.agent_profile_picker.query.is_empty());
        assert_eq!(state.agent_profile_picker.selected, 0);
        assert_eq!(state.agent_profile_picker.kind_filter, None);
        assert_eq!(state.agent_profile_picker.scroll, 0);
        assert!(state.diff_agent_picker.is_none());
        assert_eq!(state.git_repo_picker.ws_idx, 0);
        assert!(state.git_repo_picker.roots.is_empty());
        assert_eq!(state.git_repo_picker.selected, 0);
        assert_eq!(state.git_repo_picker.scroll, 0);

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert_eq!(state.agent_profile_picker.ws_idx, 0);
            assert!(state.agent_profile_picker.query.is_empty());
            assert_eq!(state.agent_profile_picker.selected, 0);
            assert_eq!(state.agent_profile_picker.kind_filter, None);
            assert_eq!(state.agent_profile_picker.scroll, 0);
            assert!(state.diff_agent_picker.is_none());
            assert_eq!(state.git_repo_picker.ws_idx, 0);
            assert!(state.git_repo_picker.roots.is_empty());
            assert_eq!(state.git_repo_picker.selected, 0);
            assert_eq!(state.git_repo_picker.scroll, 0);
        });
    }

    #[test]
    fn scoped_client_context_menu_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.context_menu = Some(crate::app::state::ContextMenuState {
                kind: crate::app::state::ContextMenuKind::Pane {
                    ws_idx: 1,
                    pane_id: PaneId::from_raw(9),
                    has_manual_label: true,
                },
                x: 10,
                y: 11,
                list: crate::app::state::MenuListState::new(3),
            });
        });

        let first_menu = first_client.context_menu.as_ref().expect("context menu");
        assert_eq!(
            first_menu.kind,
            crate::app::state::ContextMenuKind::Pane {
                ws_idx: 1,
                pane_id: PaneId::from_raw(9),
                has_manual_label: true,
            }
        );
        assert_eq!(first_menu.x, 10);
        assert_eq!(first_menu.y, 11);
        assert_eq!(first_menu.list.highlighted, 3);
        assert!(state.context_menu.is_none());

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert!(state.context_menu.is_none());
        });
    }

    #[test]
    fn scoped_client_copy_selection_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.copy_mode = Some(crate::app::state::CopyModeState {
                pane_id: PaneId::from_raw(4),
                cursor_row: 2,
                cursor_col: 3,
                entry_offset_from_bottom: 5,
                selection: Some(crate::app::state::CopyModeSelection::Character),
            });
            state.selection = Some(crate::selection::Selection::anchor(
                PaneId::from_raw(6),
                1,
                2,
                None,
            ));
            state.selection_autoscroll = Some(crate::app::state::SelectionAutoscroll {
                direction: crate::app::state::SelectionAutoscrollDirection::Down,
                last_mouse_screen_col: 7,
                last_mouse_screen_row: 8,
                inner_rect: ratatui::layout::Rect::new(1, 2, 3, 4),
            });
        });

        let first_copy = first_client.copy_mode.expect("copy mode");
        assert_eq!(first_copy.pane_id, PaneId::from_raw(4));
        assert_eq!(first_copy.cursor_row, 2);
        assert_eq!(first_copy.cursor_col, 3);
        assert_eq!(first_copy.entry_offset_from_bottom, 5);
        assert_eq!(
            first_copy.selection,
            Some(crate::app::state::CopyModeSelection::Character)
        );
        assert_eq!(
            first_client
                .selection
                .as_ref()
                .map(|selection| selection.pane_id),
            Some(PaneId::from_raw(6))
        );
        let autoscroll = first_client
            .selection_autoscroll
            .as_ref()
            .expect("selection autoscroll");
        assert_eq!(
            autoscroll.direction,
            crate::app::state::SelectionAutoscrollDirection::Down
        );
        assert_eq!(autoscroll.last_mouse_screen_col, 7);
        assert_eq!(autoscroll.last_mouse_screen_row, 8);
        assert_eq!(
            autoscroll.inner_rect,
            ratatui::layout::Rect::new(1, 2, 3, 4)
        );

        assert!(state.copy_mode.is_none());
        assert!(state.selection.is_none());
        assert!(state.selection_autoscroll.is_none());

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert!(state.copy_mode.is_none());
            assert!(state.selection.is_none());
            assert!(state.selection_autoscroll.is_none());
        });
    }

    #[test]
    fn scoped_client_drag_press_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.drag = Some(crate::app::state::DragState {
                target: crate::app::state::DragTarget::TabReorder {
                    ws_idx: 1,
                    source_tab_idx: 2,
                    insert_idx: Some(3),
                },
            });
            state.workspace_press = Some(crate::app::state::WorkspacePressState {
                ws_idx: 4,
                start_col: 5,
                start_row: 6,
            });
            state.group_press = Some(crate::app::state::GroupPressState {
                group_idx: 7,
                start_col: 8,
                start_row: 9,
            });
            state.tab_press = Some(crate::app::state::TabPressState {
                ws_idx: 10,
                tab_idx: 11,
                start_col: 12,
                start_row: 13,
            });
            state.previous_pane_focus = Some(crate::app::state::PaneFocusTarget {
                workspace_id: "workspace-a".to_string(),
                pane_id: PaneId::from_raw(14),
            });
            state.right_click_passthrough = Some(crate::app::state::RightClickPassthroughGesture {
                pane_info: crate::layout::PaneInfo {
                    id: PaneId::from_raw(15),
                    rect: ratatui::layout::Rect::new(1, 2, 10, 6),
                    inner_rect: ratatui::layout::Rect::new(2, 3, 8, 4),
                    scrollbar_rect: None,
                    is_focused: true,
                },
                modifiers: crossterm::event::KeyModifiers::SHIFT,
            });
        });

        let drag = first_client.drag.as_ref().expect("drag");
        assert!(matches!(
            drag.target,
            crate::app::state::DragTarget::TabReorder {
                ws_idx: 1,
                source_tab_idx: 2,
                insert_idx: Some(3)
            }
        ));
        let workspace_press = first_client
            .workspace_press
            .as_ref()
            .expect("workspace press");
        assert_eq!(workspace_press.ws_idx, 4);
        assert_eq!(workspace_press.start_col, 5);
        assert_eq!(workspace_press.start_row, 6);
        let group_press = first_client.group_press.as_ref().expect("group press");
        assert_eq!(group_press.group_idx, 7);
        assert_eq!(group_press.start_col, 8);
        assert_eq!(group_press.start_row, 9);
        let tab_press = first_client.tab_press.as_ref().expect("tab press");
        assert_eq!(tab_press.ws_idx, 10);
        assert_eq!(tab_press.tab_idx, 11);
        assert_eq!(tab_press.start_col, 12);
        assert_eq!(tab_press.start_row, 13);
        assert_eq!(
            first_client
                .previous_pane_focus
                .as_ref()
                .map(|target| (target.workspace_id.as_str(), target.pane_id)),
            Some(("workspace-a", PaneId::from_raw(14)))
        );
        let passthrough = first_client
            .right_click_passthrough
            .as_ref()
            .expect("right click passthrough");
        assert_eq!(passthrough.pane_info.id, PaneId::from_raw(15));
        assert_eq!(passthrough.modifiers, crossterm::event::KeyModifiers::SHIFT);

        assert!(state.drag.is_none());
        assert!(state.workspace_press.is_none());
        assert!(state.group_press.is_none());
        assert!(state.tab_press.is_none());
        assert!(state.previous_pane_focus.is_none());
        assert!(state.right_click_passthrough.is_none());

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert!(state.drag.is_none());
            assert!(state.workspace_press.is_none());
            assert!(state.group_press.is_none());
            assert!(state.tab_press.is_none());
            assert!(state.previous_pane_focus.is_none());
            assert!(state.right_click_passthrough.is_none());
        });
    }

    #[test]
    fn scoped_client_modal_draft_state_does_not_rewrite_shared_view() {
        let mut state = AppState::test_new();
        let runtimes = TerminalRuntimeRegistry::new();
        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);

        with_client_view_app_state(&mut state, &runtimes, &mut first_client, |state| {
            state.creating_new_tab = true;
            state.creating_new_group = true;
            state.group_icon_input = "★".to_string();
            state.group_default_directory_input = "/tmp/group".to_string();
            state.group_modal_selected_field = 2;
            state.group_icon_picker_open = true;
            state.rename_group_target = Some(3);
            state.requested_new_tab_name = Some("api".to_string());
            state.rename_pane_target = Some(PaneId::from_raw(4));
            state.confirm_delete_group = Some(5);
            state.name_input = "draft name".to_string();
            state.name_input_replace_on_type = true;
            state.release_notes = Some(crate::app::state::ReleaseNotesState {
                version: "0.2.0".to_string(),
                body: "notes".to_string(),
                scroll: 6,
                preview: true,
            });
            state.product_announcement = Some(crate::app::state::ProductAnnouncementState {
                version: "0.2.0".to_string(),
                id: "announcement".to_string(),
                title: "Title".to_string(),
                body: "Body".to_string(),
                scroll: 7,
                preview: true,
            });
        });

        assert!(first_client.creating_new_tab);
        assert!(first_client.creating_new_group);
        assert_eq!(first_client.group_icon_input, "★");
        assert_eq!(first_client.group_default_directory_input, "/tmp/group");
        assert_eq!(first_client.group_modal_selected_field, 2);
        assert!(first_client.group_icon_picker_open);
        assert_eq!(first_client.rename_group_target, Some(3));
        assert_eq!(first_client.requested_new_tab_name.as_deref(), Some("api"));
        assert_eq!(first_client.rename_pane_target, Some(PaneId::from_raw(4)));
        assert_eq!(first_client.confirm_delete_group, Some(5));
        assert_eq!(first_client.name_input, "draft name");
        assert!(first_client.name_input_replace_on_type);
        assert_eq!(
            first_client.release_notes.as_ref().map(|notes| (
                notes.version.as_str(),
                notes.scroll,
                notes.preview
            )),
            Some(("0.2.0", 6, true))
        );
        assert_eq!(
            first_client
                .product_announcement
                .as_ref()
                .map(|announcement| {
                    (
                        announcement.version.as_str(),
                        announcement.id.as_str(),
                        announcement.scroll,
                        announcement.preview,
                    )
                }),
            Some(("0.2.0", "announcement", 7, true))
        );

        assert!(!state.creating_new_tab);
        assert!(!state.creating_new_group);
        assert_eq!(
            state.group_icon_input,
            crate::app::state::DEFAULT_GROUP_ICON
        );
        assert!(state.group_default_directory_input.is_empty());
        assert_eq!(state.group_modal_selected_field, 0);
        assert!(!state.group_icon_picker_open);
        assert_eq!(state.rename_group_target, None);
        assert_eq!(state.requested_new_tab_name, None);
        assert_eq!(state.rename_pane_target, None);
        assert_eq!(state.confirm_delete_group, None);
        assert!(state.name_input.is_empty());
        assert!(!state.name_input_replace_on_type);
        assert!(state.release_notes.is_none());
        assert!(state.product_announcement.is_none());

        with_client_view_app_state(&mut state, &runtimes, &mut second_client, |state| {
            assert!(!state.creating_new_tab);
            assert!(!state.creating_new_group);
            assert_eq!(
                state.group_icon_input,
                crate::app::state::DEFAULT_GROUP_ICON
            );
            assert!(state.group_default_directory_input.is_empty());
            assert_eq!(state.group_modal_selected_field, 0);
            assert!(!state.group_icon_picker_open);
            assert_eq!(state.rename_group_target, None);
            assert_eq!(state.requested_new_tab_name, None);
            assert_eq!(state.rename_pane_target, None);
            assert_eq!(state.confirm_delete_group, None);
            assert!(state.name_input.is_empty());
            assert!(!state.name_input_replace_on_type);
            assert!(state.release_notes.is_none());
            assert!(state.product_announcement.is_none());
        });
    }
    #[tokio::test]
    async fn terminal_scroll_offset_state_is_client_local() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("terminal")];
        state.active = Some(0);
        let pane_id = state.workspaces[0].focused_pane_id().expect("focused pane");
        let terminal_id = state.workspaces[0]
            .pane_state(pane_id)
            .and_then(|pane| pane.terminal_id_cloned())
            .expect("terminal id");
        let mut runtimes = TerminalRuntimeRegistry::new();
        runtimes.insert(
            terminal_id.clone(),
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                12,
                3,
                10_000,
                b"one\ntwo\nthree\nfour\nfive\nsix\n",
            ),
        );

        let mut first_client = ClientViewState::from_app_state(&state);
        let mut second_client = ClientViewState::from_app_state(&state);
        capture_terminal_offsets_from_app_state(&state, &runtimes, &mut first_client);
        capture_terminal_offsets_from_app_state(&state, &runtimes, &mut second_client);
        assert_eq!(
            second_client
                .terminal_offsets_from_bottom
                .get(&terminal_id)
                .copied(),
            Some(0)
        );

        runtimes.get(&terminal_id).expect("runtime").scroll_up(2);
        capture_terminal_offsets_from_app_state(&state, &runtimes, &mut first_client);
        let first_offset = first_client
            .terminal_offsets_from_bottom
            .get(&terminal_id)
            .copied()
            .expect("first client terminal offset");
        assert!(first_offset > 0);

        apply_terminal_offsets_to_runtimes(&state, &runtimes, &second_client);
        assert_eq!(
            runtimes
                .get(&terminal_id)
                .and_then(|runtime| runtime.scroll_metrics())
                .map(|metrics| metrics.offset_from_bottom),
            Some(0)
        );

        apply_terminal_offsets_to_runtimes(&state, &runtimes, &first_client);
        assert_eq!(
            runtimes
                .get(&terminal_id)
                .and_then(|runtime| runtime.scroll_metrics())
                .map(|metrics| metrics.offset_from_bottom),
            Some(first_offset)
        );
    }
}
