//! Lifecycle bridge, daemon activation, and connection accept loop.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, Instant, SystemTime};

use crate::execution_host::lifecycle::{
    complete_lifecycle_frame, decide_activate, read_legacy_lifecycle_frame, read_lifecycle_frame,
    write_lifecycle_frame, ActivateReply, ActivateRequest, LifecycleDecision,
    LIFECYCLE_FRAME_PREFIX,
};
use crate::execution_host::protocol::{
    read_worker_message, validate_first_coordinator_message, write_worker_message,
    CoordinatorMessage, RequestId, WorkerErrorCode, WorkerInstanceId, WorkerMessage,
    WorkerRuntimeId, PROTOCOL_VERSION,
};

use super::binding::DaemonBinding;
use super::state::WorkerState;
use super::util::{
    framing_io, is_disconnect, worker_error, LIFECYCLE_ACTIVATE_TIMEOUT, LOCK_WAIT_POLL_INTERVAL,
    MAX_OCCUPIED_CLIENTS_PER_POLL, OCCUPIED_CLIENT_IO_TIMEOUT, READY_POLL_INTERVAL, READY_TIMEOUT,
    SESSION_POLL_INTERVAL, WORKER_APP_VERSION,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(unix)]
pub(super) fn run_bridge_stdio() -> io::Result<()> {
    run_bridge(io::stdin(), io::stdout())
}

#[cfg(unix)]
pub(super) fn run_bridge<R: Read + Send + 'static, W: Write>(
    mut reader: R,
    mut writer: W,
) -> io::Result<()> {
    let hello: CoordinatorMessage = read_worker_message(&mut reader).map_err(framing_io)?;
    let binding = DaemonBinding::from_hello(&hello)?;
    match activate_or_spawn_daemon(&binding)? {
        BridgeDaemonOutcome::Connected(mut daemon) => {
            write_worker_message(&mut daemon, &hello).map_err(framing_io)?;
            let ack: WorkerMessage = read_worker_message(&mut daemon).map_err(framing_io)?;
            write_worker_message(&mut writer, &ack).map_err(framing_io)?;
            relay_bridge(reader, writer, daemon)
        }
        BridgeDaemonOutcome::RejectedHelloAck(ack) => {
            write_worker_message(&mut writer, &ack).map_err(framing_io)?;
            Ok(())
        }
    }
}

#[cfg(unix)]
pub(super) enum BridgeDaemonOutcome {
    Connected(UnixStream),
    RejectedHelloAck(Box<WorkerMessage>),
}

#[cfg(unix)]
pub(super) fn relay_bridge<R: Read + Send + 'static, W: Write>(
    mut reader: R,
    mut writer: W,
    mut daemon: UnixStream,
) -> io::Result<()> {
    let mut daemon_writer = daemon.try_clone()?;
    let _inbound = std::thread::spawn(move || {
        let result: io::Result<()> = loop {
            let message: CoordinatorMessage = match read_worker_message(&mut reader) {
                Ok(message) => message,
                Err(error) => break Err(framing_io(error)),
            };
            if let Err(error) =
                write_worker_message(&mut daemon_writer, &message).map_err(framing_io)
            {
                break Err(error);
            }
        };
        let _ = daemon_writer.shutdown(std::net::Shutdown::Write);
        result
    });
    loop {
        let message: WorkerMessage = match read_worker_message(&mut daemon) {
            Ok(message) => message,
            Err(crate::protocol::FramingError::UnexpectedEof) => return Ok(()),
            Err(crate::protocol::FramingError::Io(error)) if is_disconnect(&error) => {
                return Ok(());
            }
            Err(error) => return Err(framing_io(error)),
        };
        write_worker_message(&mut writer, &message).map_err(framing_io)?;
    }
}

