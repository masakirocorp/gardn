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
mod hook_ingress;
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

use crate::execution_host::runtime_paths::{inventory_owned_bindings, retire_owned_bindings};

/// Entry point for `omh execution-worker` (bridge) and `--daemon` mode.
pub(crate) fn run(args: &[String]) -> io::Result<()> {
    if args.first().map(String::as_str) == Some("--daemon") {
        return lifecycle::run_daemon(binding::DaemonBinding::parse(&args[1..])?);
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
    if !args.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown execution-worker argument: {}", args[0]),
        ));
    }
    lifecycle::run_bridge_stdio()
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
