use std::sync::atomic::{AtomicU64, Ordering};

use crate::detect::{Agent, AgentDetection, AgentState};

pub(super) const AGENT_PTY_ACTIVITY_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1800);
pub(super) const AGENT_INPUT_TAINT_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(1200);
const AGENT_PENDING_IDLE_CONFIRMATIONS: u8 = 3;
pub(super) const AGENT_PENDING_IDLE_CAP: std::time::Duration =
    std::time::Duration::from_millis(700);
pub(super) const AGENT_PENDING_IDLE_RECHECK: std::time::Duration =
    std::time::Duration::from_millis(100);
pub(super) const AGENT_PENDING_WORKING_FAST_RECHECK: std::time::Duration =
    std::time::Duration::from_millis(100);
const AGENT_PENDING_WORKING_CONFIRM_DELAY: std::time::Duration =
    std::time::Duration::from_millis(250);
const AGENT_PENDING_WORKING_CAP: std::time::Duration = std::time::Duration::from_secs(2);
const POST_TAINT_WORKING_LEASE: std::time::Duration = AGENT_PTY_ACTIVITY_WINDOW;
pub(super) const STABLE_VISIBLE_SIGNAL_REFRESH: std::time::Duration =
    std::time::Duration::from_millis(800);
pub(super) const AGENT_STARTUP_GRACE_WINDOW: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DetectionPublishState {
    pub(super) state: AgentState,
    pub(super) visible_blocker: bool,
    pub(super) visible_idle: bool,
    pub(super) visible_working: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PtyActivitySignal {
    pub(super) active: bool,
    pub(super) tainted: bool,
    pub(super) taint_just_ended: bool,
    pub(super) fresh_output: bool,
    pub(super) output_seq: u64,
}

#[derive(Debug, Default)]
pub(super) struct PtyCausalityTracker {
    last_pty_output_seq: u64,
    last_input_seq: u64,
    input_tainted_until: Option<std::time::Instant>,
    last_agent_pty_at: Option<std::time::Instant>,
}

pub(super) fn baseline_pty_causality(
    tracker: &mut PtyCausalityTracker,
    pty_output_seq: u64,
    input_seq: u64,
) {
    tracker.last_pty_output_seq = pty_output_seq;
    tracker.last_input_seq = input_seq;
    tracker.input_tainted_until = None;
    tracker.last_agent_pty_at = None;
}

pub(super) fn agent_caused_pty_activity_active(
    pty_output_seq: u64,
    input_seq: u64,
    tracker: &mut PtyCausalityTracker,
    now: std::time::Instant,
) -> PtyActivitySignal {
    if input_seq != tracker.last_input_seq {
        tracker.last_input_seq = input_seq;
        tracker.input_tainted_until = Some(now + AGENT_INPUT_TAINT_WINDOW);
        tracker.last_agent_pty_at = None;
    }

    let mut taint_just_ended = false;
    let tainted = match tracker.input_tainted_until {
        Some(until) if now < until => true,
        Some(_) => {
            tracker.input_tainted_until = None;
            taint_just_ended = true;
            false
        }
        None => false,
    };

    let mut fresh_output = false;
    if pty_output_seq != tracker.last_pty_output_seq {
        tracker.last_pty_output_seq = pty_output_seq;
        if !tainted {
            tracker.last_agent_pty_at = Some(now);
            fresh_output = true;
        }
    }

    if tainted {
        return PtyActivitySignal {
            active: false,
            tainted: true,
            taint_just_ended: false,
            fresh_output: false,
            output_seq: tracker.last_pty_output_seq,
        };
    }

    let active = tracker
        .last_agent_pty_at
        .is_some_and(|last| now.duration_since(last) < AGENT_PTY_ACTIVITY_WINDOW);
    PtyActivitySignal {
        active,
        tainted: false,
        taint_just_ended,
        fresh_output,
        output_seq: tracker.last_pty_output_seq,
    }
}

pub(super) fn observe_pty_output_activity(bytes: &[u8], pty_output_seq: &AtomicU64) {
    if !bytes.is_empty() {
        pty_output_seq.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
pub(super) struct PendingIdleConfirmation {
    started_at: Option<std::time::Instant>,
    confirmations: u8,
}

impl PendingIdleConfirmation {
    pub(super) fn active(&self) -> bool {
        self.started_at.is_some()
    }

    pub(super) fn clear(&mut self) {
        self.started_at = None;
        self.confirmations = 0;
    }

    fn should_hold_working_to_idle(
        &mut self,
        previous: DetectionPublishState,
        next: DetectionPublishState,
        agent_changed: bool,
        process_exited: bool,
        pty_signal: Option<PtyActivitySignal>,
        now: std::time::Instant,
    ) -> bool {
        let plain_working_to_idle = previous.state == AgentState::Working
            && next.state == AgentState::Idle
            && !next.visible_blocker
            && !next.visible_working
            && !agent_changed
            && !process_exited;
        if !plain_working_to_idle {
            self.clear();
            return false;
        }
        if pty_signal.is_some_and(|signal| !signal.active && !signal.tainted) {
            self.clear();
            return false;
        }
        let Some(started_at) = self.started_at else {
            self.started_at = Some(now);
            self.confirmations = 0;
            return true;
        };
        if now.duration_since(started_at) >= AGENT_PENDING_IDLE_CAP {
            self.clear();
            return false;
        }
        self.confirmations = self.confirmations.saturating_add(1);
        if self.confirmations >= AGENT_PENDING_IDLE_CONFIRMATIONS {
            self.clear();
            return false;
        }
        true
    }
}

#[derive(Debug, Default)]
pub(super) struct PendingWorkingConfirmation {
    started_at: Option<std::time::Instant>,
    last_observed_output_seq: u64,
}

impl PendingWorkingConfirmation {
    pub(super) fn active(&self) -> bool {
        self.started_at.is_some()
    }

    pub(super) fn clear(&mut self) {
        self.started_at = None;
        self.last_observed_output_seq = 0;
    }

    pub(super) fn recheck_delay(&self) -> std::time::Duration {
        AGENT_PENDING_WORKING_FAST_RECHECK
    }

    fn should_hold_idle_to_working(
        &mut self,
        previous: DetectionPublishState,
        next: DetectionPublishState,
        agent_changed: bool,
        process_exited: bool,
        pty_signal: Option<PtyActivitySignal>,
        now: std::time::Instant,
    ) -> bool {
        let idle_to_working = previous.state == AgentState::Idle
            && next.state == AgentState::Working
            && !next.visible_blocker
            && !agent_changed
            && !process_exited;
        if !idle_to_working {
            self.clear();
            return false;
        }
        let Some(pty_signal) = pty_signal else {
            self.clear();
            return false;
        };
        if !pty_signal.active {
            self.clear();
            return false;
        }
        let Some(started_at) = self.started_at else {
            self.started_at = Some(now);
            self.last_observed_output_seq = pty_signal.output_seq;
            return true;
        };
        if pty_signal.fresh_output && pty_signal.output_seq != self.last_observed_output_seq {
            self.last_observed_output_seq = pty_signal.output_seq;
            if now.duration_since(started_at) >= AGENT_PENDING_WORKING_CONFIRM_DELAY {
                self.clear();
                return false;
            }
        }
        if now.duration_since(started_at) >= AGENT_PENDING_WORKING_CAP {
            self.clear();
        }
        true
    }

    fn should_publish_held_working_before_exit(
        &mut self,
        previous: DetectionPublishState,
        next: DetectionPublishState,
        process_exited: bool,
    ) -> bool {
        if self.started_at.is_none()
            || !process_exited
            || previous.state != AgentState::Idle
            || next.state != AgentState::Idle
        {
            return false;
        }
        self.clear();
        true
    }
}

#[derive(Debug, Default)]
pub(super) struct PostTaintWorkingLease {
    until: Option<std::time::Instant>,
}

impl PostTaintWorkingLease {
    pub(super) fn active(&self) -> bool {
        self.until.is_some()
    }

    fn start(&mut self, now: std::time::Instant) {
        self.until = Some(now + POST_TAINT_WORKING_LEASE);
    }

    pub(super) fn clear(&mut self) {
        self.until = None;
    }

    fn should_hold_working_to_idle(
        &mut self,
        previous: DetectionPublishState,
        next: DetectionPublishState,
        agent_changed: bool,
        process_exited: bool,
        pty_signal: Option<PtyActivitySignal>,
        now: std::time::Instant,
    ) -> bool {
        let plain_working_to_idle = previous.state == AgentState::Working
            && next.state == AgentState::Idle
            && !next.visible_blocker
            && !agent_changed
            && !process_exited;
        if !plain_working_to_idle {
            self.clear();
            return false;
        }
        let Some(pty_signal) = pty_signal else {
            self.clear();
            return false;
        };
        if pty_signal.active || pty_signal.tainted {
            self.clear();
            return false;
        }
        if pty_signal.taint_just_ended {
            self.start(now);
            return true;
        }
        if self.until.is_some_and(|until| now < until) {
            return true;
        }
        self.clear();
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DetectionScreenReadDecision {
    Read,
    Skip,
    EvaluatePtyWorking,
    Publish {
        state: AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DetectionScreenReadInput {
    pub(super) state: AgentState,
    pub(super) agent: Option<Agent>,
    pub(super) pending_idle_active: bool,
    pub(super) pending_working_active: bool,
    pub(super) post_taint_working_active: bool,
    pub(super) agent_changed: bool,
    pub(super) process_exited: bool,
    pub(super) pty_activity: Option<PtyActivitySignal>,
    pub(super) last_screen_scan_pty_output_seq: Option<u64>,
}

fn agent_activity_veto_requires_screen(agent: Option<Agent>) -> bool {
    matches!(agent, Some(Agent::Claude))
}

pub(super) fn decide_detection_screen_read(input: DetectionScreenReadInput) -> DetectionScreenReadDecision {
    if !input.agent_changed
        && !input.process_exited
        && !input.pending_idle_active
        && !input.post_taint_working_active
        && input.agent.is_some()
        && input.pty_activity.is_some_and(|signal| signal.active && !signal.tainted)
    {
        return match input.state {
            AgentState::Working => DetectionScreenReadDecision::Skip,
            AgentState::Blocked => DetectionScreenReadDecision::Publish {
                state: AgentState::Working,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
            },
            AgentState::Idle | AgentState::Unknown
                if agent_activity_veto_requires_screen(input.agent) && !input.pending_working_active =>
            {
                DetectionScreenReadDecision::Read
            }
            AgentState::Idle | AgentState::Unknown => DetectionScreenReadDecision::EvaluatePtyWorking,
        };
    }

    if input.state == AgentState::Idle
        && input.agent.is_some()
        && !input.pending_idle_active
        && !input.pending_working_active
        && !input.post_taint_working_active
        && !input.agent_changed
        && !input.process_exited
        && input.pty_activity.is_some_and(|signal| {
            !signal.active
                && !signal.tainted
                && !signal.taint_just_ended
                && input.last_screen_scan_pty_output_seq == Some(signal.output_seq)
        })
    {
        DetectionScreenReadDecision::Skip
    } else {
        DetectionScreenReadDecision::Read
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DetectionPublishDecision {
    NoPublish,
    Publish {
        state: AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PtyWorkingPublishInput {
    pub(super) agent: Option<Agent>,
    pub(super) current_state: AgentState,
    pub(super) last_visible_blocker: bool,
    pub(super) last_visible_idle: bool,
    pub(super) last_visible_working: bool,
    pub(super) last_visible_signal_refresh: Option<std::time::Instant>,
    pub(super) pty_activity: Option<PtyActivitySignal>,
    pub(super) now: std::time::Instant,
}

#[derive(Debug)]
pub(super) struct ScreenDetectionPublishInput<'a> {
    pub(super) agent: Option<Agent>,
    pub(super) current_state: AgentState,
    pub(super) last_visible_blocker: bool,
    pub(super) last_visible_idle: bool,
    pub(super) last_visible_working: bool,
    pub(super) last_visible_signal_refresh: Option<std::time::Instant>,
    pub(super) screen_detection: AgentDetection,
    pub(super) process_exited: bool,
    pub(super) agent_changed: bool,
    pub(super) pty_activity: Option<PtyActivitySignal>,
    pub(super) content: &'a str,
    pub(super) now: std::time::Instant,
    pub(super) last_claude_working_at: &'a mut Option<std::time::Instant>,
}

fn should_publish_detection_update(
    previous: DetectionPublishState,
    next: DetectionPublishState,
    agent_changed: bool,
    process_exited: bool,
    stable_visible_signal_refresh_due: bool,
) -> bool {
    next.state != previous.state
        || next.visible_blocker != previous.visible_blocker
        || next.visible_idle != previous.visible_idle
        || next.visible_working != previous.visible_working
        || agent_changed
        || process_exited
        || (stable_visible_signal_refresh_due && next.visible_blocker && previous.visible_blocker)
}

fn stable_visible_signal_refresh_due(
    previous: DetectionPublishState,
    next: DetectionPublishState,
    last_refresh: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    let stable_visible_signal = next.visible_blocker && previous.visible_blocker;
    stable_visible_signal
        && last_refresh.is_none_or(|last_refresh| now.duration_since(last_refresh) >= STABLE_VISIBLE_SIGNAL_REFRESH)
}

fn pty_working_transition_is_vetoed(
    agent: Option<Agent>,
    previous: DetectionPublishState,
    next: DetectionPublishState,
    content: &str,
) -> bool {
    previous.state == AgentState::Idle
        && next.state == AgentState::Working
        && !next.visible_blocker
        && matches!(agent, Some(Agent::Claude))
        && content.contains("※ recap: Done")
}

fn decide_transition(
    agent: Option<Agent>,
    previous: DetectionPublishState,
    next: DetectionPublishState,
    agent_changed: bool,
    process_exited: bool,
    pty_activity: Option<PtyActivitySignal>,
    stable_refresh_due: bool,
    content: &str,
    now: std::time::Instant,
    pending_idle: &mut PendingIdleConfirmation,
    pending_working: &mut PendingWorkingConfirmation,
    post_taint_working: &mut PostTaintWorkingLease,
) -> DetectionPublishDecision {
    if pty_working_transition_is_vetoed(agent, previous, next, content) {
        pending_idle.clear();
        pending_working.clear();
        post_taint_working.clear();
        return DetectionPublishDecision::NoPublish;
    }
    if pending_working.should_publish_held_working_before_exit(previous, next, process_exited) {
        pending_idle.clear();
        post_taint_working.clear();
        return DetectionPublishDecision::Publish {
            state: AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
        };
    }
    if pending_working.should_hold_idle_to_working(
        previous,
        next,
        agent_changed,
        process_exited,
        pty_activity,
        now,
    ) {
        pending_idle.clear();
        post_taint_working.clear();
        return DetectionPublishDecision::NoPublish;
    }
    if post_taint_working.should_hold_working_to_idle(
        previous,
        next,
        agent_changed,
        process_exited,
        pty_activity,
        now,
    ) {
        pending_idle.clear();
        pending_working.clear();
        return DetectionPublishDecision::NoPublish;
    }
    if pending_idle.should_hold_working_to_idle(
        previous,
        next,
        agent_changed,
        process_exited,
        pty_activity,
        now,
    ) {
        pending_working.clear();
        post_taint_working.clear();
        return DetectionPublishDecision::NoPublish;
    }
    if should_publish_detection_update(previous, next, agent_changed, process_exited, stable_refresh_due) {
        return DetectionPublishDecision::Publish {
            state: next.state,
            visible_blocker: next.visible_blocker,
            visible_idle: next.visible_idle,
            visible_working: next.visible_working,
            process_exited,
        };
    }
    DetectionPublishDecision::NoPublish
}

pub(super) fn decide_pty_working_publish_without_screen(
    input: PtyWorkingPublishInput,
    pending_idle: &mut PendingIdleConfirmation,
    pending_working: &mut PendingWorkingConfirmation,
    post_taint_working: &mut PostTaintWorkingLease,
) -> DetectionPublishDecision {
    let previous = DetectionPublishState {
        state: input.current_state,
        visible_blocker: input.last_visible_blocker,
        visible_idle: input.last_visible_idle,
        visible_working: input.last_visible_working,
    };
    let next = DetectionPublishState {
        state: AgentState::Working,
        visible_blocker: false,
        visible_idle: false,
        visible_working: false,
    };
    let stable_refresh_due = stable_visible_signal_refresh_due(
        previous,
        next,
        input.last_visible_signal_refresh,
        input.now,
    );
    decide_transition(
        input.agent,
        previous,
        next,
        false,
        false,
        input.pty_activity,
        stable_refresh_due,
        "",
        input.now,
        pending_idle,
        pending_working,
        post_taint_working,
    )
}

pub(super) fn decide_screen_detection_publish(
    input: ScreenDetectionPublishInput<'_>,
    pending_idle: &mut PendingIdleConfirmation,
    pending_working: &mut PendingWorkingConfirmation,
    post_taint_working: &mut PostTaintWorkingLease,
) -> DetectionPublishDecision {
    let pty_signal = input.pty_activity.map(|signal| crate::agent_detection_policy::PtySignal {
        active: signal.active,
        tainted: signal.tainted,
    });
    let detection = match crate::agent_detection_policy::apply_detection_policy(
        crate::agent_detection_policy::DetectionPolicyInput {
            agent: input.agent,
            screen_detection: input.screen_detection,
            process_exited: input.process_exited,
            startup_grace_active: false,
            pty_signal,
        },
    ) {
        crate::agent_detection_policy::DetectionPolicyDecision::Publish(detection) => detection,
        crate::agent_detection_policy::DetectionPolicyDecision::Freeze => {
            pending_idle.clear();
            pending_working.clear();
            post_taint_working.clear();
            return DetectionPublishDecision::NoPublish;
        }
    };
    let new_state = crate::terminal::state::stabilize_agent_detection(
        input.agent,
        input.current_state,
        detection,
        input.process_exited,
        input.now,
        input.last_claude_working_at,
    );
    let visible_blocker = detection.visible_blocker && new_state == AgentState::Blocked;
    let visible_idle = detection.visible_idle && new_state == AgentState::Idle;
    let visible_working = detection.visible_working && new_state == AgentState::Working;
    let previous = DetectionPublishState {
        state: input.current_state,
        visible_blocker: input.last_visible_blocker,
        visible_idle: input.last_visible_idle,
        visible_working: input.last_visible_working,
    };
    let next = DetectionPublishState {
        state: new_state,
        visible_blocker,
        visible_idle,
        visible_working,
    };
    let stable_refresh_due = stable_visible_signal_refresh_due(
        previous,
        next,
        input.last_visible_signal_refresh,
        input.now,
    );
    decide_transition(
        input.agent,
        previous,
        next,
        input.agent_changed,
        input.process_exited,
        input.pty_activity,
        stable_refresh_due,
        input.content,
        input.now,
        pending_idle,
        pending_working,
        post_taint_working,
    )
}

pub(super) fn handle_skipped_detection_update(
    state: AgentState,
    pty_signal: Option<PtyActivitySignal>,
    post_taint_working: &mut PostTaintWorkingLease,
    tracker: &mut PtyCausalityTracker,
    pty_output_seq: u64,
    input_seq: u64,
    now: std::time::Instant,
) {
    if state == AgentState::Working {
        if pty_signal.is_some_and(|signal| signal.taint_just_ended) {
            post_taint_working.start(now);
        }
        return;
    }
    post_taint_working.clear();
    baseline_pty_causality(tracker, pty_output_seq, input_seq);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_state(state: AgentState) -> DetectionPublishState {
        DetectionPublishState {
            state,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
        }
    }

    fn pty_activity(active: bool, fresh_output: bool, output_seq: u64) -> PtyActivitySignal {
        PtyActivitySignal {
            active,
            tainted: false,
            taint_just_ended: false,
            fresh_output,
            output_seq,
        }
    }

    fn pty_activity_after_taint(output_seq: u64) -> PtyActivitySignal {
        PtyActivitySignal {
            active: false,
            tainted: false,
            taint_just_ended: true,
            fresh_output: false,
            output_seq,
        }
    }

    fn screen_detection(state: AgentState) -> AgentDetection {
        AgentDetection {
            state,
            visible_blocker: false,
            visible_idle: state == AgentState::Idle,
            visible_working: state == AgentState::Working,
        }
    }

    #[test]
    fn pending_idle_holds_working_to_plain_idle_until_confirmed() {
        let now = std::time::Instant::now();
        let previous = publish_state(AgentState::Working);
        let next = publish_state(AgentState::Idle);
        let mut pending = PendingIdleConfirmation::default();

        assert!(pending.should_hold_working_to_idle(previous, next, false, false, None, now));
        assert!(pending.should_hold_working_to_idle(
            previous,
            next,
            false,
            false,
            None,
            now + AGENT_PENDING_IDLE_RECHECK
        ));
        assert!(pending.should_hold_working_to_idle(
            previous,
            next,
            false,
            false,
            None,
            now + AGENT_PENDING_IDLE_RECHECK * 2
        ));
        assert!(!pending.should_hold_working_to_idle(
            previous,
            next,
            false,
            false,
            None,
            now + AGENT_PENDING_IDLE_RECHECK * 3
        ));
    }

    #[test]
    fn pending_working_requires_confirmed_pty_output() {
        let now = std::time::Instant::now();
        let idle = publish_state(AgentState::Idle);
        let working = publish_state(AgentState::Working);
        let mut pending = PendingWorkingConfirmation::default();

        assert!(pending.should_hold_idle_to_working(
            idle,
            working,
            false,
            false,
            Some(pty_activity(true, true, 10)),
            now
        ));
        assert!(pending.active());
        assert!(!pending.should_hold_idle_to_working(
            idle,
            working,
            false,
            false,
            Some(pty_activity(true, true, 11)),
            now + AGENT_PENDING_WORKING_CONFIRM_DELAY
        ));
        assert!(!pending.active());
    }

    #[test]
    fn pty_activity_lease_survives_sparse_heartbeat_jitter() {
        let now = std::time::Instant::now();
        let mut tracker = PtyCausalityTracker::default();
        baseline_pty_causality(&mut tracker, 1, 1);

        let first = agent_caused_pty_activity_active(2, 1, &mut tracker, now);
        assert!(first.active);
        assert!(first.fresh_output);

        let held = agent_caused_pty_activity_active(
            2,
            1,
            &mut tracker,
            now + std::time::Duration::from_millis(1700),
        );
        assert!(held.active);
        assert!(!held.fresh_output);

        let expired = agent_caused_pty_activity_active(
            2,
            1,
            &mut tracker,
            now + std::time::Duration::from_millis(1801),
        );
        assert!(!expired.active);
    }

    #[test]
    fn input_taint_discards_pty_activity_until_fresh_post_taint_output() {
        let now = std::time::Instant::now();
        let mut tracker = PtyCausalityTracker::default();
        baseline_pty_causality(&mut tracker, 1, 1);

        let tainted = agent_caused_pty_activity_active(2, 2, &mut tracker, now);
        assert!(!tainted.active);
        assert!(tainted.tainted);

        let after_taint = agent_caused_pty_activity_active(
            2,
            2,
            &mut tracker,
            now + AGENT_INPUT_TAINT_WINDOW + std::time::Duration::from_millis(1),
        );
        assert!(!after_taint.active);
        assert!(after_taint.taint_just_ended);

        let fresh = agent_caused_pty_activity_active(
            3,
            2,
            &mut tracker,
            now + AGENT_INPUT_TAINT_WINDOW + std::time::Duration::from_millis(2),
        );
        assert!(fresh.active);
    }

    #[test]
    fn pty_output_activity_tracks_nonempty_reads() {
        let seq = AtomicU64::new(0);
        observe_pty_output_activity(b"", &seq);
        assert_eq!(seq.load(Ordering::Relaxed), 0);
        observe_pty_output_activity(b"bytes", &seq);
        assert_eq!(seq.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn active_pty_transitions_to_working_without_screen_authority() {
        let now = std::time::Instant::now();
        let mut pending_idle = PendingIdleConfirmation::default();
        let mut pending_working = PendingWorkingConfirmation::default();
        let mut post_taint_working = PostTaintWorkingLease::default();

        assert_eq!(
            decide_pty_working_publish_without_screen(
                PtyWorkingPublishInput {
                    agent: Some(Agent::Codex),
                    current_state: AgentState::Idle,
                    last_visible_blocker: false,
                    last_visible_idle: true,
                    last_visible_working: false,
                    last_visible_signal_refresh: None,
                    pty_activity: Some(pty_activity(true, true, 10)),
                    now,
                },
                &mut pending_idle,
                &mut pending_working,
                &mut post_taint_working,
            ),
            DetectionPublishDecision::NoPublish
        );

        assert_eq!(
            decide_pty_working_publish_without_screen(
                PtyWorkingPublishInput {
                    agent: Some(Agent::Codex),
                    current_state: AgentState::Idle,
                    last_visible_blocker: false,
                    last_visible_idle: true,
                    last_visible_working: false,
                    last_visible_signal_refresh: None,
                    pty_activity: Some(pty_activity(true, true, 11)),
                    now: now + AGENT_PENDING_WORKING_CONFIRM_DELAY,
                },
                &mut pending_idle,
                &mut pending_working,
                &mut post_taint_working,
            ),
            DetectionPublishDecision::Publish {
                state: AgentState::Working,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
            }
        );
    }

    #[test]
    fn active_pty_overrides_stale_idle_screen_detection() {
        let now = std::time::Instant::now();
        let mut pending_idle = PendingIdleConfirmation::default();
        let mut pending_working = PendingWorkingConfirmation::default();
        let mut post_taint_working = PostTaintWorkingLease::default();
        let mut last_claude = None;

        assert_eq!(
            decide_screen_detection_publish(
                ScreenDetectionPublishInput {
                    agent: Some(Agent::Codex),
                    current_state: AgentState::Working,
                    last_visible_blocker: false,
                    last_visible_idle: true,
                    last_visible_working: false,
                    last_visible_signal_refresh: None,
                    screen_detection: screen_detection(AgentState::Idle),
                    process_exited: false,
                    agent_changed: false,
                    pty_activity: Some(pty_activity(true, true, 10)),
                    content: "stale prompt",
                    now,
                    last_claude_working_at: &mut last_claude,
                },
                &mut pending_idle,
                &mut pending_working,
                &mut post_taint_working,
            ),
            DetectionPublishDecision::Publish {
                state: AgentState::Working,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
            }
        );
    }

    #[test]
    fn post_taint_lease_holds_working_before_idle_fallback() {
        let now = std::time::Instant::now();
        let previous = publish_state(AgentState::Working);
        let idle = publish_state(AgentState::Idle);
        let mut lease = PostTaintWorkingLease::default();

        assert!(lease.should_hold_working_to_idle(
            previous,
            idle,
            false,
            false,
            Some(pty_activity_after_taint(10)),
            now
        ));
        assert!(lease.should_hold_working_to_idle(
            previous,
            idle,
            false,
            false,
            Some(pty_activity(false, false, 10)),
            now + POST_TAINT_WORKING_LEASE - std::time::Duration::from_millis(1)
        ));
    }
}
