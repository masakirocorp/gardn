#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_socket_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    PathBuf::from(format!(
        "/tmp/gardn-skill-cli-{stamp}-{:x}.sock",
        std::process::id()
    ))
}

fn run_gardn(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gardn"))
        .args(args)
        .env("GARDN_SOCKET_PATH", unique_socket_path())
        .output()
        .expect("gardn CLI should start")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn bundled_skill() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join("../../SKILL.md"))
        .expect("repo-root SKILL.md should be readable")
}

#[test]
fn skill_flag_prints_bundled_agent_skill_and_exits() {
    let output = run_gardn(&["--skill"]);
    assert!(
        output.status.success(),
        "gardn --skill failed: status={:?} stderr={}",
        output.status.code(),
        stderr(&output)
    );

    let printed = stdout(&output);
    assert_eq!(printed, bundled_skill());
    assert!(
        printed.contains("# Gardn — agent skill"),
        "printed skill lost Gardn identity: {printed}"
    );
    assert!(
        stderr(&output).is_empty(),
        "gardn --skill should write only to stdout: {}",
        stderr(&output)
    );
}

#[test]
fn root_help_documents_skill_flag_and_next_cli_step() {
    let output = run_gardn(&["--help"]);
    assert!(
        output.status.success(),
        "gardn --help failed: {}",
        stderr(&output)
    );

    let help = stdout(&output);
    assert!(
        help.contains("--skill             print the agent skill file and exit"),
        "root help is missing --skill: {help}"
    );
    assert!(
        help.contains("gardn --skill prints agent instructions for driving gardn from a pane"),
        "root help is missing the skill next-step hint: {help}"
    );
}

#[test]
fn next_step_hints_render_without_replacing_existing_help() {
    let agent_start = run_gardn(&["agent", "start", "--help"]);
    assert!(
        agent_start.status.success(),
        "agent start help failed: {}",
        stderr(&agent_start)
    );
    let agent_start_help = stderr(&agent_start);
    assert!(
        agent_start_help.contains(
            "usage: gardn agent start <name> [--cwd PATH] [--host EXECUTION_HOST_ID] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>"
        ),
        "agent start dropped its existing usage: {agent_start_help}"
    );
    assert!(
        agent_start_help.contains(
            "The pane must be at its interactive shell prompt. Success means the expected agent was detected in the same terminal and is ready for input."
        ),
        "agent start dropped its existing after_help: {agent_start_help}"
    );
    assert!(
        agent_start_help.contains("next: gardn agent prompt <TARGET> <TEXT> --wait"),
        "agent start is missing its next-step hint: {agent_start_help}"
    );

    let pane_send_text = run_gardn(&["pane", "send-text", "--help"]);
    assert!(
        pane_send_text.status.success(),
        "pane send-text help failed: {}",
        stderr(&pane_send_text)
    );
    let pane_send_text_help = stderr(&pane_send_text);
    assert!(
        pane_send_text_help.contains("usage: gardn pane send-text <pane_id> <text>"),
        "pane send-text dropped its existing usage: {pane_send_text_help}"
    );
    assert!(
        pane_send_text_help
            .contains("next: gardn pane run <PANE_ID> <COMMAND> sends text and Enter in one call"),
        "pane send-text is missing its next-step hint: {pane_send_text_help}"
    );
}
