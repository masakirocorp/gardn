//! Terminal capability handlers for the protocol dispatcher.

use std::collections::HashMap;
use std::io;
use std::sync::mpsc as std_mpsc;

use bytes::Bytes;

use crate::execution_host::protocol::{
    AgentLaunch, AttachResume, RequestId, RuntimeIdentity, RuntimeOpSeq, TerminalSize,
    TerminateMode, WorkerErrorCode, WorkerMessage, WorkerRuntimeId, WorkerSignal,
};
use crate::execution_host::ResourceLocation;

use super::host_job::{spawn_create_job, spawn_host_job, HostJobKind, HostJobResult};
use super::protocol_io::{write_ack, write_message};
use super::state::{validated_scrollback_limit, CreateKind, CreateRequest, WorkerState};
use super::terminal::{send_checkpoint, signal_runtime};
use super::util::worker_error;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
pub(super) fn handle_create_terminal(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    job_tx: &std_mpsc::Sender<HostJobResult>,
    request_id: RequestId,
    location: ResourceLocation,
    size: TerminalSize,
    command: Option<crate::execution_host::protocol::CommandSpec>,
    env: Vec<(String, String)>,
    scrollback_limit_bytes: u64,
) -> io::Result<()> {
    match validated_scrollback_limit(scrollback_limit_bytes) {
        Ok(scrollback_limit_bytes) => spawn_create_job(
            state,
            request_id,
            CreateRequest {
                kind: CreateKind::Terminal,
                location,
                size,
                command,
                env,
                scrollback_limit_bytes,
            },
            job_tx,
            stream,
        ),
        Err(error) => write_message(
            stream,
            WorkerMessage::CreateTerminalResult {
                request_id,
                identity: None,
                location,
                error: Some(error),
            },
        ),
    }
}

#[cfg(unix)]
pub(super) fn handle_start_agent(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    job_tx: &std_mpsc::Sender<HostJobResult>,
    request_id: RequestId,
    location: ResourceLocation,
    agent: AgentLaunch,
    size: TerminalSize,
    scrollback_limit_bytes: u64,
) -> io::Result<()> {
    let agent_id = agent.agent_id.clone();
    match agent
        .validate()
        .map_err(|err| worker_error(WorkerErrorCode::Failed, err.to_string()))
    {
        Ok(()) => match validated_scrollback_limit(scrollback_limit_bytes) {
            Ok(scrollback_limit_bytes) => spawn_create_job(
                state,
                request_id,
                CreateRequest {
                    kind: CreateKind::Agent(agent_id),
                    location,
                    size,
                    command: Some(agent.command),
                    env: Vec::new(),
                    scrollback_limit_bytes,
                },
                job_tx,
                stream,
            ),
            Err(error) => write_message(
                stream,
                WorkerMessage::CreateTerminalResult {
                    request_id,
                    identity: None,
                    location,
                    error: Some(error),
                },
            ),
        },
        Err(error) => write_message(
            stream,
            WorkerMessage::StartAgentResult {
                request_id,
                location,
                identity: None,
                error: Some(error),
            },
        ),
    }
}

#[cfg(unix)]
pub(super) fn handle_adopt_terminal(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
) -> io::Result<()> {
    let (adopted_identity, last_applied_op_seq, error) =
        match state.validate_runtime(&identity, &location) {
            Ok(record) => (
                Some(record.identity.clone()),
                RuntimeOpSeq::new(record.last_op_seq),
                None,
            ),
            Err(error) => (None, RuntimeOpSeq::new(0), Some(error)),
        };
    write_message(
        stream,
        WorkerMessage::AdoptTerminalResult {
            request_id,
            identity: adopted_identity,
            location,
            last_applied_op_seq,
            error,
        },
    )
}

