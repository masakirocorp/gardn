use std::{
    io::Write,
    os::fd::RawFd,
    path::PathBuf,
    process::{Command, Stdio},
};

use super::{
    read_limited_reader, ClipboardCommand, ClipboardImage, ForegroundJob, ForegroundProcess,
    LimitedRead, Signal,
};

pub fn raise_server_nofile_limit() {}

/// Collect the foreground terminal job for a given child PID.
pub fn foreground_job(child_pid: u32) -> Option<ForegroundJob> {
    let foreground_pgid = foreground_process_group_id(child_pid);
    let process_group_id = foreground_pgid.unwrap_or(child_pid);
    let mut processes = foreground_pgid
        .map(collect_processes_in_group)
        .unwrap_or_default();
    let mut seen_pids: std::collections::HashSet<u32> =
        processes.iter().map(|process| process.pid).collect();

    // Some CI/container PTYs do not expose a useful foreground process group
    // while a command is being launched from the pane shell. Only fall back to
    // the pane shell descendants when the foreground group is missing, empty,
    // or just the pane shell itself. A real foreground job should stay
    // authoritative so background agents in the same terminal session cannot
    // steal detection.
    if foreground_pgid.is_none()
        || foreground_group_needs_descendant_fallback(child_pid, &processes)
    {
        for pid in session_processes(child_pid) {
            if seen_pids.contains(&pid) {
                continue;
            }
            if let Some(process) = foreground_process_info(pid) {
                seen_pids.insert(pid);
                processes.push(process);
            }
        }
    }

    if processes.is_empty() {
        return None;
    }

    Some(ForegroundJob {
        process_group_id,
        processes,
    })
}

fn collect_processes_in_group(process_group_id: u32) -> Vec<ForegroundProcess> {
    let mut processes = Vec::new();

    for pid in all_pids() {
        let Some(stat) = process_stat(pid) else {
            continue;
        };
        if stat.pgrp as u32 != process_group_id {
            continue;
        }
        if let Some(process) = foreground_process_info(pid) {
            processes.push(process);
        }
    }

    processes
}

fn foreground_group_needs_descendant_fallback(
    child_pid: u32,
    processes: &[ForegroundProcess],
) -> bool {
    processes.is_empty() || processes.iter().all(|process| process.pid == child_pid)
}

