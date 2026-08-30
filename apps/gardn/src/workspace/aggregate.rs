use std::collections::HashMap;

use crate::detect::{Agent, AgentState};
use crate::layout::PaneId;
use crate::terminal::{TerminalId, TerminalRuntimeRegistry, TerminalState};

use super::{fallback_label_from_cwd, Tab, Workspace};

/// Detail info for a single pane, used by the agent detail panel.
pub struct PaneDetail {
    pub pane_id: PaneId,
    pub tab_idx: usize,
    pub tab_label: String,
    pub pane_label: String,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub label: String,
    pub agent_label: String,
    #[allow(dead_code)]
    pub agent: Option<Agent>,
    pub state: AgentState,
    pub seen: bool,
    pub custom_status: Option<String>,
    pub state_labels: HashMap<String, String>,
    pub tokens: HashMap<String, String>,
    pub last_meaningful_agent_activity_seq: u64,
    pub last_meaningful_agent_activity_unix_secs: Option<u64>,
}

impl Tab {
    fn pane_details_from(
        &self,
        terminals: &HashMap<TerminalId, TerminalState>,
        terminal_runtimes: &TerminalRuntimeRegistry,
        tab_idx: usize,
        tab_label: &str,
    ) -> Vec<PaneDetail> {
        self.layout
            .pane_ids()
            .iter()
            .filter_map(|id| {
                let pane = self.panes.get(id)?;
                let terminal = terminals.get(&pane.attached_terminal_id);
                let fallback_agent_label = {
                    #[cfg(test)]
                    {
                        pane.detected_agent
                            .map(crate::detect::agent_label)
                            .map(str::to_string)
                    }
                    #[cfg(not(test))]
                    {
                        None
                    }
                };
                let agent_label = terminal
                    .and_then(|terminal| {
                        let fallback = terminal
                            .agent_name
                            .as_deref()
                            .or_else(|| terminal.effective_agent_label())?;
                        Some(
                            terminal
                                .effective_display_agent()
                                .unwrap_or_else(|| fallback.to_string()),
                        )
                    })
                    .or(fallback_agent_label)?;
                let fallback_agent = {
                    #[cfg(test)]
                    {
                        pane.detected_agent
                    }
                    #[cfg(not(test))]
                    {
                        None
                    }
                };
                let agent = terminal
                    .and_then(TerminalState::effective_known_agent)
                    .or(fallback_agent);
                let state = terminal.map_or_else(
                    || {
                        #[cfg(test)]
                        {
                            pane.state
                        }
                        #[cfg(not(test))]
                        {
                            AgentState::Unknown
                        }
                    },
                    |terminal| terminal.state,
                );
                #[cfg(test)]
                let state = if state == AgentState::Unknown {
                    pane.state
                } else {
                    state
                };
                let presentation = terminal.map(TerminalState::effective_presentation);
                let pane_label = terminal
                    .map(|terminal| fallback_label_from_cwd(&terminal.cwd))
                    .unwrap_or_else(|| self.display_name());
                let terminal_title = terminal
                    .and_then(|terminal| terminal_runtimes.get(&terminal.id))
                    .map(|runtime| runtime.agent_osc_title())
                    .filter(|title| !title.trim().is_empty());
                let terminal_title_stripped = terminal_title
                    .as_deref()
                    .and_then(crate::terminal::stripped_terminal_title);
                Some(PaneDetail {
                    pane_id: *id,
                    tab_idx,
                    tab_label: tab_label.to_string(),
                    pane_label,
                    terminal_title,
                    terminal_title_stripped,
                    label: agent_label.clone(),
                    agent_label,
                    agent,
                    state,
                    seen: pane.seen,
                    custom_status: presentation
                        .as_ref()
                        .and_then(|presentation| presentation.custom_status.clone()),
                    state_labels: presentation
                        .as_ref()
                        .map(|presentation| presentation.state_labels.clone())
                        .unwrap_or_default(),
                    tokens: presentation
                        .as_ref()
                        .map(|presentation| presentation.tokens.clone())
                        .unwrap_or_default(),
                    last_meaningful_agent_activity_seq: terminal
                        .map(TerminalState::last_meaningful_agent_activity_seq)
                        .unwrap_or_default(),
                    last_meaningful_agent_activity_unix_secs: terminal
                        .and_then(TerminalState::last_meaningful_agent_activity_unix_secs),
                })
            })
            .collect()
    }
}

fn pane_attention_priority(state: AgentState, seen: bool) -> u8 {
    match (state, seen) {
        (AgentState::Blocked, _) => 4,
        (AgentState::Working, _) => 3,
        (AgentState::Idle, false) => 2,
        (AgentState::Idle, true) => 1,
        (AgentState::Unknown, _) => 0,
    }
}

impl Workspace {
    pub fn aggregate_state(
        &self,
        terminals: &HashMap<TerminalId, TerminalState>,
    ) -> (AgentState, bool) {
        self.tabs
            .iter()
            .flat_map(|tab| tab.panes.values())
            .filter_map(|pane| {
                let state = terminals.get(&pane.attached_terminal_id).map_or_else(
                    || {
                        #[cfg(test)]
                        {
                            pane.state
                        }
                        #[cfg(not(test))]
                        {
                            AgentState::Unknown
                        }
                    },
                    |terminal| terminal.state,
                );
                #[cfg(test)]
                let state = if state == AgentState::Unknown {
                    pane.state
                } else {
                    state
                };
                (state != AgentState::Unknown).then_some((state, pane.seen))
            })
            .max_by_key(|(state, seen)| pane_attention_priority(*state, *seen))
            .unwrap_or((AgentState::Unknown, true))
    }

