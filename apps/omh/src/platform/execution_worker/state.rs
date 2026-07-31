//! Worker daemon state: runtime registry, replays, staging, host jobs.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
#[cfg(unix)]
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
#[cfg(test)]
use std::time::Instant;

use tokio::sync::{mpsc, Notify};

use crate::events::AppEvent;
use crate::execution_host::lifecycle::{lifecycle_decision_input, worker_instance_id_string};
use crate::execution_host::protocol::{
    CommandSpec, RequestId, RuntimeIdentity, RuntimeIncarnation, RuntimeOpSeq, TerminalSize,
    TerminateMode, WorkerCapability, WorkerError, WorkerErrorCode, WorkerRuntimeId,
    PROTOCOL_VERSION,
};
use crate::execution_host::staging::StagedFileStore;
use crate::execution_host::{HostPath, ResourceLocation};
use crate::layout::PaneId;
use crate::pane::{PaneLaunchEnv, PaneShellConfig};
use crate::terminal::{TerminalId, TerminalRuntime};
use crate::terminal_theme::TerminalTheme;

use super::binding::DaemonBinding;
use super::event::{RuntimeLocalId, WorkerEvent};
#[cfg(unix)]
use super::hook_ingress::{QueuedHookReport, WorkerHookIngress};
#[cfg(unix)]
use super::host_job::HostJobResult;
use super::output::OutputLog;
#[cfg(unix)]
use super::state_tables::{HostJobTable, RuntimeTable};
use super::util::{
    worker_error, DEFAULT_WORKER_SCROLLBACK_BYTES, MAX_CREATE_REPLAYS, MAX_TERMINATION_TOMBSTONES,
    WORKER_APP_VERSION,
};

// Re-export public-to-sibling types from the private owner module.
#[cfg(unix)]
pub(super) use super::state_tables::{
    CreateKind, CreateRequest, HostJobGeneration, HostJobKind, HostJobSnapshot, HostJobTimeout,
    IntegrationJobTurn, RuntimeRecord,
};

