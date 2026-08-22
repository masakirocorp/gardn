//! Terminal runtime I/O: output flush, state events, checkpoints, signals.

use std::collections::HashMap;
use std::io;

use crate::execution_host::protocol::{
    OutputRevision, RuntimeExitStatus, WorkerError, WorkerErrorCode, WorkerMessage,
    WorkerRuntimeId, WorkerSignal,
};

use super::event::{RuntimeLocalId, WorkerEvent};
use super::protocol_io::write_message;
use super::state::{RuntimeRecord, WorkerState};
use super::util::worker_error;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
pub(super) fn flush_output(
    state: &WorkerState,
    stream: &mut UnixStream,
    sent: &mut HashMap<WorkerRuntimeId, u64>,
) -> io::Result<()> {
    let attached = sent.keys().cloned().collect::<Vec<_>>();
    for runtime_id in attached {
        let Some(record) = state.runtime_record(&runtime_id) else {
            sent.remove(&runtime_id);
            continue;
        };
        let previous = sent.get(&runtime_id).copied().unwrap_or(0);
        let Some(deltas) = record.output.deltas_after(previous) else {
            let checkpoint_revision = send_checkpoint(stream, record)?;
            sent.insert(runtime_id, checkpoint_revision.get());
            continue;
        };
        for (base, revision, data) in deltas {
            write_message(
                stream,
                WorkerMessage::OutputDelta {
                    identity: record.identity.clone(),
                    location: record.location.clone(),
                    base_revision: OutputRevision::new(base),
                    revision: OutputRevision::new(revision),
                    data,
                },
            )?;
            sent.insert(runtime_id.clone(), revision);
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn flush_state_events(
    state: &mut WorkerState,
    stream: &mut UnixStream,
) -> io::Result<()> {
    while let Some(queued) = state.next_hook_report() {
        write_message(
            stream,
            WorkerMessage::AgentHookReported {
                identity: queued.identity.clone(),
                report: queued.report.clone(),
            },
        )?;
        state.confirm_hook_report(&queued);
    }
    while let Ok(event) = state.try_recv_event() {
        match event {
            WorkerEvent::StateChanged {
                local_id,
                agent,
                state: agent_state,
                visible_blocker,
                visible_idle,
                visible_working,
                process_exited,
                ..
            } => {
                let Some(record) = state.runtime_record_by_local_id(local_id) else {
                    continue;
                };
                write_message(
                    stream,
                    WorkerMessage::TerminalStateChanged {
                        identity: record.identity.clone(),
                        location: record.location.clone(),
                        agent,
                        state: agent_state,
                        visible_blocker,
                        visible_idle,
                        visible_working,
                        process_exited,
                    },
                )?;
            }
            WorkerEvent::RuntimeExit {
                local_id,
                exit_code,
                exit_signal,
            } => {
                emit_runtime_exit(state, stream, local_id, exit_code, exit_signal)?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn emit_runtime_exit(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    local_id: RuntimeLocalId,
    exit_code: Option<i32>,
    exit_signal: Option<i32>,
) -> io::Result<()> {
    let Some(record) = state.take_exited_runtime(local_id) else {
        return Ok(());
    };
    let status = match (exit_signal, exit_code) {
        (Some(signal), _) => RuntimeExitStatus::Signal(signal),
        (_, Some(code)) => RuntimeExitStatus::Code(code),
        _ => RuntimeExitStatus::Code(1),
    };
    state.record_termination(record.identity.clone(), record.location.clone());
    write_message(
        stream,
        WorkerMessage::RuntimeExit {
            identity: record.identity,
            location: record.location,
            status,
        },
    )
}

#[cfg(unix)]
pub(super) fn send_checkpoint(
    stream: &mut UnixStream,
    record: &RuntimeRecord,
) -> io::Result<OutputRevision> {
    let (revision, data) = record.output.checkpoint();
    write_message(
        stream,
        WorkerMessage::OutputCheckpoint {
            identity: record.identity.clone(),
            location: record.location.clone(),
            revision,
            data,
        },
    )?;
    Ok(revision)
}

#[cfg(unix)]
pub(super) fn signal_runtime(pid: u32, signal: WorkerSignal) -> Result<(), WorkerError> {
    let signal = match signal {
        WorkerSignal::Hangup => libc::SIGHUP,
        WorkerSignal::Interrupt => libc::SIGINT,
        WorkerSignal::Terminate => libc::SIGTERM,
        WorkerSignal::Kill => libc::SIGKILL,
        WorkerSignal::Winch => libc::SIGWINCH,
    };
    let result = unsafe { libc::kill(pid as libc::pid_t, signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(worker_error(
            WorkerErrorCode::Failed,
            format!(
                "failed to signal terminal process: {}",
                io::Error::last_os_error()
            ),
        ))
    }
}
