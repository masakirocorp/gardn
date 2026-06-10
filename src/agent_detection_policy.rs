use crate::detect::{Agent, AgentDetection, AgentState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PtySignal {
    pub active: bool,
    pub tainted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DetectionPolicyInput {
    pub agent: Option<Agent>,
    pub screen_detection: AgentDetection,
    pub process_exited: bool,
    pub startup_grace_active: bool,
    pub pty_signal: Option<PtySignal>,
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

fn quiet_pty_screen_fallback(screen: AgentDetection) -> AgentDetection {
    if screen.visible_blocker {
        return detection(AgentState::Blocked, true, false, false);
    }

    detection(AgentState::Idle, false, false, false)
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
    if input.process_exited {
        return DetectionPolicyDecision::Publish(input.screen_detection);
    }

    if input.startup_grace_active {
        return DetectionPolicyDecision::Freeze;
    }

    if input.agent.is_none() {
        return DetectionPolicyDecision::Publish(input.screen_detection);
    }

    let Some(pty_signal) = input.pty_signal else {
        return DetectionPolicyDecision::Publish(input.screen_detection);
    };

    if pty_signal.tainted {
        return DetectionPolicyDecision::Freeze;
    }

    if pty_signal.active {
        return DetectionPolicyDecision::Publish(detection(
            AgentState::Working,
            false,
            false,
            false,
        ));
    }

    DetectionPolicyDecision::Publish(quiet_pty_screen_fallback(input.screen_detection))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(state: AgentState) -> AgentDetection {
        detection(state, false, state == AgentState::Idle, state == AgentState::Working)
    }

    fn input(screen_detection: AgentDetection) -> DetectionPolicyInput {
        DetectionPolicyInput {
            agent: Some(Agent::Codex),
            screen_detection,
            process_exited: false,
            startup_grace_active: false,
            pty_signal: Some(PtySignal {
                active: false,
                tainted: false,
            }),
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

        assert_eq!(apply_detection_policy(input), DetectionPolicyDecision::Freeze);
    }

    #[test]
    fn tainted_pty_freezes_detection() {
        let mut input = input(screen(AgentState::Idle));
        input.pty_signal = Some(PtySignal {
            active: false,
            tainted: true,
        });

        assert_eq!(apply_detection_policy(input), DetectionPolicyDecision::Freeze);
    }

    #[test]
    fn pty_activity_publishes_working_without_screen_authority() {
        let mut input = input(screen(AgentState::Idle));
        input.pty_signal = Some(PtySignal {
            active: true,
            tainted: false,
        });

        assert_eq!(
            apply_detection_policy(input),
            DetectionPolicyDecision::Publish(detection(AgentState::Working, false, false, false))
        );
    }

    #[test]
    fn active_pty_overrides_stale_visible_idle_and_blocker() {
        let mut stale = screen(AgentState::Blocked);
        stale.visible_blocker = true;
        stale.visible_idle = true;
        let mut input = input(stale);
        input.pty_signal = Some(PtySignal {
            active: true,
            tainted: false,
        });

        assert_eq!(
            apply_detection_policy(input),
            DetectionPolicyDecision::Publish(detection(AgentState::Working, false, false, false))
        );
    }

    #[test]
    fn quiet_pty_downgrades_stale_screen_working_and_idle_authority() {
        assert_eq!(
            apply_detection_policy(input(screen(AgentState::Working))),
            DetectionPolicyDecision::Publish(detection(AgentState::Idle, false, false, false))
        );
        assert_eq!(
            apply_detection_policy(input(screen(AgentState::Idle))),
            DetectionPolicyDecision::Publish(detection(AgentState::Idle, false, false, false))
        );
    }

    #[test]
    fn quiet_pty_keeps_visible_blocker_only() {
        let mut blocked = screen(AgentState::Blocked);
        blocked.visible_blocker = true;

        assert_eq!(
            apply_detection_policy(input(blocked)),
            DetectionPolicyDecision::Publish(detection(AgentState::Blocked, true, false, false))
        );
    }
}
