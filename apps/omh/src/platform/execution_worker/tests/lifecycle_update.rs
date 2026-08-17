use std::io;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};

use crate::execution_host::lifecycle::{
    read_legacy_lifecycle_frame, read_lifecycle_frame, write_lifecycle_frame, ActivateReply,
    ActivateRequest, LifecycleDecision,
};
use crate::execution_host::protocol::{
    read_worker_message, write_worker_message, CommandSpec, CoordinatorMessage, RequestId,
    TerminalSize, TerminateMode, WorkerErrorCode, WorkerMessage, PROTOCOL_VERSION,
};
use crate::execution_host::runtime_paths::{
    inventory_owned_bindings, retire_owned_bindings, BindingOwnershipManifest, WorkerRolePaths,
};
use crate::execution_host::{HostPath, ResourceLocation};

use super::super::binding::DaemonBinding;
use super::super::event::WorkerEvent;
use super::super::lifecycle::{
    acquire_daemon_lock, serve_connection, shutdown_owned_binding, try_activate_existing_daemon,
    BridgeActivateResult, ConnectionOutcome,
};
use super::super::state::WorkerState;
use super::super::util::{is_disconnect, DEFAULT_WORKER_SCROLLBACK_BYTES, WORKER_APP_VERSION};
use super::support::{hello, test_binding};

fn start_locked_daemon(
    binding: DaemonBinding,
    configure: impl FnOnce(&mut WorkerState) + Send + 'static,
) -> (
    std::thread::JoinHandle<io::Result<()>>,
    WorkerRolePaths,
    std::sync::mpsc::Receiver<()>,
) {
    let role_paths = binding.role_paths();
    role_paths.prepare().unwrap();
    let returned_paths = role_paths.clone();
    let lock_path = role_paths.lock_path();
    let socket_path = binding.socket_path.clone();
    let (ready_tx, ready_rx) = std_mpsc::channel();
    let handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let binding_lock = acquire_daemon_lock(&lock_path)?;
            role_paths.remove_socket_if_present()?;
            let listener = UnixListener::bind(&socket_path)?;
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
            let mut state = WorkerState::new(binding)?;
            configure(&mut state);
            let _ = ready_tx.send(());
            let result = loop {
                let (stream, _) = listener.accept()?;
                match serve_connection(stream, &mut state) {
                    Ok(ConnectionOutcome::Continue) => {}
                    Ok(ConnectionOutcome::Shutdown) => break Ok(()),
                    Err(err) if is_disconnect(&err) => {}
                    Err(err) => break Err(err),
                }
            };
            drop(listener);
            drop(state);
            let cleanup = role_paths.cleanup();
            drop(binding_lock);
            match result {
                Err(error) => Err(error),
                Ok(()) => cleanup,
            }
        })
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (handle, returned_paths, ready_rx)
}

fn activate_on_binding(binding: &DaemonBinding) -> io::Result<BridgeActivateResult> {
    try_activate_existing_daemon(binding)
}

#[test]
fn incompatible_idle_daemon_is_replaced_cooperatively() {
    let binding = test_binding("lifecycle-idle", 1);
    let lock_path = binding.role_paths().lock_path();
    let (daemon, role_paths, _) = start_locked_daemon(binding.clone(), |state| {
        state.set_advertised_versions_for_test("0.0.1", PROTOCOL_VERSION.saturating_add(1));
    });

    let result = activate_on_binding(&binding).unwrap();
    assert!(matches!(result, BridgeActivateResult::ShuttingDownIdle));

    daemon.join().unwrap().unwrap();
    // Lock inode released; successor can acquire and bind.
    let successor_lock = acquire_daemon_lock(&lock_path).unwrap();
    assert!(role_paths.lock_path().exists());
    assert!(!role_paths.socket_path().exists());
    drop(successor_lock);
}

