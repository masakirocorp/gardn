//! Staged-file capability handlers.

use std::io;
use std::time::SystemTime;

use crate::execution_host::protocol::{RequestId, WorkerErrorCode, WorkerMessage};

use super::protocol_io::{write_ack, write_message};
use super::state::WorkerState;
use super::util::worker_error;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
pub(super) fn handle_stage_file(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    location: crate::execution_host::ResourceLocation,
    extension: String,
    data: Vec<u8>,
    ttl_secs: u32,
) -> io::Result<()> {
    let result = state.resolve_location(&location).and_then(|_| {
        state
            .staging_mut()
            .stage(&extension, &data, ttl_secs, SystemTime::now())
            .map_err(|error| worker_error(WorkerErrorCode::Failed, error.to_string()))
    });
    let (path, error) = result.map_or_else(|error| (None, Some(error)), |path| (Some(path), None));
    write_message(
        stream,
        WorkerMessage::StageFileResult {
            request_id,
            location,
            path,
            error,
        },
    )
}

#[cfg(unix)]
pub(super) fn handle_remove_staged_file(
    state: &mut WorkerState,
    stream: &mut UnixStream,
    request_id: RequestId,
    location: crate::execution_host::ResourceLocation,
) -> io::Result<()> {
    let error = if location.execution_host_id != state.binding().execution_host_id {
        Some(worker_error(
            WorkerErrorCode::BindingMismatch,
            "staged file belongs to another execution host",
        ))
    } else {
        state
            .staging_mut()
            .remove(&location.path)
            .map(|_| ())
            .map_err(|error| worker_error(WorkerErrorCode::Failed, error.to_string()))
            .err()
    };
    write_ack(stream, request_id, error)
}
