#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_socket_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    PathBuf::from(format!(
        "/tmp/gardn-eng-163-{}-{stamp}.sock",
        std::process::id()
    ))
}

fn run_agent(args: &[&str]) -> Output {
    let socket_path = unique_socket_path();
    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(["agent"])
        .args(args)
        .env("GARDN_SOCKET_PATH", &socket_path)
        .output()
        .expect("agent CLI should start");
    let _ = fs::remove_file(socket_path);
    output
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn agent_help_shows_target_before_options() {
    let output = run_agent(&["--help"]);
    assert!(
        output.status.success(),
        "agent help failed: {}",
        stderr(&output)
    );

    let help = stderr(&output);
    for usage in [
        "gardn agent read <target> [--source visible|recent|recent-unwrapped] [--lines N] [--format text|ansi] [--ansi]",
        "gardn agent prompt <target> <text> [--wait-for STATUS] [--timeout MS]",
        "gardn agent wait <target> --status <idle|working|blocked|unknown> [--timeout MS]",
        "gardn agent attach <target> [--takeover]",
        "gardn agent start <name> [--cwd PATH] [--host EXECUTION_HOST_ID] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>",
    ] {
        assert!(help.contains(usage), "missing target-first usage {usage:?}: {help}");
    }
    assert!(
        !help.contains("gardn agent"),
        "help must use Gardn's gardn name: {help}"
    );
}

#[test]
fn agent_subcommand_help_uses_target_first_forms() {
    let cases = [
        (
            ["read", "--help"].as_slice(),
            "usage: gardn agent read <target> [--source visible|recent|recent-unwrapped] [--lines N] [--format text|ansi] [--ansi]",
        ),
        (
            ["prompt", "--help"].as_slice(),
            "usage: gardn agent prompt <target> <text> [--wait-for STATUS] [--timeout MS]",
        ),
        (
            ["wait", "--help"].as_slice(),
            "usage: gardn agent wait <target> --status <idle|working|blocked|unknown> [--timeout MS]",
        ),
        (
            ["attach", "--help"].as_slice(),
            "usage: gardn agent attach <target> [--takeover]",
        ),
        (
            ["start", "--help"].as_slice(),
            "usage: gardn agent start <name> [--cwd PATH] [--host EXECUTION_HOST_ID] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>\n\nThe pane must be at its interactive shell prompt. Success means the expected agent was detected in the same terminal and is ready for input.\n\nnext: gardn agent prompt <TARGET> <TEXT> --wait",
        ),
    ];

    for (args, usage) in cases {
        let output = run_agent(args);
        assert!(
            output.status.success(),
            "gardn agent {} failed: {}",
            args[0],
            stderr(&output)
        );
        assert_eq!(stderr(&output).trim(), usage);
    }
}

#[test]
fn agent_start_parses_target_first_options_and_preserves_native_argv() {
    let socket_path = unique_socket_path();
    let listener = UnixListener::bind(&socket_path).expect("fake API socket should bind");
    let server = thread::spawn(move || {
        let mut captured = None;
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("CLI should connect to fake API");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("stream should clone"))
                .read_line(&mut line)
                .expect("request should be readable");
            let request: serde_json::Value =
                serde_json::from_str(&line).expect("request should be JSON");
            let response = if request["method"] == "ping" {
                serde_json::json!({
                    "id": request["id"],
                    "result": {
                        "type": "pong",
                        "version": "0.2.19",
                        "protocol": 12,
                        "capabilities": {"live_handoff": true}
                    }
                })
            } else {
                captured = Some(request.clone());
                serde_json::json!({"id": request["id"], "result": {"argv": request["params"]["argv"]}})
            };
            writeln!(stream, "{response}").expect("response should be writable");
        }
        captured.expect("agent.start request should be captured")
    });

    let output = Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args([
            "agent",
            "start",
            "worker",
            "--cwd",
            "/tmp/project",
            "--workspace",
            "workspace-1",
            "--tab",
            "tab-1",
            "--split",
            "right",
            "--focus",
            "--",
            "--native",
            "--flag",
            "value",
        ])
        .env("GARDN_SOCKET_PATH", &socket_path)
        .output()
        .expect("agent start should run");

    let request = server.join().expect("fake API should finish");
    let _ = fs::remove_file(socket_path);
    assert!(
        output.status.success(),
        "agent start failed: {}",
        stderr(&output)
    );
    assert_eq!(request["method"], "agent.start");
    assert_eq!(request["params"]["name"], "worker");
    assert_eq!(request["params"]["cwd"], "/tmp/project");
    assert_eq!(request["params"]["workspace_id"], "workspace-1");
    assert_eq!(request["params"]["tab_id"], "tab-1");
    assert_eq!(request["params"]["split"], "right");
    assert_eq!(request["params"]["focus"], true);
    assert_eq!(
        request["params"]["argv"],
        serde_json::json!(["--native", "--flag", "value"])
    );
}