pub fn foreground_process_group_id(child_pid: u32) -> Option<u32> {
    // /proc/<pid>/stat format: "pid (comm) state ppid pgrp session tty_nr tpgid ..."
    // The (comm) field can contain spaces and parens, so we find the last ')' first.
    let stat = std::fs::read_to_string(format!("/proc/{child_pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 2..)?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After (comm): state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5)
    let tpgid: i32 = fields.get(5)?.parse().ok()?;
    (tpgid > 0).then_some(tpgid as u32)
}

pub fn foreground_process_group_id_for_tty_fd(fd: RawFd) -> Option<u32> {
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    (pgid > 0).then_some(pgid as u32)
}

fn foreground_process_info(pid: u32) -> Option<ForegroundProcess> {
    let name = process_stat(pid)?.comm;
    let argv = process_argv(pid);
    Some(ForegroundProcess {
        pid,
        name,
        argv0: argv.as_ref().and_then(|parts| parts.first().cloned()),
        cmdline: argv.as_ref().map(|parts| parts.join(" ")),
        argv,
    })
}

#[derive(Clone)]
struct ProcStat {
    ppid: i32,
    pgrp: i32,
    session: i32,
    comm: String,
}

fn process_stat(pid: u32) -> Option<ProcStat> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let close = stat.rfind(')')?;
    let comm = stat.get(1 + stat.find('(')?..close)?.to_string();
    let rest = stat.get(close + 2..)?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let ppid: i32 = fields.get(1)?.parse().ok()?;
    let pgrp: i32 = fields.get(2)?.parse().ok()?;
    let session: i32 = fields.get(3)?.parse().ok()?;
    Some(ProcStat {
        ppid,
        pgrp,
        session,
        comm,
    })
}

fn process_argv(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let parts: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    (!parts.is_empty()).then_some(parts)
}

/// Get the current working directory of a process.
/// Uses /proc/<pid>/cwd symlink.
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

pub fn active_tcp_listeners() -> Vec<super::TcpListenerInfo> {
    super::active_tcp_listeners_from_lsof()
}

pub fn session_processes(child_pid: u32) -> Vec<u32> {
    let Some(shell_session) = process_stat(child_pid).map(|stat| stat.session) else {
        return Vec::new();
    };

    let mut table = std::collections::HashMap::new();
    for pid in all_pids() {
        let Some(stat) = process_stat(pid) else {
            continue;
        };
        if stat.session == shell_session {
            table.insert(pid, stat);
        }
    }

    let mut pids = table
        .keys()
        .copied()
        .filter(|pid| process_is_descendant_of_child(*pid, child_pid, &table))
        .collect::<Vec<_>>();
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn process_is_descendant_of_child(
    mut pid: u32,
    child_pid: u32,
    table: &std::collections::HashMap<u32, ProcStat>,
) -> bool {
    for _ in 0..=table.len() {
        if pid == child_pid {
            return true;
        }
        let Some(stat) = table.get(&pid) else {
            return false;
        };
        if stat.ppid <= 0 {
            return false;
        }
        pid = stat.ppid as u32;
    }
    false
}

pub fn signal_processes(pids: &[u32], signal: Signal) {
    let sig = match signal {
        Signal::Hangup => libc::SIGHUP,
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };

    for &pid in pids {
        if pid == 0 {
            continue;
        }
        unsafe {
            libc::kill(pid as i32, sig);
        }
    }
}

pub fn process_exists(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        true
    } else {
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

pub fn write_clipboard(bytes: &[u8]) -> bool {
    for command in clipboard_commands() {
        if run_clipboard_command(&command, bytes) {
            return true;
        }
    }
    false
}

pub fn read_clipboard_text() -> Option<String> {
    for command in read_clipboard_text_commands() {
        if let Some(text) = read_clipboard_text_with_command(&command) {
            return Some(text);
        }
    }
    None
}

pub fn open_url(url: &str) -> std::io::Result<()> {
    Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

pub fn read_clipboard_image() -> Option<ClipboardImage> {
    for (mime, extension) in [
        ("image/png", "png"),
        ("image/jpeg", "jpg"),
        ("image/jpg", "jpg"),
        ("image/gif", "gif"),
        ("image/webp", "webp"),
        ("image/bmp", "bmp"),
    ] {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            if let Some(image) =
                read_validated_clipboard_image("wl-paste", &["--type", mime], extension)
            {
                return Some(image);
            }
        }

        if std::env::var_os("DISPLAY").is_some() {
            if let Some(image) = read_validated_clipboard_image(
                "xclip",
                &["-selection", "clipboard", "-t", mime, "-o"],
                extension,
            ) {
                return Some(image);
            }
        }
    }

    None
}

fn read_validated_clipboard_image(
    program: &str,
    args: &[&str],
    extension: &'static str,
) -> Option<ClipboardImage> {
    let bytes = read_clipboard_image_with_command(program, args)?;
    if !bytes_match_image_signature(extension, &bytes) {
        return None;
    }
    Some(ClipboardImage { bytes, extension })
}

fn bytes_match_image_signature(extension: &str, bytes: &[u8]) -> bool {
    match extension {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP",
        "bmp" => {
            if bytes.len() < 26 || !bytes.starts_with(b"BM") {
                return false;
            }
            let offset = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
            (26..=bytes.len()).contains(&offset)
        }
        _ => false,
    }
}

/// Show a native desktop notification through libnotify's command-line helper.
pub fn show_desktop_notification(title: &str, body: Option<&str>) -> std::io::Result<bool> {
    show_desktop_notification_with_command(title, body, |program| Command::new(program))
}

fn show_desktop_notification_with_command(
    title: &str,
    body: Option<&str>,
    mut command: impl FnMut(&str) -> Command,
) -> std::io::Result<bool> {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Ok(false);
    }

    let mut cmd = command("notify-send");
    cmd.arg("--").arg(title);
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        cmd.arg(body);
    }
    run_notification_command(cmd)
}

fn run_notification_command(mut command: Command) -> std::io::Result<bool> {
    let status = match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => status,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };

    Ok(status.success())
}

