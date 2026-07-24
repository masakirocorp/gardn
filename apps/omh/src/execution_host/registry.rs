use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::auth::{
    AuthenticationChallenge, AuthenticationOwner, AuthenticationResponse,
    AuthenticationResponseError,
};
use super::connection_catalog::{
    ConnectionCatalog, ConnectionCatalogEvent, ConnectionCatalogPoll, HostConnectionAction,
};
use super::observation::ObservationBroker;
use super::operations::{HostObservation, HostOperationError};
use super::protocol::{
    AttachResume, CommandSpec, CoordinatorInstallationId, CoordinatorMessage, GitStatusSnapshot,
    PortSnapshot, ProcessObservation, ProjectCommandSnapshot, RequestId, RuntimeExitStatus,
    RuntimeIdentity, RuntimeOpSeq, SessionNamespaceId, TerminalSize, TerminateMode,
    WorkerCapability, WorkerMessage, WorktreeSnapshot,
};
use super::remote::WorkerInstaller;
use super::stage_requests::StageRequestTracker;
use super::terminals::{
    JournaledRuntimeOp, PendingCreate, RemoteTerminalCoordinator, RemoteTerminalEffect,
    MAX_RUNTIME_INPUT_CHUNK_BYTES, MAX_RUNTIME_OP_JOURNAL_BYTES, MAX_RUNTIME_OP_JOURNAL_OPS,
};

