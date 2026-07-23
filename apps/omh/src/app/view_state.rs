use std::collections::{HashMap, HashSet};

static NEXT_CLIENT_VIEW_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

use crate::app::state::{
    AgentProfilePickerState, AppState, CommandPaletteState, ContextMenuState, DragState,
    GitRepoPickerState, GroupPressState, KeybindHelpState, ModalListState, Mode, NavigatorState,
    PaneFocusTarget, ProductAnnouncementState, ReleaseNotesState, RightClickPassthroughGesture,
    SelectionAutoscroll, SettingsState, TabPressState, ViewState, WorkspacePressState,
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

#[derive(Clone)]
struct ClientOverlayReturnState {
    tab: ClientTabViewKey,
    focused_pane: PaneId,
    zoomed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalViewportOffset {
    pub(crate) offset_from_bottom: usize,
    pub(crate) max_offset_from_bottom: usize,
}

impl TerminalViewportOffset {
    fn from_metrics(metrics: crate::pane::ScrollMetrics) -> Self {
        Self {
            offset_from_bottom: metrics.offset_from_bottom,
            max_offset_from_bottom: metrics.max_offset_from_bottom,
        }
    }

    fn for_metrics(self, metrics: crate::pane::ScrollMetrics) -> usize {
        if self.offset_from_bottom == 0 {
            return 0;
        }
        self.offset_from_bottom
            .saturating_add(
                metrics
                    .max_offset_from_bottom
                    .saturating_sub(self.max_offset_from_bottom),
            )
            .min(metrics.max_offset_from_bottom)
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
    id: u64,
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
    pub(crate) collapsed_sidebar_hover: Option<crate::app::state::CollapsedSidebarHover>,
    pub(crate) mobile_switcher_scroll: usize,
    pub(crate) mobile_switcher_level: crate::app::state::MobileSwitcherLevel,
    pub(crate) mobile_switcher_selected: usize,
    pub(crate) sidebar_width: u16,
    pub(crate) sidebar_width_source: crate::app::state::SidebarWidthSource,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) right_sidebar_collapsed: bool,
    pub(crate) context_bar_visibility_override: Option<bool>,
    pub(crate) zen_mode: bool,
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
    overlay_return_states: HashMap<PaneId, ClientOverlayReturnState>,
    pub(crate) terminal_offsets_from_bottom: HashMap<TerminalId, TerminalViewportOffset>,
    pub(crate) suppressed_repeat_keys: HashSet<crossterm::event::KeyCode>,
    pub(crate) forwarded_terminal_keys:
        HashMap<crossterm::event::KeyCode, crate::app::input::TerminalKeyTarget>,
    pub(crate) settings: SettingsState,
    pub(crate) command_palette: CommandPaletteState,
    pub(crate) navigator: NavigatorState,
    pub(crate) agent_profile_picker: AgentProfilePickerState,
    pub(crate) git_repo_picker: GitRepoPickerState,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) selection: Option<crate::selection::Selection>,
    pub(crate) selection_autoscroll: Option<SelectionAutoscroll>,
    pub(crate) last_pane_click: Option<crate::app::PaneClickState>,
    pub(crate) selection_highlight_clear_deadline: Option<std::time::Instant>,
    pub(crate) copy_mode: Option<crate::app::state::CopyModeState>,
    pub(crate) drag: Option<DragState>,
    pub(crate) workspace_press: Option<WorkspacePressState>,
    pub(crate) group_press: Option<GroupPressState>,
    pub(crate) tab_press: Option<TabPressState>,
    pub(crate) previous_pane_focus: Option<PaneFocusTarget>,
    pub(crate) right_click_passthrough: Option<RightClickPassthroughGesture>,
    pub(crate) keybind_help: KeybindHelpState,
    pub(crate) config_diagnostics_scroll: u16,
    pub(crate) global_menu: ModalListState,
    pub(crate) group_menu: ModalListState,
    pub(crate) agent_menu: ModalListState,
    pub(crate) creating_new_tab: bool,
    pub(crate) creating_new_group: bool,
    pub(crate) group_icon_input: String,
    pub(crate) group_default_directory_input: String,
    pub(crate) group_modal_selected_field: usize,
    pub(crate) group_icon_picker_open: bool,
    pub(crate) rename_group_target: Option<usize>,
    pub(crate) requested_new_tab_name: Option<String>,
    pub(crate) pending_workspace_create_cwd: Option<std::path::PathBuf>,
    pub(crate) pending_workspace_create_group: Option<usize>,
    pub(crate) rename_pane_target: Option<PaneId>,
    pub(crate) confirm_delete_group: Option<usize>,
    pub(crate) name_input: String,
    pub(crate) name_input_replace_on_type: bool,
    pub(crate) release_notes: Option<ReleaseNotesState>,
    pub(crate) product_announcement: Option<ProductAnnouncementState>,
    pub(crate) computed: ViewState,
}

impl ClientViewState {
    pub(crate) fn from_default_client_state(state: &AppState) -> Self {
        let mut view = Self {
            id: NEXT_CLIENT_VIEW_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
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
            collapsed_sidebar_hover: state.collapsed_sidebar_hover,
            mobile_switcher_scroll: state.mobile_switcher_scroll,
            mobile_switcher_level: state.mobile_switcher_level,
            mobile_switcher_selected: state.mobile_switcher_selected,
            sidebar_width: state.sidebar_width,
            sidebar_width_source: state.sidebar_width_source,
            sidebar_collapsed: state.sidebar_collapsed,
            right_sidebar_collapsed: state.right_sidebar_collapsed,
            context_bar_visibility_override: None,
            zen_mode: false,
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
            overlay_return_states: HashMap::new(),
            terminal_offsets_from_bottom: HashMap::new(),
            settings: state.settings.clone(),
            command_palette: state.command_palette.clone(),
            navigator: state.navigator.clone(),
            agent_profile_picker: state.agent_profile_picker.clone(),
            git_repo_picker: state.git_repo_picker.clone(),
            context_menu: state.context_menu.clone(),
            selection: state.selection.clone(),
            selection_autoscroll: state.selection_autoscroll.clone(),
            last_pane_click: None,
            selection_highlight_clear_deadline: None,
            copy_mode: state.copy_mode.clone(),
            drag: state.drag.clone(),
            workspace_press: state.workspace_press.clone(),
            group_press: state.group_press.clone(),
            tab_press: state.tab_press.clone(),
            previous_pane_focus: state.previous_pane_focus.clone(),
            right_click_passthrough: state.right_click_passthrough.clone(),
            keybind_help: state.keybind_help.clone(),
            config_diagnostics_scroll: state.config_diagnostics_scroll,
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
            pending_workspace_create_cwd: state.pending_workspace_create_cwd.clone(),
            pending_workspace_create_group: None,
            rename_pane_target: state.rename_pane_target,
            confirm_delete_group: state.confirm_delete_group,
            name_input: state.name_input.clone(),
            name_input_replace_on_type: state.name_input_replace_on_type,
            release_notes: state.release_notes.clone(),
            product_announcement: state.product_announcement.clone(),
            computed: state.view.clone(),
            suppressed_repeat_keys: HashSet::new(),
            forwarded_terminal_keys: HashMap::new(),
        };
        view.reconcile(state);
        view
    }

    pub(crate) fn clone_reconciled(&self, state: &AppState) -> Self {
        let mut view = self.clone();
        view.reconcile(state);
        view
    }

    pub(crate) fn for_new_client(state: &AppState) -> Self {
        let mut view = Self::from_default_client_state(state);
        let sidebar_collapsed = matches!(
            state.sidebar_config.initial_state,
            crate::config::SidebarInitialStateConfig::Collapsed
        );
        view.group_filter_enabled = false;
        view.agent_panel_scope =
            super::agent_panel_scope_from_config(state.sidebar_config.initial_agent_scope);
        view.sidebar_collapsed = sidebar_collapsed;
        view.right_sidebar_collapsed = sidebar_collapsed;
        view.agent_panel_scroll = 0;
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
            self.overlay_return_states.clear();
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
        self.focused_panes.retain(|key, pane_id| {
            valid_workspace_ids.contains(key.workspace_id.as_str())
                && state
                    .client_overlay_owners
                    .get(pane_id)
                    .is_none_or(|owner| *owner == self.id)
        });
        self.zoomed_tabs
            .retain(|key| valid_workspace_ids.contains(key.workspace_id.as_str()));

        loop {
            let missing_overlay = self
                .overlay_return_states
                .keys()
                .copied()
                .find(|overlay_pane| {
                    !state.workspaces.iter().any(|workspace| {
                        workspace
                            .tabs
                            .iter()
                            .any(|tab| tab.panes.contains_key(overlay_pane))
                    })
                });
            let Some(overlay_pane) = missing_overlay else {
                break;
            };
            let Some(return_state) = self.overlay_return_states.remove(&overlay_pane) else {
                continue;
            };

            let mut promoted = false;
            for child_return in self.overlay_return_states.values_mut() {
                if child_return.focused_pane == overlay_pane {
                    child_return.focused_pane = return_state.focused_pane;
                    child_return.zoomed = return_state.zoomed;
                    promoted = true;
                }
            }
            if promoted {
                continue;
            }

            let Some((ws_idx, workspace)) = state
                .workspaces
                .iter()
                .enumerate()
                .find(|(_, workspace)| workspace.id == return_state.tab.workspace_id)
            else {
                continue;
            };
            let Some(tab_idx) = return_state.tab.tab_number.checked_sub(1) else {
                continue;
            };
            let Some(tab) = workspace.tabs.get(tab_idx) else {
                continue;
            };
            if !tab.panes.contains_key(&return_state.focused_pane) {
                continue;
            }

            self.active_workspace = Some(ws_idx);
            self.selected_workspace = ws_idx;
            self.active_tabs.insert(workspace.id.clone(), tab_idx);
            self.focused_panes
                .insert(return_state.tab.clone(), return_state.focused_pane);
            if return_state.zoomed {
                self.zoomed_tabs.insert(return_state.tab.clone());
            } else {
                self.zoomed_tabs.remove(&return_state.tab);
            }
        }
        self.overlay_return_states.retain(|_, return_state| {
            let Some(workspace) = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == return_state.tab.workspace_id)
            else {
                return false;
            };
            let Some(tab_idx) = return_state.tab.tab_number.checked_sub(1) else {
                return false;
            };
            workspace
                .tabs
                .get(tab_idx)
                .is_some_and(|tab| tab.panes.contains_key(&return_state.focused_pane))
        });

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
        if state
            .client_overlay_owners
            .get(&pane_id)
            .is_some_and(|owner| *owner != self.id)
        {
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

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub(crate) fn focus_client_overlay(
        &mut self,
        state: &AppState,
        ws_idx: usize,
        tab_idx: usize,
        overlay_pane: PaneId,
    ) -> bool {
        let Some(workspace) = state.workspaces.get(ws_idx) else {
            return false;
        };
        let Some(tab) = workspace.tabs.get(tab_idx) else {
            return false;
        };
        if !tab.panes.contains_key(&overlay_pane) {
            return false;
        }

        let tab_key = ClientTabViewKey::new(&workspace.id, tab_idx + 1);
        let focused_pane = self
            .focused_panes
            .get(&tab_key)
            .copied()
            .unwrap_or_else(|| tab.layout.focused());
        if !tab.panes.contains_key(&focused_pane) {
            return false;
        }
        self.overlay_return_states.insert(
            overlay_pane,
            ClientOverlayReturnState {
                tab: tab_key.clone(),
                focused_pane,
                zoomed: self.zoomed_tabs.contains(&tab_key),
            },
        );
        self.focus_pane_in_workspace(state, ws_idx, tab_idx, overlay_pane);
        self.zoomed_tabs.insert(tab_key);
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

    pub(crate) fn screen_rect(&self) -> ratatui::layout::Rect {
        let sidebar = self.computed.sidebar_rect;
        let right_sidebar = self.computed.right_sidebar_rect;
        let terminal = self.computed.terminal_area;
        let mobile_header = self.computed.mobile_header_rect;
        let context_bar = self.computed.context_bar.rect;
        let x = sidebar
            .x
            .min(right_sidebar.x)
            .min(terminal.x)
            .min(mobile_header.x)
            .min(context_bar.x);
        let y = sidebar
            .y
            .min(right_sidebar.y)
            .min(terminal.y)
            .min(mobile_header.y)
            .min(context_bar.y);
        let right = (sidebar.x + sidebar.width)
            .max(right_sidebar.x + right_sidebar.width)
            .max(terminal.x + terminal.width)
            .max(mobile_header.x + mobile_header.width)
            .max(context_bar.x + context_bar.width);
        let bottom = (sidebar.y + sidebar.height)
            .max(right_sidebar.y + right_sidebar.height)
            .max(terminal.y + terminal.height)
            .max(mobile_header.y + mobile_header.height)
            .max(context_bar.y + context_bar.height);
        ratatui::layout::Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }

    pub(crate) fn clear_due_selection_highlight(&mut self, now: std::time::Instant) -> bool {
        if self
            .selection_highlight_clear_deadline
            .is_none_or(|deadline| now < deadline)
        {
            return false;
        }

        self.selection_highlight_clear_deadline = None;
        if self
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_in_progress())
        {
            self.selection = None;
            self.selection_autoscroll = None;
            return true;
        }
        false
    }

    pub(crate) fn return_to_active_workspace_mode(&mut self) {
        self.mode = if self.active_workspace.is_some() {
            Mode::Terminal
        } else {
            Mode::Navigate
        };
    }
}

#[cfg(test)]
pub(crate) fn capture_terminal_offset_from_runtimes(
    terminal_id: &TerminalId,
    runtimes: &TerminalRuntimeRegistry,
    view: &mut ClientViewState,
) {
    let Some(metrics) = runtimes
        .get(terminal_id)
        .and_then(|runtime| runtime.scroll_metrics())
    else {
        return;
    };
    view.terminal_offsets_from_bottom.insert(
        terminal_id.clone(),
        TerminalViewportOffset::from_metrics(metrics),
    );
}

pub(crate) fn set_terminal_offset_from_bottom(
    terminal_id: &TerminalId,
    metrics: crate::pane::ScrollMetrics,
    offset_from_bottom: usize,
    view: &mut ClientViewState,
) {
    view.terminal_offsets_from_bottom.insert(
        terminal_id.clone(),
        TerminalViewportOffset {
            offset_from_bottom: offset_from_bottom.min(metrics.max_offset_from_bottom),
            max_offset_from_bottom: metrics.max_offset_from_bottom,
        },
    );
}

pub(crate) fn terminal_offset_from_bottom(
    terminal_id: &TerminalId,
    metrics: crate::pane::ScrollMetrics,
    view: &ClientViewState,
) -> usize {
    view.terminal_offsets_from_bottom
        .get(terminal_id)
        .copied()
        .map(|offset| offset.for_metrics(metrics))
        .unwrap_or(metrics.offset_from_bottom)
}

pub(crate) fn capture_terminal_offsets_from_runtimes(
    live_terminal_ids: &[TerminalId],
    runtimes: &TerminalRuntimeRegistry,
    view: &mut ClientViewState,
) {
    let live_terminal_ids = live_terminal_ids.iter().collect::<HashSet<_>>();
    for terminal_id in &live_terminal_ids {
        let Some(metrics) = runtimes
            .get(terminal_id)
            .and_then(|runtime| runtime.scroll_metrics())
        else {
            continue;
        };
        view.terminal_offsets_from_bottom.insert(
            (*terminal_id).clone(),
            TerminalViewportOffset::from_metrics(metrics),
        );
    }
    view.terminal_offsets_from_bottom
        .retain(|terminal_id, _| live_terminal_ids.contains(terminal_id));
}

pub(crate) fn apply_terminal_offsets_to_runtimes(
    live_terminal_ids: &[TerminalId],
    runtimes: &TerminalRuntimeRegistry,
    view: &ClientViewState,
) {
    for terminal_id in live_terminal_ids {
        let Some(offset) = view.terminal_offsets_from_bottom.get(terminal_id) else {
            continue;
        };
        let Some(runtime) = runtimes.get(terminal_id) else {
            continue;
        };
        let Some(metrics) = runtime.scroll_metrics() else {
            continue;
        };
        runtime.set_scroll_offset_from_bottom(offset.for_metrics(metrics));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    #[test]
    fn default_view_matches_current_empty_app_state() {
        let state = AppState::test_new();

        let view = ClientViewState::from_default_client_state(&state);

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

        let view = ClientViewState::from_default_client_state(&state);

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

        let mut view = ClientViewState::from_default_client_state(&state);
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

        let mut view = ClientViewState::from_default_client_state(&state);
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

        let mut view = ClientViewState::from_default_client_state(&state);
        view.active_group = 1;
        view.active_workspace = None;
        view.selected_workspace = 0;
        view.reconcile(&state);

        assert_eq!(view.active_group, 1);
        assert_eq!(view.active_workspace, None);
        assert_eq!(view.selected_workspace, 0);
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
        let live_terminal_ids = vec![terminal_id.clone()];

        let mut first_client = ClientViewState::from_default_client_state(&state);
        let mut second_client = ClientViewState::from_default_client_state(&state);
        capture_terminal_offsets_from_runtimes(&live_terminal_ids, &runtimes, &mut first_client);
        capture_terminal_offsets_from_runtimes(&live_terminal_ids, &runtimes, &mut second_client);
        assert_eq!(
            second_client
                .terminal_offsets_from_bottom
                .get(&terminal_id)
                .map(|offset| offset.offset_from_bottom),
            Some(0)
        );

        runtimes.get(&terminal_id).expect("runtime").scroll_up(2);
        capture_terminal_offsets_from_runtimes(&live_terminal_ids, &runtimes, &mut first_client);
        let first_offset = first_client
            .terminal_offsets_from_bottom
            .get(&terminal_id)
            .copied()
            .expect("first client terminal offset");
        assert!(first_offset.offset_from_bottom > 0);

        apply_terminal_offsets_to_runtimes(&live_terminal_ids, &runtimes, &second_client);
        assert_eq!(
            runtimes
                .get(&terminal_id)
                .and_then(|runtime| runtime.scroll_metrics())
                .map(|metrics| metrics.offset_from_bottom),
            Some(0)
        );

        apply_terminal_offsets_to_runtimes(&live_terminal_ids, &runtimes, &first_client);
        assert_eq!(
            runtimes
                .get(&terminal_id)
                .and_then(|runtime| runtime.scroll_metrics())
                .map(|metrics| metrics.offset_from_bottom),
            Some(first_offset.offset_from_bottom)
        );
    }

    #[tokio::test]
    async fn scrolled_terminal_client_view_stays_anchored_when_output_grows() {
        let mut state = AppState::test_new();
        state.workspaces = vec![Workspace::test_new("terminal")];
        state.active = Some(0);
        let pane_id = state.workspaces[0].focused_pane_id().expect("focused pane");
        let terminal_id = state.workspaces[0]
            .pane_state(pane_id)
            .and_then(|pane| pane.terminal_id_cloned())
            .expect("terminal id");
        let live_terminal_ids = vec![terminal_id.clone()];
        let mut runtimes = TerminalRuntimeRegistry::new();
        runtimes.insert(
            terminal_id.clone(),
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                80,
                3,
                10_000,
                b"000000\r\n000001\r\n000002\r\n000003\r\n000004",
            ),
        );
        let runtime = runtimes.get(&terminal_id).expect("initial runtime");
        runtime.scroll_up(1);
        let visible_before = runtime.visible_text();
        let mut client = ClientViewState::from_default_client_state(&state);
        capture_terminal_offsets_from_runtimes(&live_terminal_ids, &runtimes, &mut client);

        runtimes.insert(
            terminal_id.clone(),
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                80,
                3,
                10_000,
                b"000000\r\n000001\r\n000002\r\n000003\r\n000004\r\n000005",
            ),
        );
        apply_terminal_offsets_to_runtimes(&live_terminal_ids, &runtimes, &client);
        let runtime = runtimes.get(&terminal_id).expect("streamed runtime");

        assert_eq!(
            runtime
                .scroll_metrics()
                .map(|metrics| metrics.offset_from_bottom),
            Some(2)
        );
        assert_eq!(runtime.visible_text(), visible_before);
    }
}