fn read_clipboard_image_with_command(program: &str, args: &[&str]) -> Option<Vec<u8>> {
    let mut command = Command::new(program);
    command.args(args);
    read_clipboard_image_with_spawned_command(command)
}

fn read_clipboard_image_with_spawned_command(command: Command) -> Option<Vec<u8>> {
    read_clipboard_image_with_spawned_command_max(
        command,
        crate::protocol::MAX_CLIPBOARD_IMAGE_PAYLOAD,
    )
}

fn read_clipboard_image_with_spawned_command_max(
    mut command: Command,
    max_bytes: usize,
) -> Option<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;

    let read = match read_limited_reader(stdout, max_bytes) {
        Ok(read) => read,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };

    if read == LimitedRead::Oversized {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }

    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }

    match read {
        LimitedRead::Complete(bytes) => Some(bytes),
        LimitedRead::Empty | LimitedRead::Oversized => None,
    }
}

fn clipboard_commands() -> Vec<ClipboardCommand> {
    let mut commands = Vec::new();

    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        commands.push(ClipboardCommand {
            program: "wl-copy",
            args: &["--type", "text/plain;charset=utf-8"],
        });
    }

    if std::env::var_os("DISPLAY").is_some() {
        commands.push(ClipboardCommand {
            program: "xclip",
            args: &["-selection", "clipboard", "-in"],
        });
        commands.push(ClipboardCommand {
            program: "xsel",
            args: &["--clipboard", "--input"],
        });
    }

    commands
}

fn read_clipboard_text_commands() -> Vec<ClipboardCommand> {
    let mut commands = Vec::new();

    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        commands.push(ClipboardCommand {
            program: "wl-paste",
            args: &["--type", "text/plain;charset=utf-8"],
        });
        commands.push(ClipboardCommand {
            program: "wl-paste",
            args: &["--type", "text/plain"],
        });
    }

    if std::env::var_os("DISPLAY").is_some() {
        commands.push(ClipboardCommand {
            program: "xclip",
            args: &["-selection", "clipboard", "-out"],
        });
        commands.push(ClipboardCommand {
            program: "xsel",
            args: &["--clipboard", "--output"],
        });
    }

    commands
}

fn read_clipboard_text_with_command(command: &ClipboardCommand) -> Option<String> {
    const MAX_CLIPBOARD_TEXT_BYTES: usize = 1024 * 1024;

    let mut child = Command::new(command.program)
        .args(command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let stdout = child.stdout.take()?;
    let read = match read_limited_reader(stdout, MAX_CLIPBOARD_TEXT_BYTES) {
        Ok(LimitedRead::Oversized) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        Ok(read) => read,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };

    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }

    match read {
        LimitedRead::Complete(bytes) => String::from_utf8(bytes).ok(),
        LimitedRead::Empty => None,
        LimitedRead::Oversized => unreachable!("oversized clipboard text is handled before wait"),
    }
}

fn run_clipboard_command(command: &ClipboardCommand, bytes: &[u8]) -> bool {
    let mut child = match Command::new(command.program)
        .args(command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    };

    if stdin.write_all(bytes).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return false;
    }
    drop(stdin);

    child.wait().map(|status| status.success()).unwrap_or(false)
}

