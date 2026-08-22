#![cfg(unix)]

//! Integration tests for multi-client server behavior.

mod support;

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use serde_json::Value;
use support::{
    cleanup_test_base, connect_unix_socket, register_runtime_dir, register_spawned_gardn_pid,
    unregister_spawned_gardn_pid,
};

fn unique_test_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!(
        "/tmp/gardn-multi-client-test-{}-{nanos}",
        std::process::id()
    ))
}

struct SpawnedGardn {
    _master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
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

fn wait_for_child_exit(child: &mut Box<dyn Child + Send + Sync>) {
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_for_socket(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() && UnixStream::connect(path).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("socket did not appear at {}", path.display());
}

fn wait_for_file(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("file did not appear at {}", path.display());
}

fn spawn_server(config_home: &Path, runtime_dir: &Path, api_socket_path: &Path) -> SpawnedGardn {
    fs::create_dir_all(config_home.join("gardn")).unwrap();
    fs::create_dir_all(runtime_dir).unwrap();
    register_runtime_dir(runtime_dir);
    fs::write(
        config_home.join("gardn/config.toml"),
        "onboarding = false\n",
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
    cmd.env("GARDN_SOCKET_PATH", api_socket_path);
    cmd.env_remove("GARDN_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("GARDN_ENV");

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_gardn_pid(child.process_id());
    drop(pair.slave);

    SpawnedGardn {
        _master: pair.master,
        child,
    }
}

fn spawn_client_process(
    config_home: &Path,
    runtime_dir: &Path,
    api_socket_path: &Path,
) -> SpawnedGardn {
    register_runtime_dir(runtime_dir);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_gardn"));
    cmd.arg("client");
    cmd.env("GARDN_DISABLE_SOUND", "1");
    cmd.env("XDG_CONFIG_HOME", config_home);
    cmd.env("XDG_RUNTIME_DIR", runtime_dir);
    cmd.env("GARDN_SOCKET_PATH", api_socket_path);
    cmd.env_remove("GARDN_CLIENT_SOCKET_PATH");
    cmd.env("SHELL", "/bin/sh");
    cmd.env_remove("GARDN_ENV");

    let child = pair.slave.spawn_command(cmd).unwrap();
    register_spawned_gardn_pid(child.process_id());
    drop(pair.slave);

    SpawnedGardn {
        _master: pair.master,
        child,
    }
}

fn server_log_path(config_home: &Path) -> PathBuf {
    let app_dir = if cfg!(debug_assertions) {
        "gardn-dev"
    } else {
        "gardn"
    };
    config_home.join(app_dir).join("gardn-server.log")
}

fn count_log_occurrences(path: &Path, needle: &str) -> usize {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.lines().filter(|line| line.contains(needle)).count())
        .unwrap_or(0)
}

fn wait_for_log_occurrence_count(
    path: &Path,
    needle: &str,
    min_count: usize,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if count_log_occurrences(path, needle) >= min_count {
            return true;
        }
        thread::sleep(Duration::from_millis(40));
    }
    false
}

fn ping_socket(socket_path: &Path) -> String {
    let mut stream = connect_unix_socket(socket_path, Duration::from_secs(5));
    writeln!(
        stream,
        "{{\"id\":\"ping\",\"method\":\"ping\",\"params\":{{}}}}"
    )
    .unwrap();

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();
    response.trim().to_string()
}

fn send_json_request(socket_path: &Path, request: &str) -> Value {
    let mut stream = connect_unix_socket(socket_path, Duration::from_secs(5));
    writeln!(stream, "{request}").unwrap();

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).unwrap();

    serde_json::from_str(&response).expect("response should be valid JSON")
}

fn create_workspace_and_root_pane(socket_path: &Path, label: &str) -> (String, String) {
    let response = send_json_request(
        socket_path,
        &format!(
            "{{\"id\":\"ws_create\",\"method\":\"workspace.create\",\"params\":{{\"label\":\"{label}\"}}}}"
        ),
    );

    if response.get("error").is_some() {
        panic!("workspace.create failed: {response}");
    }

    let workspace_id = response
        .pointer("/result/workspace/workspace_id")
        .and_then(Value::as_str)
        .expect("workspace.create should return workspace id")
        .to_string();

    let pane_id = response
        .pointer("/result/root_pane/pane_id")
        .and_then(Value::as_str)
        .expect("workspace.create should return root pane id")
        .to_string();

    (workspace_id, pane_id)
}

fn pane_send_input(socket_path: &Path, pane_id: &str, text: &str) {
    let request = format!(
        "{{\"id\":\"send_input\",\"method\":\"pane.send_input\",\"params\":{{\"pane_id\":\"{pane_id}\",\"text\":\"{}\",\"keys\":[\"Enter\"]}}}}",
        text.replace('"', "\\\"")
    );
    let response = send_json_request(socket_path, &request);
    if response.get("error").is_some() {
        panic!("pane.send_input failed: {response}");
    }
}

fn pane_read_recent(socket_path: &Path, pane_id: &str, lines: usize) -> String {
    let response = send_json_request(
        socket_path,
        &format!(
            "{{\"id\":\"pane_read\",\"method\":\"pane.read\",\"params\":{{\"pane_id\":\"{pane_id}\",\"source\":\"recent\",\"lines\":{lines}}}}}"
        ),
    );

    if response.get("error").is_some() {
        panic!("pane.read failed: {response}");
    }

    response
        .pointer("/result/read/text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn pane_read_recent_contains(
    socket_path: &Path,
    pane_id: &str,
    needle: &str,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if pane_read_recent(socket_path, pane_id, 200).contains(needle) {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

fn parse_size_after_marker(text: &str, marker: &str) -> Option<(u16, u16)> {
    let mut seen_marker = false;
    for line in text.lines() {
        if !seen_marker {
            if line.contains(marker) {
                seen_marker = true;
            }
            continue;
        }

        let mut parts = line.split_whitespace();
        let Some(rows_raw) = parts.next() else {
            continue;
        };
        let Some(cols_raw) = parts.next() else {
            continue;
        };

        let Ok(rows) = rows_raw.parse::<u16>() else {
            continue;
        };
        let Ok(cols) = cols_raw.parse::<u16>() else {
            continue;
        };

        return Some((rows, cols));
    }

    None
}

fn try_read_pane_tty_size(
    socket_path: &Path,
    pane_id: &str,
    timeout: Duration,
) -> Option<(u16, u16)> {
    let marker = format!(
        "SIZE_MARKER_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );

    pane_send_input(socket_path, pane_id, &format!("echo {marker}; stty size"));

    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let text = pane_read_recent(socket_path, pane_id, 200);
        if let Some(size) = parse_size_after_marker(&text, &marker) {
            return Some(size);
        }
        thread::sleep(Duration::from_millis(50));
    }

    None
}

fn read_pane_tty_size(socket_path: &Path, pane_id: &str, timeout: Duration) -> (u16, u16) {
    if let Some(size) = try_read_pane_tty_size(socket_path, pane_id, timeout) {
        return size;
    }

    let snapshot = pane_read_recent(socket_path, pane_id, 200);
    panic!(
        "did not observe tty size after marker. pane output:\n{}",
        snapshot
    );
}

// ---------------------------------------------------------------------------
// Minimal bincode v2 varint helpers for protocol tests
// ---------------------------------------------------------------------------

fn encode_varint_u32(v: u32) -> Vec<u8> {
    if v < 251 {
        vec![v as u8]
    } else if v < 65536 {
        let mut buf = vec![251u8];
        buf.extend_from_slice(&(v as u16).to_le_bytes());
        buf
    } else {
        let mut buf = vec![252u8];
        buf.extend_from_slice(&v.to_le_bytes());
        buf
    }
}

fn encode_varint_u16(v: u16) -> Vec<u8> {
    if v < 251 {
        vec![v as u8]
    } else {
        let mut buf = vec![251u8];
        buf.extend_from_slice(&v.to_le_bytes());
        buf
    }
}

fn encode_varint_enum(variant_idx: u32, fields: &[&[u8]]) -> Vec<u8> {
    let mut buf = encode_varint_u32(variant_idx);
    for field in fields {
        buf.extend_from_slice(field);
    }
    buf
}

fn frame_message(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut framed = len.to_le_bytes().to_vec();
    framed.extend_from_slice(payload);
    framed
}

fn decode_varint_u32(payload: &[u8], offset: usize) -> Result<(u32, usize), String> {
    if offset >= payload.len() {
        return Err("payload too short for varint".into());
    }
    let first_byte = payload[offset];
    match first_byte {
        0..=250 => Ok((first_byte as u32, 1)),
        251 => {
            if offset + 3 > payload.len() {
                return Err("payload too short for u16 varint".into());
            }
            let v = u16::from_le_bytes(
                payload[offset + 1..offset + 3]
                    .try_into()
                    .map_err(|e: std::array::TryFromSliceError| e.to_string())?,
            );
            Ok((v as u32, 3))
        }
        252 => {
            if offset + 5 > payload.len() {
                return Err("payload too short for u32 varint".into());
            }
            let v = u32::from_le_bytes(
                payload[offset + 1..offset + 5]
                    .try_into()
                    .map_err(|e: std::array::TryFromSliceError| e.to_string())?,
            );
            Ok((v, 5))
        }
        _ => Err(format!("unsupported varint tag: {first_byte}")),
    }
}

fn is_timeout(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

fn read_server_variant(stream: &mut UnixStream, timeout: Duration) -> io::Result<u32> {
    stream.set_read_timeout(Some(timeout))?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "zero-length payload",
        ));
    }

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;

    let (variant, _consumed) = decode_varint_u32(&payload, 0)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(variant)
}

fn client_handshake(
    stream: &mut UnixStream,
    version: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;

    // ClientMessage::Hello = variant 0
    let hello_payload = encode_varint_enum(
        0,
        &[
            &encode_varint_u32(version),
            &encode_varint_u16(cols),
            &encode_varint_u16(rows),
            &encode_varint_u32(8),  // cell_width_px
            &encode_varint_u32(16), // cell_height_px
            &encode_varint_u32(0),  // RenderEncoding::SemanticFrame
            &encode_varint_u32(0),  // ClientKeybindings::Server
            &encode_varint_u32(0),  // ClientLaunchMode::App
        ],
    );
    stream
        .write_all(&frame_message(&hello_payload))
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;

    // Read ServerMessage::Welcome = variant 0
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).map_err(|e| e.to_string())?;

    let mut offset = 0;
    let (variant, consumed) = decode_varint_u32(&payload, offset)?;
    offset += consumed;
    if variant != 0 {
        return Err(format!("expected Welcome variant 0, got {variant}"));
    }

    let (_server_version, consumed) = decode_varint_u32(&payload, offset)?;
    offset += consumed;

    let (_encoding, consumed) = decode_varint_u32(&payload, offset)?;
    offset += consumed;

    if offset >= payload.len() {
        return Err("payload too short for Welcome.error option tag".into());
    }
    let option_tag = payload[offset];
    offset += 1;

    if option_tag == 1 {
        let (str_len, consumed) = decode_varint_u32(&payload, offset)?;
        offset += consumed;
        let str_len = str_len as usize;
        if offset + str_len > payload.len() {
            return Err("payload too short for welcome error string".into());
        }
        let err = String::from_utf8(payload[offset..offset + str_len].to_vec())
            .map_err(|e| e.to_string())?;
        return Err(format!("handshake rejected: {err}"));
    }

    Ok(())
}

fn connect_raw_client(client_socket: &Path, cols: u16, rows: u16) -> UnixStream {
    let mut stream = connect_unix_socket(client_socket, Duration::from_secs(5));
    client_handshake(&mut stream, 13, cols, rows).expect("handshake should succeed");
    stream
}

fn send_client_input(stream: &mut UnixStream, data: &[u8]) {
    // ClientMessage::Input = variant 1
    let payload = {
        let mut buf = encode_varint_u32(1);
        buf.extend_from_slice(&encode_varint_u32(data.len() as u32));
        buf.extend_from_slice(data);
        buf
    };
    stream.write_all(&frame_message(&payload)).unwrap();
    stream.flush().unwrap();
}
fn send_client_resize(stream: &mut UnixStream, cols: u16, rows: u16) {
    // ClientMessage::Resize = variant 3
    let payload = {
        let mut buf = encode_varint_u32(3);
        buf.extend_from_slice(&encode_varint_u16(cols));
        buf.extend_from_slice(&encode_varint_u16(rows));
        buf.extend_from_slice(&encode_varint_u32(8)); // cell_width_px
        buf.extend_from_slice(&encode_varint_u32(16)); // cell_height_px
        buf
    };
    stream.write_all(&frame_message(&payload)).unwrap();
    stream.flush().unwrap();
}

fn send_client_detach(stream: &mut UnixStream) {
    // ClientMessage::Detach = variant 4
    let payload = encode_varint_u32(4);
    stream.write_all(&frame_message(&payload)).unwrap();
    stream.flush().unwrap();
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

fn decode_frame_payload(payload: &[u8]) -> io::Result<FrameWire> {
    bincode::serde::decode_from_slice(payload, bincode::config::standard())
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))
        .and_then(|(frame, consumed): (FrameWire, usize)| {
            if consumed != payload.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
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

fn read_server_message_payload(
    stream: &mut UnixStream,
    timeout: Duration,
) -> io::Result<(u32, Vec<u8>)> {
    stream.set_read_timeout(Some(timeout))?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "zero-length payload",
        ));
    }

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;

