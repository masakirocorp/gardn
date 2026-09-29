use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::execution_host::protocol::{
    read_worker_message, write_worker_message, AttachResume, CommandSpec, CoordinatorMessage,
    OutputRevision, RequestId, RuntimeOpSeq, WorkerMessage,
};
use crate::execution_host::{
    ExecutionHostEvent, ExecutionHostId, ExecutionHostManager, HostPath, ResourceLocation,
};
use crate::terminal::{TerminalId, TerminalRuntime};

use super::super::state::WorkerState;
use super::support::{
    hello, tempfile_dir, test_binding, wait_for_worker_message, with_worker_connection,
};

fn send_pending(connection: &mut UnixStream, commands: &Arc<Mutex<Vec<CoordinatorMessage>>>) {
    for message in std::mem::take(&mut *commands.lock().unwrap()) {
        write_worker_message(connection, &message).unwrap();
    }
}

fn apply_worker_message(
    manager: &mut ExecutionHostManager,
    runtime: &TerminalRuntime,
    host: &ExecutionHostId,
    message: WorkerMessage,
) -> Option<OutputRevision> {
    let revision = match &message {
        WorkerMessage::OutputCheckpoint { revision, .. }
        | WorkerMessage::OutputDelta { revision, .. } => Some(*revision),
        _ => None,
    };
    let mut events = Vec::new();
    manager.route_worker_message(host.clone(), message, &mut events);
    let mut applied = false;
    for event in events {
        match event {
            ExecutionHostEvent::TerminalSnapshot {
                terminal_id,
                identity,
                revision,
                data,
            } => {
                if manager.terminal_snapshot_pending(&terminal_id, &identity, revision) {
                    runtime.restore_snapshot(&data).unwrap();
                    applied =
                        manager.acknowledge_terminal_snapshot(&terminal_id, &identity, revision);
                }
            }
            ExecutionHostEvent::TerminalOutput { data, .. } => {
                runtime.process_remote_output(&data);
                applied = true;
            }
            ExecutionHostEvent::TerminalFailed { message, .. } => {
                panic!("remote terminal failed: {message}");
            }
            _ => {}
        }
    }
    applied.then_some(revision).flatten()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evicted_after_revision_recovers_through_coordinator_and_continues_output() {
    let binding = test_binding("checkpoint-recovery", 4);
    let host = binding.execution_host_id.clone();
    let root = tempfile_dir("gardn-checkpoint-recovery");
    let gate = root.join("continue");
    let location = ResourceLocation::new(host.clone(), HostPath::new(&root).unwrap());
    let mut state = WorkerState::new(binding.clone()).unwrap();
    let mut manager = ExecutionHostManager::new(
        binding.installation_id.clone(),
        binding.session_namespace_id.clone(),
    );
    let commands = manager.connect_test_host(host.clone());
    let (events, _event_rx) = tokio::sync::mpsc::channel(8);
    let terminal_id = TerminalId::alloc();
    let command = r#"printf 'PRIMARY-PERSISTENT\033[?2004h\033[?1049hALT-PERSISTENT'; while [ ! -f "$RECOVERY_GATE" ]; do sleep 0.01; done; i=0; while [ "$i" -lt 4096 ]; do printf '\033[0m'; i=$((i + 1)); done; printf 'RECOVERY-EVICTED'; read -r checkpoint_resume; printf '\033[?1049lPRIMARY-AFTER-RECOVERY'; sleep 30"#;
    let runtime = manager
        .create_terminal(
            terminal_id,
            crate::layout::PaneId::alloc(),
            location.clone(),
            24,
            80,
            128,
            crate::terminal_theme::TerminalTheme::default(),
            events,
            Some(CommandSpec {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), command.into()],
                env: vec![("RECOVERY_GATE".into(), gate.to_string_lossy().into_owned())],
            }),
            Vec::new(),
        )
        .unwrap();

    let (identity, acknowledged) =
        with_worker_connection(&mut state, hello(&binding, 4), |connection| {
            assert!(matches!(
                read_worker_message(connection).unwrap(),
                WorkerMessage::HelloAck { error: None, .. }
            ));
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            send_pending(connection, &commands);
            let mut identity = None;
            let mut acknowledged = None;
            while !runtime.visible_text().contains("ALT-PERSISTENT") {
                let message = read_worker_message(connection).unwrap();
                if let WorkerMessage::CreateTerminalResult {
                    identity: created,
                    error: None,
                    ..
                } = &message
                {
                    identity = created.clone();
                }
                acknowledged =
                    apply_worker_message(&mut manager, &runtime, &host, message).or(acknowledged);
                send_pending(connection, &commands);
            }
            (identity.unwrap(), acknowledged.unwrap())
        })
        .0;
    assert!(acknowledged.get() > 0);

    std::fs::write(&gate, b"continue").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let original = state
            .runtime_for_record(state.runtime_record(&identity.runtime_id).unwrap())
            .unwrap();
        if original.visible_text().contains("RECOVERY-EVICTED") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not complete disconnected output"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    with_worker_connection(&mut state, hello(&binding, 4), |connection| {
        assert!(matches!(
            read_worker_message(connection).unwrap(),
            WorkerMessage::HelloAck { error: None, .. }
        ));
        write_worker_message(
            connection,
            &CoordinatorMessage::AdoptTerminal {
                request_id: RequestId::new(100),
                identity: identity.clone(),
                location: location.clone(),
            },
        )
        .unwrap();
        assert!(matches!(
            read_worker_message(connection).unwrap(),
            WorkerMessage::AdoptTerminalResult { error: None, .. }
        ));
        write_worker_message(
            connection,
            &CoordinatorMessage::AttachTerminal {
                request_id: RequestId::new(101),
                identity: identity.clone(),
                location: location.clone(),
                resume: AttachResume::AfterRevision(acknowledged),
            },
        )
        .unwrap();
        connection
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut recovered_revision = None;
        let mut checkpoint_received = false;
        while !runtime.visible_text().contains("RECOVERY-EVICTED") {
            let message = read_worker_message(connection).unwrap();
            checkpoint_received |= matches!(&message, WorkerMessage::OutputCheckpoint { .. });
            recovered_revision =
                apply_worker_message(&mut manager, &runtime, &host, message).or(recovered_revision);
            send_pending(connection, &commands);
        }
        assert!(
            checkpoint_received,
            "an evicted revision must recover with a canonical checkpoint"
        );
        assert!(recovered_revision.unwrap().get() > acknowledged.get());
        assert!(runtime.visible_text().contains("ALT-PERSISTENT"));
        let input = runtime.input_state().unwrap();
        assert!(input.alternate_screen);
        assert!(input.bracketed_paste);

        write_worker_message(
            connection,
            &CoordinatorMessage::Input {
                request_id: RequestId::new(102),
                identity: identity.clone(),
                location: location.clone(),
                op_seq: RuntimeOpSeq::new(1),
                data: b"\n".to_vec(),
            },
        )
        .unwrap();
        wait_for_worker_message(connection, |message| {
            matches!(message,
                WorkerMessage::RequestAck { request_id, error: None } if *request_id == RequestId::new(102)
            )
        });
        while !runtime.visible_text().contains("PRIMARY-AFTER-RECOVERY") {
            let message = read_worker_message(connection).unwrap();
            apply_worker_message(&mut manager, &runtime, &host, message);
            send_pending(connection, &commands);
        }
    });

    let original = state
        .runtime_for_record(state.runtime_record(&identity.runtime_id).unwrap())
        .unwrap();
    assert_eq!(runtime.visible_text(), original.visible_text());
    assert_eq!(runtime.input_state(), original.input_state());
    assert!(!runtime.input_state().unwrap().alternate_screen);
    assert!(runtime.visible_text().contains("PRIMARY-PERSISTENT"));
    state.shutdown_runtime_for_test(&identity.runtime_id);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_captures_kitty_pixels_before_first_attach() {
    let binding = test_binding("graphics-capture", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding).unwrap();
    let (identity, _) = state
        .create_terminal(
            location,
            crate::execution_host::protocol::TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    r"printf '\033_Ga=T,f=32,s=1,v=1,c=1,r=1,C=1,i=42,p=7;AQIDBA==\033\\GRAPHICS-READY'; sleep 30".into(),
                ],
                env: Vec::new(),
            }),
            Vec::new(),
            4096,
        )
        .unwrap();
    let runtime = state
        .runtime_for_record(state.runtime_record(&identity.runtime_id).unwrap())
        .unwrap();
    runtime.resize(24, 80, 8, 16);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !runtime.visible_text().contains("GRAPHICS-READY") {
        assert!(
            Instant::now() < deadline,
            "worker graphics output did not finish"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let placements = runtime.kitty_image_placements_with_data_filter(|_| true);
    let image = placements
        .iter()
        .find(|image| image.image_id == 42 && image.placement_id == 7)
        .expect("worker must capture image pixels before any coordinator attaches");
    assert_eq!(image.format, crate::ghostty::KittyImageFormat::Rgba);
    assert_eq!(image.data, [1, 2, 3, 4]);
    assert_eq!((image.image_width, image.image_height), (1, 1));
    state.shutdown_runtime_for_test(&identity.runtime_id);
}
