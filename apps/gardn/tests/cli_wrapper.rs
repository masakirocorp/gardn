#![cfg(unix)]
#![cfg(not(target_os = "macos"))]

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use support::{
    cleanup_test_base, connect_unix_socket, register_runtime_dir, register_spawned_gardn_pid,
    unregister_spawned_gardn_pid,
};

fn unique_test_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!("/tmp/gardn-cli-{}-{nanos}", std::process::id()))
}

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "git command failed: git -C {} {}",
        repo.display(),
        args.join(" ")
    );
}

fn create_committed_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    run_git(path, &["init", "--quiet"]);
    run_git(path, &["config", "user.email", "gardn@example.invalid"]);
    run_git(path, &["config", "user.name", "Gardn Test"]);
    fs::write(path.join("README.md"), "test\n").unwrap();
    run_git(path, &["add", "README.md"]);
    run_git(path, &["commit", "--quiet", "-m", "initial"]);
}

struct SpawnedGardn {
    _master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

struct SpawnedServerProcess {
    child: std::process::Child,
}

impl Drop for SpawnedServerProcess {
    fn drop(&mut self) {
        let pid = self.child.id();
        let _ = self.child.kill();
        let _ = self.child.wait();
        unregister_spawned_gardn_pid(Some(pid));
    }
}

impl Drop for SpawnedGardn {
    fn drop(&mut self) {
        let pid = self.child.process_id();
        let _ = self.child.kill();

        if let Some(pid) = pid {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                let mut status = 0;
                let result =
                    unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
                if result == pid as libc::pid_t || result == -1 {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }

            unregister_spawned_gardn_pid(Some(pid));
        }
    }
}

fn cleanup_spawned_gardn(spawned: SpawnedGardn, base: PathBuf) {
    drop(spawned);
    cleanup_test_base(&base);
}

fn wait_for_socket(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() && std::os::unix::net::UnixStream::connect(path).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("socket did not appear at {}", path.display());
}

fn spawn_gardn(config_home: &Path, runtime_dir: &Path, socket_path: &Path) -> SpawnedGardn {
    spawn_gardn_with_config(
        config_home,
        runtime_dir,
        socket_path,
        None,
        "onboarding = false\n",
    )
}

fn spawn_gardn_with_pane_history(
    config_home: &Path,
    runtime_dir: &Path,
    socket_path: &Path,
) -> SpawnedGardn {
    spawn_gardn_with_config(
        config_home,
        runtime_dir,
        socket_path,
        None,
        "onboarding = false\n[experimental]\npane_history = true\n",
    )
}

fn app_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "gardn-dev"
    } else {
        "gardn"
    }
}

fn named_session_socket(config_home: &Path, session: &str) -> PathBuf {
    config_home
        .join(app_dir_name())
        .join("sessions")
        .join(session)
        .join("gardn.sock")
}

fn spawn_named_server(
    config_home: &Path,
    runtime_dir: &Path,
    session: &str,
) -> SpawnedServerProcess {
    fs::create_dir_all(config_home.join(app_dir_name())).unwrap();
    fs::create_dir_all(runtime_dir).unwrap();
    register_runtime_dir(runtime_dir);
    fs::write(
        config_home.join(app_dir_name()).join("config.toml"),
        "onboarding = false\n",
    )
    .unwrap();

    let mut command = Command::new(env!("CARGO_BIN_EXE_gardn"));
    command
        .args(["--session", session, "server"])
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .env_remove("GARDN_SOCKET_PATH")
        .env_remove("GARDN_CLIENT_SOCKET_PATH")
        .env_remove("GARDN_ENV")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let child = command.spawn().unwrap();
    register_spawned_gardn_pid(Some(child.id()));
    SpawnedServerProcess { child }
}

fn run_named_cli(config_home: &Path, runtime_dir: &Path, args: &[&str]) -> std::process::Output {
    run_named_cli_with_socket_override(config_home, runtime_dir, args, None)
}

fn run_named_cli_with_socket_override(
    config_home: &Path,
    runtime_dir: &Path,
    args: &[&str],
    socket_override: Option<&Path>,
) -> std::process::Output {
    run_named_cli_with_env_and_socket_override(config_home, runtime_dir, args, &[], socket_override)
}

fn run_named_cli_with_env(
    config_home: &Path,
    runtime_dir: &Path,
    args: &[&str],
    envs: &[(&str, &Path)],
) -> std::process::Output {
    run_named_cli_with_env_and_socket_override(config_home, runtime_dir, args, envs, None)
}

fn run_named_cli_with_env_and_socket_override(
    config_home: &Path,
    runtime_dir: &Path,
    args: &[&str],
    envs: &[(&str, &Path)],
    socket_override: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gardn"));
    command
        .args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .env_remove("GARDN_CLIENT_SOCKET_PATH")
        .env_remove("GARDN_ENV");
    for (key, value) in envs {
        command.env(key, value);
    }
    if let Some(socket_override) = socket_override {
        command.env("GARDN_SOCKET_PATH", socket_override);
    } else {
        command.env_remove("GARDN_SOCKET_PATH");
    }
    command.output().unwrap()
}

fn run_named_cli_json(config_home: &Path, runtime_dir: &Path, args: &[&str]) -> serde_json::Value {
    let output = run_named_cli(config_home, runtime_dir, args);
    assert!(
        output.status.success(),
        "command failed: gardn {}\nstatus: {:?}\nstderr: {}\nstdout: {}",
        args.join(" "),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn spawn_gardn_with_path(
    config_home: &Path,
    runtime_dir: &Path,
    socket_path: &Path,
    path_override: Option<&Path>,
) -> SpawnedGardn {
    spawn_gardn_with_config(
        config_home,
        runtime_dir,
        socket_path,
        path_override,
        "onboarding = false\n",
    )
}

fn spawn_gardn_with_config(
    config_home: &Path,
    runtime_dir: &Path,
    socket_path: &Path,
    path_override: Option<&Path>,
    config_toml: &str,
) -> SpawnedGardn {
    fs::create_dir_all(config_home.join(app_dir_name())).unwrap();
    fs::create_dir_all(runtime_dir).unwrap();
    register_runtime_dir(runtime_dir);
    fs::write(
        config_home.join(app_dir_name()).join("config.toml"),
        config_toml,
    )
    .unwrap();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_gardn"));
    cmd.arg("server");
    cmd.env("XDG_CONFIG_HOME", config_home);
    cmd.env("XDG_RUNTIME_DIR", runtime_dir);
    cmd.env("GARDN_SOCKET_PATH", socket_path);
    cmd.env_remove("GARDN_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("GARDN_ENV");
    if let Some(path) = path_override {
        cmd.env("PATH", path);
    }

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_gardn_pid(child.process_id());
    SpawnedGardn {
        _master: pair.master,
        child,
    }
}

fn run_cli(socket_path: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gardn"));
    command.args(args);
    command.env("GARDN_SOCKET_PATH", socket_path);
    command.output().unwrap()
}

fn run_cli_in_dir(socket_path: &Path, args: &[&str], current_dir: &Path) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gardn"));
    command.args(args);
    command.current_dir(current_dir);
    command.env("GARDN_SOCKET_PATH", socket_path);
    command.output().unwrap()
}

fn run_cli_json(socket_path: &Path, args: &[&str]) -> serde_json::Value {
    let output = run_cli(socket_path, args);
    parse_cli_json_output(args, output)
}

fn run_cli_json_in_dir(socket_path: &Path, args: &[&str], current_dir: &Path) -> serde_json::Value {
    let output = run_cli_in_dir(socket_path, args, current_dir);
    parse_cli_json_output(args, output)
}

fn parse_cli_json_output(args: &[&str], output: std::process::Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "command failed: gardn {}\nstatus: {:?}\nstderr: {}\nstdout: {}",
        args.join(" "),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "failed to parse JSON response for `gardn {}`: {}\nstdout: {}\nstderr: {}",
            args.join(" "),
            err,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn wait_until(timeout: Duration, interval: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(interval);
    }
    false
}

fn pane_read_recent_contains(socket_path: &Path, pane_id: &str, expected: &str) -> bool {
    let output = run_cli(
        socket_path,
        &["pane", "read", pane_id, "--source", "recent"],
    );
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout).contains(expected)
}

fn process_exists(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as i32, 0) };
    if result == 0 {
        true
    } else {
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

fn wait_for_pid_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_exists(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(25));
    }
    !process_exists(pid)
}

fn wait_for_pid_file(pid_file: &Path, timeout: Duration) -> Result<u32, String> {
    const STABLE_PID_CONTENT_WINDOW: Duration = Duration::from_millis(250);

    let deadline = Instant::now() + timeout;
    let mut last_contents = String::new();
    let mut stable_candidate: Option<(String, u32, Instant)> = None;

    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(pid_file) {
            let trimmed = contents.trim().to_string();
            last_contents = contents;

            if let Ok(pid) = trimmed.parse::<u32>() {
                match &stable_candidate {
                    Some((candidate_text, candidate_pid, stable_since))
                        if candidate_text == &trimmed && *candidate_pid == pid =>
                    {
                        if stable_since.elapsed() >= STABLE_PID_CONTENT_WINDOW {
                            return Ok(pid);
                        }
                    }
                    _ => {
                        stable_candidate = Some((trimmed, pid, Instant::now()));
                    }
                }
            } else {
                stable_candidate = None;
            }
        }

        thread::sleep(Duration::from_millis(25));
    }

    Err(format!(
        "pid file {} did not contain stable parseable pid before timeout; last contents={:?}",
        pid_file.display(),
        last_contents
    ))
}

#[test]
fn wait_for_pid_file_retries_until_pid_is_written() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let pid_file = base.join("delayed.pid");
    fs::write(&pid_file, "").unwrap();

    let writer = thread::spawn({
        let pid_file = pid_file.clone();
        move || {
            thread::sleep(Duration::from_millis(100));
            fs::write(pid_file, "424242\n").unwrap();
        }
    });

    let pid = wait_for_pid_file(&pid_file, Duration::from_secs(2)).unwrap();
    assert_eq!(pid, 424242);

    writer.join().unwrap();
    cleanup_test_base(&base);
}

