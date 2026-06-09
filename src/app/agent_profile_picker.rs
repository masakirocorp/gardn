use super::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentProfilePickerEntry {
    pub profile_id: String,
    pub name: String,
    pub section: &'static str,
}

impl AgentProfilePickerEntry {
    fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return true;
        }

        let haystack = format!("{} {}", self.name, self.section).to_ascii_lowercase();
        query.split_whitespace().all(|term| haystack.contains(term))
    }
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
        .filter(|profile| profile.available())
        .map(|profile| AgentProfilePickerEntry {
            profile_id: profile.id.clone(),
            name: profile.name.clone(),
            section: "favorites",
        })
        .chain(
            available
                .into_iter()
                .filter(|profile| profile.available())
                .map(|profile| AgentProfilePickerEntry {
                    profile_id: profile.id.clone(),
                    name: profile.name.clone(),
                    section: "available",
                }),
        )
        .collect()
}

pub(crate) fn agent_profile_picker_filtered_entries(
    state: &AppState,
) -> Vec<AgentProfilePickerEntry> {
    let query = state.agent_profile_picker.query.as_str();
    agent_profile_picker_entries(state)
        .into_iter()
        .filter(|entry| entry.matches(query))
        .collect()
}