#[test]
fn legacy_v1_idle_daemon_is_drained_before_replacement() {
    let binding = test_binding("lifecycle-legacy-idle", 1);
    let role_paths = binding.role_paths();
    role_paths.prepare().unwrap();
    role_paths.remove_socket_if_present().unwrap();
    let listener = UnixListener::bind(&binding.socket_path).unwrap();
    let server_binding = binding.clone();
    let daemon = std::thread::spawn(move || {
        let (mut first, _) = listener.accept().unwrap();
        let _ = read_lifecycle_frame(&mut first).unwrap();
        drop(first);

        let (mut legacy, _) = listener.accept().unwrap();
        let request = read_legacy_lifecycle_frame(&mut legacy).unwrap();
        assert_eq!(&request[12..14], &1u16.to_le_bytes());
        let mut reply = ActivateReply::new(
            server_binding.binding_digest(),
            LifecycleDecision::ShuttingDownIdle,
            PROTOCOL_VERSION.saturating_sub(1),
            0,
            "0.3.1",
            "legacy-worker",
        )
        .unwrap()
        .encode()
        .unwrap();
        reply[12..14].copy_from_slice(&1u16.to_le_bytes());
        write_lifecycle_frame(&mut legacy, &reply).unwrap();
    });

    let result = try_activate_existing_daemon(&binding).unwrap();
    assert!(matches!(result, BridgeActivateResult::ShuttingDownIdle));
    daemon.join().unwrap();
    role_paths.cleanup().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compatible_busy_daemon_is_reused_and_preserves_runtime() {
    let binding = test_binding("lifecycle-busy", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let (identity_tx, identity_rx) = std_mpsc::channel();
    let (daemon, role_paths, _) = start_locked_daemon(binding.clone(), move |state| {
        state.set_artifact_digest_for_test([0x5a; 32]);
        let (identity, _) = state
            .create_terminal(
                location,
                TerminalSize { cols: 80, rows: 24 },
                Some(CommandSpec {
                    program: "/bin/sh".to_string(),
                    args: vec!["-c".to_string(), "sleep 120".to_string()],
                    env: Vec::new(),
                }),
                Vec::new(),
                DEFAULT_WORKER_SCROLLBACK_BYTES,
            )
            .unwrap();
        identity_tx.send(identity).unwrap();
    });

    let identity = identity_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Give the child a moment to start.
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(matches!(
        activate_on_binding(&binding).unwrap(),
        BridgeActivateResult::UseExisting(_)
    ));

    // Incumbent remains owner; socket/lock/runtime preserved.
    assert!(role_paths.socket_path().exists());
    assert!(role_paths.lock_path().exists());
    assert!(acquire_daemon_lock(&role_paths.lock_path()).is_err());

    // A bridge from the incumbent artifact can still reconnect normally.
    let mut stream = UnixStream::connect(&binding.socket_path).unwrap();
    write_worker_message(&mut stream, &hello(&binding, 1)).unwrap();
    let ack: WorkerMessage = read_worker_message(&mut stream).unwrap();
    assert!(matches!(
        ack,
        WorkerMessage::HelloAck {
            error: None,
            worker_instance_id,
            ..
        } if worker_instance_id == binding.worker_instance_id
    ));

    // Cleanup live child via normal terminate path on a fresh connection.
    drop(stream);
    let mut stream = UnixStream::connect(&binding.socket_path).unwrap();
    // Lifecycle then Hello so we can send Terminate.
    let request = ActivateRequest::new(
        binding.binding_digest(),
        [0x5a; 32],
        PROTOCOL_VERSION,
        WORKER_APP_VERSION,
    )
    .unwrap();
    write_lifecycle_frame(&mut stream, &request.encode().unwrap()).unwrap();
    let reply = ActivateReply::decode(&read_lifecycle_frame(&mut stream).unwrap()).unwrap();
    assert_eq!(reply.decision, LifecycleDecision::UseExisting);
    write_worker_message(&mut stream, &hello(&binding, 1)).unwrap();
    let _: WorkerMessage = read_worker_message(&mut stream).unwrap();
    write_worker_message(
        &mut stream,
        &CoordinatorMessage::Terminate {
            request_id: RequestId::new(9),
            identity: identity.clone(),
            location: ResourceLocation::new(
                binding.execution_host_id.clone(),
                HostPath::new(std::env::temp_dir()).unwrap(),
            ),
            mode: TerminateMode::Terminate,
        },
    )
    .unwrap();
    let _: WorkerMessage = read_worker_message(&mut stream).unwrap();
    write_worker_message(
        &mut stream,
        &CoordinatorMessage::Shutdown {
            request_id: RequestId::new(10),
        },
    )
    .unwrap();
    let _: WorkerMessage = read_worker_message(&mut stream).unwrap();
    drop(stream);
    daemon.join().unwrap().unwrap();
}

#[test]
fn unsupported_lifecycle_daemon_is_never_unlinked_or_signaled() {
    let binding = test_binding("lifecycle-unsupported", 1);
    let role_paths = binding.role_paths();
    role_paths.prepare().unwrap();
    let lock = acquire_daemon_lock(&role_paths.lock_path()).unwrap();
    role_paths.remove_socket_if_present().unwrap();
    let listener = UnixListener::bind(&binding.socket_path).unwrap();
    std::fs::set_permissions(&binding.socket_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    // Pre-lifecycle daemon: accepts connections and only speaks worker framing.
    let socket_path = binding.socket_path.clone();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // Any lifecycle frame looks oversized to normal framing.
        let err = read_worker_message::<_, CoordinatorMessage>(&mut stream).unwrap_err();
        assert!(matches!(
            err,
            crate::protocol::FramingError::Oversized { .. }
        ));
        // Leave socket/lock alone; just drop the connection.
        drop(stream);
        // Keep listener alive briefly so path still exists for the assertion.
        std::thread::sleep(Duration::from_millis(200));
        drop(listener);
        Ok::<(), io::Error>(())
    });

    match activate_on_binding(&binding).unwrap() {
        BridgeActivateResult::Rejected(ack) => match *ack {
            WorkerMessage::HelloAck {
                error: Some(error), ..
            } => {
                assert_eq!(error.code, WorkerErrorCode::ProtocolMismatch);
            }
            other => panic!("expected unsupported rejection HelloAck, got {other:?}"),
        },
        other => panic!("expected unsupported rejection, got {other:?}"),
    }

    assert!(
        socket_path.exists(),
        "bridge must not unlink incumbent socket"
    );
    assert!(role_paths.lock_path().exists());
    assert!(
        acquire_daemon_lock(&role_paths.lock_path()).is_err(),
        "incumbent lock must remain held"
    );
    worker.join().unwrap().unwrap();
    drop(lock);
}

#[test]
fn two_bridges_converge_on_one_worker_instance() {
    let binding = test_binding("lifecycle-converge", 1);
    // Single lock-owning daemon. Serial accept means bridges handshakes
    // one at a time, but both must observe the same worker instance id.
    let (daemon, role_paths, _) = start_locked_daemon(binding.clone(), |_| {});

    let mut instances = Vec::new();
    for _ in 0..2 {
        match activate_on_binding(&binding).unwrap() {
            BridgeActivateResult::UseExisting(mut stream) => {
                write_worker_message(&mut stream, &hello(&binding, 1)).unwrap();
                let ack: WorkerMessage = read_worker_message(&mut stream).unwrap();
                match ack {
                    WorkerMessage::HelloAck {
                        error: None,
                        worker_instance_id,
                        ..
                    } => instances.push(worker_instance_id),
                    other => panic!("expected successful HelloAck, got {other:?}"),
                }
                // Drop stream so the serial accept loop can take the next bridge.
                drop(stream);
            }
            other => panic!("expected UseExisting for live compatible daemon, got {other:?}"),
        }
    }
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0], instances[1]);
    assert_eq!(instances[0], binding.worker_instance_id);

    // Flock still owned by the daemon; a contender cannot steal the binding.
    assert!(acquire_daemon_lock(&role_paths.lock_path()).is_err());

    // Drain the idle daemon via lifecycle so the thread exits.
    let mut stream = UnixStream::connect(&binding.socket_path).unwrap();
    let request = ActivateRequest::new(
        binding.binding_digest(),
        super::super::artifact_digest().unwrap(),
        PROTOCOL_VERSION.saturating_add(1),
        "9.9.9",
    )
    .unwrap();
    write_lifecycle_frame(&mut stream, &request.encode().unwrap()).unwrap();
    let reply = ActivateReply::decode(&read_lifecycle_frame(&mut stream).unwrap()).unwrap();
    assert_eq!(reply.decision, LifecycleDecision::ShuttingDownIdle);
    drop(stream);
    daemon.join().unwrap().unwrap();
}