#[test]
fn wait_for_pid_file_errors_when_file_never_contains_pid() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let pid_file = base.join("empty.pid");
    fs::write(&pid_file, "").unwrap();

    let err = wait_for_pid_file(&pid_file, Duration::from_millis(150)).unwrap_err();
    assert!(
        err.contains("did not contain stable parseable pid"),
        "unexpected error: {err}"
    );

    cleanup_test_base(&base);
}

#[test]
fn wait_for_pid_file_rejects_unparseable_partial_write_until_stable_contents() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let pid_file = base.join("partial-race.pid");
    fs::write(&pid_file, "").unwrap();

    let writer = thread::spawn({
        let pid_file = pid_file.clone();
        move || {
            thread::sleep(Duration::from_millis(40));
            fs::write(&pid_file, "pid=").unwrap();
            thread::sleep(Duration::from_millis(40));
            fs::write(&pid_file, "pid=424242").unwrap();
            thread::sleep(Duration::from_millis(40));
            fs::write(&pid_file, "424242\n").unwrap();
        }
    });

    let start = Instant::now();
    let pid = wait_for_pid_file(&pid_file, Duration::from_secs(2)).unwrap();
    assert_eq!(pid, 424242);
    assert!(
        start.elapsed() >= Duration::from_millis(300),
        "helper should wait for stable complete contents, elapsed={:?}",
        start.elapsed()
    );

    writer.join().unwrap();
    cleanup_test_base(&base);
}

fn send_request(socket_path: &Path, json: &str) -> serde_json::Value {
    let mut stream = connect_unix_socket(socket_path, Duration::from_secs(5));
    stream.write_all(json.as_bytes()).unwrap();
    stream.write_all(b"\n").unwrap();
    stream.flush().unwrap();

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
fn accept_fake_cli_operation(listener: &UnixListener) -> (UnixStream, String) {
    loop {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        reader.read_line(&mut line).unwrap();
        let request: serde_json::Value = serde_json::from_str(&line).unwrap();
        if request["method"] != "ping" {
            return (stream, line);
        }

        writeln!(
            stream,
            "{}",
            serde_json::json!({
                "id": request["id"],
                "result": {
                    "type": "pong",
                    "version": "different-build-same-protocol",
                    "protocol": 13,
                    "capabilities": { "live_handoff": true }
                }
            })
        )
        .unwrap();
        stream.flush().unwrap();
    }
}

fn run_claude_hook(action: &str, hook_input: &str) -> Option<serde_json::Value> {
    run_shell_hook(
        "src/integration/assets/claude/gardn-agent-state.sh",
        &[action],
        hook_input,
    )
}

fn run_codex_hook(action: &str, hook_input: &str) -> Option<serde_json::Value> {
    run_shell_hook(
        "src/integration/assets/codex/gardn-agent-state.sh",
        &[action],
        hook_input,
    )
}

fn run_grok_hook(action: &str, hook_input: &str) -> Option<serde_json::Value> {
    run_shell_hook(
        "src/integration/assets/grok/gardn-agent-state.sh",
        &[action],
        hook_input,
    )
}

fn run_copilot_hook(hook_input: &str) -> Option<serde_json::Value> {
    run_shell_hook(
        "src/integration/assets/copilot/gardn-agent-state.sh",
        &[],
        hook_input,
    )
}

fn run_devin_hook(
    action: &str,
    hook_input: &str,
    envs: &[(&str, &str)],
) -> Option<serde_json::Value> {
    run_shell_hook_with_env(
        "src/integration/assets/devin/gardn-agent-state.sh",
        &[action],
        hook_input,
        envs,
    )
}

fn run_shell_hook(asset_path: &str, args: &[&str], hook_input: &str) -> Option<serde_json::Value> {
    run_shell_hook_with_env(asset_path, args, hook_input, &[])
}

fn run_shell_hook_with_env(
    asset_path: &str,
    args: &[&str],
    hook_input: &str,
    envs: &[(&str, &str)],
) -> Option<serde_json::Value> {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let socket_path = base.join("gardn.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(700);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut line = String::new();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    reader.read_line(&mut line).unwrap();
                    let _ = stream.write_all(br#"{"id":"test","result":{"type":"ok"}}"#);
                    let _ = stream.write_all(b"\n");
                    let _ = stream.flush();
                    return Some(line);
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => panic!("accept failed: {err}"),
            }
        }
        None
    });

    let hook_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(asset_path);
    let mut command = Command::new("bash");
    command
        .arg(hook_path)
        .args(args)
        .env("GARDN_ENV", "1")
        .env("GARDN_SOCKET_PATH", &socket_path)
        .env("GARDN_PANE_ID", "p_test")
        .env_remove("CODEX_THREAD_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(hook_input.as_bytes()).unwrap();
    drop(stdin);

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "hook failed: status={:?} stderr={} stdout={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    let request = server.join().unwrap();
    cleanup_test_base(&base);
    request.map(|line| serde_json::from_str(&line).unwrap())
}

#[test]
fn claude_hook_reports_session_identity_and_lifecycle_state() {
    let session = run_claude_hook(
        "session",
        r#"{"hook_event_name":"SessionStart","session_id":"claude-session"}"#,
    )
    .expect("session start should report session identity");
    assert_eq!(session["method"], "pane.report_agent_session");
    assert_eq!(session["params"]["agent_session_id"], "claude-session");
    assert_eq!(session["params"]["agent"], "claude");

    let working = run_claude_hook(
        "working",
        r#"{"hook_event_name":"SubagentStart","session_id":"claude-session","agent_id":"agent-abc123","agent_type":"Explore"}"#,
    )
    .expect("subagent start should keep parent pane working");
    assert_eq!(working["method"], "pane.report_agent");
    assert_eq!(working["params"]["state"], "working");
    assert_eq!(working["params"]["agent_session_id"], "claude-session");

    let blocked = run_claude_hook(
        "blocked",
        r#"{"hook_event_name":"Notification","notification_type":"permission_prompt","session_id":"claude-session"}"#,
    )
    .expect("permission notification should block the pane");
    assert_eq!(blocked["method"], "pane.report_agent");
    assert_eq!(blocked["params"]["state"], "blocked");

    let idle = run_claude_hook(
        "idle",
        r#"{"hook_event_name":"Notification","notification_type":"idle_prompt","session_id":"claude-session"}"#,
    )
    .expect("idle notification should idle the pane");
    assert_eq!(idle["method"], "pane.report_agent");
    assert_eq!(idle["params"]["state"], "idle");

    assert!(
        run_claude_hook(
            "idle",
            r#"{"hook_event_name":"SubagentStop","agent_id":"agent-abc123","agent_type":"Explore"}"#,
        )
        .is_none(),
        "SubagentStop must not idle the parent pane"
    );
}

#[test]
fn codex_hook_reports_persisted_root_session_and_ignores_ephemeral_or_nested_sessions() {
    let session = run_codex_hook(
        "session",
        r#"{"hook_event_name":"SessionStart","session_id":"codex-session","transcript_path":"/tmp/codex-session.jsonl"}"#,
    )
    .expect("codex hook should report session identity");
    assert_eq!(session["method"], "pane.report_agent_session");
    assert_eq!(session["params"]["agent_session_id"], "codex-session");
    assert_eq!(session["params"]["agent"], "codex");

    let matching_request = run_shell_hook_with_env(
        "src/integration/assets/codex/gardn-agent-state.sh",
        &["session"],
        r#"{"hook_event_name":"SessionStart","session_id":"codex-session","transcript_path":"/tmp/codex-session.jsonl"}"#,
        &[("CODEX_THREAD_ID", "codex-session")],
    )
    .expect("matching inherited session should still report");
    assert_eq!(
        matching_request["params"]["agent_session_id"],
        "codex-session"
    );

    assert!(run_codex_hook(
        "session",
        r#"{"hook_event_name":"SessionStart","session_id":"side-session","transcript_path":null}"#,
    )
    .is_none());

    assert!(run_shell_hook_with_env(
        "src/integration/assets/codex/gardn-agent-state.sh",
        &["session"],
        r#"{"hook_event_name":"SessionStart","session_id":"nested-session","transcript_path":"/tmp/nested-session.jsonl"}"#,
        &[("CODEX_THREAD_ID", "parent-session")],
    )
    .is_none());

    let working = run_codex_hook(
        "working",
        r#"{"hook_event_name":"SubagentStart","session_id":"codex-session","agent_id":"agent-abc123"}"#,
    )
    .expect("codex subagent start should keep parent pane working");
    assert_eq!(working["method"], "pane.report_agent");
    assert_eq!(working["params"]["state"], "working");
    assert_eq!(working["params"]["agent_session_id"], "codex-session");

    let blocked = run_codex_hook(
        "blocked",
        r#"{"hook_event_name":"PermissionRequest","session_id":"codex-session"}"#,
    )
    .expect("codex permission request should block the pane");
    assert_eq!(blocked["method"], "pane.report_agent");
    assert_eq!(blocked["params"]["state"], "blocked");

    let idle = run_codex_hook(
        "idle",
        r#"{"hook_event_name":"Stop","session_id":"codex-session"}"#,
    )
    .expect("codex stop should idle the pane");
    assert_eq!(idle["method"], "pane.report_agent");
    assert_eq!(idle["params"]["state"], "idle");

    assert!(
        run_codex_hook(
            "idle",
            r#"{"hook_event_name":"SubagentStop","agent_id":"agent-abc123"}"#,
        )
        .is_none(),
        "SubagentStop must not idle the parent pane"
    );
}

