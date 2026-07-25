use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::os::unix::net::UnixStream;

use crate::execution_host::protocol::{
    read_worker_message, write_worker_message, CommandSpec, CoordinatorMessage, RequestId,
    RuntimeExitStatus, RuntimeOpSeq, TerminalSize, TerminateMode, WorkerErrorCode, WorkerMessage,
};
use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};

use super::super::host_job::{
    discover_project_commands_at, expire_host_jobs, observe_runtime_process, run_command_at,
    HostJobKind,
};
use super::super::lifecycle::ConnectionOutcome;
use super::super::state::WorkerState;
use super::super::util::{
    COMMAND_OUTPUT_BYTES, DEFAULT_WORKER_SCROLLBACK_BYTES, HOST_JOB_TIMEOUT,
    MAX_COMMAND_MANIFEST_BYTES,
};
use super::support::{
    hello, tempfile_dir, test_binding, wait_for_worker_message, with_worker_connection,
};

#[test]
fn worker_command_returns_separate_bounded_output_and_exit() {
    let cancel = Arc::new(AtomicBool::new(false));
    let (exit, stdout, stderr) = run_command_at(
        std::env::temp_dir(),
        CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf command-out; printf command-err >&2; exit 7".to_string(),
            ],
            env: Vec::new(),
        },
        cancel,
    )
    .unwrap();
    assert_eq!(exit, RuntimeExitStatus::Code(7));
    assert_eq!(stdout, b"command-out");
    assert_eq!(stderr, b"command-err");
}

#[test]
fn worker_command_oversized_output_is_typed_bound_error() {
    let cancel = Arc::new(AtomicBool::new(false));
    let error = run_command_at(
        std::env::temp_dir(),
        CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!(
                    "dd if=/dev/zero bs=1024 count={} 2>/dev/null",
                    (COMMAND_OUTPUT_BYTES / 1024) + 8
                ),
            ],
            env: Vec::new(),
        },
        cancel,
    )
    .unwrap_err();
    assert_eq!(error.code, WorkerErrorCode::OutputTooLarge);
}

#[test]
fn cancelled_host_command_terminates_and_frees_waiters() {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel.clone();
    let worker = std::thread::spawn(move || {
        run_command_at(
            std::env::temp_dir(),
            CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 30".to_string()],
                env: Vec::new(),
            },
            cancel_flag,
        )
    });
    std::thread::sleep(Duration::from_millis(50));
    cancel.store(true, Ordering::Relaxed);
    let error = worker
        .join()
        .expect("command worker thread")
        .expect_err("cancelled command must fail");
    assert_eq!(error.code, WorkerErrorCode::TimedOut);
}