#[cfg(unix)]
pub(super) fn activate_or_spawn_daemon(binding: &DaemonBinding) -> io::Result<BridgeDaemonOutcome> {
    let role_paths = binding.role_paths();
    role_paths.prepare()?;
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        match try_activate_existing_daemon(binding) {
            Ok(BridgeActivateResult::UseExisting(stream)) => {
                return Ok(BridgeDaemonOutcome::Connected(stream));
            }
            Ok(BridgeActivateResult::ShuttingDownIdle) => {
                wait_for_lock_release(&role_paths.lock_path(), deadline)?;
                // Contenders converge through flock; only the lock owner binds.
            }
            Ok(BridgeActivateResult::Rejected(ack)) => {
                return Ok(BridgeDaemonOutcome::RejectedHelloAck(ack));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) => {}
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => {
                std::thread::sleep(READY_POLL_INTERVAL);
                continue;
            }
        }

        match spawn_daemon_contender(binding) {
            Ok(BridgeActivateResult::UseExisting(stream)) => {
                return Ok(BridgeDaemonOutcome::Connected(stream));
            }
            Ok(BridgeActivateResult::ShuttingDownIdle) => {
                wait_for_lock_release(&role_paths.lock_path(), deadline)?;
            }
            Ok(BridgeActivateResult::Rejected(ack)) => {
                return Ok(BridgeDaemonOutcome::RejectedHelloAck(ack));
            }
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists && Instant::now() < deadline =>
            {
                std::thread::sleep(READY_POLL_INTERVAL);
            }
            Err(error) if Instant::now() >= deadline => {
                return Err(io::Error::new(
                    error.kind(),
                    format!("execution worker daemon did not become ready: {error}"),
                ));
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
#[derive(Debug)]
pub(super) enum BridgeActivateResult {
    UseExisting(UnixStream),
    ShuttingDownIdle,
    Rejected(Box<WorkerMessage>),
}

#[cfg(unix)]
pub(super) fn try_activate_existing_daemon(
    binding: &DaemonBinding,
) -> io::Result<BridgeActivateResult> {
    let mut stream = UnixStream::connect(&binding.socket_path)?;
    stream.set_read_timeout(Some(LIFECYCLE_ACTIVATE_TIMEOUT))?;
    stream.set_write_timeout(Some(LIFECYCLE_ACTIVATE_TIMEOUT))?;

    let request = ActivateRequest::new(
        binding.binding_digest(),
        super::artifact_digest()?,
        PROTOCOL_VERSION,
        WORKER_APP_VERSION,
    )
    .map_err(io::Error::from)?;
    let request_bytes = request.encode().map_err(io::Error::from)?;
    if let Err(error) = write_lifecycle_frame(&mut stream, &request_bytes) {
        return activation_transport_error(binding, error);
    }

    let reply_bytes = match read_lifecycle_frame(&mut stream) {
        Ok(bytes) => bytes,
        Err(error) => {
            return try_activate_legacy_v1(binding)
                .or_else(|_| activation_transport_error(binding, error));
        }
    };
    let reply = match ActivateReply::decode(&reply_bytes) {
        Ok(reply) => reply,
        Err(error) => {
            return Ok(BridgeActivateResult::Rejected(Box::new(
                lifecycle_rejection_hello_ack(
                    binding,
                    WorkerErrorCode::InvalidHandshake,
                    format!("daemon lifecycle reply malformed: {error}"),
                    None,
                ),
            )));
        }
    };

    // Clear timeouts before upgrading the stream to the normal worker protocol.
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);

    match reply.decision {
        LifecycleDecision::UseExisting | LifecycleDecision::UseExistingDeferred => {
            Ok(BridgeActivateResult::UseExisting(stream))
        }
        LifecycleDecision::ShuttingDownIdle => Ok(BridgeActivateResult::ShuttingDownIdle),
        LifecycleDecision::BlockedBusy => Ok(BridgeActivateResult::Rejected(Box::new(
            lifecycle_rejection_hello_ack(
                binding,
                WorkerErrorCode::Busy,
                format!(
                    "execution worker update blocked: incumbent owns {} live runtime(s) (running {}/protocol {})",
                    reply.owned_runtime_count,
                    reply.running_app_version,
                    reply.running_worker_protocol
                ),
                parse_worker_instance_id(&reply.worker_instance_id),
            ),
        ))),
        LifecycleDecision::Unsupported => Ok(BridgeActivateResult::Rejected(Box::new(
            lifecycle_rejection_hello_ack(
                binding,
                WorkerErrorCode::ProtocolMismatch,
                format!(
                    "execution worker update unsupported by incumbent (running {}/protocol {})",
                    reply.running_app_version, reply.running_worker_protocol
                ),
                parse_worker_instance_id(&reply.worker_instance_id),
            ),
        ))),
    }
}

#[cfg(unix)]
fn retirement_hello(binding: &DaemonBinding) -> CoordinatorMessage {
    CoordinatorMessage::Hello {
        version: PROTOCOL_VERSION,
        coordinator_installation_id: binding.installation_id.clone(),
        session_namespace_id: binding.session_namespace_id.clone(),
        execution_host_id: binding.execution_host_id.clone(),
        host_binding_generation: binding.host_binding_generation,
        auth_proof: None,
        capabilities: Vec::new(),
    }
}

#[cfg(unix)]
fn flush_daemon_events_before_retirement(binding: &DaemonBinding) -> io::Result<()> {
    let mut stream = UnixStream::connect(&binding.socket_path)?;
    stream.set_read_timeout(Some(LIFECYCLE_ACTIVATE_TIMEOUT))?;
    stream.set_write_timeout(Some(LIFECYCLE_ACTIVATE_TIMEOUT))?;
    write_worker_message(&mut stream, &retirement_hello(binding)).map_err(framing_io)?;
    match read_worker_message::<_, WorkerMessage>(&mut stream).map_err(framing_io)? {
        WorkerMessage::HelloAck { error: None, .. } => {}
        WorkerMessage::HelloAck {
            error: Some(error), ..
        } => return Err(io::Error::new(io::ErrorKind::ResourceBusy, error.message)),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "live execution worker returned an invalid retirement handshake",
            ));
        }
    }

    // The daemon flushes queued runtime exits after each session poll. A short
    // quiet read lets it reconcile children that exited while disconnected.
    stream.set_read_timeout(Some(SESSION_POLL_INTERVAL * 5))?;
    loop {
        match read_worker_message::<_, WorkerMessage>(&mut stream) {
            Ok(_) => {}
            Err(crate::protocol::FramingError::UnexpectedEof) => return Ok(()),
            Err(crate::protocol::FramingError::Io(error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::ConnectionReset
                        | io::ErrorKind::BrokenPipe
                ) =>
            {
                return Ok(());
            }
            Err(error) => return Err(framing_io(error)),
        }
    }
}

