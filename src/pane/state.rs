#[cfg(test)]
use crate::detect::{Agent, AgentState};
use crate::native_diff::NativeDiffPaneState;
use crate::terminal::TerminalId;

pub enum PaneContent {
    Terminal,
    NativeDiff(NativeDiffPaneState),
}

/// Viewport state for a pane.
///
/// Terminal identity, cwd, labels, and agent metadata live in TerminalState.
pub struct PaneState {
    pub attached_terminal_id: TerminalId,
    pub env_pane_id_raw: Option<u32>,
    pub content: PaneContent,
    #[cfg(test)]
    pub detected_agent: Option<Agent>,
    #[cfg(test)]
    pub state: AgentState,
    /// Whether the user has seen this pane since its last state change to Idle.
    /// False = "Done" (agent finished while user was in another workspace).
    pub seen: bool,
}

impl PaneState {
    pub fn new_terminal(attached_terminal_id: TerminalId, env_pane_id_raw: Option<u32>) -> Self {
        Self {
            attached_terminal_id,
            env_pane_id_raw,
            content: PaneContent::Terminal,
            #[cfg(test)]
            detected_agent: None,
            #[cfg(test)]
            state: AgentState::Unknown,
            seen: true,
        }
    }

    pub fn new_native_diff(diff: NativeDiffPaneState) -> Self {
        Self {
            attached_terminal_id: TerminalId::alloc(),
            env_pane_id_raw: None,
            content: PaneContent::NativeDiff(diff),
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
        Self::new_terminal(attached_terminal_id, Some(pane_id.raw()))
    }

    pub fn terminal_id(&self) -> Option<&TerminalId> {
        match self.content {
            PaneContent::Terminal => Some(&self.attached_terminal_id),
            PaneContent::NativeDiff(_) => None,
        }
    }

    pub fn terminal_id_cloned(&self) -> Option<TerminalId> {
        self.terminal_id().cloned()
    }

    pub fn native_diff(&self) -> Option<&NativeDiffPaneState> {
        match &self.content {
            PaneContent::NativeDiff(diff) => Some(diff),
            PaneContent::Terminal => None,
        }
    }
}
