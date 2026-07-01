use std::collections::{HashMap, HashSet};

use crate::app::state::{AppState, CommandPaletteState, Mode, SettingsState, ViewState};
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
    pub(crate) mode: Mode,
    pub(crate) active_tabs: HashMap<String, usize>,
    pub(crate) focused_panes: HashMap<ClientTabViewKey, PaneId>,
    pub(crate) zoomed_tabs: HashSet<ClientTabViewKey>,
    pub(crate) native_diff_panes: HashMap<ClientPaneViewKey, NativeDiffPaneViewState>,
    pub(crate) terminal_offsets_from_bottom: HashMap<TerminalId, usize>,
    pub(crate) settings: SettingsState,
    pub(crate) command_palette: CommandPaletteState,
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
            native_diff_panes: HashMap::new(),
            terminal_offsets_from_bottom: HashMap::new(),
            settings: state.settings.clone(),
            command_palette: state.command_palette.clone(),
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
    state.mode = view.mode;
    state.view = view.computed.clone();
    state.settings = view.settings.clone();
    state.command_palette = view.command_palette.clone();

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
