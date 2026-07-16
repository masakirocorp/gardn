#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn unique_test_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!(
        "/tmp/hako-cli-protocol-{}-{nanos}",
        std::process::id()
    ))
}

fn current_protocol() -> u64 {
    let output = Command::new(env!("CARGO_BIN_EXE_hako"))
        .args(["status", "client", "--json"])
        .output()
        .expect("hako status client should run");
    assert!(output.status.success());
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .expect("client status should be JSON")["protocol"]
        .as_u64()
        .expect("client status should include a protocol")
}

fn read_request(stream: &UnixStream) -> serde_json::Value {
    let mut line = String::new();
    BufReader::new(stream.try_clone().expect("stream should clone"))
        .read_line(&mut line)
        .expect("request should be readable");
    serde_json::from_str(&line).expect("request should be JSON")
}

fn write_pong(stream: &mut UnixStream, request: &serde_json::Value, protocol: u64) {
    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "id": request["id"],
            "result": {
                "type": "pong",
                "version": "test-server",
                "protocol": protocol,
                "capabilities": { "live_handoff": true }
            }
        })
    )
    .expect("pong should be writable");
    stream.flush().expect("pong should flush");
}

fn run_cli(socket_path: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hako"))
        .args(args)
        .env("HAKO_SOCKET_PATH", socket_path)
        .output()
        .expect("hako CLI should run")
}

#[test]
fn mismatched_cli_reports_json_error_without_dispatching_operation() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).expect("test directory should be created");
    let socket_path = base.join("hako.sock");
    let listener = UnixListener::bind(&socket_path).expect("fake server should bind");
    let mismatch = current_protocol() + 1;

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("protocol check should connect");
        let request = read_request(&stream);
        assert_eq!(request["method"], "ping");
        write_pong(&mut stream, &request, mismatch);

        listener
            .set_nonblocking(true)
            .expect("listener should become nonblocking");
        let deadline = Instant::now() + Duration::from_millis(250);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok(_) => return true,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("accept failed: {error}"),
            }
        }
        false
    });

    let output = run_cli(&socket_path, &["server", "reload-config"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: serde_json::Value =
        serde_json::from_slice(&output.stderr).expect("stderr should be one JSON error");
    assert_eq!(error["id"], "cli:server:reload-config");
    assert_eq!(error["error"]["code"], "protocol_mismatch");
    assert!(error["error"]["message"]
        .as_str()
        .expect("message should be text")
        .contains("Update and restart Hako"));
    assert!(!server.join().expect("fake server should finish"));

    fs::remove_dir_all(base).expect("test directory should be removed");
}

#[test]
fn matching_protocol_dispatches_operation_across_version_difference() {
    let base = unique_test_dir();
    fs::create_dir_all(&base).expect("test directory should be created");
    let socket_path = base.join("hako.sock");
    let listener = UnixListener::bind(&socket_path).expect("fake server should bind");
    let protocol = current_protocol();

    let server = thread::spawn(move || {
        let (mut ping_stream, _) = listener.accept().expect("protocol check should connect");
        let ping = read_request(&ping_stream);
        assert_eq!(ping["method"], "ping");
        write_pong(&mut ping_stream, &ping, protocol);

        let (mut operation_stream, _) = listener.accept().expect("operation should connect");
        let operation = read_request(&operation_stream);
        assert_eq!(operation["method"], "server.reload_config");
        writeln!(
            operation_stream,
            "{}",
            serde_json::json!({
                "id": operation["id"],
                "result": { "type": "ok" }
            })
        )
        .expect("operation response should be writable");
        operation_stream
            .flush()
            .expect("operation response should flush");
    });

    let output = run_cli(&socket_path, &["server", "reload-config"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(response["result"]["type"], "ok");
    server.join().expect("fake server should finish");

    fs::remove_dir_all(base).expect("test directory should be removed");
}
