use std::collections::{HashMap, HashSet};

use super::protocol::{
    AttachResume, CommandSpec, CoordinatorMessage, OutputRevision, RequestId, RuntimeExitStatus,
    RuntimeIdentity, RuntimeOpSeq, TerminalSize, WorkerError, WorkerErrorCode, WorkerMessage,
    MAX_TERMINAL_SNAPSHOT_BYTES,
};
use super::{ExecutionHostId, ResourceLocation};
use crate::pane::RemotePaneControl;
use crate::terminal::TerminalId;

/// Keep journaled input frames below the worker protocol max frame size.
pub(crate) const MAX_RUNTIME_INPUT_CHUNK_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_RUNTIME_OP_JOURNAL_OPS: usize = 256;
pub(crate) const MAX_RUNTIME_OP_JOURNAL_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) enum JournaledRuntimeOp {
    Input { data: Vec<u8> },
    Resize { size: TerminalSize },
}

#[derive(Clone, Debug)]
pub(crate) struct PendingRuntimeOp {
    request_id: RequestId,
    op_seq: RuntimeOpSeq,
    op: JournaledRuntimeOp,
}

impl PendingRuntimeOp {
    #[cfg(test)]
    pub(crate) fn new(request_id: RequestId, op_seq: RuntimeOpSeq, op: JournaledRuntimeOp) -> Self {
        Self {
            request_id,
            op_seq,
            op,
        }
    }

    pub(crate) fn request_id(&self) -> RequestId {
        self.request_id
    }

    pub(crate) fn op_seq(&self) -> RuntimeOpSeq {
        self.op_seq
    }

    pub(crate) fn op(&self) -> &JournaledRuntimeOp {
        &self.op
    }
}

/// Coordinator-owned remote terminal record. Fields stay private; registry mutates
/// through typed methods so transition rules stay local to this module.
pub(crate) struct ManagedRemoteTerminal {
    host_id: ExecutionHostId,
    location: ResourceLocation,
    identity: Option<RuntimeIdentity>,
    control: Option<RemotePaneControl>,
    next_op_seq: u64,
    /// Ordered input/resize ops retained until exact successful RequestAck.
    op_journal: Vec<PendingRuntimeOp>,
    /// False until create succeeds or adopt returns a validated worker sequence.
    op_seq_ready: bool,
    output_revision: OutputRevision,
    adopt_pending: bool,
    attach_pending: bool,
    force_checkpoint_pending: bool,
    termination_pending: bool,
    tombstone_recorded: bool,
}

impl ManagedRemoteTerminal {
    pub(crate) fn host_id(&self) -> &ExecutionHostId {
        &self.host_id
    }

    #[cfg(test)]
    pub(crate) fn set_host_id(&mut self, host_id: ExecutionHostId) {
        self.host_id = host_id;
    }

    pub(crate) fn location(&self) -> &ResourceLocation {
        &self.location
    }

    pub(crate) fn identity(&self) -> Option<&RuntimeIdentity> {
        self.identity.as_ref()
    }

    pub(crate) fn set_location(&mut self, location: ResourceLocation) {
        self.location = location;
    }

    pub(crate) fn set_identity(&mut self, identity: Option<RuntimeIdentity>) {
        self.identity = identity;
    }

    pub(crate) fn control_mut(&mut self) -> Option<&mut RemotePaneControl> {
        self.control.as_mut()
    }

    pub(crate) fn take_control(&mut self) {
        self.control = None;
    }

    pub(crate) fn next_op_seq(&self) -> u64 {
        self.next_op_seq
    }

    #[cfg(test)]
    pub(crate) fn set_next_op_seq(&mut self, next_op_seq: u64) {
        self.next_op_seq = next_op_seq;
    }

    pub(crate) fn op_journal(&self) -> &[PendingRuntimeOp] {
        &self.op_journal
    }

    #[cfg(test)]
    pub(crate) fn op_journal_mut(&mut self) -> &mut Vec<PendingRuntimeOp> {
        &mut self.op_journal
    }

    pub(crate) fn clear_op_journal(&mut self) {
        self.op_journal.clear();
    }

    pub(crate) fn op_seq_ready(&self) -> bool {
        self.op_seq_ready
    }

    pub(crate) fn set_op_seq_ready(&mut self, ready: bool) {
        self.op_seq_ready = ready;
    }

    pub(crate) fn adopt_pending(&self) -> bool {
        self.adopt_pending
    }

    pub(crate) fn set_adopt_pending(&mut self, pending: bool) {
        self.adopt_pending = pending;
    }

    pub(crate) fn output_revision(&self) -> OutputRevision {
        self.output_revision
    }

    pub(crate) fn attach_pending(&self) -> bool {
        self.attach_pending
    }

    pub(crate) fn force_checkpoint_pending(&self) -> bool {
        self.force_checkpoint_pending
    }

    pub(crate) fn request_checkpoint(&mut self) {
        self.attach_pending = true;
        self.force_checkpoint_pending = true;
    }

    pub(crate) fn set_attach_pending(&mut self, pending: bool) {
        self.attach_pending = pending;
        if !pending {
            self.force_checkpoint_pending = false;
        }
    }

    pub(crate) fn termination_pending(&self) -> bool {
        self.termination_pending
    }

    pub(crate) fn set_termination_pending(&mut self, pending: bool) {
        self.termination_pending = pending;
    }

    pub(crate) fn tombstone_recorded(&self) -> bool {
        self.tombstone_recorded
    }

    pub(crate) fn set_tombstone_recorded(&mut self, recorded: bool) {
        self.tombstone_recorded = recorded;
    }

