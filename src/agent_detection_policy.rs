use crate::detect::{Agent, AgentDetection, AgentState};

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

fn detection(
    state: AgentState,
    visible_blocker: bool,
    visible_idle: bool,
    visible_working: bool,
) -> AgentDetection {
    AgentDetection {
        state,
        visible_blocker,
        visible_idle,
        visible_working,
    }
}

#[cfg(test)]
pub(crate) fn full_lifecycle_hook_authority(source: &str, agent_label: &str) -> bool {
    matches!(
        (source, agent_label),
        ("hako:pi", "pi")
            | ("hako:omp", "omp")
            | ("hako:hermes", "hermes")
            | ("hako:opencode", "opencode")
            | ("hako:kilo", "kilo")
    )
}

pub(crate) fn apply_detection_policy(input: DetectionPolicyInput) -> DetectionPolicyDecision {
    let _agent = input.agent;
    if input.process_exited {
        return DetectionPolicyDecision::Publish(input.screen_detection);
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
    fn full_lifecycle_hook_sources_use_hako_namespace() {
        assert!(full_lifecycle_hook_authority("hako:pi", "pi"));
        assert!(full_lifecycle_hook_authority("hako:omp", "omp"));
        assert!(full_lifecycle_hook_authority("hako:hermes", "hermes"));
        assert!(full_lifecycle_hook_authority("hako:opencode", "opencode"));
        assert!(full_lifecycle_hook_authority("hako:kilo", "kilo"));
        assert!(!full_lifecycle_hook_authority("herdr:pi", "pi"));
        assert!(!full_lifecycle_hook_authority("hako:codex", "codex"));
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
    fn process_exit_publishes_screen_state() {
        let mut input = input(screen(AgentState::Idle));
        input.process_exited = true;

        assert_eq!(
            apply_detection_policy(input),
            DetectionPolicyDecision::Publish(screen(AgentState::Idle))
        );
    }
}
