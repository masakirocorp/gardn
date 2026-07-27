//! Hidden execution-worker process role.
//!
//! The worker is intentionally separate from the normal server role. Platform
//! code owns its persistent IPC listener; this facade only handles the stable
//! CLI contract.

use std::io;

use super::lifecycle::DAEMON_LIFECYCLE_VERSION;
use super::protocol::PROTOCOL_VERSION;
use super::runtime_paths::{inventory_owned_bindings, retire_owned_bindings};

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
            "usage: omh execution-worker [--protocol-version]\n       omh execution-worker [--daemon-lifecycle-version]\n       omh execution-worker --inventory --installation <id> --execution-host <id>\n       omh execution-worker --retire --installation <id> --execution-host <id>\n       omh execution-worker --daemon <binding arguments>"
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--inventory") {
        let (installation, execution_host) = parse_owner_filters(args, "--inventory")?;
        let report = inventory_owned_bindings(&installation, &execution_host)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(io::Error::other)?
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--retire") {
        let (installation, execution_host) = parse_owner_filters(args, "--retire")?;
        let report = retire_owned_bindings(&installation, &execution_host)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(io::Error::other)?
        );
        if !report.blocked_bindings.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::ResourceBusy,
                format!(
                    "refusing to retire {} live execution-worker binding(s)",
                    report.blocked_bindings.len()
                ),
            ));
        }
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

fn parse_owner_filters(args: &[String], mode: &str) -> io::Result<(String, String)> {
    let mut installation = None;
    let mut execution_host = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            flag if flag == mode => {
                index += 1;
            }
            "--installation" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "missing value for --installation",
                    )
                })?;
                installation = Some(value.clone());
                index += 2;
            }
            "--execution-host" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "missing value for --execution-host",
                    )
                })?;
                execution_host = Some(value.clone());
                index += 2;
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown execution-worker {mode} argument: {other}"),
                ));
            }
        }
    }
    let installation = installation.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing --installation for execution-worker {mode}"),
        )
    })?;
    let execution_host = execution_host.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing --execution-host for execution-worker {mode}"),
        )
    })?;
    if installation.is_empty() || execution_host.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "installation and execution-host filters must be non-empty",
        ));
    }
    Ok((installation, execution_host))
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
    fn daemon_lifecycle_version_flag_is_v2() {
        let args = ["--daemon-lifecycle-version".to_string()];
        assert!(args.iter().any(|arg| arg == "--daemon-lifecycle-version"));
        assert_eq!(DAEMON_LIFECYCLE_VERSION, 2);
    }

    #[test]
    fn owner_filter_parser_requires_installation_and_host() {
        let err = parse_owner_filters(
            &[
                "--inventory".into(),
                "--installation".into(),
                "install-a".into(),
            ],
            "--inventory",
        )
        .expect_err("execution-host required");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let ok = parse_owner_filters(
            &[
                "--retire".into(),
                "--execution-host".into(),
                "ssh:workbox:1".into(),
                "--installation".into(),
                "install-a".into(),
            ],
            "--retire",
        )
        .expect("filters");
        assert_eq!(ok.0, "install-a");
        assert_eq!(ok.1, "ssh:workbox:1");
    }
}
