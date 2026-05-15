//! Platform-specific process and filesystem operations.
//!
//! Centralizes OS-dependent behavior behind a clean boundary so core
//! modules don't scatter `#[cfg]` branches through product logic.

use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundProcess {
    pub pid: u32,
    pub name: String,
    pub argv0: Option<String>,
    pub cmdline: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundJob {
    pub process_group_id: u32,
    pub processes: Vec<ForegroundProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcpListenerInfo {
    pub bind_addr: IpAddr,
    pub port: u16,
    pub pid: u32,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Hangup,
    Terminate,
    Kill,
}

fn active_tcp_listeners_from_lsof() -> Vec<TcpListenerInfo> {
    let output = match std::process::Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    parse_lsof_tcp_listeners(&text)
}

fn parse_lsof_tcp_listeners(output: &str) -> Vec<TcpListenerInfo> {
    let mut listeners = Vec::new();
    let mut pid = None;
    let mut command = None;

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }
        let (field, value) = line.split_at(1);
        match field {
            "p" => {
                pid = value.parse::<u32>().ok();
                command = None;
            }
            "c" => command = (!value.is_empty()).then(|| value.to_string()),
            "n" => {
                let Some(pid) = pid else {
                    continue;
                };
                let Some((bind_addr, port)) = parse_lsof_listener_name(value) else {
                    continue;
                };
                listeners.push(TcpListenerInfo {
                    bind_addr,
                    port,
                    pid,
                    command: command.clone(),
                });
            }
            _ => {}
        }
    }

    listeners
}

fn parse_lsof_listener_name(name: &str) -> Option<(IpAddr, u16)> {
    let name = name
        .trim()
        .strip_prefix("TCP ")
        .unwrap_or(name.trim())
        .split(" (")
        .next()?
        .trim();
    if name.contains("->") {
        return None;
    }

    let (host, port) = if let Some(rest) = name.strip_prefix('[') {
        let end = rest.find(']')?;
        let host = &rest[..end];
        let port = rest[end + 1..].strip_prefix(':')?;
        (host, port)
    } else {
        name.rsplit_once(':')?
    };

    let bind_addr = match host {
        "*" => "0.0.0.0".parse().ok()?,
        "" => return None,
        value => value.parse().ok()?,
    };
    let port = port.parse().ok()?;
    Some((bind_addr, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsof_listener_parser_reads_ipv4_ipv6_and_commands() {
        let listeners = parse_lsof_tcp_listeners(
            "p10\ncnode\nn127.0.0.1:5173\np20\ncwrangler\nn[::1]:8787 (LISTEN)\n",
        );

        assert_eq!(listeners.len(), 2);
        assert_eq!(listeners[0].pid, 10);
        assert_eq!(listeners[0].command.as_deref(), Some("node"));
        assert_eq!(
            listeners[0].bind_addr,
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
        assert_eq!(listeners[0].port, 5173);
        assert_eq!(listeners[1].pid, 20);
        assert_eq!(listeners[1].bind_addr, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(listeners[1].port, 8787);
    }

    #[test]
    fn lsof_listener_parser_maps_wildcard_to_unspecified_address() {
        let listeners = parse_lsof_tcp_listeners("p1\ncserver\nn*:3000\n");

        assert_eq!(listeners.len(), 1);
        assert_eq!(listeners[0].bind_addr, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(listeners[0].port, 3000);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCommand {
    pub program: &'static str,
    pub args: &'static [&'static str],
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod fallback;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use fallback::*;
