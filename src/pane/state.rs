#[cfg(test)]
use crate::detect::{Agent, AgentState};
use crate::terminal::TerminalId;

/// Viewport state for a pane.
///
/// Terminal identity, cwd, labels, and agent metadata live in TerminalState.
pub struct PaneState {
    pub attached_terminal_id: TerminalId,
    pub env_pane_id_raw: Option<u32>,
    #[cfg(test)]
    pub detected_agent: Option<Agent>,
    #[cfg(test)]
    pub state: AgentState,
    /// Whether the user has seen this pane since its last state change to Idle.
    /// False = "Done" (agent finished while user was in another workspace).
    pub seen: bool,
}

impl PaneState {
    pub fn new(attached_terminal_id: TerminalId) -> Self {
        Self {
            attached_terminal_id,
            env_pane_id_raw: None,
            #[cfg(test)]
            detected_agent: None,
            #[cfg(test)]
            state: AgentState::Unknown,
            seen: true,
        }
    }

    pub fn new_with_env_pane_id(
        attached_terminal_id: TerminalId,
        pane_id: crate::layout::PaneId,
    ) -> Self {
        let mut state = Self::new(attached_terminal_id);
        state.env_pane_id_raw = Some(pane_id.raw());
        state
    }
}