#[cfg(unix)]
pub(super) fn shutdown_owned_binding(
    entry: &crate::execution_host::runtime_paths::OwnedBindingInventoryEntry,
) -> io::Result<()> {
    let binding = DaemonBinding::from_inventory_entry(entry)?;
    match flush_daemon_events_before_retirement(&binding) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) => {}
        Err(error) => return Err(error),
    }
    let deadline = Instant::now() + READY_TIMEOUT;
    match try_activate_existing_daemon(&binding) {
        Ok(BridgeActivateResult::ShuttingDownIdle) => {
            wait_for_lock_release(&binding.role_paths().lock_path(), deadline)
        }
        Ok(BridgeActivateResult::UseExisting(mut stream)) => {
            write_worker_message(&mut stream, &retirement_hello(&binding)).map_err(framing_io)?;
            let hello_ack: WorkerMessage = read_worker_message(&mut stream).map_err(framing_io)?;
            if !matches!(hello_ack, WorkerMessage::HelloAck { error: None, .. }) {
                return Err(io::Error::new(
                    io::ErrorKind::ResourceBusy,
                    "live execution worker rejected retirement",
                ));
            }
            let request_id = RequestId::new(1);
            write_worker_message(&mut stream, &CoordinatorMessage::Shutdown { request_id })
                .map_err(framing_io)?;
            loop {
                match read_worker_message::<_, WorkerMessage>(&mut stream).map_err(framing_io)? {
                    WorkerMessage::RequestAck {
                        request_id: ack_id,
                        error: None,
                    } if ack_id == request_id => {
                        drop(stream);
                        break wait_for_lock_release(&binding.role_paths().lock_path(), deadline);
                    }
                    WorkerMessage::RequestAck {
                        request_id: ack_id,
                        error: Some(error),
                    } if ack_id == request_id => {
                        break Err(io::Error::new(io::ErrorKind::ResourceBusy, error.message));
                    }
                    // Runtime events can race the shutdown request. The daemon
                    // emits them before the acknowledgement so ownership is current.
                    _ => {}
                }
            }
        }
        Ok(BridgeActivateResult::Rejected(ack)) => match *ack {
            WorkerMessage::HelloAck {
                error: Some(error), ..
            } => Err(io::Error::new(io::ErrorKind::ResourceBusy, error.message)),
            _ => Err(io::Error::new(
                io::ErrorKind::ResourceBusy,
                "live execution worker rejected retirement",
            )),
        },
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn try_activate_legacy_v1(binding: &DaemonBinding) -> io::Result<BridgeActivateResult> {
    let mut stream = UnixStream::connect(&binding.socket_path)?;
    stream.set_read_timeout(Some(LIFECYCLE_ACTIVATE_TIMEOUT))?;
    stream.set_write_timeout(Some(LIFECYCLE_ACTIVATE_TIMEOUT))?;
    // Force an incompatibility decision. V1 cannot compare artifact checksums,
    // so it may only drain an idle incumbent or report that live work blocks it.
    let forced_protocol = if PROTOCOL_VERSION == u32::MAX {
        0
    } else {
        PROTOCOL_VERSION + 1
    };
    let request = ActivateRequest::new(
        binding.binding_digest(),
        super::artifact_digest()?,
        forced_protocol,
        WORKER_APP_VERSION,
    )
    .map_err(io::Error::from)?;
    write_lifecycle_frame(
        &mut stream,
        &request.encode_legacy_v1().map_err(io::Error::from)?,
    )?;
    let reply = ActivateReply::decode_legacy_v1(&read_legacy_lifecycle_frame(&mut stream)?)
        .map_err(io::Error::from)?;
    match reply.decision {
        LifecycleDecision::ShuttingDownIdle => Ok(BridgeActivateResult::ShuttingDownIdle),
        LifecycleDecision::BlockedBusy => Ok(BridgeActivateResult::Rejected(Box::new(
            lifecycle_rejection_hello_ack(
                binding,
                WorkerErrorCode::Busy,
                format!(
                    "execution worker update blocked: legacy incumbent owns {} live runtime(s) (running {}/protocol {})",
                    reply.owned_runtime_count,
                    reply.running_app_version,
                    reply.running_worker_protocol
                ),
                parse_worker_instance_id(&reply.worker_instance_id),
            ),
        ))),
        LifecycleDecision::UseExisting
        | LifecycleDecision::UseExistingDeferred
        | LifecycleDecision::Unsupported => Ok(BridgeActivateResult::Rejected(Box::new(
            lifecycle_rejection_hello_ack(
                binding,
                WorkerErrorCode::ProtocolMismatch,
                "legacy execution worker could not prove exact artifact compatibility",
                parse_worker_instance_id(&reply.worker_instance_id),
            ),
        ))),
    }
}

#[cfg(unix)]
pub(super) fn activation_transport_error(
    binding: &DaemonBinding,
    error: io::Error,
) -> io::Result<BridgeActivateResult> {
    if matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
    ) {
        return Err(error);
    }
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        return Ok(BridgeActivateResult::Rejected(Box::new(
            lifecycle_rejection_hello_ack(
                binding,
                WorkerErrorCode::TimedOut,
                "execution worker lifecycle activation timed out; incumbent left untouched",
                None,
            ),
        )));
    }
    // Malformed/unsupported pre-lifecycle daemons reject the oversized lifecycle
    // frame without a trustworthy runtime count. Never unlink or replace them.
    Ok(BridgeActivateResult::Rejected(Box::new(
        lifecycle_rejection_hello_ack(
            binding,
            WorkerErrorCode::ProtocolMismatch,
            format!(
            "execution worker lifecycle activation refused by incumbent: {error}; left untouched"
        ),
            None,
        ),
    )))
}