#[test]
fn grok_hook_prefers_injected_session_id_and_reports_session_identity() {
    let session = run_grok_hook(
        "session",
        r#"{"hookEventName":"SessionStart","sessionId":"payload-session"}"#,
    )
    .expect("grok hook should report payload session identity");
    assert_eq!(session["method"], "pane.report_agent_session");
    assert_eq!(session["params"]["source"], "gardn:grok");
    assert_eq!(session["params"]["agent"], "grok");
    assert_eq!(session["params"]["agent_session_id"], "payload-session");

    let injected = run_shell_hook_with_env(
        "src/integration/assets/grok/gardn-agent-state.sh",
        &["session"],
        r#"{"hookEventName":"SessionStart","sessionId":"payload-session"}"#,
        &[("GROK_SESSION_ID", "injected-session")],
    )
    .expect("injected GROK_SESSION_ID should win");
    assert_eq!(injected["params"]["agent_session_id"], "injected-session");
}

#[test]
fn copilot_hook_reports_prompt_tool_and_session_state() {
    let request = run_copilot_hook(
        r#"{"hook_event_name":"SessionStart","session_id":"copilot-session","initial_prompt":"build this"}"#,
    )
    .expect("session start with an initial prompt should report working");
    assert_eq!(request["method"], "pane.report_agent");
    assert_eq!(request["params"]["state"], "working");
    assert_eq!(request["params"]["agent_session_id"], "copilot-session");
    assert_eq!(request["params"]["source"], "gardn:copilot");

    let request = run_copilot_hook(
        r#"{"hook_event_name":"PreToolUse","session_id":"copilot-session","tool_name":"ask_user"}"#,
    )
    .expect("ask_user should report blocked");
    assert_eq!(request["method"], "pane.report_agent");
    assert_eq!(request["params"]["state"], "blocked");

    let request = run_copilot_hook(
        r#"{"hook_event_name":"agentStop","session_id":"copilot-session","stop_reason":"end_turn"}"#,
    )
    .expect("turn end should report idle");
    assert_eq!(request["method"], "pane.report_agent");
    assert_eq!(request["params"]["state"], "idle");
}

#[test]
fn copilot_hook_reports_permission_notifications() {
    let request = run_copilot_hook(
        r#"{"hook_event_name":"notification","session_id":"copilot-session","notification_type":"permission_prompt"}"#,
    )
    .expect("permission prompt notification should report blocked");
    assert_eq!(request["method"], "pane.report_agent");
    assert_eq!(request["params"]["state"], "blocked");

    let request = run_copilot_hook(
        r#"{"hook_event_name":"notification","session_id":"copilot-session","notification_type":"agent_idle"}"#,
    )
    .expect("agent idle notification should report idle");
    assert_eq!(request["method"], "pane.report_agent");
    assert_eq!(request["params"]["state"], "idle");
}

#[test]
fn copilot_hook_releases_on_user_exit_only() {
    assert!(
        run_copilot_hook(
            r#"{"hook_event_name":"SessionEnd","session_id":"copilot-session","reason":"complete"}"#
        )
        .is_none(),
        "normal Copilot turn completion should keep session ownership"
    );

    let request = run_copilot_hook(
        r#"{"hook_event_name":"SessionEnd","session_id":"copilot-session","reason":"user_exit"}"#,
    )
    .expect("user exit should release session ownership");
    assert_eq!(request["method"], "pane.release_agent");
}

#[test]
fn devin_hook_reports_session_identity_without_lifecycle_state() {
    let request = run_devin_hook(
        "session",
        r#"{"hook_event_name":"SessionStart","session_id":"devin-session","source":"startup"}"#,
        &[("GARDN_DEVIN_LIST_JSON", r#"[{"id":"older-session"}]"#)],
    )
    .expect("session start should report devin session identity");

    assert_eq!(request["method"], "pane.report_agent_session");
    assert_eq!(request["params"]["source"], "gardn:devin");
    assert_eq!(request["params"]["agent"], "devin");
    assert_eq!(request["params"]["agent_session_id"], "devin-session");
    assert!(request["params"].get("state").is_none());
}

#[test]
fn devin_hook_uses_session_list_only_for_non_prompt_events() {
    let request = run_devin_hook(
        "session",
        r#"{"hook_event_name":"PreToolUse","tool_name":"exec"}"#,
        &[
            ("DEVIN_PROJECT_DIR", "/tmp/project"),
            (
                "GARDN_DEVIN_LIST_JSON",
                r#"[{"id":"other-session","working_directory":"/tmp/other"},{"id":"devin-session","working_directory":"/tmp/project"}]"#,
            ),
        ],
    )
    .expect("tool event should resolve session from devin list");

    assert_eq!(request["method"], "pane.report_agent_session");
    assert_eq!(request["params"]["agent_session_id"], "devin-session");
    assert!(
        run_devin_hook(
            "session",
            r#"{"hook_event_name":"UserPromptSubmit","prompt":"run tests"}"#,
            &[
                ("DEVIN_PROJECT_DIR", "/tmp/project"),
                (
                    "GARDN_DEVIN_LIST_JSON",
                    r#"[{"id":"devin-session","working_directory":"/tmp/project"}]"#,
                ),
            ],
        )
        .is_none(),
        "prompt events must not fall back to potentially stale devin list output"
    );
}
#[test]
fn pane_run_sends_one_send_input_request_with_enter_key() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let socket_path = base.join("gardn.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = thread::spawn(move || {
        let (mut first_stream, first_line) = accept_fake_cli_operation(&listener);
        first_stream
            .write_all(br#"{"id":"cli:request","result":{"type":"ok"}}"#)
            .unwrap();
        first_stream.write_all(b"\n").unwrap();
        first_stream.flush().unwrap();

        let mut second_line = None;
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(250);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut second_stream, _)) => {
                    let mut line = String::new();
                    let mut reader = BufReader::new(second_stream.try_clone().unwrap());
                    reader.read_line(&mut line).unwrap();
                    second_stream
                        .write_all(br#"{"id":"cli:request","result":{"type":"ok"}}"#)
                        .unwrap();
                    second_stream.write_all(b"\n").unwrap();
                    second_stream.flush().unwrap();
                    second_line = Some(line);
                    break;
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => panic!("second accept failed: {err}"),
            }
        }

        (first_line, second_line)
    });

    let run = run_cli(&socket_path, &["pane", "run", "1-1", "echo hello"]);
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let (first_line, second_line) = server.join().unwrap();
    let first_request: serde_json::Value = serde_json::from_str(&first_line).unwrap();
    assert_eq!(first_request["method"], "pane.send_input");
    assert_eq!(first_request["params"]["pane_id"], "1-1");
    assert_eq!(first_request["params"]["text"], "echo hello");
    assert_eq!(
        first_request["params"]["keys"],
        serde_json::json!(["Enter"])
    );
    assert!(
        second_line.is_none(),
        "pane run sent an unexpected second request: {:?}",
        second_line
    );

    cleanup_test_base(&base);
}

#[test]
fn pane_report_metadata_sends_presentation_request() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let socket_path = base.join("gardn.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let server = thread::spawn(move || {
        let (mut stream, line) = accept_fake_cli_operation(&listener);
        stream
            .write_all(br#"{"id":"cli:request","result":{"type":"ok"}}"#)
            .unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
        line
    });

    let run = run_cli(
        &socket_path,
        &[
            "pane",
            "report-metadata",
            "1-1",
            "--source",
            "user:claude-title",
            "--agent",
            "claude",
            "--applies-to-source",
            "gardn:claude",
            "--title",
            "Refactor auth",
            "--display-agent",
            "Claude auth",
            "--custom-status",
            "middleware",
            "--state-label",
            "working=deep in the mines",
            "--ttl-ms",
            "3600000",
        ],
    );
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let line = server.join().unwrap();
    let request: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(request["method"], "pane.report_metadata");
    assert_eq!(request["params"]["pane_id"], "1-1");
    assert_eq!(request["params"]["source"], "user:claude-title");
    assert_eq!(request["params"]["agent"], "claude");
    assert_eq!(request["params"]["applies_to_source"], "gardn:claude");
    assert_eq!(request["params"]["title"], "Refactor auth");
    assert_eq!(request["params"]["display_agent"], "Claude auth");
    assert_eq!(request["params"]["custom_status"], "middleware");
    assert_eq!(
        request["params"]["state_labels"]["working"],
        "deep in the mines"
    );
    assert_eq!(request["params"]["ttl_ms"], 3_600_000);

    cleanup_test_base(&base);
}

#[test]
fn pane_report_metadata_rejects_blank_source_before_socket_request() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let socket_path = base.join("missing.sock");

    let run = run_cli(
        &socket_path,
        &[
            "pane",
            "report-metadata",
            "1-1",
            "--source",
            "   ",
            "--custom-status",
            "middleware",
        ],
    );

    assert_eq!(run.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("missing required --source"),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    cleanup_test_base(&base);
}

#[test]
fn pane_report_metadata_rejects_blank_applies_to_source_before_socket_request() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();
    let socket_path = base.join("missing.sock");

    let run = run_cli(
        &socket_path,
        &[
            "pane",
            "report-metadata",
            "1-1",
            "--source",
            "user:claude-title",
            "--applies-to-source",
            "   ",
            "--custom-status",
            "middleware",
        ],
    );

    assert_eq!(run.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("missing value for --applies-to-source"),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    cleanup_test_base(&base);
}

#[test]
fn help_commands_exit_successfully() {
    let help_cases: &[&[&str]] = &[
        &["-h"],
        &["--help"],
        &["status", "-h"],
        &["server", "-h"],
        &["workspace", "-h"],
        &["tab", "-h"],
        &["pane", "-h"],
        &["wait", "-h"],
        &["session", "-h"],
        &["session", "attach", "-h"],
        &["integration", "-h"],
    ];

    for args in help_cases {
        let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
            .args(*args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "gardn {} failed: status={:?} stdout={} stderr={}",
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn root_help_hides_explicit_client_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("gardn client"),
        "root help should not advertise the internal client command: {stdout}"
    );
}

#[test]
fn explicit_client_command_respects_nested_guard() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .arg("client")
        .env("GARDN_ENV", "1")
        .env("XDG_CONFIG_HOME", &base)
        .env_remove("GARDN_CONFIG_PATH")
        .output()
        .unwrap();

    cleanup_test_base(&base);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nested Gardn is disabled by default"),
        "client should fail at the nested guard before connecting: {stderr}"
    );
}

