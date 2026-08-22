use std::collections::HashMap;
use std::io;
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::os::unix::net::UnixStream;

use crate::execution_host::protocol::{
    read_worker_message, write_worker_message, CommandSpec, CoordinatorInstallationId,
    CoordinatorMessage, HostBindingGeneration, RequestId, RuntimeExitStatus, RuntimeIdentity,
    RuntimeIncarnation, RuntimeOpSeq, SessionNamespaceId, TerminalSize, TerminateMode, WorkerError,
    WorkerErrorCode, WorkerInstanceId, WorkerMessage, WorkerRuntimeId,
};
use crate::execution_host::runtime_paths::WorkerRolePaths;
use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};
use crate::terminal::TerminalId;

use super::super::binding::DaemonBinding;
use super::super::dispatch::handle_request;
use super::super::event::{RuntimeLocalId, WorkerEvent};
use super::super::hook_ingress::WorkerHookIngress;
use super::super::host_job::HostJobResult;
use super::super::lifecycle::ConnectionOutcome;
use super::super::output::OutputLog;
use super::super::state::{CreateKind, CreateRequest, OpDisposition, RuntimeRecord, WorkerState};
use super::super::terminal::flush_state_events;
use super::super::util::DEFAULT_WORKER_SCROLLBACK_BYTES;
use super::support::{hello, test_binding, wait_for_worker_message, with_worker_connection};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_terminal_runs_command_and_captures_output() {
    let host_id = ExecutionHostId::new("ssh:test-worker").unwrap();
    let binding = DaemonBinding {
        installation_id: CoordinatorInstallationId::new("test-installation").unwrap(),
        session_namespace_id: SessionNamespaceId::new("test-session").unwrap(),
        execution_host_id: host_id.clone(),
        host_binding_generation: HostBindingGeneration::new(1),
        worker_instance_id: WorkerInstanceId::new("test-worker-instance").unwrap(),
        socket_path: std::env::temp_dir().join("gardn-test-worker.sock"),
    };
    let mut state = WorkerState::new(binding).unwrap();
    let location = ResourceLocation::new(host_id, HostPath::new(std::env::temp_dir()).unwrap());
    let (identity, _) = state
        .create_terminal(
            location,
            TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "printf worker-proof; sleep 30".to_string(),
                ],
                env: Vec::new(),
            }),
            Vec::new(),
            DEFAULT_WORKER_SCROLLBACK_BYTES,
        )
        .unwrap();

    let mut output = None;
    for _ in 0..100 {
        let bytes = state
            .runtime_record(&identity.runtime_id)
            .map(|record| record.output.checkpoint().1)
            .unwrap_or_default();
        if String::from_utf8_lossy(&bytes).contains("worker-proof") {
            output = Some(bytes);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let output = output.expect("remote command output should arrive");
    assert!(String::from_utf8_lossy(&output).contains("worker-proof"));

    state
        .try_send_event(WorkerEvent::StateChanged {
            local_id: state.runtime_record(&identity.runtime_id).unwrap().local_id,
            agent: Some(crate::detect::Agent::Codex),
            state: crate::detect::AgentState::Working,
            visible_blocker: false,
            visible_idle: false,
            visible_working: true,
            process_exited: false,
        })
        .unwrap();
    let (mut worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    flush_state_events(&mut state, &mut worker_stream).unwrap();
    let message = read_worker_message(&mut coordinator_stream).unwrap();
    assert!(matches!(
        message,
        WorkerMessage::TerminalStateChanged {
            identity: message_identity,
            agent: Some(crate::detect::Agent::Codex),
            state: crate::detect::AgentState::Working,
            visible_working: true,
            ..
        } if message_identity == identity
    ));

    state.shutdown_runtime_for_test(&identity.runtime_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_terminal_routes_authenticated_hook_report_without_coordinator_socket() {
    let binding = test_binding("hook-report", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding).unwrap();
    let script = r#"import json, os, socket, time
request = {
    "id": "hook-proof",
    "method": "pane.report_agent",
    "params": {
        "pane_id": os.environ["GARDN_PANE_ID"],
        "source": "gardn:test",
        "agent": "codex",
        "state": "working",
        "seq": 7
    }
}
client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
client.connect(os.environ["GARDN_SOCKET_PATH"])
client.sendall((json.dumps(request) + "\n").encode())
client.recv(4096)
client.close()
time.sleep(30)
"#;
    let (identity, _) = state
        .create_terminal(
            location,
            TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "python3".to_string(),
                args: vec!["-c".to_string(), script.to_string()],
                env: vec![
                    (
                        "GARDN_SOCKET_PATH".to_string(),
                        "/tmp/spoofed.sock".to_string(),
                    ),
                    ("GARDN_PANE_ID".to_string(), "spoofed-pane".to_string()),
                ],
            }),
            Vec::new(),
            DEFAULT_WORKER_SCROLLBACK_BYTES,
        )
        .unwrap();

    for _ in 0..100 {
        if state.next_hook_report().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        state.next_hook_report().is_some(),
        "managed hook report should reach the worker"
    );

    let (mut worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    flush_state_events(&mut state, &mut worker_stream).unwrap();
    let message = read_worker_message(&mut coordinator_stream).unwrap();
    assert!(matches!(
        message,
        WorkerMessage::AgentHookReported {
            identity: message_identity,
            report: crate::integration::host::WorkerHookReport::Agent(params),
        } if message_identity == identity
            && params.source == "gardn:test"
            && params.agent == "codex"
            && params.seq == Some(7)
    ));

    state.shutdown_runtime_for_test(&identity.runtime_id);
}

#[test]
fn worker_hook_ingress_rejects_unknown_runtime_token() {
    let binding = test_binding("hook-reject", 1);
    let hook_socket = binding.role_paths().hook_socket_path();
    let state = WorkerState::new(binding).unwrap();
    let mut client = UnixStream::connect(hook_socket).unwrap();
    let request = serde_json::json!({
        "id": "hook-reject",
        "method": "pane.report_agent",
        "params": {
            "pane_id": "not-a-runtime-token",
            "source": "gardn:test",
            "agent": "codex",
            "state": "working",
            "seq": 1
        }
    });
    std::io::Write::write_all(&mut client, format!("{request}\n").as_bytes()).unwrap();
    let mut response = String::new();
    std::io::Read::read_to_string(&mut client, &mut response).unwrap();

    assert!(response.contains("\"code\":\"unauthorized\""));
    assert!(state.next_hook_report().is_none());
}

#[test]
fn stalled_hook_client_does_not_block_authenticated_report() {
    let binding = test_binding("hook-stalled-client", 1);
    let ingress = WorkerHookIngress::start(binding.role_paths().hook_socket_path()).unwrap();
    let identity = RuntimeIdentity::new(
        binding.host_binding_generation,
        binding.worker_instance_id.clone(),
        WorkerRuntimeId::new("hook-runtime").unwrap(),
        RuntimeIncarnation::new(1),
    );
    let token = ingress.register(identity.clone()).unwrap();
    let _stalled_client = UnixStream::connect(ingress.socket_path()).unwrap();
    std::thread::sleep(Duration::from_millis(40));

    let mut authenticated = UnixStream::connect(ingress.socket_path()).unwrap();
    authenticated
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let request = serde_json::json!({
        "id": "hook-after-stall",
        "method": "pane.report_agent",
        "params": {
            "pane_id": token,
            "source": "gardn:test",
            "agent": "codex",
            "state": "working",
            "seq": 2
        }
    });
    std::io::Write::write_all(&mut authenticated, format!("{request}\n").as_bytes()).unwrap();
    let mut response = String::new();
    std::io::Read::read_to_string(&mut authenticated, &mut response).unwrap();

    assert!(response.contains("\"result\":{}"));
    assert!(matches!(
        ingress.next_report(),
        Some(report) if report.identity == identity
    ));
}

#[test]
fn socket_identity_includes_host_and_generation() {
    let installation = CoordinatorInstallationId::new("install-a").unwrap();
    let namespace = SessionNamespaceId::new("session-a").unwrap();
    let first = WorkerRolePaths::for_binding(
        &installation,
        &namespace,
        &ExecutionHostId::new("ssh:first").unwrap(),
        HostBindingGeneration::new(1),
    )
    .socket_path();
    let second = WorkerRolePaths::for_binding(
        &installation,
        &namespace,
        &ExecutionHostId::new("ssh:second").unwrap(),
        HostBindingGeneration::new(1),
    )
    .socket_path();
    let newer = WorkerRolePaths::for_binding(
        &installation,
        &namespace,
        &ExecutionHostId::new("ssh:first").unwrap(),
        HostBindingGeneration::new(2),
    )
    .socket_path();
    assert_ne!(first, second);
    assert_ne!(first, newer);
}

#[test]
fn tunnel_loss_preserves_runtime_and_reconnect_adopts_full_identity() {
    let binding = test_binding("reconnect", 4);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding.clone()).unwrap();

    let ((identity, resolved_location), first_outcome) =
        with_worker_connection(&mut state, hello(&binding, 4), |connection| {
            let ack: WorkerMessage = read_worker_message(connection).unwrap();
            assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));
            write_worker_message(
                connection,
                &CoordinatorMessage::CreateTerminal {
                    request_id: RequestId::new(41),
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
            let result: WorkerMessage = read_worker_message(connection).unwrap();
            match result {
                WorkerMessage::CreateTerminalResult {
                    identity: Some(identity),
                    location,
                    error: None,
                    ..
                } => (identity, location),
                other => panic!("unexpected create result: {other:?}"),
            }
        });
    assert!(matches!(first_outcome, ConnectionOutcome::Continue));
    assert!(state.contains_runtime(&identity.runtime_id));

    let adopted_identity = identity.clone();
    let adopted_location = resolved_location.clone();
    let (_, second_outcome) =
        with_worker_connection(&mut state, hello(&binding, 4), |connection| {
            let ack: WorkerMessage = read_worker_message(connection).unwrap();
            assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));
            write_worker_message(
                connection,
                &CoordinatorMessage::AdoptTerminal {
                    request_id: RequestId::new(42),
                    identity: adopted_identity.clone(),
                    location: adopted_location.clone(),
                },
            )
            .unwrap();
            let adopted: WorkerMessage = read_worker_message(connection).unwrap();
            assert!(matches!(
                &adopted,
                WorkerMessage::AdoptTerminalResult {
                    identity: Some(identity),
                    last_applied_op_seq,
                    error: None,
                    ..
                } if identity == &adopted_identity && last_applied_op_seq.get() == 0
            ));
            write_worker_message(
                connection,
                &CoordinatorMessage::Terminate {
                    request_id: RequestId::new(43),
                    identity: adopted_identity.clone(),
                    location: adopted_location.clone(),
                    mode: TerminateMode::Terminate,
                },
            )
            .unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let terminated = (0..32)
                .find_map(|_| match read_worker_message(connection).unwrap() {
                    message @ WorkerMessage::RequestAck { request_id, .. }
                        if request_id == RequestId::new(43) =>
                    {
                        Some(message)
                    }
                    _ => None,
                })
                .expect("terminate acknowledgement should arrive");
            assert!(matches!(
                terminated,
                WorkerMessage::RequestAck { error: None, .. }
            ));
        });
    assert!(matches!(second_outcome, ConnectionOutcome::Continue));
    assert!(!state.has_runtime_records());
}