#[cfg(unix)]
pub(super) fn lifecycle_rejection_hello_ack(
    binding: &DaemonBinding,
    code: WorkerErrorCode,
    message: impl Into<String>,
    worker_instance_id: Option<WorkerInstanceId>,
) -> WorkerMessage {
    WorkerMessage::HelloAck {
        version: PROTOCOL_VERSION,
        worker_instance_id: worker_instance_id
            .unwrap_or_else(|| binding.worker_instance_id.clone()),
        host_binding_generation: binding.host_binding_generation,
        execution_host_id: binding.execution_host_id.clone(),
        capabilities: Vec::new(),
        auth_challenge: None,
        error: Some(worker_error(code, message)),
    }
}

#[cfg(unix)]
pub(super) fn parse_worker_instance_id(value: &str) -> Option<WorkerInstanceId> {
    WorkerInstanceId::new(value.to_string()).ok()
}

#[cfg(unix)]
pub(super) fn spawn_daemon_contender(binding: &DaemonBinding) -> io::Result<BridgeActivateResult> {
    // Bridges never unlink sockets. Only a lock-owning daemon may bind/remove.
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .args(binding.daemon_args())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    crate::platform::detach_server_daemon_command(&mut command);
    command.spawn()?;

    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        match try_activate_existing_daemon(binding) {
            Ok(result) => return Ok(result),
            Err(err) if Instant::now() >= deadline => {
                return Err(io::Error::new(
                    err.kind(),
                    format!("execution worker daemon did not become ready: {err}"),
                ));
            }
            Err(_) => std::thread::sleep(READY_POLL_INTERVAL),
        }
    }
}