#[test]
fn removed_show_changelog_flag_fails_before_nested_guard() {
    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .arg("--show-changelog")
        .env("GARDN_ENV", "1")
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown option: --show-changelog"),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("nested gardn"),
        "unknown flag should be rejected before nested guard: {stderr}"
    );
}

#[test]
fn named_sessions_use_separate_servers_and_workspace_state() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");

    let alpha = spawn_named_server(&config_home, &runtime_dir, "alpha");
    let beta = spawn_named_server(&config_home, &runtime_dir, "beta");

    wait_for_socket(
        &named_session_socket(&config_home, "alpha"),
        Duration::from_secs(5),
    );
    wait_for_socket(
        &named_session_socket(&config_home, "beta"),
        Duration::from_secs(5),
    );

    run_named_cli_json(
        &config_home,
        &runtime_dir,
        &[
            "--session",
            "alpha",
            "workspace",
            "create",
            "--label",
            "alpha-ws",
            "--no-focus",
        ],
    );
    run_named_cli_json(
        &config_home,
        &runtime_dir,
        &[
            "--session",
            "beta",
            "workspace",
            "create",
            "--label",
            "beta-ws",
            "--no-focus",
        ],
    );

    let alpha_list = run_named_cli_json(
        &config_home,
        &runtime_dir,
        &["--session", "alpha", "workspace", "list"],
    );
    let beta_list = run_named_cli_json(
        &config_home,
        &runtime_dir,
        &["--session", "beta", "workspace", "list"],
    );

    let alpha_labels: Vec<_> = alpha_list["result"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|workspace| workspace["label"].as_str().unwrap())
        .collect();
    let beta_labels: Vec<_> = beta_list["result"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|workspace| workspace["label"].as_str().unwrap())
        .collect();

    assert_eq!(alpha_labels, vec!["alpha-ws"]);
    assert_eq!(beta_labels, vec!["beta-ws"]);

    let beta_via_explicit_session = run_named_cli_with_socket_override(
        &config_home,
        &runtime_dir,
        &["--session", "beta", "workspace", "list"],
        Some(&named_session_socket(&config_home, "alpha")),
    );
    assert!(
        beta_via_explicit_session.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&beta_via_explicit_session.stderr)
    );
    let beta_via_explicit_session: serde_json::Value =
        serde_json::from_slice(&beta_via_explicit_session.stdout).unwrap();
    let labels_via_explicit: Vec<_> = beta_via_explicit_session["result"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|workspace| workspace["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels_via_explicit, vec!["beta-ws"]);

    let human_sessions = run_named_cli(&config_home, &runtime_dir, &["session", "list"]);
    assert!(human_sessions.status.success());
    let human_sessions = String::from_utf8_lossy(&human_sessions.stdout);
    assert!(human_sessions.contains("name"), "stdout: {human_sessions}");
    assert!(
        human_sessions.contains("status"),
        "stdout: {human_sessions}"
    );
    assert!(human_sessions.contains("alpha"), "stdout: {human_sessions}");
    assert!(
        human_sessions.contains("running"),
        "stdout: {human_sessions}"
    );
    assert!(
        human_sessions.contains("/sessions/beta"),
        "stdout: {human_sessions}"
    );

    let sessions = run_named_cli_json(&config_home, &runtime_dir, &["session", "list", "--json"]);
    let sessions = sessions["sessions"].as_array().unwrap();
    let default_session = sessions
        .iter()
        .find(|session| session["name"] == "default")
        .unwrap();
    let alpha_session = sessions
        .iter()
        .find(|session| session["name"] == "alpha")
        .unwrap();
    let beta_session = sessions
        .iter()
        .find(|session| session["name"] == "beta")
        .unwrap();
    assert_eq!(default_session["default"], true);
    assert_eq!(default_session["running"], false);
    assert_eq!(alpha_session["running"], true);
    assert_eq!(beta_session["running"], true);
    assert!(alpha_session["socket_path"]
        .as_str()
        .unwrap()
        .ends_with("/sessions/alpha/gardn.sock"));
    assert!(beta_session["session_dir"]
        .as_str()
        .unwrap()
        .ends_with("/sessions/beta"));

    let delete_running = run_named_cli(&config_home, &runtime_dir, &["session", "delete", "alpha"]);
    assert_eq!(delete_running.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&delete_running.stderr).contains("stop it before deleting"),
        "stderr: {}",
        String::from_utf8_lossy(&delete_running.stderr)
    );

    let delete_default = run_named_cli(
        &config_home,
        &runtime_dir,
        &["session", "delete", "default"],
    );
    assert_eq!(delete_default.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&delete_default.stderr).contains("default session"),
        "stderr: {}",
        String::from_utf8_lossy(&delete_default.stderr)
    );

    let stopped_alpha = run_named_cli_json(
        &config_home,
        &runtime_dir,
        &["session", "stop", "alpha", "--json"],
    );
    assert_eq!(stopped_alpha["stopped"], true);
    assert_eq!(stopped_alpha["session"]["running"], false);

    let deleted_alpha = run_named_cli_json(
        &config_home,
        &runtime_dir,
        &["session", "delete", "alpha", "--json"],
    );
    assert_eq!(deleted_alpha["deleted"], true);
    assert!(!config_home
        .join(app_dir_name())
        .join("sessions")
        .join("alpha")
        .exists());

    let _ = run_named_cli(&config_home, &runtime_dir, &["session", "stop", "beta"]);
    drop(alpha);
    drop(beta);
    cleanup_test_base(&base);
}

#[test]
fn integration_commands_run_locally_when_server_is_missing() {
    let base = unique_test_dir();
    let home_dir = base.join("home");
    let extensions_dir = home_dir.join(".pi/agent/extensions");
    let omp_extensions_dir = home_dir.join(".omp/agent/extensions");
    fs::create_dir_all(&extensions_dir).unwrap();
    fs::create_dir_all(&omp_extensions_dir).unwrap();

    let runtime_dir = base.join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);
    let missing_socket = runtime_dir.join("missing.sock");

    let expected_extension = extensions_dir.join("gardn-pi-agent-state.ts");
    let expected_omp_extension = omp_extensions_dir.join("gardn-omp-agent-state.ts");
    assert!(
        !expected_extension.exists(),
        "test setup should start without extension file"
    );

    let workspace_list = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["workspace", "list"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .output()
        .unwrap();
    assert_eq!(workspace_list.status.code(), Some(1));

    let integration_install = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "install", "pi"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("PI_CONFIG_DIR")
        .output()
        .unwrap();
    assert_eq!(integration_install.status.code(), Some(0));
    assert!(
        expected_extension.exists(),
        "integration install should write local files without a server"
    );

    let omp_integration_install = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "install", "omp"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("PI_CONFIG_DIR")
        .output()
        .unwrap();
    assert_eq!(omp_integration_install.status.code(), Some(0));
    assert!(
        expected_omp_extension.exists(),
        "omp integration install should write local files without a server"
    );
    let omp_content = fs::read_to_string(&expected_omp_extension).unwrap();
    assert!(omp_content.contains("GARDN_INTEGRATION_ID=omp"));
    assert!(omp_content.contains("GARDN_INTEGRATION_VERSION=9"));
    assert!(omp_content.contains("agent: \"omp\""));

    let integration_status = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "status"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("PI_CONFIG_DIR")
        .output()
        .unwrap();
    assert_eq!(integration_status.status.code(), Some(0));
    let status_stdout = String::from_utf8_lossy(&integration_status.stdout);
    assert!(status_stdout.contains("pi: current (v7)"));
    assert!(status_stdout.contains("claude: not installed"));
    assert!(status_stdout.contains("omp: current (v8)"));

    let integration_uninstall = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "uninstall", "pi"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("PI_CONFIG_DIR")
        .output()
        .unwrap();
    assert_eq!(integration_uninstall.status.code(), Some(0));
    assert!(
        !expected_extension.exists(),
        "integration uninstall should remove local files without a server"
    );
    assert!(
        expected_omp_extension.exists(),
        "uninstalling pi must not remove the separate omp integration"
    );

    let omp_integration_uninstall = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "uninstall", "omp"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .env_remove("PI_CODING_AGENT_DIR")
        .env_remove("PI_CONFIG_DIR")
        .output()
        .unwrap();
    assert_eq!(omp_integration_uninstall.status.code(), Some(0));
    assert!(
        !expected_omp_extension.exists(),
        "omp integration uninstall should remove local files without a server"
    );

    cleanup_test_base(&base);
}

#[test]
fn integration_status_outdated_only_prints_action_for_legacy_install() {
    let base = unique_test_dir();
    let home_dir = base.join("home");
    let extensions_dir = home_dir.join(".pi/agent/extensions");
    fs::create_dir_all(&extensions_dir).unwrap();
    fs::write(
        extensions_dir.join("gardn-pi-agent-state.ts"),
        "// installed by Gardn\n// GARDN_INTEGRATION_ID=pi\n// GARDN_INTEGRATION_VERSION=1\n",
    )
    .unwrap();

    let runtime_dir = base.join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);
    let missing_socket = runtime_dir.join("missing.sock");

    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "status", "--outdated-only"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("installed Gardn integrations need updating"));
    assert!(stderr.contains("gardn integration install pi"));

    cleanup_test_base(&base);
}

#[test]
fn integration_status_rejects_unknown_flags() {
    let base = unique_test_dir();
    let home_dir = base.join("home");
    fs::create_dir_all(&home_dir).unwrap();
    let runtime_dir = base.join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);
    let missing_socket = runtime_dir.join("missing.sock");

    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["integration", "status", "--wat"])
        .env("GARDN_SOCKET_PATH", &missing_socket)
        .env("HOME", &home_dir)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));

    cleanup_test_base(&base);
}

