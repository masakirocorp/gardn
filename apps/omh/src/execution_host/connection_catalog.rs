use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use super::auth::{
    AuthenticationChallenge, AuthenticationChallengeChannel, AuthenticationOwner,
    AuthenticationResponse, AuthenticationResponseError,
};
use super::operations::HostOperationError;
use super::protocol::{
    CoordinatorInstallationId, CoordinatorMessage, RequestId, SessionNamespaceId, WorkerCapability,
};
use super::remote::{SshExecutionHost, SshExecutionHostEvent, WorkerInstaller};
use super::{ConnectionStatus, ExecutionHostId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostConnectionAction {
    Test,
    Connect,
    Disconnect,
}

use crate::persist::ssh_profiles::SshConnectionProfile;

type ConnectionRequestOutcome = Option<(ExecutionHostId, Result<(), String>)>;

pub(crate) struct ConnectionCatalogPoll {
    pub(crate) statuses: HashMap<ExecutionHostId, ConnectionStatus>,
    pub(crate) reconnected_hosts: Vec<ExecutionHostId>,
    pub(crate) worker_messages: Vec<(ExecutionHostId, super::protocol::WorkerMessage)>,
    pub(crate) events: Vec<ConnectionCatalogEvent>,
}

/// Catalog of configured SSH execution hosts and their live transports.
///
/// Owns profile sync, authentication channel sharing, capability checks, and
/// worker message dispatch. Terminal/observation/staging state lives elsewhere.
///
/// All request-id allocation and coordinator message delivery for production SSH
/// hosts and in-process test hosts goes through [`Self::allocate_and_send`] /
/// [`Self::allocate_request_id`] / [`Self::send_message`] so both paths share one
/// overflow policy and one transport seam.
pub(crate) struct ConnectionCatalog {
    installation_id: CoordinatorInstallationId,
    session_namespace_id: SessionNamespaceId,
    authentication: Arc<AuthenticationChallengeChannel>,
    ssh_hosts: HashMap<ExecutionHostId, SshExecutionHost>,
    #[cfg(test)]
    test_worker_messages:
        HashMap<ExecutionHostId, std::sync::Arc<std::sync::Mutex<Vec<CoordinatorMessage>>>>,
    #[cfg(test)]
    next_test_request_id: u64,
}

impl ConnectionCatalog {
    pub(crate) fn new(
        installation_id: CoordinatorInstallationId,
        session_namespace_id: SessionNamespaceId,
    ) -> Self {
        Self {
            installation_id,
            session_namespace_id,
            authentication: Arc::new(AuthenticationChallengeChannel::default()),
            ssh_hosts: HashMap::new(),
            #[cfg(test)]
            test_worker_messages: HashMap::new(),
            #[cfg(test)]
            next_test_request_id: 1,
        }
    }

    /// Converge runtime connections on the persisted profile catalog.
    ///
    /// Returns host ids removed by the sync so callers can stale dependent state.
    pub(crate) fn sync_profiles(
        &mut self,
        profiles: &[SshConnectionProfile],
    ) -> Vec<ExecutionHostId> {
        let expected = profiles
            .iter()
            .map(SshConnectionProfile::execution_host_id)
            .collect::<HashSet<_>>();
        let removed_host_ids = self
            .ssh_hosts
            .keys()
            .filter(|host_id| !expected.contains(*host_id))
            .cloned()
            .collect::<Vec<_>>();
        self.ssh_hosts
            .retain(|host_id, _| expected.contains(host_id));

        for profile in profiles {
            let host_id = profile.execution_host_id();
            if let Some(host) = self.ssh_hosts.get_mut(&host_id) {
                host.update_profile_metadata(profile.clone());
            } else {
                self.ssh_hosts.insert(
                    host_id,
                    SshExecutionHost::with_authentication_channel(
                        profile.clone(),
                        self.installation_id.clone(),
                        self.session_namespace_id.clone(),
                        self.authentication.clone(),
                    ),
                );
            }
        }
        removed_host_ids
    }

    pub(crate) fn request_for(
        &mut self,
        owner: AuthenticationOwner,
        profile_id: &str,
        action: HostConnectionAction,
    ) -> Result<ConnectionRequestOutcome, String> {
        let (host_id, host) = self
            .ssh_hosts
            .iter_mut()
            .find(|(_, host)| host.profile().id() == profile_id)
            .ok_or_else(|| format!("unknown SSH connection profile {profile_id}"))?;
        match action {
            HostConnectionAction::Test => Ok(host
                .request_test_for(owner)
                .map(|result| (host_id.clone(), result))),
            HostConnectionAction::Connect => {
                host.request_connect_for(owner);
                Ok(None)
            }
            HostConnectionAction::Disconnect => {
                host.request_disconnect_for(owner);
                Ok(None)
            }
        }
    }

    pub(crate) fn authentication_challenge(
        &self,
        owner: AuthenticationOwner,
    ) -> Option<AuthenticationChallenge> {
        self.authentication.challenge_for(owner)
    }

    pub(crate) fn respond_to_authentication(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
        response: AuthenticationResponse,
    ) -> Result<(), AuthenticationResponseError> {
        self.authentication.respond(owner, challenge_id, response)
    }

    pub(crate) fn cancel_authentication(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
    ) -> Result<(), AuthenticationResponseError> {
        self.authentication.cancel(owner, challenge_id)
    }

    pub(crate) fn cancel_authentication_owner(&self, owner: AuthenticationOwner) {
        self.authentication.cancel_owner(owner);
    }

    #[cfg(test)]
    pub(crate) fn authentication_channel_for_test(&self) -> Arc<AuthenticationChallengeChannel> {
        self.authentication.clone()
    }

    pub(crate) fn worker_installer_for(
        &self,
        owner: AuthenticationOwner,
        profile_id: &str,
    ) -> Result<WorkerInstaller, String> {
        let profile = self
            .ssh_hosts
            .values()
            .find(|host| host.profile().id() == profile_id)
            .map(|host| host.profile().clone())
            .ok_or_else(|| format!("unknown SSH connection profile {profile_id}"))?;
        Ok(WorkerInstaller::new(
            profile,
            self.authentication.clone(),
            owner,
        ))
    }

    #[cfg(test)]
    pub(crate) fn connect_test_host(
        &mut self,
        host_id: ExecutionHostId,
    ) -> std::sync::Arc<std::sync::Mutex<Vec<CoordinatorMessage>>> {
        let messages = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        self.test_worker_messages.insert(host_id, messages.clone());
        messages
    }

    /// Whether this host can journal runtime ops offline or online.
    ///
    /// Test hosts, live senders, and profile-bound SSH hosts all qualify so
    /// disconnect→type→poll does not drop input.
    pub(crate) fn can_journal_runtime_ops(&self, host_id: &ExecutionHostId) -> bool {
        #[cfg(test)]
        if self.test_worker_messages.contains_key(host_id) {
            return true;
        }
        self.ssh_hosts.contains_key(host_id)
    }

    /// True when a live worker sender or in-process test transport is available.
    pub(crate) fn has_transport(&self, host_id: &ExecutionHostId) -> bool {
        #[cfg(test)]
        if self.test_worker_messages.contains_key(host_id) {
            return true;
        }
        self.ssh_hosts
            .get(host_id)
            .and_then(SshExecutionHost::sender)
            .is_some()
    }

    /// Validate that a connected worker advertises `capability` before starting
    /// host-routed work that depends on it. Test hosts always pass.
    pub(crate) fn ensure_host_capability(
        &self,
        host_id: &ExecutionHostId,
        capability: WorkerCapability,
    ) -> Result<(), HostOperationError> {
        if host_id.is_local() {
            return Ok(());
        }
        #[cfg(test)]
        if self.test_worker_messages.contains_key(host_id) {
            return Ok(());
        }
        let host = self
            .ssh_hosts
            .get(host_id)
            .ok_or_else(|| HostOperationError::Unavailable {
                host_id: host_id.clone(),
            })?;
        if host.status() != ConnectionStatus::Connected {
            return Err(HostOperationError::Unavailable {
                host_id: host_id.clone(),
            });
        }
        let capabilities = host
            .capabilities()
            .ok_or_else(|| HostOperationError::Unavailable {
                host_id: host_id.clone(),
            })?;
        if !capabilities.contains(&capability) {
            return Err(HostOperationError::Unsupported {
                host_id: host_id.clone(),
                capability,
            });
        }
        Ok(())
    }

    /// Allocate the next request id for `host_id` without sending.
    ///
    /// Live SSH hosts and test hosts share one saturating overflow policy.
    /// Offline SSH hosts still allocate from the host binding's persistent
    /// counter so reconnect cannot collide with journaled ids.
    pub(crate) fn allocate_request_id(&mut self, host_id: &ExecutionHostId) -> Option<RequestId> {
        #[cfg(test)]
        if self.test_worker_messages.contains_key(host_id) {
            return Some(self.alloc_test_request_id());
        }
        let host = self.ssh_hosts.get(host_id)?;
        if let Some(sender) = host.sender() {
            return Some(sender.next_request_id());
        }
        Some(host.next_request_id())
    }

    /// Deliver an already-built coordinator message on the host transport.
    ///
    /// Used for reconnect replay and journaled ops that retain a prior request
    /// id. Missing live transport is a no-op success so offline journaling can
    /// retain work for later replay; unknown hosts fail.
    pub(crate) fn send_message(
        &self,
        host_id: &ExecutionHostId,
        message: &CoordinatorMessage,
    ) -> Result<(), HostOperationError> {
        #[cfg(test)]
        if let Some(messages) = self.test_worker_messages.get(host_id) {
            messages
                .lock()
                .map_err(|_| {
                    HostOperationError::Failed("test worker message lock is poisoned".to_string())
                })?
                .push(message.clone());
            return Ok(());
        }
        let Some(host) = self.ssh_hosts.get(host_id) else {
            return Err(HostOperationError::Unavailable {
                host_id: host_id.clone(),
            });
        };
        let Some(sender) = host.sender() else {
            // Offline host binding: caller retains the message for reconnect replay.
            return Ok(());
        };
        sender
            .send(message)
            .map_err(|error| HostOperationError::Failed(error.to_string()))
    }

    /// Allocate a request id, build a coordinator message, and send it.
    ///
    /// Single transport seam for live SSH and test hosts. When `require_transport`
    /// is false, a known offline host binding still allocates an id and reports
    /// success without delivery so journaled ops can wait for reconnect.
    pub(crate) fn allocate_and_send(
        &mut self,
        host_id: &ExecutionHostId,
        require_transport: bool,
        build: impl FnOnce(RequestId) -> CoordinatorMessage,
    ) -> Result<RequestId, HostOperationError> {
        #[cfg(test)]
        if self.test_worker_messages.contains_key(host_id) {
            let request_id = self.alloc_test_request_id();
            self.send_message(host_id, &build(request_id))?;
            return Ok(request_id);
        }
        let host = self
            .ssh_hosts
            .get(host_id)
            .ok_or_else(|| HostOperationError::Unavailable {
                host_id: host_id.clone(),
            })?;
        if let Some(sender) = host.sender() {
            let request_id = sender.next_request_id();
            sender
                .send(&build(request_id))
                .map_err(|error| HostOperationError::Failed(error.to_string()))?;
            return Ok(request_id);
        }
        if require_transport {
            return Err(HostOperationError::Unavailable {
                host_id: host_id.clone(),
            });
        }
        // Offline host binding: allocate from the persistent counter and leave
        // delivery to reconnect replay.
        let request_id = host.next_request_id();
        let _ = build(request_id);
        Ok(request_id)
    }

    /// Capability-checked allocate+send used by observation/staging/command paths.
    pub(crate) fn send_host_operation(
        &mut self,
        host_id: ExecutionHostId,
        capability: WorkerCapability,
        build: impl FnOnce(RequestId) -> CoordinatorMessage,
    ) -> Result<RequestId, HostOperationError> {
        if host_id.is_local() {
            return Err(HostOperationError::InvalidLocation(
                "remote operation routing requires a non-local resource location".to_string(),
            ));
        }
        self.ensure_host_capability(&host_id, capability)?;
        self.allocate_and_send(&host_id, true, build)
    }

    pub(crate) fn has_active_connections(&self) -> bool {
        self.ssh_hosts
            .values()
            .any(|host| host.status() != ConnectionStatus::Disconnected)
    }

    #[cfg(test)]
    fn alloc_test_request_id(&mut self) -> RequestId {
        let request_id = RequestId::new(self.next_test_request_id);
        self.next_test_request_id = self.next_test_request_id.saturating_add(1);
        request_id
    }

    #[cfg(test)]
    pub(crate) fn host_count(&self) -> usize {
        self.ssh_hosts.len()
    }

    #[cfg(test)]
    pub(crate) fn contains_host(&self, host_id: &ExecutionHostId) -> bool {
        self.ssh_hosts.contains_key(host_id)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.ssh_hosts.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn set_next_test_request_id_for_test(&mut self, value: u64) {
        self.next_test_request_id = value;
    }

    #[cfg(test)]
    pub(crate) fn set_host_next_request_id_for_test(
        &self,
        host_id: &ExecutionHostId,
        value: u64,
    ) -> bool {
        let Some(host) = self.ssh_hosts.get(host_id) else {
            return false;
        };
        host.set_next_request_id_for_test(value);
        true
    }

    /// Poll every host transport. Returns status map, reconnected hosts, worker
    /// messages, and diagnostic/test events as host-local outcomes.
    pub(crate) fn poll(&mut self, now: Instant) -> ConnectionCatalogPoll {
        let mut statuses = HashMap::with_capacity(self.ssh_hosts.len());
        let mut reconnected_hosts = Vec::new();
        let mut worker_messages = Vec::new();
        let mut events = Vec::new();
        for (host_id, host) in &mut self.ssh_hosts {
            for event in host.poll(now) {
                match event {
                    SshExecutionHostEvent::Status(status) => {
                        if status == ConnectionStatus::Connected {
                            reconnected_hosts.push(host_id.clone());
                        }
                    }
                    SshExecutionHostEvent::Worker(message) => {
                        worker_messages.push((host_id.clone(), *message));
                    }
                    SshExecutionHostEvent::Diagnostic(message) => {
                        events.push(ConnectionCatalogEvent::Diagnostic {
                            host_id: host_id.clone(),
                            message,
                        });
                    }
                    SshExecutionHostEvent::Tested(result) => {
                        events.push(ConnectionCatalogEvent::TestFinished {
                            host_id: host_id.clone(),
                            result,
                        });
                    }
                }
            }
            statuses.insert(host_id.clone(), host.status());
        }
        ConnectionCatalogPoll {
            statuses,
            reconnected_hosts,
            worker_messages,
            events,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ConnectionCatalogEvent {
    Diagnostic {
        host_id: ExecutionHostId,
        message: String,
    },
    TestFinished {
        host_id: ExecutionHostId,
        result: Result<(), String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::HostPath;
    use crate::persist::ssh_profiles::SshConnectionProfile;

    fn catalog() -> ConnectionCatalog {
        ConnectionCatalog::new(
            CoordinatorInstallationId::new("install-a").unwrap(),
            SessionNamespaceId::new("session-a").unwrap(),
        )
    }

    fn offline_profile(name: &str) -> SshConnectionProfile {
        SshConnectionProfile::new(
            name,
            name,
            format!("{name}.example"),
            Some(HostPath::new("/srv/work").unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn allocate_and_send_uses_saturating_overflow_on_test_hosts() {
        let mut catalog = catalog();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = catalog.connect_test_host(host_id.clone());
        catalog.set_next_test_request_id_for_test(u64::MAX);

        let first = catalog
            .allocate_and_send(&host_id, true, |request_id| CoordinatorMessage::Shutdown {
                request_id,
            })
            .expect("first allocate");
        let second = catalog
            .allocate_and_send(&host_id, true, |request_id| CoordinatorMessage::Shutdown {
                request_id,
            })
            .expect("second allocate");

        assert_eq!(first, RequestId::new(u64::MAX));
        assert_eq!(second, RequestId::new(u64::MAX));
        let locked = messages.lock().expect("message lock");
        assert_eq!(locked.len(), 2);
        assert!(matches!(
            &locked[0],
            CoordinatorMessage::Shutdown {
                request_id
            } if *request_id == first
        ));
        assert!(matches!(
            &locked[1],
            CoordinatorMessage::Shutdown {
                request_id
            } if *request_id == second
        ));
    }

    #[test]
    fn offline_ssh_host_allocation_uses_same_saturating_overflow_policy() {
        let mut catalog = catalog();
        let profile = offline_profile("overflow-box");
        let host_id = profile.execution_host_id();
        catalog.sync_profiles(&[profile]);
        assert!(catalog.set_host_next_request_id_for_test(&host_id, u64::MAX));

        let first = catalog
            .allocate_request_id(&host_id)
            .expect("offline host allocates");
        let second = catalog
            .allocate_request_id(&host_id)
            .expect("offline host still allocates at ceiling");

        assert_eq!(first, RequestId::new(u64::MAX));
        assert_eq!(second, RequestId::new(u64::MAX));

        // Offline send is a no-op success; reconnect replay reuses retained ids.
        catalog
            .send_message(
                &host_id,
                &CoordinatorMessage::Shutdown { request_id: first },
            )
            .expect("offline send is retained without transport");
    }

    #[test]
    fn test_and_offline_hosts_share_allocate_and_send_semantics() {
        let mut catalog = catalog();
        let profile = offline_profile("shared-semantics");
        let offline_host = profile.execution_host_id();
        catalog.sync_profiles(&[profile]);

        let test_host = ExecutionHostId::new("ssh:test-box:1").unwrap();
        let messages = catalog.connect_test_host(test_host.clone());

        // require_transport=true fails offline, succeeds for test hosts.
        let offline_err = catalog
            .allocate_and_send(&offline_host, true, |request_id| {
                CoordinatorMessage::Shutdown { request_id }
            })
            .expect_err("offline host has no live transport");
        assert!(matches!(
            offline_err,
            HostOperationError::Unavailable { host_id } if host_id == offline_host
        ));

        let test_id = catalog
            .allocate_and_send(&test_host, true, |request_id| {
                CoordinatorMessage::Shutdown { request_id }
            })
            .expect("test host transport is live");
        assert_eq!(test_id, RequestId::new(1));

        // require_transport=false still allocates offline without delivery.
        catalog.set_host_next_request_id_for_test(&offline_host, 7);
        let offline_id = catalog
            .allocate_and_send(&offline_host, false, |request_id| {
                CoordinatorMessage::Shutdown { request_id }
            })
            .expect("offline allocate without delivery");
        assert_eq!(offline_id, RequestId::new(7));
        assert!(messages.lock().expect("lock").len() == 1);
    }

    #[test]
    fn send_host_operation_routes_test_host_through_catalog_transport() {
        let mut catalog = catalog();
        let host_id = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let messages = catalog.connect_test_host(host_id.clone());
        let request_id = catalog
            .send_host_operation(host_id.clone(), WorkerCapability::Git, |request_id| {
                CoordinatorMessage::GitStatus {
                    request_id,
                    location: crate::execution_host::ResourceLocation::new(
                        host_id.clone(),
                        HostPath::new("/srv/work").unwrap(),
                    ),
                }
            })
            .expect("test host capability always passes");
        assert_eq!(request_id, RequestId::new(1));
        let locked = messages.lock().expect("lock");
        assert!(matches!(
            locked.as_slice(),
            [CoordinatorMessage::GitStatus {
                request_id: sent,
                ..
            }] if *sent == request_id
        ));
    }
}