    pub(crate) fn matches_runtime(
        &self,
        host_id: &ExecutionHostId,
        identity: &RuntimeIdentity,
    ) -> bool {
        &self.host_id == host_id && self.identity.as_ref() == Some(identity)
    }

    pub(crate) fn matches_process_observation(
        &self,
        host_id: &ExecutionHostId,
        identity: &RuntimeIdentity,
        location: &ResourceLocation,
    ) -> bool {
        self.matches_runtime(host_id, identity) && &self.location == location
    }
}

#[derive(Clone)]
pub(crate) struct PendingCreate {
    terminal_id: TerminalId,
    location: ResourceLocation,
    size: TerminalSize,
    command: Option<CommandSpec>,
    env: Vec<(String, String)>,
    scrollback_limit_bytes: usize,
}

impl PendingCreate {
    pub(crate) fn new(
        terminal_id: TerminalId,
        location: ResourceLocation,
        size: TerminalSize,
        command: Option<CommandSpec>,
        env: Vec<(String, String)>,
        scrollback_limit_bytes: usize,
    ) -> Self {
        Self {
            terminal_id,
            location,
            size,
            command,
            env,
            scrollback_limit_bytes,
        }
    }

    pub(crate) fn terminal_id(&self) -> &TerminalId {
        &self.terminal_id
    }

    pub(crate) fn location(&self) -> &ResourceLocation {
        &self.location
    }

    pub(crate) fn size(&self) -> &TerminalSize {
        &self.size
    }

    pub(crate) fn message(&self, request_id: RequestId) -> CoordinatorMessage {
        CoordinatorMessage::CreateTerminal {
            request_id,
            location: self.location.clone(),
            size: self.size.clone(),
            command: self.command.clone(),
            env: self.env.clone(),
            scrollback_limit_bytes: self.scrollback_limit_bytes as u64,
        }
    }
}

/// Domain effects produced by terminal worker-message transitions.
///
/// The host manager applies these: lifecycle events go to callers, attach
/// requests are sent on the worker transport, and unhandled acks fall through.
#[derive(Debug)]
pub(crate) enum RemoteTerminalEffect {
    Ready {
        terminal_id: TerminalId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
    },
    Output {
        terminal_id: TerminalId,
        data: Vec<u8>,
    },
    Snapshot {
        terminal_id: TerminalId,
        data: Vec<u8>,
    },
    StateChanged {
        terminal_id: TerminalId,
        agent: Option<crate::detect::Agent>,
        state: crate::detect::AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    },
    Exited {
        terminal_id: TerminalId,
        status: RuntimeExitStatus,
    },
    TerminationPending {
        terminal_id: TerminalId,
        location: ResourceLocation,
        identity: RuntimeIdentity,
    },
    TerminationFinished {
        terminal_id: TerminalId,
    },
    Failed {
        terminal_id: TerminalId,
        message: String,
    },
    Diagnostic {
        host_id: ExecutionHostId,
        message: String,
    },
    /// Manager should send AttachTerminal; on success clear attach_pending.
    Attach {
        host_id: ExecutionHostId,
        terminal_id: TerminalId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        resume: AttachResume,
    },
    /// RequestAck did not match a journaled op or pending termination.
    UnhandledRequestAck {
        host_id: ExecutionHostId,
        request_id: RequestId,
        error: Option<WorkerError>,
    },
}

struct PendingOutputCheckpoint {
    revision: OutputRevision,
    total_bytes: usize,
    data: Vec<u8>,
}

/// Coordinator-owned remote terminal registry and pending create/adopt/terminate state.
pub(crate) struct RemoteTerminalCoordinator {
    remote_terminals: HashMap<TerminalId, ManagedRemoteTerminal>,
    pending_creates: HashMap<(ExecutionHostId, RequestId), PendingCreate>,
    pending_adopts: HashMap<(ExecutionHostId, RequestId), TerminalId>,
    pending_terminations: HashMap<(ExecutionHostId, RequestId), TerminalId>,
    pending_checkpoints: HashMap<(ExecutionHostId, RuntimeIdentity), PendingOutputCheckpoint>,
    pending_checkpoint_bytes: usize,
}

