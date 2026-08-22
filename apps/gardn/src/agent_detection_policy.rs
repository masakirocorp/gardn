use crate::detect::{Agent, AgentDetection};

#[cfg(test)]
use crate::detect::AgentState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DetectionPolicyInput {
    pub agent: Option<Agent>,
    pub screen_detection: AgentDetection,
    pub process_exited: bool,
    pub startup_grace_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetectionPolicyDecision {
    Publish(AgentDetection),
    Freeze,
}

#[cfg(test)]
fn detection(
    state: AgentState,
    visible_blocker: bool,
    visible_idle: bool,
    visible_working: bool,
) -> AgentDetection {
    AgentDetection {
        state,
        skip_state_update: false,
        visible_blocker,
        visible_idle,
        visible_working,
    }
}

#[cfg(test)]
pub(crate) fn full_lifecycle_hook_authority(source: &str, agent_label: &str) -> bool {
    crate::detect::full_lifecycle_hook_authority(source, agent_label)
}

pub(crate) fn apply_detection_policy(input: DetectionPolicyInput) -> DetectionPolicyDecision {
    let _agent = input.agent;
    if input.process_exited {
        return DetectionPolicyDecision::Publish(input.screen_detection);
    }

    if input.screen_detection.skip_state_update {
        return DetectionPolicyDecision::Freeze;
    }
    if input.startup_grace_active {
        return DetectionPolicyDecision::Freeze;
    }

    DetectionPolicyDecision::Publish(input.screen_detection)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(state: AgentState) -> AgentDetection {
        detection(
            state,
            false,
            state == AgentState::Idle,
            state == AgentState::Working,
        )
    }

    fn input(screen_detection: AgentDetection) -> DetectionPolicyInput {
        DetectionPolicyInput {
            agent: Some(Agent::Codex),
            screen_detection,
            process_exited: false,
            startup_grace_active: false,
        }
    }

    #[test]
    fn full_lifecycle_hook_sources_use_gardn_namespace() {
        assert!(!full_lifecycle_hook_authority("gardn:pi", "pi"));
        assert!(!full_lifecycle_hook_authority("gardn:omp", "omp"));
        assert!(full_lifecycle_hook_authority("gardn:claude", "claude"));
        assert!(full_lifecycle_hook_authority("gardn:codex", "codex"));
        assert!(full_lifecycle_hook_authority("gardn:grok", "grok"));
        assert!(!full_lifecycle_hook_authority("gardn:hermes", "hermes"));
        assert!(full_lifecycle_hook_authority("gardn:opencode", "opencode"));
        assert!(full_lifecycle_hook_authority("gardn:kilo", "kilo"));
        assert!(full_lifecycle_hook_authority("gardn:kimi", "kimi"));
        assert!(!full_lifecycle_hook_authority("gardn:pi", "pi"));
    }

    #[test]
    fn startup_grace_freezes_detection() {
        let mut input = input(screen(AgentState::Working));
        input.startup_grace_active = true;

        assert_eq!(
            apply_detection_policy(input),
            DetectionPolicyDecision::Freeze
        );
    }

    #[test]
    fn screen_manifest_state_publishes_without_pty_override() {
        assert_eq!(
            apply_detection_policy(input(screen(AgentState::Working))),
            DetectionPolicyDecision::Publish(screen(AgentState::Working))
        );
        assert_eq!(
            apply_detection_policy(input(screen(AgentState::Idle))),
            DetectionPolicyDecision::Publish(screen(AgentState::Idle))
        );
    }

    #[test]
    fn manifest_skip_state_update_freezes_detection() {
        let mut detection = screen(AgentState::Unknown);
        detection.skip_state_update = true;

        assert_eq!(
            apply_detection_policy(input(detection)),
            DetectionPolicyDecision::Freeze
        );
    }

    #[test]
    fn process_exit_publishes_screen_state() {
        let mut input = input(screen(AgentState::Idle));
        input.process_exited = true;

        assert_eq!(
            apply_detection_policy(input),
            DetectionPolicyDecision::Publish(screen(AgentState::Idle))
        );
    }
}
