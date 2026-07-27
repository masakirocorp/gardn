use std::io::{BufRead as _, BufReader};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::auth::{AskpassServer, AuthenticationChallengeChannel, AuthenticationOwner};
use super::connection::{ConnectionLifecycle, ConnectionStatus};
use super::protocol::{
    read_worker_message, validate_first_worker_message, write_worker_message, AuthChallenge,
    CoordinatorInstallationId, CoordinatorMessage, HostBindingGeneration, RequestId,
    SessionNamespaceId, WorkerCapability, WorkerMessage, PROTOCOL_VERSION,
};
use crate::persist::ssh_profiles::SshConnectionProfile;

const WORKER_HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const WORKER_HELLO_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub(crate) enum WorkerConnectionEvent {
    Message(Box<WorkerMessage>),
    Stderr(String),
    TransportClosed(String),
}

#[derive(Clone)]
pub(crate) struct WorkerSender {
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    connected: Arc<AtomicBool>,
    next_request_id: Arc<AtomicU64>,
}

impl WorkerSender {
    pub(crate) fn next_request_id(&self) -> RequestId {
        allocate_request_id(&self.next_request_id)
    }

    pub(crate) fn send(&self, message: &CoordinatorMessage) -> std::io::Result<()> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "execution worker transport is disconnected",
            ));
        }
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| std::io::Error::other("execution worker writer lock is poisoned"))?;
        write_worker_message(&mut *stdin, message).map_err(|err| {
            self.connected.store(false, Ordering::Release);
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                format!("failed to write execution worker message: {err}"),
            )
        })
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

