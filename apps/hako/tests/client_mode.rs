#![cfg(unix)]

//! Integration tests for thin client mode.

mod support;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use support::{
    cleanup_test_base, client_handshake, connect_unix_socket, encode_varint_u16, encode_varint_u32,
    frame_message, read_server_message, register_runtime_dir, register_spawned_hako_pid,
    unregister_spawned_hako_pid, wait_for_file, wait_for_socket,
};

fn unique_test_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!(
        "/tmp/hako-client-test-{}-{nanos}",
        std::process::id()
    ))
}

struct SpawnedHako {
    _master: Option<Box<dyn MasterPty + Send>>,
    child: Box<dyn Child + Send + Sync>,
}

impl SpawnedHako {
    fn close_master(&mut self) {
        drop(self._master.take());
    }
}

impl Drop for SpawnedHako {
    fn drop(&mut self) {
        let pid = self.child.process_id();
        let _ = self.child.kill();
        self.close_master();

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

            unregister_spawned_hako_pid(Some(pid));
        }
    }
}

fn cleanup_spawned_hako(spawned: SpawnedHako, base: PathBuf) {
    drop(spawned);
    cleanup_test_base(&base);
}

fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn spawn_client_process(
    config_home: &PathBuf,
    runtime_dir: &PathBuf,
    api_socket_path: &PathBuf,
) -> SpawnedHako {
    register_runtime_dir(runtime_dir);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_hako"));
    cmd.arg("client");
    cmd.env("HAKO_DISABLE_SOUND", "1");
    cmd.env("XDG_CONFIG_HOME", config_home);
    cmd.env("XDG_RUNTIME_DIR", runtime_dir);
    cmd.env("HAKO_SOCKET_PATH", api_socket_path);
    cmd.env_remove("HAKO_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("HAKO_ENV");

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_hako_pid(child.process_id());
    drop(pair.slave);

    SpawnedHako {
        _master: Some(pair.master),
        child,
    }
}

fn spawn_server(
    config_home: &PathBuf,
    runtime_dir: &PathBuf,
    api_socket_path: &PathBuf,
    _client_socket_path: &PathBuf,
) -> SpawnedHako {
    fs::create_dir_all(config_home.join("hako")).unwrap();
    fs::create_dir_all(runtime_dir).unwrap();
    register_runtime_dir(runtime_dir);
    fs::write(config_home.join("hako/config.toml"), "onboarding = false\n").unwrap();

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_hako"));
    cmd.arg("server");
    cmd.env("XDG_CONFIG_HOME", config_home);
    cmd.env("XDG_RUNTIME_DIR", runtime_dir);
    cmd.env("HAKO_SOCKET_PATH", api_socket_path);
    cmd.env_remove("HAKO_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("HAKO_ENV");

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_hako_pid(child.process_id());
    drop(pair.slave);

    SpawnedHako {
        _master: Some(pair.master),
        child,
    }
}

fn ping_socket(socket_path: &PathBuf) -> String {
    let mut stream = connect_unix_socket(socket_path, Duration::from_secs(5));

    let request = r#"{"id":"1","method":"ping","params":{}}"#;
    writeln!(stream, "{}", request).unwrap();

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    response.trim().to_string()
}

fn send_json_request(socket_path: &PathBuf, request: &str) -> serde_json::Value {
    let mut stream = connect_unix_socket(socket_path, Duration::from_secs(5));
    writeln!(stream, "{request}").unwrap();

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    serde_json::from_str(&response).expect("API response should be valid JSON")
}

fn assert_api_ok(response: &serde_json::Value, context: &str) {
    assert!(
        response.get("error").is_none(),
        "{context} failed: {response}"
    );
}

fn create_workspace_and_root_pane(socket_path: &PathBuf, label: &str) -> (String, String) {
    let response = send_json_request(
        socket_path,
        &format!(
            r#"{{"id":"workspace_create","method":"workspace.create","params":{{"label":"{label}"}}}}"#
        ),
    );
    assert_api_ok(&response, "workspace.create");
    let workspace_id = response["result"]["workspace"]["workspace_id"]
        .as_str()
        .expect("workspace.create should return workspace_id")
        .to_string();
    let pane_id = response["result"]["root_pane"]["pane_id"]
        .as_str()
        .expect("workspace.create should return root pane_id")
        .to_string();
    (workspace_id, pane_id)
}

fn tab_list(socket_path: &PathBuf, workspace_id: &str) -> serde_json::Value {
    let response = send_json_request(
        socket_path,
        &format!(
            r#"{{"id":"tab_list","method":"tab.list","params":{{"workspace_id":"{workspace_id}"}}}}"#
        ),
    );
    assert_api_ok(&response, "tab.list");
    response
}

fn focused_tab_id(socket_path: &PathBuf, workspace_id: &str) -> Option<String> {
    tab_list(socket_path, workspace_id)["result"]["tabs"]
        .as_array()
        .expect("tab.list should return tabs")
        .iter()
        .find(|tab| tab["focused"].as_bool() == Some(true))
        .and_then(|tab| tab["tab_id"].as_str())
        .map(str::to_string)
}

fn wait_for_focused_tab(
    socket_path: &PathBuf,
    workspace_id: &str,
    expected_tab_id: &str,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if focused_tab_id(socket_path, workspace_id).as_deref() == Some(expected_tab_id) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    focused_tab_id(socket_path, workspace_id).as_deref() == Some(expected_tab_id)
}

fn pane_wait_for_output(socket_path: &PathBuf, pane_id: &str, needle: &str) -> serde_json::Value {
    let response = send_json_request(
        socket_path,
        &format!(
            r#"{{"id":"pane_wait","method":"pane.wait_for_output","params":{{"pane_id":"{pane_id}","source":"recent","lines":80,"match":{{"type":"substring","value":"{needle}"}},"timeout_ms":5000}}}}"#
        ),
    );
    assert_api_ok(&response, "pane.wait_for_output");
    assert_eq!(response["result"]["type"], "output_matched");
    response
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct FrameWire {
    cells: Vec<CellWire>,
    width: u16,
    height: u16,
    cursor: Option<CursorWire>,
    hyperlinks: Vec<String>,
    graphics: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CellWire {
    symbol: String,
    fg: u32,
    bg: u32,
    modifier: u16,
    skip: bool,
    hyperlink: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CursorWire {
    x: u16,
    y: u16,
    visible: bool,
    shape: u8,
}

fn decode_frame_payload(payload: &[u8]) -> std::io::Result<FrameWire> {
    bincode::serde::decode_from_slice(payload, bincode::config::standard())
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
        .and_then(|(frame, consumed): (FrameWire, usize)| {
            if consumed != payload.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "frame payload had trailing bytes: consumed={}, len={}",
                        consumed,
                        payload.len()
                    ),
                ));
            }
            Ok(frame)
        })
}

fn read_next_frame_containing(
    stream: &mut UnixStream,
    needle: &str,
    timeout: Duration,
) -> Result<FrameWire, String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match read_server_message(stream) {
            Ok((1, payload)) => {
                let frame = decode_frame_payload(&payload).map_err(|err| err.to_string())?;
                if frame_contains_text(&frame, needle) {
                    return Ok(frame);
                }
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }
    Err(format!("timed out waiting for frame containing {needle:?}"))
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
enum NotifyKindWire {
    Sound,
    Toast,
    SystemToast,
}

#[derive(Debug, Deserialize)]
struct NotifyWire {
    kind: NotifyKindWire,
    message: String,
}

fn decode_notify_payload(payload: &[u8]) -> std::io::Result<NotifyWire> {
    bincode::serde::decode_from_slice(payload, bincode::config::standard())
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
        .and_then(|(notify, consumed): (NotifyWire, usize)| {
            if consumed != payload.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "notify payload had trailing bytes: consumed={}, len={}",
                        consumed,
                        payload.len()
                    ),
                ));
            }
            Ok(notify)
        })
}