#[cfg(test)]
use super::auth::AuthenticationChallengeChannel;
#[cfg(test)]
use super::operations::OBSERVATION_REQUEST_TIMEOUT;
#[cfg(test)]
use super::protocol::{WorkerError, WorkerErrorCode};
use super::{ConnectionStatus, ExecutionHostId, HostPath, ResourceLocation};
use crate::layout::PaneId;
use crate::persist::ssh_profiles::SshConnectionProfile;
use crate::terminal::{TerminalId, TerminalRuntime};
#[cfg(test)]
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum ExecutionHostEvent {
    Worker {
        host_id: ExecutionHostId,
        message: WorkerMessage,
    },
    TerminalReady {
        terminal_id: TerminalId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
    },
    TerminalOutput {
        terminal_id: TerminalId,
        data: Vec<u8>,
        reset: bool,
    },
    TerminalStateChanged {
        terminal_id: TerminalId,
        agent: Option<crate::detect::Agent>,
        state: crate::detect::AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    },
    TerminalExited {
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
    TerminalFailed {
        terminal_id: TerminalId,
        message: String,
    },
    Diagnostic {
        host_id: ExecutionHostId,
        message: String,
    },
    TestFinished {
        host_id: ExecutionHostId,
        result: Result<(), String>,
    },
    FileStaged {
        host_id: ExecutionHostId,
        request_id: RequestId,
        location: ResourceLocation,
        result: Result<HostPath, super::protocol::WorkerError>,
    },
}

pub(crate) struct ExecutionHostManager {
    connections: ConnectionCatalog,
    terminals: RemoteTerminalCoordinator,
    lifecycle_events: Vec<ExecutionHostEvent>,
    process_observations: ObservationBroker<TerminalId, ProcessObservation>,
    git_observations: ObservationBroker<ResourceLocation, GitStatusSnapshot>,
    worktree_observations: ObservationBroker<ResourceLocation, Vec<WorktreeSnapshot>>,
    port_observations: ObservationBroker<ResourceLocation, Vec<PortSnapshot>>,
    project_command_observations: ObservationBroker<ResourceLocation, Vec<ProjectCommandSnapshot>>,
    stage_requests: StageRequestTracker,
}

impl ExecutionHostManager {
    pub(crate) fn new(
        installation_id: CoordinatorInstallationId,
        session_namespace_id: SessionNamespaceId,
    ) -> Self {
        Self {
            connections: ConnectionCatalog::new(installation_id, session_namespace_id),
            terminals: RemoteTerminalCoordinator::new(),
            lifecycle_events: Vec::new(),
            process_observations: ObservationBroker::new(),
            git_observations: ObservationBroker::new(),
            worktree_observations: ObservationBroker::new(),
            port_observations: ObservationBroker::new(),
            project_command_observations: ObservationBroker::new(),
            stage_requests: StageRequestTracker::new(),
        }
    }

    /// Converge runtime connections on the persisted profile catalog.
    ///
    /// A target or host-binding-generation change replaces the old runtime so
    /// stale transports cannot be adopted under the new binding.
    pub(crate) fn sync_profiles(&mut self, profiles: &[SshConnectionProfile]) {
        for host_id in self.connections.sync_profiles(profiles) {
            self.mark_host_observations_stale(&host_id);
        }
    }

    #[cfg(test)]
    pub(crate) fn request(
        &mut self,
        profile_id: &str,
        action: HostConnectionAction,
    ) -> Result<Option<ExecutionHostEvent>, String> {
        self.request_for(AuthenticationOwner::SYSTEM, profile_id, action)
    }

    pub(crate) fn request_for(
        &mut self,
        owner: AuthenticationOwner,
        profile_id: &str,
        action: HostConnectionAction,
    ) -> Result<Option<ExecutionHostEvent>, String> {
        Ok(self
            .connections
            .request_for(owner, profile_id, action)?
            .map(|(host_id, result)| ExecutionHostEvent::TestFinished { host_id, result }))
    }

    pub(crate) fn authentication_challenge(
        &self,
        owner: AuthenticationOwner,
    ) -> Option<AuthenticationChallenge> {
        self.connections.authentication_challenge(owner)
    }

    pub(crate) fn respond_to_authentication(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
        response: AuthenticationResponse,
    ) -> Result<(), AuthenticationResponseError> {
        self.connections
            .respond_to_authentication(owner, challenge_id, response)
    }

    pub(crate) fn cancel_authentication(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
    ) -> Result<(), AuthenticationResponseError> {
        self.connections.cancel_authentication(owner, challenge_id)
    }

    pub(crate) fn cancel_authentication_owner(&self, owner: AuthenticationOwner) {
        self.connections.cancel_authentication_owner(owner);
    }

    #[cfg(test)]
    pub(crate) fn authentication_channel_for_test(&self) -> Arc<AuthenticationChallengeChannel> {
        self.connections.authentication_channel_for_test()
    }

    fn transport_error_message(error: HostOperationError) -> String {
        match error {
            HostOperationError::Unavailable { host_id } => {
                format!("execution host {host_id} is not connected")
            }
            other => other.to_string(),
        }
    }

    pub(crate) fn worker_installer_for(
        &self,
        owner: AuthenticationOwner,
        profile_id: &str,
    ) -> Result<WorkerInstaller, String> {
        self.connections.worker_installer_for(owner, profile_id)
    }

    #[cfg(test)]
    pub(crate) fn connect_test_host(
        &mut self,
        host_id: ExecutionHostId,
    ) -> std::sync::Arc<std::sync::Mutex<Vec<CoordinatorMessage>>> {
        self.connections.connect_test_host(host_id)
    }

    /// Validate that a connected worker advertises `capability` before starting
    /// host-routed work that depends on it. Test hosts always pass.
    pub(crate) fn ensure_host_capability(
        &self,
        host_id: &ExecutionHostId,
        capability: WorkerCapability,
    ) -> Result<(), HostOperationError> {
        self.connections.ensure_host_capability(host_id, capability)
    }

    fn send_host_operation(
        &mut self,
        host_id: ExecutionHostId,
        capability: WorkerCapability,
        build: impl FnOnce(RequestId) -> CoordinatorMessage,
    ) -> Result<RequestId, HostOperationError> {
        self.connections
            .send_host_operation(host_id, capability, build)
    }

    pub(crate) fn request_process_observation(
        &mut self,
        terminal_id: &TerminalId,
    ) -> Result<RequestId, HostOperationError> {
        if let Some(request_id) = self.process_observations.inflight(terminal_id) {
            return Ok(request_id);
        }
        let record = self.terminals.get(terminal_id).ok_or_else(|| {
            HostOperationError::Failed("remote terminal is not registered".into())
        })?;
        let identity = record.identity().cloned().ok_or_else(|| {
            HostOperationError::Failed("remote terminal runtime is not ready".into())
        })?;
        let location = record.location().clone();
        let host_id = location.execution_host_id.clone();
        let request_id = self.send_host_operation(
            host_id.clone(),
            WorkerCapability::ProcessObservation,
            |request_id| CoordinatorMessage::ObserveProcess {
                request_id,
                identity,
                location,
            },
        )?;
        Ok(self.process_observations.track_started(
            terminal_id.clone(),
            host_id,
            request_id,
            Instant::now(),
        ))
    }

    pub(crate) fn process_observation(
        &self,
        terminal_id: &TerminalId,
    ) -> Option<&HostObservation<ProcessObservation>> {
        self.process_observations.get(terminal_id)
    }

    pub(crate) fn request_git_status(
        &mut self,
        location: ResourceLocation,
    ) -> Result<RequestId, HostOperationError> {
        if let Some(request_id) = self.git_observations.inflight(&location) {
            return Ok(request_id);
        }
        let expected_location = location.clone();
        let host_id = expected_location.execution_host_id.clone();
        let request_id =
            self.send_host_operation(host_id.clone(), WorkerCapability::Git, |request_id| {
                CoordinatorMessage::GitStatus {
                    request_id,
                    location,
                }
            })?;
        Ok(self.git_observations.track_started(
            expected_location,
            host_id,
            request_id,
            Instant::now(),
        ))
    }

    pub(crate) fn git_status(
        &self,
        location: &ResourceLocation,
    ) -> Option<&HostObservation<GitStatusSnapshot>> {
        self.git_observations.get(location)
    }

    pub(crate) fn request_worktrees(
        &mut self,
        location: ResourceLocation,
    ) -> Result<RequestId, HostOperationError> {
        if let Some(request_id) = self.worktree_observations.inflight(&location) {
            return Ok(request_id);
        }
        let expected_location = location.clone();
        let host_id = expected_location.execution_host_id.clone();
        let request_id =
            self.send_host_operation(host_id.clone(), WorkerCapability::Worktree, |request_id| {
                CoordinatorMessage::ListWorktrees {
                    request_id,
                    location,
                }
            })?;
        Ok(self.worktree_observations.track_started(
            expected_location,
            host_id,
            request_id,
            Instant::now(),
        ))
    }

    pub(crate) fn worktrees(
        &self,
        location: &ResourceLocation,
    ) -> Option<&HostObservation<Vec<WorktreeSnapshot>>> {
        self.worktree_observations.get(location)
    }

    pub(crate) fn request_ports(
        &mut self,
        location: ResourceLocation,
    ) -> Result<RequestId, HostOperationError> {
        if let Some(request_id) = self.port_observations.inflight(&location) {
            return Ok(request_id);
        }
        let expected_location = location.clone();
        let host_id = expected_location.execution_host_id.clone();
        let request_id =
            self.send_host_operation(host_id.clone(), WorkerCapability::Ports, |request_id| {
                CoordinatorMessage::ObservePorts {
                    request_id,
                    location,
                }
            })?;
        Ok(self.port_observations.track_started(
            expected_location,
            host_id,
            request_id,
            Instant::now(),
        ))
    }

    pub(crate) fn ports(
        &self,
        location: &ResourceLocation,
    ) -> Option<&HostObservation<Vec<PortSnapshot>>> {
        self.port_observations.get(location)
    }

    pub(crate) fn request_project_commands(
        &mut self,
        location: ResourceLocation,
    ) -> Result<RequestId, HostOperationError> {
        if let Some(request_id) = self.project_command_observations.inflight(&location) {
            return Ok(request_id);
        }
        let expected_location = location.clone();
        let host_id = expected_location.execution_host_id.clone();
        // Reuse Command capability: discovery is a host FS observation adjacent
        // to RunCommand and does not need a separate handshake bit.
        let request_id =
            self.send_host_operation(host_id.clone(), WorkerCapability::Command, |request_id| {
                CoordinatorMessage::DiscoverProjectCommands {
                    request_id,
                    location,
                }
            })?;
        Ok(self.project_command_observations.track_started(
            expected_location,
            host_id,
            request_id,
            Instant::now(),
        ))
    }

    pub(crate) fn project_commands(
        &self,
        location: &ResourceLocation,
    ) -> Option<&HostObservation<Vec<ProjectCommandSnapshot>>> {
        self.project_command_observations.get(location)
    }

    pub(crate) fn request_stage_file(
        &mut self,
        location: ResourceLocation,
        extension: String,
        data: Vec<u8>,
        ttl_secs: u32,
    ) -> Result<RequestId, HostOperationError> {
        let expected_location = location.clone();
        let host_id = expected_location.execution_host_id.clone();
        let request_id = self.send_host_operation(
            host_id.clone(),
            WorkerCapability::FileStaging,
            |request_id| CoordinatorMessage::StageFile {
                request_id,
                location,
                extension,
                data,
                ttl_secs,
            },
        )?;
        self.stage_requests
            .track(host_id, request_id, expected_location);
        Ok(request_id)
    }

    pub(crate) fn remove_staged_file(
        &mut self,
        location: ResourceLocation,
    ) -> Result<RequestId, HostOperationError> {
        self.send_host_operation(
            location.execution_host_id.clone(),
            WorkerCapability::FileStaging,
            |request_id| CoordinatorMessage::RemoveStagedFile {
                request_id,
                location,
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_terminal(
        &mut self,
        terminal_id: TerminalId,
        pane_id: PaneId,
        location: ResourceLocation,
        rows: u16,
        cols: u16,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        events: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
        command: Option<CommandSpec>,
        env: Vec<(String, String)>,
    ) -> Result<TerminalRuntime, String> {
        if location.is_local() {
            return Err(
                "remote terminal creation requires a non-local resource location".to_string(),
            );
        }
        let host_id = location.execution_host_id.clone();
        let pending_create = PendingCreate::new(
            terminal_id.clone(),
            location,
            TerminalSize { cols, rows },
            command,
            env,
            scrollback_limit_bytes,
        );
        // Register the local proxy and pending request identity before any transport
        // send so a failed send can roll back without orphaning worker state, and a
        // successful send always has a local record ready for ACK/cancel reconcile.
        if !self.connections.has_transport(&host_id) {
            return Err(format!("execution host {host_id} is not connected"));
        }
        let request_id = self
            .connections
            .allocate_request_id(&host_id)
            .ok_or_else(|| format!("execution host {host_id} is not connected"))?;
        let runtime = self.register_remote_terminal(
            request_id,
            pending_create.clone(),
            pane_id,
            scrollback_limit_bytes,
            host_terminal_theme,
            events,
        )?;
        if let Err(error) = self
            .connections
            .send_message(&host_id, &pending_create.message(request_id))
        {
            self.rollback_pending_create(&host_id, request_id, &terminal_id);
            runtime.shutdown();
            return Err(Self::transport_error_message(error));
        }
        Ok(runtime)
    }

    #[allow(clippy::too_many_arguments)]
    fn register_remote_terminal(
        &mut self,
        request_id: RequestId,
        pending_create: PendingCreate,
        pane_id: PaneId,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        events: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    ) -> Result<TerminalRuntime, String> {
        let terminal_id = pending_create.terminal_id().clone();
        let host_id = pending_create.location().execution_host_id.clone();
        let location = pending_create.location().clone();
        let (runtime, control) = TerminalRuntime::remote(
            pane_id,
            pending_create.size().rows,
            pending_create.size().cols,
            scrollback_limit_bytes,
            host_terminal_theme,
            events,
        )
        .map_err(|error| error.to_string())?;
        self.terminals
            .track_pending_create(host_id.clone(), request_id, pending_create);
        self.terminals
            .insert_created(terminal_id, host_id, location, control);
        Ok(runtime)
    }

    fn rollback_pending_create(
        &mut self,
        host_id: &ExecutionHostId,
        request_id: RequestId,
        terminal_id: &TerminalId,
    ) {
        self.terminals.remove_pending_create(host_id, request_id);
        self.terminals.remove(terminal_id);
    }

    /// Cancel an in-flight create without dropping request identity.
    ///
    /// Keeps pending creates so a late/replayed CreateTerminalResult can still be
    /// correlated and any returned runtime identity is driven to termination-pending.
    pub(crate) fn cancel_pending_create(&mut self, terminal_id: &TerminalId) -> bool {
        let had_pending_request = self.terminals.has_pending_create_for(terminal_id);
        let Some(record) = self.terminals.get_mut(terminal_id) else {
            return had_pending_request;
        };
        if record.termination_pending() {
            return true;
        }
        record.set_termination_pending(true);
        record.set_attach_pending(false);
        record.set_adopt_pending(false);
        record.take_control();
        if let Some(identity) = record.identity().cloned() {
            if !record.tombstone_recorded() {
                record.set_tombstone_recorded(true);
                let location = record.location().clone();
                self.lifecycle_events
                    .push(ExecutionHostEvent::TerminationPending {
                        terminal_id: terminal_id.clone(),
                        location,
                        identity,
                    });
            }
        }
        true
    }

    /// Durably mark a known remote runtime for termination (commit/source failure).
    pub(crate) fn begin_runtime_termination(
        &mut self,
        terminal_id: TerminalId,
        location: ResourceLocation,
        identity: RuntimeIdentity,
    ) {
        if let Some(record) = self.terminals.get_mut(&terminal_id) {
            record.set_location(location.clone());
            record.set_identity(Some(identity.clone()));
            record.set_termination_pending(true);
            record.set_attach_pending(false);
            record.set_adopt_pending(false);
            record.take_control();
            if !record.tombstone_recorded() {
                record.set_tombstone_recorded(true);
                self.lifecycle_events
                    .push(ExecutionHostEvent::TerminationPending {
                        terminal_id,
                        location,
                        identity,
                    });
            }
            return;
        }
        self.restore_termination_pending(terminal_id.clone(), location.clone(), identity.clone());
        self.lifecycle_events
            .push(ExecutionHostEvent::TerminationPending {
                terminal_id,
                location,
                identity,
            });
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn adopt_terminal(
        &mut self,
        terminal_id: TerminalId,
        pane_id: PaneId,
        location: ResourceLocation,
        identity: RuntimeIdentity,
        rows: u16,
        cols: u16,
        scrollback_limit_bytes: usize,
        host_terminal_theme: crate::terminal_theme::TerminalTheme,
        events: tokio::sync::mpsc::Sender<crate::events::AppEvent>,
    ) -> Result<TerminalRuntime, String> {
        let (runtime, control) = TerminalRuntime::remote(
            pane_id,
            rows,
            cols,
            scrollback_limit_bytes,
            host_terminal_theme,
            events,
        )
        .map_err(|error| error.to_string())?;
        self.terminals
            .insert_adopted(terminal_id, location, identity, control);
        Ok(runtime)
    }

    fn flush_terminal_adopts(&mut self, events: &mut Vec<ExecutionHostEvent>) {
        let in_flight = self.terminals.pending_adopt_terminal_ids();
        let terminal_ids = self
            .terminals
            .iter()
            .filter(|(terminal_id, record)| {
                record.adopt_pending()
                    && !record.termination_pending()
                    && record.identity().is_some()
                    && !in_flight.contains(*terminal_id)
            })
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            let Some(record) = self.terminals.get(&terminal_id) else {
                continue;
            };
            let Some(identity) = record.identity().cloned() else {
                continue;
            };
            let host_id = record.host_id().clone();
            let location = record.location().clone();
            match self
                .connections
                .allocate_and_send(&host_id, true, |request_id| {
                    CoordinatorMessage::AdoptTerminal {
                        request_id,
                        identity,
                        location,
                    }
                }) {
                Ok(request_id) => {
                    self.terminals
                        .track_pending_adopt(host_id, request_id, terminal_id);
                }
                Err(HostOperationError::Unavailable { .. }) => {}
                Err(error) => events.push(ExecutionHostEvent::Diagnostic {
                    host_id,
                    message: error.to_string(),
                }),
            }
        }
    }

    fn flush_terminal_attaches(&mut self, events: &mut Vec<ExecutionHostEvent>) {
        let terminal_ids = self
            .terminals
            .iter()
            .filter(|(_, record)| {
                record.attach_pending()
                    && record.op_seq_ready()
                    && !record.adopt_pending()
                    && !record.termination_pending()
            })
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            let Some(record) = self.terminals.get(&terminal_id) else {
                continue;
            };
            let Some(identity) = record.identity().cloned() else {
                continue;
            };
            let host_id = record.host_id().clone();
            let location = record.location().clone();
            let revision = record.output_revision();
            let resume = if revision.get() == 0 {
                AttachResume::Checkpoint
            } else {
                AttachResume::AfterRevision(revision)
            };
            match self
                .connections
                .allocate_and_send(&host_id, true, |request_id| {
                    CoordinatorMessage::AttachTerminal {
                        request_id,
                        identity,
                        location,
                        resume,
                    }
                }) {
                Ok(_) => {
                    if let Some(record) = self.terminals.get_mut(&terminal_id) {
                        record.set_attach_pending(false);
                    }
                }
                Err(HostOperationError::Unavailable { .. }) => {}
                Err(error) => events.push(ExecutionHostEvent::Diagnostic {
                    host_id,
                    message: error.to_string(),
                }),
            }
        }
    }

    pub(crate) fn retain_terminals(&mut self, live: &HashSet<TerminalId>) {
        let removed = self
            .terminals
            .terminal_ids()
            .filter(|terminal_id| !live.contains(*terminal_id))
            .cloned()
            .collect::<Vec<_>>();
        for terminal_id in removed {
            let tombstone = self.terminals.get_mut(&terminal_id).and_then(|record| {
                if record.termination_pending() {
                    return None;
                }
                record.set_termination_pending(true);
                record.set_attach_pending(false);
                record.set_adopt_pending(false);
                let identity = record.identity().cloned()?;
                record.set_tombstone_recorded(true);
                Some((record.location().clone(), identity))
            });
            if let Some((location, identity)) = tombstone {
                self.lifecycle_events
                    .push(ExecutionHostEvent::TerminationPending {
                        terminal_id,
                        location,
                        identity,
                    });
            }
        }
    }

    pub(crate) fn restore_termination_pending(
        &mut self,
        terminal_id: TerminalId,
        location: ResourceLocation,
        identity: RuntimeIdentity,
    ) {
        self.terminals
            .restore_termination_pending(terminal_id, location, identity);
    }

    pub(crate) fn forget_terminal(&mut self, terminal_id: &TerminalId) -> bool {
        self.terminals.forget_terminal(terminal_id)
    }

    pub(crate) fn has_host_references(&self, host_id: &ExecutionHostId) -> bool {
        self.terminals.has_host_references(host_id)
    }

    fn flush_terminal_terminations(&mut self, events: &mut Vec<ExecutionHostEvent>) {
        let in_flight = self.terminals.pending_termination_terminal_ids();
        let terminal_ids = self
            .terminals
            .iter()
            .filter(|(terminal_id, record)| {
                record.termination_pending()
                    && record.identity().is_some()
                    && !in_flight.contains(*terminal_id)
            })
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            if let Err((host_id, message)) = self.send_termination(&terminal_id) {
                events.push(ExecutionHostEvent::Diagnostic { host_id, message });
            }
        }
    }

    fn send_termination(
        &mut self,
        terminal_id: &TerminalId,
    ) -> Result<(), (ExecutionHostId, String)> {
        let Some(record) = self.terminals.get(terminal_id) else {
            return Ok(());
        };
        let Some(identity) = record.identity().cloned() else {
            return Ok(());
        };
        let host_id = record.host_id().clone();
        let location = record.location().clone();
        let request_id = match self
            .connections
            .allocate_and_send(&host_id, true, |request_id| CoordinatorMessage::Terminate {
                request_id,
                identity,
                location,
                mode: TerminateMode::Terminate,
            }) {
            Ok(request_id) => request_id,
            Err(HostOperationError::Unavailable { .. }) => return Ok(()),
            Err(error) => return Err((host_id, error.to_string())),
        };
        self.terminals
            .track_pending_termination(host_id, request_id, terminal_id.clone());
        Ok(())
    }

    fn replay_pending_creates(
        &self,
        host_id: &ExecutionHostId,
        events: &mut Vec<ExecutionHostEvent>,
    ) {
        let should_replay = |pending: &PendingCreate| {
            self.terminals
                .get(pending.terminal_id())
                .is_none_or(|record| !record.termination_pending())
        };
        if !self.connections.has_transport(host_id) {
            events.push(ExecutionHostEvent::Diagnostic {
                host_id: host_id.clone(),
                message: "execution host reconnected without a worker sender".to_string(),
            });
            return;
        }
        for ((pending_host_id, request_id), pending) in self.terminals.pending_creates() {
            if pending_host_id != host_id || !should_replay(pending) {
                continue;
            }
            if let Err(error) = self
                .connections
                .send_message(host_id, &pending.message(*request_id))
            {
                events.push(ExecutionHostEvent::Diagnostic {
                    host_id: host_id.clone(),
                    message: format!("failed to replay terminal creation: {error}"),
                });
            }
        }
    }

    fn replay_pending_terminations(
        &self,
        host_id: &ExecutionHostId,
        events: &mut Vec<ExecutionHostEvent>,
    ) {
        let messages = self
            .terminals
            .pending_terminations()
            .filter_map(|((pending_host_id, request_id), terminal_id)| {
                if pending_host_id != host_id {
                    return None;
                }
                let record = self.terminals.get(terminal_id)?;
                let identity = record.identity().cloned()?;
                Some(CoordinatorMessage::Terminate {
                    request_id: *request_id,
                    identity,
                    location: record.location().clone(),
                    mode: TerminateMode::Terminate,
                })
            })
            .collect::<Vec<_>>();
        if !self.connections.has_transport(host_id) {
            events.push(ExecutionHostEvent::Diagnostic {
                host_id: host_id.clone(),
                message: "execution host reconnected without a worker sender".to_string(),
            });
            return;
        }
        for message in messages {
            if let Err(error) = self.connections.send_message(host_id, &message) {
                events.push(ExecutionHostEvent::Diagnostic {
                    host_id: host_id.clone(),
                    message: format!("failed to replay terminal termination: {error}"),
                });
            }
        }
    }

    fn allocate_runtime_op_request_id(&mut self, host_id: &ExecutionHostId) -> Option<RequestId> {
        // Prefer the live worker sender counter when connected. When offline, use the
        // host's persistent next_request_id Arc so reconnect/live ids share one
        // monotonic namespace and cannot collide. Test hosts share the same
        // saturating overflow policy via ConnectionCatalog.
        self.connections.allocate_request_id(host_id)
    }

    fn journaled_op_bytes(op: &JournaledRuntimeOp) -> usize {
        match op {
            JournaledRuntimeOp::Input { data } => data.len(),
            JournaledRuntimeOp::Resize { .. } => 0,
        }
    }

    fn journal_retained_bytes(record: &super::terminals::ManagedRemoteTerminal) -> usize {
        record
            .op_journal()
            .iter()
            .map(|pending| Self::journaled_op_bytes(pending.op()))
            .sum()
    }

    fn fail_runtime_journal_overflow(
        &mut self,
        terminal_id: &TerminalId,
        host_id: ExecutionHostId,
        events: &mut Vec<ExecutionHostEvent>,
        detail: &str,
    ) {
        if let Some(record) = self.terminals.get_mut(terminal_id) {
            record.clear_op_journal();
            record.take_control();
            record.set_termination_pending(true);
            if !record.tombstone_recorded() {
                if let Some(identity) = record.identity().cloned() {
                    record.set_tombstone_recorded(true);
                    events.push(ExecutionHostEvent::TerminationPending {
                        terminal_id: terminal_id.clone(),
                        location: record.location().clone(),
                        identity,
                    });
                }
            }
        }
        events.push(ExecutionHostEvent::TerminalFailed {
            terminal_id: terminal_id.clone(),
            message: format!("runtime input journal unavailable: {detail}"),
        });
        events.push(ExecutionHostEvent::Diagnostic {
            host_id,
            message: format!("runtime input journal unavailable for {terminal_id}: {detail}"),
        });
    }

    fn enqueue_runtime_op(
        &mut self,
        terminal_id: &TerminalId,
        op: JournaledRuntimeOp,
        events: &mut Vec<ExecutionHostEvent>,
    ) {
        // Chunk oversized input so every wire frame stays below MAX_FRAME_SIZE.
        if let JournaledRuntimeOp::Input { data } = &op {
            if data.len() > MAX_RUNTIME_INPUT_CHUNK_BYTES {
                for chunk in data.chunks(MAX_RUNTIME_INPUT_CHUNK_BYTES) {
                    self.enqueue_runtime_op(
                        terminal_id,
                        JournaledRuntimeOp::Input {
                            data: chunk.to_vec(),
                        },
                        events,
                    );
                }
                return;
            }
        }

        let Some(record) = self.terminals.get(terminal_id) else {
            return;
        };
        if record.termination_pending() || !record.op_seq_ready() || record.identity().is_none() {
            return;
        }
        let host_id = record.host_id().clone();
        let next_op_seq = record.next_op_seq();
        let current_ops = record.op_journal().len();
        let current_bytes = Self::journal_retained_bytes(record);
        let op_bytes = Self::journaled_op_bytes(&op);
        if current_ops >= MAX_RUNTIME_OP_JOURNAL_OPS
            || current_bytes.saturating_add(op_bytes) > MAX_RUNTIME_OP_JOURNAL_BYTES
        {
            self.fail_runtime_journal_overflow(
                terminal_id,
                host_id,
                events,
                &format!(
                    "capacity exceeded (ops={current_ops}/{}, bytes={}/{})",
                    MAX_RUNTIME_OP_JOURNAL_OPS,
                    current_bytes.saturating_add(op_bytes),
                    MAX_RUNTIME_OP_JOURNAL_BYTES
                ),
            );
            return;
        }
        let Some(request_id) = self.allocate_runtime_op_request_id(&host_id) else {
            // Unknown host binding: leave control bytes queued for a later poll.
            return;
        };
        let Some(record) = self.terminals.get(terminal_id) else {
            return;
        };
        if record.termination_pending() || !record.op_seq_ready() || record.identity().is_none() {
            return;
        }
        let op_seq = RuntimeOpSeq::new(next_op_seq.max(record.next_op_seq()));
        if !self
            .terminals
            .push_journaled_op(terminal_id, request_id, op_seq, op)
        {
            return;
        }
        // send_journaled_op is a no-op when the host has no sender; the journal
        // retains the op for reconnect replay.
        if let Err((host_id, message)) = self.send_journaled_op(terminal_id, request_id) {
            events.push(ExecutionHostEvent::Diagnostic { host_id, message });
        }
    }

    fn send_journaled_op(
        &mut self,
        terminal_id: &TerminalId,
        request_id: RequestId,
    ) -> Result<(), (ExecutionHostId, String)> {
        let Some(record) = self.terminals.get(terminal_id) else {
            return Ok(());
        };
        let Some(pending) = record
            .op_journal()
            .iter()
            .find(|pending| pending.request_id() == request_id)
            .cloned()
        else {
            return Ok(());
        };
        let Some(identity) = record.identity().cloned() else {
            return Ok(());
        };
        let host_id = record.host_id().clone();
        let location = record.location().clone();
        let message = match pending.op().clone() {
            JournaledRuntimeOp::Input { data } => CoordinatorMessage::Input {
                request_id: pending.request_id(),
                identity,
                location,
                op_seq: pending.op_seq(),
                data,
            },
            JournaledRuntimeOp::Resize { size } => CoordinatorMessage::Resize {
                request_id: pending.request_id(),
                identity,
                location,
                op_seq: pending.op_seq(),
                size,
            },
        };
        self.connections
            .send_message(&host_id, &message)
            .map_err(|error| (host_id, error.to_string()))
    }

    fn replay_pending_runtime_ops(
        &mut self,
        host_id: &ExecutionHostId,
        events: &mut Vec<ExecutionHostEvent>,
    ) {
        let terminal_ids = self
            .terminals
            .iter()
            .filter(|(_, record)| {
                record.host_id() == host_id
                    && record.op_seq_ready()
                    && !record.termination_pending()
                    && !record.op_journal().is_empty()
            })
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            let request_ids = self
                .terminals
                .get(&terminal_id)
                .map(|record| {
                    record
                        .op_journal()
                        .iter()
                        .map(|pending| pending.request_id())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for request_id in request_ids {
                if let Err((host_id, message)) = self.send_journaled_op(&terminal_id, request_id) {
                    events.push(ExecutionHostEvent::Diagnostic { host_id, message });
                    break;
                }
            }
        }
    }

    fn can_journal_runtime_ops(&self, host_id: &ExecutionHostId) -> bool {
        self.connections.can_journal_runtime_ops(host_id)
    }

    fn flush_terminal_controls(&mut self, events: &mut Vec<ExecutionHostEvent>) {
        let terminal_ids = self.terminals.terminal_ids().cloned().collect::<Vec<_>>();
        for terminal_id in terminal_ids {
            let Some(host_id) = self.terminals.get(&terminal_id).and_then(|record| {
                if record.termination_pending()
                    || !record.op_seq_ready()
                    || record.identity().is_none()
                {
                    return None;
                }
                Some(record.host_id().clone())
            }) else {
                continue;
            };
            // Do not drain the local control queues unless we can journal the ops.
            // Otherwise disconnect→type→poll would drop input permanently.
            if !self.can_journal_runtime_ops(&host_id) {
                continue;
            }
            let Some(record) = self.terminals.get_mut(&terminal_id) else {
                continue;
            };
            let Some(control) = record.control_mut() else {
                continue;
            };
            let mut pending_ops = Vec::new();
            while let Ok(data) = control.try_recv_input() {
                pending_ops.push(JournaledRuntimeOp::Input {
                    data: data.to_vec(),
                });
            }
            if let Some((rows, cols, _, _)) = control.take_resize() {
                pending_ops.push(JournaledRuntimeOp::Resize {
                    size: TerminalSize { cols, rows },
                });
            }
            for op in pending_ops {
                self.enqueue_runtime_op(&terminal_id, op, events);
            }
        }
    }

    fn apply_terminal_effects(
        &mut self,
        effects: Vec<RemoteTerminalEffect>,
        events: &mut Vec<ExecutionHostEvent>,
    ) {
        for effect in effects {
            match effect {
                RemoteTerminalEffect::Ready {
                    terminal_id,
                    identity,
                    location,
                } => events.push(ExecutionHostEvent::TerminalReady {
                    terminal_id,
                    identity,
                    location,
                }),
                RemoteTerminalEffect::Output {
                    terminal_id,
                    data,
                    reset,
                } => events.push(ExecutionHostEvent::TerminalOutput {
                    terminal_id,
                    data,
                    reset,
                }),
                RemoteTerminalEffect::StateChanged {
                    terminal_id,
                    agent,
                    state,
                    visible_blocker,
                    visible_idle,
                    visible_working,
                    process_exited,
                } => events.push(ExecutionHostEvent::TerminalStateChanged {
                    terminal_id,
                    agent,
                    state,
                    visible_blocker,
                    visible_idle,
                    visible_working,
                    process_exited,
                }),
                RemoteTerminalEffect::Exited {
                    terminal_id,
                    status,
                } => events.push(ExecutionHostEvent::TerminalExited {
                    terminal_id,
                    status,
                }),
                RemoteTerminalEffect::TerminationPending {
                    terminal_id,
                    location,
                    identity,
                } => {
                    // Match prior create/cancel paths: queue on lifecycle so poll
                    // coalesces tombstones with other deferred host events.
                    self.lifecycle_events
                        .push(ExecutionHostEvent::TerminationPending {
                            terminal_id,
                            location,
                            identity,
                        });
                }
                RemoteTerminalEffect::TerminationFinished { terminal_id } => {
                    events.push(ExecutionHostEvent::TerminationFinished { terminal_id })
                }
                RemoteTerminalEffect::Failed {
                    terminal_id,
                    message,
                } => events.push(ExecutionHostEvent::TerminalFailed {
                    terminal_id,
                    message,
                }),
                RemoteTerminalEffect::Diagnostic { host_id, message } => {
                    events.push(ExecutionHostEvent::Diagnostic { host_id, message })
                }
                RemoteTerminalEffect::Attach {
                    host_id,
                    terminal_id,
                    identity,
                    location,
                    resume,
                } => {
                    match self
                        .connections
                        .allocate_and_send(&host_id, true, |request_id| {
                            CoordinatorMessage::AttachTerminal {
                                request_id,
                                identity,
                                location,
                                resume,
                            }
                        }) {
                        Ok(_) => {
                            if let Some(record) = self.terminals.get_mut(&terminal_id) {
                                if record.attach_pending() {
                                    record.set_attach_pending(false);
                                }
                            }
                        }
                        Err(HostOperationError::Unavailable { .. }) => {}
                        Err(error) => events.push(ExecutionHostEvent::Diagnostic {
                            host_id,
                            message: error.to_string(),
                        }),
                    }
                }
                RemoteTerminalEffect::UnhandledRequestAck {
                    host_id,
                    request_id,
                    error,
                } => events.push(ExecutionHostEvent::Worker {
                    host_id,
                    message: WorkerMessage::RequestAck { request_id, error },
                }),
            }
        }
    }

    /// Shallow dispatcher: terminal messages go to the coordinator; observation
    /// and staging responses go to their brokers; everything else is surfaced.
    pub(crate) fn route_worker_message(
        &mut self,
        host_id: ExecutionHostId,
        message: WorkerMessage,
        events: &mut Vec<ExecutionHostEvent>,
    ) {
        match message {
            WorkerMessage::CreateTerminalResult { .. }
            | WorkerMessage::AdoptTerminalResult { .. }
            | WorkerMessage::OutputDelta { .. }
            | WorkerMessage::OutputCheckpoint { .. }
            | WorkerMessage::TerminalStateChanged { .. }
            | WorkerMessage::RuntimeExit { .. }
            | WorkerMessage::AttachTerminalResult { .. }
            | WorkerMessage::RequestAck { .. } => {
                if let Some(effects) = self.terminals.handle_message(host_id, message) {
                    self.apply_terminal_effects(effects, events);
                }
            }
            WorkerMessage::ProcessObservationResult {
                request_id,
                identity,
                location,
                process,
                error,
            } => {
                let Some(terminal_id) =
                    self.process_observations.take_pending(&host_id, request_id)
                else {
                    return;
                };
                let response_matches = self.terminals.get(&terminal_id).is_some_and(|record| {
                    record.matches_process_observation(&host_id, &identity, &location)
                });
                self.process_observations.complete_keyed(
                    &terminal_id,
                    request_id,
                    response_matches,
                    process,
                    error,
                    "process observation response did not match the pending request",
                );
            }
            WorkerMessage::GitStatusResult {
                request_id,
                location,
                status,
                error,
            } => {
                self.git_observations.complete_location(
                    host_id,
                    request_id,
                    location,
                    status,
                    error,
                    "git status",
                );
            }
            WorkerMessage::WorktreeListResult {
                request_id,
                location,
                worktrees,
                error,
            } => {
                let payload = worktrees
                    .iter()
                    .all(|worktree| worktree.location.execution_host_id == host_id)
                    .then_some(worktrees);
                self.worktree_observations.complete_location(
                    host_id,
                    request_id,
                    location,
                    payload,
                    error,
                    "worktree list",
                );
            }
            WorkerMessage::PortObservationResult {
                request_id,
                location,
                ports,
                error,
            } => {
                let payload = ports
                    .iter()
                    .all(|port| port.execution_host_id == host_id)
                    .then_some(ports);
                self.port_observations.complete_location(
                    host_id,
                    request_id,
                    location,
                    payload,
                    error,
                    "port observation",
                );
            }
            WorkerMessage::ProjectCommandsResult {
                request_id,
                location,
                commands,
                error,
            } => {
                self.project_command_observations
                    .complete_project_commands(host_id, request_id, location, commands, error);
            }
            WorkerMessage::StageFileResult {
                request_id,
                location,
                path,
                error,
            } => {
                if let Some(completion) = self
                    .stage_requests
                    .complete(&host_id, request_id, &location, path, error)
                {
                    events.push(ExecutionHostEvent::FileStaged {
                        host_id: completion.host_id,
                        request_id: completion.request_id,
                        location: completion.location,
                        result: completion.result,
                    });
                }
            }
            message => events.push(ExecutionHostEvent::Worker { host_id, message }),
        }
    }

    fn mark_host_observations_stale(&mut self, host_id: &ExecutionHostId) {
        let host_terminal_ids = self.terminals.host_terminal_ids(host_id);
        self.process_observations
            .mark_stale_where(|terminal_id| host_terminal_ids.contains(terminal_id));
        self.process_observations
            .drop_pending_for_host(host_id, |terminal_id| {
                host_terminal_ids.contains(terminal_id)
            });
        self.git_observations.mark_host_locations_stale(host_id);
        self.worktree_observations
            .mark_host_locations_stale(host_id);
        self.port_observations.mark_host_locations_stale(host_id);
        self.project_command_observations
            .mark_host_locations_stale(host_id);
        for completion in self.stage_requests.fail_host(host_id) {
            self.lifecycle_events.push(ExecutionHostEvent::FileStaged {
                host_id: completion.host_id,
                request_id: completion.request_id,
                location: completion.location,
                result: completion.result,
            });
        }
    }

    fn expire_pending_observations(&mut self, now: Instant) {
        self.process_observations.expire_pending(now);
        self.git_observations.expire_pending(now);
        self.worktree_observations.expire_pending(now);
        self.port_observations.expire_pending(now);
        self.project_command_observations.expire_pending(now);
    }

    pub(crate) fn has_active_connections(&self) -> bool {
        self.connections.has_active_connections()
    }

    pub(crate) fn poll(
        &mut self,
        now: Instant,
    ) -> (
        HashMap<ExecutionHostId, ConnectionStatus>,
        Vec<ExecutionHostEvent>,
    ) {
        let mut events = std::mem::take(&mut self.lifecycle_events);
        self.flush_terminal_controls(&mut events);
        let ConnectionCatalogPoll {
            statuses,
            reconnected_hosts,
            worker_messages,
            events: catalog_events,
        } = self.connections.poll(now);
        for event in catalog_events {
            match event {
                ConnectionCatalogEvent::Diagnostic { host_id, message } => {
                    events.push(ExecutionHostEvent::Diagnostic { host_id, message });
                }
                ConnectionCatalogEvent::TestFinished { host_id, result } => {
                    events.push(ExecutionHostEvent::TestFinished { host_id, result });
                }
            }
        }
        self.expire_pending_observations(now);
        for host_id in statuses
            .iter()
            .filter(|(_, status)| **status != ConnectionStatus::Connected)
            .map(|(host_id, _)| host_id.clone())
            .collect::<Vec<_>>()
        {
            self.mark_host_observations_stale(&host_id);
        }
        for host_id in reconnected_hosts {
            self.replay_pending_creates(&host_id, &mut events);
            self.replay_pending_terminations(&host_id, &mut events);
            self.replay_pending_runtime_ops(&host_id, &mut events);
            // Drop in-flight adopt request ids; reconnect will re-adopt and re-validate seq.
            self.terminals.clear_pending_adopts_for_host(&host_id);
            for record in self.terminals.values_mut() {
                if record.host_id() == &host_id
                    && record.identity().is_some()
                    && !record.termination_pending()
                {
                    record.set_adopt_pending(true);
                    record.set_op_seq_ready(false);
                    record.set_attach_pending(false);
                }
            }
        }
        for (host_id, message) in worker_messages {
            self.route_worker_message(host_id, message, &mut events);
        }
        self.flush_terminal_terminations(&mut events);
        self.flush_terminal_adopts(&mut events);
        self.flush_terminal_attaches(&mut events);
        (statuses, events)
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::OutputRevision;
    use super::super::terminals::PendingRuntimeOp;
    use super::*;
    use crate::execution_host::HostPath;

    fn manager() -> ExecutionHostManager {
        ExecutionHostManager::new(
            CoordinatorInstallationId::new("install-a").unwrap(),
            SessionNamespaceId::new("session-a").unwrap(),
        )
    }

    #[test]
    fn profile_sync_replaces_changed_host_binding_and_removes_deleted_profiles() {
        let mut manager = manager();
        let mut first = SshConnectionProfile::new(
            "workbox",
            "Work box",
            "first.example",
            Some(HostPath::new("/srv/work").unwrap()),
        )
        .unwrap();
        manager.sync_profiles(&[first.clone()]);
        let first_host_id = first.execution_host_id();
        assert_eq!(manager.connections.host_count(), 1);

        first.set_target("second.example").unwrap();
        let second_host_id = first.execution_host_id();
        manager.sync_profiles(&[first]);
        assert_ne!(first_host_id, second_host_id);
        assert!(!manager.connections.contains_host(&first_host_id));
        assert!(manager.connections.contains_host(&second_host_id));

        manager.sync_profiles(&[]);
        assert!(manager.connections.is_empty());
    }

    #[tokio::test]
    async fn persisted_remote_terminal_is_adopted_pending_attach() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events, _event_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events,
            )
            .unwrap();

        let record = manager.terminals.get(&terminal_id).unwrap();
        assert_eq!(record.host_id(), &host_id);
        assert_eq!(record.location(), &location);
        assert_eq!(record.identity(), Some(&identity));
        assert!(record.adopt_pending());
        assert!(!record.op_seq_ready());
        assert!(!record.attach_pending());
        runtime.shutdown();
    }

    #[tokio::test]
    async fn adopt_result_sets_next_op_seq_and_first_input_uses_last_plus_one() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();

        // Input before validated adopt is buffered locally and must not emit yet.
        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"early"))
            .unwrap();
        manager.flush_terminal_controls(&mut Vec::new());
        assert!(messages
            .lock()
            .map(|messages| messages.is_empty())
            .unwrap_or(false));

        manager.flush_terminal_adopts(&mut Vec::new());
        let adopt_request_id = {
            let locked = messages.lock().expect("test worker message lock");
            match locked.as_slice() {
                [CoordinatorMessage::AdoptTerminal {
                    request_id,
                    identity: adopted_identity,
                    location: adopted_location,
                }] if adopted_identity == &identity && adopted_location == &location => *request_id,
                other => panic!("expected single adopt request, got {other:?}"),
            }
        };
        messages.lock().expect("test worker message lock").clear();

        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::AdoptTerminalResult {
                request_id: adopt_request_id,
                identity: Some(identity.clone()),
                location: location.clone(),
                last_applied_op_seq: RuntimeOpSeq::new(7),
                error: None,
            },
            &mut Vec::new(),
        );

        let record = manager.terminals.get(&terminal_id).unwrap();
        assert!(record.op_seq_ready());
        assert!(!record.adopt_pending());
        assert!(record.attach_pending());
        assert_eq!(record.next_op_seq(), 8);

        // Buffered pre-adopt input flushes first at last_applied+1.
        manager.flush_terminal_controls(&mut Vec::new());
        {
            let locked = messages.lock().expect("test worker message lock");
            assert!(
                locked.iter().any(|message| matches!(
                    message,
                    CoordinatorMessage::Input {
                        identity: input_identity,
                        location: input_location,
                        op_seq,
                        data,
                        ..
                    } if input_identity == &identity
                        && input_location == &location
                        && op_seq.get() == 8
                        && data.as_slice() == b"early"
                )),
                "first post-adopt op must be last_applied+1 for buffered input, got {locked:?}"
            );
        }
        assert_eq!(
            manager.terminals.get(&terminal_id).unwrap().next_op_seq(),
            9
        );
        messages.lock().expect("test worker message lock").clear();

        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"post-adopt"))
            .unwrap();
        manager.flush_terminal_controls(&mut Vec::new());
        let locked = messages.lock().expect("test worker message lock");
        assert!(
            locked.iter().any(|message| matches!(
                message,
                CoordinatorMessage::Input {
                    identity: input_identity,
                    location: input_location,
                    op_seq,
                    data,
                    ..
                } if input_identity == &identity
                    && input_location == &location
                    && op_seq.get() == 9
                    && data.as_slice() == b"post-adopt"
            )),
            "next input after buffered flush must continue at last+2, got {locked:?}"
        );
        assert_eq!(
            manager.terminals.get(&terminal_id).unwrap().next_op_seq(),
            10
        );
        runtime.shutdown();
    }

    #[tokio::test]
    async fn offline_close_retains_termination_tombstone_until_acknowledged() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();

        manager.retain_terminals(&HashSet::new());
        let (_, events) = manager.poll(Instant::now());

        assert!(manager.has_host_references(&host_id));
        let retained = manager.terminals.get(&terminal_id).unwrap();
        assert!(retained.termination_pending());
        assert_eq!(retained.location(), &location);
        assert_eq!(retained.identity(), Some(&identity));
        assert!(events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::TerminationPending {
                terminal_id: pending_id,
                location: pending_location,
                identity: pending_identity,
            } if pending_id == &terminal_id
                && pending_location == &location
                && pending_identity == &identity
        )));
        runtime.shutdown();
    }

    #[test]
    fn explicit_forget_drops_offline_mapping_without_terminating() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        manager.restore_termination_pending(terminal_id.clone(), location, identity);

        assert!(manager.forget_terminal(&terminal_id));
        assert!(!manager.has_host_references(&host_id));
        assert!(!manager.forget_terminal(&terminal_id));
    }

    #[test]
    fn unknown_profile_request_returns_a_specific_error() {
        let mut manager = manager();
        let error = manager
            .request("missing", HostConnectionAction::Connect)
            .unwrap_err();
        assert!(error.contains("unknown SSH connection profile missing"));
    }

    #[tokio::test]
    async fn remote_terminal_process_observation_is_sent_to_its_worker() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id, HostPath::new("/srv/same-path").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events, _event_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events,
            )
            .unwrap();

        manager.request_process_observation(&terminal_id).unwrap();

        let routed = messages
            .lock()
            .map(|messages| {
                messages.iter().any(|message| {
                    matches!(
                        message,
                        CoordinatorMessage::ObserveProcess {
                            identity: observed_identity,
                            location: observed_location,
                            ..
                        } if observed_identity == &identity && observed_location == &location
                    )
                })
            })
            .unwrap_or(false);
        assert!(routed);
        runtime.shutdown();
    }

    #[tokio::test]
    async fn reconnect_replays_identical_create_with_the_same_request_id() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let messages = manager.connect_test_host(host_id.clone());
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .create_terminal(
                terminal_id,
                PaneId::alloc(),
                location,
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
                None,
                vec![("TERM".to_string(), "xterm-256color".to_string())],
            )
            .unwrap();
        let original = {
            let Ok(mut messages) = messages.lock() else {
                panic!("test worker message lock is poisoned");
            };
            let original = messages[0].clone();
            messages.clear();
            original
        };

        manager.replay_pending_creates(&host_id, &mut Vec::new());

        let Ok(replayed) = messages.lock() else {
            panic!("test worker message lock is poisoned");
        };
        assert_eq!(replayed.as_slice(), &[original]);
        runtime.shutdown();
    }

    #[tokio::test]
    async fn create_registers_local_proxy_before_transport_send() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .create_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location,
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
                None,
                Vec::new(),
            )
            .unwrap();

        assert!(
            manager.terminals.contains(&terminal_id),
            "local proxy must exist after create"
        );
        assert!(
            manager.terminals.has_pending_create_for(&terminal_id),
            "pending create identity must be retained"
        );
        assert_eq!(
            messages.lock().map(|m| m.len()).unwrap_or(0),
            1,
            "exactly one CreateTerminal must be sent"
        );
        let record = manager.terminals.get(&terminal_id).unwrap();
        assert!(record.op_seq_ready());
        assert!(record.op_journal().is_empty());
        assert_eq!(record.output_revision(), OutputRevision::new(0));
        runtime.shutdown();
    }

    #[tokio::test]
    async fn create_proxy_setup_failure_does_not_send_or_register() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        // No connected host / test worker → sender missing.
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let err = match manager.create_terminal(
            terminal_id.clone(),
            PaneId::alloc(),
            location,
            24,
            80,
            1024,
            crate::terminal_theme::TerminalTheme::default(),
            events_tx,
            None,
            Vec::new(),
        ) {
            Ok(runtime) => {
                runtime.shutdown();
                panic!("disconnected host must fail create");
            }
            Err(error) => error,
        };
        assert!(err.contains("not connected"), "unexpected: {err}");
        assert!(!manager.terminals.contains(&terminal_id));
        assert!(!manager.terminals.contains(&terminal_id));
    }

    #[tokio::test]
    async fn cancel_pending_create_skips_replay_and_late_ack_terminates() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .create_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
                None,
                Vec::new(),
            )
            .unwrap();
        let request_id = manager
            .terminals
            .pending_create_request_id(&host_id, &terminal_id)
            .expect("pending create request id");
        {
            let Ok(mut messages) = messages.lock() else {
                panic!("test worker message lock is poisoned");
            };
            messages.clear();
        }

        assert!(manager.cancel_pending_create(&terminal_id));
        assert!(
            manager
                .terminals
                .get(&terminal_id)
                .unwrap()
                .termination_pending(),
            "cancel must mark termination-pending while retaining request identity"
        );
        assert!(
            manager
                .terminals
                .has_pending_create_request(&host_id, request_id),
            "cancel must retain pending create for late ACK reconcile"
        );

        manager.replay_pending_creates(&host_id, &mut Vec::new());
        assert!(
            messages
                .lock()
                .map(|messages| messages.is_empty())
                .unwrap_or(false),
            "cancelled creates must not be replayed on reconnect"
        );

        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let mut events = Vec::new();
        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::CreateTerminalResult {
                request_id,
                identity: Some(identity.clone()),
                location: location.clone(),
                error: None,
            },
            &mut events,
        );
        assert!(
            !events.iter().any(|event| matches!(
                event,
                ExecutionHostEvent::TerminalReady { terminal_id: id, .. } if id == &terminal_id
            )),
            "late ACK after cancel must not surface TerminalReady"
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                ExecutionHostEvent::TerminationPending {
                    terminal_id: id,
                    identity: pending_identity,
                    ..
                } if id == &terminal_id && pending_identity == &identity
            )) || manager.terminals.get(&terminal_id).is_some_and(|record| {
                record.termination_pending() && record.identity() == Some(&identity)
            }),
            "late ACK after cancel must drive returned identity to termination-pending"
        );
        runtime.shutdown();
    }

    #[test]
    fn reconnect_replays_identical_termination_with_the_same_request_id() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let messages = manager.connect_test_host(host_id.clone());
        let terminal_id = TerminalId::alloc();
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        manager.restore_termination_pending(terminal_id, location, identity);
        manager.flush_terminal_terminations(&mut Vec::new());
        let original = {
            let Ok(mut messages) = messages.lock() else {
                panic!("test worker message lock is poisoned");
            };
            let original = messages[0].clone();
            messages.clear();
            original
        };

        manager.replay_pending_terminations(&host_id, &mut Vec::new());

        let Ok(replayed) = messages.lock() else {
            panic!("test worker message lock is poisoned");
        };
        assert_eq!(replayed.as_slice(), &[original]);
    }

    #[test]
    fn local_operation_is_not_sent_to_a_remote_worker() {
        let mut manager = manager();
        let remote_host = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(remote_host);
        let location = ResourceLocation::local("/srv/same-path").unwrap();

        let error = manager.request_git_status(location).unwrap_err();

        assert!(matches!(error, HostOperationError::InvalidLocation(_)));
        assert!(messages
            .lock()
            .map(|messages| messages.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn identical_request_ids_and_paths_on_two_hosts_do_not_collide() {
        let mut manager = manager();
        let host_a = ExecutionHostId::new("ssh:a:1").unwrap();
        let host_b = ExecutionHostId::new("ssh:b:1").unwrap();
        let location_a = ResourceLocation::new(host_a.clone(), HostPath::new("/srv/work").unwrap());
        let location_b = ResourceLocation::new(host_b.clone(), HostPath::new("/srv/work").unwrap());
        let request_id = RequestId::new(1);
        let now = Instant::now();
        manager
            .git_observations
            .track_started(location_a.clone(), host_a.clone(), request_id, now);
        manager
            .git_observations
            .track_started(location_b.clone(), host_b.clone(), request_id, now);

        for (host_id, location, branch) in [
            (host_a, location_a.clone(), "host-a"),
            (host_b, location_b.clone(), "host-b"),
        ] {
            manager.route_worker_message(
                host_id,
                WorkerMessage::GitStatusResult {
                    request_id,
                    location,
                    status: Some(GitStatusSnapshot {
                        branch: Some(branch.into()),
                        dirty: false,
                        upstream: None,
                        ahead: 0,
                        behind: 0,
                    }),
                    error: None,
                },
                &mut Vec::new(),
            );
        }

        assert_eq!(
            manager
                .git_status(&location_a)
                .and_then(HostObservation::current)
                .and_then(|status| status.branch.as_deref()),
            Some("host-a")
        );
        assert_eq!(
            manager
                .git_status(&location_b)
                .and_then(HostObservation::current)
                .and_then(|status| status.branch.as_deref()),
            Some("host-b")
        );
    }

    #[test]
    fn mismatched_response_location_cannot_poison_another_host_cache() {
        let mut manager = manager();
        let host_a = ExecutionHostId::new("ssh:a:1").unwrap();
        let host_b = ExecutionHostId::new("ssh:b:1").unwrap();
        let requested = ResourceLocation::new(host_a.clone(), HostPath::new("/srv/work").unwrap());
        let injected = ResourceLocation::new(host_b, HostPath::new("/srv/work").unwrap());
        manager.connect_test_host(host_a.clone());
        let request_id = manager
            .request_git_status(requested.clone())
            .expect("remote Git request should be routed");

        manager.route_worker_message(
            host_a,
            WorkerMessage::GitStatusResult {
                request_id,
                location: injected.clone(),
                status: Some(GitStatusSnapshot {
                    branch: Some("injected".into()),
                    dirty: false,
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                }),
                error: None,
            },
            &mut Vec::new(),
        );

        assert!(matches!(
            manager.git_status(&requested),
            Some(HostObservation::Failed { .. })
        ));
        assert!(manager.git_status(&injected).is_none());
    }

    #[test]
    fn disconnected_remote_observation_is_not_current() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        manager.connect_test_host(host_id.clone());
        let request_id = manager
            .request_git_status(location.clone())
            .expect("remote Git request should be routed");
        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::GitStatusResult {
                request_id,
                location: location.clone(),
                status: Some(GitStatusSnapshot {
                    branch: Some("main".into()),
                    dirty: false,
                    upstream: Some("origin/main".into()),
                    ahead: 0,
                    behind: 0,
                }),
                error: None,
            },
            &mut Vec::new(),
        );
        assert!(manager
            .git_status(&location)
            .and_then(HostObservation::current)
            .is_some());

        manager.mark_host_observations_stale(&host_id);

        assert!(manager
            .git_status(&location)
            .and_then(HostObservation::current)
            .is_none());
        assert!(matches!(
            manager.git_status(&location),
            Some(HostObservation::Stale { .. })
        ));
    }

    #[test]
    fn stale_older_git_response_is_rejected() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        manager.connect_test_host(host_id.clone());
        let first = manager
            .request_git_status(location.clone())
            .expect("first git request");
        // Complete first.
        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::GitStatusResult {
                request_id: first,
                location: location.clone(),
                status: Some(GitStatusSnapshot {
                    branch: Some("first".into()),
                    dirty: false,
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                }),
                error: None,
            },
            &mut Vec::new(),
        );
        let second = manager
            .request_git_status(location.clone())
            .expect("second git request");
        assert_ne!(first, second);
        // Stale older response must not overwrite the newer pending/fresh slot.
        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::GitStatusResult {
                request_id: first,
                location: location.clone(),
                status: Some(GitStatusSnapshot {
                    branch: Some("stale".into()),
                    dirty: false,
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                }),
                error: None,
            },
            &mut Vec::new(),
        );
        assert!(matches!(
            manager.git_status(&location),
            Some(HostObservation::Pending { request_id, .. }) if *request_id == second
        ));
        manager.route_worker_message(
            host_id,
            WorkerMessage::GitStatusResult {
                request_id: second,
                location: location.clone(),
                status: Some(GitStatusSnapshot {
                    branch: Some("second".into()),
                    dirty: false,
                    upstream: None,
                    ahead: 0,
                    behind: 0,
                }),
                error: None,
            },
            &mut Vec::new(),
        );
        assert_eq!(
            manager
                .git_status(&location)
                .and_then(HostObservation::current)
                .and_then(|status| status.branch.as_deref()),
            Some("second")
        );
    }

    #[test]
    fn coalesced_git_refresh_reuses_inflight_request_id() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let messages = manager.connect_test_host(host_id);
        let first = manager
            .request_git_status(location.clone())
            .expect("first git request");
        let second = manager
            .request_git_status(location)
            .expect("coalesced git request");
        assert_eq!(first, second);
        let sent = messages
            .lock()
            .map(|messages| {
                messages
                    .iter()
                    .filter(|message| matches!(message, CoordinatorMessage::GitStatus { .. }))
                    .count()
            })
            .unwrap_or(0);
        assert_eq!(sent, 1);
    }

    #[test]
    fn observation_timeout_marks_typed_failed_state() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        manager.connect_test_host(host_id);
        let request_id = manager
            .request_git_status(location.clone())
            .expect("git request");
        match manager.git_observations.get_mut(&location) {
            Some(HostObservation::Pending { requested_at, .. }) => {
                *requested_at = Instant::now()
                    - OBSERVATION_REQUEST_TIMEOUT
                    - std::time::Duration::from_secs(1);
            }
            _ => panic!("expected pending observation"),
        }
        manager.expire_pending_observations(Instant::now());
        match manager.git_status(&location) {
            Some(HostObservation::Failed {
                error,
                request_id: failed_id,
                ..
            }) => {
                assert_eq!(error.code, WorkerErrorCode::TimedOut);
                assert_eq!(*failed_id, request_id);
            }
            other => panic!("expected Failed timeout, got {other:?}"),
        }
        assert_eq!(manager.git_observations.pending_len(), 0);
    }

    #[test]
    fn stage_file_rejects_mismatched_location_and_unsolicited_results() {
        let mut manager = manager();
        let host_a = ExecutionHostId::new("ssh:a:1").unwrap();
        let host_b = ExecutionHostId::new("ssh:b:1").unwrap();
        let location_a = ResourceLocation::new(host_a.clone(), HostPath::new("/tmp").unwrap());
        let location_b = ResourceLocation::new(host_b.clone(), HostPath::new("/tmp").unwrap());
        manager.connect_test_host(host_a.clone());
        manager.connect_test_host(host_b.clone());
        let request_id = manager
            .request_stage_file(location_a.clone(), "png".into(), vec![1, 2, 3], 30)
            .expect("stage request");
        let mut events = Vec::new();
        // Unsolicited for host_b identical request id.
        manager.route_worker_message(
            host_b,
            WorkerMessage::StageFileResult {
                request_id,
                location: location_b,
                path: Some(HostPath::new("/tmp/evil.png").unwrap()),
                error: None,
            },
            &mut events,
        );
        assert!(events.is_empty());
        // Mismatched location on the correct host.
        manager.route_worker_message(
            host_a.clone(),
            WorkerMessage::StageFileResult {
                request_id,
                location: ResourceLocation::new(host_a.clone(), HostPath::new("/other").unwrap()),
                path: Some(HostPath::new("/other/x.png").unwrap()),
                error: None,
            },
            &mut events,
        );
        assert!(events.is_empty());
        // Matching completion.
        manager.route_worker_message(
            host_a.clone(),
            WorkerMessage::StageFileResult {
                request_id,
                location: location_a.clone(),
                path: Some(HostPath::new("/tmp/ok.png").unwrap()),
                error: None,
            },
            &mut events,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            ExecutionHostEvent::FileStaged {
                host_id,
                request_id: completed_id,
                location,
                result,
            } => {
                assert_eq!(host_id, &host_a);
                assert_eq!(*completed_id, request_id);
                assert_eq!(location, &location_a);
                assert_eq!(
                    result.as_ref().unwrap().to_string(),
                    HostPath::new("/tmp/ok.png").unwrap().to_string()
                );
            }
            other => panic!("unexpected event {other:?}"),
        }
        assert!(manager.stage_requests.pending_is_empty());
    }

    #[test]
    fn identical_stage_request_ids_on_two_hosts_do_not_collide() {
        let mut manager = manager();
        let host_a = ExecutionHostId::new("ssh:a:1").unwrap();
        let host_b = ExecutionHostId::new("ssh:b:1").unwrap();
        let location_a = ResourceLocation::new(host_a.clone(), HostPath::new("/tmp").unwrap());
        let location_b = ResourceLocation::new(host_b.clone(), HostPath::new("/tmp").unwrap());
        let request_id = RequestId::new(9);
        manager
            .stage_requests
            .insert_for_test(host_a.clone(), request_id, location_a.clone());
        manager
            .stage_requests
            .insert_for_test(host_b.clone(), request_id, location_b.clone());
        let mut events = Vec::new();
        for (host_id, location, path) in [
            (host_a.clone(), location_a.clone(), "/tmp/a.png"),
            (host_b.clone(), location_b.clone(), "/tmp/b.png"),
        ] {
            manager.route_worker_message(
                host_id,
                WorkerMessage::StageFileResult {
                    request_id,
                    location,
                    path: Some(HostPath::new(path).unwrap()),
                    error: None,
                },
                &mut events,
            );
        }
        assert_eq!(events.len(), 2);
        let paths: Vec<_> = events
            .iter()
            .map(|event| match event {
                ExecutionHostEvent::FileStaged {
                    host_id, result, ..
                } => (host_id.clone(), result.as_ref().unwrap().to_string()),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert!(paths.contains(&(host_a, "/tmp/a.png".into())));
        assert!(paths.contains(&(host_b, "/tmp/b.png".into())));
    }

    #[tokio::test]
    async fn runtime_exit_status_is_forwarded_and_record_removed() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();
        // Make op_seq ready as if adopt completed.
        if let Some(record) = manager.terminals.get_mut(&terminal_id) {
            record.set_op_seq_ready(true);
            record.set_adopt_pending(false);
        }
        let mut events = Vec::new();
        manager.route_worker_message(
            host_id,
            WorkerMessage::RuntimeExit {
                identity,
                location,
                status: RuntimeExitStatus::Code(7),
            },
            &mut events,
        );
        assert!(!manager.terminals.contains(&terminal_id));
        assert!(events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::TerminalExited {
                terminal_id: exited_id,
                status: RuntimeExitStatus::Code(7),
            } if exited_id == &terminal_id
        )));
        runtime.shutdown();
    }

    #[tokio::test]
    async fn offline_input_is_journaled_and_replayed_after_reconnect() {
        let mut manager = manager();
        let profile = SshConnectionProfile::new(
            "offline-box",
            "Offline box",
            "offline.example",
            Some(HostPath::new("/srv/work").unwrap()),
        )
        .unwrap();
        let host_id = profile.execution_host_id();
        manager.sync_profiles(&[profile]);
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location,
                identity,
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();
        if let Some(record) = manager.terminals.get_mut(&terminal_id) {
            record.set_op_seq_ready(true);
            record.set_adopt_pending(false);
            // Keep the record bound to the synced offline host.
            record.set_host_id(host_id.clone());
        }
        assert!(
            manager.connections.contains_host(&host_id),
            "offline host binding must exist so request ids can be allocated"
        );
        // Host exists but has no live worker sender: input must still be journaled
        // using the host's persistent request-id counter.
        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"offline"))
            .unwrap();
        let mut events = Vec::new();
        manager.flush_terminal_controls(&mut events);
        let journaled_request_id = {
            let record = manager
                .terminals
                .get(&terminal_id)
                .expect("terminal retained");
            assert_eq!(
                record.op_journal().len(),
                1,
                "offline input should be journaled, host_id={host_id:?}"
            );
            assert!(matches!(
                record.op_journal()[0].op().clone(),
                JournaledRuntimeOp::Input { ref data } if data.as_slice() == b"offline"
            ));
            record.op_journal()[0].request_id()
        };
        // Connect and replay the retained journal exactly once with the same request id.
        let messages = manager.connect_test_host(host_id.clone());
        manager.replay_pending_runtime_ops(&host_id, &mut events);
        let locked = messages.lock().expect("test worker message lock");
        assert_eq!(locked.len(), 1);
        assert!(matches!(
            &locked[0],
            CoordinatorMessage::Input {
                request_id,
                data,
                ..
            } if *request_id == journaled_request_id && data.as_slice() == b"offline"
        ));
        runtime.shutdown();
    }

    #[tokio::test]
    async fn input_journal_replays_unacked_ops_exactly_once_after_reconnect() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location,
                identity,
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();
        if let Some(record) = manager.terminals.get_mut(&terminal_id) {
            record.set_op_seq_ready(true);
            record.set_adopt_pending(false);
        }
        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"hello"))
            .unwrap();
        let mut events = Vec::new();
        manager.flush_terminal_controls(&mut events);
        let first = {
            let Ok(mut locked) = messages.lock() else {
                panic!("poisoned");
            };
            assert_eq!(locked.len(), 1);
            let msg = locked[0].clone();
            locked.clear();
            msg
        };
        manager.replay_pending_runtime_ops(&host_id, &mut events);
        {
            let Ok(locked) = messages.lock() else {
                panic!("poisoned");
            };
            assert_eq!(locked.as_slice(), std::slice::from_ref(&first));
        }
        let request_id = match first {
            CoordinatorMessage::Input { request_id, .. } => request_id,
            other => panic!("unexpected {other:?}"),
        };
        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::RequestAck {
                request_id,
                error: None,
            },
            &mut events,
        );
        {
            let Ok(mut locked) = messages.lock() else {
                panic!("poisoned");
            };
            locked.clear();
        }
        manager.replay_pending_runtime_ops(&host_id, &mut events);
        assert!(messages.lock().map(|m| m.is_empty()).unwrap_or(false));
        runtime.shutdown();
    }

    #[test]
    fn termination_ack_unknown_runtime_clears_tombstone() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        manager.restore_termination_pending(terminal_id.clone(), location, identity);
        manager.flush_terminal_terminations(&mut Vec::new());
        let request_id = manager
            .terminals
            .pending_termination_request_id(&terminal_id)
            .expect("termination request");
        let mut events = Vec::new();
        manager.route_worker_message(
            host_id,
            WorkerMessage::RequestAck {
                request_id,
                error: Some(WorkerError::new(
                    WorkerErrorCode::UnknownRuntime,
                    "runtime does not exist",
                )),
            },
            &mut events,
        );
        assert!(!manager.terminals.contains(&terminal_id));
        assert!(events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::TerminationFinished {
                terminal_id: finished,
            } if finished == &terminal_id
        )));
    }

    #[tokio::test]
    async fn negative_attach_unknown_runtime_is_terminal_lost() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();
        let mut events = Vec::new();
        manager.route_worker_message(
            host_id,
            WorkerMessage::AttachTerminalResult {
                request_id: RequestId::new(9),
                identity,
                location,
                resume: AttachResume::Checkpoint,
                error: Some(WorkerError::new(
                    WorkerErrorCode::UnknownRuntime,
                    "runtime does not exist",
                )),
            },
            &mut events,
        );
        assert!(!manager.terminals.contains(&terminal_id));
        assert!(events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::TerminalFailed {
                terminal_id: failed_id,
                message,
            } if failed_id == &terminal_id && message.contains("remote terminal lost")
        )));
        runtime.shutdown();
    }

    #[tokio::test]
    async fn process_observation_result_preserves_session_and_foreground_fields() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                identity.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();

        let request_id = manager.request_process_observation(&terminal_id).unwrap();
        let observation = ProcessObservation {
            pid: 42,
            ppid: None,
            command: Some("zsh".into()),
            cwd: Some(HostPath::new("/srv/work").unwrap()),
            foreground_process_group_id: None,
            foreground_processes: Vec::new(),
            session_processes: vec![
                super::super::protocol::ObservedProcess {
                    pid: 42,
                    name: "zsh".into(),
                    argv0: None,
                    argv: None,
                    cmdline: None,
                    cwd: Some(HostPath::new("/srv/work").unwrap()),
                },
                super::super::protocol::ObservedProcess {
                    pid: 99,
                    name: "node".into(),
                    argv0: Some("node".into()),
                    argv: None,
                    cmdline: Some("node server.js".into()),
                    cwd: Some(HostPath::new("/srv/work").unwrap()),
                },
            ],
        };
        manager.route_worker_message(
            host_id,
            WorkerMessage::ProcessObservationResult {
                request_id,
                identity,
                location,
                process: Some(observation.clone()),
                error: None,
            },
            &mut Vec::new(),
        );

        let stored = manager
            .process_observation(&terminal_id)
            .and_then(HostObservation::current)
            .cloned()
            .expect("fresh process observation");
        assert_eq!(stored.pid, 42);
        assert!(stored.foreground_processes.is_empty());
        assert_eq!(
            stored
                .session_processes
                .iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>(),
            vec![42, 99]
        );
        runtime.shutdown();
    }

    #[tokio::test]
    async fn project_command_discovery_is_routed_and_stored_by_location() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/app").unwrap());

        let request_id = manager
            .request_project_commands(location.clone())
            .expect("discovery request should send");
        let routed = messages
            .lock()
            .map(|messages| {
                messages.iter().any(|message| {
                    matches!(
                        message,
                        CoordinatorMessage::DiscoverProjectCommands {
                            request_id: sent_id,
                            location: sent_location,
                        } if *sent_id == request_id && sent_location == &location
                    )
                })
            })
            .unwrap_or(false);
        assert!(routed);

        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::ProjectCommandsResult {
                request_id,
                location: location.clone(),
                commands: vec![super::super::protocol::ProjectCommandSnapshot {
                    location: location.clone(),
                    source: super::super::protocol::ProjectCommandSource::PackageJson,
                    name: "dev".into(),
                    command: "npm run dev".into(),
                    confidence: super::super::protocol::ProjectCommandConfidence::Explicit,
                }],
                error: None,
            },
            &mut Vec::new(),
        );

        let commands = manager
            .project_commands(&location)
            .and_then(HostObservation::current)
            .cloned()
            .expect("fresh project commands");
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "dev");
        assert_eq!(commands[0].location, location);
    }

    #[tokio::test]
    async fn oversized_paste_is_chunked_below_frame_and_capacity_overflow_fails_terminal() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location,
                identity,
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();
        if let Some(record) = manager.terminals.get_mut(&terminal_id) {
            record.set_op_seq_ready(true);
            record.set_adopt_pending(false);
        }
        let oversized = vec![b'a'; MAX_RUNTIME_INPUT_CHUNK_BYTES + 32];
        runtime
            .try_send_bytes(bytes::Bytes::from(oversized.clone()))
            .unwrap();
        let mut events = Vec::new();
        manager.flush_terminal_controls(&mut events);
        {
            let locked = messages.lock().expect("lock");
            assert!(
                locked.len() >= 2,
                "oversized paste must be chunked, got {}",
                locked.len()
            );
            for message in locked.iter() {
                match message {
                    CoordinatorMessage::Input { data, .. } => {
                        assert!(data.len() <= MAX_RUNTIME_INPUT_CHUNK_BYTES);
                    }
                    other => panic!("unexpected {other:?}"),
                }
            }
            let total: usize = locked
                .iter()
                .map(|message| match message {
                    CoordinatorMessage::Input { data, .. } => data.len(),
                    _ => 0,
                })
                .sum();
            assert_eq!(total, oversized.len());
        }

        // Force capacity overflow with many retained ops.
        if let Some(record) = manager.terminals.get_mut(&terminal_id) {
            record.clear_op_journal();
            for i in 0..MAX_RUNTIME_OP_JOURNAL_OPS {
                record.op_journal_mut().push(PendingRuntimeOp::new(
                    RequestId::new(1000 + i as u64),
                    RuntimeOpSeq::new(i as u64 + 1),
                    JournaledRuntimeOp::Input { data: vec![b'x'] },
                ));
            }
            record.set_next_op_seq((MAX_RUNTIME_OP_JOURNAL_OPS as u64) + 1);
        }
        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"one-more"))
            .unwrap();
        events.clear();
        manager.flush_terminal_controls(&mut events);
        assert!(events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::TerminalFailed {
                terminal_id: failed,
                message,
            } if failed == &terminal_id && message.contains("journal unavailable")
        )));
        runtime.shutdown();
    }

    #[tokio::test]
    async fn negative_ack_retires_head_so_following_input_is_not_bricked() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let identity = RuntimeIdentity::new(
            super::super::protocol::HostBindingGeneration::new(1),
            super::super::protocol::WorkerInstanceId::new("worker-a").unwrap(),
            super::super::protocol::WorkerRuntimeId::new("runtime-a").unwrap(),
            super::super::protocol::RuntimeIncarnation::new(1),
        );
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .adopt_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location,
                identity,
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
            )
            .unwrap();
        if let Some(record) = manager.terminals.get_mut(&terminal_id) {
            record.set_op_seq_ready(true);
            record.set_adopt_pending(false);
        }
        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"first"))
            .unwrap();
        runtime
            .try_send_bytes(bytes::Bytes::from_static(b"second"))
            .unwrap();
        let mut events = Vec::new();
        manager.flush_terminal_controls(&mut events);
        let first_request = {
            let locked = messages.lock().expect("lock");
            assert!(locked.len() >= 2);
            match &locked[0] {
                CoordinatorMessage::Input { request_id, .. } => *request_id,
                other => panic!("unexpected {other:?}"),
            }
        };
        manager.route_worker_message(
            host_id.clone(),
            WorkerMessage::RequestAck {
                request_id: first_request,
                error: Some(WorkerError::new(
                    WorkerErrorCode::Conflict,
                    "runtime operation sequence has a gap",
                )),
            },
            &mut events,
        );
        {
            let record = manager.terminals.get(&terminal_id).expect("retained");
            // Failed head retired; trailing op may remain or be cleared with head drain.
            assert!(
                record
                    .op_journal()
                    .iter()
                    .all(|pending| pending.request_id() != first_request),
                "negative ACK must retire the failed head"
            );
        }
        assert!(events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::TerminalFailed { terminal_id: failed, .. }
                if failed == &terminal_id
        )));
        // Head is retired so a later successful path can journal fresh ops.
        if let Some(record) = manager.terminals.get(&terminal_id) {
            assert!(record
                .op_journal()
                .iter()
                .all(|pending| pending.request_id() != first_request));
        }
        runtime.shutdown();
    }

    #[test]
    fn disconnect_emits_file_staged_error_for_pending_stage_requests() {
        let mut manager = manager();
        let host_a = ExecutionHostId::new("ssh:a:1").unwrap();
        let host_b = ExecutionHostId::new("ssh:b:1").unwrap();
        let location_a =
            ResourceLocation::new(host_a.clone(), HostPath::new("/tmp/a.png").unwrap());
        let location_b =
            ResourceLocation::new(host_b.clone(), HostPath::new("/tmp/b.png").unwrap());
        let request_id = RequestId::new(44);
        manager
            .stage_requests
            .insert_for_test(host_a.clone(), request_id, location_a.clone());
        manager
            .stage_requests
            .insert_for_test(host_b.clone(), request_id, location_b.clone());
        manager.mark_host_observations_stale(&host_a);
        assert!(manager.lifecycle_events.iter().any(|event| matches!(
            event,
            ExecutionHostEvent::FileStaged {
                host_id,
                request_id: staged_id,
                location,
                result: Err(error),
            } if host_id == &host_a
                && *staged_id == request_id
                && location == &location_a
                && error.code == WorkerErrorCode::Gone
        )));
        // Host B pending stage remains until its own disconnect.
        assert!(manager.stage_requests.has_pending(&host_b, request_id));
        assert!(!manager.stage_requests.has_pending(&host_a, request_id));
    }

    #[test]
    fn catalog_transport_allocates_and_sends_identically_for_live_style_ops() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        manager
            .connections
            .set_next_test_request_id_for_test(u64::MAX - 1);

        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let first = manager
            .request_git_status(location.clone())
            .expect("first git request");
        let second = manager
            .request_stage_file(location.clone(), "txt".into(), b"x".to_vec(), 10)
            .expect("stage request");

        // Shared saturating overflow: first id is MAX-1, second saturates at MAX.
        assert_eq!(first, RequestId::new(u64::MAX - 1));
        assert_eq!(second, RequestId::new(u64::MAX));

        // A third allocation stays at the ceiling — same policy as offline SSH hosts.
        let third = manager
            .connections
            .allocate_request_id(&host_id)
            .expect("test host still allocates at ceiling");
        assert_eq!(third, RequestId::new(u64::MAX));

        let locked = messages.lock().expect("message lock");
        assert_eq!(locked.len(), 2);
        assert!(matches!(
            &locked[0],
            CoordinatorMessage::GitStatus {
                request_id,
                location: sent_location,
            } if *request_id == first && sent_location == &location
        ));
        assert!(matches!(
            &locked[1],
            CoordinatorMessage::StageFile {
                request_id,
                location: sent_location,
                ..
            } if *request_id == second && sent_location == &location
        ));
    }

    #[tokio::test]
    async fn create_terminal_uses_catalog_transport_not_parallel_test_path() {
        let mut manager = manager();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = manager.connect_test_host(host_id.clone());
        let location = ResourceLocation::new(host_id.clone(), HostPath::new("/srv/work").unwrap());
        let terminal_id = TerminalId::alloc();
        let (events_tx, _events_rx) = tokio::sync::mpsc::channel(4);
        let runtime = manager
            .create_terminal(
                terminal_id.clone(),
                PaneId::alloc(),
                location.clone(),
                24,
                80,
                1024,
                crate::terminal_theme::TerminalTheme::default(),
                events_tx,
                None,
                Vec::new(),
            )
            .expect("create through catalog transport");
        let request_id = manager
            .terminals
            .pending_create_request_id(&host_id, &terminal_id)
            .expect("pending create");
        assert_eq!(request_id, RequestId::new(1));
        let locked = messages.lock().expect("message lock");
        assert!(matches!(
            locked.as_slice(),
            [CoordinatorMessage::CreateTerminal {
                request_id: sent_id,
                location: sent_location,
                ..
            }] if *sent_id == request_id && sent_location == &location
        ));
        runtime.shutdown();
    }
}
