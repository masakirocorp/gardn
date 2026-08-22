//! Protocol compatibility and framing tests.

use serde::Serialize;

use super::*;
use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};
use crate::protocol::{self, FramingError};

fn location() -> ResourceLocation {
    ResourceLocation::new(
        ExecutionHostId::new("ssh:workbox").unwrap(),
        HostPath::new("/srv/api").unwrap(),
    )
}

fn runtime_identity() -> RuntimeIdentity {
    RuntimeIdentity::new(
        HostBindingGeneration::new(3),
        WorkerInstanceId::new("worker-1").unwrap(),
        WorkerRuntimeId::new("rt-9").unwrap(),
        RuntimeIncarnation::new(7),
    )
}

fn assert_bincode_bytes<T>(value: &T, expected: &[u8])
where
    T: Serialize,
{
    let encoded = bincode::serde::encode_to_vec(value, bincode::config::standard()).unwrap();
    assert_eq!(
        encoded.as_slice(),
        expected,
        "encoded bytes diverged:\n left: {encoded:02x?}\nright: {expected:02x?}"
    );
}

#[test]
fn coordinator_hello_bincode_bytes_are_stable() {
    let msg = CoordinatorMessage::Hello {
        version: PROTOCOL_VERSION,
        coordinator_installation_id: CoordinatorInstallationId::new("install-a").unwrap(),
        session_namespace_id: SessionNamespaceId::new("01234567-89ab-cdef-0123-456789abcdef")
            .unwrap(),
        execution_host_id: ExecutionHostId::new("ssh:a").unwrap(),
        host_binding_generation: HostBindingGeneration::new(1),
        auth_proof: None,
        capabilities: vec![WorkerCapability::Terminal, WorkerCapability::Git],
    };

    // Enum tag 0, version 2, string lens, generation 1, None proof, two capabilities.
    assert_bincode_bytes(
        &msg,
        &[
            0x00, // CoordinatorMessage::Hello
            0x02, // version
            0x09, b'i', b'n', b's', b't', b'a', b'l', b'l', b'-', b'a', // installation id
            0x24, // session namespace len 36
            b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'-', b'8', b'9', b'a', b'b', b'-',
            b'c', b'd', b'e', b'f', b'-', b'0', b'1', b'2', b'3', b'-', b'4', b'5', b'6', b'7',
            b'8', b'9', b'a', b'b', b'c', b'd', b'e', b'f', // uuid
            0x05, b's', b's', b'h', b':', b'a', // execution host id
            0x01, // host_binding_generation
            0x00, // auth_proof: None
            0x02, // capabilities len
            0x00, // Terminal
            0x03, // Git
        ],
    );
}