    let (variant, consumed) = decode_varint_u32(&payload, 0)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok((variant, payload[consumed..].to_vec()))
}

fn drain_server_messages(stream: &mut UnixStream, max_drain: Duration) {
    let deadline = Instant::now() + max_drain;
    while Instant::now() < deadline {
        match read_server_variant(stream, Duration::from_millis(50)) {
            Ok(_) => {}
            Err(err) if is_timeout(&err) => break,
            Err(_) => break,
        }
    }
}

fn wait_for_frame(stream: &mut UnixStream, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let slice = remaining.min(Duration::from_millis(75));
        match read_server_variant(stream, slice) {
            Ok(1) => return true, // ServerMessage::Frame
            Ok(_) => {}
            Err(err) if is_timeout(&err) => {}
            Err(_) => return false,
        }
    }
    false
}

fn wait_for_frame_matching(
    stream: &mut UnixStream,
    timeout: Duration,
    predicate: impl Fn(&FrameWire) -> bool,
) -> io::Result<bool> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let slice = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(80));
        match read_server_message_payload(stream, slice) {
            Ok((1, frame_payload)) => {
                let frame = decode_frame_payload(&frame_payload)?;
                if predicate(&frame) {
                    return Ok(true);
                }
            }
            Ok((_variant, _payload)) => {}
            Err(err) if is_timeout(&err) => {}
            Err(err) => return Err(err),
        }
    }

    Ok(false)
}

