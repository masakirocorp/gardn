use std::path::PathBuf;

use super::{ClipboardImage, ForegroundJob, Signal, TcpListenerInfo};

pub(crate) fn should_draw_host_cursor_by_default_platform() -> bool {
    false
}

pub(crate) fn scrollback_editor_argv_platform(
    _path: &std::path::Path,
) -> std::io::Result<Vec<String>> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "opening scrollback in an editor is not supported on this platform",
    ))
}
/// Unsupported platform stub.
pub fn detach_server_daemon_command(_command: &mut std::process::Command) {}
pub fn raise_server_nofile_limit() {}

fn custom_command_argv(command: &str, flag: &str) -> Vec<std::ffi::OsString> {
    vec!["/bin/sh".into(), flag.into(), command.into()]
}

pub(crate) fn detached_custom_command_process_platform(command: &str) -> std::process::Command {
    let argv = custom_command_argv(command, "-lc");
    let mut process = std::process::Command::new(&argv[0]);
    process.args(&argv[1..]);
    process
}

pub(crate) fn pane_custom_command_pty_builder_platform(
    command: &str,
) -> portable_pty::CommandBuilder {
    portable_pty::CommandBuilder::from_argv(custom_command_argv(command, "-c"))
}

/// Unsupported platform stub.
pub fn foreground_job(_child_pid: u32) -> Option<ForegroundJob> {
    None
}

/// Unsupported platform stub.
pub fn foreground_process_group_id(_child_pid: u32) -> Option<u32> {
    None
}

/// Unsupported platform stub.
pub fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

pub fn active_tcp_listeners() -> Vec<TcpListenerInfo> {
    Vec::new()
}

/// Unsupported platform stub.
pub fn session_processes(_child_pid: u32) -> Vec<u32> {
    Vec::new()
}

/// Unsupported platform stub.
pub fn signal_processes(_pids: &[u32], _signal: Signal) {}

/// Unsupported platform stub.
pub fn process_exists(_pid: u32) -> bool {
    false
}

/// Unsupported platform stub.
pub fn write_clipboard(_bytes: &[u8]) -> bool {
    false
}

/// Unsupported platform stub.
pub fn read_clipboard_text() -> Option<String> {
    None
}

/// Unsupported platform stub.
pub fn open_url(_url: &str) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "opening URLs is not supported on this platform",
    ))
}

/// Unsupported platform stub.
pub fn read_clipboard_image() -> Option<ClipboardImage> {
    None
}

/// Unsupported platform stub.
pub fn show_desktop_notification(_title: &str, _body: Option<&str>) -> std::io::Result<bool> {
    Ok(false)
}