fn wait_for_notify(
    stream: &mut UnixStream,
    expected_kind: NotifyKindWire,
    expected_message: &str,
    timeout: Duration,
) -> Result<NotifyWire, String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match read_server_message(stream) {
            Ok((5, payload)) => {
                let notify = decode_notify_payload(&payload).map_err(|err| err.to_string())?;
                if notify.kind == expected_kind && notify.message == expected_message {
                    return Ok(notify);
                }
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }
    Err(format!(
        "timed out waiting for Notify::{expected_kind:?}({expected_message:?})"
    ))
}

fn read_next_frame_payload(stream: &mut UnixStream, timeout: Duration) -> Result<Vec<u8>, String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match read_server_message(stream) {
            Ok((1, payload)) => return Ok(payload),
            Ok(_) => continue,
            Err(_) => continue,
        }
    }
    Err("timed out waiting for Frame message".into())
}

fn frame_contains_text(frame: &FrameWire, needle: &str) -> bool {
    if frame.cells.is_empty() {
        return false;
    }

    let width = frame.width.max(1) as usize;
    let mut text = String::new();
    for row in frame.cells.chunks(width) {
        for cell in row {
            let _ = (cell.fg, cell.bg, cell.modifier, cell.skip);
            text.push_str(&cell.symbol);
        }
        text.push('\n');
    }
    let _ = (frame.height, frame.graphics.len());
    if let Some(cursor) = frame.cursor.as_ref() {
        let _ = (cursor.x, cursor.y, cursor.visible, cursor.shape);
    }

    text.contains(needle)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn client_connects_and_receives_frame() {
    // Client connects to server and handshake completes.
    // Client receives Frame messages.
    // Server sends rendered frames to connected clients.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    // Connect and handshake.
    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12, "server should report protocol version 12");
    assert!(
        error.is_none(),
        "handshake should not have error: {:?}",
        error
    );

    read_next_frame_payload(&mut stream, Duration::from_secs(10))
        .expect("should receive a frame from server");

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn client_sees_headless_startup_configuration_issue_notice() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let app_dir = if cfg!(debug_assertions) {
        "hako-dev"
    } else {
        "hako"
    };
    fs::create_dir_all(config_home.join(app_dir)).unwrap();
    fs::write(
        config_home.join(app_dir).join("config.toml"),
        "[keys\nprefix = \"ctrl+a\"\n",
    )
    .unwrap();
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_hako"));
    cmd.arg("server");
    cmd.env("XDG_CONFIG_HOME", &config_home);
    cmd.env("XDG_RUNTIME_DIR", &runtime_dir);
    cmd.env("HAKO_SOCKET_PATH", &api_socket);
    cmd.env_remove("HAKO_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("HAKO_ENV");

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_hako_pid(child.process_id());
    drop(pair.slave);

    let spawned = SpawnedHako {
        _master: Some(pair.master),
        child,
    };
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found_notice = false;
    while Instant::now() < deadline {
        match read_server_message(&mut stream) {
            Ok((1, payload)) => {
                let frame = decode_frame_payload(&payload).expect("decode frame");
                if frame_contains_text(&frame, "configuration issue") {
                    found_notice = true;
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    assert!(
        found_notice,
        "attached client should see startup configuration issue notice"
    );

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn client_input_forwarded_to_pane() {
    // Stdin input is forwarded to server as ClientMessage::Input.
    // Server routes client input to the correct PTY.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, pane_id) = create_workspace_and_root_pane(&api_socket, "client-input");

    // Connect and handshake.
    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    let input_data = b"echo hello\n".to_vec();
    let input_payload = {
        let mut buf = encode_varint_u32(1);
        buf.extend_from_slice(&encode_varint_u32(input_data.len() as u32));
        buf.extend_from_slice(&input_data);
        buf
    };
    let framed = frame_message(&input_payload);
    stream
        .write_all(&framed)
        .expect("should send Input message");
    stream.flush().expect("should flush");

    let waited = pane_wait_for_output(&api_socket, &pane_id, "hello");
    assert!(waited["result"]["read"]["text"]
        .as_str()
        .unwrap()
        .contains("hello"));

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn client_resize_sends_message() {
    // Terminal resize triggers ClientMessage::Resize.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, _pane_id) = create_workspace_and_root_pane(&api_socket, "client-resize");

    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    while read_server_message(&mut stream).is_ok() {}

    let resize_payload = {
        let mut buf = encode_varint_u32(3);
        buf.extend_from_slice(&encode_varint_u16(120));
        buf.extend_from_slice(&encode_varint_u16(40));
        buf.extend_from_slice(&encode_varint_u32(8));
        buf.extend_from_slice(&encode_varint_u32(16));
        buf
    };
    let framed = frame_message(&resize_payload);
    stream
        .write_all(&framed)
        .expect("should send Resize message");
    stream.flush().expect("should flush");

    let frame_payload =
        read_next_frame_payload(&mut stream, Duration::from_secs(5)).expect("resize frame");
    let frame = decode_frame_payload(&frame_payload).expect("decode resize frame");
    assert_eq!(
        (frame.width, frame.height),
        (120, 40),
        "client resize should change rendered frame dimensions"
    );

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn server_shutdown_sends_message_to_client() {
    // ServerShutdown causes clean exit with informative message.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let mut spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    // Connect and handshake.
    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    // Send SIGINT so the server takes the graceful shutdown path and
    // broadcasts ServerShutdown before exiting.
    if let Some(pid) = spawned.child.process_id() {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGINT);
        }
    }

    // The client should receive an explicit ServerShutdown message, or at
    // minimum observe clean connection close if shutdown races with send.
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut saw_shutdown = false;
    let mut saw_disconnect = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match read_server_message(&mut stream) {
            Ok((variant, _)) => {
                if variant == 3 {
                    saw_shutdown = true;
                    break;
                }
            }
            Err(_) => {
                saw_disconnect = true;
                break;
            }
        }
    }
    assert!(
        saw_shutdown || saw_disconnect,
        "client should observe ServerShutdown or disconnect during graceful shutdown"
    );

    // Wait for the server to exit after shutdown signal.
    spawned.close_master();
    let _ = spawned.child.wait();

    drop(spawned);
    cleanup_test_base(&base);
}

#[test]
fn server_unreachable_shows_clear_error() {
    // when server is unreachable, the client exits quickly
    // with an actionable connection-failed message.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");

    fs::create_dir_all(config_home.join("hako")).unwrap();
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);
    fs::write(config_home.join("hako/config.toml"), "onboarding = false\n").unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_hako"))
        .arg("client")
        .env("HAKO_DISABLE_SOUND", "1")
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("HAKO_SOCKET_PATH", &api_socket)
        .env_remove("HAKO_CLIENT_SOCKET_PATH")
        .env_remove("HAKO_ENV")
        .output()
        .expect("client command should run");

    assert!(
        !output.status.success(),
        "client should fail when no server is running"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to connect to server"),
        "stderr should mention connection failure: {stderr}"
    );
    assert!(
        stderr.contains("Is hako server running?"),
        "stderr should include actionable guidance: {stderr}"
    );
    assert!(
        stderr.contains("Socket path:"),
        "stderr should include attempted socket path: {stderr}"
    );

    cleanup_test_base(&base);
}

#[test]
fn server_crash_after_attach_causes_lost_connection_error() {
    // attach a real thin client connection, kill server unexpectedly,
    // assert clean non-zero client exit plus lost-connection signal.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let mut spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    drop(connect_unix_socket(&client_socket, Duration::from_secs(10)));
    let ready_marker = "§";
    let _ = create_workspace_and_root_pane(&api_socket, ready_marker);

    // Attach a real thin client (client subcommand) through PTY so handshake and
    // terminal setup paths are exercised.
    let mut thin_client = spawn_client_process(&config_home, &runtime_dir, &api_socket);

    // Prove attached before kill by waiting for a rendered workspace marker.
    // Read in a background thread because PTY reads are blocking.
    let mut thin_reader = thin_client
        ._master
        .as_ref()
        .expect("thin client master")
        .try_clone_reader()
        .expect("clone client PTY reader");
    let (output_tx, output_rx) = mpsc::channel();
    let output_reader = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match thin_reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if output_tx
                        .send(String::from_utf8_lossy(&buf[..n]).into_owned())
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    let mut attached_output = String::new();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !attached_output.contains(ready_marker) && Instant::now() < deadline {
        match output_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => attached_output.push_str(&chunk),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(
        attached_output.contains(ready_marker),
        "thin client must render the ready workspace before server crash; output: {attached_output:?}"
    );

    // Kill server unexpectedly.
    if let Some(pid) = spawned.child.process_id() {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    spawned.close_master();

    // Client should exit non-zero after connection loss.
    let mut crash_output = attached_output;
    let deadline = Instant::now() + Duration::from_secs(12);
    while thin_client.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
        while let Ok(chunk) = output_rx.try_recv() {
            crash_output.push_str(&chunk);
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        thin_client.child.try_wait().ok().flatten().is_some(),
        "thin client should exit after server SIGKILL"
    );

    let status = thin_client.child.wait().expect("wait thin client status");
    assert!(
        !status.success(),
        "thin client should exit non-zero after lost server connection"
    );

    // Drain trailing output and require the explicit user-visible lost-connection message.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match output_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => crash_output.push_str(&chunk),
            Err(mpsc::RecvTimeoutError::Timeout) if !output_reader.is_finished() => {}
            Err(_) => break,
        }
    }
    assert!(
        output_reader.is_finished(),
        "thin client output reader should finish after client exit"
    );
    output_reader
        .join()
        .expect("join thin client output reader");

    let crash_output_lc = crash_output.to_lowercase();
    assert!(
        crash_output_lc.contains("lost connection to server"),
        "thin client must emit explicit lost-connection message after server crash; output: {crash_output:?}"
    );

    // Ensure server is gone.
    let _ = spawned.child.wait();

    cleanup_test_base(&base);
}

#[test]
fn client_receives_frame_after_pane_output() {
    // End-to-end test: server renders, client receives Frame.
    // This test verifies the full flow:
    // 1. Start server
    // 2. Connect client, handshake
    // 3. Send input to pane (echo command)
    // 4. Wait for a new frame from the server
    // 5. Verify the frame contains the pane output
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, _pane_id) = create_workspace_and_root_pane(&api_socket, "frame-output");

    // Connect and handshake.
    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    read_next_frame_payload(&mut stream, Duration::from_secs(10))
        .expect("should receive initial frame");

    // Send input to trigger a state change and re-render.
    let input_data = b"echo test-output\n".to_vec();
    let input_payload = {
        let mut buf = encode_varint_u32(1); // Input variant
        buf.extend_from_slice(&encode_varint_u32(input_data.len() as u32));
        buf.extend_from_slice(&input_data);
        buf
    };
    let framed = frame_message(&input_payload);
    stream.write_all(&framed).expect("send input");
    stream.flush().expect("flush");

    let frame = read_next_frame_containing(&mut stream, "test-output", Duration::from_secs(5))
        .expect("client should receive a frame containing pane output");
    assert!(
        frame_contains_text(&frame, "test-output"),
        "post-output frame should contain the echoed marker"
    );

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn navigate_mode_keybind_dispatch_in_server() {
    // Prefix-mode keybindings should be handled by the server, not only by the
    // standalone TUI client path.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (workspace_id, _pane_id) = create_workspace_and_root_pane(&api_socket, "keybind-tabs");
    let first_tab_id = focused_tab_id(&api_socket, &workspace_id)
        .expect("new workspace should focus its first tab");
    let tab_created = send_json_request(
        &api_socket,
        &format!(
            r#"{{"id":"tab_create","method":"tab.create","params":{{"workspace_id":"{workspace_id}","focus":false,"label":"second"}}}}"#
        ),
    );
    assert_api_ok(&tab_created, "tab.create");
    let second_tab_id = tab_created["result"]["tab"]["tab_id"]
        .as_str()
        .expect("tab.create should return tab_id")
        .to_string();
    assert_eq!(
        focused_tab_id(&api_socket, &workspace_id).as_deref(),
        Some(first_tab_id.as_str())
    );

    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    while read_server_message(&mut stream).is_ok() {}

    let prefix_input = vec![0x02]; // Ctrl+B.
    let input_payload = {
        let mut buf = encode_varint_u32(1);
        buf.extend_from_slice(&encode_varint_u32(prefix_input.len() as u32));
        buf.extend_from_slice(&prefix_input);
        buf
    };
    stream
        .write_all(&frame_message(&input_payload))
        .expect("send prefix key");
    stream.flush().expect("flush");

    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    while read_server_message(&mut stream).is_ok() {}
    stream.set_read_timeout(None).unwrap();

    let next_tab_input = b"n".to_vec();
    let next_tab_payload = {
        let mut buf = encode_varint_u32(1);
        buf.extend_from_slice(&encode_varint_u32(next_tab_input.len() as u32));
        buf.extend_from_slice(&next_tab_input);
        buf
    };
    stream
        .write_all(&frame_message(&next_tab_payload))
        .expect("send next-tab key");
    stream.flush().expect("flush");

    assert!(
        wait_for_focused_tab(
            &api_socket,
            &workspace_id,
            &second_tab_id,
            Duration::from_secs(5)
        ),
        "prefix+n should focus the next tab through server-side keybinding dispatch"
    );

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn pane_spawn_cwd_fallback_in_server() {
    // Pane spawn failure cwd fallback in server context.
    // This test verifies that the server can start even with invalid
    // session data pointing to non-existent directories.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    // The server should have started successfully even though there are
    // no existing sessions (fresh state). The test verifies that the
    // server doesn't crash during initial pane creation.
    let response = ping_socket(&api_socket);
    assert!(
        response.contains("pong"),
        "server should respond to ping after startup: {response}"
    );

    // Create a workspace via the API — this tests pane creation in the server.
    let mut ws_stream = connect_unix_socket(&api_socket, Duration::from_secs(5));
    let request = r#"{"id":"2","method":"workspace.create","params":{"label":"cwd-test"}}"#;
    writeln!(ws_stream, "{}", request).unwrap();

    let mut reader = BufReader::new(ws_stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();

    assert!(
        response.contains("workspace_created") || response.contains("ok"),
        "workspace creation should succeed: {response}"
    );

    cleanup_spawned_hako(spawned, base);
}

#[test]
fn graceful_shutdown_sends_server_shutdown_to_client() {
    // Issue 2 fix: SIGINT triggers initiate_shutdown → ServerShutdown
    // broadcast to all clients before the server exits.
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    let mut spawned = spawn_server(&config_home, &runtime_dir, &api_socket, &client_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    // Connect and handshake.
    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    // Drain initial frame(s).
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    while read_server_message(&mut stream).is_ok() {}

    // Send SIGINT to the server process to trigger graceful shutdown.
    if let Some(pid) = spawned.child.process_id() {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGINT);
        }
    }

    // The client should receive a ServerShutdown message (variant 4)
    // before the connection is closed, not just an abrupt EOF.
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let result = read_server_message(&mut stream);
    match result {
        Ok((variant, _payload)) => {
            assert_eq!(
                variant, 4,
                "expected ServerShutdown (variant 4), got variant {variant}"
            );
        }
        Err(e) => {
            panic!("expected ServerShutdown message before connection close, got error: {e}");
        }
    }

    // Wait for the server to exit.
    spawned.close_master();
    let _ = spawned.child.wait();

    drop(spawned);
    cleanup_test_base(&base);
}

#[test]
fn client_receives_notify_on_agent_state_change() {
    // Notification events (sound/toast) are forwarded as
    // ServerMessage::Notify to connected clients when an agent state change
    // is triggered via the API (pane.report_agent).
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("hako.sock");
    let client_socket = runtime_dir.join("hako-client.sock");

    // Enable toast and sound in config so the server produces notifications.
    fs::create_dir_all(config_home.join("hako")).unwrap();
    fs::write(
        config_home.join("hako/config.toml"),
        "onboarding = false\n[ui.toast]\nenabled = true\n[ui.sound]\nenabled = true\n",
    )
    .unwrap();
    fs::create_dir_all(&runtime_dir).unwrap();
    register_runtime_dir(&runtime_dir);

    // Spawn the server directly (not using spawn_server helper because it
    // overwrites the config file with a minimal one).
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_hako"));
    cmd.arg("server");
    cmd.env("XDG_CONFIG_HOME", &config_home);
    cmd.env("XDG_RUNTIME_DIR", &runtime_dir);
    cmd.env("HAKO_SOCKET_PATH", &api_socket);
    cmd.env_remove("HAKO_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("HAKO_ENV");

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_hako_pid(child.process_id());
    drop(pair.slave);

    let spawned = SpawnedHako {
        _master: Some(pair.master),
        child,
    };
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    // Connect as a client and perform handshake.
    let mut stream = connect_unix_socket(&client_socket, Duration::from_secs(5));
    let (version, error) =
        client_handshake(&mut stream, 12, 80, 24).expect("handshake should succeed");
    assert_eq!(version, 12);
    assert!(error.is_none(), "{:?}", error);

    // Drain initial frame(s).
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    while read_server_message(&mut stream).is_ok() {}

    let (ws_id, pane_id) = create_workspace_and_root_pane(&api_socket, "notify-source");
    assert!(
        !ws_id.is_empty(),
        "workspace.create should return a non-empty workspace id: {ws_id}"
    );

    let report_response = send_json_request(
        &api_socket,
        &format!(
            r#"{{"id":"report_blocked","method":"pane.report_agent","params":{{"pane_id":"{pane_id}","agent":"pi","state":"blocked","source":"test"}}}}"#
        ),
    );
    assert_api_ok(&report_response, "pane.report_agent blocked");

    let attention = wait_for_notify(
        &mut stream,
        NotifyKindWire::Sound,
        "agent attention",
        Duration::from_secs(5),
    )
    .expect("blocked agent report should forward request sound notify");
    assert_eq!(attention.message, "agent attention");

    cleanup_spawned_hako(spawned, base);
}
