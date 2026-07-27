//! Messages sent from an execution worker to the coordinator.

use serde::{Deserialize, Serialize};

use super::identity::{
    AuthChallengeId, HostBindingGeneration, OutputRevision, RequestId, RuntimeIdentity,
    RuntimeOpSeq, WorkerInstanceId,
};
use super::types::{
    AttachResume, GitStatusSnapshot, HostHealthStatus, PathCompletionEntry, PortSnapshot,
    ProcessObservation, ProjectCommandSnapshot, RuntimeExitStatus, WorkerCapability, WorkerError,
    WorktreeSnapshot,
};
use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};

// ---------------------------------------------------------------------------
// Worker → Coordinator
// ---------------------------------------------------------------------------

/// Messages sent from an execution worker to the coordinator.
///
/// `HelloAck` is variant 0 and MUST be the first message on a new channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerMessage {
    /// Mandatory first response to coordinator Hello.
    HelloAck {
        version: u32,
        worker_instance_id: WorkerInstanceId,
        host_binding_generation: HostBindingGeneration,
        execution_host_id: ExecutionHostId,
        capabilities: Vec<WorkerCapability>,
        /// Present when the worker requires an AuthProof before serving requests.
        auth_challenge: Option<AuthChallenge>,
        /// Present when the handshake is rejected; the channel should close.
        error: Option<WorkerError>,
    },
    AuthResult {
        request_id: RequestId,
        accepted: bool,
        error: Option<WorkerError>,
    },
    CreateTerminalResult {
        request_id: RequestId,
        identity: Option<RuntimeIdentity>,
        location: ResourceLocation,
        error: Option<WorkerError>,
    },
    AdoptTerminalResult {
        request_id: RequestId,
        identity: Option<RuntimeIdentity>,
        location: ResourceLocation,
        /// Last runtime op the worker applied for this identity; coordinator resumes at last+1.
        last_applied_op_seq: RuntimeOpSeq,
        error: Option<WorkerError>,
    },
    AttachTerminalResult {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        resume: AttachResume,
        error: Option<WorkerError>,
    },
    /// Ordered output after an acknowledged revision.
    OutputDelta {
        identity: RuntimeIdentity,
        location: ResourceLocation,
        base_revision: OutputRevision,
        revision: OutputRevision,
        data: Vec<u8>,
    },
    /// Complete canonical checkpoint when deltas were evicted or on fresh attach.
    OutputCheckpoint {
        identity: RuntimeIdentity,
        location: ResourceLocation,
        revision: OutputRevision,
        data: Vec<u8>,
    },
    RuntimeExit {
        identity: RuntimeIdentity,
        location: ResourceLocation,
        status: RuntimeExitStatus,
    },
    TerminalStateChanged {
        identity: RuntimeIdentity,
        location: ResourceLocation,
        agent: Option<crate::detect::Agent>,
        state: crate::detect::AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    },
    HostHealth {
        request_id: Option<RequestId>,
        location: ResourceLocation,
        status: HostHealthStatus,
        detail: Option<String>,
    },
    PathCompletion {
        request_id: RequestId,
        location: ResourceLocation,
        entries: Vec<PathCompletionEntry>,
        error: Option<WorkerError>,
    },
    PathValidation {
        request_id: RequestId,
        location: ResourceLocation,
        exists: bool,
        is_dir: bool,
        error: Option<WorkerError>,
    },
    ProcessObservationResult {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        process: Option<ProcessObservation>,
        error: Option<WorkerError>,
    },
    GitStatusResult {
        request_id: RequestId,
        location: ResourceLocation,
        status: Option<GitStatusSnapshot>,
        error: Option<WorkerError>,
    },
    WorktreeListResult {
        request_id: RequestId,
        location: ResourceLocation,
        worktrees: Vec<WorktreeSnapshot>,
        error: Option<WorkerError>,
    },
    CommandResult {
        request_id: RequestId,
        location: ResourceLocation,
        exit: Option<RuntimeExitStatus>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        error: Option<WorkerError>,
    },
    StartAgentResult {
        request_id: RequestId,
        location: ResourceLocation,
        identity: Option<RuntimeIdentity>,
        error: Option<WorkerError>,
    },
    PortObservationResult {
        request_id: RequestId,
        location: ResourceLocation,
        ports: Vec<PortSnapshot>,
        error: Option<WorkerError>,
    },
    RequestAck {
        request_id: RequestId,
        error: Option<WorkerError>,
    },
    Error {
        request_id: Option<RequestId>,
        error: WorkerError,
    },
    StageFileResult {
        request_id: RequestId,
        location: ResourceLocation,
        path: Option<HostPath>,
        error: Option<WorkerError>,
    },
    ProjectCommandsResult {
        request_id: RequestId,
        location: ResourceLocation,
        commands: Vec<ProjectCommandSnapshot>,
        error: Option<WorkerError>,
    },
    AgentIntegrationsResult {
        request_id: RequestId,
        result: Option<crate::integration::host::HostIntegrationResult>,
        error: Option<WorkerError>,
    },
    AgentHookReported {
        identity: RuntimeIdentity,
        report: crate::integration::host::WorkerHookReport,
    },
}

/// Ephemeral authentication challenge issued by the worker.
///
/// Contains no private keys or passphrases — only a challenge id and nonce.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthChallenge {
    pub(crate) challenge_id: AuthChallengeId,
    pub(crate) nonce: Vec<u8>,
}
