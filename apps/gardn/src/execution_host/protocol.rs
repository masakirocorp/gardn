//! Execution Worker Protocol v1.
//!
//! Framed, authenticated, versioned bincode messages exchanged over a persistent
//! OpenSSH stdio channel between the coordinator and a remote execution worker.
//! This protocol is distinct from the Local API and the thin-client wire protocol.
//!
//! Framing reuses [`crate::protocol::{read_message, write_message}`] with
//! [`MAX_FRAME_SIZE`]. Live runtimes and credentials stay outside this module:
//! no private-key or passphrase fields are defined.

mod codec;
mod coordinator;
mod identity;
mod types;
mod worker;

#[cfg(test)]
mod tests;

pub(crate) use codec::{
    read_worker_message, validate_first_coordinator_message, validate_first_worker_message,
    write_worker_message, HandshakeError,
};
pub(crate) use coordinator::CoordinatorMessage;
#[cfg(test)]
pub(crate) use identity::{AuthProof, ProtocolIdError, MAX_FRAME_SIZE};
pub(crate) use identity::{
    CoordinatorInstallationId, HostBindingGeneration, OutputRevision, RequestId, RuntimeIdentity,
    RuntimeIncarnation, RuntimeOpSeq, SessionNamespaceId, WorkerInstanceId, WorkerRuntimeId,
    PROTOCOL_VERSION,
};
pub(crate) use types::{
    AgentLaunch, AttachResume, CommandSpec, GitStatusSnapshot, HostHealthStatus, ObservedProcess,
    PathCompletionEntry, PortSnapshot, PortTransport, ProcessObservation, ProjectCommandConfidence,
    ProjectCommandSnapshot, ProjectCommandSource, RuntimeExitStatus, TerminalSize, TerminateMode,
    WorkerCapability, WorkerError, WorkerErrorCode, WorkerSignal, WorktreeSnapshot,
};
pub(crate) use worker::{AuthChallenge, WorkerMessage};