#[cfg(unix)]
pub(super) fn wait_for_lock_release(path: &Path, deadline: Instant) -> io::Result<()> {
    loop {
        match try_acquire_daemon_lock(path) {
            Ok(file) => {
                // Immediately release; we only needed proof the inode is free.
                drop(file);
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "timed out waiting for execution worker lock release",
                    ));
                }
                std::thread::sleep(LOCK_WAIT_POLL_INTERVAL);
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
pub(super) fn run_daemon(binding: DaemonBinding) -> io::Result<()> {
    let role_paths = binding.role_paths();
    role_paths.prepare()?;
    // Hold the flock for the entire daemon lifetime, including cleanup.
    let _binding_lock = acquire_daemon_lock(&role_paths.lock_path())?;
    // Only the lock owner may remove a stale socket and bind a replacement.
    if binding.socket_path.exists() {
        match UnixStream::connect(&binding.socket_path) {
            Ok(_) => {
                // Another live owner already serves this binding; exit quietly.
                return Ok(());
            }
            Err(err)
                if matches!(
                    err.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                role_paths.remove_socket_if_present()?;
            }
            Err(err) => return Err(err),
        }
    }
    let listener = UnixListener::bind(&binding.socket_path)?;
    std::fs::set_permissions(&binding.socket_path, std::fs::Permissions::from_mode(0o600))?;
    let mut state = WorkerState::new(binding.clone())?;
    let (lease_heartbeat_stop, heartbeat_stop) = std_mpsc::channel();
    let lease_heartbeat = std::thread::spawn(move || {
        while let Err(std_mpsc::RecvTimeoutError::Timeout) =
            heartbeat_stop.recv_timeout(Duration::from_secs(60))
        {
            let _ = super::touch_artifact_lease();
        }
    });
    listener.set_nonblocking(true)?;
    let result = loop {
        let (stream, _) = match listener.accept() {
            Ok(accepted) => accepted,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(SESSION_POLL_INTERVAL);
                continue;
            }
            Err(error) => break Err(error),
        };
        let _ = stream.set_nonblocking(false);
        match serve_connection_maybe_listening(stream, &mut state, Some(&listener)) {
            Ok(ConnectionOutcome::Continue) => {}
            Ok(ConnectionOutcome::Shutdown) => break Ok(()),
            Err(err) if is_disconnect(&err) => {}
            Err(err) => eprintln!("execution worker connection failed: {err}"),
        }
    };
    let _ = lease_heartbeat_stop.send(());
    let _ = lease_heartbeat.join();
    drop(listener);
    drop(state);
    // Socket removed while lock still held; lock inode + runtime dir persist.
    let cleanup = role_paths.cleanup();
    match result {
        Err(error) => Err(error),
        Ok(()) => cleanup,
    }
}

