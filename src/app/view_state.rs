use std::collections::{HashMap, HashSet};

use crate::app::state::{
    AgentProfilePickerState, AppState, CommandPaletteState, ContextMenuState, CopyModeState,
    DragState, GitRepoPickerState, GroupPressState, KeybindHelpState, MenuListState, Mode,
    NavigatorState, PaneFocusTarget, ProductAnnouncementState, ReleaseNotesState,
    RightClickPassthroughGesture, SelectionAutoscroll, SettingsState, TabPressState, ViewState,
    WorkspacePressState,
};
use crate::layout::PaneId;
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
    pub(crate) sidebar_width: u16,
    pub(crate) sidebar_width_source: crate::app::state::SidebarWidthSource,
    pub(crate) sidebar_width_auto: bool,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) right_sidebar_collapsed: bool,
    pub(crate) right_sidebar_width: u16,
    pub(crate) sidebar_section_split: f32,
    pub(crate) activity_agents_expanded: bool,
    pub(crate) activity_commands_expanded: bool,
    pub(crate) activity_ports_expanded: bool,
    pub(crate) collapsed_agent_sections: Vec<String>,
    pub(crate) collapsed_command_groups: Vec<String>,
    pub(crate) collapsed_command_status_groups: Vec<String>,
    pub(crate) collapsed_workspace_groups: Vec<String>,
    pub(crate) mode: Mode,
    pub(crate) active_tabs: HashMap<String, usize>,
    pub(crate) pending_active_tabs: HashMap<String, usize>,
    pub(crate) focused_panes: HashMap<ClientTabViewKey, PaneId>,
    pub(crate) zoomed_tabs: HashSet<ClientTabViewKey>,
    pub(crate) terminal_offsets_from_bottom: HashMap<TerminalId, usize>,
    pub(crate) settings: SettingsState,
    pub(crate) command_palette: CommandPaletteState,
    pub(crate) navigator: NavigatorState,
    pub(crate) agent_profile_picker: AgentProfilePickerState,
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
            sidebar_width: state.sidebar_width,
            sidebar_width_source: state.sidebar_width_source,
            sidebar_width_auto: state.sidebar_width_auto,
            sidebar_collapsed: state.sidebar_collapsed,
            right_sidebar_collapsed: state.right_sidebar_collapsed,
            right_sidebar_width: state.right_sidebar_width,
            sidebar_section_split: state.sidebar_section_split,
            activity_agents_expanded: state.activity_agents_expanded,
            activity_commands_expanded: state.activity_commands_expanded,
            activity_ports_expanded: state.activity_ports_expanded,
            collapsed_agent_sections: state.collapsed_agent_sections.clone(),
            collapsed_command_groups: state.collapsed_command_groups.clone(),
            collapsed_command_status_groups: state.collapsed_command_status_groups.clone(),
            collapsed_workspace_groups: state.collapsed_workspace_groups.clone(),
            mode: state.mode,
            active_tabs: HashMap::new(),
            pending_active_tabs: HashMap::new(),
            focused_panes: HashMap::new(),
            zoomed_tabs: HashSet::new(),
            terminal_offsets_from_bottom: HashMap::new(),
            settings: state.settings.clone(),
            command_palette: state.command_palette.clone(),
            navigator: state.navigator.clone(),
            agent_profile_picker: state.agent_profile_picker.clone(),
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
            global_menu: state.global_menu,
            group_menu: state.group_menu,
            agent_menu: state.agent_menu,
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

    pub(crate) fn clone_reconciled(&self, state: &AppState) -> Self {
        let mut view = self.clone();
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
                state
                    .active
                    .filter(|idx| *idx < state.workspaces.len())
                    .or_else(first_visible_workspace)
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
        self.pending_active_tabs
            .retain(|workspace_id, _| valid_workspace_ids.contains(workspace_id.as_str()));
        self.focused_panes
            .retain(|key, _| valid_workspace_ids.contains(key.workspace_id.as_str()));
        self.zoomed_tabs
            .retain(|key| valid_workspace_ids.contains(key.workspace_id.as_str()));

        for workspace in &state.workspaces {
            if workspace.tabs.is_empty() {
                self.active_tabs.remove(&workspace.id);
                self.pending_active_tabs.remove(&workspace.id);
                self.focused_panes
                    .retain(|key, _| key.workspace_id != workspace.id);
                self.zoomed_tabs
                    .retain(|key| key.workspace_id != workspace.id);
                continue;
            }

            let pending_active_tab = self.pending_active_tabs.get(&workspace.id).copied();
            let active_tab = if let Some(tab_idx) = pending_active_tab {
                if tab_idx < workspace.tabs.len() {
                    self.pending_active_tabs.remove(&workspace.id);
                    tab_idx
                } else {
                    self.active_tabs
                        .get(&workspace.id)
                        .copied()
                        .filter(|idx| *idx < workspace.tabs.len())
                        .unwrap_or_else(|| workspace.active_tab.min(workspace.tabs.len() - 1))
                }
            } else {
                self.active_tabs
                    .get(&workspace.id)
                    .copied()
                    .filter(|idx| *idx < workspace.tabs.len())
                    .unwrap_or_else(|| workspace.active_tab.min(workspace.tabs.len() - 1))
            };
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
            }

            let tab_count = workspace.tabs.len();
            self.focused_panes.retain(|key, _| {
                key.workspace_id != workspace.id || (1..=tab_count).contains(&key.tab_number)
            });
            self.zoomed_tabs.retain(|key| {
                key.workspace_id != workspace.id || (1..=tab_count).contains(&key.tab_number)
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

    pub(crate) fn active_tab_index_for_workspace(
        &self,
        state: &AppState,
        ws_idx: usize,
    ) -> Option<usize> {
        let workspace = state.workspaces.get(ws_idx)?;
        self.active_tab_for_workspace(&workspace.id)
            .filter(|idx| *idx < workspace.tabs.len())
    }

    pub(crate) fn focused_pane_for_workspace(
        &self,
        state: &AppState,
        ws_idx: usize,
    ) -> Option<(usize, PaneId)> {
        let workspace = state.workspaces.get(ws_idx)?;
        let tab_idx = self.active_tab_index_for_workspace(state, ws_idx)?;
        let pane_id = self.focused_pane_for_tab(&workspace.id, tab_idx + 1)?;
        workspace
            .tabs
            .get(tab_idx)?
            .panes
            .contains_key(&pane_id)
            .then_some((tab_idx, pane_id))
    }

    pub(crate) fn current_pane_focus_target(&self, state: &AppState) -> Option<PaneFocusTarget> {
        let ws_idx = self.active_workspace?;
        let workspace = state.workspaces.get(ws_idx)?;
        let (_, pane_id) = self.focused_pane_for_workspace(state, ws_idx)?;
        Some(PaneFocusTarget {
            workspace_id: workspace.id.clone(),
            pane_id,
        })
    }

    pub(crate) fn focus_pane_in_workspace(
        &mut self,
        state: &AppState,
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    ) -> bool {
        let Some(workspace) = state.workspaces.get(ws_idx) else {
            return false;
        };
        let Some(tab) = workspace.tabs.get(tab_idx) else {
            return false;
        };
        if !tab.panes.contains_key(&pane_id) {
            return false;
        }

        let previous = self.current_pane_focus_target(state);
        let target = PaneFocusTarget {
            workspace_id: workspace.id.clone(),
            pane_id,
        };
        if previous.as_ref() == Some(&target) {
            return false;
        }

        let workspace_changed = self.active_workspace != Some(ws_idx);
        self.active_workspace = Some(ws_idx);
        self.selected_workspace = ws_idx;
        if let Some(group_idx) = state
            .groups
            .iter()
            .position(|group| group.id == workspace.group_id)
        {
            self.active_group = group_idx;
        }
        self.active_tabs.insert(workspace.id.clone(), tab_idx);
        self.focused_panes
            .insert(ClientTabViewKey::new(&workspace.id, tab_idx + 1), pane_id);
        self.mode = Mode::Terminal;
        self.selection = None;
        self.selection_autoscroll = None;
        self.tab_scroll_follow_active = true;
        if workspace_changed
            && matches!(
                self.agent_panel_scope,
                crate::app::state::AgentPanelScope::CurrentWorkspace
            )
        {
            self.agent_panel_scroll = 0;
        }
        self.previous_pane_focus = previous;
        true
    }

    pub(crate) fn tab_is_zoomed(&self, workspace_id: &str, tab_number: usize) -> bool {
        self.zoomed_tabs
            .contains(&ClientTabViewKey::new(workspace_id, tab_number))
    }

    pub(crate) fn set_tab_zoomed(&mut self, workspace_id: &str, tab_number: usize, zoomed: bool) {
        let key = ClientTabViewKey::new(workspace_id, tab_number);
        if zoomed {
            self.zoomed_tabs.insert(key);
        } else {
            self.zoomed_tabs.remove(&key);
        }
    }
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
    state.sidebar_width = view.sidebar_width;
    state.sidebar_width_source = view.sidebar_width_source;
    state.sidebar_width_auto = view.sidebar_width_auto;
    state.sidebar_collapsed = view.sidebar_collapsed;
    state.right_sidebar_collapsed = view.right_sidebar_collapsed;
    state.right_sidebar_width = view.right_sidebar_width;
    state.sidebar_section_split = view.sidebar_section_split;
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
    state.global_menu = view.global_menu;
    state.group_menu = view.group_menu;
    state.agent_menu = view.agent_menu;
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
    fn reconcile_preserves_pending_future_tab_focus_until_tab_exists() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("shell")];
        state.workspaces[0].active_tab = 0;
        let workspace_id = state.workspaces[0].id.clone();

        let mut view = ClientViewState::from_app_state(&state);
        view.pending_active_tabs.insert(workspace_id.clone(), 1);
        view.reconcile(&state);

        assert_eq!(view.active_tab_for_workspace(&workspace_id), Some(0));
        assert_eq!(view.pending_active_tabs.get(&workspace_id), Some(&1));

        state.workspaces[0].test_add_tab(Some("diff"));
        state.workspaces[0].active_tab = 1;
        view.reconcile(&state);

        assert_eq!(view.active_tab_for_workspace(&workspace_id), Some(1));
        assert!(!view.pending_active_tabs.contains_key(&workspace_id));
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
