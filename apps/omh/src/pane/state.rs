#[cfg(test)]
use crate::detect::{Agent, AgentState};
use crate::terminal::TerminalId;

/// Viewport state for a pane.
///
/// Terminal identity, cwd, labels, and agent metadata live in TerminalState.
#[derive(Clone)]
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
    /// Whether unmodified right-click gestures should be forwarded to the pane application.
    pub right_click_passthrough: bool,
}

impl PaneState {
    pub fn new_terminal(attached_terminal_id: TerminalId, env_pane_id_raw: Option<u32>) -> Self {
        Self {
            attached_terminal_id,
            env_pane_id_raw,
            #[cfg(test)]
            detected_agent: None,
            #[cfg(test)]
            state: AgentState::Unknown,
            seen: true,
            right_click_passthrough: false,
        }
    }

    pub fn new_with_env_pane_id(
        attached_terminal_id: TerminalId,
        pane_id: crate::layout::PaneId,
    ) -> Self {
        Self::new_terminal(attached_terminal_id, Some(pane_id.raw()))
    }

    pub fn terminal_id(&self) -> Option<&TerminalId> {
        Some(&self.attached_terminal_id)
    }

    pub fn terminal_id_cloned(&self) -> Option<TerminalId> {
        self.terminal_id().cloned()
    }
}
