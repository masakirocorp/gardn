use std::collections::{HashMap, HashSet};

use crate::app::state::{AppState, Mode, ViewState};
use crate::layout::PaneId;

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
/// This is the migration seam for fields that describe what one attached app
/// client is looking at. Shared session structures remain in `AppState`; this
/// type can be constructed from today's monolithic `AppState` and reconciled
/// after shared workspace/tab/pane mutations.
#[derive(Clone)]
pub(crate) struct ClientViewState {
    pub(crate) active_workspace: Option<usize>,
    pub(crate) selected_workspace: usize,
    pub(crate) active_group: usize,
    pub(crate) group_filter_enabled: bool,
    pub(crate) mode: Mode,
    pub(crate) active_tabs: HashMap<String, usize>,
    pub(crate) focused_panes: HashMap<ClientTabViewKey, PaneId>,
    pub(crate) zoomed_tabs: HashSet<ClientTabViewKey>,
    pub(crate) computed: ViewState,
}

impl ClientViewState {
    pub(crate) fn from_app_state(state: &AppState) -> Self {
        let mut view = Self {
            active_workspace: state.active,
            selected_workspace: state.selected,
            active_group: state.active_group,
            group_filter_enabled: state.group_filter_enabled,
            mode: state.mode,
            active_tabs: HashMap::new(),
            focused_panes: HashMap::new(),
            zoomed_tabs: HashSet::new(),
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
            return;
        }

        self.selected_workspace = self.selected_workspace.min(state.workspaces.len() - 1);
        if !self
            .active_workspace
            .is_some_and(|idx| idx < state.workspaces.len())
        {
            self.active_workspace = state.active.filter(|idx| *idx < state.workspaces.len());
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

        for workspace in &state.workspaces {
            if workspace.tabs.is_empty() {
                self.active_tabs.remove(&workspace.id);
                self.focused_panes
                    .retain(|key, _| key.workspace_id != workspace.id);
                self.zoomed_tabs
                    .retain(|key| key.workspace_id != workspace.id);
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
                let key = ClientTabViewKey::new(&workspace.id, tab_idx + 1);
                if !tab.panes.contains_key(
                    self.focused_panes
                        .get(&key)
                        .unwrap_or(&tab.layout.focused()),
                ) {
                    self.focused_panes.insert(key.clone(), tab.layout.focused());
                } else {
                    self.focused_panes
                        .entry(key.clone())
                        .or_insert_with(|| tab.layout.focused());
                }

                if tab.zoomed {
                    self.zoomed_tabs.insert(key);
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

    pub(crate) fn tab_is_zoomed(&self, workspace_id: &str, tab_number: usize) -> bool {
        self.zoomed_tabs
            .contains(&ClientTabViewKey::new(workspace_id, tab_number))
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
}