#[test]
fn status_commands_report_client_and_server_versions() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let full = run_cli(&socket_path, &["status"]);
    assert!(
        full.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&full.stderr)
    );
    let full_stdout = String::from_utf8_lossy(&full.stdout);
    assert!(full_stdout.contains("client:\n"), "stdout: {full_stdout}");
    assert!(
        full_stdout.contains(&format!("  version: {}", env!("CARGO_PKG_VERSION"))),
        "stdout: {full_stdout}"
    );
    assert!(
        full_stdout.contains("  protocol: 13"),
        "stdout: {full_stdout}"
    );
    assert!(full_stdout.contains("server:\n"), "stdout: {full_stdout}");
    assert!(
        full_stdout.contains("  status: running"),
        "stdout: {full_stdout}"
    );
    assert!(
        full_stdout.contains("  compatible: yes"),
        "stdout: {full_stdout}"
    );
    assert!(
        full_stdout.contains("  restart_needed: no"),
        "stdout: {full_stdout}"
    );
    assert!(
        full_stdout.contains(&socket_path.display().to_string()),
        "stdout: {full_stdout}"
    );

    let server = run_cli(&socket_path, &["status", "server"]);
    assert!(server.status.success());
    let server_stdout = String::from_utf8_lossy(&server.stdout);
    assert!(
        server_stdout.contains("status: running"),
        "stdout: {server_stdout}"
    );
    assert!(
        server_stdout.contains(&format!("version: {}", env!("CARGO_PKG_VERSION"))),
        "stdout: {server_stdout}"
    );
    assert!(
        server_stdout.contains("protocol: 13"),
        "stdout: {server_stdout}"
    );

    let client = run_cli(&socket_path, &["status", "client"]);
    assert!(client.status.success());
    let client_stdout = String::from_utf8_lossy(&client.stdout);
    assert!(
        client_stdout.contains(&format!("version: {}", env!("CARGO_PKG_VERSION"))),
        "stdout: {client_stdout}"
    );
    assert!(
        client_stdout.contains("protocol: 13"),
        "stdout: {client_stdout}"
    );
    assert!(
        client_stdout.contains("binary: "),
        "stdout: {client_stdout}"
    );

    let full_json = run_cli_json(&socket_path, &["status", "--json"]);
    assert_eq!(full_json["client"]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(full_json["client"]["protocol"], 13);
    assert_eq!(full_json["server"]["status"], "running");
    assert_eq!(full_json["server"]["running"], true);
    assert_eq!(full_json["server"]["compatible"], true);
    assert_eq!(
        full_json["server"]["socket"],
        socket_path.display().to_string()
    );
    assert_eq!(full_json["server"]["restart_needed"], false);
    assert_eq!(full_json["update"]["restart_needed"], false);

    let server_json = run_cli_json(&socket_path, &["status", "server", "--json"]);
    assert_eq!(server_json["status"], "running");
    assert_eq!(server_json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(server_json["protocol"], 13);
    assert_eq!(server_json["compatible"], true);

    let client_json = run_cli_json(&socket_path, &["status", "client", "--json"]);
    assert_eq!(client_json["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(client_json["protocol"], 13);
    assert!(client_json["binary"]
        .as_str()
        .is_some_and(|path| !path.is_empty()));

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn status_reports_not_running_when_server_socket_is_missing() {
    let base = unique_test_dir();
    let runtime_dir = base.join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);
    let socket_path = runtime_dir.join("missing.sock");

    let status = run_cli(&socket_path, &["status"]);
    assert!(status.status.success());
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("  status: not running"), "stdout: {stdout}");
    assert!(stdout.contains("  restart_needed: no"), "stdout: {stdout}");
    assert!(
        stdout.contains(&socket_path.display().to_string()),
        "stdout: {stdout}"
    );

    let status_json = run_cli_json(&socket_path, &["status", "--json"]);
    assert_eq!(status_json["server"]["status"], "not_running");
    assert_eq!(status_json["server"]["running"], false);
    assert_eq!(
        status_json["server"]["socket"],
        socket_path.display().to_string()
    );
    assert_eq!(status_json["server"]["restart_needed"], false);
    assert_eq!(status_json["update"]["restart_needed"], false);

    cleanup_test_base(&base);
}

#[test]
fn server_stop_command_shuts_down_running_server() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let mut gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));
    wait_for_socket(&client_socket, Duration::from_secs(5));

    let stopped = run_cli(&socket_path, &["server", "stop"]);
    assert!(
        stopped.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert!(
        stopped.stdout.is_empty(),
        "server stop should not print stdout: {}",
        String::from_utf8_lossy(&stopped.stdout)
    );

    assert!(
        !socket_path.exists() || UnixStream::connect(&socket_path).is_err(),
        "api socket should be removed or stale before server stop returns"
    );
    assert!(
        !client_socket.exists() || UnixStream::connect(&client_socket).is_err(),
        "client socket should be removed or stale before server stop returns"
    );
    let pid = gardn.child.process_id();
    let exit_status = gardn.child.wait().unwrap();
    unregister_spawned_gardn_pid(pid);
    assert!(exit_status.success(), "server stop should exit cleanly");

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn server_stop_then_restart_restores_pane_history() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");
    let marker = "PERSISTED_HISTORY_AFTER_STOP";

    let mut gardn = spawn_gardn_with_pane_history(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));
    wait_for_socket(&client_socket, Duration::from_secs(5));

    let created = run_cli_json(
        &socket_path,
        &[
            "workspace",
            "create",
            "--cwd",
            base.to_str().expect("test path should be utf-8"),
            "--label",
            "history-restart",
        ],
    );
    let pane_id = created["result"]["root_pane"]["pane_id"]
        .as_str()
        .expect("workspace create should return root pane id")
        .to_string();
    let sent = run_cli(
        &socket_path,
        &["pane", "send-text", &pane_id, &format!("echo {marker}\n")],
    );
    assert!(
        sent.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&sent.stderr)
    );
    assert!(
        wait_until(Duration::from_secs(3), Duration::from_millis(25), || {
            pane_read_recent_contains(&socket_path, &pane_id, marker)
        }),
        "pane should contain marker before server stop"
    );

    let stopped = run_cli(&socket_path, &["server", "stop"]);
    assert!(
        stopped.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stopped.stderr)
    );

    let pid = gardn.child.process_id();
    let exit_status = gardn.child.wait().unwrap();
    unregister_spawned_gardn_pid(pid);
    assert!(exit_status.success(), "server stop should exit cleanly");
    drop(gardn);

    let restarted = spawn_gardn_with_pane_history(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));
    wait_for_socket(&client_socket, Duration::from_secs(5));

    let workspaces = run_cli_json(&socket_path, &["workspace", "list"]);
    let workspace_id = workspaces["result"]["workspaces"]
        .as_array()
        .expect("workspace.list should return workspaces")
        .iter()
        .find(|workspace| workspace["label"] == "history-restart")
        .and_then(|workspace| workspace["workspace_id"].as_str())
        .expect("restored workspace should exist")
        .to_string();
    let panes = run_cli_json(
        &socket_path,
        &["pane", "list", "--workspace", &workspace_id],
    );
    let restored_pane_id = panes["result"]["panes"]
        .as_array()
        .expect("pane.list should return panes")
        .first()
        .and_then(|pane| pane["pane_id"].as_str())
        .expect("restored pane should exist")
        .to_string();

    assert!(
        wait_until(Duration::from_secs(3), Duration::from_millis(25), || {
            pane_read_recent_contains(&socket_path, &restored_pane_id, marker)
        }),
        "restarted server should restore saved pane history"
    );

    cleanup_spawned_gardn(restarted, base);
}

