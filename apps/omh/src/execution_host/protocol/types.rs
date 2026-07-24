//! Shared worker-protocol payload types.
//!
//! Capabilities, errors, command/agent specs, and observation snapshots used by
//! both coordinator and worker message enums.

use serde::{Deserialize, Serialize};

use super::identity::{validate_token, OutputRevision, ProtocolIdError};
use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};

pub(super) const MAX_ERROR_MESSAGE_LEN: usize = 1024;
pub(super) const MAX_COMMAND_LEN: usize = 4096;
pub(super) const MAX_AGENT_ID_LEN: usize = 128;

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
    FileStaging,
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
    TimedOut,
    OutputTooLarge,
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
    /// Resolved executable and arguments. The worker never resolves
    /// coordinator-local profile ids or plugin catalogs.
    pub(crate) command: CommandSpec,
}

impl AgentLaunch {
    pub(crate) fn validate(&self) -> Result<(), ProtocolIdError> {
        validate_token(&self.agent_id, MAX_AGENT_ID_LEN, &[])?;
        self.command.validate()?;
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
pub(crate) struct ObservedProcess {
    pub(crate) pid: u32,
    pub(crate) name: String,
    pub(crate) argv0: Option<String>,
    pub(crate) argv: Option<Vec<String>>,
    pub(crate) cmdline: Option<String>,
    pub(crate) cwd: Option<HostPath>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessObservation {
    pub(crate) pid: u32,
    pub(crate) ppid: Option<u32>,
    pub(crate) command: Option<String>,
    pub(crate) cwd: Option<HostPath>,
    pub(crate) foreground_process_group_id: Option<u32>,
    pub(crate) foreground_processes: Vec<ObservedProcess>,
    pub(crate) session_processes: Vec<ObservedProcess>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectCommandSource {
    BuiltIn,
    Vscode,
    PackageJson,
    Composer,
    Just,
    Make,
    Cargo,
    Go,
    Maven,
    Gradle,
    Dotnet,
    Python,
    Php,
    Ruby,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProjectCommandConfidence {
    Explicit,
    NativeDefault,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectCommandSnapshot {
    pub(crate) location: ResourceLocation,
    pub(crate) source: ProjectCommandSource,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) confidence: ProjectCommandConfidence,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortTransport {
    Tcp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortSnapshot {
    pub(crate) execution_host_id: ExecutionHostId,
    pub(crate) transport: PortTransport,
    pub(crate) bind_address: String,
    pub(crate) port: u16,
    pub(crate) pid: Option<u32>,
    pub(crate) command: Option<String>,
}
