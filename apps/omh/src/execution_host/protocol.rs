//! Execution Worker Protocol v1.
//!
//! Framed, authenticated, versioned bincode messages exchanged over a persistent
//! OpenSSH stdio channel between the coordinator and a remote execution worker.
//! This protocol is distinct from the Local API and the thin-client wire protocol.
//!
//! Framing reuses [`crate::protocol::{read_message, write_message}`] with
//! [`MAX_FRAME_SIZE`]. Live runtimes and credentials stay outside this module:
//! no private-key or passphrase fields are defined.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::protocol::{self, FramingError};

use super::{HostPath, ResourceLocation};

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

/// Execution Worker Protocol compatibility marker.
///
/// Exact-match negotiation only for v1. Distinct from the thin-client
/// [`crate::protocol::PROTOCOL_VERSION`].
pub(crate) const PROTOCOL_VERSION: u32 = 1;

/// Maximum worker-protocol frame payload (16 MiB).
///
/// Sized so a canonical terminal checkpoint can cover configured scrollback
/// without borrowing the thin-client graphics exception.
pub(crate) const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

const MAX_INSTALLATION_ID_LEN: usize = 128;
const MAX_SESSION_NAMESPACE_ID_LEN: usize = 64;
const MAX_WORKER_INSTANCE_ID_LEN: usize = 128;
const MAX_WORKER_RUNTIME_ID_LEN: usize = 128;
const MAX_AUTH_CHALLENGE_ID_LEN: usize = 128;
const MAX_AUTH_PROOF_LEN: usize = 512;
const MAX_ERROR_MESSAGE_LEN: usize = 1024;
const MAX_COMMAND_LEN: usize = 4096;
const MAX_AGENT_ID_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Monotonic numeric newtypes
// ---------------------------------------------------------------------------

macro_rules! u64_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) const fn new(value: u64) -> Self {
                Self(value)
            }

            pub(crate) const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

u64_newtype!(
    /// Correlates a coordinator request with worker replies.
    RequestId
);
u64_newtype!(
    /// Monotonic ordered output revision for one runtime incarnation.
    OutputRevision
);
u64_newtype!(
    /// Generation of the coordinator's host binding for this logical host.
    HostBindingGeneration
);
u64_newtype!(
    /// Worker-assigned incarnation for one live runtime identity.
    RuntimeIncarnation
);
u64_newtype!(
    /// Per-runtime operation sequence for ordered input/signal/resize.
    RuntimeOpSeq
);

// ---------------------------------------------------------------------------
// Validated opaque string newtypes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtocolIdError {
    Empty,
    TooLong,
    InvalidCharacter,
}

impl fmt::Display for ProtocolIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "value must not be empty",
            Self::TooLong => "value is too long",
            Self::InvalidCharacter => "value contains an invalid character",
        })
    }
}