fn all_pids() -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestEnvVar;
    use std::sync::{Mutex, OnceLock};

    use std::os::unix::process::CommandExt;
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn session_processes_are_scoped_to_pane_session() {
        let mut command = std::process::Command::new("/bin/sh");
        command.arg("-c").arg("sleep 5");
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().expect("spawn shell");

        let pids = session_processes(child.id());
        let _ = child.kill();
        let _ = child.wait();

        assert!(pids.contains(&child.id()));
        assert!(!pids.contains(&std::process::id()));
    }

    #[test]
    fn process_descendant_check_ignores_same_session_siblings() {
        let table = std::collections::HashMap::from([
            (
                10,
                ProcStat {
                    ppid: 1,
                    pgrp: 10,
                    session: 10,
                    comm: "sh".into(),
                },
            ),
            (
                11,
                ProcStat {
                    ppid: 10,
                    pgrp: 11,
                    session: 10,
                    comm: "pi".into(),
                },
            ),
            (
                12,
                ProcStat {
                    ppid: 1,
                    pgrp: 12,
                    session: 10,
                    comm: "codex".into(),
                },
            ),
        ]);

        assert!(process_is_descendant_of_child(11, 10, &table));
        assert!(!process_is_descendant_of_child(12, 10, &table));
    }

    #[test]
    fn clipboard_commands_prefer_wayland_when_available() {
        let _guard = env_lock().lock().unwrap();
        let _wayland_env = TestEnvVar::set("WAYLAND_DISPLAY", "wayland-0");
        let _display_env = TestEnvVar::remove("DISPLAY");
        let commands = clipboard_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program, "wl-copy");
    }

    #[test]
    fn clipboard_commands_include_x11_fallbacks() {
        let _guard = env_lock().lock().unwrap();
        let _wayland_env = TestEnvVar::remove("WAYLAND_DISPLAY");
        let _display_env = TestEnvVar::set("DISPLAY", ":0");
        let commands = clipboard_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].program, "xclip");
        assert_eq!(commands[1].program, "xsel");
    }

    #[test]
    fn read_clipboard_text_commands_include_session_backends() {
        let _guard = env_lock().lock().unwrap();
        let _wayland_env = TestEnvVar::set("WAYLAND_DISPLAY", "wayland-0");
        let _display_env = TestEnvVar::set("DISPLAY", ":0");

        let commands = read_clipboard_text_commands();
        assert_eq!(commands[0].program, "wl-paste");
        assert_eq!(commands[1].program, "wl-paste");
        assert_eq!(commands[2].program, "xclip");
        assert_eq!(commands[3].program, "xsel");
    }

    #[test]
    fn read_clipboard_text_with_command_reads_utf8() {
        let command = ClipboardCommand {
            program: "printf",
            args: &["feature/linear-302"],
        };

        assert_eq!(
            read_clipboard_text_with_command(&command).as_deref(),
            Some("feature/linear-302")
        );
    }

    #[test]
    fn read_clipboard_text_with_command_rejects_oversized_output() {
        let command = ClipboardCommand {
            program: "sh",
            args: &["-c", "yes x | head -c 1048578"],
        };

        assert_eq!(read_clipboard_text_with_command(&command), None);
    }

    #[test]
    fn read_clipboard_image_with_spawned_command_reads_under_limit() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("printf image");

        assert_eq!(
            read_clipboard_image_with_spawned_command_max(command, 16),
            Some(b"image".to_vec())
        );
    }

    #[test]
    fn read_clipboard_image_with_spawned_command_rejects_over_limit() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("printf oversized");

        assert_eq!(
            read_clipboard_image_with_spawned_command_max(command, 4),
            None
        );
    }

    #[test]
    fn read_clipboard_image_rejects_xclip_text_served_for_image_target() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!("hako-fake-xclip-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let fake_xclip = temp_dir.join("xclip");
        std::fs::write(&fake_xclip, "#!/bin/sh\nprintf '# Tasks'\n")
            .expect("fake xclip should be written");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(&fake_xclip)
                .expect("fake xclip metadata")
                .permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(&fake_xclip, permissions)
                .expect("fake xclip should be executable");
        }

        let test_path = {
            let mut paths = vec![temp_dir.clone()];
            if let Some(path) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&path));
            }
            std::env::join_paths(paths).expect("test path should be valid")
        };

        let _wayland_env = TestEnvVar::remove("WAYLAND_DISPLAY");
        let _display_env = TestEnvVar::set("DISPLAY", ":0");
        let _path_env = TestEnvVar::set("PATH", test_path);

        let result = read_clipboard_image();

        let _ = std::fs::remove_file(fake_xclip);
        let _ = std::fs::remove_dir(temp_dir);

        assert_eq!(result, None);
    }

    #[test]
    fn read_clipboard_image_rejects_wayland_xclip_fallback_text_for_image_target() {
        let _guard = env_lock().lock().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("hako-fake-wayland-xclip-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let fake_wl_paste = temp_dir.join("wl-paste");
        let fake_xclip = temp_dir.join("xclip");
        std::fs::write(&fake_wl_paste, "#!/bin/sh\nexit 1\n")
            .expect("fake wl-paste should be written");
        std::fs::write(&fake_xclip, "#!/bin/sh\nprintf '# Tasks'\n")
            .expect("fake xclip should be written");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            for command in [&fake_wl_paste, &fake_xclip] {
                let mut permissions = std::fs::metadata(command)
                    .expect("fake clipboard command metadata")
                    .permissions();
                permissions.set_mode(0o700);
                std::fs::set_permissions(command, permissions)
                    .expect("fake clipboard command should be executable");
            }
        }

        let test_path = {
            let mut paths = vec![temp_dir.clone()];
            if let Some(path) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&path));
            }
            std::env::join_paths(paths).expect("test path should be valid")
        };

        let _wayland_env = TestEnvVar::set("WAYLAND_DISPLAY", "wayland-0");
        let _display_env = TestEnvVar::set("DISPLAY", ":0");
        let _path_env = TestEnvVar::set("PATH", test_path);

        let result = read_clipboard_image();

        let _ = std::fs::remove_file(fake_wl_paste);
        let _ = std::fs::remove_file(fake_xclip);
        let _ = std::fs::remove_dir(temp_dir);

        assert_eq!(result, None);
    }

    #[test]
    fn read_validated_clipboard_image_accepts_real_png_payload() {
        assert_eq!(
            read_validated_clipboard_image(
                "sh",
                &["-c", "printf '\\211PNG\\r\\n\\032\\nrest-of-image'"],
                "png"
            ),
            Some(ClipboardImage {
                bytes: b"\x89PNG\r\n\x1a\nrest-of-image".to_vec(),
                extension: "png",
            })
        );
    }

    #[test]
    fn image_signatures_match_only_their_format() {
        assert!(bytes_match_image_signature("png", b"\x89PNG\r\n\x1a\n..."));
        assert!(bytes_match_image_signature(
            "jpg",
            &[0xFF, 0xD8, 0xFF, 0xE0]
        ));
        assert!(bytes_match_image_signature("gif", b"GIF87a..."));
        assert!(bytes_match_image_signature("gif", b"GIF89a..."));
        assert!(bytes_match_image_signature(
            "webp",
            b"RIFF\x10\x00\x00\x00WEBPVP8 "
        ));

        let mut bmp = vec![0u8; 26];
        bmp[..2].copy_from_slice(b"BM");
        bmp[10] = 26;
        assert!(bytes_match_image_signature("bmp", &bmp));

        assert!(!bytes_match_image_signature("png", b"# Tasks"));
        assert!(!bytes_match_image_signature("jpg", b"plain clipboard text"));
        assert!(!bytes_match_image_signature("gif", b""));
        assert!(!bytes_match_image_signature("webp", b"RIFF but not webp"));
        assert!(!bytes_match_image_signature("bmp", b"\x89PNG\r\n\x1a\n"));
        assert!(!bytes_match_image_signature(
            "bmp",
            b"BM text is not a bitmap"
        ));
        assert!(!bytes_match_image_signature("svg", b"<svg></svg>"));
    }

    #[test]
    fn desktop_notification_separates_option_like_titles() {
        let _guard = env_lock().lock().unwrap();
        let _wayland_env = TestEnvVar::remove("WAYLAND_DISPLAY");
        let _display_env = TestEnvVar::set("DISPLAY", ":0");

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "hako-notify-send-args-{}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let script = "printf '%s\\n' \"$@\" > \"$HAKO_NOTIFY_ARGS\"";
        let shown = show_desktop_notification_with_command("-danger", Some("body"), |_| {
            let mut cmd = Command::new("sh");
            cmd.arg("-c")
                .arg(script)
                .arg("notify-send")
                .env("HAKO_NOTIFY_ARGS", &path);
            cmd
        })
        .expect("notification command should run");

        assert!(shown);
        let args = std::fs::read_to_string(&path).expect("args file");
        let _ = std::fs::remove_file(&path);
        assert_eq!(args, "--\n-danger\nbody\n");
    }
}