#[test]
fn worker_hello_ack_framing_matches_golden_fixture() {
    let msg = WorkerMessage::HelloAck {
        version: PROTOCOL_VERSION,
        worker_instance_id: WorkerInstanceId::new("worker-1").unwrap(),
        host_binding_generation: HostBindingGeneration::new(1),
        execution_host_id: ExecutionHostId::new("ssh:a").unwrap(),
        capabilities: vec![WorkerCapability::Terminal],
        auth_challenge: None,
        error: None,
    };
    let expected_payload = [
        0x00, // WorkerMessage::HelloAck
        0x02, // version
        0x08, b'w', b'o', b'r', b'k', b'e', b'r', b'-', b'1', // instance
        0x01, // host_binding_generation
        0x05, b's', b's', b'h', b':', b'a', // execution host id
        0x01, // capabilities len
        0x00, // Terminal
        0x00, // auth_challenge: None
        0x00, // error: None
    ];
    let mut expected = Vec::with_capacity(4 + expected_payload.len());
    expected.extend_from_slice(&(expected_payload.len() as u32).to_le_bytes());
    expected.extend_from_slice(&expected_payload);

    let mut buf = Vec::new();
    write_worker_message(&mut buf, &msg).unwrap();
    assert_eq!(buf.as_slice(), expected.as_slice());

    let decoded: WorkerMessage = read_worker_message(&mut expected.as_slice()).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn output_delta_bincode_bytes_are_stable() {
    let msg = WorkerMessage::OutputDelta {
        identity: runtime_identity(),
        location: location(),
        base_revision: OutputRevision::new(10),
        revision: OutputRevision::new(11),
        data: vec![b'o', b'k'],
    };
    assert_bincode_bytes(
        &msg,
        &[
            0x05, // WorkerMessage::OutputDelta (variant index 5)
            0x03, // host_binding_generation
            0x08, b'w', b'o', b'r', b'k', b'e', b'r', b'-', b'1', //
            0x04, b'r', b't', b'-', b'9', //
            0x07, // incarnation
            0x0b, b's', b's', b'h', b':', b'w', b'o', b'r', b'k', b'b', b'o', b'x', // host
            0x08, b'/', b's', b'r', b'v', b'/', b'a', b'p', b'i', // path
            0x0a, // base_revision 10
            0x0b, // revision 11
            0x02, b'o', b'k', // data
        ],
    );
}

#[test]
fn max_frame_rejection_uses_public_framing_helpers() {
    let msg = WorkerMessage::OutputCheckpoint {
        identity: runtime_identity(),
        location: location(),
        revision: OutputRevision::new(1),
        data: vec![0x61; 64],
    };
    let mut buf = Vec::new();
    write_worker_message(&mut buf, &msg).unwrap();
    let payload_len = u32::from_le_bytes(buf[..4].try_into().unwrap()) as usize;
    assert!(payload_len > 0);
    assert!(payload_len <= MAX_FRAME_SIZE);

    match protocol::read_message::<_, WorkerMessage>(&mut buf.as_slice(), payload_len - 1) {
        Err(FramingError::Oversized { claimed, max }) => {
            assert_eq!(claimed, payload_len);
            assert_eq!(max, payload_len - 1);
        }
        other => panic!("expected oversized framing error, got {other:?}"),
    }

    // Declared length above the worker cap is rejected without allocating the payload.
    let mut oversized = Vec::new();
    let too_big = (MAX_FRAME_SIZE as u32).saturating_add(1).to_le_bytes();
    oversized.extend_from_slice(&too_big);
    match read_worker_message::<_, WorkerMessage>(&mut oversized.as_slice()) {
        Err(FramingError::Oversized { claimed, max }) => {
            assert_eq!(claimed, MAX_FRAME_SIZE + 1);
            assert_eq!(max, MAX_FRAME_SIZE);
        }
        other => panic!("expected worker max-frame rejection, got {other:?}"),
    }
}

#[test]
fn write_worker_message_rejects_oversized_payload_before_io() {
    let message = WorkerMessage::CommandResult {
        request_id: RequestId::new(9),
        location: location(),
        exit: Some(RuntimeExitStatus::Code(0)),
        stdout: vec![b'x'; MAX_FRAME_SIZE],
        stderr: Vec::new(),
        error: None,
    };
    let mut sink = Vec::new();
    match write_worker_message(&mut sink, &message) {
        Err(FramingError::Oversized { claimed, max }) => {
            assert!(claimed > MAX_FRAME_SIZE);
            assert_eq!(max, MAX_FRAME_SIZE);
        }
        other => panic!("expected write-side oversized rejection, got {other:?}"),
    }
    assert!(
        sink.is_empty(),
        "oversized write must not emit partial frames"
    );
}

#[test]
fn runtime_identity_distinguishes_incarnation_and_instance() {
    let a = runtime_identity();
    let mut b = a.clone();
    b.incarnation = RuntimeIncarnation::new(8);
    let mut c = a.clone();
    c.worker_instance_id = WorkerInstanceId::new("worker-2").unwrap();
    let mut d = a.clone();
    d.host_binding_generation = HostBindingGeneration::new(4);
    let mut e = a.clone();
    e.runtime_id = WorkerRuntimeId::new("rt-10").unwrap();

    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_ne!(a, d);
    assert_ne!(a, e);
    assert_eq!(a, runtime_identity());
}

#[test]
fn handshake_validation_accepts_hello_pair_and_rejects_other_first_messages() {
    let hello = CoordinatorMessage::Hello {
        version: PROTOCOL_VERSION,
        coordinator_installation_id: CoordinatorInstallationId::new("install-a").unwrap(),
        session_namespace_id: SessionNamespaceId::new("ns-1").unwrap(),
        execution_host_id: ExecutionHostId::new("ssh:a").unwrap(),
        host_binding_generation: HostBindingGeneration::new(1),
        auth_proof: None,
        capabilities: Vec::new(),
    };
    assert_eq!(validate_first_coordinator_message(&hello), Ok(()));

    let not_hello = CoordinatorMessage::Shutdown {
        request_id: RequestId::new(1),
    };
    assert_eq!(
        validate_first_coordinator_message(&not_hello),
        Err(HandshakeError::ExpectedHello)
    );

    let bad_version = CoordinatorMessage::Hello {
        version: 99,
        coordinator_installation_id: CoordinatorInstallationId::new("install-a").unwrap(),
        session_namespace_id: SessionNamespaceId::new("ns-1").unwrap(),
        execution_host_id: ExecutionHostId::new("ssh:a").unwrap(),
        host_binding_generation: HostBindingGeneration::new(1),
        auth_proof: None,
        capabilities: Vec::new(),
    };
    assert_eq!(
        validate_first_coordinator_message(&bad_version),
        Err(HandshakeError::ProtocolMismatch {
            expected: PROTOCOL_VERSION,
            received: 99
        })
    );

    let ack = WorkerMessage::HelloAck {
        version: PROTOCOL_VERSION,
        worker_instance_id: WorkerInstanceId::new("worker-1").unwrap(),
        host_binding_generation: HostBindingGeneration::new(1),
        execution_host_id: ExecutionHostId::new("ssh:a").unwrap(),
        capabilities: vec![WorkerCapability::Terminal],
        auth_challenge: None,
        error: None,
    };
    assert_eq!(validate_first_worker_message(&ack), Ok(()));

    let rejected = WorkerMessage::HelloAck {
        version: PROTOCOL_VERSION,
        worker_instance_id: WorkerInstanceId::new("worker-1").unwrap(),
        host_binding_generation: HostBindingGeneration::new(1),
        execution_host_id: ExecutionHostId::new("ssh:a").unwrap(),
        capabilities: Vec::new(),
        auth_challenge: None,
        error: Some(WorkerError::new(
            WorkerErrorCode::ProtocolMismatch,
            "wrong version",
        )),
    };
    assert!(matches!(
        validate_first_worker_message(&rejected),
        Err(HandshakeError::Rejected(_))
    ));

    let not_ack = WorkerMessage::RequestAck {
        request_id: RequestId::new(1),
        error: None,
    };
    assert_eq!(
        validate_first_worker_message(&not_ack),
        Err(HandshakeError::ExpectedHelloAck)
    );
}

#[test]
fn string_ids_reject_ambiguous_values() {
    assert_eq!(
        CoordinatorInstallationId::new(""),
        Err(ProtocolIdError::Empty)
    );
    assert_eq!(
        SessionNamespaceId::new("bad id"),
        Err(ProtocolIdError::InvalidCharacter)
    );
    assert!(WorkerInstanceId::new("worker-1").is_ok());
    assert!(AuthProof::new("abc+/=_-").is_ok());
}

#[test]
fn resource_location_is_required_on_terminal_ops() {
    let create = CoordinatorMessage::CreateTerminal {
        request_id: RequestId::new(1),
        location: location(),
        size: TerminalSize { cols: 80, rows: 24 },
        command: None,
        env: Vec::new(),
        scrollback_limit_bytes: 2 * 1024 * 1024,
    };
    match create {
        CoordinatorMessage::CreateTerminal { location, .. } => {
            assert_eq!(location.execution_host_id.as_str(), "ssh:workbox");
            assert_eq!(location.path.as_path(), std::path::Path::new("/srv/api"));
        }
        other => panic!("expected create terminal, got {other:?}"),
    }
}

#[test]
fn process_observation_result_roundtrips_idle_shell_and_session_members() {
    let msg = WorkerMessage::ProcessObservationResult {
        request_id: RequestId::new(4),
        identity: runtime_identity(),
        location: location(),
        process: Some(ProcessObservation {
            pid: 42,
            ppid: None,
            command: Some("zsh".into()),
            cwd: Some(HostPath::new("/srv/api").unwrap()),
            foreground_process_group_id: None,
            foreground_processes: Vec::new(),
            session_processes: vec![ObservedProcess {
                pid: 99,
                name: "node".into(),
                argv0: Some("node".into()),
                argv: None,
                cmdline: Some("node server.js".into()),
                cwd: Some(HostPath::new("/srv/api").unwrap()),
            }],
        }),
        error: None,
    };
    let encoded = bincode::serde::encode_to_vec(&msg, bincode::config::standard()).unwrap();
    let (decoded, _): (WorkerMessage, usize) =
        bincode::serde::decode_from_slice(&encoded, bincode::config::standard()).unwrap();
    assert_eq!(decoded, msg);
    match decoded {
        WorkerMessage::ProcessObservationResult {
            process: Some(process),
            ..
        } => {
            assert!(process.foreground_processes.is_empty());
            assert_eq!(process.session_processes.len(), 1);
            assert_eq!(process.session_processes[0].pid, 99);
        }
        other => panic!("unexpected decode: {other:?}"),
    }
}