fn validate_token(value: &str, max_len: usize, allow_extra: &[u8]) -> Result<(), ProtocolIdError> {
    if value.is_empty() {
        return Err(ProtocolIdError::Empty);
    }
    if value.len() > max_len {
        return Err(ProtocolIdError::TooLong);
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
            || allow_extra.contains(&byte)
    }) {
        return Err(ProtocolIdError::InvalidCharacter);
    }
    Ok(())
}

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident, $max:expr) => {
        string_id!($(#[$meta])* $name, $max, &[]);
    };
    ($(#[$meta:meta])* $name:ident, $max:expr, $extra:expr) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn new(value: impl Into<String>) -> Result<Self, ProtocolIdError> {
                let value = value.into();
                validate_token(&value, $max, $extra)?;
                Ok(Self(value))
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

string_id!(
    /// Durable coordinator installation identity scoping worker leases.
    CoordinatorInstallationId,
    MAX_INSTALLATION_ID_LEN
);
string_id!(
    /// Session Namespace UUID (opaque string form) for this coordinator session.
    SessionNamespaceId,
    MAX_SESSION_NAMESPACE_ID_LEN
);
string_id!(
    /// Worker process/instance identity assigned at worker boot.
    WorkerInstanceId,
    MAX_WORKER_INSTANCE_ID_LEN
);
string_id!(
    /// Worker-local runtime id. Never a public coordinator id.
    WorkerRuntimeId,
    MAX_WORKER_RUNTIME_ID_LEN
);
string_id!(
    /// Ephemeral authentication challenge identifier.
    AuthChallengeId,
    MAX_AUTH_CHALLENGE_ID_LEN
);
string_id!(
    /// Ephemeral authentication proof material (not a private key).
    AuthProof,
    MAX_AUTH_PROOF_LEN,
    &[b'+', b'=', b'/']
);

// ---------------------------------------------------------------------------
// Runtime identity
// ---------------------------------------------------------------------------

/// Full adoption key for a worker-owned runtime.
///
/// Adoption matches the complete tuple — never PID alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeIdentity {
    pub(crate) host_binding_generation: HostBindingGeneration,
    pub(crate) worker_instance_id: WorkerInstanceId,
    pub(crate) runtime_id: WorkerRuntimeId,
    pub(crate) incarnation: RuntimeIncarnation,
}

impl RuntimeIdentity {
    pub(crate) fn new(
        host_binding_generation: HostBindingGeneration,
        worker_instance_id: WorkerInstanceId,
        runtime_id: WorkerRuntimeId,
        incarnation: RuntimeIncarnation,
    ) -> Self {
        Self {
            host_binding_generation,
            worker_instance_id,
            runtime_id,
            incarnation,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared payload types
// ---------------------------------------------------------------------------

/// Worker feature surface advertised during handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerCapability {
    Terminal,
    PathCompletion,
    ProcessObservation,
    Git,
    Worktree,
    Command,
    Agent,
    Ports,
}

/// Process signal applied on the execution host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerSignal {
    Hangup,
    Interrupt,
    Terminate,
    Kill,
    Winch,
}

/// How a create/attach request should resume ordered output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttachResume {
    /// Stream deltas strictly after this acknowledged revision.
    AfterRevision(OutputRevision),
    /// Prefer a complete canonical checkpoint when deltas are unavailable.
    Checkpoint,
}

/// Terminal close outcome requested by the coordinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TerminateMode {
    /// Request process teardown and wait for exit when reachable.
    Terminate,
    /// Drop coordinator mapping without sending a kill (offline forget).
    Forget,
}

/// Observed process exit on the execution host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeExitStatus {
    Code(i32),
    Signal(i32),
}

/// User-visible host connectivity from the worker's perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostHealthStatus {
    Ready,
    Degraded,
    Unavailable,
}

/// Typed protocol error without free-form shell payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkerErrorCode {
    Unauthorized,
    ProtocolMismatch,
    InvalidHandshake,
    UnknownRuntime,
    IncarnationMismatch,
    BindingMismatch,
    UnsupportedCapability,
    InvalidLocation,
    NotFound,
    Conflict,
    Gone,
    Busy,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerError {
    pub(crate) code: WorkerErrorCode,
    pub(crate) message: String,
}