    pub fn pane_details_from(
        &self,
        terminals: &HashMap<TerminalId, TerminalState>,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) -> Vec<PaneDetail> {
        let multi_tab = self.tabs.len() > 1;
        self.tabs
            .iter()
            .enumerate()
            .flat_map(|(tab_idx, tab)| {
                let tab_label = self
                    .tab_display_name(tab_idx)
                    .unwrap_or_else(|| (tab_idx + 1).to_string());
                tab.pane_details_from(terminals, terminal_runtimes, tab_idx, &tab_label)
            })
            .map(|mut detail| {
                if multi_tab {
                    detail.label = format!("{}·{}", detail.tab_label, detail.agent_label);
                }
                if let Some(name) = &self.custom_name {
                    detail.pane_label = name.clone();
                }
                detail
            })
            .collect()
    }

    #[cfg(test)]
    pub fn pane_details(&self, terminals: &HashMap<TerminalId, TerminalState>) -> Vec<PaneDetail> {
        let empty_runtimes = TerminalRuntimeRegistry::new();
        self.pane_details_from(terminals, &empty_runtimes)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Direction;

    use super::*;
    use crate::detect::Agent;

    fn terminal_for_pane(ws: &Workspace, pane_id: PaneId) -> TerminalState {
        TerminalState::new(ws.terminal_id(pane_id).unwrap().clone(), "/tmp".into())
    }

    #[test]
    fn aggregate_state_all_unknown() {
        let ws = Workspace::test_new("test");
        let mut terminals = HashMap::new();
        let root = ws.tabs[0].root_pane;
        let terminal = terminal_for_pane(&ws, root);
        terminals.insert(terminal.id.clone(), terminal);
        let (state, seen) = ws.aggregate_state(&terminals);
        assert_eq!(state, AgentState::Unknown);
        assert!(seen);
    }

    #[test]
    fn aggregate_state_priority() {
        let mut ws = Workspace::test_new("test");
        let id2 = ws.test_split(Direction::Horizontal);
        let root_id = ws.tabs[0]
            .panes
            .keys()
            .find(|id| **id != id2)
            .copied()
            .unwrap();
        let mut terminals = HashMap::new();
        let mut root_terminal = terminal_for_pane(&ws, root_id);
        root_terminal.state = AgentState::Idle;
        terminals.insert(root_terminal.id.clone(), root_terminal);
        let mut second_terminal = terminal_for_pane(&ws, id2);
        second_terminal.state = AgentState::Working;
        terminals.insert(second_terminal.id.clone(), second_terminal);

        let (state, seen) = ws.aggregate_state(&terminals);

        assert_eq!(state, AgentState::Working);
        assert!(seen);
    }

    #[test]
    fn aggregate_state_working_beats_done_unseen() {
        let mut ws = Workspace::test_new("test");
        let id2 = ws.test_split(Direction::Horizontal);
        let root_id = ws.tabs[0]
            .panes
            .keys()
            .find(|id| **id != id2)
            .copied()
            .unwrap();
        let mut terminals = HashMap::new();
        let mut root_terminal = terminal_for_pane(&ws, root_id);
        root_terminal.state = AgentState::Idle;
        terminals.insert(root_terminal.id.clone(), root_terminal);
        let mut second_terminal = terminal_for_pane(&ws, id2);
        second_terminal.state = AgentState::Working;
        terminals.insert(second_terminal.id.clone(), second_terminal);
        let root = ws.tabs[0].panes.get_mut(&root_id).unwrap();
        root.seen = false;

        let (state, seen) = ws.aggregate_state(&terminals);

        assert_eq!(state, AgentState::Working);
        assert!(seen);
    }

    #[test]
    fn pane_details_prefers_agent_name_over_detected_agent_label() {
        let ws = Workspace::test_new("test");
        let root_pane = ws.tabs[0].root_pane;
        let mut terminals = HashMap::new();
        let mut terminal = terminal_for_pane(&ws, root_pane);
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Working);
        terminal.set_agent_name("planner".into());
        terminals.insert(terminal.id.clone(), terminal);

        let labels: Vec<_> = ws
            .pane_details(&terminals)
            .into_iter()
            .map(|detail| (detail.label, detail.agent_label, detail.agent))
            .collect();

        assert_eq!(
            labels,
            vec![("planner".into(), "planner".into(), Some(Agent::Pi))]
        );
    }

    #[test]
    fn pane_details_includes_tab_context_for_multi_tab_workspace() {
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].custom_name = Some("main".into());
        let root_pane = ws.tabs[0].root_pane;
        let second_tab = ws.test_add_tab(Some("review"));
        let review_pane = ws.tabs[second_tab].root_pane;
        let mut terminals = HashMap::new();
        let mut root_terminal = terminal_for_pane(&ws, root_pane);
        root_terminal.set_hook_authority(
            "test".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
        );
        terminals.insert(root_terminal.id.clone(), root_terminal);
        let mut review_terminal = terminal_for_pane(&ws, review_pane);
        review_terminal.set_hook_authority(
            "test".into(),
            "claude".into(),
            AgentState::Idle,
            None,
            None,
        );
        terminals.insert(review_terminal.id.clone(), review_terminal);

        let labels: Vec<_> = ws
            .pane_details(&terminals)
            .into_iter()
            .map(|detail| (detail.label, detail.agent_label, detail.agent))
            .collect();

        assert_eq!(
            labels,
            vec![
                ("main·pi".into(), "pi".into(), Some(Agent::Pi)),
                ("review·claude".into(), "claude".into(), Some(Agent::Claude)),
            ]
        );
    }
}
