//! Shallow protocol dispatcher that delegates by capability.

use std::collections::HashMap;
use std::io;
use std::sync::mpsc as std_mpsc;

use crate::execution_host::protocol::{
    CoordinatorMessage, WorkerErrorCode, WorkerMessage, WorkerRuntimeId,
};

use super::host_job::{HostJobKind, HostJobResult};
use super::protocol_io::write_message;
use super::staging::{handle_remove_staged_file, handle_stage_file};
use super::state::WorkerState;
use super::terminal_ops::{
    handle_ack_output, handle_adopt_terminal, handle_attach_terminal, handle_create_terminal,
    handle_input, handle_observe_process, handle_query_host_health, handle_resize, handle_shutdown,
    handle_signal, handle_start_agent, handle_terminate, spawn_observation_job,
};
use super::util::worker_error;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

/// Dispatch one coordinator request. Returns `true` when the connection should shut down.
#[cfg(unix)]
pub(super) fn handle_request(
    message: CoordinatorMessage,
    state: &mut WorkerState,
    stream: &mut UnixStream,
    sent_revisions: &mut HashMap<WorkerRuntimeId, u64>,
    job_tx: &std_mpsc::Sender<HostJobResult>,
) -> io::Result<bool> {
    match message {
        CoordinatorMessage::Hello { .. } => write_message(
            stream,
            WorkerMessage::Error {
                request_id: None,
                error: worker_error(
                    WorkerErrorCode::InvalidHandshake,
                    "Hello is only valid as the first frame",
                ),
            },
        )?,
        CoordinatorMessage::AuthProof { request_id, .. } => write_message(
            stream,
            WorkerMessage::AuthResult {
                request_id,
                accepted: false,
                error: Some(worker_error(
                    WorkerErrorCode::UnsupportedCapability,
                    "the OpenSSH-authenticated worker does not use protocol auth proofs",
                )),
            },
        )?,
        CoordinatorMessage::CreateTerminal {
            request_id,
            location,
            size,
            command,
            env,
            scrollback_limit_bytes,
        } => handle_create_terminal(
            state,
            stream,
            job_tx,
            request_id,
            location,
            size,
            command,
            env,
            scrollback_limit_bytes,
        )?,
        CoordinatorMessage::AdoptTerminal {
            request_id,
            identity,
            location,
        } => handle_adopt_terminal(state, stream, request_id, identity, location)?,
        CoordinatorMessage::AttachTerminal {
            request_id,
            identity,
            location,
            resume,
        } => handle_attach_terminal(
            state,
            stream,
            sent_revisions,
            request_id,
            identity,
            location,
            resume,
        )?,
        CoordinatorMessage::Input {
            request_id,
            identity,
            location,
            op_seq,
            data,
        } => handle_input(state, stream, request_id, identity, location, op_seq, data)?,
        CoordinatorMessage::Resize {
            request_id,
            identity,
            location,
            op_seq,
            size,
        } => handle_resize(state, stream, request_id, identity, location, op_seq, size)?,
        CoordinatorMessage::Signal {
            request_id,
            identity,
            location,
            op_seq,
            signal,
        } => handle_signal(
            state, stream, request_id, identity, location, op_seq, signal,
        )?,
        CoordinatorMessage::Terminate {
            request_id,
            identity,
            location,
            mode,
        } => handle_terminate(
            state,
            stream,
            sent_revisions,
            request_id,
            identity,
            location,
            mode,
        )?,
        CoordinatorMessage::ObserveProcess {
            request_id,
            identity,
            location,
        } => handle_observe_process(state, stream, request_id, identity, location)?,
        CoordinatorMessage::CompletePath {
            request_id,
            location,
            prefix,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::CompletePath { prefix },
            None,
        )?,
        CoordinatorMessage::ValidatePath {
            request_id,
            location,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::ValidatePath,
            None,
        )?,
        CoordinatorMessage::GitStatus {
            request_id,
            location,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::GitStatus,
            None,
        )?,
        CoordinatorMessage::ListWorktrees {
            request_id,
            location,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::ListWorktrees,
            None,
        )?,
        CoordinatorMessage::RunCommand {
            request_id,
            location,
            command,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::RunCommand,
            Some(command),
        )?,
        CoordinatorMessage::StartAgent {
            request_id,
            location,
            agent,
            size,
            scrollback_limit_bytes,
        } => handle_start_agent(
            state,
            stream,
            job_tx,
            request_id,
            location,
            agent,
            size,
            scrollback_limit_bytes,
        )?,
        CoordinatorMessage::ObservePorts {
            request_id,
            location,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::ObservePorts,
            None,
        )?,
        CoordinatorMessage::DiscoverProjectCommands {
            request_id,
            location,
        } => spawn_observation_job(
            state,
            stream,
            job_tx,
            request_id,
            location,
            HostJobKind::DiscoverProjectCommands,
            None,
        )?,
        CoordinatorMessage::QueryHostHealth {
            request_id,
            location,
        } => handle_query_host_health(state, stream, request_id, location)?,
        CoordinatorMessage::AckOutput {
            request_id,
            identity,
            location,
            revision,
        } => handle_ack_output(state, stream, request_id, identity, location, revision)?,
        CoordinatorMessage::StageFile {
            request_id,
            location,
            extension,
            data,
            ttl_secs,
        } => handle_stage_file(
            state, stream, request_id, location, extension, data, ttl_secs,
        )?,
        CoordinatorMessage::RemoveStagedFile {
            request_id,
            location,
        } => handle_remove_staged_file(state, stream, request_id, location)?,
        CoordinatorMessage::Shutdown { request_id } => {
            return handle_shutdown(state, stream, request_id);
        }
    }
    Ok(false)
}