#[cfg(unix)]
#[derive(Clone)]
struct CreateReplay {
    request: CreateRequest,
    result: Result<(RuntimeIdentity, ResourceLocation), WorkerError>,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpDisposition {
    Apply,
    Replay,
}

/// Translates PTY-engine `AppEvent`s (still required by TerminalRuntime) into
/// worker-native [`WorkerEvent`]s keyed by [`RuntimeLocalId`].
///
/// The adapter edge is the only place AppEvent is mentioned; worker core and
/// protocol code only see [`WorkerEvent`].
#[cfg(unix)]
struct RuntimeEventBridge;

#[cfg(unix)]
impl RuntimeEventBridge {
    fn spawn(
        local_id: RuntimeLocalId,
        worker_tx: mpsc::Sender<WorkerEvent>,
    ) -> mpsc::Sender<AppEvent> {
        let (app_tx, mut app_rx) = mpsc::channel::<AppEvent>(64);
        // Daemon connection loops are synchronous; bridge with a std thread and
        // tokio mpsc blocking ops so we do not require a runtime at spawn time.
        std::thread::spawn(move || {
            while let Some(event) = app_rx.blocking_recv() {
                let mapped = match event {
                    AppEvent::StateChanged {
                        agent,
                        state,
                        visible_blocker,
                        visible_idle,
                        visible_working,
                        process_exited,
                        ..
                    } => Some(WorkerEvent::StateChanged {
                        local_id,
                        agent,
                        state,
                        visible_blocker,
                        visible_idle,
                        visible_working,
                        process_exited,
                    }),
                    AppEvent::PaneDied {
                        exit_code,
                        exit_signal,
                        ..
                    } => Some(WorkerEvent::RuntimeExit {
                        local_id,
                        exit_code,
                        exit_signal,
                    }),
                    _ => None,
                };
                if let Some(event) = mapped {
                    if worker_tx.blocking_send(event).is_err() {
                        break;
                    }
                }
            }
        });
        app_tx
    }
}

#[cfg(unix)]
pub(super) struct WorkerState {
    binding: DaemonBinding,
    /// App version advertised by this daemon process (frozen at boot).
    app_version: String,
    /// SHA-256 identity of the executable that started this daemon.
    artifact_digest: [u8; 32],
    /// Worker protocol advertised by this daemon process (frozen at boot).
    worker_protocol: u32,
    /// Once set, the daemon refuses new work and exits after draining.
    draining: bool,
    runtimes: RuntimeTable,
    create_replays: HashMap<RequestId, CreateReplay>,
    create_replay_order: VecDeque<RequestId>,
    termination_tombstones: HashMap<RuntimeIdentity, ResourceLocation>,
    termination_order: VecDeque<RuntimeIdentity>,
    staging: StagedFileStore,
    /// Host observation/create-preflight jobs owned across bridge connections.
    host_jobs: HostJobTable,
    host_job_tx: std_mpsc::Sender<HostJobResult>,
    host_job_rx: std_mpsc::Receiver<HostJobResult>,
    hook_ingress: WorkerHookIngress,
    /// Worker-native runtime events (no AppEvent in the worker core).
    events: mpsc::Sender<WorkerEvent>,
    event_rx: mpsc::Receiver<WorkerEvent>,
    render_notify: Arc<Notify>,
    render_dirty: Arc<AtomicBool>,
}

#[cfg(unix)]
impl WorkerState {
    pub(super) fn new(binding: DaemonBinding) -> io::Result<Self> {
        let role_paths = binding.role_paths();
        role_paths.prepare()?;
        let (events, event_rx) = mpsc::channel(256);
        let (host_job_tx, host_job_rx) = std_mpsc::channel();
        let staging_root = role_paths.artifact_dir().join("staged-files");
        let hook_ingress = WorkerHookIngress::start(role_paths.hook_socket_path())?;
        Ok(Self {
            binding,
            app_version: WORKER_APP_VERSION.to_string(),
            artifact_digest: super::artifact_digest()?,
            worker_protocol: PROTOCOL_VERSION,
            draining: false,
            runtimes: RuntimeTable::new(),
            create_replays: HashMap::new(),
            create_replay_order: VecDeque::new(),
            termination_tombstones: HashMap::new(),
            termination_order: VecDeque::new(),
            staging: StagedFileStore::new(staging_root)?,
            host_jobs: HostJobTable::new(),
            host_job_tx,
            host_job_rx,
            hook_ingress,
            events,
            event_rx,
            render_notify: Arc::new(Notify::new()),
            render_dirty: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn binding(&self) -> &DaemonBinding {
        &self.binding
    }

    pub(super) fn is_draining(&self) -> bool {
        self.draining
    }

    pub(super) fn begin_draining(&mut self) {
        self.draining = true;
    }

    pub(super) fn staging_mut(&mut self) -> &mut StagedFileStore {
        &mut self.staging
    }

    pub(super) fn owned_runtime_count(&self) -> u64 {
        self.runtimes.owned_count()
    }

    #[cfg(test)]
    pub(super) fn runtime_record_count(&self) -> usize {
        self.runtimes.record_count()
    }

    #[cfg(test)]
    pub(super) fn has_runtime_records(&self) -> bool {
        self.runtimes.record_count() > 0
    }

    #[cfg(test)]
    pub(super) fn contains_runtime(&self, runtime_id: &WorkerRuntimeId) -> bool {
        self.runtimes.contains_record(runtime_id)
    }

    pub(super) fn runtime_record(&self, runtime_id: &WorkerRuntimeId) -> Option<&RuntimeRecord> {
        self.runtimes.record(runtime_id)
    }

    pub(super) fn runtime_record_by_local_id(
        &self,
        local_id: RuntimeLocalId,
    ) -> Option<&RuntimeRecord> {
        self.runtimes.record_by_local_id(local_id)
    }

    pub(super) fn is_idle_for_shutdown(&self) -> bool {
        self.runtimes.is_empty() && self.live_host_job_count() == 0
    }

    pub(super) fn lifecycle_snapshot(
        &self,
    ) -> crate::execution_host::lifecycle::LifecycleDecisionInput {
        let owned = self.owned_runtime_count();
        lifecycle_decision_input(
            true,
            self.binding.binding_digest(),
            self.artifact_digest,
            self.worker_protocol,
            self.app_version.clone(),
            worker_instance_id_string(&self.binding.worker_instance_id),
            owned,
            owned > 0 || self.live_host_job_count() > 0 || self.draining,
            false,
        )
    }

    pub(super) fn capabilities(&self) -> Vec<WorkerCapability> {
        vec![
            WorkerCapability::Terminal,
            WorkerCapability::PathCompletion,
            WorkerCapability::ProcessObservation,
            WorkerCapability::Git,
            WorkerCapability::Worktree,
            WorkerCapability::Command,
            WorkerCapability::Agent,
            WorkerCapability::Ports,
            WorkerCapability::FileStaging,
            WorkerCapability::AgentIntegrations,
        ]
    }

    pub(super) fn resolve_location(
        &self,
        location: &ResourceLocation,
    ) -> Result<ResourceLocation, WorkerError> {
        if location.execution_host_id != self.binding.execution_host_id {
            return Err(worker_error(
                WorkerErrorCode::BindingMismatch,
                "resource location belongs to another execution host",
            ));
        }
        let path = location.path.as_path();
        let resolved = if path == Path::new("~") {
            remote_home()?
        } else if let Ok(suffix) = path.strip_prefix(Path::new("~")) {
            if suffix.as_os_str().is_empty() {
                remote_home()?
            } else {
                remote_home()?.join(suffix)
            }
        } else if path
            .components()
            .next()
            .is_some_and(|component| component.as_os_str().to_string_lossy().starts_with('~'))
        {
            return Err(worker_error(
                WorkerErrorCode::InvalidLocation,
                "named-user tilde expansion is not supported",
            ));
        } else {
            path.to_path_buf()
        };
        let path = HostPath::new(resolved)
            .map_err(|error| worker_error(WorkerErrorCode::InvalidLocation, error.to_string()))?;
        Ok(ResourceLocation::new(
            self.binding.execution_host_id.clone(),
            path,
        ))
    }

    pub(super) fn validate_location(
        &self,
        location: &ResourceLocation,
    ) -> Result<PathBuf, WorkerError> {
        Ok(self
            .resolve_location(location)?
            .path
            .as_path()
            .to_path_buf())
    }

    pub(super) fn validate_runtime<'a>(
        &'a self,
        identity: &RuntimeIdentity,
        location: &ResourceLocation,
    ) -> Result<&'a RuntimeRecord, WorkerError> {
        if identity.host_binding_generation != self.binding.host_binding_generation {
            return Err(worker_error(
                WorkerErrorCode::BindingMismatch,
                "runtime host-binding generation is stale",
            ));
        }
        if identity.worker_instance_id != self.binding.worker_instance_id {
            return Err(worker_error(
                WorkerErrorCode::IncarnationMismatch,
                "runtime belongs to another worker instance",
            ));
        }
        let record = self.runtimes.record(&identity.runtime_id).ok_or_else(|| {
            worker_error(WorkerErrorCode::UnknownRuntime, "runtime does not exist")
        })?;
        if record.identity != *identity {
            return Err(worker_error(
                WorkerErrorCode::IncarnationMismatch,
                "runtime incarnation does not match",
            ));
        }
        if record.location != self.resolve_location(location)? {
            return Err(worker_error(
                WorkerErrorCode::InvalidLocation,
                "runtime location does not match",
            ));
        }
        Ok(record)
    }

    pub(super) fn create_terminal(
        &mut self,
        location: ResourceLocation,
        size: TerminalSize,
        command: Option<CommandSpec>,
        env: Vec<(String, String)>,
        scrollback_limit_bytes: usize,
    ) -> Result<(RuntimeIdentity, ResourceLocation), WorkerError> {
        if self.draining {
            return Err(worker_error(
                WorkerErrorCode::Busy,
                "execution worker is draining for replacement",
            ));
        }
        let location = self.resolve_location(&location)?;
        let cwd = location.path.as_path().to_path_buf();
        let metadata = std::fs::metadata(&cwd)
            .map_err(|error| worker_error(WorkerErrorCode::InvalidLocation, error.to_string()))?;
        if !metadata.is_dir() {
            return Err(worker_error(
                WorkerErrorCode::InvalidLocation,
                "terminal launch location is not a directory",
            ));
        }
        let scrollback_limit_bytes = scrollback_limit_bytes.max(1);
        let runtime_number = self.runtimes.alloc_runtime_number();
        let runtime_id = WorkerRuntimeId::new(format!("runtime-{runtime_number}"))
            .map_err(|err| worker_error(WorkerErrorCode::Failed, err.to_string()))?;
        let identity = RuntimeIdentity::new(
            self.binding.host_binding_generation,
            self.binding.worker_instance_id.clone(),
            runtime_id.clone(),
            RuntimeIncarnation::new(runtime_number),
        );
        let terminal_id = TerminalId::alloc();
        let local_id = RuntimeLocalId::new(runtime_number);
        let output = OutputLog::new(scrollback_limit_bytes);
        let mut env = env;
        if let Some(command) = &command {
            command
                .validate()
                .map_err(|err| worker_error(WorkerErrorCode::Failed, err.to_string()))?;
            env.extend(command.env.clone());
        }
        let hook_token = self
            .hook_ingress
            .register(identity.clone())
            .map_err(|error| worker_error(WorkerErrorCode::Failed, error.to_string()))?;
        let launch_env = PaneLaunchEnv::from_extra(env)
            .with_worker_hook_endpoint(self.hook_ingress.socket_path(), hook_token)
            .with_output_observer(output.observer());
        // PaneId is still required by TerminalRuntime's PTY adapter; it is not
        // stored on worker records or exposed through worker events.
        let pane_id = PaneId::alloc();
        let app_events = RuntimeEventBridge::spawn(local_id, self.events.clone());
        let runtime_result = if let Some(command) = command {
            let mut argv = Vec::with_capacity(command.args.len() + 1);
            argv.push(command.program);
            argv.extend(command.args);
            TerminalRuntime::spawn_argv_command(
                pane_id,
                size.rows,
                size.cols,
                cwd,
                &argv,
                &launch_env,
                scrollback_limit_bytes,
                TerminalTheme::default(),
                app_events,
                self.render_notify.clone(),
                self.render_dirty.clone(),
            )
        } else {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
            TerminalRuntime::spawn(
                pane_id,
                size.rows,
                size.cols,
                cwd,
                scrollback_limit_bytes,
                TerminalTheme::default(),
                PaneShellConfig::new(&shell, crate::config::ShellModeConfig::default()),
                &launch_env,
                app_events,
                self.render_notify.clone(),
                self.render_dirty.clone(),
            )
        };
        let runtime = match runtime_result {
            Ok(runtime) => runtime,
            Err(error) => {
                self.hook_ingress.unregister(&identity);
                return Err(worker_error(WorkerErrorCode::Failed, error.to_string()));
            }
        };
        self.runtimes.insert_pair(
            runtime_id,
            RuntimeRecord {
                terminal_id,
                local_id,
                identity: identity.clone(),
                location: location.clone(),
                output,
                last_op_seq: 0,
            },
            runtime,
        );
        Ok((identity, location))
    }

    pub(super) fn create_once(
        &mut self,
        request_id: RequestId,
        request: CreateRequest,
    ) -> Result<(RuntimeIdentity, ResourceLocation), WorkerError> {
        if let Some(replay) = self.create_replays.get(&request_id) {
            return if replay.request == request {
                replay.result.clone()
            } else {
                Err(worker_error(
                    WorkerErrorCode::Conflict,
                    "create request id was already used for different parameters",
                ))
            };
        }
        let result = self.create_terminal(
            request.location.clone(),
            request.size.clone(),
            request.command.clone(),
            request.env.clone(),
            request.scrollback_limit_bytes,
        );
        self.create_replays.insert(
            request_id,
            CreateReplay {
                request,
                result: result.clone(),
            },
        );
        self.create_replay_order.push_back(request_id);
        while self.create_replay_order.len() > MAX_CREATE_REPLAYS {
            if let Some(expired) = self.create_replay_order.pop_front() {
                self.create_replays.remove(&expired);
            }
        }
        result
    }

    pub(super) fn op_disposition(
        record: &RuntimeRecord,
        op_seq: RuntimeOpSeq,
    ) -> Result<OpDisposition, WorkerError> {
        if op_seq.get() == 0 {
            return Err(worker_error(
                WorkerErrorCode::Conflict,
                "runtime operation sequence starts at one",
            ));
        }
        if op_seq.get() == record.last_op_seq {
            return Ok(OpDisposition::Replay);
        }
        if op_seq.get() == record.last_op_seq.saturating_add(1) {
            return Ok(OpDisposition::Apply);
        }
        if op_seq.get() < record.last_op_seq {
            return Err(worker_error(
                WorkerErrorCode::Conflict,
                "runtime operation sequence is stale",
            ));
        }
        Err(worker_error(
            WorkerErrorCode::Conflict,
            "runtime operation sequence has a gap",
        ))
    }

    /// Apply a sequenced runtime op against the live record+runtime pair.
    pub(super) fn apply_runtime_op(
        &mut self,
        identity: &RuntimeIdentity,
        location: &ResourceLocation,
        op_seq: RuntimeOpSeq,
        apply: impl FnOnce(&TerminalRuntime) -> Result<(), WorkerError>,
    ) -> Result<(), WorkerError> {
        self.validate_runtime(identity, location)?;
        let terminal_id = {
            let record = self
                .runtimes
                .record_mut(&identity.runtime_id)
                .ok_or_else(|| {
                    worker_error(WorkerErrorCode::Gone, "terminal runtime is no longer live")
                })?;
            if Self::op_disposition(record, op_seq)? == OpDisposition::Replay {
                return Ok(());
            }
            record.terminal_id.clone()
        };
        let runtime = self.runtimes.runtime(&terminal_id).ok_or_else(|| {
            worker_error(WorkerErrorCode::Gone, "terminal runtime is no longer live")
        })?;
        apply(runtime)?;
        let record = self
            .runtimes
            .record_mut(&identity.runtime_id)
            .ok_or_else(|| {
                worker_error(WorkerErrorCode::Gone, "terminal runtime is no longer live")
            })?;
        record.last_op_seq = op_seq.get();
        Ok(())
    }

    pub(super) fn child_pid_for(
        &self,
        identity: &RuntimeIdentity,
        location: &ResourceLocation,
    ) -> Result<u32, WorkerError> {
        let record = self.validate_runtime(identity, location)?;
        let runtime = self.runtimes.runtime(&record.terminal_id).ok_or_else(|| {
            worker_error(WorkerErrorCode::Gone, "terminal runtime is no longer live")
        })?;
        Ok(runtime.child_pid())
    }

    /// Terminate or detach a runtime, removing the paired record+runtime atomically.
    pub(super) fn terminate_runtime(
        &mut self,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        mode: TerminateMode,
    ) -> Result<(), WorkerError> {
        let resolved_location = self.resolve_location(&location)?;
        if let Some(tombstone_location) = self.termination_tombstones.get(&identity) {
            return if tombstone_location == &resolved_location {
                Ok(())
            } else {
                Err(worker_error(
                    WorkerErrorCode::InvalidLocation,
                    "terminated runtime location does not match",
                ))
            };
        }
        match self.validate_runtime(&identity, &resolved_location) {
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.code,
                    WorkerErrorCode::UnknownRuntime
                        | WorkerErrorCode::Gone
                        | WorkerErrorCode::IncarnationMismatch
                ) =>
            {
                // Fully fenced absence / obsolete incarnation converges as
                // successful termination so durable tombstones can clear.
                self.record_termination(identity, resolved_location);
                return Ok(());
            }
            Err(error) => return Err(error),
        }
        let Some((record, runtime)) = self.runtimes.remove_pair(&identity.runtime_id) else {
            return Err(worker_error(
                WorkerErrorCode::Gone,
                "terminal runtime is no longer live",
            ));
        };
        if let Some(runtime) = runtime {
            if mode == TerminateMode::Terminate {
                runtime.shutdown();
            } else {
                std::mem::forget(runtime);
            }
        }
        self.record_termination(identity, record.location);
        Ok(())
    }