impl RemoteTerminalCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            remote_terminals: HashMap::new(),
            pending_creates: HashMap::new(),
            pending_adopts: HashMap::new(),
            pending_terminations: HashMap::new(),
            pending_checkpoints: HashMap::new(),
            pending_checkpoint_bytes: 0,
        }
    }
    pub(crate) fn has_host_references(&self, host_id: &ExecutionHostId) -> bool {
        self.remote_terminals
            .values()
            .any(|record| &record.host_id == host_id)
    }

    pub(crate) fn host_terminal_ids(&self, host_id: &ExecutionHostId) -> HashSet<TerminalId> {
        self.remote_terminals
            .iter()
            .filter(|(_, record)| &record.host_id == host_id)
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect()
    }

    pub(crate) fn get(&self, terminal_id: &TerminalId) -> Option<&ManagedRemoteTerminal> {
        self.remote_terminals.get(terminal_id)
    }

    pub(crate) fn get_mut(
        &mut self,
        terminal_id: &TerminalId,
    ) -> Option<&mut ManagedRemoteTerminal> {
        self.remote_terminals.get_mut(terminal_id)
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, terminal_id: &TerminalId) -> bool {
        self.remote_terminals.contains_key(terminal_id)
    }

    pub(crate) fn remove(&mut self, terminal_id: &TerminalId) -> Option<ManagedRemoteTerminal> {
        let record = self.remote_terminals.remove(terminal_id)?;
        if let Some(identity) = &record.identity {
            self.discard_checkpoint(&(record.host_id.clone(), identity.clone()));
        }
        Some(record)
    }

    pub(crate) fn terminal_ids(&self) -> impl Iterator<Item = &TerminalId> {
        self.remote_terminals.keys()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&TerminalId, &ManagedRemoteTerminal)> {
        self.remote_terminals.iter()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut ManagedRemoteTerminal> {
        self.remote_terminals.values_mut()
    }

    pub(crate) fn insert_created(
        &mut self,
        terminal_id: TerminalId,
        host_id: ExecutionHostId,
        location: ResourceLocation,
        control: RemotePaneControl,
    ) {
        self.remote_terminals.insert(
            terminal_id,
            ManagedRemoteTerminal {
                host_id,
                location,
                identity: None,
                control: Some(control),
                next_op_seq: 1,
                op_journal: Vec::new(),
                op_seq_ready: true,
                adopt_pending: false,
                output_revision: OutputRevision::new(0),
                attach_pending: false,
                force_checkpoint_pending: false,
                termination_pending: false,
                tombstone_recorded: false,
            },
        );
    }

    pub(crate) fn insert_adopted(
        &mut self,
        terminal_id: TerminalId,
        location: ResourceLocation,
        identity: RuntimeIdentity,
        control: RemotePaneControl,
    ) {
        self.remote_terminals.insert(
            terminal_id,
            ManagedRemoteTerminal {
                host_id: location.execution_host_id.clone(),
                location,
                identity: Some(identity),
                control: Some(control),
                // Placeholder until AdoptTerminalResult validates the worker's last applied seq.
                next_op_seq: 1,
                op_journal: Vec::new(),
                op_seq_ready: false,
                adopt_pending: true,
                output_revision: OutputRevision::new(0),
                attach_pending: false,
                force_checkpoint_pending: false,
                termination_pending: false,
                tombstone_recorded: false,
            },
        );
    }

    pub(crate) fn restore_termination_pending(
        &mut self,
        terminal_id: TerminalId,
        location: ResourceLocation,
        identity: RuntimeIdentity,
    ) {
        self.remote_terminals
            .entry(terminal_id)
            .or_insert_with(|| ManagedRemoteTerminal {
                host_id: location.execution_host_id.clone(),
                location,
                identity: Some(identity),
                control: None,
                next_op_seq: 1,
                op_journal: Vec::new(),
                op_seq_ready: false,
                adopt_pending: false,
                output_revision: OutputRevision::new(0),
                attach_pending: false,
                force_checkpoint_pending: false,
                termination_pending: true,
                tombstone_recorded: true,
            });
    }

    pub(crate) fn track_pending_create(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        pending: PendingCreate,
    ) {
        self.pending_creates.insert((host_id, request_id), pending);
    }

    pub(crate) fn remove_pending_create(
        &mut self,
        host_id: &ExecutionHostId,
        request_id: RequestId,
    ) -> Option<PendingCreate> {
        self.pending_creates.remove(&(host_id.clone(), request_id))
    }

    #[cfg(test)]
    pub(crate) fn pending_create_request_id(
        &self,
        host_id: &ExecutionHostId,
        terminal_id: &TerminalId,
    ) -> Option<RequestId> {
        self.pending_creates
            .iter()
            .find_map(|((pending_host, request_id), pending)| {
                (pending_host == host_id && pending.terminal_id() == terminal_id)
                    .then_some(*request_id)
            })
    }

    #[cfg(test)]
    pub(crate) fn has_pending_create_request(
        &self,
        host_id: &ExecutionHostId,
        request_id: RequestId,
    ) -> bool {
        self.pending_creates
            .contains_key(&(host_id.clone(), request_id))
    }

    #[cfg(test)]
    pub(crate) fn pending_termination_request_id(
        &self,
        terminal_id: &TerminalId,
    ) -> Option<RequestId> {
        self.pending_terminations
            .iter()
            .find_map(|((_, request_id), pending)| (pending == terminal_id).then_some(*request_id))
    }

    pub(crate) fn has_pending_create_for(&self, terminal_id: &TerminalId) -> bool {
        self.pending_creates
            .values()
            .any(|pending| pending.terminal_id() == terminal_id)
    }

    pub(crate) fn pending_creates(
        &self,
    ) -> impl Iterator<Item = (&(ExecutionHostId, RequestId), &PendingCreate)> {
        self.pending_creates.iter()
    }

    pub(crate) fn track_pending_adopt(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        terminal_id: TerminalId,
    ) {
        self.pending_adopts
            .insert((host_id, request_id), terminal_id);
    }

    pub(crate) fn pending_adopt_terminal_ids(&self) -> HashSet<TerminalId> {
        self.pending_adopts.values().cloned().collect()
    }

    pub(crate) fn track_pending_termination(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        terminal_id: TerminalId,
    ) {
        self.pending_terminations
            .insert((host_id, request_id), terminal_id);
    }

    pub(crate) fn pending_termination_terminal_ids(&self) -> HashSet<TerminalId> {
        self.pending_terminations.values().cloned().collect()
    }

    pub(crate) fn pending_terminations(
        &self,
    ) -> impl Iterator<Item = (&(ExecutionHostId, RequestId), &TerminalId)> {
        self.pending_terminations.iter()
    }

    pub(crate) fn clear_pending_for_host(&mut self, host_id: &ExecutionHostId) {
        self.pending_adopts
            .retain(|(pending_host_id, _), _| pending_host_id != host_id);
        self.pending_checkpoints
            .retain(|(pending_host, _), checkpoint| {
                if pending_host == host_id {
                    self.pending_checkpoint_bytes -= checkpoint.total_bytes;
                    false
                } else {
                    true
                }
            });
    }

    #[cfg(test)]
    pub(crate) fn insert_record_for_test(
        &mut self,
        terminal_id: TerminalId,
        record: ManagedRemoteTerminal,
    ) {
        self.remote_terminals.insert(terminal_id, record);
    }

    #[cfg(test)]
    pub(crate) fn managed_for_test(
        host_id: ExecutionHostId,
        location: ResourceLocation,
        identity: Option<RuntimeIdentity>,
        termination_pending: bool,
        tombstone_recorded: bool,
    ) -> ManagedRemoteTerminal {
        ManagedRemoteTerminal {
            host_id,
            location,
            identity,
            control: None,
            next_op_seq: 1,
            op_journal: Vec::new(),
            op_seq_ready: true,
            adopt_pending: false,
            output_revision: OutputRevision::new(0),
            attach_pending: false,
            force_checkpoint_pending: false,
            termination_pending,
            tombstone_recorded,
        }
    }

    pub(crate) fn forget_terminal(&mut self, terminal_id: &TerminalId) -> bool {
        self.pending_creates
            .retain(|_, pending| &pending.terminal_id != terminal_id);
        self.pending_adopts
            .retain(|_, pending_terminal_id| pending_terminal_id != terminal_id);
        self.pending_terminations
            .retain(|_, pending_terminal_id| pending_terminal_id != terminal_id);
        self.remove(terminal_id).is_some()
    }

    pub(crate) fn find_by_identity(
        &self,
        host_id: &ExecutionHostId,
        identity: &RuntimeIdentity,
    ) -> Option<(&TerminalId, &ManagedRemoteTerminal)> {
        self.remote_terminals
            .iter()
            .find(|(_, record)| record.matches_runtime(host_id, identity))
    }

    pub(crate) fn push_journaled_op(
        &mut self,
        terminal_id: &TerminalId,
        request_id: RequestId,
        op_seq: RuntimeOpSeq,
        op: JournaledRuntimeOp,
    ) -> bool {
        let Some(record) = self.remote_terminals.get_mut(terminal_id) else {
            return false;
        };
        if record.termination_pending || !record.op_seq_ready || record.identity.is_none() {
            return false;
        }
        record.next_op_seq = op_seq.get().saturating_add(1);
        record.op_journal.push(PendingRuntimeOp {
            request_id,
            op_seq,
            op,
        });
        true
    }

    /// Handle create/adopt/output/exit/attach/ack worker messages.
    ///
    /// Returns domain effects for the manager to apply (events + transport sends).
    /// Messages outside the terminal lifecycle return an empty effect list so the
    /// manager can route them elsewhere.
    pub(crate) fn handle_message(
        &mut self,
        host_id: ExecutionHostId,
        message: WorkerMessage,
    ) -> Option<Vec<RemoteTerminalEffect>> {
        match message {
            WorkerMessage::CreateTerminalResult {
                request_id,
                identity,
                location,
                error,
            } => Some(self.handle_create_result(host_id, request_id, identity, location, error)),
            WorkerMessage::AdoptTerminalResult {
                request_id,
                identity,
                location,
                last_applied_op_seq,
                error,
            } => Some(self.handle_adopt_result(
                host_id,
                request_id,
                identity,
                location,
                last_applied_op_seq,
                error,
            )),
            WorkerMessage::OutputDelta {
                identity,
                base_revision,
                revision,
                data,
                ..
            } => Some(self.handle_output_delta(host_id, identity, base_revision, revision, data)),
            WorkerMessage::OutputCheckpoint {
                identity,
                revision,
                total_bytes,
                offset,
                data,
                ..
            } => Some(self.handle_output_checkpoint_chunk(
                host_id,
                identity,
                revision,
                total_bytes,
                offset,
                data,
            )),
            WorkerMessage::TerminalStateChanged {
                identity,
                agent,
                state,
                visible_blocker,
                visible_idle,
                visible_working,
                process_exited,
                ..
            } => Some(self.handle_state_changed(
                host_id,
                identity,
                agent,
                state,
                visible_blocker,
                visible_idle,
                visible_working,
                process_exited,
            )),
            WorkerMessage::RuntimeExit {
                identity, status, ..
            } => Some(self.handle_runtime_exit(host_id, identity, status)),
            WorkerMessage::AttachTerminalResult {
                identity,
                location,
                error,
                ..
            } => Some(self.handle_attach_result(host_id, identity, location, error)),
            WorkerMessage::RequestAck { request_id, error } => {
                Some(self.handle_request_ack(host_id, request_id, error))
            }
            _ => None,
        }
    }

    fn handle_create_result(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        identity: Option<RuntimeIdentity>,
        location: ResourceLocation,
        error: Option<WorkerError>,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        let key = (host_id.clone(), request_id);
        let Some(pending_create) = self.pending_creates.get(&key) else {
            return effects;
        };
        if pending_create.location.execution_host_id != host_id
            || location.execution_host_id != host_id
        {
            effects.push(RemoteTerminalEffect::Diagnostic {
                host_id,
                message: "execution worker returned create result for the wrong host".to_string(),
            });
            return effects;
        }
        let Some(pending_create) = self.pending_creates.remove(&key) else {
            return effects;
        };
        let terminal_id = pending_create.terminal_id;
        let termination_pending = self
            .remote_terminals
            .get(&terminal_id)
            .is_some_and(|record| record.termination_pending);
        if let Some(error) = error {
            self.remove(&terminal_id);
            if !termination_pending {
                effects.push(RemoteTerminalEffect::Failed {
                    terminal_id,
                    message: error.message,
                });
            }
            return effects;
        }
        let Some(identity) = identity else {
            self.remove(&terminal_id);
            if !termination_pending {
                effects.push(RemoteTerminalEffect::Failed {
                    terminal_id,
                    message: "execution worker returned no runtime identity".to_string(),
                });
            }
            return effects;
        };
        let tombstone = if let Some(record) = self.remote_terminals.get_mut(&terminal_id) {
            record.location = location.clone();
            record.identity = Some(identity.clone());
            if record.termination_pending {
                if record.tombstone_recorded {
                    None
                } else {
                    record.tombstone_recorded = true;
                    Some((record.location.clone(), identity.clone()))
                }
            } else {
                record.attach_pending = true;
                effects.push(RemoteTerminalEffect::Ready {
                    terminal_id: terminal_id.clone(),
                    identity: identity.clone(),
                    location: location.clone(),
                });
                None
            }
        } else {
            // Mapping already dropped while cancelled, or unexpected ready:
            // restore termination-pending so the worker runtime is not orphaned.
            self.restore_termination_pending(
                terminal_id.clone(),
                location.clone(),
                identity.clone(),
            );
            Some((location.clone(), identity.clone()))
        };
        if let Some((location, identity)) = tombstone {
            effects.push(RemoteTerminalEffect::TerminationPending {
                terminal_id,
                location,
                identity,
            });
            return effects;
        }
        if termination_pending {
            return effects;
        }
        effects.push(RemoteTerminalEffect::Attach {
            host_id,
            terminal_id,
            identity,
            location,
            resume: AttachResume::Checkpoint,
        });
        effects
    }

    fn handle_adopt_result(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        identity: Option<RuntimeIdentity>,
        location: ResourceLocation,
        last_applied_op_seq: RuntimeOpSeq,
        error: Option<WorkerError>,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        let key = (host_id.clone(), request_id);
        let Some(terminal_id) = self.pending_adopts.remove(&key) else {
            return effects;
        };
        let Some(record) = self.remote_terminals.get_mut(&terminal_id) else {
            return effects;
        };
        if record.host_id != host_id {
            effects.push(RemoteTerminalEffect::Diagnostic {
                host_id,
                message: "execution worker returned adopt result for the wrong host".to_string(),
            });
            return effects;
        }
        if record.termination_pending {
            record.adopt_pending = false;
            return effects;
        }
        if let Some(error) = error {
            record.adopt_pending = false;
            let lost = matches!(
                error.code,
                WorkerErrorCode::UnknownRuntime
                    | WorkerErrorCode::Gone
                    | WorkerErrorCode::IncarnationMismatch
                    | WorkerErrorCode::NotFound
            );
            if lost {
                self.remove(&terminal_id);
                effects.push(RemoteTerminalEffect::Failed {
                    terminal_id,
                    message: format!("remote terminal lost: {}", error.message),
                });
            } else {
                effects.push(RemoteTerminalEffect::Failed {
                    terminal_id,
                    message: format!("remote terminal adopt degraded: {}", error.message),
                });
            }
            return effects;
        }
        let Some(identity) = identity else {
            record.adopt_pending = false;
            effects.push(RemoteTerminalEffect::Failed {
                terminal_id,
                message: "execution worker returned no runtime identity on adopt".to_string(),
            });
            return effects;
        };
        if record.identity.as_ref() != Some(&identity) {
            record.adopt_pending = false;
            effects.push(RemoteTerminalEffect::Failed {
                terminal_id,
                message: "execution worker returned mismatched runtime identity on adopt"
                    .to_string(),
            });
            return effects;
        }
        if location.execution_host_id != host_id {
            record.adopt_pending = false;
            effects.push(RemoteTerminalEffect::Diagnostic {
                host_id,
                message: "execution worker returned adopt location for the wrong host".to_string(),
            });
            return effects;
        }
        // Only accept a worker-reported sequence after identity/location validation.
        record.location = location;
        record.next_op_seq = last_applied_op_seq.get().saturating_add(1);
        record.op_seq_ready = true;
        record.adopt_pending = false;
        record.attach_pending = true;
        effects
    }

    fn handle_output_delta(
        &mut self,
        host_id: ExecutionHostId,
        identity: RuntimeIdentity,
        base_revision: OutputRevision,
        revision: OutputRevision,
        data: Vec<u8>,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        let Some((terminal_id, record)) = self
            .remote_terminals
            .iter_mut()
            .find(|(_, record)| record.matches_runtime(&host_id, &identity))
            .map(|(terminal_id, record)| (terminal_id.clone(), record))
        else {
            return effects;
        };
        if record.output_revision != base_revision {
            effects.push(RemoteTerminalEffect::Attach {
                host_id,
                terminal_id,
                identity,
                location: record.location.clone(),
                resume: AttachResume::Checkpoint,
            });
            return effects;
        }
        record.output_revision = revision;
        effects.push(RemoteTerminalEffect::Output { terminal_id, data });
        effects
    }

    fn handle_output_checkpoint_chunk(
        &mut self,
        host_id: ExecutionHostId,
        identity: RuntimeIdentity,
        revision: OutputRevision,
        total_bytes: u64,
        offset: u64,
        data: Vec<u8>,
    ) -> Vec<RemoteTerminalEffect> {
        let key = (host_id, identity);
        let Some((terminal_id, record)) = self
            .remote_terminals
            .iter()
            .find(|(_, record)| record.matches_runtime(&key.0, &key.1))
            .map(|(terminal_id, record)| (terminal_id.clone(), record))
        else {
            self.discard_checkpoint(&key);
            return Vec::new();
        };
        if revision.get() < record.output_revision.get() {
            self.discard_checkpoint(&key);
            return Vec::new();
        }
        let Ok(total_bytes) = usize::try_from(total_bytes) else {
            self.discard_checkpoint(&key);
            return Vec::new();
        };
        let Ok(offset) = usize::try_from(offset) else {
            self.discard_checkpoint(&key);
            return Vec::new();
        };
        if total_bytes == 0 || total_bytes > MAX_TERMINAL_SNAPSHOT_BYTES || data.is_empty() {
            self.discard_checkpoint(&key);
            return Vec::new();
        }

        if offset == 0 {
            self.discard_checkpoint(&key);
            if self
                .pending_checkpoint_bytes
                .checked_add(total_bytes)
                .is_none_or(|bytes| bytes > MAX_TERMINAL_SNAPSHOT_BYTES)
            {
                return Vec::new();
            }
            if data.len() > total_bytes {
                return Vec::new();
            }
            let mut bytes = data;
            if bytes.try_reserve_exact(total_bytes - bytes.len()).is_err() {
                return Vec::new();
            }
            let complete = bytes.len() == total_bytes;
            self.pending_checkpoint_bytes += total_bytes;
            self.pending_checkpoints.insert(
                key.clone(),
                PendingOutputCheckpoint {
                    revision,
                    total_bytes,
                    data: bytes,
                },
            );
            if !complete {
                return Vec::new();
            }
        } else {
            let Some(checkpoint) = self.pending_checkpoints.get_mut(&key) else {
                return Vec::new();
            };
            if checkpoint.revision != revision
                || checkpoint.total_bytes != total_bytes
                || checkpoint.data.len() != offset
                || data.len() > total_bytes.saturating_sub(offset)
            {
                self.discard_checkpoint(&key);
                return Vec::new();
            }
            checkpoint.data.extend_from_slice(&data);
            if checkpoint.data.len() != total_bytes {
                return Vec::new();
            }
        }
        let Some(checkpoint) = self.pending_checkpoints.remove(&key) else {
            return Vec::new();
        };
        self.pending_checkpoint_bytes = self
            .pending_checkpoint_bytes
            .saturating_sub(checkpoint.total_bytes);
        let Some(record) = self.remote_terminals.get_mut(&terminal_id) else {
            return Vec::new();
        };
        record.output_revision = checkpoint.revision;
        vec![RemoteTerminalEffect::Snapshot {
            terminal_id,
            data: checkpoint.data,
        }]
    }

    fn discard_checkpoint(&mut self, key: &(ExecutionHostId, RuntimeIdentity)) {
        if let Some(checkpoint) = self.pending_checkpoints.remove(key) {
            self.pending_checkpoint_bytes = self
                .pending_checkpoint_bytes
                .saturating_sub(checkpoint.total_bytes);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_state_changed(
        &self,
        host_id: ExecutionHostId,
        identity: RuntimeIdentity,
        agent: Option<crate::detect::Agent>,
        state: crate::detect::AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        if let Some((terminal_id, _)) = self.find_by_identity(&host_id, &identity) {
            effects.push(RemoteTerminalEffect::StateChanged {
                terminal_id: terminal_id.clone(),
                agent,
                state,
                visible_blocker,
                visible_idle,
                visible_working,
                process_exited,
            });
        }
        effects
    }

    fn handle_runtime_exit(
        &mut self,
        host_id: ExecutionHostId,
        identity: RuntimeIdentity,
        status: RuntimeExitStatus,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        let terminal = self
            .remote_terminals
            .iter()
            .find(|(_, record)| record.matches_runtime(&host_id, &identity))
            .map(|(terminal_id, record)| (terminal_id.clone(), record.termination_pending));
        if let Some((terminal_id, termination_pending)) = terminal {
            self.remove(&terminal_id);
            self.pending_terminations
                .retain(|_, pending_terminal_id| pending_terminal_id != &terminal_id);
            if termination_pending {
                effects.push(RemoteTerminalEffect::TerminationFinished { terminal_id });
            } else {
                effects.push(RemoteTerminalEffect::Exited {
                    terminal_id,
                    status,
                });
            }
        }
        effects
    }

    fn handle_attach_result(
        &mut self,
        host_id: ExecutionHostId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        error: Option<WorkerError>,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        let Some((terminal_id, termination_pending)) = self
            .remote_terminals
            .iter()
            .find(|(_, record)| record.matches_runtime(&host_id, &identity))
            .map(|(terminal_id, record)| (terminal_id.clone(), record.termination_pending))
        else {
            return effects;
        };
        if let Some(error) = error {
            if termination_pending {
                return effects;
            }
            let lost = matches!(
                error.code,
                WorkerErrorCode::UnknownRuntime
                    | WorkerErrorCode::Gone
                    | WorkerErrorCode::IncarnationMismatch
                    | WorkerErrorCode::NotFound
            );
            if lost {
                self.remove(&terminal_id);
                effects.push(RemoteTerminalEffect::Failed {
                    terminal_id,
                    message: format!("remote terminal lost: {}", error.message),
                });
            } else {
                if let Some(record) = self.remote_terminals.get_mut(&terminal_id) {
                    record.attach_pending = true;
                }
                effects.push(RemoteTerminalEffect::Diagnostic {
                    host_id,
                    message: format!(
                        "remote terminal attach degraded for {terminal_id}: {}",
                        error.message
                    ),
                });
            }
        } else if location.execution_host_id != host_id {
            effects.push(RemoteTerminalEffect::Diagnostic {
                host_id,
                message: "execution worker returned attach result for the wrong host".to_string(),
            });
        }
        effects
    }

    fn handle_request_ack(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        error: Option<WorkerError>,
    ) -> Vec<RemoteTerminalEffect> {
        let mut effects = Vec::new();
        if self.acknowledge_runtime_op(&host_id, request_id, error.clone(), &mut effects) {
            return effects;
        }
        let Some(terminal_id) = self
            .pending_terminations
            .remove(&(host_id.clone(), request_id))
        else {
            effects.push(RemoteTerminalEffect::UnhandledRequestAck {
                host_id,
                request_id,
                error,
            });
            return effects;
        };
        match error {
            None => {
                self.remove(&terminal_id);
                effects.push(RemoteTerminalEffect::TerminationFinished { terminal_id });
            }
            Some(error)
                if matches!(
                    error.code,
                    WorkerErrorCode::UnknownRuntime
                        | WorkerErrorCode::Gone
                        | WorkerErrorCode::IncarnationMismatch
                ) =>
            {
                self.remove(&terminal_id);
                effects.push(RemoteTerminalEffect::TerminationFinished { terminal_id });
            }
            Some(error) => {
                effects.push(RemoteTerminalEffect::Diagnostic {
                    host_id,
                    message: error.message,
                });
            }
        }
        effects
    }

    fn acknowledge_runtime_op(
        &mut self,
        host_id: &ExecutionHostId,
        request_id: RequestId,
        error: Option<WorkerError>,
        effects: &mut Vec<RemoteTerminalEffect>,
    ) -> bool {
        let Some((terminal_id, index)) =
            self.remote_terminals
                .iter()
                .find_map(|(terminal_id, record)| {
                    if &record.host_id != host_id {
                        return None;
                    }
                    record
                        .op_journal
                        .iter()
                        .position(|pending| pending.request_id == request_id)
                        .map(|index| (terminal_id.clone(), index))
                })
        else {
            return false;
        };
        if let Some(error) = error {
            // Negative ACK on a journaled op must not leave an irrecoverable head
            // that bricks following op_seq. Retire through the failed op and surface
            // a concrete terminal failure when the head cannot be replayed safely.
            let failed_head = index == 0;
            if let Some(record) = self.remote_terminals.get_mut(&terminal_id) {
                record.op_journal.drain(..=index);
            }
            effects.push(RemoteTerminalEffect::Diagnostic {
                host_id: host_id.clone(),
                message: error.message.clone(),
            });
            if failed_head
                && matches!(
                    error.code,
                    WorkerErrorCode::Conflict
                        | WorkerErrorCode::Gone
                        | WorkerErrorCode::UnknownRuntime
                        | WorkerErrorCode::IncarnationMismatch
                        | WorkerErrorCode::Busy
                )
            {
                effects.push(RemoteTerminalEffect::Failed {
                    terminal_id: terminal_id.clone(),
                    message: format!("runtime control rejected: {}", error.message),
                });
            }
            return true;
        }
        if let Some(record) = self.remote_terminals.get_mut(&terminal_id) {
            record.op_journal.drain(..=index);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::HostPath;

    fn host() -> ExecutionHostId {
        ExecutionHostId::new("ssh:workbox:1").unwrap()
    }

    fn location(host_id: &ExecutionHostId) -> ResourceLocation {
        ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap())
    }

    fn identity() -> RuntimeIdentity {
        RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        )
    }

    #[test]
    fn create_failure_emits_terminal_failed_effect() {
        let mut coordinator = RemoteTerminalCoordinator::new();
        let host_id = host();
        let terminal_id = TerminalId::alloc();
        let location = location(&host_id);
        coordinator.track_pending_create(
            host_id.clone(),
            RequestId::new(1),
            PendingCreate::new(
                terminal_id.clone(),
                location.clone(),
                TerminalSize { cols: 80, rows: 24 },
                None,
                Vec::new(),
                1024,
            ),
        );
        coordinator.insert_record_for_test(
            terminal_id.clone(),
            RemoteTerminalCoordinator::managed_for_test(
                host_id.clone(),
                location.clone(),
                None,
                false,
                false,
            ),
        );

        let effects = coordinator
            .handle_message(
                host_id,
                WorkerMessage::CreateTerminalResult {
                    request_id: RequestId::new(1),
                    identity: None,
                    location,
                    error: Some(WorkerError::new(WorkerErrorCode::Failed, "boom")),
                },
            )
            .expect("terminal message");

        assert!(matches!(
            effects.as_slice(),
            [RemoteTerminalEffect::Failed {
                terminal_id: failed,
                message,
            }] if failed == &terminal_id && message == "boom"
        ));
        assert!(!coordinator.contains(&terminal_id));
    }

    #[test]
    fn runtime_exit_while_terminating_finishes_termination() {
        let mut coordinator = RemoteTerminalCoordinator::new();
        let host_id = host();
        let terminal_id = TerminalId::alloc();
        let identity = identity();
        coordinator.insert_record_for_test(
            terminal_id.clone(),
            RemoteTerminalCoordinator::managed_for_test(
                host_id.clone(),
                location(&host_id),
                Some(identity.clone()),
                true,
                true,
            ),
        );

        let effects = coordinator
            .handle_message(
                host_id,
                WorkerMessage::RuntimeExit {
                    identity,
                    location: location(&host()),
                    status: RuntimeExitStatus::Code(0),
                },
            )
            .expect("terminal message");

        assert!(matches!(
            effects.as_slice(),
            [RemoteTerminalEffect::TerminationFinished {
                terminal_id: finished,
            }] if finished == &terminal_id
        ));
        assert!(!coordinator.contains(&terminal_id));
    }
    fn coordinator_with_terminal() -> (
        RemoteTerminalCoordinator,
        ExecutionHostId,
        RuntimeIdentity,
        TerminalId,
    ) {
        let mut coordinator = RemoteTerminalCoordinator::new();
        let host_id = host();
        let identity = identity();
        let terminal_id = TerminalId::alloc();
        coordinator.insert_record_for_test(
            terminal_id.clone(),
            RemoteTerminalCoordinator::managed_for_test(
                host_id.clone(),
                location(&host_id),
                Some(identity.clone()),
                false,
                false,
            ),
        );
        (coordinator, host_id, identity, terminal_id)
    }

    fn checkpoint_chunk(
        identity: RuntimeIdentity,
        host_id: &ExecutionHostId,
        total_bytes: u64,
        offset: u64,
        data: &[u8],
    ) -> WorkerMessage {
        WorkerMessage::OutputCheckpoint {
            identity,
            location: location(host_id),
            revision: OutputRevision::new(1),
            total_bytes,
            offset,
            data: data.to_vec(),
        }
    }

    #[test]
    fn checkpoint_chunks_apply_only_after_complete_contiguous_snapshot() {
        let (mut coordinator, host_id, identity, terminal_id) = coordinator_with_terminal();
        let first = coordinator
            .handle_message(
                host_id.clone(),
                checkpoint_chunk(identity.clone(), &host_id, 6, 0, b"abc"),
            )
            .unwrap();
        assert!(first.is_empty());

        let complete = coordinator
            .handle_message(
                host_id.clone(),
                checkpoint_chunk(identity, &host_id, 6, 3, b"def"),
            )
            .unwrap();
        assert!(matches!(
            complete.as_slice(),
            [RemoteTerminalEffect::Snapshot { terminal_id: id, data }]
                if id == &terminal_id && data == b"abcdef"
        ));
    }

    #[test]
    fn malformed_and_truncated_checkpoint_chunks_never_apply_partial_state() {
        let (mut coordinator, host_id, identity, _) = coordinator_with_terminal();
        assert!(coordinator
            .handle_message(
                host_id.clone(),
                checkpoint_chunk(identity.clone(), &host_id, 6, 0, b"abc"),
            )
            .unwrap()
            .is_empty());
        assert!(coordinator
            .handle_message(
                host_id.clone(),
                checkpoint_chunk(identity.clone(), &host_id, 6, 4, b"ef"),
            )
            .unwrap()
            .is_empty());
        assert!(coordinator
            .handle_message(
                host_id.clone(),
                checkpoint_chunk(identity.clone(), &host_id, 6, 3, b"def"),
            )
            .unwrap()
            .is_empty());

        let oversized = checkpoint_chunk(
            identity,
            &host_id,
            MAX_TERMINAL_SNAPSHOT_BYTES as u64 + 1,
            0,
            b"x",
        );
        assert!(coordinator
            .handle_message(host_id, oversized)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn checkpoint_chunks_do_not_mix_identical_runtime_ids_from_different_hosts() {
        let (mut coordinator, first_host, identity, first_terminal) = coordinator_with_terminal();
        let second_host = ExecutionHostId::new("ssh:other:1").unwrap();
        let second_terminal = TerminalId::alloc();
        coordinator.insert_record_for_test(
            second_terminal.clone(),
            RemoteTerminalCoordinator::managed_for_test(
                second_host.clone(),
                location(&second_host),
                Some(identity.clone()),
                false,
                false,
            ),
        );
        for (host, bytes) in [(&first_host, b"abc"), (&second_host, b"123")] {
            assert!(coordinator
                .handle_message(
                    host.clone(),
                    checkpoint_chunk(identity.clone(), host, 6, 0, bytes),
                )
                .unwrap()
                .is_empty());
        }
        for (host, terminal, bytes, expected) in [
            (&first_host, &first_terminal, b"def", b"abcdef"),
            (&second_host, &second_terminal, b"456", b"123456"),
        ] {
            let effects = coordinator
                .handle_message(
                    host.clone(),
                    checkpoint_chunk(identity.clone(), host, 6, 3, bytes),
                )
                .unwrap();
            assert!(matches!(
                effects.as_slice(),
                [RemoteTerminalEffect::Snapshot { terminal_id, data }]
                    if terminal_id == terminal && data == expected
            ));
        }
    }

    #[test]
    fn reconnect_discards_incomplete_checkpoints_before_accepting_new_output() {
        let (mut coordinator, host, identity, terminal) = coordinator_with_terminal();
        coordinator
            .handle_message(
                host.clone(),
                checkpoint_chunk(identity.clone(), &host, 6, 0, b"old"),
            )
            .unwrap();
        coordinator.clear_pending_for_host(&host);
        assert!(coordinator
            .handle_message(
                host.clone(),
                checkpoint_chunk(identity.clone(), &host, 6, 3, b"end"),
            )
            .unwrap()
            .is_empty());
        let effects = coordinator
            .handle_message(
                host.clone(),
                checkpoint_chunk(identity, &host, 3, 0, b"new"),
            )
            .unwrap();
        assert!(matches!(
            effects.as_slice(),
            [RemoteTerminalEffect::Snapshot { terminal_id, data }]
                if terminal_id == &terminal && data == b"new"
        ));
    }
}
