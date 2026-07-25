//! Persistent execution worker: host runtime daemon and SSH bridge.
//!
//! Module boundaries:
//! - [`lifecycle`] — bridge activation, daemon lock/bind, connection accept
//! - [`state`] — worker-owned runtimes, replays, staging handle, host job table
//! - [`terminal`] / [`terminal_ops`] — PTY I/O, signals, terminal protocol handlers
//! - [`host_job`] — observation/command/path jobs off the PTY loop
//! - [`staging`] — staged file capability
//! - [`dispatch`] — shallow protocol match that delegates by capability
//! - [`event`] — worker-native runtime events (no `AppEvent` in worker core)

mod binding;
mod dispatch;
mod event;
mod host_job;
mod host_job_ops;
mod lifecycle;
mod output;
mod protocol_io;
mod staging;
mod state;
mod state_tables;
mod terminal;
mod terminal_ops;
mod util;

#[cfg(all(test, unix))]
mod tests;

use std::io;

/// Entry point for `omh execution-worker` (bridge) and `--daemon` mode.
pub(crate) fn run(args: &[String]) -> io::Result<()> {
    if args.first().map(String::as_str) == Some("--daemon") {
        return lifecycle::run_daemon(binding::DaemonBinding::parse(&args[1..])?);
    }
    if !args.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown execution-worker argument: {}", args[0]),
        ));
    }
    lifecycle::run_bridge_stdio()
}