    /// Remove a runtime that exited via PaneDied; shuts down the paired runtime.
    pub(super) fn take_exited_runtime(
        &mut self,
        local_id: RuntimeLocalId,
    ) -> Option<RuntimeRecord> {
        let (_runtime_id, record, runtime) = self.runtimes.take_by_local_id(local_id)?;
        if let Some(runtime) = runtime {
            // RuntimeExit is emitted only after the child wait completes, so shutdown
            // releases detector and PTY resources without racing the reaped child.
            runtime.shutdown();
        }
        Some(record)
    }

    pub(super) fn record_termination(
        &mut self,
        identity: RuntimeIdentity,
        location: ResourceLocation,
    ) {
        self.hook_ingress.unregister(&identity);
        self.termination_tombstones
            .insert(identity.clone(), location);
        self.termination_order.push_back(identity);
        while self.termination_order.len() > MAX_TERMINATION_TOMBSTONES {
            if let Some(expired) = self.termination_order.pop_front() {
                self.termination_tombstones.remove(&expired);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn has_termination_tombstone(&self, identity: &RuntimeIdentity) -> bool {
        self.termination_tombstones.contains_key(identity)
    }

    pub(super) fn try_recv_event(&mut self) -> Result<WorkerEvent, mpsc::error::TryRecvError> {
        self.event_rx.try_recv()
    }

    pub(super) fn next_hook_report(&self) -> Option<QueuedHookReport> {
        self.hook_ingress.next_report()
    }

    pub(super) fn confirm_hook_report(&self, delivered: &QueuedHookReport) {
        self.hook_ingress.confirm_report(delivered);
    }

    #[cfg(test)]
    pub(super) fn try_send_event(
        &self,
        event: WorkerEvent,
    ) -> Result<(), mpsc::error::TrySendError<WorkerEvent>> {
        self.events.try_send(event)
    }

    pub(super) fn live_host_job_count(&self) -> usize {
        self.host_jobs.live_count()
    }

    #[cfg(test)]
    pub(super) fn host_jobs_is_empty(&self) -> bool {
        self.host_jobs.is_empty()
    }

    #[cfg(test)]
    pub(super) fn host_job_contains(&self, request_id: &RequestId) -> bool {
        self.host_jobs.contains(request_id)
    }

    pub(super) fn host_job_snapshot(&self, request_id: &RequestId) -> Option<HostJobSnapshot> {
        self.host_jobs.snapshot(request_id)
    }

    pub(super) fn insert_host_job(
        &mut self,
        request_id: RequestId,
        kind: HostJobKind,
        location: ResourceLocation,
    ) -> Result<(Arc<AtomicBool>, Arc<AtomicBool>, HostJobGeneration), WorkerError> {
        self.host_jobs.insert(request_id, kind, location)
    }

    pub(super) fn reserve_integration_job_turn(&mut self) -> IntegrationJobTurn {
        self.host_jobs.reserve_integration_turn()
    }

    pub(super) fn host_job_sender(&self) -> std_mpsc::Sender<HostJobResult> {
        self.host_job_tx.clone()
    }

    pub(super) fn try_recv_host_job_result(&self) -> Result<HostJobResult, std_mpsc::TryRecvError> {
        self.host_job_rx.try_recv()
    }

    pub(super) fn cancel_host_jobs_for_disconnect(&mut self) {
        self.host_jobs.cancel_for_disconnect();
    }

    pub(super) fn timed_out_host_jobs(&self, timeout: std::time::Duration) -> Vec<RequestId> {
        self.host_jobs.timed_out_unanswered(timeout)
    }

    pub(super) fn mark_host_job_timeout(
        &mut self,
        request_id: RequestId,
    ) -> Option<HostJobTimeout> {
        self.host_jobs.mark_timeout_response(request_id)
    }

    pub(super) fn remove_host_job(&mut self, request_id: RequestId) {
        self.host_jobs.remove(request_id);
    }

    pub(super) fn finish_host_job_after_response(
        &mut self,
        request_id: RequestId,
        generation: HostJobGeneration,
    ) {
        self.host_jobs.finish_after_response(request_id, generation);
    }

    pub(super) fn complete_host_job(
        &mut self,
        request_id: RequestId,
        generation: HostJobGeneration,
    ) {
        self.host_jobs.complete_and_remove(request_id, generation);
    }

    pub(super) fn reap_completed_host_jobs(&mut self) {
        self.host_jobs.reap_completed();
    }

    /// Test-only: force advertised version/protocol for lifecycle replacement tests.
    #[cfg(test)]
    pub(super) fn set_advertised_versions_for_test(
        &mut self,
        app_version: impl Into<String>,
        worker_protocol: u32,
    ) {
        self.app_version = app_version.into();
        self.worker_protocol = worker_protocol;
    }

    #[cfg(test)]
    pub(super) fn set_artifact_digest_for_test(&mut self, artifact_digest: [u8; 32]) {
        self.artifact_digest = artifact_digest;
    }

    /// Test-only: insert a stale pending job for timeout accounting tests.
    #[cfg(test)]
    pub(super) fn insert_host_job_for_test(
        &mut self,
        request_id: RequestId,
        kind: HostJobKind,
        location: ResourceLocation,
        started_at: Instant,
        cancel: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    ) {
        self.host_jobs
            .insert_for_test(request_id, kind, location, started_at, cancel, finished);
    }

    #[cfg(test)]
    pub(super) fn host_job_is_responded_for_test(&self, request_id: &RequestId) -> bool {
        self.host_jobs.is_responded(request_id)
    }

    #[cfg(test)]
    pub(super) fn mark_host_job_finished_for_test(&mut self, request_id: RequestId) {
        self.host_jobs.mark_finished_for_test(request_id);
    }

    /// Test-only: shut down and drop a live runtime pair after assertions.
    #[cfg(test)]
    pub(super) fn shutdown_runtime_for_test(&mut self, runtime_id: &WorkerRuntimeId) {
        if let Some((_record, Some(runtime))) = self.runtimes.remove_pair(runtime_id) {
            runtime.shutdown();
        }
    }
}

pub(super) fn validated_scrollback_limit(requested: u64) -> Result<usize, WorkerError> {
    if requested == 0 {
        return Ok(DEFAULT_WORKER_SCROLLBACK_BYTES);
    }
    usize::try_from(requested).map_err(|_| {
        worker_error(
            WorkerErrorCode::InvalidLocation,
            format!("scrollback_limit_bytes {requested} exceeds platform addressable memory"),
        )
    })
}

pub(super) fn remote_home() -> Result<PathBuf, WorkerError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        worker_error(
            WorkerErrorCode::InvalidLocation,
            "remote HOME is unavailable for tilde expansion",
        )
    })?;
    if home.is_empty() {
        return Err(worker_error(
            WorkerErrorCode::InvalidLocation,
            "remote HOME is empty",
        ));
    }
    Ok(PathBuf::from(home))
}

#[cfg(all(test, unix))]
mod ownership_tests {
    use super::*;
    use crate::execution_host::protocol::{HostBindingGeneration, WorkerInstanceId};
    use crate::execution_host::ExecutionHostId;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    static NEXT_TEST_STATE_ID: AtomicU64 = AtomicU64::new(1);

