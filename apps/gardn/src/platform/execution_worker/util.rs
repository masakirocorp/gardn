use std::io;
use std::time::Duration;

use crate::execution_host::protocol::{WorkerError, WorkerErrorCode};

pub(super) const READY_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const READY_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub(super) const LIFECYCLE_ACTIVATE_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const OCCUPIED_CLIENT_IO_TIMEOUT: Duration = Duration::from_millis(100);
pub(super) const MAX_OCCUPIED_CLIENTS_PER_POLL: usize = 4;
pub(super) const LOCK_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(20);
pub(super) const SESSION_POLL_INTERVAL: Duration = Duration::from_millis(20);
pub(super) const WORKER_APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub(super) const DEFAULT_WORKER_SCROLLBACK_BYTES: usize = 2 * 1024 * 1024;
pub(super) const COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub(super) const GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
pub(super) const LSOF_OUTPUT_BYTES: usize = 1024 * 1024;
pub(super) const HOST_JOB_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const MAX_HOST_JOBS: usize = 32;
pub(super) const MAX_PATH_COMPLETION_ENTRIES: usize = 256;
pub(super) const MAX_COMMAND_MANIFEST_BYTES: usize = 1024 * 1024;
pub(super) const MAX_COMMAND_DIRECTORY_ENTRIES: usize = 4096;
pub(super) const MAX_CREATE_REPLAYS: usize = 4096;
pub(super) const MAX_TERMINATION_TOMBSTONES: usize = 4096;

pub(super) fn worker_error(code: WorkerErrorCode, message: impl Into<String>) -> WorkerError {
    WorkerError::new(code, message)
}

pub(super) fn framing_io(error: crate::protocol::FramingError) -> io::Error {
    match error {
        crate::protocol::FramingError::UnexpectedEof => {
            io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected end of stream")
        }
        crate::protocol::FramingError::Io(error) => error,
        error => io::Error::new(io::ErrorKind::InvalidData, error.to_string()),
    }
}

pub(super) fn is_disconnect(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
    )
}

#[cfg(unix)]
pub(super) fn required<T>(value: Option<T>, name: &str) -> io::Result<T> {
    value.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("missing {name}")))
}