#[test]
fn adopt_returns_last_applied_op_seq_and_next_input_applies_once() {
    let binding = test_binding("adopt-seq", 5);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding.clone()).unwrap();

    let (_, outcome) = with_worker_connection(&mut state, hello(&binding, 5), |connection| {
        let ack: WorkerMessage = read_worker_message(connection).unwrap();
        assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));
        write_worker_message(
            connection,
            &CoordinatorMessage::CreateTerminal {
                request_id: RequestId::new(11),
                location: location.clone(),
                size: TerminalSize { cols: 80, rows: 24 },
                command: Some(CommandSpec {
                    program: "/bin/sh".to_string(),
                    args: vec!["-c".to_string(), "sleep 300".to_string()],
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
        let (identity, resolved) = match created {
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
            &CoordinatorMessage::Input {
                request_id: RequestId::new(12),
                identity: identity.clone(),
                location: resolved.clone(),
                op_seq: RuntimeOpSeq::new(1),
                data: b"a".to_vec(),
            },
        )
        .unwrap();
        let _ = wait_for_worker_message(connection, |message| {
            matches!(
                message,
                WorkerMessage::RequestAck {
                    request_id,
                    error: None,
                } if *request_id == RequestId::new(12)
            )
        });
        write_worker_message(
            connection,
            &CoordinatorMessage::Input {
                request_id: RequestId::new(13),
                identity: identity.clone(),
                location: resolved.clone(),
                op_seq: RuntimeOpSeq::new(2),
                data: b"b".to_vec(),
            },
        )
        .unwrap();
        let _ = wait_for_worker_message(connection, |message| {
            matches!(
                message,
                WorkerMessage::RequestAck {
                    request_id,
                    error: None,
                } if *request_id == RequestId::new(13)
            )
        });

        // Adopt on the live runtime recovers last applied seq for coordinator resume.
        write_worker_message(
            connection,
            &CoordinatorMessage::AdoptTerminal {
                request_id: RequestId::new(21),
                identity: identity.clone(),
                location: resolved.clone(),
            },
        )
        .unwrap();
        let adopted = wait_for_worker_message(connection, |message| {
            matches!(message, WorkerMessage::AdoptTerminalResult { .. })
        });
        match adopted {
            WorkerMessage::AdoptTerminalResult {
                identity: Some(adopted_identity),
                last_applied_op_seq,
                error: None,
                ..
            } => {
                assert_eq!(adopted_identity, identity);
                assert_eq!(last_applied_op_seq.get(), 2);
            }
            other => panic!("unexpected adopt result: {other:?}"),
        }

        // Same-seq replay of last applied op is idempotent.
        write_worker_message(
            connection,
            &CoordinatorMessage::Input {
                request_id: RequestId::new(22),
                identity: identity.clone(),
                location: resolved.clone(),
                op_seq: RuntimeOpSeq::new(2),
                data: b"b".to_vec(),
            },
        )
        .unwrap();
        let replay_ack = wait_for_worker_message(connection, |message| {
            matches!(
                message,
                WorkerMessage::RequestAck {
                    request_id,
                    ..
                } if *request_id == RequestId::new(22)
            )
        });
        assert!(
            matches!(
                &replay_ack,
                WorkerMessage::RequestAck {
                    request_id,
                    error: None,
                } if *request_id == RequestId::new(22)
            ),
            "same-seq replay should ack ok, got {replay_ack:?}"
        );

        // First post-adopt op must be last+1 and apply exactly once.
        write_worker_message(
            connection,
            &CoordinatorMessage::Input {
                request_id: RequestId::new(23),
                identity: identity.clone(),
                location: resolved.clone(),
                op_seq: RuntimeOpSeq::new(3),
                data: b"c".to_vec(),
            },
        )
        .unwrap();
        let apply_ack = wait_for_worker_message(connection, |message| {
            matches!(
                message,
                WorkerMessage::RequestAck {
                    request_id,
                    ..
                } if *request_id == RequestId::new(23)
            )
        });
        assert!(
            matches!(
                &apply_ack,
                WorkerMessage::RequestAck {
                    request_id,
                    error: None,
                } if *request_id == RequestId::new(23)
            ),
            "last+1 apply should ack ok, got {apply_ack:?}"
        );

        // Stale seq below last applied is rejected.
        write_worker_message(
            connection,
            &CoordinatorMessage::Input {
                request_id: RequestId::new(24),
                identity: identity.clone(),
                location: resolved.clone(),
                op_seq: RuntimeOpSeq::new(1),
                data: b"z".to_vec(),
            },
        )
        .unwrap();
        let stale = wait_for_worker_message(connection, |message| {
            matches!(
                message,
                WorkerMessage::RequestAck {
                    request_id,
                    ..
                } if *request_id == RequestId::new(24)
            )
        });
        assert!(matches!(
            stale,
            WorkerMessage::RequestAck {
                request_id,
                error: Some(WorkerError {
                    code: WorkerErrorCode::Conflict,
                    ..
                }),
            } if request_id == RequestId::new(24)
        ));

        write_worker_message(
            connection,
            &CoordinatorMessage::Terminate {
                request_id: RequestId::new(25),
                identity: identity.clone(),
                location: resolved,
                mode: TerminateMode::Terminate,
            },
        )
        .unwrap();
        let terminated = wait_for_worker_message(connection, |message| {
            matches!(
                message,
                WorkerMessage::RequestAck {
                    request_id,
                    ..
                } if *request_id == RequestId::new(25)
            )
        });
        assert!(matches!(
            terminated,
            WorkerMessage::RequestAck {
                request_id,
                error: None,
            } if request_id == RequestId::new(25)
        ));
    });
    assert!(matches!(outcome, ConnectionOutcome::Continue));
    assert!(!state.has_runtime_records());
}

#[test]
fn stale_host_binding_fence_is_rejected_before_requests() {
    let binding = test_binding("stale-fence", 8);
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let (_, outcome) = with_worker_connection(&mut state, hello(&binding, 7), |connection| {
        let ack: WorkerMessage = read_worker_message(connection).unwrap();
        assert!(matches!(
            ack,
            WorkerMessage::HelloAck {
                error: Some(WorkerError {
                    code: WorkerErrorCode::BindingMismatch,
                    ..
                }),
                ..
            }
        ));
    });
    assert!(matches!(outcome, ConnectionOutcome::Continue));
    assert!(!state.has_runtime_records());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repeated_create_request_returns_one_runtime_identity() {
    let binding = test_binding("idempotent-create", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding).unwrap();
    let request = CreateRequest {
        kind: CreateKind::Terminal,
        location,
        size: TerminalSize { cols: 80, rows: 24 },
        command: Some(CommandSpec {
            program: "/bin/sh".to_string(),
            args: vec!["-c".to_string(), "sleep 30".to_string()],
            env: Vec::new(),
        }),
        env: Vec::new(),
        scrollback_limit_bytes: DEFAULT_WORKER_SCROLLBACK_BYTES,
    };

    let first = state
        .create_once(RequestId::new(91), request.clone())
        .unwrap();
    let replay = state
        .create_once(RequestId::new(91), request.clone())
        .unwrap();
    assert_eq!(replay, first);
    assert_eq!(state.runtime_record_count(), 1);

    let mut conflicting = request;
    conflicting.size.cols = 120;
    let error = state
        .create_once(RequestId::new(91), conflicting)
        .unwrap_err();
    assert_eq!(error.code, WorkerErrorCode::Conflict);
    state.shutdown_runtime_for_test(&first.0.runtime_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tilde_location_resolves_against_worker_home_before_create_result() {
    let _environment = crate::integration::integration_env_lock();
    let home = std::env::temp_dir().join(format!(
        "gardn-worker-home-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let project = home.join("projects").join("demo");
    std::fs::create_dir_all(&project).unwrap();
    let _home = crate::config::TestEnvVar::set("HOME", &home);
    let binding = test_binding("tilde", 1);
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let raw_location = ResourceLocation::new(
        binding.execution_host_id,
        HostPath::new("~/projects/demo").unwrap(),
    );

    let (identity, resolved) = state
        .create_terminal(
            raw_location,
            TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 30".to_string()],
                env: Vec::new(),
            }),
            Vec::new(),
            DEFAULT_WORKER_SCROLLBACK_BYTES,
        )
        .unwrap();
    assert_eq!(resolved.path.as_path(), project);
    state.shutdown_runtime_for_test(&identity.runtime_id);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn op_disposition_accepts_only_last_or_last_plus_one() {
    let record = RuntimeRecord {
        terminal_id: TerminalId::alloc(),
        local_id: RuntimeLocalId::new(1),
        identity: RuntimeIdentity::new(
            HostBindingGeneration::new(1),
            WorkerInstanceId::new("worker-a").unwrap(),
            WorkerRuntimeId::new("runtime-a").unwrap(),
            RuntimeIncarnation::new(1),
        ),
        location: ResourceLocation::new(
            ExecutionHostId::new("ssh:test").unwrap(),
            HostPath::new("/tmp").unwrap(),
        ),
        output: OutputLog::new(DEFAULT_WORKER_SCROLLBACK_BYTES),
        last_op_seq: 3,
    };
    assert_eq!(
        WorkerState::op_disposition(&record, RuntimeOpSeq::new(3)).unwrap(),
        OpDisposition::Replay
    );
    assert_eq!(
        WorkerState::op_disposition(&record, RuntimeOpSeq::new(4)).unwrap(),
        OpDisposition::Apply
    );
    let gap = WorkerState::op_disposition(&record, RuntimeOpSeq::new(5)).unwrap_err();
    assert_eq!(gap.code, WorkerErrorCode::Conflict);
    let stale = WorkerState::op_disposition(&record, RuntimeOpSeq::new(2)).unwrap_err();
    assert_eq!(stale.code, WorkerErrorCode::Conflict);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pane_died_emits_exactly_one_runtime_exit_and_removes_record() {
    let binding = test_binding("runtime-exit", 1);
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let (identity, resolved) = state
        .create_terminal(
            location,
            TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "exit 0".to_string()],
                env: Vec::new(),
            }),
            Vec::new(),
            DEFAULT_WORKER_SCROLLBACK_BYTES,
        )
        .unwrap();
    let local_id = state.runtime_record(&identity.runtime_id).unwrap().local_id;
    // Wait for the short-lived child wait to complete, then drain residual events.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match state.try_recv_event() {
            Ok(WorkerEvent::RuntimeExit { local_id: died, .. }) if died == local_id => break,
            Ok(_) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
    while state.try_recv_event().is_ok() {}
    state
        .try_send_event(WorkerEvent::RuntimeExit {
            local_id,
            exit_code: Some(7),
            exit_signal: None,
        })
        .unwrap();
    let (mut worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    flush_state_events(&mut state, &mut worker_stream).unwrap();
    let message = wait_for_worker_message(&mut coordinator_stream, |message| {
        matches!(
            message,
            WorkerMessage::RuntimeExit {
                status: RuntimeExitStatus::Code(7),
                ..
            }
        )
    });
    assert!(matches!(
        message,
        WorkerMessage::RuntimeExit {
            identity: message_identity,
            location: message_location,
            status: RuntimeExitStatus::Code(7),
        } if message_identity == identity && message_location == resolved
    ));
    assert!(!state.has_runtime_records());
    // Duplicate PaneDied after removal must not emit another RuntimeExit.
    state
        .try_send_event(WorkerEvent::RuntimeExit {
            local_id,
            exit_code: Some(7),
            exit_signal: None,
        })
        .unwrap();
    flush_state_events(&mut state, &mut worker_stream).unwrap();
    // Keep the writer side open and use a short non-blocking poll: no second frame.
    coordinator_stream.set_nonblocking(true).unwrap();
    match read_worker_message::<_, WorkerMessage>(&mut coordinator_stream) {
        Err(crate::protocol::FramingError::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) => {}
        Err(crate::protocol::FramingError::UnexpectedEof) => {}
        Ok(other) => panic!("expected no second RuntimeExit, got {other:?}"),
        Err(other) => panic!("unexpected framing error: {other}"),
    }
    drop(worker_stream);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pane_died_signal_exit_maps_to_runtime_exit_signal() {
    let binding = test_binding("runtime-exit-signal", 1);
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let (identity, _) = state
        .create_terminal(
            location,
            TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "exit 0".to_string()],
                env: Vec::new(),
            }),
            Vec::new(),
            DEFAULT_WORKER_SCROLLBACK_BYTES,
        )
        .unwrap();
    let local_id = state.runtime_record(&identity.runtime_id).unwrap().local_id;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match state.try_recv_event() {
            Ok(WorkerEvent::RuntimeExit { local_id: died, .. }) if died == local_id => break,
            Ok(_) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
    while state.try_recv_event().is_ok() {}
    state
        .try_send_event(WorkerEvent::RuntimeExit {
            local_id,
            exit_code: None,
            exit_signal: Some(15),
        })
        .unwrap();
    let (mut worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    flush_state_events(&mut state, &mut worker_stream).unwrap();
    let message = wait_for_worker_message(&mut coordinator_stream, |message| {
        matches!(
            message,
            WorkerMessage::RuntimeExit {
                status: RuntimeExitStatus::Signal(15),
                ..
            }
        )
    });
    assert!(matches!(
        message,
        WorkerMessage::RuntimeExit {
            status: RuntimeExitStatus::Signal(15),
            ..
        }
    ));
    assert!(!state.has_runtime_records());
    let _ = identity;
}

#[test]
fn terminate_unknown_or_obsolete_runtime_converges_as_successful_absence() {
    let binding = test_binding("terminate-absence", 1);
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let unknown = RuntimeIdentity::new(
        binding.host_binding_generation,
        binding.worker_instance_id.clone(),
        WorkerRuntimeId::new("runtime-missing").unwrap(),
        RuntimeIncarnation::new(9),
    );
    let (mut worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    let (job_tx, _job_rx) = std_mpsc::channel::<HostJobResult>();
    handle_request(
        CoordinatorMessage::Terminate {
            request_id: RequestId::new(11),
            identity: unknown.clone(),
            location: location.clone(),
            mode: TerminateMode::Terminate,
        },
        &mut state,
        &mut worker_stream,
        &mut HashMap::new(),
        &job_tx,
    )
    .unwrap();
    let ack = read_worker_message(&mut coordinator_stream).unwrap();
    assert!(matches!(
        ack,
        WorkerMessage::RequestAck {
            request_id,
            error: None,
        } if request_id == RequestId::new(11)
    ));
    assert!(state.has_termination_tombstone(&unknown));

    // Replay after tombstone remains successful absence.
    handle_request(
        CoordinatorMessage::Terminate {
            request_id: RequestId::new(12),
            identity: unknown.clone(),
            location,
            mode: TerminateMode::Terminate,
        },
        &mut state,
        &mut worker_stream,
        &mut HashMap::new(),
        &job_tx,
    )
    .unwrap();
    let replay = read_worker_message(&mut coordinator_stream).unwrap();
    assert!(matches!(
        replay,
        WorkerMessage::RequestAck {
            request_id,
            error: None,
        } if request_id == RequestId::new(12)
    ));
}
