use crate::app::state::AppState;
use crate::execution_host::{ExecutionHostId, SshProfileId};
use crate::layout::PaneId;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ConnectionIdentity {
    Coordinator,
    Profile(SshProfileId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ConnectionScope {
    #[default]
    All,
    Only(ConnectionIdentity),
}

fn host_matches_identity(host: &ExecutionHostId, identity: &ConnectionIdentity) -> bool {
    match identity {
        ConnectionIdentity::Coordinator => host.is_local(),
        ConnectionIdentity::Profile(profile_id) => {
            host.ssh_profile_id() == Some(profile_id.as_str())
        }
    }
}

pub(crate) fn workspace_host_ids(
    state: &AppState,
    workspace: &crate::workspace::Workspace,
) -> Vec<ExecutionHostId> {
    let mut hosts = Vec::new();
    for pane in workspace
        .terminal_tabs()
        .flat_map(|(_, tab)| tab.panes.values())
    {
        let Some(terminal) = state.terminals.get(&pane.attached_terminal_id) else {
            continue;
        };
        if !hosts.contains(&terminal.location.execution_host_id) {
            hosts.push(terminal.location.execution_host_id.clone());
        }
    }
    if hosts.is_empty() {
        hosts.push(workspace.default_location.execution_host_id.clone());
    }
    hosts
}

pub(crate) fn workspace_matches(state: &AppState, ws_idx: usize, scope: &ConnectionScope) -> bool {
    let Some(workspace) = state.workspaces.get(ws_idx) else {
        return false;
    };
    let ConnectionScope::Only(selected) = scope else {
        return true;
    };

    let mut has_resolved_terminal = false;
    for pane in workspace
        .terminal_tabs()
        .flat_map(|(_, tab)| tab.panes.values())
    {
        let Some(terminal) = state.terminals.get(&pane.attached_terminal_id) else {
            continue;
        };
        has_resolved_terminal = true;
        if host_matches_identity(&terminal.location.execution_host_id, selected) {
            return true;
        }
    }

    !has_resolved_terminal
        && host_matches_identity(&workspace.default_location.execution_host_id, selected)
}

pub(crate) fn pane_matches(
    state: &AppState,
    ws_idx: usize,
    pane_id: PaneId,
    scope: &ConnectionScope,
) -> bool {
    let ConnectionScope::Only(selected) = scope else {
        return state.workspaces.get(ws_idx).is_some();
    };
    state
        .workspaces
        .get(ws_idx)
        .and_then(|workspace| workspace.pane_state(pane_id))
        .and_then(|pane| state.terminals.get(&pane.attached_terminal_id))
        .is_some_and(|terminal| {
            host_matches_identity(&terminal.location.execution_host_id, selected)
        })
}

pub(crate) fn workspace_is_visible(
    state: &AppState,
    ws_idx: usize,
    active_group: usize,
    group_filter_enabled: bool,
    connection_scope: &ConnectionScope,
) -> bool {
    let Some(workspace) = state.workspaces.get(ws_idx) else {
        return false;
    };
    let group_id = state
        .groups
        .get(active_group)
        .map(|group| group.id.as_str())
        .unwrap_or(crate::workspace::DEFAULT_GROUP_ID);
    (!group_filter_enabled || workspace.group_id == group_id)
        && workspace_matches(state, ws_idx, connection_scope)
}

pub(crate) fn visible_workspace_indices<'a>(
    state: &'a AppState,
    active_group: usize,
    group_filter_enabled: bool,
    connection_scope: &'a ConnectionScope,
) -> impl DoubleEndedIterator<Item = usize> + 'a {
    state
        .workspaces
        .iter()
        .enumerate()
        .filter_map(move |(idx, _)| {
            workspace_is_visible(
                state,
                idx,
                active_group,
                group_filter_enabled,
                connection_scope,
            )
            .then_some(idx)
        })
}

pub(crate) fn scope_label(state: &AppState, scope: &ConnectionScope) -> String {
    match scope {
        ConnectionScope::All => "All connections".to_string(),
        ConnectionScope::Only(ConnectionIdentity::Coordinator) => {
            state.host_display.coordinator().to_string()
        }
        ConnectionScope::Only(ConnectionIdentity::Profile(profile_id)) => state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == profile_id.as_str())
            .map(|profile| profile.name().to_string())
            .unwrap_or_else(|| "All connections".to_string()),
    }
}

pub(crate) fn choices(state: &AppState) -> Vec<(ConnectionScope, String)> {
    let mut choices = vec![
        (ConnectionScope::All, "All".to_string()),
        (
            ConnectionScope::Only(ConnectionIdentity::Coordinator),
            state.host_display.coordinator().to_string(),
        ),
    ];
    choices.extend(state.ssh_connection_profiles.iter().filter_map(|profile| {
        Some((
            ConnectionScope::Only(ConnectionIdentity::Profile(SshProfileId::new(
                profile.id(),
            )?)),
            profile.name().to_string(),
        ))
    }));
    choices
}

pub(crate) fn scope_color(
    state: &AppState,
    scope: &ConnectionScope,
) -> Option<ratatui::style::Color> {
    match scope {
        ConnectionScope::All => None,
        ConnectionScope::Only(ConnectionIdentity::Coordinator) => Some(state.palette.accent),
        ConnectionScope::Only(ConnectionIdentity::Profile(profile_id)) => state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == profile_id.as_str())
            .and_then(|profile| profile.accent())
            .map(|accent| state.global_palette.theme_accent_color(accent))
            .or(Some(state.palette.surface1)),
    }
}

