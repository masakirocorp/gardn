use std::io::{self, Write};
use std::sync::mpsc as std_mpsc;

use std::os::unix::net::UnixStream;

use crate::execution_host::protocol::{
    read_worker_message, write_worker_message, CommandSpec, RequestId, TerminalSize, WorkerMessage,
};
use crate::execution_host::{HostPath, ResourceLocation};

use super::super::lifecycle::relay_bridge;
use super::super::state::{validated_scrollback_limit, WorkerState};
use super::support::test_binding;

struct FlushRecordingWriter {
    pending: Vec<u8>,
    flushed: std_mpsc::Sender<Vec<u8>>,
}

impl Write for FlushRecordingWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushed
            .send(std::mem::take(&mut self.pending))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "flush receiver closed"))
    }
}

#[test]
fn bridge_flushes_each_worker_frame_before_daemon_eof() {
    let (bridge_input, coordinator_input) = UnixStream::pair().unwrap();
    let (bridge_daemon, mut daemon) = UnixStream::pair().unwrap();
    let (flushed_tx, flushed_rx) = std_mpsc::channel();
    let writer = FlushRecordingWriter {
        pending: Vec::new(),
        flushed: flushed_tx,
    };
    let relay = std::thread::spawn(move || relay_bridge(bridge_input, writer, bridge_daemon));

    write_worker_message(
        &mut daemon,
        &WorkerMessage::RequestAck {
            request_id: RequestId::new(7),
            error: None,
        },
    )
    .unwrap();
    daemon.shutdown(std::net::Shutdown::Write).unwrap();
    relay.join().unwrap().unwrap();
    drop(coordinator_input);

    let bytes = flushed_rx.recv().unwrap();
    assert!(flushed_rx.try_recv().is_err());
    let mut visible = std::io::Cursor::new(bytes);
    let message: WorkerMessage = read_worker_message(&mut visible).unwrap();
    assert!(matches!(
        message,
        WorkerMessage::RequestAck {
            request_id,
            error: None,
        } if request_id == RequestId::new(7)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_scrollback_above_frame_cap_is_accepted() {
    // Protocol carries u64; worker must not clamp to MAX_FRAME_SIZE.
    let above_frame = (16 * 1024 * 1024) + (1024 * 1024);
    let limit = validated_scrollback_limit(above_frame as u64).unwrap();
    assert_eq!(limit, above_frame);

    let binding = test_binding("scrollback-limit", 1);
    let location = ResourceLocation::new(
        binding.execution_host_id.clone(),
        HostPath::new(std::env::temp_dir()).unwrap(),
    );
    let mut state = WorkerState::new(binding).unwrap();
    let (identity, _) = state
        .create_terminal(
            location,
            TerminalSize { cols: 80, rows: 24 },
            Some(CommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 30".to_string()],
                env: Vec::new(),
            }),
            Vec::new(),
            limit,
        )
        .unwrap();
    state.shutdown_runtime_for_test(&identity.runtime_id);
}
