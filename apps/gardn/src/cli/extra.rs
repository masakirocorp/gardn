use std::io::{self, Write};

use serde::Serialize;

use crate::session::{self, SessionInfo};

#[derive(Debug, Serialize)]
struct ExtraListResponse {
    coordinators: Vec<ExtraCoordinatorInfo>,
}

#[derive(Debug, Serialize)]
struct ExtraCoordinatorInfo {
    id: String,
    kind: &'static str,
    name: String,
    session: String,
    running: bool,
    socket_path: String,
}

pub(super) fn run_extra_command(args: &[String]) -> io::Result<i32> {
    match args.first().map(String::as_str) {
        Some("list") => extra_list(&args[1..]),
        Some("connect") => extra_connect(&args[1..]),
        Some("help" | "--help" | "-h") | None => {
            print_extra_usage();
            Ok(0)
        }
        Some(other) => {
            eprintln!("unknown extra command: {other}");
            print_extra_usage();
            Ok(1)
        }
    }
}

fn extra_list(args: &[String]) -> io::Result<i32> {
    let json = args.iter().any(|arg| arg == "--json");
    if args.iter().any(|arg| arg != "--json") {
        eprintln!("usage: gardn extra list [--json]");
        return Ok(1);
    }
    let sessions = session::list_sessions()?;
    let coordinators = sessions.into_iter().map(local_coordinator).collect();
    if json {
        print_json(&ExtraListResponse { coordinators })?;
    } else {
        for coordinator in coordinators {
            let state = if coordinator.running {
                "running"
            } else {
                "stopped"
            };
            println!(
                "{}\t{}\t{}\t{}",
                coordinator.id, coordinator.name, state, coordinator.socket_path
            );
        }
    }
    Ok(0)
}

fn extra_connect(args: &[String]) -> io::Result<i32> {
    let mut remote = None;
    let mut session = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                json = true;
                index += 1;
            }
            "--remote" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!(
                        "usage: gardn extra connect --remote <target> [--session <name>] [--json]"
                    );
                    return Ok(1);
                };
                remote = Some(value.clone());
                index += 2;
            }
            "--session" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!(
                        "usage: gardn extra connect --remote <target> [--session <name>] [--json]"
                    );
                    return Ok(1);
                };
                session = Some(value.clone());
                index += 2;
            }
            other => {
                eprintln!("unknown extra connect argument: {other}");
                eprintln!(
                    "usage: gardn extra connect --remote <target> [--session <name>] [--json]"
                );
                return Ok(1);
            }
        }
    }
    let Some(target) = remote else {
        eprintln!("usage: gardn extra connect --remote <target> [--session <name>] [--json]");
        return Ok(1);
    };
    let session_name = session.unwrap_or_else(|| session::DEFAULT_SESSION_NAME.to_string());
    match crate::remote::run_extra_api_connect(&target, &session_name, json) {
        Ok(()) => Ok(0),
        Err(error) => {
            crate::remote::print_remote_error_hint(&error, &target);
            Err(error)
        }
    }
}

fn local_coordinator(session: SessionInfo) -> ExtraCoordinatorInfo {
    ExtraCoordinatorInfo {
        id: format!("local:{}", session.name),
        kind: "local",
        name: local_display_name(&session),
        session: session.name.clone(),
        running: session.running,
        socket_path: session.socket_path,
    }
}

fn local_display_name(session: &SessionInfo) -> String {
    if session.default {
        crate::platform::hostname()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("localhost"))
            .unwrap_or_else(|| "This Mac".to_string())
    } else {
        session.name.clone()
    }
}

fn print_json(value: &impl Serialize) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(io::Error::other)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

fn print_extra_usage() {
    eprintln!("gardn extra commands:");
    eprintln!("  gardn extra list [--json]");
    eprintln!("  gardn extra connect --remote <target> [--session <name>] [--json]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_list_rejects_unknown_flags() {
        assert_eq!(extra_list(&["--nope".into()]).unwrap(), 1);
    }
}