pub(crate) fn reanchor_view_selection(
    state: &AppState,
    view: &mut crate::app::view_state::ClientViewState,
) {
    let next = if view.sidebar_collapsed || view.group_filter_enabled {
        nearest_visible(
            view.selected_workspace,
            visible_workspace_indices(
                state,
                view.active_group,
                view.group_filter_enabled,
                &view.connection_scope,
            ),
        )
    } else {
        nearest_visible(
            view.selected_workspace,
            state
                .workspaces
                .iter()
                .enumerate()
                .filter_map(|(idx, workspace)| {
                    (!view
                        .collapsed_workspace_groups
                        .iter()
                        .any(|group_id| group_id == &workspace.group_id)
                        && workspace_matches(state, idx, &view.connection_scope))
                    .then_some(idx)
                }),
        )
    };
    if let Some(next) = next {
        view.selected_workspace = next;
    }
}

pub(crate) fn nearest_visible(
    previous: usize,
    visible: impl IntoIterator<Item = usize>,
) -> Option<usize> {
    let mut first = None;
    let mut previous_visible = None;
    let mut next_visible = None;
    for idx in visible {
        first.get_or_insert(idx);
        if idx == previous {
            return Some(idx);
        }
        if idx < previous {
            previous_visible = Some(idx);
        } else if next_visible.is_none() {
            next_visible = Some(idx);
        }
    }
    next_visible.or(previous_visible).or(first)
}

pub(crate) fn menu_scroll_offset(selected: usize, total_rows: usize, visible_rows: usize) -> usize {
    if visible_rows == 0 || total_rows <= visible_rows {
        return 0;
    }
    selected
        .saturating_sub(visible_rows - 1)
        .min(total_rows - visible_rows)
}
pub(crate) fn profile_for_host<'a>(
    state: &'a AppState,
    host: &ExecutionHostId,
) -> Option<&'a crate::persist::ssh_profiles::SshConnectionProfile> {
    let profile_id = host.ssh_profile_id()?;
    state
        .ssh_connection_profiles
        .iter()
        .find(|profile| profile.id() == profile_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::{HostPath, ResourceLocation};
    use crate::workspace::Workspace;

    fn location(host: &str) -> ResourceLocation {
        ResourceLocation::new(
            ExecutionHostId::new(host).expect("valid host id"),
            HostPath::new("/work").expect("valid path"),
        )
    }

    fn profile_scope(profile_id: &str) -> ConnectionScope {
        ConnectionScope::Only(ConnectionIdentity::Profile(
            SshProfileId::new(profile_id).expect("valid profile id"),
        ))
    }

    #[test]
    fn workspace_uses_resolved_terminal_hosts_instead_of_default_location() {
        let mut state = AppState::test_new();
        let workspace = Workspace::test_new("mixed");
        let pane_id = workspace.terminal_tab(0).unwrap().root_pane;
        let terminal_id = workspace.terminal_id(pane_id).unwrap().clone();
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        state.workspaces[0].default_location = location("ssh:default:1");
        state.terminals.get_mut(&terminal_id).unwrap().location = location("ssh:actual:3");

        assert!(workspace_matches(&state, 0, &profile_scope("actual")));
        assert!(!workspace_matches(&state, 0, &profile_scope("default")));
    }

    #[test]
    fn workspace_uses_default_location_only_without_a_resolved_terminal() {
        let mut state = AppState::test_new();
        let mut workspace = Workspace::test_new("remote");
        workspace.default_location = location("ssh:default:4");
        state.workspaces = vec![workspace];

        assert!(workspace_matches(&state, 0, &profile_scope("default")));
    }

    #[test]
    fn pane_matching_uses_its_own_resolved_terminal_host() {
        let mut state = AppState::test_new();
        let mut workspace = Workspace::test_new("mixed");
        let coordinator_pane = workspace.terminal_tab(0).unwrap().root_pane;
        let remote_pane = workspace.test_split(ratatui::layout::Direction::Horizontal);
        let remote_terminal_id = workspace.terminal_id(remote_pane).unwrap().clone();
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        state
            .terminals
            .get_mut(&remote_terminal_id)
            .unwrap()
            .location = location("ssh:workbox:7");

        let remote = profile_scope("workbox");
        let coordinator = ConnectionScope::Only(ConnectionIdentity::Coordinator);
        assert!(pane_matches(&state, 0, remote_pane, &remote));
        assert!(!pane_matches(&state, 0, coordinator_pane, &remote));
        assert!(pane_matches(&state, 0, coordinator_pane, &coordinator));
    }

    #[test]
    fn connection_scope_intersects_group_scope() {
        let mut state = AppState::test_new();
        let side = state.create_group("Side".to_string());
        let mut first = Workspace::test_new("first");
        first.default_location = location("ssh:workbox:1");
        let mut second = Workspace::test_new("second");
        second.group_id = state.groups[side].id.clone();
        second.default_location = location("ssh:workbox:2");
        state.workspaces = vec![first, second];

        let scope = profile_scope("workbox");
        assert_eq!(
            visible_workspace_indices(&state, 0, true, &scope).collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            visible_workspace_indices(&state, 0, false, &scope).collect::<Vec<_>>(),
            vec![0, 1]
        );
    }

    #[test]
    fn menu_scroll_keeps_the_selected_row_visible() {
        assert_eq!(menu_scroll_offset(0, 20, 8), 0);
        assert_eq!(menu_scroll_offset(12, 20, 8), 5);
        assert_eq!(menu_scroll_offset(19, 20, 8), 12);
        assert_eq!(menu_scroll_offset(12, 8, 8), 0);
    }
}
