use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::os::unix::net::UnixStream;

use crate::execution_host::protocol::{
    read_worker_message, write_worker_message, CoordinatorInstallationId, CoordinatorMessage,
    HostBindingGeneration, SessionNamespaceId, WorkerCapability, WorkerInstanceId, WorkerMessage,
    PROTOCOL_VERSION,
};
use crate::execution_host::runtime_paths::WorkerRolePaths;
use crate::execution_host::ExecutionHostId;

use super::super::binding::DaemonBinding;
use super::super::lifecycle::{serve_connection, ConnectionOutcome};
use super::super::state::WorkerState;

pub(super) fn test_binding(name: &str, generation: u64) -> DaemonBinding {
    let installation_id =
        CoordinatorInstallationId::new(format!("test-installation-{name}")).unwrap();
    let session_namespace_id = SessionNamespaceId::new(format!("test-session-{name}")).unwrap();
    let execution_host_id = ExecutionHostId::new(format!("ssh:test-{name}")).unwrap();
    let host_binding_generation = HostBindingGeneration::new(generation);
    let paths = WorkerRolePaths::for_binding(
        &installation_id,
        &session_namespace_id,
        &execution_host_id,
        host_binding_generation,
    );
    paths.prepare().unwrap();
    DaemonBinding {
        installation_id,
        session_namespace_id,
        execution_host_id,
        host_binding_generation,
        worker_instance_id: WorkerInstanceId::new(format!("worker-{name}")).unwrap(),
        socket_path: paths.socket_path(),
    }
}

pub(super) fn hello(binding: &DaemonBinding, generation: u64) -> CoordinatorMessage {
    CoordinatorMessage::Hello {
        version: PROTOCOL_VERSION,
        coordinator_installation_id: binding.installation_id.clone(),
        session_namespace_id: binding.session_namespace_id.clone(),
        execution_host_id: binding.execution_host_id.clone(),
        host_binding_generation: HostBindingGeneration::new(generation),
        auth_proof: None,
        capabilities: vec![WorkerCapability::Terminal],
    }
}

pub(super) fn with_worker_connection<T>(
    state: &mut WorkerState,
    hello: CoordinatorMessage,
    interact: impl FnOnce(&mut UnixStream) -> T,
) -> (T, ConnectionOutcome) {
    let (worker_stream, mut coordinator_stream) = UnixStream::pair().unwrap();
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async { serve_connection(worker_stream, state) })
        });
        write_worker_message(&mut coordinator_stream, &hello).unwrap();
        let result = interact(&mut coordinator_stream);
        drop(coordinator_stream);
        let outcome = worker.join().unwrap().unwrap();
        (result, outcome)
    })
}

pub(super) fn tempfile_dir(prefix: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

pub(super) fn wait_for_worker_message(
    connection: &mut UnixStream,
    mut matched: impl FnMut(&WorkerMessage) -> bool,
) -> WorkerMessage {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for worker message");
        }
        connection.set_read_timeout(Some(remaining)).unwrap();
        match read_worker_message::<_, WorkerMessage>(connection) {
            Ok(message) if matched(&message) => return message,
            Ok(_) => continue,
            Err(crate::protocol::FramingError::Io(error))
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(error) => panic!("failed waiting for worker message: {error}"),
        }
    }
}