pub(crate) struct WorkerConnection {
    capabilities: Vec<WorkerCapability>,
    auth_challenge: Option<AuthChallenge>,
    sender: WorkerSender,
    events: mpsc::Receiver<WorkerConnectionEvent>,
    connected: Arc<AtomicBool>,
    _transport: crate::remote::ExecutionWorkerTransport,
    _askpass: AskpassServer,
    reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerSetupPolicy {
    Ensure,
    ProbeOnly,
}

fn coordinator_worker_capabilities() -> Vec<WorkerCapability> {
    vec![
        WorkerCapability::Terminal,
        WorkerCapability::PathCompletion,
        WorkerCapability::ProcessObservation,
        WorkerCapability::Git,
        WorkerCapability::Worktree,
        WorkerCapability::Command,
        WorkerCapability::Agent,
        WorkerCapability::Ports,
        WorkerCapability::FileStaging,
        WorkerCapability::AgentIntegrations,
    ]
}

impl WorkerConnection {
    fn connect(
        profile: &SshConnectionProfile,
        installation_id: CoordinatorInstallationId,
        session_namespace_id: SessionNamespaceId,
        authentication: Arc<AuthenticationChallengeChannel>,
        authentication_owner: AuthenticationOwner,
        next_request_id: Arc<AtomicU64>,
        setup_policy: WorkerSetupPolicy,
        cancel: Option<&crate::remote::ConnectCancel>,
    ) -> std::io::Result<Self> {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        let host_id = profile.execution_host_id();
        let askpass = AskpassServer::start(authentication, authentication_owner, host_id.clone())?;
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        let askpass_config = askpass.command_config()?;
        let mut transport = match crate::remote::spawn_execution_worker_cancellable(
            profile.target(),
            askpass_config.clone(),
            cancel,
        ) {
            Err(crate::remote::ExecutionWorkerTransportError::BootstrapRequired { .. })
                if setup_policy == WorkerSetupPolicy::Ensure =>
            {
                crate::remote::ensure_execution_worker(profile.target(), askpass_config.clone())?;
                crate::remote::spawn_execution_worker_cancellable(
                    profile.target(),
                    askpass_config,
                    cancel,
                )
                .map_err(std::io::Error::other)?
            }
            result => result.map_err(std::io::Error::other)?,
        };
        if let Some(cancel) = cancel {
            if let Err(err) = cancel.check() {
                drop(transport);
                return Err(err);
            }
        }
        let hello = CoordinatorMessage::Hello {
            version: PROTOCOL_VERSION,
            coordinator_installation_id: installation_id,
            session_namespace_id,
            execution_host_id: host_id.clone(),
            host_binding_generation: HostBindingGeneration::new(profile.host_binding_generation()),
            auth_proof: None,
            capabilities: coordinator_worker_capabilities(),
        };
        write_worker_message(transport.stdin_mut()?, &hello)
            .map_err(|err| std::io::Error::other(format!("worker hello failed: {err}")))?;

        let (first, mut stdout) = read_worker_hello(&mut transport, cancel)?;
        validate_first_worker_message(&first)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        let WorkerMessage::HelloAck {
            worker_instance_id: _,
            host_binding_generation,
            execution_host_id,
            capabilities,
            auth_challenge,
            error: _,
            version: _,
        } = first
        else {
            return Err(std::io::Error::other(
                "execution worker did not return HelloAck",
            ));
        };
        if host_binding_generation.get() != profile.host_binding_generation() {
            return Err(std::io::Error::other(format!(
                "execution worker binding generation mismatch: expected {}, received {}",
                profile.host_binding_generation(),
                host_binding_generation.get()
            )));
        }
        if execution_host_id != host_id {
            return Err(std::io::Error::other(format!(
                "execution worker host mismatch: expected {host_id}, received {execution_host_id}"
            )));
        }

        let stdin = Arc::new(Mutex::new(transport.take_stdin()?));
        let connected = Arc::new(AtomicBool::new(true));
        let sender = WorkerSender {
            stdin,
            connected: connected.clone(),
            next_request_id,
        };
        let (event_tx, events) = mpsc::channel();
        let reader_connected = connected.clone();
        let reader_tx = event_tx.clone();
        let reader = std::thread::spawn(move || loop {
            match read_worker_message::<_, WorkerMessage>(&mut stdout) {
                Ok(message) => {
                    if reader_tx
                        .send(WorkerConnectionEvent::Message(Box::new(message)))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    reader_connected.store(false, Ordering::Release);
                    let _ = reader_tx.send(WorkerConnectionEvent::TransportClosed(err.to_string()));
                    break;
                }
            }
        });

        let stderr = transport.take_stderr()?;
        let stderr_reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) if !line.is_empty() => {
                        if event_tx.send(WorkerConnectionEvent::Stderr(line)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        let _ = event_tx.send(WorkerConnectionEvent::TransportClosed(format!(
                            "execution worker stderr failed: {err}"
                        )));
                        break;
                    }
                }
            }
        });

        Ok(Self {
            capabilities,
            auth_challenge,
            sender,
            events,
            connected,
            _transport: transport,
            _askpass: askpass,
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
        })
    }

    pub(crate) fn capabilities(&self) -> &[WorkerCapability] {
        &self.capabilities
    }

    pub(crate) fn auth_challenge(&self) -> Option<&AuthChallenge> {
        self.auth_challenge.as_ref()
    }

    pub(crate) fn sender(&self) -> WorkerSender {
        self.sender.clone()
    }

    pub(crate) fn try_recv(&self) -> Option<WorkerConnectionEvent> {
        self.events.try_recv().ok()
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire) && self.sender.is_connected()
    }
}

impl Drop for WorkerConnection {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::Release);
        // Dropping the SSH transport closes only this bridge. The role-scoped
        // worker daemon and its terminal runtimes remain available for reconnect.
        self.reader.take();
        self.stderr_reader.take();
    }
}