    fn test_state() -> WorkerState {
        let state_id = NEXT_TEST_STATE_ID.fetch_add(1, Ordering::Relaxed);
        let host_id = format!("ssh:own-{}-{state_id}", std::process::id());
        let binding = DaemonBinding {
            installation_id: crate::execution_host::protocol::CoordinatorInstallationId::new(
                "own-install",
            )
            .unwrap(),
            session_namespace_id: crate::execution_host::protocol::SessionNamespaceId::new(
                "own-session",
            )
            .unwrap(),
            execution_host_id: ExecutionHostId::new(host_id).unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            worker_instance_id: WorkerInstanceId::new("own-worker").unwrap(),
            socket_path: std::env::temp_dir().join("omh-own-worker.sock"),
        };
        WorkerState::new(binding).unwrap()
    }

    #[test]
    fn host_job_complete_removes_slot_without_duplicate_response_flag() {
        let mut state = test_state();
        let location = ResourceLocation::new(
            state.binding().execution_host_id.clone(),
            HostPath::new(std::env::temp_dir()).unwrap(),
        );
        let request_id = RequestId::new(1);
        let (_, _, generation) = state
            .insert_host_job(request_id, HostJobKind::ValidatePath, location)
            .unwrap();
        assert_eq!(state.live_host_job_count(), 1);
        assert!(state.host_job_contains(&request_id));

        state.complete_host_job(request_id, generation);
        assert!(!state.host_job_contains(&request_id));
        assert_eq!(state.live_host_job_count(), 0);

        // A second completion cannot resurrect the removed slot.
        state.complete_host_job(request_id, generation);
        assert!(state.host_jobs_is_empty());
    }