impl WorkerError {
    pub(crate) fn new(code: WorkerErrorCode, message: impl Into<String>) -> Self {
        let mut message = message.into();
        if message.len() > MAX_ERROR_MESSAGE_LEN {
            message.truncate(MAX_ERROR_MESSAGE_LEN);
        }
        Self { code, message }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalSize {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommandSpec {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl CommandSpec {
    pub(crate) fn validate(&self) -> Result<(), ProtocolIdError> {
        if self.program.is_empty() || self.program.len() > MAX_COMMAND_LEN {
            return Err(ProtocolIdError::InvalidCharacter);
        }
        for arg in &self.args {
            if arg.len() > MAX_COMMAND_LEN {
                return Err(ProtocolIdError::TooLong);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentLaunch {
    pub(crate) agent_id: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl AgentLaunch {
    pub(crate) fn validate(&self) -> Result<(), ProtocolIdError> {
        validate_token(&self.agent_id, MAX_AGENT_ID_LEN, &[])?;
        for arg in &self.args {
            if arg.len() > MAX_COMMAND_LEN {
                return Err(ProtocolIdError::TooLong);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathCompletionEntry {
    pub(crate) path: HostPath,
    pub(crate) is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessObservation {
    pub(crate) pid: u32,
    pub(crate) ppid: Option<u32>,
    pub(crate) command: Option<String>,
    pub(crate) cwd: Option<HostPath>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GitStatusSnapshot {
    pub(crate) branch: Option<String>,
    pub(crate) dirty: bool,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: u32,
    pub(crate) behind: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorktreeSnapshot {
    pub(crate) location: ResourceLocation,
    pub(crate) branch: Option<String>,
    pub(crate) bare: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortSnapshot {
    pub(crate) bind_addr: String,
    pub(crate) port: u16,
    pub(crate) pid: Option<u32>,
    pub(crate) command: Option<String>,
}

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
    },
    ObservePorts {
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
}

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

// ---------------------------------------------------------------------------
// Handshake validation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HandshakeError {
    ExpectedHello,
    ExpectedHelloAck,
    ProtocolMismatch {
        expected: u32,
        received: u32,
    },
    Rejected(WorkerError),
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedHello => f.write_str("first coordinator message must be Hello"),
            Self::ExpectedHelloAck => f.write_str("first worker message must be HelloAck"),
            Self::ProtocolMismatch { expected, received } => {
                write!(
                    f,
                    "worker protocol mismatch: expected {expected}, received {received}"
                )
            }
            Self::Rejected(error) => {
                write!(f, "worker handshake rejected: {:?} {}", error.code, error.message)
            }
        }
    }
}

impl std::error::Error for HandshakeError {}

/// Reject any non-Hello message as the first coordinator→worker frame.
pub(crate) fn validate_first_coordinator_message(
    message: &CoordinatorMessage,
) -> Result<(), HandshakeError> {
    match message {
        CoordinatorMessage::Hello { version, .. } => {
            if *version != PROTOCOL_VERSION {
                return Err(HandshakeError::ProtocolMismatch {
                    expected: PROTOCOL_VERSION,
                    received: *version,
                });
            }
            Ok(())
        }
        _ => Err(HandshakeError::ExpectedHello),
    }
}

/// Reject any non-HelloAck message as the first worker→coordinator frame.
pub(crate) fn validate_first_worker_message(
    message: &WorkerMessage,
) -> Result<(), HandshakeError> {
    match message {
        WorkerMessage::HelloAck {
            version, error, ..
        } => {
            if *version != PROTOCOL_VERSION {
                return Err(HandshakeError::ProtocolMismatch {
                    expected: PROTOCOL_VERSION,
                    received: *version,
                });
            }
            if let Some(error) = error {
                return Err(HandshakeError::Rejected(error.clone()));
            }
            Ok(())
        }
        _ => Err(HandshakeError::ExpectedHelloAck),
    }
}

// ---------------------------------------------------------------------------
// Framing helpers (public seams over wire framing)
// ---------------------------------------------------------------------------

/// Write one worker-protocol message using length-prefixed bincode framing.
pub(crate) fn write_worker_message<W: std::io::Write, M: Serialize>(
    writer: &mut W,
    message: &M,
) -> Result<(), FramingError> {
    protocol::write_message(writer, message)
}

/// Read one worker-protocol message, enforcing [`MAX_FRAME_SIZE`].
pub(crate) fn read_worker_message<R: std::io::Read, M: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<M, FramingError> {
    protocol::read_message(reader, MAX_FRAME_SIZE)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};
    use crate::protocol::FramingError;

    fn location() -> ResourceLocation {
        ResourceLocation::new(
            ExecutionHostId::new("ssh:workbox").unwrap(),
            HostPath::new("/srv/api").unwrap(),
        )
    }

    fn runtime_identity() -> RuntimeIdentity {
        RuntimeIdentity::new(
            HostBindingGeneration::new(3),
            WorkerInstanceId::new("worker-1").unwrap(),
            WorkerRuntimeId::new("rt-9").unwrap(),
            RuntimeIncarnation::new(7),
        )
    }

    fn assert_bincode_bytes<T>(value: &T, expected: &[u8])
    where
        T: Serialize,
    {
        let encoded = bincode::serde::encode_to_vec(value, bincode::config::standard()).unwrap();
        assert_eq!(
            encoded.as_slice(),
            expected,
            "encoded bytes diverged:\n left: {encoded:02x?}\nright: {expected:02x?}"
        );
    }

    #[test]
    fn coordinator_hello_bincode_bytes_are_stable() {
        let msg = CoordinatorMessage::Hello {
            version: PROTOCOL_VERSION,
            coordinator_installation_id: CoordinatorInstallationId::new("install-a").unwrap(),
            session_namespace_id: SessionNamespaceId::new(
                "01234567-89ab-cdef-0123-456789abcdef",
            )
            .unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            auth_proof: None,
            capabilities: vec![WorkerCapability::Terminal, WorkerCapability::Git],
        };

        // Enum tag 0, version 1, string lens, generation 1, None proof, two capabilities.
        assert_bincode_bytes(
            &msg,
            &[
                0x00, // CoordinatorMessage::Hello
                0x01, // version
                0x09, b'i', b'n', b's', b't', b'a', b'l', b'l', b'-', b'a', // installation id
                0x24, // session namespace len 36
                b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'-', b'8', b'9', b'a', b'b',
                b'-', b'c', b'd', b'e', b'f', b'-', b'0', b'1', b'2', b'3', b'-', b'4', b'5',
                b'6', b'7', b'8', b'9', b'a', b'b', b'c', b'd', b'e', b'f', // uuid
                0x01, // host_binding_generation
                0x00, // auth_proof: None
                0x02, // capabilities len
                0x00, // Terminal
                0x03, // Git
            ],
        );
    }

    #[test]
    fn worker_hello_ack_framing_matches_golden_fixture() {
        let msg = WorkerMessage::HelloAck {
            version: PROTOCOL_VERSION,
            worker_instance_id: WorkerInstanceId::new("worker-1").unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            capabilities: vec![WorkerCapability::Terminal],
            auth_challenge: None,
            error: None,
        };
        let expected_payload = [
            0x00, // WorkerMessage::HelloAck
            0x01, // version
            0x08, b'w', b'o', b'r', b'k', b'e', b'r', b'-', b'1', // instance
            0x01, // host_binding_generation
            0x01, // capabilities len
            0x00, // Terminal
            0x00, // auth_challenge: None
            0x00, // error: None
        ];
        let mut expected = Vec::with_capacity(4 + expected_payload.len());
        expected.extend_from_slice(&(expected_payload.len() as u32).to_le_bytes());
        expected.extend_from_slice(&expected_payload);

        let mut buf = Vec::new();
        write_worker_message(&mut buf, &msg).unwrap();
        assert_eq!(buf.as_slice(), expected.as_slice());

        let decoded: WorkerMessage = read_worker_message(&mut expected.as_slice()).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn output_delta_bincode_bytes_are_stable() {
        let msg = WorkerMessage::OutputDelta {
            identity: runtime_identity(),
            location: location(),
            base_revision: OutputRevision::new(10),
            revision: OutputRevision::new(11),
            data: vec![b'o', b'k'],
        };
        assert_bincode_bytes(
            &msg,
            &[
                0x05, // WorkerMessage::OutputDelta (variant index 5)
                0x03, // host_binding_generation
                0x08, b'w', b'o', b'r', b'k', b'e', b'r', b'-', b'1', //
                0x04, b'r', b't', b'-', b'9', //
                0x07, // incarnation
                0x0b, b's', b's', b'h', b':', b'w', b'o', b'r', b'k', b'b', b'o', b'x', // host
                0x08, b'/', b's', b'r', b'v', b'/', b'a', b'p', b'i', // path
                0x0a, // base_revision 10
                0x0b, // revision 11
                0x02, b'o', b'k', // data
            ],
        );
    }

    #[test]
    fn max_frame_rejection_uses_public_framing_helpers() {
        let msg = WorkerMessage::OutputCheckpoint {
            identity: runtime_identity(),
            location: location(),
            revision: OutputRevision::new(1),
            data: vec![0x61; 64],
        };
        let mut buf = Vec::new();
        write_worker_message(&mut buf, &msg).unwrap();
        let payload_len = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
        assert!(payload_len > 0);
        assert!(payload_len <= MAX_FRAME_SIZE);

        match protocol::read_message::<_, WorkerMessage>(&mut buf.as_slice(), payload_len - 1) {
            Err(FramingError::Oversized { claimed, max }) => {
                assert_eq!(claimed, payload_len);
                assert_eq!(max, payload_len - 1);
            }
            other => panic!("expected oversized framing error, got {other:?}"),
        }

        // Declared length above the worker cap is rejected without allocating the payload.
        let mut oversized = Vec::new();
        let too_big = (MAX_FRAME_SIZE as u32).saturating_add(1).to_le_bytes();
        oversized.extend_from_slice(&too_big);
        match read_worker_message::<_, WorkerMessage>(&mut oversized.as_slice()) {
            Err(FramingError::Oversized { claimed, max }) => {
                assert_eq!(claimed, MAX_FRAME_SIZE + 1);
                assert_eq!(max, MAX_FRAME_SIZE);
            }
            other => panic!("expected worker max-frame rejection, got {other:?}"),
        }
    }

    #[test]
    fn runtime_identity_distinguishes_incarnation_and_instance() {
        let a = runtime_identity();
        let mut b = a.clone();
        b.incarnation = RuntimeIncarnation::new(8);
        let mut c = a.clone();
        c.worker_instance_id = WorkerInstanceId::new("worker-2").unwrap();
        let mut d = a.clone();
        d.host_binding_generation = HostBindingGeneration::new(4);
        let mut e = a.clone();
        e.runtime_id = WorkerRuntimeId::new("rt-10").unwrap();

        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_ne!(a, e);
        assert_eq!(a, runtime_identity());
    }

    #[test]
    fn handshake_validation_accepts_hello_pair_and_rejects_other_first_messages() {
        let hello = CoordinatorMessage::Hello {
            version: PROTOCOL_VERSION,
            coordinator_installation_id: CoordinatorInstallationId::new("install-a").unwrap(),
            session_namespace_id: SessionNamespaceId::new("ns-1").unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            auth_proof: None,
            capabilities: Vec::new(),
        };
        assert_eq!(validate_first_coordinator_message(&hello), Ok(()));

        let not_hello = CoordinatorMessage::Shutdown {
            request_id: RequestId::new(1),
        };
        assert_eq!(
            validate_first_coordinator_message(&not_hello),
            Err(HandshakeError::ExpectedHello)
        );

        let bad_version = CoordinatorMessage::Hello {
            version: 99,
            coordinator_installation_id: CoordinatorInstallationId::new("install-a").unwrap(),
            session_namespace_id: SessionNamespaceId::new("ns-1").unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            auth_proof: None,
            capabilities: Vec::new(),
        };
        assert_eq!(
            validate_first_coordinator_message(&bad_version),
            Err(HandshakeError::ProtocolMismatch {
                expected: PROTOCOL_VERSION,
                received: 99
            })
        );

        let ack = WorkerMessage::HelloAck {
            version: PROTOCOL_VERSION,
            worker_instance_id: WorkerInstanceId::new("worker-1").unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            capabilities: vec![WorkerCapability::Terminal],
            auth_challenge: None,
            error: None,
        };
        assert_eq!(validate_first_worker_message(&ack), Ok(()));

        let rejected = WorkerMessage::HelloAck {
            version: PROTOCOL_VERSION,
            worker_instance_id: WorkerInstanceId::new("worker-1").unwrap(),
            host_binding_generation: HostBindingGeneration::new(1),
            capabilities: Vec::new(),
            auth_challenge: None,
            error: Some(WorkerError::new(
                WorkerErrorCode::ProtocolMismatch,
                "wrong version",
            )),
        };
        assert!(matches!(
            validate_first_worker_message(&rejected),
            Err(HandshakeError::Rejected(_))
        ));

        let not_ack = WorkerMessage::RequestAck {
            request_id: RequestId::new(1),
            error: None,
        };
        assert_eq!(
            validate_first_worker_message(&not_ack),
            Err(HandshakeError::ExpectedHelloAck)
        );
    }

    #[test]
    fn string_ids_reject_ambiguous_values() {
        assert_eq!(
            CoordinatorInstallationId::new(""),
            Err(ProtocolIdError::Empty)
        );
        assert_eq!(
            SessionNamespaceId::new("bad id"),
            Err(ProtocolIdError::InvalidCharacter)
        );
        assert!(WorkerInstanceId::new("worker-1").is_ok());
        assert!(AuthProof::new("abc+/=_-").is_ok());
    }

    #[test]
    fn resource_location_is_required_on_terminal_ops() {
        let create = CoordinatorMessage::CreateTerminal {
            request_id: RequestId::new(1),
            location: location(),
            size: TerminalSize { cols: 80, rows: 24 },
            command: None,
        };
        match create {
            CoordinatorMessage::CreateTerminal { location, .. } => {
                assert_eq!(location.execution_host_id.as_str(), "ssh:workbox");
                assert_eq!(
                    location.path.as_path(),
                    std::path::Path::new("/srv/api")
                );
            }
            other => panic!("expected create terminal, got {other:?}"),
        }
    }
}
