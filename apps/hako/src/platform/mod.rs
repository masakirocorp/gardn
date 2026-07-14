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
    pub argv: Option<Vec<String>>,
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn detach_server_daemon_command(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn detach_server_daemon_command(_command: &mut std::process::Command) {}

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
mod tcp_listener_tests {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LimitedRead {
    Empty,
    Complete(Vec<u8>),
    Oversized,
}

pub(crate) fn read_limited_reader(
    mut reader: impl std::io::Read,
    max_bytes: usize,
) -> std::io::Result<LimitedRead> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];

    while bytes.len() < max_bytes {
        let remaining = max_bytes - bytes.len();
        let read_len = remaining.min(buffer.len());
        let bytes_read = match reader.read(&mut buffer[..read_len]) {
            Ok(bytes_read) => bytes_read,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if bytes_read == 0 {
            return if bytes.is_empty() {
                Ok(LimitedRead::Empty)
            } else {
                Ok(LimitedRead::Complete(bytes))
            };
        }
        bytes.extend_from_slice(&buffer[..bytes_read]);
    }

    let mut sentinel = [0_u8; 1];
    loop {
        return match reader.read(&mut sentinel) {
            Ok(0) if bytes.is_empty() => Ok(LimitedRead::Empty),
            Ok(0) => Ok(LimitedRead::Complete(bytes)),
            Ok(_) => Ok(LimitedRead::Oversized),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => Err(err),
        };
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod fallback;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub use fallback::*;

#[cfg(not(target_os = "linux"))]
pub fn process_agent_hint(_pid: u32) -> Option<crate::detect::Agent> {
    None
}

/// Whether the platform should draw Hako's cursor into frame cells by default.
pub(crate) fn should_draw_host_cursor_by_default() -> bool {
    should_draw_host_cursor_by_default_platform()
}

pub(crate) fn scrollback_editor_argv(path: &std::path::Path) -> std::io::Result<Vec<String>> {
    scrollback_editor_argv_platform(path)
}

pub(crate) fn detached_custom_command_process(command: &str) -> std::process::Command {
    detached_custom_command_process_platform(command)
}

pub(crate) fn pane_custom_command_pty_builder(command: &str) -> portable_pty::CommandBuilder {
    pane_custom_command_pty_builder_platform(command)
}
#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct InputSourceRestore;

#[cfg(not(target_os = "macos"))]
pub(crate) fn switch_to_ascii_input_source() -> Option<InputSourceRestore> {
    None
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn pump_input_source_runloop() {}

/// Switches the host keyboard input source while prefix mode is active.
///
/// `App` drives this through a trait so prefix-mode transitions can be tested
/// with a fake without touching the real macOS APIs.
pub(crate) trait PrefixInputSource {
    /// Switch to an ASCII-capable input source for prefix commands. No-op if
    /// the current source is already ASCII-capable, the platform is
    /// unsupported, or the switch fails. Calling it again before `restore`
    /// keeps the source saved by the first call.
    fn switch_to_ascii(&mut self);

    /// Restore whatever `switch_to_ascii` saved. No-op if nothing was switched.
    fn restore(&mut self);
}

/// Production [`PrefixInputSource`] backed by the per-platform API.
#[derive(Default)]
pub(crate) struct RealPrefixInputSource {
    restore: Option<InputSourceRestore>,
}

impl PrefixInputSource for RealPrefixInputSource {
    fn switch_to_ascii(&mut self) {
        if self.restore.is_none() {
            // Drain input-source change notifications before Carbon's current
            // source query; this is a no-op outside macOS.
            pump_input_source_runloop();
            self.restore = switch_to_ascii_input_source();
        }
    }

    fn restore(&mut self) {
        let _ = self.restore.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_limited_reader_returns_complete_data_under_limit() {
        let input = std::io::Cursor::new(b"image".to_vec());
        assert_eq!(
            read_limited_reader(input, 16).expect("limited read"),
            LimitedRead::Complete(b"image".to_vec())
        );
    }

    #[test]
    fn read_limited_reader_returns_empty_for_empty_input() {
        let input = std::io::Cursor::new(Vec::<u8>::new());
        assert_eq!(
            read_limited_reader(input, 16).expect("limited read"),
            LimitedRead::Empty
        );
    }

    #[test]
    fn read_limited_reader_accepts_data_exactly_at_limit() {
        let input = std::io::Cursor::new(b"four".to_vec());
        assert_eq!(
            read_limited_reader(input, 4).expect("limited read"),
            LimitedRead::Complete(b"four".to_vec())
        );
    }

    #[test]
    fn read_limited_reader_rejects_data_over_limit() {
        let input = std::io::Cursor::new(b"oversized".to_vec());
        assert_eq!(
            read_limited_reader(input, 4).expect("limited read"),
            LimitedRead::Oversized
        );
    }

    #[test]
    fn read_limited_reader_retries_interrupted_reads() {
        struct InterruptedOnce {
            interrupted: bool,
            inner: std::io::Cursor<Vec<u8>>,
        }

        impl std::io::Read for InterruptedOnce {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                self.inner.read(buffer)
            }
        }

        let input = InterruptedOnce {
            interrupted: false,
            inner: std::io::Cursor::new(b"image".to_vec()),
        };
        assert_eq!(
            read_limited_reader(input, 16).expect("limited read"),
            LimitedRead::Complete(b"image".to_vec())
        );
    }
    #[cfg(unix)]
    #[test]
    fn custom_commands_preserve_unix_shell_modes() {
        let detached = detached_custom_command_process("echo hello");
        assert_eq!(detached.get_program(), std::ffi::OsStr::new("/bin/sh"));
        assert_eq!(
            detached.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new("-lc"),
                std::ffi::OsStr::new("echo hello")
            ]
        );

        let pane = pane_custom_command_pty_builder("echo hello");
        let expected: Vec<std::ffi::OsString> =
            vec!["/bin/sh".into(), "-c".into(), "echo hello".into()];
        assert_eq!(pane.get_argv(), &expected);
    }
}