#[test]
fn stale_socket_is_removed_only_after_lock_acquisition() {
    let binding = test_binding("lifecycle-stale-socket", 1);
    let role_paths = binding.role_paths();
    role_paths.prepare().unwrap();
    // Plant a stale socket without a live listener or lock owner.
    std::fs::write(&binding.socket_path, b"stale").unwrap();
    assert!(binding.socket_path.exists());
    assert!(
        binding.socket_path.exists(),
        "bridge path must not remove socket without lock"
    );

    // Lock owner may remove stale socket and bind.
    let lock = acquire_daemon_lock(&role_paths.lock_path()).unwrap();
    role_paths.remove_socket_if_present().unwrap();
    assert!(!binding.socket_path.exists());
    let listener = UnixListener::bind(&binding.socket_path).unwrap();
    assert!(binding.socket_path.exists());
    drop(listener);
    role_paths.cleanup().unwrap();
    assert!(!binding.socket_path.exists());
    assert!(role_paths.lock_path().exists(), "lock inode must persist");
    drop(lock);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_message_is_busy_while_runtime_owned() {
    let binding = test_binding("shutdown-busy", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let (identity, _) = state
        .create_terminal(
            location.clone(),
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

    let (worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async { serve_connection(worker_stream, &mut state) })
        });
        write_worker_message(&mut coordinator_stream, &hello(&binding, 1)).unwrap();
        let ack: WorkerMessage = read_worker_message(&mut coordinator_stream).unwrap();
        assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));
        write_worker_message(
            &mut coordinator_stream,
            &CoordinatorMessage::Shutdown {
                request_id: RequestId::new(42),
            },
        )
        .unwrap();
        let ack: WorkerMessage = read_worker_message(&mut coordinator_stream).unwrap();
        match ack {
            WorkerMessage::RequestAck {
                request_id,
                error: Some(error),
            } => {
                assert_eq!(request_id, RequestId::new(42));
                assert_eq!(error.code, WorkerErrorCode::Busy);
            }
            other => panic!("expected busy shutdown ack, got {other:?}"),
        }
        // Drop coordinator to end the session; ignore disconnect write races.
        drop(coordinator_stream);
        match worker.join().unwrap() {
            Ok(outcome) => outcome,
            Err(err) if is_disconnect(&err) => ConnectionOutcome::Continue,
            Err(err) => panic!("unexpected worker error: {err}"),
        }
    });
    assert!(matches!(outcome, ConnectionOutcome::Continue));
    assert_eq!(state.owned_runtime_count(), 1);
    state.shutdown_runtime_for_test(&identity.runtime_id);
}