#[test]
fn workspace_and_pane_management_commands_work() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let reloaded = run_cli(&socket_path, &["server", "reload-config"]);
    assert!(
        reloaded.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&reloaded.stderr)
    );
    let reload_json: serde_json::Value = serde_json::from_slice(&reloaded.stdout).unwrap();
    assert_eq!(reload_json["result"]["type"], "config_reload");
    assert_eq!(reload_json["result"]["status"], "applied");

    let manifests = run_cli(&socket_path, &["server", "agent-manifests"]);
    assert!(
        manifests.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&manifests.stderr)
    );
    let manifests_json: serde_json::Value = serde_json::from_slice(&manifests.stdout).unwrap();
    assert_eq!(manifests_json["result"]["type"], "agent_manifest_status");
    assert!(manifests_json["result"]["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .any(|manifest| manifest["agent"] == "omp"));

    let reloaded_manifests = run_cli(&socket_path, &["server", "reload-agent-manifests"]);
    assert!(
        reloaded_manifests.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&reloaded_manifests.stderr)
    );
    let reloaded_manifests_json: serde_json::Value =
        serde_json::from_slice(&reloaded_manifests.stdout).unwrap();
    assert_eq!(
        reloaded_manifests_json["result"]["type"],
        "agent_manifest_reload"
    );

    let listed = run_cli(&socket_path, &["workspace", "list"]);
    assert!(listed.status.success());
    let listed_json: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed_json["result"]["type"], "workspace_list");
    assert_eq!(
        listed_json["result"]["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());
    let created_json: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let workspace_id = created_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();

    let panes = run_cli(&socket_path, &["pane", "list", "--workspace", "1"]);
    assert!(panes.status.success());
    let panes_json: serde_json::Value = serde_json::from_slice(&panes.stdout).unwrap();
    assert_eq!(panes_json["result"]["panes"].as_array().unwrap().len(), 1);

    let split = run_cli(
        &socket_path,
        &["pane", "split", "1-1", "--direction", "right"],
    );
    assert!(
        split.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&split.stderr)
    );
    let split_json: serde_json::Value = serde_json::from_slice(&split.stdout).unwrap();
    let split_pane_id = split_json["result"]["pane"]["pane_id"].as_str().unwrap();

    let fetched = run_cli(&socket_path, &["pane", "get", split_pane_id]);
    assert!(fetched.status.success());
    let fetched_json: serde_json::Value = serde_json::from_slice(&fetched.stdout).unwrap();
    assert_eq!(fetched_json["result"]["pane"]["pane_id"], split_pane_id);

    let closed = run_cli(&socket_path, &["pane", "close", split_pane_id]);
    assert!(closed.status.success());
    let closed_json: serde_json::Value = serde_json::from_slice(&closed.stdout).unwrap();
    assert_eq!(closed_json["result"]["type"], "ok");

    let renamed = run_cli(
        &socket_path,
        &["workspace", "rename", &workspace_id, "demo"],
    );
    assert!(renamed.status.success());
    let renamed_json: serde_json::Value = serde_json::from_slice(&renamed.stdout).unwrap();
    assert_eq!(renamed_json["result"]["workspace"]["label"], "demo");

    let focused = run_cli(&socket_path, &["workspace", "focus", &workspace_id]);
    assert!(focused.status.success());

    let closed_workspace = run_cli(&socket_path, &["workspace", "close", &workspace_id]);
    assert!(closed_workspace.status.success());
    let closed_workspace_json: serde_json::Value =
        serde_json::from_slice(&closed_workspace.stdout).unwrap();
    assert_eq!(closed_workspace_json["result"]["type"], "ok");

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn tab_management_commands_work() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());
    let created_json: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let workspace_id = created_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_tab_id = created_json["result"]["workspace"]["active_tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_tab = run_cli(
        &socket_path,
        &["tab", "create", "--workspace", &workspace_id],
    );
    assert!(created_tab.status.success());
    let created_tab_json: serde_json::Value = serde_json::from_slice(&created_tab.stdout).unwrap();
    let second_tab_id = created_tab_json["result"]["tab"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string();

    let listed_tabs = run_cli(&socket_path, &["tab", "list", "--workspace", &workspace_id]);
    assert!(listed_tabs.status.success());
    let listed_tabs_json: serde_json::Value = serde_json::from_slice(&listed_tabs.stdout).unwrap();
    assert_eq!(
        listed_tabs_json["result"]["tabs"].as_array().unwrap().len(),
        2
    );

    let renamed_tab = run_cli(&socket_path, &["tab", "rename", &second_tab_id, "logs"]);
    assert!(renamed_tab.status.success());
    let renamed_tab_json: serde_json::Value = serde_json::from_slice(&renamed_tab.stdout).unwrap();
    assert_eq!(renamed_tab_json["result"]["tab"]["label"], "logs");

    let focused_tab = run_cli(&socket_path, &["tab", "focus", &first_tab_id]);
    assert!(focused_tab.status.success());
    let focused_tab_json: serde_json::Value = serde_json::from_slice(&focused_tab.stdout).unwrap();
    assert_eq!(focused_tab_json["result"]["tab"]["tab_id"], first_tab_id);

    let tab_get = run_cli(&socket_path, &["tab", "get", &second_tab_id]);
    assert!(tab_get.status.success());
    let tab_get_json: serde_json::Value = serde_json::from_slice(&tab_get.stdout).unwrap();
    assert_eq!(tab_get_json["result"]["tab"]["tab_id"], second_tab_id);

    let closed_tab = run_cli(&socket_path, &["tab", "close", &second_tab_id]);
    assert!(closed_tab.status.success());
    let closed_tab_json: serde_json::Value = serde_json::from_slice(&closed_tab.stdout).unwrap();
    assert_eq!(closed_tab_json["result"]["type"], "ok");

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn agent_start_command_works() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let started = run_cli_json(
        &socket_path,
        &[
            "agent",
            "start",
            "main",
            "--cwd",
            base.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            "printf cli-agent-start-ok; sleep 2",
        ],
    );
    assert_eq!(started["result"]["type"], "agent_started");
    assert_eq!(started["result"]["agent"]["name"], "main");
    assert_eq!(started["result"]["argv"][0], "/bin/sh");
    let terminal_id = started["result"]["agent"]["terminal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let listed = run_cli_json(&socket_path, &["agent", "list"]);
    assert_eq!(listed["result"]["agents"][0]["terminal_id"], terminal_id);
    assert_eq!(listed["result"]["agents"][0]["name"], "main");

    let duplicate = run_cli(
        &socket_path,
        &[
            "agent",
            "start",
            "main",
            "--cwd",
            base.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            "true",
        ],
    );
    assert!(!duplicate.status.success());
    let duplicate_json: serde_json::Value = serde_json::from_slice(&duplicate.stderr).unwrap();
    assert_eq!(duplicate_json["error"]["code"], "agent_name_taken");

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn agent_commands_work() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let started = run_cli_json(
        &socket_path,
        &[
            "agent",
            "start",
            "initial",
            "--cwd",
            base.to_str().unwrap(),
            "--",
            "/bin/sh",
        ],
    );
    let root_pane_id = started["result"]["agent"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();
    let terminal_id = started["result"]["agent"]["terminal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let renamed = run_cli(&socket_path, &["agent", "rename", &root_pane_id, "worker"]);
    assert!(
        renamed.status.success(),
        "agent rename failed: {}",
        String::from_utf8_lossy(&renamed.stderr)
    );

    let listed = run_cli_json(&socket_path, &["agent", "list"]);
    assert_eq!(listed["result"]["type"], "agent_list");
    assert_eq!(listed["result"]["agents"][0]["terminal_id"], terminal_id);
    assert_eq!(listed["result"]["agents"][0]["name"], "worker");

    let fetched = run_cli_json(&socket_path, &["agent", "get", "worker"]);
    assert_eq!(fetched["result"]["agent"]["pane_id"], root_pane_id);

    let waited = run_cli_json(
        &socket_path,
        &[
            "agent",
            "wait",
            "worker",
            "--status",
            "unknown",
            "--timeout",
            "100",
        ],
    );
    assert_eq!(waited["result"]["agent"]["pane_id"], root_pane_id);

    let read = run_cli_json(
        &socket_path,
        &["agent", "read", &terminal_id, "--source", "visible"],
    );
    assert_eq!(read["result"]["type"], "pane_read");

    let agent_renamed = run_cli_json(&socket_path, &["agent", "rename", "worker", "reviewer"]);
    assert_eq!(agent_renamed["result"]["agent"]["name"], "reviewer");

    let focused = run_cli_json(&socket_path, &["agent", "focus", "reviewer"]);
    assert_eq!(focused["result"]["agent"]["focused"], true);

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn pane_close_only_removes_the_target_tab_when_other_tabs_exist() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());
    let created_json: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let workspace_id = created_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();

    let created_tab = run_cli(
        &socket_path,
        &["tab", "create", "--workspace", &workspace_id],
    );
    assert!(created_tab.status.success());
    let created_tab_json: serde_json::Value = serde_json::from_slice(&created_tab.stdout).unwrap();
    let second_root_pane_id = created_tab_json["result"]["root_pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let closed = run_cli(&socket_path, &["pane", "close", &second_root_pane_id]);
    assert!(closed.status.success());
    let closed_json: serde_json::Value = serde_json::from_slice(&closed.stdout).unwrap();
    assert_eq!(closed_json["result"]["type"], "ok");

    let workspaces = run_cli(&socket_path, &["workspace", "list"]);
    assert!(workspaces.status.success());
    let workspaces_json: serde_json::Value = serde_json::from_slice(&workspaces.stdout).unwrap();
    assert_eq!(
        workspaces_json["result"]["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        workspaces_json["result"]["workspaces"][0]["workspace_id"],
        workspace_id
    );

    let tabs = run_cli(&socket_path, &["tab", "list", "--workspace", &workspace_id]);
    assert!(tabs.status.success());
    let tabs_json: serde_json::Value = serde_json::from_slice(&tabs.stdout).unwrap();
    assert_eq!(tabs_json["result"]["tabs"].as_array().unwrap().len(), 1);

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn pane_close_deletes_the_workspace_when_it_closes_the_last_pane() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());
    let created_json: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    let root_pane_id = created_json["result"]["root_pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let closed = run_cli(&socket_path, &["pane", "close", &root_pane_id]);
    assert!(closed.status.success());
    let closed_json: serde_json::Value = serde_json::from_slice(&closed.stdout).unwrap();
    assert_eq!(closed_json["result"]["type"], "ok");

    let workspaces = run_cli(&socket_path, &["workspace", "list"]);
    assert!(workspaces.status.success());
    let workspaces_json: serde_json::Value = serde_json::from_slice(&workspaces.stdout).unwrap();
    let workspaces = workspaces_json["result"]["workspaces"].as_array().unwrap();
    assert!(workspaces.is_empty());

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn pane_run_read_and_wait_commands_work() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = send_request(
        &socket_path,
        &format!(
            r#"{{"id":"req_cli_1","method":"workspace.create","params":{{"cwd":"{}","focus":true}}}}"#,
            base.display()
        ),
    );
    assert!(created["result"]["workspace"]["workspace_id"].is_string());

    let create = run_cli(
        &socket_path,
        &[
            "pane",
            "run",
            "1-1",
            "echo alpha && echo beta && printf 'ready\\n'",
        ],
    );
    assert!(create.status.success());

    let waited = run_cli(
        &socket_path,
        &[
            "wait",
            "output",
            "1-1",
            "--match",
            "ready",
            "--source",
            "recent",
            "--lines",
            "40",
            "--timeout",
            "5000",
        ],
    );
    assert!(
        waited.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&waited.stderr)
    );
    let waited_json: serde_json::Value = serde_json::from_slice(&waited.stdout).unwrap();
    assert_eq!(waited_json["result"]["type"], "output_matched");

    let read = run_cli(
        &socket_path,
        &["pane", "read", "1-1", "--source", "recent", "--lines", "40"],
    );
    assert!(read.status.success());
    let text = String::from_utf8(read.stdout).unwrap();
    assert!(text.contains("alpha"));
    assert!(text.contains("ready"));

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn wait_output_matches_recent_unwrapped_text() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());

    let token = "WRAP_WAIT_TEST_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
    let script = base.join("emit-long-token.sh");
    std::fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n' '{token}'\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();
    }

    let run = run_cli(
        &socket_path,
        &["pane", "run", "1-1", &format!("sh {}", script.display())],
    );
    assert!(run.status.success());

    let waited = run_cli(
        &socket_path,
        &[
            "wait",
            "output",
            "1-1",
            "--match",
            token,
            "--source",
            "recent",
            "--lines",
            "80",
            "--timeout",
            "5000",
        ],
    );
    assert!(
        waited.status.success(),
        "stderr: {} stdout: {}",
        String::from_utf8_lossy(&waited.stderr),
        String::from_utf8_lossy(&waited.stdout)
    );

    let read = run_cli(
        &socket_path,
        &[
            "pane",
            "read",
            "1-1",
            "--source",
            "recent-unwrapped",
            "--lines",
            "80",
        ],
    );
    assert!(read.status.success());
    let text = String::from_utf8(read.stdout).unwrap();
    assert!(text.contains(token));

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn closing_pane_terminates_processes_inside_it() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());

    let split = run_cli(
        &socket_path,
        &["pane", "split", "1-1", "--direction", "right"],
    );
    assert!(split.status.success());
    let split_json: serde_json::Value = serde_json::from_slice(&split.stdout).unwrap();
    let pane_id = split_json["result"]["pane"]["pane_id"].as_str().unwrap();

    let pid_file = base.join("pane-close.pid");
    let command = format!(
        "python3 -c 'import os,time,pathlib; pathlib.Path(r\"{}\").write_text(str(os.getpid())); time.sleep(1000)'",
        pid_file.display()
    );
    let ran = run_cli(&socket_path, &["pane", "run", pane_id, &command]);
    assert!(
        ran.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ran.stderr)
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !pid_file.exists() {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(pid_file.exists(), "pid file was not created");

    let pid = wait_for_pid_file(&pid_file, Duration::from_secs(3)).unwrap_or_else(|err| {
        panic!("failed to read pane child pid: {err}");
    });
    assert!(process_exists(pid), "child process was not running");

    let closed = run_cli(&socket_path, &["pane", "close", pane_id]);
    assert!(
        closed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&closed.stderr)
    );
    assert!(
        wait_for_pid_exit(pid, Duration::from_secs(3)),
        "process {pid} survived pane close"
    );

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn closing_workspace_terminates_processes_inside_it() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = run_cli(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    assert!(created.status.success());

    let pid_file = base.join("workspace-close.pid");
    let command = format!(
        "python3 -c 'import os,time,pathlib; pathlib.Path(r\"{}\").write_text(str(os.getpid())); time.sleep(1000)'",
        pid_file.display()
    );
    let ran = run_cli(&socket_path, &["pane", "run", "1-1", &command]);
    assert!(
        ran.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ran.stderr)
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !pid_file.exists() {
        thread::sleep(Duration::from_millis(25));
    }
    assert!(pid_file.exists(), "pid file was not created");

    let pid = wait_for_pid_file(&pid_file, Duration::from_secs(3)).unwrap_or_else(|err| {
        panic!("failed to read pane child pid: {err}");
    });
    assert!(process_exists(pid), "child process was not running");

    let closed = run_cli(&socket_path, &["workspace", "close", "1"]);
    assert!(
        closed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&closed.stderr)
    );
    assert!(
        wait_for_pid_exit(pid, Duration::from_secs(3)),
        "process {pid} survived workspace close"
    );

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn workspace_ids_are_stable_and_pane_aliases_stay_compact() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let ws1_json = run_cli_json(
        &socket_path,
        &["workspace", "create", "--cwd", base.to_str().unwrap()],
    );
    let ws1_id = ws1_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let ws1_root_pane_id = ws1_json["result"]["root_pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let split_12_json = run_cli_json(
        &socket_path,
        &["pane", "split", "1-1", "--direction", "right", "--no-focus"],
    );
    let split_12_id = split_12_json["result"]["pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let split_13_json = run_cli_json(
        &socket_path,
        &["pane", "split", "1-1", "--direction", "down", "--no-focus"],
    );
    let split_13_id = split_13_json["result"]["pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let ws2_json = run_cli_json(
        &socket_path,
        &["workspace", "create", "--cwd", "/tmp", "--no-focus"],
    );
    let ws2_id = ws2_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(ws2_id, ws1_id);

    let ws2_focus = run_cli(&socket_path, &["workspace", "focus", &ws2_id]);
    assert!(
        ws2_focus.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ws2_focus.stderr)
    );

    let ws2_split_json = run_cli_json(
        &socket_path,
        &["pane", "split", "2-1", "--direction", "right", "--no-focus"],
    );
    assert_eq!(
        ws2_split_json["result"]["pane"]["workspace_id"],
        ws2_json["result"]["workspace"]["workspace_id"]
    );

    let ws3_json = run_cli_json(
        &socket_path,
        &["workspace", "create", "--cwd", "/", "--no-focus"],
    );
    let ws3_id = ws3_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(ws3_id, ws1_id);
    assert_ne!(ws3_id, ws2_id);

    let close_ws2 = run_cli(&socket_path, &["workspace", "close", &ws2_id]);
    assert!(
        close_ws2.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&close_ws2.stderr)
    );

    let workspaces_json = run_cli_json(&socket_path, &["workspace", "list"]);
    let ids: Vec<String> = workspaces_json["result"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|ws| ws["workspace_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec![ws1_id.clone(), ws3_id.clone()]);

    let new_ws_json = run_cli_json(
        &socket_path,
        &["workspace", "create", "--cwd", "/var/tmp", "--no-focus"],
    );
    let new_ws_id = new_ws_json["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(new_ws_id, ws1_id);
    assert_ne!(new_ws_id, ws2_id);
    assert_ne!(new_ws_id, ws3_id);

    let ws3_panes_json = run_cli_json(&socket_path, &["pane", "list", "--workspace", &ws3_id]);
    assert_eq!(ws3_panes_json["result"]["panes"][0]["workspace_id"], ws3_id);

    let close_middle = run_cli(&socket_path, &["pane", "close", &split_12_id]);
    assert!(
        close_middle.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&close_middle.stderr)
    );

    let ws1_panes_json = run_cli_json(&socket_path, &["pane", "list", "--workspace", &ws1_id]);
    let pane_ids: Vec<String> = ws1_panes_json["result"]["panes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pane| pane["pane_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(pane_ids, vec![ws1_root_pane_id, split_13_id]);

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn pane_shell_gets_gardn_socket_and_pane_env() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = send_request(
        &socket_path,
        &format!(
            r#"{{"id":"req_env_1","method":"workspace.create","params":{{"cwd":"{}","focus":true}}}}"#,
            base.display()
        ),
    );
    assert!(created["result"]["workspace"]["workspace_id"].is_string());

    let env_capture = base.join("pane-env.txt");
    let ran = run_cli(
        &socket_path,
        &[
            "pane",
            "run",
            "1-1",
            &format!(
                "printf '%s\\n%s\\n' \"$GARDN_SOCKET_PATH\" \"$GARDN_PANE_ID\" > {}",
                env_capture.display()
            ),
        ],
    );
    assert!(ran.status.success());

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut text = String::new();
    while Instant::now() < deadline {
        if env_capture.exists() {
            text = fs::read_to_string(&env_capture).unwrap();
            if text.contains(&socket_path.display().to_string()) && text.contains(":p") {
                break;
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(env_capture.exists(), "env capture file was not created");
    assert!(
        text.contains(&socket_path.display().to_string()),
        "env file was: {text:?}"
    );
    assert!(text.contains(":p"), "env file was: {text:?}");

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn wait_agent_status_exits_when_idle_status_matches() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");
    let bin_dir = base.join("bin");

    fs::create_dir_all(&bin_dir).unwrap();
    let fake_pi = bin_dir.join("pi");
    fs::write(
        &fake_pi,
        "#!/bin/sh\nprintf 'Working...\\n'\nsleep 1\nprintf '\\033[2J\\033[Hdone\\n'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&fake_pi).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_pi, perms).unwrap();
    }

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let path_override = format!("{}:{}", bin_dir.display(), inherited_path);
    let gardn = spawn_gardn_with_path(
        &config_home,
        &runtime_dir,
        &socket_path,
        Some(Path::new(&path_override)),
    );

    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = send_request(
        &socket_path,
        &format!(
            r#"{{"id":"req_cli_2","method":"workspace.create","params":{{"cwd":"{}","focus":true}}}}"#,
            base.display()
        ),
    );
    assert!(created["result"]["workspace"]["workspace_id"].is_string());
    let pane_id = created["result"]["root_pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let start_pi = run_cli(&socket_path, &["pane", "run", &pane_id, "pi"]);
    assert!(start_pi.status.success());

    let waited = run_cli(
        &socket_path,
        &[
            "wait",
            "agent-status",
            &pane_id,
            "--status",
            "idle",
            "--timeout",
            "5000",
        ],
    );
    assert!(
        waited.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&waited.stderr)
    );
    let waited_json: serde_json::Value = serde_json::from_slice(&waited.stdout).unwrap();
    assert_eq!(waited_json["event"], "pane.agent_status_changed");

    assert!(
        matches!(
            waited_json["data"]["agent_status"].as_str(),
            Some("idle" | "done")
        ),
        "unexpected agent status: {waited_json}"
    );
    assert_eq!(waited_json["data"]["agent"], "pi");

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn plugin_link_list_unlink_cli_smoke_test() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");
    let plugin_dir = base.join("plugins").join("layout");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("gardn-plugin.toml"),
        r#"
id = "example.layout"
name = "Layout"
version = "0.1.0"
min_gardn_version = "0.2.0"
description = "Apply a preferred Gardn layout"

[[actions]]
id = "apply"
title = "Apply layout"
contexts = ["workspace"]
command = ["sh", "-c", "echo layout"]

[[events]]
on = "workspace.created"
command = ["sh", "-c", "echo workspace"]

[[panes]]
id = "board"
title = "Board"
placement = "tab"
command = ["sh", "-c", "sleep 5"]
"#,
    )
    .unwrap();

    let gardn = spawn_gardn(&config_home, &runtime_dir, &socket_path);
    wait_for_socket(&socket_path, Duration::from_secs(5));
    let workspace = run_cli_json(
        &socket_path,
        &[
            "workspace",
            "create",
            "--cwd",
            base.to_str().unwrap(),
            "--focus",
        ],
    );
    assert_eq!(workspace["result"]["type"], "workspace_created");
    let workspace_id = workspace["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();

    let linked = run_cli_json_in_dir(&socket_path, &["plugin", "link", "plugins/layout"], &base);
    assert_eq!(linked["result"]["type"], "plugin_linked");
    assert_eq!(linked["result"]["plugin"]["plugin_id"], "example.layout");
    assert_eq!(linked["result"]["plugin"]["actions"][0]["id"], "apply");
    assert_eq!(
        linked["result"]["plugin"]["events"][0]["on"],
        "workspace.created"
    );
    assert_eq!(linked["result"]["plugin"]["panes"][0]["id"], "board");

    let listed_human = run_cli(&socket_path, &["plugin", "list"]);
    assert!(listed_human.status.success());
    assert!(String::from_utf8_lossy(&listed_human.stdout).contains("example.layout"));

    let listed = run_cli_json(&socket_path, &["plugin", "list", "--json"]);
    assert_eq!(listed["result"]["type"], "plugin_list");
    let plugin_ids: Vec<_> = listed["result"]["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|plugin| plugin["plugin_id"].as_str())
        .collect();
    assert!(plugin_ids.contains(&"example.layout"));

    let invoked = run_cli_json(
        &socket_path,
        &[
            "plugin",
            "action",
            "invoke",
            "apply",
            "--plugin",
            "example.layout",
        ],
    );
    assert_eq!(invoked["result"]["type"], "plugin_action_invoked");
    assert_eq!(invoked["result"]["action"]["action_id"], "apply");

    let pane = run_cli_json(
        &socket_path,
        &[
            "plugin",
            "pane",
            "open",
            "--plugin",
            "example.layout",
            "--entrypoint",
            "board",
            "--workspace",
            &workspace_id,
            "--env",
            "GARDN_ROLE=board",
            "--no-focus",
        ],
    );
    assert_eq!(pane["result"]["type"], "plugin_pane_opened");
    assert_eq!(pane["result"]["plugin_pane"]["entrypoint"], "board");

    let missing_plugin_value = run_cli(&socket_path, &["plugin", "list", "--plugin"]);
    assert_eq!(missing_plugin_value.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&missing_plugin_value.stderr)
        .contains("missing value for --plugin"));

    let invalid_limit = run_cli(
        &socket_path,
        &["plugin", "log", "list", "--limit", "not-a-number"],
    );
    assert_eq!(invalid_limit.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid_limit.stderr).contains("invalid --limit value"));

    let unlinked = run_cli_json(&socket_path, &["plugin", "unlink", "example.layout"]);
    assert_eq!(unlinked["result"]["type"], "plugin_unlinked");
    assert_eq!(unlinked["result"]["removed"], true);

    let listed = run_cli_json(&socket_path, &["plugin", "list", "--json"]);
    assert!(listed["result"]["plugins"].as_array().unwrap().is_empty());

    cleanup_spawned_gardn(gardn, base);
}

#[test]
fn plugin_install_list_uninstall_offline_cli_smoke_test() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let source_repo = base.join("source-repo");
    let plugin_dir = source_repo.join("workspace-bootstrap");
    fs::create_dir_all(&plugin_dir).unwrap();
    create_committed_repo(&source_repo);
    fs::write(
        plugin_dir.join("gardn-plugin.toml"),
        r#"
id = "example.workspace-bootstrap"
name = "Workspace Bootstrap"
version = "0.1.0"
platforms = ["linux", "macos", "windows"]
min_gardn_version = "0.2.0"

[[build]]
command = ["sh", "-c", "echo built > built.txt; if [ -n \"$GARDN_SESSION\" ]; then echo \"$GARDN_SESSION\" > leaked-session.txt; fi"]

[[actions]]
id = "bootstrap"
title = "Bootstrap"
command = ["sh", "-c", "echo bootstrap"]
"#,
    )
    .unwrap();
    run_git(
        &source_repo,
        &["add", "workspace-bootstrap/gardn-plugin.toml"],
    );
    run_git(&source_repo, &["commit", "--quiet", "-m", "add plugin"]);

    fs::create_dir_all(&config_home).unwrap();
    fs::create_dir_all(&runtime_dir).unwrap();
    let git_config = base.join("gitconfig");
    fs::write(
        &git_config,
        format!(
            "[url \"file://{}\"]\n    insteadOf = https://github.com/masakirocorp/gardn-plugin-examples.git\n",
            source_repo.display()
        ),
    )
    .unwrap();

    let install = run_named_cli_with_env(
        &config_home,
        &runtime_dir,
        &[
            "--session",
            "plugins",
            "plugin",
            "install",
            "masakirocorp/gardn-plugin-examples/workspace-bootstrap",
            "--yes",
        ],
        &[
            ("GIT_CONFIG_GLOBAL", &git_config),
            ("GARDN_SESSION", Path::new("leaked-session")),
        ],
    );
    assert!(
        install.status.success(),
        "install failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&install.stdout),
        String::from_utf8_lossy(&install.stderr)
    );

    let listed = run_named_cli_json(
        &config_home,
        &runtime_dir,
        &["--session", "plugins", "plugin", "list", "--json"],
    );
    let plugin = &listed["result"]["plugins"][0];
    assert_eq!(plugin["plugin_id"], "example.workspace-bootstrap");
    assert_eq!(plugin["source"]["kind"], "github");
    assert_eq!(plugin["source"]["owner"], "masakirocorp");
    assert_eq!(plugin["source"]["repo"], "gardn-plugin-examples");
    assert_eq!(plugin["source"]["subdir"], "workspace-bootstrap");
    assert!(plugin["source"]["resolved_commit"].as_str().is_some());
    let managed_path = PathBuf::from(plugin["source"]["managed_path"].as_str().unwrap());
    assert!(managed_path.exists(), "managed checkout should exist");
    assert!(
        managed_path
            .join("workspace-bootstrap")
            .join("built.txt")
            .exists(),
        "build artifact should be preserved in managed checkout"
    );
    assert!(
        !managed_path
            .join("workspace-bootstrap")
            .join("leaked-session.txt")
            .exists(),
        "build command should not inherit GARDN_SESSION"
    );

    let uninstall = run_named_cli(
        &config_home,
        &runtime_dir,
        &[
            "--session",
            "plugins",
            "plugin",
            "uninstall",
            "example.workspace-bootstrap",
        ],
    );
    assert!(
        uninstall.status.success(),
        "uninstall failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&uninstall.stdout),
        String::from_utf8_lossy(&uninstall.stderr)
    );
    assert!(
        !managed_path.exists(),
        "managed checkout should be deleted on uninstall"
    );

    let listed = run_named_cli_json(
        &config_home,
        &runtime_dir,
        &["--session", "plugins", "plugin", "list", "--json"],
    );
    assert!(listed["result"]["plugins"].as_array().unwrap().is_empty());

    cleanup_test_base(&base);
}
#[test]
fn wait_agent_status_exits_when_background_agent_finishes() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let socket_path = runtime_dir.join("gardn.sock");
    let bin_dir = base.join("bin");

    fs::create_dir_all(&bin_dir).unwrap();
    let fake_pi = bin_dir.join("pi");
    fs::write(
        &fake_pi,
        "#!/bin/sh\nprintf 'Working...\\n'\nsleep 1\nprintf '\\033[2J\\033[Hdone\\n'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&fake_pi).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_pi, perms).unwrap();
    }

    let inherited_path = std::env::var("PATH").unwrap_or_default();
    let path_override = format!("{}:{}", bin_dir.display(), inherited_path);
    let gardn = spawn_gardn_with_path(
        &config_home,
        &runtime_dir,
        &socket_path,
        Some(Path::new(&path_override)),
    );

    wait_for_socket(&socket_path, Duration::from_secs(5));

    let created = send_request(
        &socket_path,
        &format!(
            r#"{{"id":"req_cli_status_1","method":"workspace.create","params":{{"cwd":"{}","focus":true}}}}"#,
            base.display()
        ),
    );
    let workspace_id = created["result"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    let pane_id = created["result"]["root_pane"]["pane_id"]
        .as_str()
        .unwrap()
        .to_string();

    let tab_created = send_request(
        &socket_path,
        &format!(
            r#"{{"id":"req_cli_status_2","method":"tab.create","params":{{"workspace_id":"{}","focus":true}}}}"#,
            workspace_id
        ),
    );
    assert_eq!(tab_created["result"]["type"], "tab_created");

    let start_pi = run_cli(&socket_path, &["pane", "run", &pane_id, "pi"]);
    assert!(start_pi.status.success());

    let waited = run_cli(
        &socket_path,
        &[
            "wait",
            "agent-status",
            &pane_id,
            "--status",
            "idle",
            "--timeout",
            "5000",
        ],
    );
    assert!(
        waited.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&waited.stderr)
    );
    let waited_json: serde_json::Value = serde_json::from_slice(&waited.stdout).unwrap();
    assert_eq!(waited_json["event"], "pane.agent_status_changed");
    assert!(
        matches!(
            waited_json["data"]["agent_status"].as_str(),
            Some("idle" | "done")
        ),
        "unexpected agent status: {waited_json}"
    );
    assert_eq!(waited_json["data"]["agent"], "pi");

    cleanup_spawned_gardn(gardn, base);
}
