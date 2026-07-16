use crate::config::{
    AgentSidebarToken, AgentsSidebarConfig, SpaceSidebarToken, SpacesSidebarConfig,
};

use super::AgentPanelEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedToken {
    StateIcon,
    StateText(String),
    Workspace(String),
    Tab(String),
    Pane(String),
    Agent(String),
    TerminalTitle(String),
    Branch(String),
    GitStatus { ahead: usize, behind: usize },
    Custom(String),
}

pub(super) fn agent_rows(
    config: &AgentsSidebarConfig,
    entry: &AgentPanelEntry,
    state_text: &str,
) -> Vec<Vec<ResolvedToken>> {
    config
        .rows_for_agent(entry.agent)
        .iter()
        .filter_map(|row| {
            let resolved = row
                .iter()
                .filter_map(|token| match token {
                    AgentSidebarToken::StateIcon => Some(ResolvedToken::StateIcon),
                    AgentSidebarToken::StateText => {
                        Some(ResolvedToken::StateText(state_text.into()))
                    }
                    AgentSidebarToken::Workspace => {
                        Some(ResolvedToken::Workspace(entry.primary_label.clone()))
                    }
                    AgentSidebarToken::Tab => {
                        entry.primary_tab_label.clone().map(ResolvedToken::Tab)
                    }
                    AgentSidebarToken::Pane => entry.pane_label.clone().map(ResolvedToken::Pane),
                    AgentSidebarToken::Agent => entry.agent_label.clone().map(ResolvedToken::Agent),
                    AgentSidebarToken::TerminalTitle => entry
                        .terminal_title
                        .clone()
                        .map(ResolvedToken::TerminalTitle),
                    AgentSidebarToken::TerminalTitleStripped => entry
                        .terminal_title_stripped
                        .clone()
                        .map(ResolvedToken::TerminalTitle),
                    AgentSidebarToken::Custom(name) => {
                        entry.tokens.get(name).cloned().map(ResolvedToken::Custom)
                    }
                })
                .collect::<Vec<_>>();
            (!resolved.is_empty()).then_some(resolved)
        })
        .collect()
}

pub(super) struct SpaceTokenContext<'a> {
    pub workspace: &'a str,
    pub branch: Option<&'a str>,
    pub state_text: &'a str,
    pub ahead_behind: Option<(usize, usize)>,
    pub tokens: &'a std::collections::HashMap<String, String>,
    pub suppress_git_details: bool,
}

pub(super) fn space_rows(
    config: &SpacesSidebarConfig,
    context: SpaceTokenContext<'_>,
) -> Vec<Vec<ResolvedToken>> {
    config
        .rows
        .iter()
        .filter_map(|row| {
            let resolved = row
                .iter()
                .filter_map(|token| match token {
                    SpaceSidebarToken::StateIcon => Some(ResolvedToken::StateIcon),
                    SpaceSidebarToken::StateText => {
                        Some(ResolvedToken::StateText(context.state_text.into()))
                    }
                    SpaceSidebarToken::Workspace => {
                        Some(ResolvedToken::Workspace(context.workspace.into()))
                    }
                    SpaceSidebarToken::Branch if !context.suppress_git_details => context
                        .branch
                        .map(|branch| ResolvedToken::Branch(branch.into())),
                    SpaceSidebarToken::Branch => None,
                    SpaceSidebarToken::GitStatus if !context.suppress_git_details => context
                        .ahead_behind
                        .filter(|(ahead, behind)| *ahead > 0 || *behind > 0)
                        .map(|(ahead, behind)| ResolvedToken::GitStatus { ahead, behind }),
                    SpaceSidebarToken::GitStatus => None,
                    SpaceSidebarToken::Custom(name) => {
                        context.tokens.get(name).cloned().map(ResolvedToken::Custom)
                    }
                })
                .collect::<Vec<_>>();
            (!resolved.is_empty()).then_some(resolved)
        })
        .collect()
}

pub(super) fn separator(previous: &ResolvedToken, current: &ResolvedToken) -> &'static str {
    if matches!(previous, ResolvedToken::StateIcon)
        || matches!(current, ResolvedToken::GitStatus { .. })
    {
        " "
    } else {
        " · "
    }
}
