//! Private paired owners for worker runtime and host-job maps.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

use crate::execution_host::protocol::{
    CommandSpec, RequestId, RuntimeIdentity, TerminalSize, WorkerError, WorkerErrorCode,
    WorkerRuntimeId,
};
use crate::execution_host::ResourceLocation;
use crate::terminal::{TerminalId, TerminalRuntime, TerminalRuntimeRegistry};

use super::event::RuntimeLocalId;
use super::output::OutputLog;
use super::util::{worker_error, MAX_HOST_JOBS};

#[cfg(unix)]
pub(super) struct RuntimeRecord {
    pub(super) terminal_id: TerminalId,
    /// Local correlation id for worker-native events (not a UI pane id).
    pub(super) local_id: RuntimeLocalId,
    pub(super) identity: RuntimeIdentity,
    pub(super) location: ResourceLocation,
    pub(super) output: OutputLog,
    pub(super) last_op_seq: u64,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CreateKind {
    Terminal,
    Agent(String),
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CreateRequest {
    pub(super) kind: CreateKind,
    pub(super) location: ResourceLocation,
    pub(super) size: TerminalSize,
    pub(super) command: Option<CommandSpec>,
    pub(super) env: Vec<(String, String)>,
    pub(super) scrollback_limit_bytes: usize,
}

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HostJobKind {
    GitStatus,
    ListWorktrees,
    RunCommand,
    ObservePorts,
    DiscoverProjectCommands,
    CompletePath {
        prefix: String,
    },
    ValidatePath,
    ManageAgentIntegrations {
        request: crate::integration::host::HostIntegrationRequest,
    },
    Create(CreateRequest),
}

/// Paired runtime records + PTY runtimes. Insert/remove always keep both sides consistent.
#[cfg(unix)]
pub(super) struct RuntimeTable {
    runtimes: TerminalRuntimeRegistry,
    records: HashMap<WorkerRuntimeId, RuntimeRecord>,
    next_runtime: AtomicU64,
}

#[cfg(unix)]
impl RuntimeTable {
    pub(super) fn new() -> Self {
        Self {
            runtimes: TerminalRuntimeRegistry::new(),
            records: HashMap::new(),
            next_runtime: AtomicU64::new(1),
        }
    }

    pub(super) fn owned_count(&self) -> u64 {
        // Conservatively treat any inconsistency between records and runtimes as busy.
        self.records.len().max(self.runtimes.len()) as u64
    }

    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty() && self.runtimes.len() == 0
    }

    #[cfg(test)]
    pub(super) fn record_count(&self) -> usize {
        self.records.len()
    }

    #[cfg(test)]
    pub(super) fn contains_record(&self, runtime_id: &WorkerRuntimeId) -> bool {
        self.records.contains_key(runtime_id)
    }

    pub(super) fn record(&self, runtime_id: &WorkerRuntimeId) -> Option<&RuntimeRecord> {
        self.records.get(runtime_id)
    }

    pub(super) fn record_mut(
        &mut self,
        runtime_id: &WorkerRuntimeId,
    ) -> Option<&mut RuntimeRecord> {
        self.records.get_mut(runtime_id)
    }

    pub(super) fn record_by_local_id(&self, local_id: RuntimeLocalId) -> Option<&RuntimeRecord> {
        self.records
            .values()
            .find(|record| record.local_id == local_id)
    }

    pub(super) fn runtime(&self, terminal_id: &TerminalId) -> Option<&TerminalRuntime> {
        self.runtimes.get(terminal_id)
    }

    pub(super) fn alloc_runtime_number(&self) -> u64 {
        self.next_runtime.fetch_add(1, Ordering::Relaxed)
    }

    pub(super) fn insert_pair(
        &mut self,
        runtime_id: WorkerRuntimeId,
        record: RuntimeRecord,
        runtime: TerminalRuntime,
    ) {
        let terminal_id = record.terminal_id.clone();
        self.runtimes.insert(terminal_id, runtime);
        self.records.insert(runtime_id, record);
    }

    /// Remove record and paired runtime together.
    pub(super) fn remove_pair(
        &mut self,
        runtime_id: &WorkerRuntimeId,
    ) -> Option<(RuntimeRecord, Option<TerminalRuntime>)> {
        let record = self.records.remove(runtime_id)?;
        let runtime = self.runtimes.remove(&record.terminal_id);
        Some((record, runtime))
    }

    /// Remove by local event id; returns the detached pair for exit handling.
    pub(super) fn take_by_local_id(
        &mut self,
        local_id: RuntimeLocalId,
    ) -> Option<(WorkerRuntimeId, RuntimeRecord, Option<TerminalRuntime>)> {
        let runtime_id = self.records.iter().find_map(|(runtime_id, record)| {
            (record.local_id == local_id).then(|| runtime_id.clone())
        })?;
        let (record, runtime) = self.remove_pair(&runtime_id)?;
        Some((runtime_id, record, runtime))
    }
}

pub(super) type HostJobGeneration = u64;

/// Pending host observation/create job. Transition flags are owned by [`HostJobTable`].
#[cfg(unix)]
struct PendingHostJob {
    generation: HostJobGeneration,
    kind: HostJobKind,
    location: ResourceLocation,
    started_at: Instant,
    cancel: Arc<AtomicBool>,
    responded: bool,
    finished: Arc<AtomicBool>,
}

/// Read-only snapshot of a live host job for response routing.
#[cfg(unix)]
#[derive(Clone)]
pub(super) struct HostJobSnapshot {
    pub(super) generation: HostJobGeneration,
    pub(super) kind: HostJobKind,
    pub(super) location: ResourceLocation,
    pub(super) responded: bool,
    pub(super) cancelled: bool,
}

/// Timed-out job payload used when writing the timeout response.
#[cfg(unix)]
pub(super) struct HostJobTimeout {
    pub(super) kind: HostJobKind,
    pub(super) location: ResourceLocation,
    pub(super) finished: bool,
}

#[cfg(unix)]
struct IntegrationJobGate {
    active_ticket: Mutex<u64>,
    ready: Condvar,
}

#[cfg(unix)]
pub(super) struct IntegrationJobTurn {
    ticket: u64,
    gate: Arc<IntegrationJobGate>,
}

#[cfg(unix)]
impl IntegrationJobTurn {
    pub(super) fn run<T>(self, operation: impl FnOnce() -> T) -> T {
        let mut active_ticket = self
            .gate
            .active_ticket
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *active_ticket != self.ticket {
            active_ticket = self
                .gate
                .ready
                .wait(active_ticket)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        let _advance = IntegrationTurnAdvance {
            active_ticket,
            ready: &self.gate.ready,
        };
        operation()
    }
}

#[cfg(unix)]
struct IntegrationTurnAdvance<'a> {
    active_ticket: MutexGuard<'a, u64>,
    ready: &'a Condvar,
}

#[cfg(unix)]
impl Drop for IntegrationTurnAdvance<'_> {
    fn drop(&mut self) {
        *self.active_ticket = self.active_ticket.wrapping_add(1);
        self.ready.notify_all();
    }
}