#[test]
fn shutdown_message_exits_when_worker_is_empty() {
    let binding = test_binding("shutdown-empty", 1);
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let (worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async { serve_connection(worker_stream, &mut state) })
        });
        write_worker_message(&mut coordinator_stream, &hello(&binding, 1)).unwrap();
        let ack: WorkerMessage = read_worker_message(&mut coordinator_stream).unwrap();
        assert!(matches!(ack, WorkerMessage::HelloAck { error: None, .. }));
        write_worker_message(
            &mut coordinator_stream,
            &CoordinatorMessage::Shutdown {
                request_id: RequestId::new(7),
            },
        )
        .unwrap();
        let ack: WorkerMessage = read_worker_message(&mut coordinator_stream).unwrap();
        assert!(matches!(
            ack,
            WorkerMessage::RequestAck {
                request_id,
                error: None,
            } if request_id == RequestId::new(7)
        ));
        // Keep the coordinator side open until the worker reports Shutdown so
        // the final RequestAck write cannot race a BrokenPipe on drop.
        let outcome = worker.join().unwrap().unwrap();
        drop(coordinator_stream);
        outcome
    });
    assert!(matches!(outcome, ConnectionOutcome::Shutdown));
}

#[test]
fn retirement_reconciles_a_queued_runtime_exit_before_removing_its_binding() {
    let binding = test_binding("retire-disconnected", 1);
    let role_paths = binding.role_paths();
    let ownership = BindingOwnershipManifest::new(
        &binding.installation_id,
        &binding.session_namespace_id,
        &binding.execution_host_id,
        binding.host_binding_generation,
        binding.worker_instance_id.to_string(),
        WORKER_APP_VERSION,
    );
    role_paths.write_ownership_manifest(&ownership).unwrap();
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let (daemon, _, _) = start_locked_daemon(binding.clone(), move |state| {
        state.set_artifact_digest_for_test([0x5a; 32]);
        let (identity, _) = state
            .create_terminal(
                location,
                TerminalSize { cols: 80, rows: 24 },
                Some(CommandSpec {
                    program: "/bin/sh".to_string(),
                    args: vec!["-c".to_string(), "sleep 120".to_string()],
                    env: Vec::new(),
                }),
                Vec::new(),
                DEFAULT_WORKER_SCROLLBACK_BYTES,
            )
            .unwrap();
        let local_id = state
            .runtime_record(&identity.runtime_id)
            .expect("runtime should be registered")
            .local_id;
        state
            .try_send_event(WorkerEvent::RuntimeExit {
                local_id,
                exit_code: Some(0),
                exit_signal: None,
            })
            .unwrap();
    });
    let inventory = inventory_owned_bindings(
        binding.installation_id.as_str(),
        binding.execution_host_id.as_str(),
    )
    .unwrap();
    let entry = inventory
        .bindings
        .first()
        .expect("live binding should be inventoried");
    assert!(entry.lock_live);
    assert!(matches!(
        activate_on_binding(&binding).unwrap(),
        BridgeActivateResult::UseExisting(_)
    ));

    shutdown_owned_binding(entry).unwrap();
    daemon.join().unwrap().unwrap();
    let report = retire_owned_bindings(
        binding.installation_id.as_str(),
        binding.execution_host_id.as_str(),
    )
    .unwrap();

    assert_eq!(report.removed_bindings.len(), 1);
    assert!(report.blocked_bindings.is_empty());
}

#[test]
fn ordinary_cleanup_preserves_lock_inode_after_listener_drop() {
    let binding = test_binding("cleanup-lock", 1);
    let role_paths = binding.role_paths();
    role_paths.prepare().unwrap();
    let lock = acquire_daemon_lock(&role_paths.lock_path()).unwrap();
    role_paths.remove_socket_if_present().unwrap();
    let listener = UnixListener::bind(&binding.socket_path).unwrap();
    drop(listener);
    role_paths.cleanup().unwrap();
    assert!(!binding.socket_path.exists());
    assert!(role_paths.lock_path().exists());
    // Same inode remains lockable by the current owner and after drop.
    drop(lock);
    let relock = acquire_daemon_lock(&role_paths.lock_path()).unwrap();
    drop(relock);
}