#[cfg(unix)]
pub(super) fn acquire_daemon_lock(path: &Path) -> io::Result<std::fs::File> {
    try_acquire_daemon_lock(path)
}

#[cfg(unix)]
pub(super) fn try_acquire_daemon_lock(path: &Path) -> io::Result<std::fs::File> {
    use std::os::fd::AsRawFd as _;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(file)
    } else {
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "execution worker binding is already owned: {}",
                io::Error::last_os_error()
            ),
        ))
    }
}

#[cfg(unix)]
pub(super) enum ConnectionOutcome {
    Continue,
    Shutdown,
}

#[cfg(all(test, unix))]
pub(super) fn serve_connection(
    stream: UnixStream,
    state: &mut WorkerState,
) -> io::Result<ConnectionOutcome> {
    serve_connection_maybe_listening(stream, state, None)
}

#[cfg(unix)]
fn serve_connection_maybe_listening(
    mut stream: UnixStream,
    state: &mut WorkerState,
    listener: Option<&UnixListener>,
) -> io::Result<ConnectionOutcome> {
    let mut len_prefix = [0u8; 4];
    stream.read_exact(&mut len_prefix)?;
    if len_prefix == LIFECYCLE_FRAME_PREFIX {
        match handle_lifecycle_activate(&mut stream, len_prefix, state)? {
            LifecycleServeResult::UpgradeToHello => {}
            LifecycleServeResult::Done(outcome) => return Ok(outcome),
        }
    } else {
        let hello: CoordinatorMessage =
            read_worker_message(&mut (&len_prefix[..]).chain(&mut stream)).map_err(framing_io)?;
        return serve_normal_hello(stream, state, hello, listener);
    }

    let hello: CoordinatorMessage = read_worker_message(&mut stream).map_err(framing_io)?;
    serve_normal_hello(stream, state, hello, listener)
}

