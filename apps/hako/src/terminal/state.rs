use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// Effective state arbitration is intentionally centralized here. Hooks are the
// default authority for agent-owned internal state, but a narrow set of strong
// visible screen signals can veto stale hook reports. Precedence is: strong
// visible working for the same agent > hook blocked > strong visible blocker >
// Claude visible idle > hook > fallback. Process-exit updates clear matching
// hook authority before recomputing state.

use crate::detect::{Agent, AgentState};
use crate::terminal::TerminalId;

#[path = "metadata.rs"]
mod metadata;
pub use metadata::{AgentMetadata, AgentMetadataReport, EffectivePresentation};

const CLAUDE_WORKING_HOLD: Duration = Duration::from_millis(1200);
const STALE_HOOK_IDLE_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookAuthority {
    pub source: String,
    pub agent_label: String,
    pub state: AgentState,
    pub message: Option<String>,
    pub custom_status: Option<String>,
    pub reported_at: Instant,
    pub session_ref: Option<crate::agent_resume::AgentSessionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HookAuthoritySnapshot {
    pub source: String,
    pub agent_label: String,
    pub state: AgentState,
    pub message: Option<String>,
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_ref: Option<AgentSessionRefSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentSessionRefSnapshot {
    pub kind: crate::agent_resume::AgentSessionRefKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AgentMetadataSnapshot {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies_to_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_labels: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_remaining_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerminalSemanticSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_agent: Option<Agent>,
    pub fallback_state: AgentState,
    #[serde(default)]
    pub fallback_visible_blocker: bool,
    #[serde(default)]
    pub fallback_visible_idle: bool,
    #[serde(default)]
    pub fallback_visible_working: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_authority: Option<HookAuthoritySnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_metadata: Vec<AgentMetadataSnapshot>,
    pub state: AgentState,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub hook_report_sequences: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata_report_sequences: HashMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_meaningful_agent_activity_unix_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveStateChange {
    pub previous_agent_label: Option<String>,
    pub previous_known_agent: Option<Agent>,
    pub previous_state: AgentState,
    pub previous_presentation: EffectivePresentation,
    pub agent_label: Option<String>,
    pub known_agent: Option<Agent>,
    pub state: AgentState,
    pub presentation: EffectivePresentation,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalStateMutation {
    pub effective_state_change: Option<EffectiveStateChange>,
    pub session_ref_changed: bool,
}

/// Pure state for a server-owned terminal.
///
/// During the migration this is still one-to-one with a pane-backed PTY, but
/// pane/view state no longer owns terminal identity, cwd, labels, or agent
/// metadata.
#[derive(Clone)]
pub struct TerminalState {
    pub id: TerminalId,
    pub cwd: PathBuf,
    pub detected_agent: Option<Agent>,
    pub fallback_state: AgentState,
    fallback_visible_blocker: bool,
    fallback_visible_idle: bool,
    fallback_visible_working: bool,
    fallback_observed_at: Option<Instant>,
    stale_hook_idle_since: Option<Instant>,
    pub hook_authority: Option<HookAuthority>,
    pub agent_metadata: HashMap<String, AgentMetadata>,
    pub persisted_agent_session: Option<crate::agent_resume::PersistedAgentSession>,
    pub manual_label: Option<String>,
    pub agent_name: Option<String>,
    hook_report_sequences: HashMap<String, u64>,
    metadata_report_sequences: HashMap<String, u64>,
    pub state: AgentState,
    pub revision: u64,
    pub launch_argv: Option<Vec<String>>,
    pub launch_env: Vec<(String, String)>,
    pub respawn_shell_on_exit: bool,
    pub pending_agent_resume_plan: Option<crate::agent_resume::AgentResumePlan>,
    last_meaningful_agent_activity_seq: u64,
    last_meaningful_agent_activity_unix_secs: Option<u64>,
    missing_integration_warning_reported_for: Option<Agent>,
}

impl TerminalState {
    pub fn new(id: TerminalId, cwd: PathBuf) -> Self {
        Self {
            id,
            cwd,
            detected_agent: None,
            fallback_state: AgentState::Unknown,
            fallback_visible_blocker: false,
            fallback_visible_idle: false,
            fallback_visible_working: false,
            fallback_observed_at: None,
            stale_hook_idle_since: None,
            hook_authority: None,
            agent_metadata: HashMap::new(),
            persisted_agent_session: None,
            manual_label: None,
            agent_name: None,
            hook_report_sequences: HashMap::new(),
            metadata_report_sequences: HashMap::new(),
            state: AgentState::Unknown,
            revision: 0,
            launch_argv: None,
            launch_env: Vec::new(),
            respawn_shell_on_exit: false,
            pending_agent_resume_plan: None,
            last_meaningful_agent_activity_seq: 0,
            last_meaningful_agent_activity_unix_secs: None,
            missing_integration_warning_reported_for: None,
        }
    }

    pub fn capture_semantic_snapshot(&self) -> Option<TerminalSemanticSnapshot> {
        let now = Instant::now();
        let agent_metadata = self.capture_agent_metadata_snapshots_at(now);
        let carries_semantics = self.detected_agent.is_some()
            || self.fallback_state != AgentState::Unknown
            || self.hook_authority.is_some()
            || !agent_metadata.is_empty()
            || self.state != AgentState::Unknown
            || !self.hook_report_sequences.is_empty()
            || self.last_meaningful_agent_activity_unix_secs.is_some()
            || !self.metadata_report_sequences.is_empty();

        carries_semantics.then(|| TerminalSemanticSnapshot {
            detected_agent: self.detected_agent,
            fallback_state: self.fallback_state,
            fallback_visible_blocker: self.fallback_visible_blocker,
            fallback_visible_idle: self.fallback_visible_idle,
            fallback_visible_working: self.fallback_visible_working,
            hook_authority: self
                .hook_authority
                .as_ref()
                .map(|authority| HookAuthoritySnapshot {
                    source: authority.source.clone(),
                    agent_label: authority.agent_label.clone(),
                    state: authority.state,
                    message: authority.message.clone(),
                    custom_status: authority.custom_status.clone(),
                    session_ref: authority.session_ref.as_ref().map(|session_ref| {
                        AgentSessionRefSnapshot {
                            kind: session_ref.kind,
                            value: session_ref.value.clone(),
                        }
                    }),
                }),
            agent_metadata,
            state: self.state,
            revision: self.revision,
            hook_report_sequences: self.hook_report_sequences.clone(),
            metadata_report_sequences: self.metadata_report_sequences.clone(),
            last_meaningful_agent_activity_unix_secs: self.last_meaningful_agent_activity_unix_secs,
        })
    }

    pub fn restore_semantic_snapshot(&mut self, snapshot: TerminalSemanticSnapshot) {
        let now = Instant::now();
        self.detected_agent = snapshot.detected_agent;
        self.fallback_state = snapshot.fallback_state;
        self.fallback_visible_blocker =
            snapshot.fallback_visible_blocker && snapshot.fallback_state == AgentState::Blocked;
        self.fallback_visible_idle =
            snapshot.fallback_visible_idle && snapshot.fallback_state == AgentState::Idle;
        self.fallback_visible_working =
            snapshot.fallback_visible_working && snapshot.fallback_state == AgentState::Working;
        self.fallback_observed_at = (snapshot.detected_agent.is_some()
            || snapshot.fallback_state != AgentState::Unknown)
            .then_some(now);
        self.stale_hook_idle_since = None;
        self.hook_authority = snapshot.hook_authority.map(|authority| HookAuthority {
            source: authority.source,
            agent_label: authority.agent_label,
            state: authority.state,
            message: authority.message,
            custom_status: authority.custom_status,
            reported_at: now,
            session_ref: authority.session_ref.map(|session_ref| {
                crate::agent_resume::AgentSessionRef {
                    kind: session_ref.kind,
                    value: session_ref.value,
                }
            }),
        });
        self.restore_agent_metadata_snapshots_at(snapshot.agent_metadata, now);
        self.state = snapshot.state;
        self.revision = snapshot.revision;
        self.hook_report_sequences = snapshot.hook_report_sequences;
        self.metadata_report_sequences = snapshot.metadata_report_sequences;
        self.last_meaningful_agent_activity_unix_secs =
            snapshot.last_meaningful_agent_activity_unix_secs;
    }

    pub fn last_meaningful_agent_activity_seq(&self) -> u64 {
        self.last_meaningful_agent_activity_seq
    }

    pub fn last_meaningful_agent_activity_unix_secs(&self) -> Option<u64> {
        self.last_meaningful_agent_activity_unix_secs
    }

    pub fn mark_meaningful_agent_activity(&mut self, seq: u64, unix_secs: u64) {
        self.last_meaningful_agent_activity_seq = seq;
        self.last_meaningful_agent_activity_unix_secs = Some(unix_secs);
    }

    pub fn with_launch_argv(mut self, argv: Vec<String>) -> Self {
        self.launch_argv = Some(argv);
        self
    }

    pub fn with_launch_env(mut self, env: Vec<(String, String)>) -> Self {
        self.launch_env = env;
        self
    }

    pub fn with_respawn_shell_on_exit(mut self) -> Self {
        self.respawn_shell_on_exit = true;
        self
    }

    pub fn with_pending_agent_resume_plan(
        mut self,
        plan: crate::agent_resume::AgentResumePlan,
    ) -> Self {
        self.pending_agent_resume_plan = Some(plan);
        self
    }

    #[cfg(test)]
    pub fn set_detected_state(
        &mut self,
        agent: Option<Agent>,
        fallback_state: AgentState,
    ) -> Option<EffectiveStateChange> {
        self.set_detected_state_with_visible_blocker(agent, fallback_state, false, false, false)
    }

    #[cfg(test)]
    pub fn set_detected_state_with_mutation(
        &mut self,
        agent: Option<Agent>,
        fallback_state: AgentState,
    ) -> TerminalStateMutation {
        self.set_detected_state_with_screen_signals_at(
            agent,
            fallback_state,
            false,
            false,
            false,
            false,
            Instant::now(),
        )
    }

    #[cfg(test)]
    pub fn set_detected_state_with_visible_blocker(
        &mut self,
        agent: Option<Agent>,
        fallback_state: AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        process_exited: bool,
    ) -> Option<EffectiveStateChange> {
        self.set_detected_state_with_screen_signals_at(
            agent,
            fallback_state,
            visible_blocker,
            visible_idle,
            false,
            process_exited,
            Instant::now(),
        )
        .effective_state_change
    }

    pub fn set_detected_state_with_screen_signals_at(
        &mut self,
        agent: Option<Agent>,
        fallback_state: AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
        now: Instant,
    ) -> TerminalStateMutation {
        let previous_agent_label = self.effective_agent_label().map(str::to_string);
        let previous_known_agent = self.effective_known_agent();
        let previous_state = self.state;
        let previous_presentation = self.effective_presentation_for_state_at(previous_state, now);
        let previous_detected_agent = self.detected_agent;
        let previous_session = self.current_session_identity_for_persistence();

        self.detected_agent = agent;
        self.fallback_state = fallback_state;
        self.fallback_visible_blocker = visible_blocker && fallback_state == AgentState::Blocked;
        self.fallback_visible_idle = visible_idle && fallback_state == AgentState::Idle;
        self.fallback_visible_working = visible_working && fallback_state == AgentState::Working;
        self.fallback_observed_at = Some(now);
        if process_exited
            && self.hook_authority_not_newer_than(now)
            && self.hook_authority.as_ref().is_some_and(|authority| {
                crate::detect::parse_agent_label(&authority.agent_label) == agent
            })
        {
            self.hook_authority = None;
            self.stale_hook_idle_since = None;
        }
        if self.hook_authority_not_newer_than(now)
            && (self.hook_authority_conflicts_with_detected_agent(agent)
                || (previous_detected_agent.is_some()
                    && agent != previous_detected_agent
                    && self.hook_authority.as_ref().is_some_and(|authority| {
                        crate::detect::parse_agent_label(&authority.agent_label)
                            == previous_detected_agent
                    })))
        {
            self.hook_authority = None;
            self.stale_hook_idle_since = None;
        }
        let detected_agent_changed_or_disappeared =
            previous_detected_agent.is_some() && agent != previous_detected_agent;
        let persisted_agent_was_previously_detected =
            self.persisted_agent_session_belongs_to_detected_agent(previous_detected_agent);
        if self.persisted_agent_session_conflicts_with_detected_agent(agent)
            || detected_agent_changed_or_disappeared && persisted_agent_was_previously_detected
        {
            self.persisted_agent_session = None;
        }
        if process_exited || agent != previous_detected_agent {
            self.missing_integration_warning_reported_for = None;
        }
        self.update_stale_hook_idle_window(now);
        TerminalStateMutation {
            effective_state_change: self.recompute_effective_state(
                previous_agent_label,
                previous_known_agent,
                previous_state,
                previous_presentation,
                now,
            ),
            session_ref_changed: previous_session
                != self.current_session_identity_for_persistence(),
        }
    }

    #[cfg(test)]
    pub fn set_hook_authority(
        &mut self,
        source: String,
        agent_label: String,
        state: AgentState,
        message: Option<String>,
        seq: Option<u64>,
    ) -> Option<EffectiveStateChange> {
        self.set_hook_authority_with_custom_status(source, agent_label, state, message, None, seq)
    }

    #[cfg(test)]
    pub fn set_hook_authority_with_custom_status(
        &mut self,
        source: String,
        agent_label: String,
        state: AgentState,
        message: Option<String>,
        custom_status: Option<String>,
        seq: Option<u64>,
    ) -> Option<EffectiveStateChange> {
        self.set_hook_authority_with_custom_status_at(
            source,
            agent_label,
            state,
            message,
            custom_status,
            None,
            seq,
            Instant::now(),
        )
        .and_then(|mutation| mutation.effective_state_change)
    }

    pub fn set_hook_authority_with_session_ref(
        &mut self,
        source: String,
        agent_label: String,
        state: AgentState,
        message: Option<String>,
        custom_status: Option<String>,
        session_ref: Option<crate::agent_resume::AgentSessionRef>,
        seq: Option<u64>,
    ) -> Option<TerminalStateMutation> {
        self.set_hook_authority_with_custom_status_at(
            source,
            agent_label,
            state,
            message,
            custom_status,
            session_ref,
            seq,
            Instant::now(),
        )
    }

    pub fn set_agent_session_ref(
        &mut self,
        source: String,
        agent_label: String,
        session_ref: Option<crate::agent_resume::AgentSessionRef>,
        seq: Option<u64>,
    ) -> Option<TerminalStateMutation> {
        let reset_sequence =
            self.should_reset_hook_sequence_for_new_session(&source, session_ref.as_ref());
        if !self.hook_sequence_allows(&source, seq, reset_sequence) {
            return None;
        }
        let session_ref = session_ref?;
        if self.known_agent_label_conflicts_with_detected_agent(&agent_label) {
            return None;
        }
        if self
            .conflicting_current_session_ref(&source, &agent_label, &session_ref)
            .is_some()
        {
            return None;
        }

        let previous_session = self.current_session_identity_for_persistence();
        self.persisted_agent_session = Some(crate::agent_resume::PersistedAgentSession {
            source,
            agent: agent_label,
            session_ref,
        });
        let current_session = self.current_session_identity_for_persistence();
        Some(TerminalStateMutation {
            effective_state_change: None,
            session_ref_changed: previous_session != current_session,
        })
    }

    pub fn set_hook_authority_with_custom_status_at(
        &mut self,
        source: String,
        agent_label: String,
        state: AgentState,
        message: Option<String>,
        custom_status: Option<String>,
        session_ref: Option<crate::agent_resume::AgentSessionRef>,
        seq: Option<u64>,
        now: Instant,
    ) -> Option<TerminalStateMutation> {
        let reset_sequence =
            self.should_reset_hook_sequence_for_new_session(&source, session_ref.as_ref());
        if !self.hook_sequence_allows(&source, seq, reset_sequence) {
            return None;
        }

        let previous_agent_label = self.effective_agent_label().map(str::to_string);
        let previous_known_agent = self.effective_known_agent();
        let previous_state = self.state;
        let previous_presentation = self.effective_presentation_for_state_at(previous_state, now);
        let previous_session = self.current_session_identity_for_persistence();
        if self.known_agent_label_conflicts_with_detected_agent(&agent_label) {
            return None;
        }
        let session_ref = self.resolved_report_session_ref(&source, &agent_label, session_ref);
        self.commit_hook_sequence(&source, seq, reset_sequence);
        self.persisted_agent_session = None;
        self.hook_authority = Some(HookAuthority {
            source,
            agent_label,
            state,
            message,
            custom_status,
            reported_at: now,
            session_ref,
        });
        self.stale_hook_idle_since = None;
        let current_session = self.current_session_identity_for_persistence();
        Some(TerminalStateMutation {
            effective_state_change: self.recompute_effective_state(
                previous_agent_label,
                previous_known_agent,
                previous_state,
                previous_presentation,
                now,
            ),
            session_ref_changed: previous_session != current_session,
        })
    }

    fn hook_authority_not_newer_than(&self, observed_at: Instant) -> bool {
        self.hook_authority
            .as_ref()
            .is_none_or(|authority| authority.reported_at <= observed_at)
    }

    fn fallback_not_older_than_hook(&self) -> bool {
        self.hook_authority.as_ref().is_none_or(|authority| {
            self.fallback_observed_at
                .is_some_and(|observed_at| authority.reported_at <= observed_at)
        })
    }

    fn hook_authority_conflicts_with_detected_agent(&self, detected_agent: Option<Agent>) -> bool {
        let Some(detected_agent) = detected_agent else {
            return false;
        };
        self.hook_authority.as_ref().is_some_and(|authority| {
            crate::detect::parse_agent_label(&authority.agent_label)
                .is_some_and(|hook_agent| hook_agent != detected_agent)
        })
    }

    fn persisted_agent_session_conflicts_with_detected_agent(
        &self,
        detected_agent: Option<Agent>,
    ) -> bool {
        let Some(detected_agent) = detected_agent else {
            return false;
        };
        self.persisted_agent_session
            .as_ref()
            .and_then(|session| crate::detect::parse_agent_label(&session.agent))
            .is_some_and(|agent| agent != detected_agent)
    }

    fn persisted_agent_session_belongs_to_detected_agent(
        &self,
        detected_agent: Option<Agent>,
    ) -> bool {
        let Some(detected_agent) = detected_agent else {
            return false;
        };
        self.persisted_agent_session
            .as_ref()
            .and_then(|session| crate::detect::parse_agent_label(&session.agent))
            .is_some_and(|agent| agent == detected_agent)
    }

    fn persisted_agent_session_matches(&self, source: &str, agent: &str) -> bool {
        self.persisted_agent_session
            .as_ref()
            .is_some_and(|session| session.source == source && session.agent == agent)
    }

    fn current_session_identity_for_persistence(
        &self,
    ) -> Option<(
        String,
        String,
        crate::agent_resume::AgentSessionRefKind,
        String,
    )> {
        if let Some(authority) = self.hook_authority.as_ref() {
            if let Some(session_ref) = authority.session_ref.as_ref() {
                return Some((
                    authority.source.clone(),
                    authority.agent_label.clone(),
                    session_ref.kind,
                    session_ref.value.clone(),
                ));
            }
        }
        self.persisted_agent_session.as_ref().map(|session| {
            (
                session.source.clone(),
                session.agent.clone(),
                session.session_ref.kind,
                session.session_ref.value.clone(),
            )
        })
    }

    fn resolved_report_session_ref(
        &self,
        source: &str,
        agent_label: &str,
        session_ref: Option<crate::agent_resume::AgentSessionRef>,
    ) -> Option<crate::agent_resume::AgentSessionRef> {
        if source == "hako:omp" && agent_label == "omp" {
            let current_ref = self.current_matching_session_ref(source, agent_label);
            if let Some(session_ref) = session_ref {
                if self.reported_omp_subagent_ref_would_replace_current(&session_ref, current_ref) {
                    return current_ref.cloned();
                }
                return Some(
                    self.conflicting_current_session_ref(source, agent_label, &session_ref)
                        .unwrap_or(session_ref),
                );
            }
            return current_ref
                .filter(|session_ref| {
                    Self::session_ref_available_for_report(source, agent_label, session_ref)
                })
                .cloned();
        }

        session_ref.map(|session_ref| {
            self.conflicting_current_session_ref(source, agent_label, &session_ref)
                .unwrap_or(session_ref)
        })
    }

    fn current_matching_session_ref(
        &self,
        source: &str,
        agent_label: &str,
    ) -> Option<&crate::agent_resume::AgentSessionRef> {
        self.hook_authority
            .as_ref()
            .and_then(|authority| {
                (authority.source == source && authority.agent_label == agent_label)
                    .then_some(authority.session_ref.as_ref())
                    .flatten()
            })
            .or_else(|| {
                self.persisted_agent_session.as_ref().and_then(|session| {
                    (session.source == source && session.agent == agent_label)
                        .then_some(&session.session_ref)
                })
            })
    }

    fn session_ref_available_for_report(
        source: &str,
        agent_label: &str,
        session_ref: &crate::agent_resume::AgentSessionRef,
    ) -> bool {
        if source == "hako:omp"
            && agent_label == "omp"
            && session_ref.kind == crate::agent_resume::AgentSessionRefKind::Path
        {
            return std::path::Path::new(&session_ref.value).is_file();
        }
        true
    }

    fn reported_omp_subagent_ref_would_replace_current(
        &self,
        session_ref: &crate::agent_resume::AgentSessionRef,
        current_ref: Option<&crate::agent_resume::AgentSessionRef>,
    ) -> bool {
        current_ref.is_some_and(|current_ref| {
            Self::session_ref_available_for_report("hako:omp", "omp", current_ref)
                && current_ref != session_ref
                && Self::is_distinguishable_omp_subagent_ref(session_ref)
        })
    }

    fn is_distinguishable_omp_subagent_ref(
        session_ref: &crate::agent_resume::AgentSessionRef,
    ) -> bool {
        if session_ref.kind != crate::agent_resume::AgentSessionRefKind::Path {
            return false;
        }
        let path = std::path::Path::new(&session_ref.value);
        if path.extension() != Some(std::ffi::OsStr::new("jsonl")) {
            return false;
        }
        let Some(parent_dir) = path.parent() else {
            return false;
        };
        let Some(parent_name) = parent_dir.file_name() else {
            return false;
        };
        let Some(project_dir) = parent_dir.parent() else {
            return false;
        };
        project_dir
            .join(parent_name)
            .with_extension("jsonl")
            .is_file()
    }

    fn conflicting_current_session_ref(
        &self,
        source: &str,
        agent_label: &str,
        session_ref: &crate::agent_resume::AgentSessionRef,
    ) -> Option<crate::agent_resume::AgentSessionRef> {
        if session_ref.kind != crate::agent_resume::AgentSessionRefKind::Id {
            return None;
        }

        let current_ref = self
            .hook_authority
            .as_ref()
            .and_then(|authority| {
                (authority.source == source && authority.agent_label == agent_label)
                    .then_some(authority.session_ref.as_ref())
                    .flatten()
            })
            .or_else(|| {
                self.persisted_agent_session.as_ref().and_then(|session| {
                    (session.source == source && session.agent == agent_label)
                        .then_some(&session.session_ref)
                })
            })?;

        (current_ref.kind == crate::agent_resume::AgentSessionRefKind::Id
            && current_ref.value != session_ref.value)
            .then(|| current_ref.clone())
    }

    pub fn set_persisted_agent_session(
        &mut self,
        session: crate::agent_resume::PersistedAgentSession,
    ) {
        self.persisted_agent_session = Some(session);
    }

    fn known_agent_label_conflicts_with_detected_agent(&self, agent_label: &str) -> bool {
        let Some(detected_agent) = self.detected_agent else {
            return false;
        };
        crate::detect::parse_agent_label(agent_label)
            .is_some_and(|hook_agent| hook_agent != detected_agent)
    }
    fn should_reset_hook_sequence_for_new_session(
        &self,
        source: &str,
        session_ref: Option<&crate::agent_resume::AgentSessionRef>,
    ) -> bool {
        let Some(session_ref) = session_ref else {
            return false;
        };

        if self.hook_authority.as_ref().is_some_and(|authority| {
            authority.source == source && authority.state == AgentState::Working
        }) {
            return false;
        }

        let current_ref = self.hook_authority.as_ref().and_then(|authority| {
            (authority.source == source)
                .then_some(authority.session_ref.as_ref())
                .flatten()
        });
        let persisted_ref = self
            .persisted_agent_session
            .as_ref()
            .and_then(|session| (session.source == source).then_some(&session.session_ref));

        current_ref
            .or(persisted_ref)
            .is_some_and(|current| current != session_ref)
    }

    fn hook_sequence_allows(&self, source: &str, seq: Option<u64>, reset_sequence: bool) -> bool {
        let Some(seq) = seq else {
            return reset_sequence || !self.hook_report_sequences.contains_key(source);
        };

        reset_sequence
            || self
                .hook_report_sequences
                .get(source)
                .is_none_or(|last_seq| seq > *last_seq)
    }

    fn commit_hook_sequence(&mut self, source: &str, seq: Option<u64>, reset_sequence: bool) {
        if reset_sequence {
            self.hook_report_sequences.remove(source);
        }
        if let Some(seq) = seq {
            self.hook_report_sequences.insert(source.to_string(), seq);
        }
    }

    #[cfg(test)]
    pub fn clear_hook_authority(
        &mut self,
        source: Option<&str>,
        seq: Option<u64>,
    ) -> Option<EffectiveStateChange> {
        self.clear_hook_authority_with_mutation(source, seq)
            .and_then(|mutation| mutation.effective_state_change)
    }

    pub fn clear_hook_authority_with_mutation(
        &mut self,
        source: Option<&str>,
        seq: Option<u64>,
    ) -> Option<TerminalStateMutation> {
        let sequence_source = source.map(str::to_string).or_else(|| {
            self.hook_authority
                .as_ref()
                .map(|authority| authority.source.clone())
        });
        let should_clear = self
            .hook_authority
            .as_ref()
            .is_some_and(|authority| source.is_none_or(|source| authority.source == source));
        if !should_clear {
            return None;
        }
        if let Some(source) = sequence_source.as_deref() {
            if !self.hook_sequence_allows(source, seq, false) {
                return None;
            }
        }

        let now = Instant::now();
        let previous_agent_label = self.effective_agent_label().map(str::to_string);
        let previous_known_agent = self.effective_known_agent();
        let previous_state = self.state;
        let previous_presentation = self.effective_presentation_for_state_at(previous_state, now);
        let previous_session = self.current_session_identity_for_persistence();
        if let Some(source) = sequence_source.as_deref() {
            self.commit_hook_sequence(source, seq, false);
        }
        self.hook_authority = None;
        self.stale_hook_idle_since = None;
        self.persisted_agent_session = None;
        Some(TerminalStateMutation {
            effective_state_change: self.recompute_effective_state(
                previous_agent_label,
                previous_known_agent,
                previous_state,
                previous_presentation,
                now,
            ),
            session_ref_changed: previous_session.is_some(),
        })
    }
    #[cfg(test)]
    pub fn release_agent(
        &mut self,
        source: &str,
        agent_label: &str,
        seq: Option<u64>,
    ) -> Option<EffectiveStateChange> {
        self.release_agent_with_mutation(source, agent_label, None, seq)
            .and_then(|mutation| mutation.effective_state_change)
    }

    pub fn release_agent_with_mutation(
        &mut self,
        source: &str,
        agent_label: &str,
        session_ref: Option<crate::agent_resume::AgentSessionRef>,
        seq: Option<u64>,
    ) -> Option<TerminalStateMutation> {
        if self.hook_authority.as_ref().is_some_and(|authority| {
            authority.agent_label != agent_label || authority.source != source
        }) {
            return None;
        }

        let matches_current_agent = self.effective_agent_label() == Some(agent_label);
        let matches_persisted_session = self.persisted_agent_session_matches(source, agent_label);
        if !matches_current_agent && !matches_persisted_session {
            return None;
        }
        if !self.release_session_matches(source, session_ref.as_ref()) {
            return None;
        }
        if !self.hook_sequence_allows(source, seq, false) {
            return None;
        }

        let now = Instant::now();
        let previous_agent_label = self.effective_agent_label().map(str::to_string);
        let previous_known_agent = self.effective_known_agent();
        let previous_state = self.state;
        let previous_presentation = self.effective_presentation_for_state_at(previous_state, now);
        let previous_session = self.current_session_identity_for_persistence();
        self.commit_hook_sequence(source, seq, false);
        self.detected_agent = None;
        self.fallback_state = AgentState::Unknown;
        self.fallback_visible_blocker = false;
        self.fallback_visible_idle = false;
        self.fallback_visible_working = false;
        self.fallback_observed_at = None;
        self.hook_authority = None;
        self.stale_hook_idle_since = None;
        self.persisted_agent_session = None;
        self.missing_integration_warning_reported_for = None;
        Some(TerminalStateMutation {
            effective_state_change: self.recompute_effective_state(
                previous_agent_label,
                previous_known_agent,
                previous_state,
                previous_presentation,
                now,
            ),
            session_ref_changed: previous_session.is_some(),
        })
    }

    fn release_session_matches(
        &self,
        source: &str,
        session_ref: Option<&crate::agent_resume::AgentSessionRef>,
    ) -> bool {
        let current_ref = self.hook_authority.as_ref().and_then(|authority| {
            (authority.source == source)
                .then_some(authority.session_ref.as_ref())
                .flatten()
        });
        let persisted_ref = self
            .persisted_agent_session
            .as_ref()
            .and_then(|session| (session.source == source).then_some(&session.session_ref));

        current_ref
            .or(persisted_ref)
            .is_none_or(|current| session_ref.is_some_and(|release_ref| release_ref == current))
    }

    pub fn effective_agent_label(&self) -> Option<&str> {
        self.hook_authority
            .as_ref()
            .map(|authority| authority.agent_label.as_str())
            .or(self.agent_name.as_deref())
            .or_else(|| self.detected_agent.map(crate::detect::agent_label))
    }

    pub fn effective_known_agent(&self) -> Option<Agent> {
        if let Some(authority) = &self.hook_authority {
            return crate::detect::parse_agent_label(&authority.agent_label);
        }
        self.detected_agent.or_else(|| {
            self.agent_name
                .as_deref()
                .and_then(crate::detect::parse_agent_label)
        })
    }

    pub fn take_missing_integration_warning_agent(&mut self, now: Instant) -> Option<Agent> {
        let agent = self.detected_agent?;
        if !Self::agent_supports_hako_integration(agent)
            || self.has_hako_integration_evidence_for_agent_at(agent, now)
            || self.missing_integration_warning_reported_for == Some(agent)
        {
            return None;
        }

        self.missing_integration_warning_reported_for = Some(agent);
        Some(agent)
    }

    pub fn has_hako_integration_evidence_for_detected_agent_at(&self, now: Instant) -> bool {
        self.detected_agent
            .is_some_and(|agent| self.has_hako_integration_evidence_for_agent_at(agent, now))
    }

    fn has_hako_integration_evidence_for_agent_at(&self, agent: Agent, now: Instant) -> bool {
        self.hook_authority.as_ref().is_some_and(|authority| {
            Self::hako_report_identity_matches_agent(
                &authority.source,
                Some(&authority.agent_label),
                agent,
            )
        }) || self
            .persisted_agent_session
            .as_ref()
            .is_some_and(|session| {
                Self::hako_report_identity_matches_agent(
                    &session.source,
                    Some(&session.agent),
                    agent,
                )
            })
            || self.agent_metadata.values().any(|metadata| {
                self.agent_metadata_is_valid(metadata, now, true)
                    && Self::hako_report_identity_matches_agent(
                        &metadata.source,
                        metadata.agent_label.as_ref(),
                        agent,
                    )
            })
    }

    fn agent_supports_hako_integration(agent: Agent) -> bool {
        matches!(
            agent,
            Agent::Pi
                | Agent::OhMyPi
                | Agent::Claude
                | Agent::Codex
                | Agent::GithubCopilot
                | Agent::Devin
                | Agent::Kimi
                | Agent::Droid
                | Agent::Cursor
                | Agent::OpenCode
                | Agent::Hermes
                | Agent::Qodercli
        )
    }

    fn hako_report_identity_matches_agent(
        source: &str,
        agent_label: Option<&String>,
        agent: Agent,
    ) -> bool {
        let label = crate::detect::agent_label(agent);
        source == format!("hako:{label}")
            && agent_label.is_none_or(|agent_label| agent_label == label)
    }

    pub fn full_lifecycle_hook_authority_active(&self) -> bool {
        self.hook_authority.as_ref().is_some_and(|authority| {
            crate::detect::full_lifecycle_hook_authority(&authority.source, &authority.agent_label)
        })
    }

    fn visible_blocker_overrides_hook(&self) -> bool {
        self.fallback_visible_blocker
            && self.fallback_not_older_than_hook()
            && self.hook_authority.as_ref().is_some_and(|authority| {
                authority.state != AgentState::Blocked
                    && crate::detect::parse_agent_label(&authority.agent_label)
                        == self.detected_agent
            })
    }

    fn visible_working_overrides_hook(&self) -> bool {
        self.fallback_visible_working
            && self.fallback_not_older_than_hook()
            && self.hook_authority.as_ref().is_some_and(|authority| {
                matches!(authority.state, AgentState::Idle | AgentState::Blocked)
                    && crate::detect::parse_agent_label(&authority.agent_label)
                        == self.detected_agent
            })
    }

    fn visible_idle_stales_hook(&self, now: Instant) -> bool {
        self.stale_hook_idle_since
            .is_some_and(|since| now.duration_since(since) >= STALE_HOOK_IDLE_GRACE)
    }

    fn visible_idle_masks_hook_custom_status(&self, state: AgentState, now: Instant) -> bool {
        self.fallback_visible_idle
            && self.fallback_not_older_than_hook()
            && self.hook_authority.as_ref().is_some_and(|authority| {
                authority.state == AgentState::Working
                    && crate::detect::parse_agent_label(&authority.agent_label)
                        == self.detected_agent
            })
            && (state == AgentState::Idle || self.visible_idle_stales_hook(now))
    }

    fn update_stale_hook_idle_window(&mut self, now: Instant) {
        let visible_idle_stales_hook = self.fallback_visible_idle
            && self.fallback_not_older_than_hook()
            && self.hook_authority.as_ref().is_some_and(|authority| {
                authority.state == AgentState::Working
                    && crate::detect::parse_agent_label(&authority.agent_label)
                        == self.detected_agent
            });

        if visible_idle_stales_hook {
            self.stale_hook_idle_since.get_or_insert(now);
        } else {
            self.stale_hook_idle_since = None;
        }
    }

    pub fn set_manual_label(&mut self, label: String) {
        let label = label.trim().to_string();
        self.manual_label = (!label.is_empty()).then_some(label);
    }

    pub fn clear_manual_label(&mut self) {
        self.manual_label = None;
    }

    pub fn set_agent_name(&mut self, name: String) {
        let name = name.trim().to_string();
        self.agent_name = (!name.is_empty()).then_some(name);
    }

    pub fn clear_agent_name(&mut self) {
        self.agent_name = None;
    }

    pub fn clear_agent_runtime_identity_after_respawn(&mut self) {
        self.detected_agent = None;
        self.fallback_state = AgentState::Unknown;
        self.fallback_visible_blocker = false;
        self.fallback_visible_idle = false;
        self.fallback_visible_working = false;
        self.fallback_observed_at = None;
        self.stale_hook_idle_since = None;
        self.hook_authority = None;
        self.persisted_agent_session = None;
        self.agent_metadata.clear();
        self.launch_argv = None;
        self.launch_env.clear();
        self.state = AgentState::Unknown;
        self.respawn_shell_on_exit = false;
        self.pending_agent_resume_plan = None;
        self.clear_agent_name();
        self.missing_integration_warning_reported_for = None;
    }

    pub fn is_agent_terminal(&self) -> bool {
        self.agent_name.is_some()
            || self.effective_agent_label().is_some()
            || self.launch_argv.is_some()
    }

    pub fn border_label(&self, show_agent_labels: bool) -> Option<String> {
        self.effective_title().or_else(|| {
            self.manual_label.clone().or_else(|| {
                show_agent_labels
                    .then(|| {
                        self.effective_display_agent()
                            .or_else(|| self.effective_agent_label().map(str::to_string))
                    })
                    .flatten()
            })
        })
    }

    fn recompute_effective_state(
        &mut self,
        previous_agent_label: Option<String>,
        previous_known_agent: Option<Agent>,
        previous_state: AgentState,
        previous_presentation: EffectivePresentation,
        now: Instant,
    ) -> Option<EffectiveStateChange> {
        let state = if self.visible_working_overrides_hook() {
            AgentState::Working
        } else if self
            .hook_authority
            .as_ref()
            .is_some_and(|authority| authority.state == AgentState::Blocked)
            || self.visible_blocker_overrides_hook()
        {
            AgentState::Blocked
        } else if self.visible_idle_stales_hook(now) {
            AgentState::Idle
        } else {
            self.hook_authority
                .as_ref()
                .map(|authority| authority.state)
                .unwrap_or(self.fallback_state)
        };
        let agent_label = self.effective_agent_label().map(str::to_string);
        let known_agent = self.effective_known_agent();

        let presentation = self.effective_presentation_for_state_at(state, now);
        self.clear_expiry_pending_for_hidden_metadata();

        if previous_agent_label == agent_label
            && previous_state == state
            && previous_presentation == presentation
        {
            return None;
        }

        self.state = state;
        Some(EffectiveStateChange {
            previous_agent_label,
            previous_known_agent,
            previous_state,
            previous_presentation,
            agent_label,
            known_agent,
            state,
            presentation,
        })
    }
}

pub(crate) fn stabilize_agent_state(
    agent: Option<Agent>,
    previous: AgentState,
    raw: AgentState,
    now: std::time::Instant,
    last_claude_working_at: &mut Option<std::time::Instant>,
) -> AgentState {
    if agent != Some(Agent::Claude) {
        return raw;
    }

    match raw {
        AgentState::Working => {
            *last_claude_working_at = Some(now);
            AgentState::Working
        }
        AgentState::Blocked => AgentState::Blocked,
        AgentState::Idle if previous == AgentState::Working => {
            if last_claude_working_at
                .is_some_and(|last_working| now.duration_since(last_working) < CLAUDE_WORKING_HOLD)
            {
                AgentState::Working
            } else {
                AgentState::Idle
            }
        }
        _ => raw,
    }
}

pub(crate) fn stabilize_agent_detection(
    agent: Option<Agent>,
    previous: AgentState,
    detection: crate::detect::AgentDetection,
    process_exited: bool,
    now: std::time::Instant,
    last_claude_working_at: &mut Option<std::time::Instant>,
) -> AgentState {
    if process_exited {
        return detection.state;
    }

    stabilize_agent_state(
        agent,
        previous,
        detection.state,
        now,
        last_claude_working_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::AgentDetection;

    fn test_terminal() -> TerminalState {
        TerminalState::new(TerminalId::alloc(), "/tmp".into())
    }

    fn unique_temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "hako-terminal-state-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn claude_working_is_sticky_for_short_gap() {
        let now = std::time::Instant::now();
        let mut last_working = None;

        let working = stabilize_agent_state(
            Some(Agent::Claude),
            AgentState::Idle,
            AgentState::Working,
            now,
            &mut last_working,
        );
        assert_eq!(working, AgentState::Working);

        let still_working = stabilize_agent_state(
            Some(Agent::Claude),
            AgentState::Working,
            AgentState::Idle,
            now + std::time::Duration::from_millis(400),
            &mut last_working,
        );
        assert_eq!(still_working, AgentState::Working);
    }

    #[test]
    fn claude_transitions_to_idle_after_hold_expires() {
        let now = std::time::Instant::now();
        let mut last_working = Some(now);

        let state = stabilize_agent_state(
            Some(Agent::Claude),
            AgentState::Working,
            AgentState::Idle,
            now + CLAUDE_WORKING_HOLD + std::time::Duration::from_millis(1),
            &mut last_working,
        );
        assert_eq!(state, AgentState::Idle);
    }

    #[test]
    fn process_exit_idle_bypasses_claude_working_hold() {
        let now = std::time::Instant::now();
        let mut last_working = Some(now);

        let state = stabilize_agent_detection(
            Some(Agent::Claude),
            AgentState::Working,
            AgentDetection {
                state: AgentState::Idle,
                skip_state_update: false,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
            },
            true,
            now + std::time::Duration::from_millis(100),
            &mut last_working,
        );

        assert_eq!(state, AgentState::Idle);
    }

    #[test]
    fn visible_idle_does_not_bypass_claude_working_hold() {
        let now = std::time::Instant::now();
        let mut last_working = Some(now);

        let state = stabilize_agent_detection(
            Some(Agent::Claude),
            AgentState::Working,
            AgentDetection {
                state: AgentState::Idle,
                skip_state_update: false,
                visible_blocker: false,
                visible_idle: true,
                visible_working: false,
            },
            false,
            now + std::time::Duration::from_millis(100),
            &mut last_working,
        );

        assert_eq!(state, AgentState::Working);
    }

    #[test]
    fn non_claude_states_are_unchanged() {
        let now = std::time::Instant::now();
        let mut last_working = None;

        let state = stabilize_agent_state(
            Some(Agent::Codex),
            AgentState::Working,
            AgentState::Idle,
            now,
            &mut last_working,
        );
        assert_eq!(state, AgentState::Idle);
    }

    #[test]
    fn semantic_snapshot_preserves_meaningful_activity_time() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Idle);
        terminal.mark_meaningful_agent_activity(42, 1_700_000_000);

        let snapshot = terminal.capture_semantic_snapshot().expect("snapshot");
        assert_eq!(
            snapshot.last_meaningful_agent_activity_unix_secs,
            Some(1_700_000_000)
        );

        let mut restored = test_terminal();
        restored.restore_semantic_snapshot(snapshot);

        assert_eq!(
            restored.last_meaningful_agent_activity_unix_secs(),
            Some(1_700_000_000)
        );
        assert_eq!(restored.last_meaningful_agent_activity_seq(), 0);
    }

    #[test]
    fn hook_authority_overrides_fallback_for_same_agent() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
        );

        assert_eq!(terminal.detected_agent, Some(Agent::Pi));
        assert_eq!(terminal.fallback_state, AgentState::Idle);
        assert_eq!(terminal.effective_agent_label(), Some("pi"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn omp_hook_authority_overrides_omp_fallback_for_same_agent() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::OhMyPi), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:omp".into(),
            "omp".into(),
            AgentState::Working,
            None,
            None,
        );

        assert_eq!(terminal.detected_agent, Some(Agent::OhMyPi));
        assert_eq!(terminal.fallback_state, AgentState::Idle);
        assert_eq!(terminal.effective_agent_label(), Some("omp"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn omp_hook_report_without_new_ref_keeps_existing_file_path_ref() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::OhMyPi), AgentState::Idle);
        let session_path = unique_temp_path("parent.jsonl");
        std::fs::write(&session_path, b"session").unwrap();
        let session_ref = crate::agent_resume::AgentSessionRef {
            kind: crate::agent_resume::AgentSessionRefKind::Path,
            value: session_path.to_string_lossy().to_string(),
        };
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "hako:omp".into(),
            agent: "omp".into(),
            session_ref: session_ref.clone(),
        });

        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Working,
                None,
                None,
                None,
                Some(1),
            )
            .expect("state report should update hook authority");

        assert!(!mutation.session_ref_changed);
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            terminal
                .hook_authority
                .as_ref()
                .and_then(|authority| authority.session_ref.as_ref()),
            Some(&session_ref)
        );
        let _ = std::fs::remove_file(session_path);
    }

    #[test]
    fn omp_hook_report_does_not_replace_parent_ref_with_subagent_transcript() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::OhMyPi), AgentState::Idle);
        let root = unique_temp_path("session-root");
        let parent_path = root.join("parent.jsonl");
        let subagent_dir = root.join("parent");
        let subagent_path = subagent_dir.join("SubagentAudit.jsonl");
        std::fs::create_dir_all(&subagent_dir).unwrap();
        std::fs::write(&parent_path, b"parent").unwrap();
        std::fs::write(&subagent_path, b"subagent").unwrap();
        let parent_ref = crate::agent_resume::AgentSessionRef {
            kind: crate::agent_resume::AgentSessionRefKind::Path,
            value: parent_path.to_string_lossy().to_string(),
        };
        let subagent_ref = crate::agent_resume::AgentSessionRef {
            kind: crate::agent_resume::AgentSessionRefKind::Path,
            value: subagent_path.to_string_lossy().to_string(),
        };

        terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Idle,
                None,
                None,
                Some(parent_ref.clone()),
                Some(1),
            )
            .expect("parent report should establish hook authority");
        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Working,
                None,
                None,
                Some(subagent_ref),
                Some(2),
            )
            .expect("subagent activity should still update visible state");

        assert!(!mutation.session_ref_changed);
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            terminal
                .hook_authority
                .as_ref()
                .and_then(|authority| authority.session_ref.as_ref()),
            Some(&parent_ref)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn omp_hook_report_drops_existing_path_ref_when_file_is_missing() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::OhMyPi), AgentState::Idle);
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "hako:omp".into(),
            agent: "omp".into(),
            session_ref: crate::agent_resume::AgentSessionRef {
                kind: crate::agent_resume::AgentSessionRefKind::Path,
                value: unique_temp_path("missing.jsonl")
                    .to_string_lossy()
                    .to_string(),
            },
        });

        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Working,
                None,
                None,
                None,
                Some(1),
            )
            .expect("state report should update hook authority");

        assert!(mutation.session_ref_changed);
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            terminal
                .hook_authority
                .as_ref()
                .and_then(|authority| authority.session_ref.as_ref()),
            None
        );
    }

    #[test]
    fn new_hook_session_ref_resets_stale_sequence_for_same_source() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::OhMyPi), AgentState::Idle);
        let old_session = crate::agent_resume::AgentSessionRef {
            kind: crate::agent_resume::AgentSessionRefKind::Path,
            value: "/tmp/old-omp-session.jsonl".to_string(),
        };
        let new_session = crate::agent_resume::AgentSessionRef {
            kind: crate::agent_resume::AgentSessionRefKind::Path,
            value: "/tmp/new-omp-session.jsonl".to_string(),
        };

        terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Idle,
                None,
                None,
                Some(old_session),
                Some(10_000),
            )
            .expect("old idle session should establish hook authority");
        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Working,
                None,
                None,
                Some(new_session),
                Some(1),
            )
            .expect("new session should reset stale sequence");

        assert!(mutation.session_ref_changed);
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn invalid_agent_report_does_not_poison_sequence() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);

        let rejected = terminal.set_hook_authority_with_session_ref(
            "hako:omp".into(),
            "omp".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::path("/tmp/omp.jsonl"),
            Some(10_000),
        );
        assert!(rejected.is_none());

        let accepted = terminal
            .set_hook_authority_with_session_ref(
                "hako:pi".into(),
                "pi".into(),
                AgentState::Working,
                None,
                None,
                crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
                Some(1),
            )
            .expect("invalid report must not advance another source sequence");

        assert!(accepted.effective_state_change.is_some());
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn hook_authority_can_override_with_unknown_agent_label() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:custom".into(),
            "custom-agent".into(),
            AgentState::Working,
            None,
            None,
        );

        assert_eq!(terminal.detected_agent, Some(Agent::Pi));
        assert_eq!(terminal.effective_agent_label(), Some("custom-agent"));
        assert_eq!(terminal.effective_known_agent(), None);
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]

    fn visible_blocker_overrides_non_blocked_hook_for_same_agent() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            None,
        );

        let change = terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Blocked,
            true,
            false,
            false,
        );

        assert_eq!(terminal.fallback_state, AgentState::Blocked);
        assert_eq!(terminal.state, AgentState::Blocked);
        assert_eq!(change.unwrap().previous_state, AgentState::Working);
    }

    #[test]
    fn weak_blocked_fallback_does_not_override_hook_authority() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            None,
        );

        let change = terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Blocked,
            false,
            false,
            false,
        );

        assert_eq!(terminal.fallback_state, AgentState::Blocked);
        assert_eq!(terminal.state, AgentState::Working);
        assert!(change.is_none());
    }

    #[test]
    fn hook_blocked_wins_over_visible_blocker() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Working);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Blocked,
            None,
            None,
        );

        terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Blocked,
            true,
            false,
            false,
        );

        assert_eq!(terminal.state, AgentState::Blocked);
        assert!(terminal.hook_authority.is_some());
    }

    #[test]
    fn visible_blocker_does_not_override_different_agent_hook() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(None, AgentState::Unknown);
        terminal.set_hook_authority(
            "custom:agent".into(),
            "custom-agent".into(),
            AgentState::Working,
            None,
            None,
        );

        terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Blocked,
            true,
            false,
            false,
        );

        assert_eq!(terminal.effective_agent_label(), Some("custom-agent"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn visible_blocker_suppresses_stale_hook_custom_status() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Idle);
        terminal.set_hook_authority_with_custom_status(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            Some("planning".into()),
            None,
        );

        terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Blocked,
            true,
            false,
            false,
        );

        assert_eq!(terminal.state, AgentState::Blocked);
        assert_eq!(terminal.effective_custom_status(), None);
    }

    #[test]
    fn visible_idle_waits_before_overriding_claude_hook_working() {
        let now = Instant::now();
        let mut terminal = test_terminal();
        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Working,
            false,
            false,
            true,
            false,
            now,
        );
        terminal.set_hook_authority_with_custom_status_at(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Working,
            None,
            Some("thinking".into()),
            None,
            None,
            now,
        );

        let waiting = terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Idle,
            false,
            true,
            false,
            false,
            now + Duration::from_millis(500),
        );

        assert!(waiting.effective_state_change.is_none());
        assert_eq!(terminal.fallback_state, AgentState::Idle);
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            terminal.effective_custom_status().as_deref(),
            Some("thinking")
        );

        let change = terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Idle,
            false,
            true,
            false,
            false,
            now + Duration::from_millis(500) + STALE_HOOK_IDLE_GRACE + Duration::from_millis(1),
        );

        assert_eq!(terminal.state, AgentState::Idle);
        assert_eq!(terminal.effective_custom_status(), None);
        assert_eq!(
            change.effective_state_change.unwrap().previous_state,
            AgentState::Working
        );
    }

    #[test]
    fn fresh_hook_working_resets_visible_idle_stale_window() {
        let now = Instant::now();
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Claude), AgentState::Working);
        terminal.set_hook_authority_with_custom_status_at(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Working,
            None,
            Some("thinking".into()),
            None,
            None,
            now,
        );
        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Idle,
            false,
            true,
            false,
            false,
            now + Duration::from_millis(500),
        );

        terminal.set_hook_authority_with_custom_status_at(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Working,
            None,
            Some("thinking".into()),
            None,
            Some(1),
            now + Duration::from_millis(800),
        );
        let change = terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Idle,
            false,
            true,
            false,
            false,
            now + STALE_HOOK_IDLE_GRACE + Duration::from_millis(1),
        );

        assert!(change.effective_state_change.is_none());
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn visible_working_overrides_hook_idle_for_same_agent() {
        let now = Instant::now();
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Claude), AgentState::Idle);
        terminal.set_hook_authority_with_custom_status_at(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Idle,
            None,
            None,
            None,
            None,
            now,
        );

        let change = terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Working,
            false,
            false,
            true,
            false,
            now + Duration::from_millis(1),
        );

        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            change.effective_state_change.unwrap().previous_state,
            AgentState::Idle
        );
    }

    #[test]
    fn refreshed_visible_working_overrides_newer_hook_blocked() {
        let now = Instant::now();
        let mut terminal = test_terminal();
        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Codex),
            AgentState::Working,
            false,
            false,
            true,
            false,
            now,
        );
        terminal.set_hook_authority_with_custom_status_at(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Blocked,
            None,
            Some("permission".into()),
            None,
            None,
            now + CLAUDE_WORKING_HOLD + Duration::from_millis(1),
        );

        assert_eq!(terminal.state, AgentState::Blocked);

        let change = terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Codex),
            AgentState::Working,
            false,
            false,
            true,
            false,
            now + CLAUDE_WORKING_HOLD + Duration::from_millis(800),
        );

        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.effective_custom_status(), None);
        assert_eq!(
            change.effective_state_change.unwrap().previous_state,
            AgentState::Blocked
        );
    }

    #[test]
    fn visible_idle_does_not_override_hook_blocked() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Claude), AgentState::Working);
        terminal.set_hook_authority(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Blocked,
            None,
            None,
        );

        let change = terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Claude),
            AgentState::Idle,
            false,
            true,
            false,
        );

        assert_eq!(terminal.fallback_state, AgentState::Idle);
        assert_eq!(terminal.state, AgentState::Blocked);
        assert!(change.is_none());
    }

    #[test]
    fn visible_idle_does_not_override_other_agent_hook_working() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Working);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            None,
        );

        let change = terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Idle,
            false,
            true,
            false,
        );

        assert_eq!(terminal.fallback_state, AgentState::Idle);
        assert_eq!(terminal.state, AgentState::Working);
        assert!(change.is_none());
    }

    #[test]
    fn known_hook_authority_does_not_override_different_detected_agent() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Grok), AgentState::Working);
        let change = terminal.set_hook_authority(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Blocked,
            None,
            None,
        );

        assert!(change.is_none());
        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, Some(Agent::Grok));
        assert_eq!(terminal.effective_agent_label(), Some("grok"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn detected_agent_clears_conflicting_known_hook_authority() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Blocked,
            None,
            None,
        );

        terminal.set_detected_state(Some(Agent::Grok), AgentState::Working);

        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, Some(Agent::Grok));
        assert_eq!(terminal.effective_agent_label(), Some("grok"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn border_label_prefers_manual_label_over_agent_label() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Claude), AgentState::Idle);

        assert_eq!(terminal.border_label(false), None);
        assert_eq!(terminal.border_label(true).as_deref(), Some("claude"));

        terminal.set_manual_label(" reviewer ".into());
        assert_eq!(terminal.border_label(false).as_deref(), Some("reviewer"));
        assert_eq!(terminal.border_label(true).as_deref(), Some("reviewer"));

        terminal.set_manual_label("   ".into());
        assert_eq!(terminal.border_label(true).as_deref(), Some("claude"));

        terminal.set_manual_label("reviewer".into());
        terminal.clear_manual_label();
        assert_eq!(terminal.border_label(true).as_deref(), Some("claude"));
    }

    #[test]
    fn server_agent_label_makes_terminal_agent_visible_without_pane_label() {
        let mut terminal = test_terminal();

        terminal.set_manual_label(" reviewer ".into());
        assert_eq!(terminal.effective_agent_label(), None);
        assert!(!terminal.is_agent_terminal());

        terminal.agent_name = Some("codex".into());
        assert_eq!(terminal.effective_agent_label(), Some("codex"));
        assert_eq!(terminal.effective_known_agent(), Some(Agent::Codex));
        assert!(terminal.is_agent_terminal());
    }

    #[test]
    fn hook_authority_survives_unrelated_detected_agent_clear() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:custom".into(),
            "custom-agent".into(),
            AgentState::Working,
            None,
            None,
        );

        terminal.set_detected_state(None, AgentState::Unknown);

        assert!(terminal.hook_authority.is_some());
        assert_eq!(terminal.detected_agent, None);
        assert_eq!(terminal.effective_agent_label(), Some("custom-agent"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn detected_agent_clear_clears_matching_hook_authority() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::OpenCode), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:opencode".into(),
            "opencode".into(),
            AgentState::Idle,
            None,
            None,
        );

        terminal.set_detected_state(None, AgentState::Unknown);

        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, None);
        assert_eq!(terminal.fallback_state, AgentState::Unknown);
        assert_eq!(terminal.effective_agent_label(), None);
        assert_eq!(terminal.state, AgentState::Unknown);
    }

    #[test]
    fn detected_agent_clear_clears_matching_working_hook_authority() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Working);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            None,
        );

        terminal.set_detected_state(None, AgentState::Unknown);

        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, None);
        assert_eq!(terminal.effective_agent_label(), None);
        assert_eq!(terminal.state, AgentState::Unknown);
    }

    #[test]
    fn process_exit_clears_matching_hook_authority_before_reporting_idle() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Working);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            None,
        );

        terminal.set_detected_state_with_visible_blocker(
            Some(Agent::Codex),
            AgentState::Idle,
            false,
            false,
            true,
        );

        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, Some(Agent::Codex));
        assert_eq!(terminal.effective_agent_label(), Some("codex"));
        assert_eq!(terminal.state, AgentState::Idle);
    }

    #[test]
    fn stale_visible_screen_signal_does_not_override_newer_hook_authority() {
        let mut terminal = test_terminal();
        let observed = Instant::now();
        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Working,
            false,
            false,
            true,
            false,
            observed,
        );
        terminal.set_hook_authority_with_custom_status_at(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Working,
            None,
            None,
            None,
            Some(1),
            observed + Duration::from_secs(1),
        );

        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Claude),
            AgentState::Idle,
            false,
            true,
            false,
            false,
            observed,
        );

        assert_eq!(terminal.state, AgentState::Working);
        assert!(terminal.stale_hook_idle_since.is_none());
    }

    #[test]
    fn stale_process_exit_does_not_clear_newer_same_agent_hook_authority() {
        let mut terminal = test_terminal();
        let observed = Instant::now();
        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Codex),
            AgentState::Working,
            false,
            false,
            false,
            false,
            observed,
        );
        terminal.set_hook_authority_with_custom_status_at(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            None,
            None,
            Some(1),
            observed,
        );
        terminal.set_hook_authority_with_custom_status_at(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Working,
            None,
            Some("new turn".into()),
            None,
            Some(2),
            observed + Duration::from_secs(1),
        );

        terminal.set_detected_state_with_screen_signals_at(
            Some(Agent::Codex),
            AgentState::Idle,
            false,
            false,
            false,
            true,
            observed,
        );

        let authority = terminal.hook_authority.as_ref().expect("hook authority");
        assert_eq!(authority.custom_status.as_deref(), Some("new turn"));
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.effective_agent_label(), Some("codex"));
    }

    #[test]
    fn detected_agent_change_clears_previous_matching_hook_authority() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:codex".into(),
            "codex".into(),
            AgentState::Idle,
            None,
            None,
        );

        terminal.set_detected_state(Some(Agent::OpenCode), AgentState::Working);

        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, Some(Agent::OpenCode));
        assert_eq!(terminal.effective_agent_label(), Some("opencode"));
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn release_agent_clears_identity_immediately() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
        );

        terminal.release_agent("hako:pi", "pi", None);

        assert!(terminal.hook_authority.is_none());
        assert_eq!(terminal.detected_agent, None);
        assert_eq!(terminal.fallback_state, AgentState::Unknown);
        assert_eq!(terminal.state, AgentState::Unknown);
    }

    #[test]
    fn stale_hook_report_sequence_is_ignored_for_same_source() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            Some(20),
        );

        let change = terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Idle,
            None,
            Some(19),
        );

        assert!(change.is_none());
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(
            terminal.hook_authority.as_ref().unwrap().state,
            AgentState::Working
        );
    }

    #[test]
    fn accepted_hook_report_stores_session_ref() {
        let mut terminal = test_terminal();
        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:pi".into(),
                "pi".into(),
                AgentState::Working,
                None,
                None,
                crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
                Some(20),
            )
            .expect("accepted report");

        assert!(mutation.session_ref_changed);
        assert_eq!(
            terminal
                .hook_authority
                .as_ref()
                .and_then(|authority| authority.session_ref.as_ref())
                .map(|session_ref| (&session_ref.kind, session_ref.value.as_str())),
            Some((
                &crate::agent_resume::AgentSessionRefKind::Path,
                "/tmp/pi.jsonl"
            ))
        );
    }

    #[test]
    fn stale_hook_report_cannot_overwrite_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority_with_session_ref(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
            Some(20),
        );

        let mutation = terminal.set_hook_authority_with_session_ref(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::path("/tmp/new.jsonl"),
            Some(19),
        );

        assert!(mutation.is_none());
        assert_eq!(
            terminal
                .hook_authority
                .as_ref()
                .and_then(|authority| authority.session_ref.as_ref())
                .map(|session_ref| session_ref.value.as_str()),
            Some("/tmp/pi.jsonl")
        );
    }

    #[test]
    fn same_agent_child_session_ref_cannot_clobber_current_hook_identity() {
        let mut terminal = test_terminal();
        terminal
            .set_hook_authority_with_session_ref(
                "hako:pi".into(),
                "pi".into(),
                AgentState::Working,
                None,
                None,
                crate::agent_resume::AgentSessionRef::id("parent-session"),
                Some(20),
            )
            .expect("parent session should establish hook authority");

        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:pi".into(),
                "pi".into(),
                AgentState::Blocked,
                Some("waiting on child tool".into()),
                None,
                crate::agent_resume::AgentSessionRef::id("child-session"),
                Some(21),
            )
            .expect("state update from same agent should be accepted");

        assert!(!mutation.session_ref_changed);
        assert_eq!(terminal.state, AgentState::Blocked);
        assert_eq!(
            terminal.current_session_identity_for_persistence(),
            Some((
                "hako:pi".to_string(),
                "pi".to_string(),
                crate::agent_resume::AgentSessionRefKind::Id,
                "parent-session".to_string(),
            ))
        );
    }

    #[test]
    fn same_agent_child_session_ref_cannot_clobber_restored_pane_identity() {
        let mut terminal = test_terminal();
        terminal
            .set_agent_session_ref(
                "hako:pi".into(),
                "pi".into(),
                crate::agent_resume::AgentSessionRef::id("parent-session"),
                Some(20),
            )
            .expect("parent session should be persisted");

        let mutation = terminal.set_agent_session_ref(
            "hako:pi".into(),
            "pi".into(),
            crate::agent_resume::AgentSessionRef::id("child-session"),
            Some(21),
        );

        assert!(mutation.is_none());
        assert_eq!(
            terminal.current_session_identity_for_persistence(),
            Some((
                "hako:pi".to_string(),
                "pi".to_string(),
                crate::agent_resume::AgentSessionRefKind::Id,
                "parent-session".to_string(),
            ))
        );
    }

    #[test]
    fn new_parent_session_ref_is_accepted_after_current_identity_clears() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Pi), AgentState::Working);
        terminal
            .set_agent_session_ref(
                "hako:pi".into(),
                "pi".into(),
                crate::agent_resume::AgentSessionRef::id("parent-session"),
                Some(20),
            )
            .expect("parent session should be persisted");

        let clear = terminal.set_detected_state_with_mutation(None, AgentState::Unknown);
        assert!(clear.session_ref_changed);

        let mutation = terminal
            .set_agent_session_ref(
                "hako:pi".into(),
                "pi".into(),
                crate::agent_resume::AgentSessionRef::id("new-parent-session"),
                Some(21),
            )
            .expect("new parent session should be accepted after clear");

        assert!(mutation.session_ref_changed);
        assert_eq!(
            terminal.current_session_identity_for_persistence(),
            Some((
                "hako:pi".to_string(),
                "pi".to_string(),
                crate::agent_resume::AgentSessionRefKind::Id,
                "new-parent-session".to_string(),
            ))
        );
    }

    #[test]
    fn accepted_hook_report_without_session_ref_clears_previous_ref() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority_with_session_ref(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
            Some(20),
        );

        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:pi".into(),
                "pi".into(),
                AgentState::Working,
                None,
                None,
                None,
                Some(21),
            )
            .expect("accepted report");

        assert!(mutation.session_ref_changed);
        assert!(mutation.effective_state_change.is_none());
        assert!(terminal
            .hook_authority
            .as_ref()
            .unwrap()
            .session_ref
            .is_none());
    }

    #[test]
    fn accepted_hook_report_marks_changed_when_session_identity_changes() {
        let mut terminal = test_terminal();
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "hako:opencode".into(),
            agent: "opencode".into(),
            session_ref: crate::agent_resume::AgentSessionRef::id("same-session").unwrap(),
        });

        let mutation = terminal
            .set_hook_authority_with_session_ref(
                "hako:hermes".into(),
                "hermes".into(),
                AgentState::Working,
                None,
                None,
                crate::agent_resume::AgentSessionRef::id("same-session"),
                Some(20),
            )
            .expect("accepted report");

        assert!(mutation.session_ref_changed);
    }

    #[test]
    fn clearing_hook_authority_clears_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority_with_session_ref(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
            Some(20),
        );

        let mutation = terminal
            .clear_hook_authority_with_mutation(Some("hako:pi"), Some(21))
            .expect("accepted clear");

        assert!(mutation.session_ref_changed);
        assert!(terminal.hook_authority.is_none());
    }

    #[test]
    fn release_agent_clears_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority_with_session_ref(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
            Some(20),
        );

        let mutation = terminal
            .release_agent_with_mutation(
                "hako:pi",
                "pi",
                crate::agent_resume::AgentSessionRef::path("/tmp/pi.jsonl"),
                Some(21),
            )
            .expect("accepted release");

        assert!(mutation.session_ref_changed);
        assert!(terminal.hook_authority.is_none());
    }

    #[test]
    fn release_agent_clears_matching_restored_session_ref_before_detection() {
        let mut terminal = test_terminal();
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "hako:hermes".into(),
            agent: "hermes".into(),
            session_ref: crate::agent_resume::AgentSessionRef::id("hermes-session").unwrap(),
        });

        let mutation = terminal
            .release_agent_with_mutation(
                "hako:hermes",
                "hermes",
                crate::agent_resume::AgentSessionRef::id("hermes-session"),
                Some(21),
            )
            .expect("accepted release");

        assert!(mutation.session_ref_changed);
        assert!(mutation.effective_state_change.is_none());
        assert!(terminal.persisted_agent_session.is_none());
    }

    #[test]
    fn mismatched_session_release_cannot_clear_active_working_authority() {
        let mut terminal = test_terminal();
        terminal
            .set_hook_authority_with_session_ref(
                "hako:omp".into(),
                "omp".into(),
                AgentState::Working,
                None,
                None,
                crate::agent_resume::AgentSessionRef::path("/tmp/current.jsonl"),
                Some(20),
            )
            .expect("current session should establish authority");

        let stale_release = terminal.release_agent_with_mutation(
            "hako:omp",
            "omp",
            crate::agent_resume::AgentSessionRef::path("/tmp/old.jsonl"),
            Some(21),
        );
        assert!(stale_release.is_none());
        assert_eq!(terminal.state, AgentState::Working);
        assert!(terminal.hook_authority.is_some());

        let matching_release = terminal.release_agent_with_mutation(
            "hako:omp",
            "omp",
            crate::agent_resume::AgentSessionRef::path("/tmp/current.jsonl"),
            Some(22),
        );
        assert!(matching_release.is_some());
        assert!(terminal.hook_authority.is_none());
    }

    #[test]
    fn detected_conflict_clears_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority_with_session_ref(
            "hako:claude".into(),
            "claude".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::id("claude-session"),
            Some(20),
        );

        let mutation =
            terminal.set_detected_state_with_mutation(Some(Agent::Grok), AgentState::Idle);

        assert!(mutation.session_ref_changed);
        assert!(terminal.hook_authority.is_none());
    }

    #[test]
    fn detected_agent_disappearance_clears_matching_hook_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_detected_state(Some(Agent::Hermes), AgentState::Idle);
        terminal.set_hook_authority_with_session_ref(
            "hako:hermes".into(),
            "hermes".into(),
            AgentState::Working,
            None,
            None,
            crate::agent_resume::AgentSessionRef::id("hermes-session"),
            Some(20),
        );

        let mutation = terminal.set_detected_state_with_mutation(None, AgentState::Unknown);

        assert!(mutation.session_ref_changed);
        assert!(terminal.hook_authority.is_none());
        assert!(terminal.persisted_agent_session.is_none());
        assert_eq!(terminal.effective_agent_label(), None);
    }

    #[test]
    fn detected_agent_disappearance_clears_matching_persisted_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "hako:opencode".into(),
            agent: "opencode".into(),
            session_ref: crate::agent_resume::AgentSessionRef::id("opencode-session").unwrap(),
        });

        let first =
            terminal.set_detected_state_with_mutation(Some(Agent::OpenCode), AgentState::Idle);
        assert!(!first.session_ref_changed);
        assert!(terminal.persisted_agent_session.is_some());

        let second = terminal.set_detected_state_with_mutation(None, AgentState::Unknown);
        assert!(second.session_ref_changed);
        assert!(terminal.persisted_agent_session.is_none());
    }

    #[test]
    fn initial_unknown_detection_preserves_restored_session_ref() {
        let mut terminal = test_terminal();
        terminal.set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
            source: "hako:hermes".into(),
            agent: "hermes".into(),
            session_ref: crate::agent_resume::AgentSessionRef::id("hermes-session").unwrap(),
        });

        let mutation = terminal.set_detected_state_with_mutation(None, AgentState::Unknown);
        assert!(!mutation.session_ref_changed);
        assert!(terminal.persisted_agent_session.is_some());
    }

    #[test]
    fn unsequenced_hook_report_is_ignored_after_source_uses_sequence() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            Some(20),
        );

        let change = terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Idle,
            None,
            None,
        );

        assert!(change.is_none());
        assert_eq!(terminal.state, AgentState::Working);
    }

    #[test]
    fn stale_release_sequence_is_ignored_for_same_source() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            Some(20),
        );

        let change = terminal.release_agent("hako:pi", "pi", Some(19));

        assert!(change.is_none());
        assert_eq!(terminal.state, AgentState::Working);
        assert!(terminal.hook_authority.is_some());
    }

    #[test]
    fn stale_clear_all_sequence_is_checked_against_current_authority_source() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            Some(20),
        );

        let change = terminal.clear_hook_authority(None, Some(19));

        assert!(change.is_none());
        assert_eq!(terminal.state, AgentState::Working);
        assert!(terminal.hook_authority.is_some());
    }

    #[test]
    fn same_sequence_from_different_sources_is_independent() {
        let mut terminal = test_terminal();
        terminal.set_hook_authority(
            "hako:pi".into(),
            "pi".into(),
            AgentState::Working,
            None,
            Some(20),
        );

        terminal.set_hook_authority(
            "custom:pi".into(),
            "pi".into(),
            AgentState::Idle,
            None,
            Some(19),
        );

        assert_eq!(terminal.state, AgentState::Idle);
        assert_eq!(
            terminal.hook_authority.as_ref().unwrap().source,
            "custom:pi"
        );
    }
}