#[cfg(unix)]
pub(super) fn handle_attach_terminal(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    sent_revisions: &mut HashMap<WorkerRuntimeId, u64>,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
    resume: AttachResume,
) -> io::Result<()> {
    let result = state.validate_runtime(&identity, &location);
    let error = result.as_ref().err().cloned();
    write_message(
        stream,
        WorkerMessage::AttachTerminalResult {
            request_id,
            identity: identity.clone(),
            location: location.clone(),
            resume,
            error,
        },
    )?;
    if let Ok(record) = result {
        match resume {
            AttachResume::AfterRevision(revision) => {
                if record.output.deltas_after(revision.get()).is_some() {
                    sent_revisions.insert(identity.runtime_id.clone(), revision.get());
                } else {
                    let checkpoint_revision = send_checkpoint(stream, record)?;
                    sent_revisions.insert(identity.runtime_id.clone(), checkpoint_revision.get());
                }
            }
            AttachResume::Checkpoint => {
                let checkpoint_revision = send_checkpoint(stream, record)?;
                sent_revisions.insert(identity.runtime_id.clone(), checkpoint_revision.get());
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn handle_input(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
    op_seq: RuntimeOpSeq,
    data: Vec<u8>,
) -> io::Result<()> {
    let result = state.apply_runtime_op(&identity, &location, op_seq, |runtime| {
        runtime.try_send_bytes(Bytes::from(data)).map_err(|err| {
            worker_error(
                WorkerErrorCode::Busy,
                format!("terminal input queue is unavailable: {err}"),
            )
        })
    });
    write_ack(stream, request_id, result.err())
}

#[cfg(unix)]
pub(super) fn handle_resize(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
    op_seq: RuntimeOpSeq,
    size: TerminalSize,
) -> io::Result<()> {
    let result = state.apply_runtime_op(&identity, &location, op_seq, |runtime| {
        runtime.resize(size.rows, size.cols, 0, 0);
        Ok(())
    });
    write_ack(stream, request_id, result.err())
}

#[cfg(unix)]
pub(super) fn handle_signal(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
    op_seq: RuntimeOpSeq,
    signal: WorkerSignal,
) -> io::Result<()> {
    let result = state.apply_runtime_op(&identity, &location, op_seq, |runtime| {
        signal_runtime(runtime.child_pid(), signal)
    });
    write_ack(stream, request_id, result.err())
}

#[cfg(unix)]
pub(super) fn handle_terminate(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    sent_revisions: &mut HashMap<WorkerRuntimeId, u64>,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
    mode: TerminateMode,
) -> io::Result<()> {
    let runtime_id = identity.runtime_id.clone();
    let result = state.terminate_runtime(identity, location, mode);
    if result.is_ok() {
        sent_revisions.remove(&runtime_id);
    }
    write_ack(stream, request_id, result.err())
}

#[cfg(unix)]
pub(super) fn handle_observe_process(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
) -> io::Result<()> {
    let result = state.validate_runtime(&identity, &location);
    let process = match &result {
        Ok(record) => state
            .child_pid_for(&identity, &location)
            .ok()
            .map(|pid| super::host_job::observe_runtime_process(pid, &record.location)),
        Err(_) => None,
    };
    write_message(
        stream,
        WorkerMessage::ProcessObservationResult {
            request_id,
            identity,
            location,
            process,
            error: result.err(),
        },
    )
}

#[cfg(unix)]
pub(super) fn handle_ack_output(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    identity: RuntimeIdentity,
    location: ResourceLocation,
    revision: crate::execution_host::protocol::OutputRevision,
) -> io::Result<()> {
    let error = state
        .validate_runtime(&identity, &location)
        .and_then(|record| {
            if revision > record.output.revision() {
                Err(worker_error(
                    WorkerErrorCode::Conflict,
                    "output acknowledgement is ahead of worker output",
                ))
            } else {
                Ok(())
            }
        })
        .err();
    write_ack(stream, request_id, error)
}

#[cfg(unix)]
pub(super) fn handle_query_host_health(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    location: ResourceLocation,
) -> io::Result<()> {
    let error = state.validate_location(&location).err();
    write_message(
        stream,
        WorkerMessage::HostHealth {
            request_id: Some(request_id),
            location,
            status: if error.is_none() {
                crate::execution_host::protocol::HostHealthStatus::Ready
            } else {
                crate::execution_host::protocol::HostHealthStatus::Unavailable
            },
            detail: error.map(|err| err.message),
        },
    )
}

#[cfg(unix)]
pub(super) fn handle_shutdown(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
) -> io::Result<bool> {
    if state.is_idle_for_shutdown() {
        write_ack(stream, request_id, None)?;
        return Ok(true);
    }
    write_ack(
        stream,
        request_id,
        Some(worker_error(
            WorkerErrorCode::Busy,
            format!(
                "execution worker still owns {} runtime(s)",
                state.owned_runtime_count()
            ),
        )),
    )?;
    Ok(false)
}

#[cfg(unix)]
pub(super) fn spawn_observation_job(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    job_tx: &std_mpsc::Sender<HostJobResult>,
    request_id: RequestId,
    location: ResourceLocation,
    kind: HostJobKind,
    command: Option<crate::execution_host::protocol::CommandSpec>,
) -> io::Result<()> {
    spawn_host_job(state, request_id, location, kind, command, job_tx, stream)
}