#[test]
fn expire_host_jobs_returns_timeout_and_clears_pending_state() {
    let binding = test_binding("expire-jobs", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding).unwrap();
    let (mut worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    coordinator_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    state.insert_host_job_for_test(
        RequestId::new(77),
        HostJobKind::RunCommand,
        location.clone(),
        Instant::now() - HOST_JOB_TIMEOUT - Duration::from_secs(1),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    expire_host_jobs(&mut state, &mut worker_stream).unwrap();
    // Timed-out jobs stay accounted until the thread finishes.
    assert!(state.host_job_contains(&RequestId::new(77)));
    assert!(state.host_job_is_responded_for_test(&RequestId::new(77)));
    let message: WorkerMessage = read_worker_message(&mut coordinator_stream).unwrap();
    match message {
        WorkerMessage::CommandResult {
            request_id,
            error: Some(error),
            exit: None,
            stdout,
            stderr,
            ..
        } => {
            assert_eq!(request_id, RequestId::new(77));
            assert_eq!(error.code, WorkerErrorCode::TimedOut);
            assert!(stdout.is_empty());
            assert!(stderr.is_empty());
        }
        other => panic!("expected timed-out command result, got {other:?}"),
    }
    // Simulate thread return reaping.
    state.mark_host_job_finished_for_test(RequestId::new(77));
    expire_host_jobs(&mut state, &mut worker_stream).unwrap();
    assert!(state.host_jobs_is_empty());
}

#[test]
fn blocked_host_command_does_not_delay_terminal_input() {
    let binding = test_binding("responsive-command", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let ((identity, resolved_location), outcome) =
        with_worker_connection(&mut state, hello(&binding, 1), |connection| {
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let ack: WorkerMessage = read_worker_message(connection).unwrap();
            assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));

            write_worker_message(
                connection,
                &CoordinatorMessage::CreateTerminal {
                    request_id: RequestId::new(1),
                    location: location.clone(),
                    size: TerminalSize { cols: 80, rows: 24 },
                    command: Some(CommandSpec {
                        program: "/bin/sh".to_string(),
                        args: vec!["-c".to_string(), "sleep 30".to_string()],
                        env: Vec::new(),
                    }),
                    env: Vec::new(),
                    scrollback_limit_bytes: DEFAULT_WORKER_SCROLLBACK_BYTES as u64,
                },
            )
            .unwrap();
            let created = wait_for_worker_message(connection, |message| {
                matches!(message, WorkerMessage::CreateTerminalResult { .. })
            });
            let (identity, resolved_location) = match created {
                WorkerMessage::CreateTerminalResult {
                    identity: Some(identity),
                    location,
                    error: None,
                    ..
                } => (identity, location),
                other => panic!("unexpected create result: {other:?}"),
            };

            write_worker_message(
                connection,
                &CoordinatorMessage::RunCommand {
                    request_id: RequestId::new(2),
                    location: resolved_location.clone(),
                    command: CommandSpec {
                        program: "/bin/sh".to_string(),
                        args: vec!["-c".to_string(), "sleep 60".to_string()],
                        env: Vec::new(),
                    },
                },
            )
            .unwrap();

            let input_started = Instant::now();
            write_worker_message(
                connection,
                &CoordinatorMessage::Input {
                    request_id: RequestId::new(3),
                    identity: identity.clone(),
                    location: resolved_location.clone(),
                    op_seq: RuntimeOpSeq::new(1),
                    data: b"x".to_vec(),
                },
            )
            .unwrap();
            let input_ack = wait_for_worker_message(connection, |message| {
                matches!(
                    message,
                    WorkerMessage::RequestAck {
                        request_id,
                        ..
                    } if *request_id == RequestId::new(3)
                )
            });
            assert!(
                input_started.elapsed() < Duration::from_secs(2),
                "blocked host command delayed terminal input: {:?}",
                input_started.elapsed()
            );
            assert!(matches!(
                input_ack,
                WorkerMessage::RequestAck {
                    request_id,
                    error: None,
                } if request_id == RequestId::new(3)
            ));

            write_worker_message(
                connection,
                &CoordinatorMessage::Terminate {
                    request_id: RequestId::new(4),
                    identity: identity.clone(),
                    location: resolved_location.clone(),
                    mode: TerminateMode::Terminate,
                },
            )
            .unwrap();
            let _ = wait_for_worker_message(connection, |message| {
                matches!(
                    message,
                    WorkerMessage::RequestAck {
                        request_id,
                        ..
                    } if *request_id == RequestId::new(4)
                )
            });

            (identity, resolved_location)
        });
    assert!(matches!(outcome, ConnectionOutcome::Continue));
    let _ = (identity, resolved_location);
    assert!(!state.has_runtime_records());
}

#[test]
fn observe_runtime_process_idle_shell_has_empty_foreground() {
    let location = ResourceLocation::new(
        ExecutionHostId::new("ssh:test").unwrap(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    // Use a real PTY so the shell owns a controlling terminal and foreground
    // process group, as it does in an execution worker.
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let child = pair
        .slave
        .spawn_command(CommandBuilder::new("/bin/sh"))
        .unwrap();
    drop(pair.slave);
    std::thread::sleep(Duration::from_millis(50));
    let shell_pid = child.process_id().expect("shell process id");
    let observation = observe_runtime_process(shell_pid, &location);
    assert_eq!(observation.pid, shell_pid);
    assert!(
        observation.foreground_processes.is_empty(),
        "idle shell must not report itself as foreground work: {:?}",
        observation.foreground_processes
    );
    assert!(
        observation
            .session_processes
            .iter()
            .any(|process| process.pid == shell_pid),
        "session_processes should include the shell"
    );
    let mut writer = pair.master.take_writer().unwrap();
    writer.write_all(b"exit\n").unwrap();
    drop(writer);
}

#[test]
fn observe_runtime_process_includes_session_descendant() {
    let root = tempfile_dir("omh-obs-session");
    let location = ResourceLocation::new(
        ExecutionHostId::new("ssh:test").unwrap(),
        HostPath::new(root.clone()).unwrap(),
    );
    let mut child = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("sleep 30 & wait")
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(80));
    let shell_pid = child.id();
    let observation = observe_runtime_process(shell_pid, &location);
    assert!(
        !observation.session_processes.is_empty(),
        "expected session members, got {:?}",
        observation.session_processes
    );
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn discover_project_commands_returns_host_qualified_package_scripts() {
    let root = tempfile_dir("omh-proj-cmds");
    std::fs::write(
        root.join("package.json"),
        r#"{"scripts":{"dev":"vite","test":"vitest"}}"#,
    )
    .unwrap();
    let binding = test_binding("project-commands", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(root.clone()).unwrap(),
    );
    let (resolved, commands) = discover_project_commands_at(
        &binding.execution_host_id,
        &location,
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(resolved, location);
    assert!(
        commands.iter().any(|command| {
            command.name == "dev"
                && command.command == "npm run dev"
                && command.location == location
        }),
        "expected host-qualified dev script, got {commands:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn process_group_cancel_reaps_pipe_holding_descendant() {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel.clone();
    let worker = std::thread::spawn(move || {
        run_command_at(
            std::env::temp_dir(),
            CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    // Child keeps stdout open after parent would otherwise exit.
                    "sleep 120 | cat".to_string(),
                ],
                env: Vec::new(),
            },
            cancel_flag,
        )
    });
    std::thread::sleep(Duration::from_millis(80));
    cancel.store(true, Ordering::Relaxed);
    let started = Instant::now();
    let error = worker
        .join()
        .expect("command worker thread")
        .expect_err("cancelled pipe-holding command must fail");
    assert_eq!(error.code, WorkerErrorCode::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "process-group cancel must not hang on descendant pipes: {:?}",
        started.elapsed()
    );
}

#[test]
fn discover_project_commands_uses_project_root_from_nested_cwd() {
    let root = tempfile_dir("omh-nested-proj");
    let nested = root.join("packages").join("app");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
    let binding = test_binding("nested-project-commands", 1);
    let nested_location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(nested.clone()).unwrap(),
    );
    let (resolved, commands) = discover_project_commands_at(
        &binding.execution_host_id,
        &nested_location,
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    assert_eq!(resolved.path.as_path(), root.as_path());
    assert!(
        commands.iter().any(|command| {
            command.name == "dev"
                && command.location.path.as_path() == root.as_path()
                && command.location.execution_host_id == binding.execution_host_id
        }),
        "expected root-qualified remote command, got {commands:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn oversized_manifest_is_typed_bound_error() {
    let root = tempfile_dir("omh-big-manifest");
    let big = "x".repeat(MAX_COMMAND_MANIFEST_BYTES + 8);
    std::fs::write(
        root.join("package.json"),
        format!(r#"{{"scripts":{{"x":"{big}"}}}}"#),
    )
    .unwrap();
    let binding = test_binding("big-manifest", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(root.clone()).unwrap(),
    );
    let error = discover_project_commands_at(
        &binding.execution_host_id,
        &location,
        Arc::new(AtomicBool::new(false)),
    )
    .expect_err("oversized manifest must fail");
    assert_eq!(error.code, WorkerErrorCode::OutputTooLarge);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stalled_host_work_keeps_pty_responsive_and_late_create_does_not_spawn() {
    let binding = test_binding("create-preflight-responsive", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let before = state.runtime_record_count();
    let ((live_identity, live_location), outcome) =
        with_worker_connection(&mut state, hello(&binding, 1), |connection| {
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let ack: WorkerMessage = read_worker_message(connection).unwrap();
            assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));

            write_worker_message(
                connection,
                &CoordinatorMessage::CreateTerminal {
                    request_id: RequestId::new(1),
                    location: location.clone(),
                    size: TerminalSize { cols: 80, rows: 24 },
                    command: Some(CommandSpec {
                        program: "/bin/sh".to_string(),
                        args: vec!["-c".to_string(), "sleep 30".to_string()],
                        env: Vec::new(),
                    }),
                    env: Vec::new(),
                    scrollback_limit_bytes: DEFAULT_WORKER_SCROLLBACK_BYTES as u64,
                },
            )
            .unwrap();
            let created = wait_for_worker_message(connection, |message| {
                matches!(message, WorkerMessage::CreateTerminalResult { .. })
            });
            let (live_identity, live_location) = match created {
                WorkerMessage::CreateTerminalResult {
                    identity: Some(identity),
                    location,
                    error: None,
                    ..
                } => (identity, location),
                other => panic!("unexpected create result: {other:?}"),
            };

            // Stall the host with a long command (process-group bounded).
            write_worker_message(
                connection,
                &CoordinatorMessage::RunCommand {
                    request_id: RequestId::new(2),
                    location: live_location.clone(),
                    command: CommandSpec {
                        program: "/bin/sh".to_string(),
                        args: vec!["-c".to_string(), "sleep 60 | cat".to_string()],
                        env: Vec::new(),
                    },
                },
            )
            .unwrap();

            let input_started = Instant::now();
            write_worker_message(
                connection,
                &CoordinatorMessage::Input {
                    request_id: RequestId::new(3),
                    identity: live_identity.clone(),
                    location: live_location.clone(),
                    op_seq: RuntimeOpSeq::new(1),
                    data: b"ping".to_vec(),
                },
            )
            .unwrap();
            let input_ack = wait_for_worker_message(connection, |message| {
                matches!(
                    message,
                    WorkerMessage::RequestAck {
                        request_id,
                        ..
                    } if *request_id == RequestId::new(3)
                )
            });
            assert!(
                input_started.elapsed() < Duration::from_secs(2),
                "stalled host work delayed live PTY input: {:?}",
                input_started.elapsed()
            );
            assert!(matches!(
                input_ack,
                WorkerMessage::RequestAck {
                    request_id,
                    error: None,
                } if request_id == RequestId::new(3)
            ));

            // Invalid create preflight must fail without spawning another runtime.
            let missing = std::env::temp_dir().join(format!(
                "omh-missing-create-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            write_worker_message(
                connection,
                &CoordinatorMessage::CreateTerminal {
                    request_id: RequestId::new(4),
                    location: ResourceLocation::new(
                        binding.execution_host_id.clone(),
                        HostPath::new(missing).unwrap(),
                    ),
                    size: TerminalSize { cols: 80, rows: 24 },
                    command: None,
                    env: Vec::new(),
                    scrollback_limit_bytes: DEFAULT_WORKER_SCROLLBACK_BYTES as u64,
                },
            )
            .unwrap();
            let failed = wait_for_worker_message(connection, |message| {
                matches!(message, WorkerMessage::CreateTerminalResult { .. })
            });
            assert!(matches!(
                failed,
                WorkerMessage::CreateTerminalResult {
                    identity: None,
                    error: Some(_),
                    ..
                }
            ));

            write_worker_message(
                connection,
                &CoordinatorMessage::Terminate {
                    request_id: RequestId::new(5),
                    identity: live_identity.clone(),
                    location: live_location.clone(),
                    mode: TerminateMode::Terminate,
                },
            )
            .unwrap();
            let _ = wait_for_worker_message(connection, |message| {
                matches!(
                    message,
                    WorkerMessage::RequestAck {
                        request_id,
                        ..
                    } if *request_id == RequestId::new(5)
                )
            });
            (live_identity, live_location)
        });
    assert!(matches!(outcome, ConnectionOutcome::Continue));
    assert_eq!(state.runtime_record_count(), before);
    let _ = (live_identity, live_location);
}