fn frame_contains_text(frame: &FrameWire, needle: &str) -> bool {
    if frame.cells.is_empty() {
        return false;
    }

    let row_width = frame.width.max(1) as usize;
    let mut full_text = String::new();

    for row in frame.cells.chunks(row_width) {
        for cell in row {
            let _ = (cell.fg, cell.bg, cell.modifier, cell.skip);
            full_text.push_str(&cell.symbol);
        }
        full_text.push('\n');
    }

    let _ = (frame.height, frame.graphics.len());
    if let Some(cursor) = frame.cursor.as_ref() {
        let _ = (cursor.x, cursor.y, cursor.visible, cursor.shape);
    }

    full_text.contains(needle)
}

#[test]
fn multi_client_allows_multiple_simultaneous_connections() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let mut client_a = connect_raw_client(&client_socket, 120, 40);
    let mut client_b = connect_raw_client(&client_socket, 100, 30);

    assert!(
        wait_for_frame(&mut client_a, Duration::from_secs(2)),
        "client A should receive frames"
    );
    assert!(
        wait_for_frame(&mut client_b, Duration::from_secs(2)),
        "client B should receive frames"
    );

    let ping = ping_socket(&api_socket);
    assert!(
        ping.contains("pong"),
        "server should remain responsive: {ping}"
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_smaller_watcher_join_does_not_resize_controller_canvas() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, pane_id) =
        create_workspace_and_root_pane(&api_socket, "stable-size-watcher-join");

    let mut controller = connect_raw_client(&client_socket, 120, 40);
    assert!(wait_for_frame(&mut controller, Duration::from_secs(2)));
    let controller_size = read_pane_tty_size(&api_socket, &pane_id, Duration::from_secs(5));

    let mut watcher = connect_raw_client(&client_socket, 80, 24);
    assert!(wait_for_frame(&mut watcher, Duration::from_secs(2)));
    let size_after_join = read_pane_tty_size(&api_socket, &pane_id, Duration::from_secs(5));

    assert_eq!(
        size_after_join, controller_size,
        "a smaller watcher must not resize the controller-owned canonical PTY"
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_eventually_broadcasts_frame_updates_to_all_clients() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    // Ensure connected clients start on the workspace whose pane will receive input.
    let (_workspace_id, pane_id) =
        create_workspace_and_root_pane(&api_socket, "broadcast-client-a-to-b");

    let mut client_a = connect_raw_client(&client_socket, 100, 30);
    let mut client_b = connect_raw_client(&client_socket, 100, 30);

    // Drain initial frames so we measure the frame caused by new input.
    drain_server_messages(&mut client_a, Duration::from_millis(300));
    drain_server_messages(&mut client_b, Duration::from_millis(300));

    let marker = format!(
        "MB{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    send_client_input(&mut client_a, format!("echo {marker}\n").as_bytes());
    let received = wait_for_frame_matching(&mut client_b, Duration::from_secs(15), |frame| {
        frame_contains_text(frame, &marker)
    })
    .expect("frame decoding should succeed");

    assert!(
        received,
        "client B should eventually receive a broadcast frame containing client A marker"
    );
    assert!(
        pane_read_recent_contains(&api_socket, &pane_id, &marker, Duration::from_secs(5)),
        "pane output should include client A marker so broadcast reflects a real state change"
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_watcher_leaving_does_not_resize_controller_canvas() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, pane_id) =
        create_workspace_and_root_pane(&api_socket, "stable-size-watcher-leave");

    let mut controller = connect_raw_client(&client_socket, 120, 40);
    let mut watcher = connect_raw_client(&client_socket, 80, 24);
    assert!(wait_for_frame(&mut controller, Duration::from_secs(2)));
    assert!(wait_for_frame(&mut watcher, Duration::from_secs(2)));
    let controller_size = read_pane_tty_size(&api_socket, &pane_id, Duration::from_secs(5));

    send_client_detach(&mut watcher);
    drop(watcher);

    let size_after_leave = read_pane_tty_size(&api_socket, &pane_id, Duration::from_secs(5));
    assert_eq!(
        size_after_leave, controller_size,
        "a watcher leaving must not resize the controller-owned canonical PTY"
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_controller_resize_changes_canonical_size() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, pane_id) =
        create_workspace_and_root_pane(&api_socket, "stable-size-controller-resize");

    let mut controller = connect_raw_client(&client_socket, 120, 40);
    assert!(wait_for_frame(&mut controller, Duration::from_secs(2)));
    let controller_size = read_pane_tty_size(&api_socket, &pane_id, Duration::from_secs(5));

    send_client_resize(&mut controller, 96, 28);

    let deadline = Instant::now() + Duration::from_secs(8);
    let mut resized_size = None;
    while Instant::now() < deadline {
        if let Some(size) =
            try_read_pane_tty_size(&api_socket, &pane_id, Duration::from_millis(400))
        {
            if size != controller_size {
                resized_size = Some(size);
                break;
            }
        }
        thread::sleep(Duration::from_millis(60));
    }

    assert!(
        resized_size.is_some(),
        "controller resize should change the canonical PTY size: before={controller_size:?}, last_seen={:?}",
        try_read_pane_tty_size(&api_socket, &pane_id, Duration::from_millis(300))
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_controller_disconnect_leaves_watcher_free_without_promotion() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let (_workspace_id, pane_id) =
        create_workspace_and_root_pane(&api_socket, "stable-size-controller-disconnect");

    let mut controller = connect_raw_client(&client_socket, 120, 40);
    let mut watcher = connect_raw_client(&client_socket, 80, 24);
    assert!(wait_for_frame(&mut controller, Duration::from_secs(2)));
    assert!(wait_for_frame(&mut watcher, Duration::from_secs(2)));
    let controller_size = read_pane_tty_size(&api_socket, &pane_id, Duration::from_secs(5));

    send_client_detach(&mut controller);
    drop(controller);

    // A released tab is free, not implicitly promoted to its remaining watcher.
    send_client_resize(&mut watcher, 120, 40);
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_seen_size = None;
    while Instant::now() < deadline {
        if let Some(size) =
            try_read_pane_tty_size(&api_socket, &pane_id, Duration::from_millis(400))
        {
            last_seen_size = Some(size);
            assert_eq!(
                size, controller_size,
                "a free watcher must not resize the canonical PTY after controller disconnect"
            );
        }
        thread::sleep(Duration::from_millis(60));
    }

    assert_eq!(
        last_seen_size,
        Some(controller_size),
        "watcher should remain connected while the released tab stays free"
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_client_crash_sigkill_does_not_affect_server() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");

    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    let mut survivor = connect_raw_client(&client_socket, 100, 30);
    assert!(wait_for_frame(&mut survivor, Duration::from_secs(2)));

    let log_path = server_log_path(&config_home);
    let connected_before = count_log_occurrences(&log_path, "client connected");

    let crashing_client = spawn_client_process(&config_home, &runtime_dir, &api_socket);

    let attached_before_kill = wait_for_log_occurrence_count(
        &log_path,
        "client connected",
        connected_before + 1,
        Duration::from_secs(8),
    );
    assert!(
        attached_before_kill,
        "thin client must complete handshake/attachment before SIGKILL"
    );

    if let Some(pid) = crashing_client.child.process_id() {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    let mut crashing_client = crashing_client;
    wait_for_child_exit(&mut crashing_client.child);

    let ping = ping_socket(&api_socket);
    assert!(
        ping.contains("pong"),
        "server should stay healthy after SIGKILLed client: {ping}"
    );

    drain_server_messages(&mut survivor, Duration::from_millis(250));
    send_client_input(&mut survivor, b"echo survivor-still-works\n");
    assert!(
        wait_for_frame(&mut survivor, Duration::from_secs(2)),
        "remaining client should continue receiving frames"
    );

    cleanup_spawned_gardn(server, base);
}

#[test]
fn multi_client_rapid_connect_disconnect_stress_10_cycles() {
    let _lock = test_lock();
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket = runtime_dir.join("gardn.sock");
    let client_socket = runtime_dir.join("gardn-client.sock");

    let server = spawn_server(&config_home, &runtime_dir, &api_socket);
    wait_for_socket(&api_socket, Duration::from_secs(10));
    wait_for_file(&client_socket, Duration::from_secs(10));

    for i in 0..10u16 {
        let mut client = connect_raw_client(&client_socket, 80 + i, 24 + (i % 4));
        assert!(
            wait_for_frame(&mut client, Duration::from_secs(2)),
            "cycle {i} should complete handshake and receive an initial frame"
        );
        send_client_detach(&mut client);
        drop(client);
        let ping = ping_socket(&api_socket);
        assert!(
            ping.contains("pong"),
            "server should stay healthy after disconnect cycle {i}: {ping}"
        );
    }

    let ping = ping_socket(&api_socket);
    assert!(
        ping.contains("pong"),
        "server should remain healthy after rapid connect/disconnect: {ping}"
    );

    let mut final_client = connect_raw_client(&client_socket, 100, 30);
    assert!(
        wait_for_frame(&mut final_client, Duration::from_secs(2)),
        "new client should still connect and receive frames after stress"
    );

    cleanup_spawned_gardn(server, base);
}