/// Host-job map with atomic responded/finished/remove transitions.
#[cfg(unix)]
pub(super) struct HostJobTable {
    jobs: HashMap<RequestId, PendingHostJob>,
    next_job_generation: HostJobGeneration,
    next_integration_ticket: u64,
    integration_gate: Arc<IntegrationJobGate>,
}

#[cfg(unix)]
impl HostJobTable {
    pub(super) fn new() -> Self {
        Self {
            next_job_generation: 1,
            jobs: HashMap::new(),
            next_integration_ticket: 0,
            integration_gate: Arc::new(IntegrationJobGate {
                active_ticket: Mutex::new(0),
                ready: Condvar::new(),
            }),
        }
    }
    pub(super) fn reserve_integration_turn(&mut self) -> IntegrationJobTurn {
        let ticket = self.next_integration_ticket;
        self.next_integration_ticket = self.next_integration_ticket.wrapping_add(1);
        IntegrationJobTurn {
            ticket,
            gate: Arc::clone(&self.integration_gate),
        }
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    #[cfg(test)]
    pub(super) fn contains(&self, request_id: &RequestId) -> bool {
        self.jobs.contains_key(request_id)
    }

    pub(super) fn live_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|job| !job.finished.load(Ordering::Relaxed))
            .count()
    }

    pub(super) fn snapshot(&self, request_id: &RequestId) -> Option<HostJobSnapshot> {
        self.jobs.get(request_id).map(|job| HostJobSnapshot {
            generation: job.generation,
            kind: job.kind.clone(),
            location: job.location.clone(),
            responded: job.responded,
            cancelled: job.cancel.load(Ordering::Relaxed),
        })
    }

    /// Insert a new in-flight job. Returns the cancel/finished flags for the worker thread.
    pub(super) fn insert(
        &mut self,
        request_id: RequestId,
        kind: HostJobKind,
        location: ResourceLocation,
    ) -> Result<(Arc<AtomicBool>, Arc<AtomicBool>, HostJobGeneration), WorkerError> {
        if self.jobs.contains_key(&request_id) {
            return Err(worker_error(
                WorkerErrorCode::Conflict,
                "host job request id is already in flight",
            ));
        }
        if self.live_count() >= MAX_HOST_JOBS {
            return Err(worker_error(
                WorkerErrorCode::Busy,
                format!("at most {MAX_HOST_JOBS} live host jobs may run at once"),
            ));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let generation = self.next_job_generation;
        self.next_job_generation = self.next_job_generation.wrapping_add(1);
        self.jobs.insert(
            request_id,
            PendingHostJob {
                generation,
                kind,
                location,
                started_at: Instant::now(),
                cancel: cancel.clone(),
                responded: false,
                finished: finished.clone(),
            },
        );
        Ok((cancel, finished, generation))
    }

    pub(super) fn cancel_for_disconnect(&mut self) {
        for job in self.jobs.values_mut() {
            job.cancel.store(true, Ordering::Relaxed);
            // Retain accounting until the worker thread returns and reaps the slot.
            if !job.responded {
                job.responded = true;
            }
        }
    }

    pub(super) fn timed_out_unanswered(&self, timeout: std::time::Duration) -> Vec<RequestId> {
        self.jobs
            .iter()
            .filter_map(|(request_id, job)| {
                (!job.responded
                    && !matches!(&job.kind, HostJobKind::ManageAgentIntegrations { .. })
                    && job.started_at.elapsed() >= timeout)
                    .then_some(*request_id)
            })
            .collect()
    }

    /// Mark timeout response. Returns payload if the job still needed a response.
    pub(super) fn mark_timeout_response(
        &mut self,
        request_id: RequestId,
    ) -> Option<HostJobTimeout> {
        let job = self.jobs.get_mut(&request_id)?;
        job.cancel.store(true, Ordering::Relaxed);
        if job.responded {
            return None;
        }
        job.responded = true;
        Some(HostJobTimeout {
            kind: job.kind.clone(),
            location: job.location.clone(),
            finished: job.finished.load(Ordering::Relaxed),
        })
    }

    pub(super) fn remove(&mut self, request_id: RequestId) {
        self.jobs.remove(&request_id);
    }

    /// Thread returned after a prior response (timeout/disconnect): mark finished and drop.
    pub(super) fn finish_after_response(
        &mut self,
        request_id: RequestId,
        generation: HostJobGeneration,
    ) {
        let matches_generation = self
            .jobs
            .get(&request_id)
            .is_some_and(|job| job.generation == generation);
        if !matches_generation {
            return;
        }
        if let Some(job) = self.jobs.get_mut(&request_id) {
            job.finished.store(true, Ordering::Relaxed);
        }
        self.jobs.remove(&request_id);
    }

    /// Mark responded+finished and remove in one transition (normal completion path).
    pub(super) fn complete_and_remove(
        &mut self,
        request_id: RequestId,
        generation: HostJobGeneration,
    ) {
        let matches_generation = self
            .jobs
            .get(&request_id)
            .is_some_and(|job| job.generation == generation);
        if !matches_generation {
            return;
        }
        if let Some(job) = self.jobs.get_mut(&request_id) {
            job.responded = true;
            job.finished.store(true, Ordering::Relaxed);
        }
        self.jobs.remove(&request_id);
    }

    pub(super) fn reap_completed(&mut self) {
        self.jobs
            .retain(|_, job| !(job.responded && job.finished.load(Ordering::Relaxed)));
    }

    #[cfg(test)]
    pub(super) fn insert_for_test(
        &mut self,
        request_id: RequestId,
        kind: HostJobKind,
        location: ResourceLocation,
        started_at: Instant,
        cancel: Arc<AtomicBool>,
        finished: Arc<AtomicBool>,
    ) {
        self.jobs.insert(
            request_id,
            PendingHostJob {
                generation: 0,
                kind,
                location,
                started_at,
                cancel,
                responded: false,
                finished,
            },
        );
    }

    #[cfg(test)]
    pub(super) fn is_responded(&self, request_id: &RequestId) -> bool {
        self.jobs.get(request_id).is_some_and(|job| job.responded)
    }

    #[cfg(test)]
    pub(super) fn mark_finished_for_test(&mut self, request_id: RequestId) {
        if let Some(job) = self.jobs.get_mut(&request_id) {
            job.finished.store(true, Ordering::Relaxed);
        }
    }
}
