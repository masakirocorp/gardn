use crate::agent_profiles::AgentKind;

use super::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentProfilePickerEntry {
    pub profile_id: String,
    pub name: String,
    pub kind: AgentKind,
    pub section: &'static str,
}

impl AgentProfilePickerEntry {
    fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return true;
        }

        let name = self.name.to_ascii_lowercase();
        let kind = self.kind.as_str();
        query
            .split_whitespace()
            .all(|term| name.contains(term) || kind.contains(term) || self.section.contains(term))
    }
}

pub(crate) const AGENT_PROFILE_PICKER_TABS: [Option<AgentKind>; 10] = [
    None,
    Some(AgentKind::Pi),
    Some(AgentKind::Omp),
    Some(AgentKind::Claude),
    Some(AgentKind::Codex),
    Some(AgentKind::Copilot),
    Some(AgentKind::Opencode),
    Some(AgentKind::Hermes),
    Some(AgentKind::Qodercli),
    Some(AgentKind::Custom),
];

pub(crate) fn agent_profile_picker_tab_label(tab: Option<AgentKind>) -> &'static str {
    tab.map_or("all", AgentKind::as_str)
}

pub(crate) fn workspace_agent_profile_ids(
    state: &AppState,
    ws_idx: usize,
) -> impl Iterator<Item = String> + '_ {
    agent_profile_picker_entries_for_workspace(state, ws_idx)
        .into_iter()
        .map(|entry| entry.profile_id)
}

pub(crate) fn agent_profile_picker_entries(state: &AppState) -> Vec<AgentProfilePickerEntry> {
    agent_profile_picker_entries_for_workspace(state, state.agent_profile_picker.ws_idx)
}

pub(crate) fn agent_profile_picker_entries_for_picker(
    state: &AppState,
    picker: &super::state::AgentProfilePickerState,
) -> Vec<AgentProfilePickerEntry> {
    agent_profile_picker_entries_for_workspace(state, picker.ws_idx)
}

pub(crate) fn agent_profile_picker_entries_for_workspace(
    state: &AppState,
    ws_idx: usize,
) -> Vec<AgentProfilePickerEntry> {
    let group_idx = state
        .workspaces
        .get(ws_idx)
        .and_then(|workspace| state.group_index_by_id(&workspace.group_id))
        .unwrap_or(state.active_group);
    let favorites = state
        .groups
        .get(group_idx)
        .map(|group| group.favorite_agent_profile_ids.as_slice())
        .unwrap_or(&[]);
    let (favorite, available) = state.agent_profiles.group_sections(favorites);
    favorite
        .into_iter()
        .filter(|profile| state.agent_profile_launchable(profile))
        .map(|profile| AgentProfilePickerEntry {
            profile_id: profile.id.clone(),
            name: profile.name.clone(),
            kind: profile.kind,
            section: "favorites",
        })
        .chain(
            available
                .into_iter()
                .filter(|profile| state.agent_profile_launchable(profile))
                .map(|profile| AgentProfilePickerEntry {
                    profile_id: profile.id.clone(),
                    name: profile.name.clone(),
                    kind: profile.kind,
                    section: "available",
                }),
        )
        .collect()
}

pub(crate) fn agent_profile_picker_filtered_entries(
    state: &AppState,
) -> Vec<AgentProfilePickerEntry> {
    let query = state.agent_profile_picker.query.as_str();
    let kind_filter = state.agent_profile_picker.kind_filter;
    agent_profile_picker_entries(state)
        .into_iter()
        .filter(|entry| kind_filter.is_none_or(|kind| entry.kind == kind))
        .filter(|entry| entry.matches(query))
        .collect()
}

pub(crate) fn agent_profile_picker_filtered_entries_for_picker(
    state: &AppState,
    picker: &super::state::AgentProfilePickerState,
) -> Vec<AgentProfilePickerEntry> {
    let query = picker.query.as_str();
    let kind_filter = picker.kind_filter;
    agent_profile_picker_entries_for_picker(state, picker)
        .into_iter()
        .filter(|entry| kind_filter.is_none_or(|kind| entry.kind == kind))
        .filter(|entry| entry.matches(query))
        .collect()
}