    #[test]
    fn host_job_timeout_then_finish_reaps_without_duplicate_live_count() {
        let mut state = test_state();
        let location = ResourceLocation::new(
            state.binding().execution_host_id.clone(),
            HostPath::new(std::env::temp_dir()).unwrap(),
        );
        let request_id = RequestId::new(2);
        let finished = Arc::new(AtomicBool::new(false));
        state.insert_host_job_for_test(
            request_id,
            HostJobKind::GitStatus,
            location,
            Instant::now() - Duration::from_secs(120),
            Arc::new(AtomicBool::new(false)),
            finished.clone(),
        );
        let timeout = state
            .mark_host_job_timeout(request_id)
            .expect("timeout once");
        assert!(!timeout.finished);
        assert!(state.host_job_is_responded_for_test(&request_id));
        assert!(state.mark_host_job_timeout(request_id).is_none());

        finished.store(true, Ordering::Relaxed);
        state.reap_completed_host_jobs();
        assert!(state.host_jobs_is_empty());
        assert_eq!(state.live_host_job_count(), 0);
    }

    #[tokio::test]
    async fn take_exited_runtime_is_idempotent_after_pair_removal() {
        let mut state = test_state();
        let location = ResourceLocation::new(
            state.binding().execution_host_id.clone(),
            HostPath::new(std::env::temp_dir()).unwrap(),
        );
        let (identity, _) = state
            .create_terminal(
                location,
                TerminalSize { cols: 80, rows: 24 },
                Some(CommandSpec {
                    program: "/bin/sh".to_string(),
                    args: vec!["-c".to_string(), "sleep 30".to_string()],
                    env: Vec::new(),
                }),
                Vec::new(),
                DEFAULT_WORKER_SCROLLBACK_BYTES,
            )
            .unwrap();
        let local_id = state
            .runtime_record(&identity.runtime_id)
            .expect("live")
            .local_id;
        assert_eq!(state.owned_runtime_count(), 1);

        let first = state.take_exited_runtime(local_id);
        assert!(first.is_some());
        assert_eq!(state.owned_runtime_count(), 0);
        assert!(!state.contains_runtime(&identity.runtime_id));

        // Duplicate exit must not observe a second record (no double completion).
        assert!(state.take_exited_runtime(local_id).is_none());
        assert!(state.is_idle_for_shutdown());
    }
}
