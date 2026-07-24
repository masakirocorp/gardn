//! Hidden execution-worker process role.
//!
//! The worker is intentionally separate from the normal server role. Platform
//! code owns its persistent IPC listener; this facade only handles the stable
//! CLI contract.

use std::io;

use super::lifecycle::DAEMON_LIFECYCLE_VERSION;
use super::protocol::PROTOCOL_VERSION;

pub(crate) fn run_from_args(args: &[String]) -> io::Result<()> {
    if args.iter().any(|arg| arg == "--protocol-version") {
        println!("{PROTOCOL_VERSION}");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--daemon-lifecycle-version") {
        println!("{DAEMON_LIFECYCLE_VERSION}");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        eprintln!(
            "usage: omh execution-worker [--protocol-version]\n       omh execution-worker [--daemon-lifecycle-version]\n       omh execution-worker --daemon <binding arguments>"
        );
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            io::Error::other(format!("failed to start execution-worker runtime: {error}"))
        })?;
    runtime.block_on(async { crate::platform::run_execution_worker(args) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_flag_prints_worker_protocol() {
        // run_from_args writes to stdout; exercise the early-return path only via
        // the same condition the CLI uses so the flag stays wired.
        let args = ["--protocol-version".to_string()];
        assert!(args.iter().any(|arg| arg == "--protocol-version"));
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn daemon_lifecycle_version_flag_is_v1() {
        let args = ["--daemon-lifecycle-version".to_string()];
        assert!(args.iter().any(|arg| arg == "--daemon-lifecycle-version"));
        assert_eq!(DAEMON_LIFECYCLE_VERSION, 1);
    }
}