fn read_worker_hello(
    transport: &mut crate::remote::ExecutionWorkerTransport,
    cancel: Option<&crate::remote::ConnectCancel>,
) -> std::io::Result<(WorkerMessage, std::process::ChildStdout)> {
    let mut stdout = transport.take_stdout()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let result = read_worker_message(&mut stdout);
        let _ = sender.send((result, stdout));
    });
    let deadline = Instant::now() + WORKER_HELLO_TIMEOUT;

    loop {
        if let Some(cancel) = cancel {
            if let Err(error) = cancel.check() {
                let _ = transport.kill();
                return Err(error);
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = transport.kill();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "execution worker timed out before sending HelloAck",
            ));
        }

        match receiver.recv_timeout(remaining.min(WORKER_HELLO_POLL_INTERVAL)) {
            Ok((result, stdout)) => {
                let _ = reader.join();
                let message = result.map_err(|error| {
                    std::io::Error::other(format!("worker hello ack failed: {error}"))
                })?;
                return Ok((message, stdout));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = reader.join();
                return Err(std::io::Error::other(
                    "execution worker HelloAck reader stopped",
                ));
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct WorkerInstaller {
    profile: SshConnectionProfile,
    installation_id: CoordinatorInstallationId,
    authentication: Arc<AuthenticationChallengeChannel>,
    owner: AuthenticationOwner,
}

impl WorkerInstaller {
    pub(crate) fn new(
        profile: SshConnectionProfile,
        installation_id: CoordinatorInstallationId,
        authentication: Arc<AuthenticationChallengeChannel>,
        owner: AuthenticationOwner,
    ) -> Self {
        Self {
            installation_id,
            profile,
            authentication,
            owner,
        }
    }

    pub(crate) fn preview(&self) -> Result<crate::remote::WorkerInstallPreview, String> {
        let askpass = AskpassServer::start(
            self.authentication.clone(),
            self.owner,
            self.profile.execution_host_id(),
        )
        .map_err(|error| error.to_string())?;
        let config = askpass
            .command_config()
            .map_err(|error| error.to_string())?;
        crate::remote::preview_execution_worker_install(self.profile.target(), config)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn install(
        &self,
        approved: &crate::remote::WorkerInstallPreview,
    ) -> Result<crate::remote::WorkerInstallReport, String> {
        let askpass = AskpassServer::start(
            self.authentication.clone(),
            self.owner,
            self.profile.execution_host_id(),
        )
        .map_err(|error| error.to_string())?;
        let config = askpass
            .command_config()
            .map_err(|error| error.to_string())?;
        crate::remote::install_execution_worker(self.profile.target(), config, approved)
            .map_err(|error| error.to_string())
    }
    pub(crate) fn inventory_owned_bindings(
        &self,
    ) -> Result<crate::execution_host::runtime_paths::BindingInventoryReport, String> {
        let askpass = AskpassServer::start(
            self.authentication.clone(),
            self.owner,
            self.profile.execution_host_id(),
        )
        .map_err(|error| error.to_string())?;
        let config = askpass
            .command_config()
            .map_err(|error| error.to_string())?;
        crate::remote::inventory_execution_worker_bindings(
            self.profile.target(),
            config,
            self.installation_id.as_str(),
            self.profile.execution_host_id().as_str(),
        )
        .map_err(|error| error.to_string())
    }

    pub(crate) fn retire_owned_bindings(
        &self,
    ) -> Result<crate::execution_host::runtime_paths::BindingRetirementReport, String> {
        let askpass = AskpassServer::start(
            self.authentication.clone(),
            self.owner,
            self.profile.execution_host_id(),
        )
        .map_err(|error| error.to_string())?;
        let config = askpass
            .command_config()
            .map_err(|error| error.to_string())?;
        crate::remote::retire_execution_worker_bindings(
            self.profile.target(),
            config,
            self.installation_id.as_str(),
            self.profile.execution_host_id().as_str(),
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub(crate) enum SshExecutionHostEvent {
    Status(ConnectionStatus),
    Worker(Box<WorkerMessage>),
    Diagnostic(String),
    Tested(Result<(), String>),
}

struct PendingConnectAttempt {
    receiver: mpsc::Receiver<std::io::Result<WorkerConnection>>,
    cancel: crate::remote::ConnectCancel,
    join: Option<JoinHandle<()>>,
}

pub(crate) struct SshExecutionHost {
    profile: SshConnectionProfile,
    installation_id: CoordinatorInstallationId,
    session_namespace_id: SessionNamespaceId,
    lifecycle: ConnectionLifecycle,
    authentication: Arc<AuthenticationChallengeChannel>,
    authentication_owner: AuthenticationOwner,
    next_request_id: Arc<AtomicU64>,
    connection: Option<WorkerConnection>,
    pending_connect: Option<PendingConnectAttempt>,
    testing: bool,
    status_override: Option<ConnectionStatus>,
    last_reported_status: ConnectionStatus,
}

impl SshExecutionHost {
    pub(crate) fn with_authentication_channel(
        profile: SshConnectionProfile,
        installation_id: CoordinatorInstallationId,
        session_namespace_id: SessionNamespaceId,
        authentication: Arc<AuthenticationChallengeChannel>,
    ) -> Self {
        Self {
            profile,
            installation_id,
            session_namespace_id,
            authentication,
            authentication_owner: AuthenticationOwner::SYSTEM,
            next_request_id: Arc::new(AtomicU64::new(request_id_seed())),
            lifecycle: ConnectionLifecycle::default(),
            connection: None,
            pending_connect: None,
            testing: false,
            status_override: None,
            last_reported_status: ConnectionStatus::Disconnected,
        }
    }

    pub(crate) fn profile(&self) -> &SshConnectionProfile {
        &self.profile
    }

    pub(crate) fn update_profile_metadata(&mut self, profile: SshConnectionProfile) {
        if self.profile.execution_host_id() == profile.execution_host_id()
            && self.profile.target() == profile.target()
        {
            self.profile = profile;
        }
    }

    pub(crate) fn status(&self) -> ConnectionStatus {
        if self
            .authentication
            .challenge_for(self.authentication_owner)
            .is_some_and(|challenge| {
                challenge.execution_host_id == self.profile.execution_host_id()
            })
        {
            return ConnectionStatus::AuthenticationRequired;
        }
        self.status_override
            .clone()
            .unwrap_or_else(|| self.lifecycle.status())
    }

    pub(crate) fn capabilities(&self) -> Option<&[WorkerCapability]> {
        self.connection.as_ref().map(WorkerConnection::capabilities)
    }

    pub(crate) fn sender(&self) -> Option<WorkerSender> {
        self.connection.as_ref().map(WorkerConnection::sender)
    }

    /// Allocate the next coordinator request id for this host binding.
    ///
    /// Uses the same persistent counter the live worker sender shares, so offline
    /// journaled ops and online requests stay in one monotonic namespace. Overflow
    /// saturates so live and test hosts share one policy.
    pub(crate) fn next_request_id(&self) -> RequestId {
        allocate_request_id(&self.next_request_id)
    }

    pub(crate) fn request_connect_for(&mut self, owner: AuthenticationOwner) {
        self.status_override = None;
        // Explicit Connect always transfers auth ownership so a later client can
        // answer prompts after the previous owner disappears mid-challenge or
        // while reconnect backoff is already armed.
        if self.authentication_owner != owner {
            self.authentication
                .cancel_host(self.authentication_owner, &self.profile.execution_host_id());
            self.authentication_owner = owner;
        }
        if self.lifecycle.request_connect() {
            self.start_connect_attempt();
            return;
        }
        // Already desired-connected (Connecting/Connected/Backoff): keep the
        // intent and, when waiting out backoff, retry immediately under the new
        // owner so interactive Connect is not stuck behind the prior schedule.
        if self.pending_connect.is_none()
            && self.connection.is_none()
            && matches!(
                self.lifecycle.state(),
                super::connection::ConnectionState::Backoff { .. }
            )
        {
            self.lifecycle.force_retry_now();
            self.start_connect_attempt();
        }
    }

    pub(crate) fn request_test_for(
        &mut self,
        owner: AuthenticationOwner,
    ) -> Option<Result<(), String>> {
        if let Some(connection) = &self.connection {
            return Some(if connection.auth_challenge().is_some() {
                Err("authentication is required".to_string())
            } else {
                Ok(())
            });
        }
        if self.pending_connect.is_some() {
            return Some(Err("connection attempt is already in progress".to_string()));
        }
        self.lifecycle.request_disconnect();
        self.lifecycle.finish_disconnect();
        self.testing = true;
        self.status_override = None;
        if self.lifecycle.request_connect() {
            self.authentication_owner = owner;
            self.start_connect_attempt();
        }
        None
    }

    pub(crate) fn request_disconnect_for(&mut self, owner: AuthenticationOwner) {
        self.authentication
            .cancel_host(owner, &self.profile.execution_host_id());
        self.request_disconnect();
    }

    pub(crate) fn request_disconnect(&mut self) {
        self.authentication
            .cancel_host(self.authentication_owner, &self.profile.execution_host_id());
        self.lifecycle.request_disconnect();
        self.cancel_pending_connect();
        self.connection = None;
        self.testing = false;
        self.status_override = None;
        self.lifecycle.finish_disconnect();
    }

    fn cancel_pending_connect(&mut self) {
        if let Some(mut pending) = self.pending_connect.take() {
            pending.cancel.cancel();
            // Drop the receiver so a late Ok(connection) cannot be installed.
            drop(pending.receiver);
            if let Some(join) = pending.join.take() {
                let _ = join.join();
            }
        }
    }

    pub(crate) fn poll(&mut self, now: Instant) -> Vec<SshExecutionHostEvent> {
        let mut events = Vec::new();
        if let Some(pending) = self.pending_connect.take() {
            match pending.receiver.try_recv() {
                Ok(Ok(connection)) => {
                    if pending.cancel.is_cancelled()
                        || !matches!(
                            self.lifecycle.state(),
                            super::connection::ConnectionState::Connecting { .. }
                        )
                    {
                        drop(connection);
                        if self.testing {
                            self.testing = false;
                            self.lifecycle.request_disconnect();
                            self.lifecycle.finish_disconnect();
                        }
                    } else {
                        let authentication_required = connection.auth_challenge().is_some();
                        if self.testing {
                            self.testing = false;
                            events.push(SshExecutionHostEvent::Tested(
                                (!authentication_required)
                                    .then_some(())
                                    .ok_or_else(|| "authentication is required".to_string()),
                            ));
                            self.lifecycle.request_disconnect();
                            self.lifecycle.finish_disconnect();
                        } else {
                            self.lifecycle.mark_connected();
                            self.connection = Some(connection);
                            self.status_override = authentication_required
                                .then_some(ConnectionStatus::AuthenticationRequired);
                        }
                    }
                }
                Ok(Err(err)) => {
                    if pending.cancel.is_cancelled() {
                        if self.testing {
                            self.testing = false;
                            self.lifecycle.request_disconnect();
                            self.lifecycle.finish_disconnect();
                        }
                    } else if self.testing {
                        self.testing = false;
                        events.push(SshExecutionHostEvent::Tested(Err(err.to_string())));
                        self.lifecycle.request_disconnect();
                        self.lifecycle.finish_disconnect();
                    } else {
                        self.lifecycle.mark_connection_failed(now, err.to_string());
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    self.pending_connect = Some(pending);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    if pending.cancel.is_cancelled() {
                        if self.testing {
                            self.testing = false;
                            self.lifecycle.request_disconnect();
                            self.lifecycle.finish_disconnect();
                        }
                    } else {
                        let error = "execution worker connection task stopped".to_string();
                        if self.testing {
                            self.testing = false;
                            events.push(SshExecutionHostEvent::Tested(Err(error)));
                            self.lifecycle.request_disconnect();
                            self.lifecycle.finish_disconnect();
                        } else {
                            self.lifecycle.mark_connection_failed(now, error);
                        }
                    }
                }
            }
        }

        if !self.testing && self.pending_connect.is_none() && self.lifecycle.begin_due_retry(now) {
            self.start_connect_attempt();
        }

        let mut transport_error = None;
        if let Some(connection) = &self.connection {
            while let Some(event) = connection.try_recv() {
                match event {
                    WorkerConnectionEvent::Message(message) => {
                        events.push(SshExecutionHostEvent::Worker(message));
                    }
                    WorkerConnectionEvent::Stderr(message) => {
                        let lowered = message.to_ascii_lowercase();
                        if lowered.contains("permission denied")
                            || lowered.contains("passphrase")
                            || lowered.contains("password")
                        {
                            self.status_override = Some(ConnectionStatus::AuthenticationRequired);
                            events.push(SshExecutionHostEvent::Diagnostic(
                                "SSH authentication is required".to_string(),
                            ));
                        } else {
                            events.push(SshExecutionHostEvent::Diagnostic(message));
                        }
                    }
                    WorkerConnectionEvent::TransportClosed(error) => {
                        transport_error = Some(error);
                        break;
                    }
                }
            }
            if !connection.is_connected() && transport_error.is_none() {
                transport_error = Some("execution worker transport closed".to_string());
            }
        }
        if let Some(error) = transport_error {
            self.connection = None;
            self.status_override = None;
            if self.testing {
                self.testing = false;
                events.push(SshExecutionHostEvent::Tested(Err(error)));
                self.lifecycle.request_disconnect();
                self.lifecycle.finish_disconnect();
            } else {
                self.lifecycle.mark_connection_failed(now, error);
            }
        }

        let status = self.status();
        if status != self.last_reported_status {
            self.last_reported_status = status.clone();
            events.push(SshExecutionHostEvent::Status(status));
        }
        events
    }

    fn start_connect_attempt(&mut self) {
        self.cancel_pending_connect();
        let profile = self.profile.clone();
        let installation_id = self.installation_id.clone();
        let session_namespace_id = self.session_namespace_id.clone();
        let authentication = self.authentication.clone();
        let authentication_owner = self.authentication_owner;
        let next_request_id = self.next_request_id.clone();
        let setup_policy = if self.testing {
            WorkerSetupPolicy::ProbeOnly
        } else {
            WorkerSetupPolicy::Ensure
        };
        let cancel = crate::remote::ConnectCancel::new();
        let cancel_for_task = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let join = std::thread::spawn(move || {
            let result = WorkerConnection::connect(
                &profile,
                installation_id,
                session_namespace_id,
                authentication,
                authentication_owner,
                next_request_id,
                setup_policy,
                Some(&cancel_for_task),
            );
            let result = match result {
                Ok(connection) if cancel_for_task.is_cancelled() => {
                    drop(connection);
                    Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "ssh connection attempt cancelled",
                    ))
                }
                other => other,
            };
            let _ = sender.send(result);
        });
        self.pending_connect = Some(PendingConnectAttempt {
            receiver,
            cancel,
            join: Some(join),
        });
    }
}

impl Drop for SshExecutionHost {
    fn drop(&mut self) {
        self.authentication
            .cancel_host(self.authentication_owner, &self.profile.execution_host_id());
        self.cancel_pending_connect();
        self.connection = None;
    }
}
fn request_id_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    (nanos ^ (u64::from(std::process::id()) << 32)).max(1)
}

/// Shared live/test request-id overflow policy: advance with saturating add.
fn allocate_request_id(counter: &AtomicU64) -> RequestId {
    loop {
        let current = counter.load(Ordering::Relaxed);
        let next = current.saturating_add(1);
        if counter
            .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return RequestId::new(current);
        }
    }
}

#[cfg(test)]
impl SshExecutionHost {
    pub(crate) fn authentication_owner_for_test(&self) -> AuthenticationOwner {
        self.authentication_owner
    }

    pub(crate) fn mark_backoff_for_test(&mut self, now: Instant, error: impl Into<String>) {
        // Ensure desired_connected is true so mark_connection_failed arms Backoff.
        let _ = self.lifecycle.request_connect();
        self.lifecycle.mark_connection_failed(now, error.into());
        self.cancel_pending_connect();
        self.connection = None;
    }

    pub(crate) fn arm_blocked_pending_connect_for_test(&mut self) -> crate::remote::ConnectCancel {
        self.cancel_pending_connect();
        let cancel = crate::remote::ConnectCancel::new();
        let cancel_for_task = cancel.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let join = std::thread::spawn(move || {
            while !cancel_for_task.is_cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let _ = sender.send(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "ssh connection attempt cancelled",
            )));
        });
        let _ = self.lifecycle.request_connect();
        self.pending_connect = Some(PendingConnectAttempt {
            receiver,
            cancel: cancel.clone(),
            join: Some(join),
        });
        cancel
    }

    pub(crate) fn has_pending_connect_for_test(&self) -> bool {
        self.pending_connect.is_some()
    }

    pub(crate) fn set_next_request_id_for_test(&self, value: u64) {
        self.next_request_id.store(value, Ordering::Relaxed);
    }
    fn worker_setup_policy_for_test(&self) -> WorkerSetupPolicy {
        if self.testing {
            WorkerSetupPolicy::ProbeOnly
        } else {
            WorkerSetupPolicy::Ensure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::auth::AuthenticationOwner;
    use crate::persist::ssh_profiles::SshConnectionProfile;
    use std::time::{Duration, Instant};

    fn test_host(channel: Arc<AuthenticationChallengeChannel>) -> SshExecutionHost {
        let profile = SshConnectionProfile::new(
            "workbox",
            "Work box",
            "workbox.example",
            Some(crate::execution_host::HostPath::new("/srv/work").unwrap()),
        )
        .unwrap();
        SshExecutionHost::with_authentication_channel(
            profile,
            CoordinatorInstallationId::new("install-a").unwrap(),
            SessionNamespaceId::new("session-a").unwrap(),
            channel,
        )
    }

    #[test]
    fn coordinator_requests_agent_integration_capability() {
        assert!(coordinator_worker_capabilities().contains(&WorkerCapability::AgentIntegrations));
    }

    #[test]
    fn explicit_connect_transfers_auth_owner_during_backoff_after_prior_owner_prompt() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let mut host = test_host(channel.clone());
        let owner_a = AuthenticationOwner::new(11);
        let owner_b = AuthenticationOwner::new(22);
        let host_id = host.profile().execution_host_id();

        host.request_connect_for(owner_a);
        assert_eq!(host.authentication_owner_for_test(), owner_a);

        // Simulate owner A sitting on an auth prompt, then disappearing while
        // reconnect backoff is armed.
        let channel_for_prompt = channel.clone();
        let host_id_for_prompt = host_id.clone();
        let waiter = std::thread::spawn(move || {
            channel_for_prompt.request(
                owner_a,
                1,
                host_id_for_prompt,
                "Enter passphrase for key".to_string(),
            )
        });
        let challenge = (0..200)
            .find_map(|_| {
                channel.challenge_for(owner_a).or_else(|| {
                    std::thread::sleep(Duration::from_millis(5));
                    None
                })
            })
            .expect("owner A challenge should surface");
        assert_eq!(challenge.execution_host_id, host_id);

        host.mark_backoff_for_test(Instant::now(), "permission denied");
        assert!(matches!(
            host.lifecycle.state(),
            super::super::connection::ConnectionState::Backoff { .. }
        ));

        // Owner B explicitly Connects: ownership transfers, prior challenge is
        // cancelled, and backoff retries immediately under B.
        host.request_connect_for(owner_b);
        assert_eq!(host.authentication_owner_for_test(), owner_b);
        assert_eq!(channel.challenge_for(owner_a), None);
        assert!(matches!(
            waiter.join().expect("prompt thread"),
            Err(crate::execution_host::auth::AuthenticationCancelled)
        ));
        assert!(matches!(
            host.lifecycle.state(),
            super::super::connection::ConnectionState::Connecting { .. }
        ));
        // Cancel the synthetic connect task so the test does not leak SSH work.
        host.cancel_pending_connect();
        host.lifecycle.finish_disconnect();
    }

    #[test]
    fn system_connect_still_starts_from_disconnected() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let mut host = test_host(channel);
        host.request_connect_for(AuthenticationOwner::SYSTEM);
        assert_eq!(
            host.authentication_owner_for_test(),
            AuthenticationOwner::SYSTEM
        );
        assert!(matches!(
            host.lifecycle.state(),
            super::super::connection::ConnectionState::Connecting { attempt: 1 }
        ));
        host.cancel_pending_connect();
        host.lifecycle.finish_disconnect();
    }

    #[test]
    fn saved_connection_ensures_worker_but_test_connection_only_probes() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let mut host = test_host(channel);
        assert_eq!(
            host.worker_setup_policy_for_test(),
            WorkerSetupPolicy::Ensure
        );

        host.testing = true;
        assert_eq!(
            host.worker_setup_policy_for_test(),
            WorkerSetupPolicy::ProbeOnly
        );
    }

    #[test]
    fn disconnect_during_blocked_connect_cancels_attempt_without_activation() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let mut host = test_host(channel.clone());
        let owner = AuthenticationOwner::new(42);
        let host_id = host.profile().execution_host_id();

        let cancel = host.arm_blocked_pending_connect_for_test();
        assert!(host.has_pending_connect_for_test());
        assert!(matches!(
            host.lifecycle.state(),
            super::super::connection::ConnectionState::Connecting { .. }
        ));

        // A challenge raised under the blocked attempt must not survive disconnect.
        let channel_for_prompt = channel.clone();
        let host_id_for_prompt = host_id.clone();
        let waiter = std::thread::spawn(move || {
            channel_for_prompt.request(
                owner,
                7,
                host_id_for_prompt,
                "Enter passphrase for key".to_string(),
            )
        });
        let challenge = (0..200)
            .find_map(|_| {
                channel.challenge_for(owner).or_else(|| {
                    std::thread::sleep(Duration::from_millis(5));
                    None
                })
            })
            .expect("challenge should surface before disconnect");
        assert_eq!(challenge.execution_host_id, host_id);

        host.request_disconnect_for(owner);

        assert!(cancel.is_cancelled());
        assert!(!host.has_pending_connect_for_test());
        assert!(host.sender().is_none());
        assert_eq!(channel.challenge_for(owner), None);
        assert!(matches!(
            waiter.join().expect("prompt thread"),
            Err(crate::execution_host::auth::AuthenticationCancelled)
        ));

        let events = host.poll(Instant::now());
        assert!(
            !events.iter().any(|event| matches!(
                event,
                SshExecutionHostEvent::Status(ConnectionStatus::Connected)
                    | SshExecutionHostEvent::Status(ConnectionStatus::AuthenticationRequired)
                    | SshExecutionHostEvent::Worker(_)
            )),
            "disconnect must not activate worker or connected status: {events:?}"
        );
        assert!(matches!(
            host.status(),
            ConnectionStatus::Disconnected | ConnectionStatus::Disconnecting
        ));
    }

    #[test]
    fn hello_wait_is_cancelled_without_waiting_for_worker_output() {
        let mut transport = crate::remote::ExecutionWorkerTransport::blocked_for_test().unwrap();
        let cancel = crate::remote::ConnectCancel::new();
        let cancel_for_task = cancel.clone();
        let cancel_task = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            cancel_for_task.cancel();
        });
        let started = Instant::now();

        let error = match read_worker_hello(&mut transport, Some(&cancel)) {
            Ok(_) => panic!("blocked worker must not complete the handshake"),
            Err(error) => error,
        };

        cancel_task.join().unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "cancellation took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn dropping_host_cancels_pending_connection_attempt() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let cancel = {
            let mut host = test_host(channel);
            let cancel = host.arm_blocked_pending_connect_for_test();
            assert!(!cancel.is_cancelled());
            cancel
        };

        assert!(cancel.is_cancelled());
    }
}