#[cfg(unix)]
fn drain_occupied_clients(
    listener: &UnixListener,
    state: &mut WorkerState,
) -> io::Result<Option<ConnectionOutcome>> {
    for _ in 0..MAX_OCCUPIED_CLIENTS_PER_POLL {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                match serve_occupied_connection(stream, state) {
                    Ok(ConnectionOutcome::Shutdown) => {
                        return Ok(Some(ConnectionOutcome::Shutdown))
                    }
                    Ok(ConnectionOutcome::Continue) | Err(_) => {}
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(None),
            Err(_) => return Ok(None),
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn serve_occupied_connection(
    mut stream: UnixStream,
    state: &mut WorkerState,
) -> io::Result<ConnectionOutcome> {
    stream.set_read_timeout(Some(OCCUPIED_CLIENT_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(OCCUPIED_CLIENT_IO_TIMEOUT))?;
    let mut len_prefix = [0u8; 4];
    stream.read_exact(&mut len_prefix)?;
    if len_prefix == LIFECYCLE_FRAME_PREFIX {
        match handle_lifecycle_activate(&mut stream, len_prefix, state)? {
            LifecycleServeResult::UpgradeToHello => {
                let hello: CoordinatorMessage =
                    read_worker_message(&mut stream).map_err(framing_io)?;
                write_occupied_hello_ack(&mut stream, state, &hello)
            }
            LifecycleServeResult::Done(outcome) => Ok(outcome),
        }
    } else {
        let hello: CoordinatorMessage =
            read_worker_message(&mut (&len_prefix[..]).chain(&mut stream)).map_err(framing_io)?;
        write_occupied_hello_ack(&mut stream, state, &hello)
    }
}

#[cfg(unix)]
fn hello_ack_message(
    state: &WorkerState,
    hello: &CoordinatorMessage,
    error: Option<crate::execution_host::protocol::WorkerError>,
) -> WorkerMessage {
    let requested_capabilities = match hello {
        CoordinatorMessage::Hello { capabilities, .. } => capabilities.clone(),
        _ => Vec::new(),
    };
    let capabilities = state
        .capabilities()
        .into_iter()
        .filter(|capability| requested_capabilities.contains(capability))
        .collect::<Vec<_>>();
    WorkerMessage::HelloAck {
        version: PROTOCOL_VERSION,
        worker_instance_id: state.binding().worker_instance_id.clone(),
        host_binding_generation: state.binding().host_binding_generation,
        execution_host_id: state.binding().execution_host_id.clone(),
        capabilities,
        auth_challenge: None,
        error,
    }
}

#[cfg(unix)]
fn write_occupied_hello_ack(
    stream: &mut UnixStream,
    state: &WorkerState,
    hello: &CoordinatorMessage,
) -> io::Result<ConnectionOutcome> {
    let ack = hello_ack_message(
        state,
        hello,
        Some(worker_error(
            WorkerErrorCode::Busy,
            "execution worker already has an active coordinator session",
        )),
    );
    write_worker_message(stream, &ack).map_err(framing_io)?;
    Ok(ConnectionOutcome::Continue)
}

#[cfg(unix)]
pub(super) enum LifecycleServeResult {
    UpgradeToHello,
    Done(ConnectionOutcome),
}

#[cfg(unix)]
pub(super) fn handle_lifecycle_activate(
    stream: &mut UnixStream,
    prefix: [u8; 4],
    state: &mut WorkerState,
) -> io::Result<LifecycleServeResult> {
    let frame = complete_lifecycle_frame(prefix, stream)?;
    let request = match ActivateRequest::decode(&frame) {
        Ok(request) => request,
        Err(_error) => {
            let input = state.lifecycle_snapshot();
            let reply = ActivateReply::from_decision_input(
                &ActivateRequest {
                    binding_digest: input.binding_digest,
                    artifact_digest: input.running_artifact_digest,
                    desired_worker_protocol: PROTOCOL_VERSION,
                    desired_app_version: WORKER_APP_VERSION.to_string(),
                },
                &input,
                LifecycleDecision::Unsupported,
            )
            .map_err(io::Error::from)?;
            write_lifecycle_frame(stream, &reply.encode().map_err(io::Error::from)?)?;
            return Ok(LifecycleServeResult::Done(ConnectionOutcome::Continue));
        }
    };

    // Atomic snapshot under the single-threaded accept/serve loop.
    let input = state.lifecycle_snapshot();
    let decision = if state.is_draining() {
        // Already cooperatively draining; never upgrade a new Hello on this path.
        if input.owned_runtime_count > 0 || input.busy {
            LifecycleDecision::BlockedBusy
        } else {
            LifecycleDecision::ShuttingDownIdle
        }
    } else {
        let decision = decide_activate(&request, &input);
        if decision == LifecycleDecision::ShuttingDownIdle {
            // Transition before replying so later creates cannot sneak in.
            state.begin_draining();
        }
        decision
    };
    let reply =
        ActivateReply::from_decision_input(&request, &input, decision).map_err(io::Error::from)?;
    write_lifecycle_frame(stream, &reply.encode().map_err(io::Error::from)?)?;

    match decision {
        LifecycleDecision::UseExisting | LifecycleDecision::UseExistingDeferred => {
            Ok(LifecycleServeResult::UpgradeToHello)
        }
        LifecycleDecision::ShuttingDownIdle => {
            Ok(LifecycleServeResult::Done(ConnectionOutcome::Shutdown))
        }
        LifecycleDecision::BlockedBusy | LifecycleDecision::Unsupported => {
            Ok(LifecycleServeResult::Done(ConnectionOutcome::Continue))
        }
    }
}

#[cfg(unix)]
pub(super) fn serve_normal_hello(
    mut stream: UnixStream,
    state: &mut WorkerState,
    hello: CoordinatorMessage,
    listener: Option<&UnixListener>,
) -> io::Result<ConnectionOutcome> {
    let handshake_error = if state.is_draining() {
        Some(worker_error(
            WorkerErrorCode::Busy,
            "execution worker is draining for replacement",
        ))
    } else {
        validate_first_coordinator_message(&hello)
            .err()
            .map(|err| worker_error(WorkerErrorCode::InvalidHandshake, err.to_string()))
            .or_else(|| {
                (!state.binding().matches_hello(&hello)).then(|| {
                    worker_error(
                        WorkerErrorCode::BindingMismatch,
                        "worker binding does not match coordinator Hello",
                    )
                })
            })
    };
    let ack = hello_ack_message(state, &hello, handshake_error.clone());
    write_worker_message(&mut stream, &ack).map_err(framing_io)?;
    if handshake_error.is_some() {
        return Ok(ConnectionOutcome::Continue);
    }

    let mut reader = stream.try_clone()?;
    let (request_tx, request_rx) = std_mpsc::channel();
    std::thread::spawn(move || loop {
        match read_worker_message::<_, CoordinatorMessage>(&mut reader) {
            Ok(message) => {
                if request_tx.send(Ok(message)).is_err() {
                    break;
                }
            }
            Err(err) => {
                let _ = request_tx.send(Err(framing_io(err)));
                break;
            }
        }
    });
    let job_tx = state.host_job_sender();
    let mut sent_revisions = HashMap::<WorkerRuntimeId, u64>::new();
    let session_result = (|| -> io::Result<ConnectionOutcome> {
        loop {
            match request_rx.recv_timeout(SESSION_POLL_INTERVAL) {
                Ok(Ok(message)) => {
                    if super::dispatch::handle_request(
                        message,
                        state,
                        &mut stream,
                        &mut sent_revisions,
                        &job_tx,
                    )? {
                        break Ok(ConnectionOutcome::Shutdown);
                    }
                }
                Ok(Err(err)) if is_disconnect(&err) => break Ok(ConnectionOutcome::Continue),
                Ok(Err(err)) => return Err(err),
                Err(std_mpsc::RecvTimeoutError::Timeout) => {
                    if let Some(listener) = listener {
                        if let Some(outcome) = drain_occupied_clients(listener, state)? {
                            break Ok(outcome);
                        }
                    }
                }
                Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                    break Ok(ConnectionOutcome::Continue);
                }
            }
            super::host_job::expire_host_jobs(state, &mut stream)?;
            super::host_job::flush_host_job_results(state, &mut stream)?;
            super::terminal::flush_output(state, &mut stream, &mut sent_revisions)?;
            super::terminal::flush_state_events(state, &mut stream)?;
            state.staging_mut().cleanup_expired(SystemTime::now());
        }
    })();
    state.cancel_host_jobs_for_disconnect();
    state.reap_completed_host_jobs();
    session_result
}
