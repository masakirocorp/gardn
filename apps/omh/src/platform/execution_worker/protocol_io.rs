//! Wire helpers for worker protocol frames.

use std::io;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

use crate::execution_host::protocol::{
    write_worker_message, RequestId, WorkerError, WorkerErrorCode, WorkerMessage,
};

use super::util::{framing_io, worker_error};

#[cfg(unix)]
pub(super) fn write_ack(
    stream: &mut UnixStream,
    request_id: RequestId,
    error: Option<WorkerError>,
) -> io::Result<()> {
    write_message(stream, WorkerMessage::RequestAck { request_id, error })
}

#[cfg(unix)]
pub(super) fn write_message(stream: &mut UnixStream, message: WorkerMessage) -> io::Result<()> {
    match write_worker_message(stream, &message) {
        Ok(()) => Ok(()),
        Err(crate::protocol::FramingError::Oversized { claimed, max }) => {
            if let Some(fallback) = oversized_worker_fallback(&message, claimed, max) {
                write_worker_message(stream, &fallback).map_err(framing_io)
            } else {
                Err(framing_io(crate::protocol::FramingError::Oversized {
                    claimed,
                    max,
                }))
            }
        }
        Err(error) => Err(framing_io(error)),
    }
}

#[cfg(unix)]
pub(super) fn oversized_worker_fallback(
    message: &WorkerMessage,
    claimed: usize,
    max: usize,
) -> Option<WorkerMessage> {
    let error = worker_error(
        WorkerErrorCode::OutputTooLarge,
        format!("worker response frame {claimed} exceeds maximum {max}"),
    );
    match message {
        WorkerMessage::GitStatusResult {
            request_id,
            location,
            ..
        } => Some(WorkerMessage::GitStatusResult {
            request_id: *request_id,
            location: location.clone(),
            status: None,
            error: Some(error),
        }),
        WorkerMessage::WorktreeListResult {
            request_id,
            location,
            ..
        } => Some(WorkerMessage::WorktreeListResult {
            request_id: *request_id,
            location: location.clone(),
            worktrees: Vec::new(),
            error: Some(error),
        }),
        WorkerMessage::CommandResult {
            request_id,
            location,
            ..
        } => Some(WorkerMessage::CommandResult {
            request_id: *request_id,
            location: location.clone(),
            exit: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            error: Some(error),
        }),
        WorkerMessage::RequestAck { request_id, .. } => Some(WorkerMessage::RequestAck {
            request_id: *request_id,
            error: Some(error),
        }),
        WorkerMessage::Error { request_id, .. } => Some(WorkerMessage::Error {
            request_id: *request_id,
            error,
        }),
        _ => None,
    }
}
