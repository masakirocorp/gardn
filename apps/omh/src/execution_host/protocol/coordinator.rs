//! Messages sent from the coordinator to an execution worker.

use serde::{Deserialize, Serialize};

use super::identity::{
    AuthChallengeId, AuthProof, CoordinatorInstallationId, HostBindingGeneration, OutputRevision,
    RequestId, RuntimeIdentity, RuntimeOpSeq, SessionNamespaceId,
};
use super::types::{
    AgentLaunch, AttachResume, CommandSpec, TerminalSize, TerminateMode, WorkerCapability,
    WorkerSignal,
};
use crate::execution_host::{ExecutionHostId, ResourceLocation};

// ---------------------------------------------------------------------------
// Coordinator → Worker
// ---------------------------------------------------------------------------

/// Messages sent from the coordinator to an execution worker.
///
/// `Hello` is variant 0 and MUST be the first message on a new channel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CoordinatorMessage {
    /// Mandatory first message: version and binding scope.
    Hello {
        version: u32,
        coordinator_installation_id: CoordinatorInstallationId,
        session_namespace_id: SessionNamespaceId,
        execution_host_id: ExecutionHostId,
        host_binding_generation: HostBindingGeneration,
        /// Optional proof responding to a prior out-of-band challenge material.
        auth_proof: Option<AuthProof>,
        capabilities: Vec<WorkerCapability>,
    },
    /// Complete an ephemeral challenge issued by the worker after HelloAck.
    AuthProof {
        request_id: RequestId,
        challenge_id: AuthChallengeId,
        proof: AuthProof,
    },
    CreateTerminal {
        request_id: RequestId,
        location: ResourceLocation,
        size: TerminalSize,
        /// Optional program; `None` means the worker default login shell.
        command: Option<CommandSpec>,
        env: Vec<(String, String)>,
        /// Validated coordinator scrollback budget for the worker PaneTerminal.
        scrollback_limit_bytes: u64,
    },
    AdoptTerminal {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
    },
    AttachTerminal {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        resume: AttachResume,
    },
    Input {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        op_seq: RuntimeOpSeq,
        data: Vec<u8>,
    },
    Resize {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        op_seq: RuntimeOpSeq,
        size: TerminalSize,
    },
    Signal {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        op_seq: RuntimeOpSeq,
        signal: WorkerSignal,
    },
    Terminate {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        mode: TerminateMode,
    },
    ObserveProcess {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
    },
    CompletePath {
        request_id: RequestId,
        location: ResourceLocation,
        prefix: String,
    },
    ValidatePath {
        request_id: RequestId,
        location: ResourceLocation,
    },
    GitStatus {
        request_id: RequestId,
        location: ResourceLocation,
    },
    ListWorktrees {
        request_id: RequestId,
        location: ResourceLocation,
    },
    RunCommand {
        request_id: RequestId,
        location: ResourceLocation,
        command: CommandSpec,
    },
    StartAgent {
        request_id: RequestId,
        location: ResourceLocation,
        agent: AgentLaunch,
        size: TerminalSize,
        /// Validated coordinator scrollback budget for the worker PaneTerminal.
        scrollback_limit_bytes: u64,
    },
    ObservePorts {
        request_id: RequestId,
        location: ResourceLocation,
    },
    DiscoverProjectCommands {
        request_id: RequestId,
        location: ResourceLocation,
    },
    QueryHostHealth {
        request_id: RequestId,
        location: ResourceLocation,
    },
    AckOutput {
        request_id: RequestId,
        identity: RuntimeIdentity,
        location: ResourceLocation,
        revision: OutputRevision,
    },
    Shutdown {
        request_id: RequestId,
    },
    StageFile {
        request_id: RequestId,
        location: ResourceLocation,
        extension: String,
        data: Vec<u8>,
        ttl_secs: u32,
    },
    RemoveStagedFile {
        request_id: RequestId,
        location: ResourceLocation,
    },
}
