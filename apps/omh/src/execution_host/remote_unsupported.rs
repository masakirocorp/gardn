use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::auth::{AuthenticationChallengeChannel, AuthenticationOwner};
use super::connection::ConnectionStatus;
use super::protocol::{
    CoordinatorInstallationId, CoordinatorMessage, RequestId, SessionNamespaceId, WorkerCapability,
    WorkerMessage,
};
use crate::persist::ssh_profiles::SshConnectionProfile;

const UNSUPPORTED_MESSAGE: &str = "SSH execution hosts are not supported on this platform";

#[derive(Clone)]
pub(crate) struct WorkerSender {
    next_request_id: Arc<AtomicU64>,
}

impl WorkerSender {
    pub(crate) fn next_request_id(&self) -> RequestId {
        allocate_request_id(&self.next_request_id)
    }

    pub(crate) fn send(&self, _message: &CoordinatorMessage) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            UNSUPPORTED_MESSAGE,
        ))
    }
}

#[derive(Clone)]
pub(crate) struct WorkerInstaller;

impl WorkerInstaller {
    pub(crate) fn new(
        _profile: SshConnectionProfile,
        _installation_id: CoordinatorInstallationId,
        _authentication: Arc<AuthenticationChallengeChannel>,
        _owner: AuthenticationOwner,
    ) -> Self {
        Self
    }

    pub(crate) fn inventory_owned_bindings(
        &self,
    ) -> Result<crate::execution_host::runtime_paths::BindingInventoryReport, String> {
        Err(UNSUPPORTED_MESSAGE.to_string())
    }

    pub(crate) fn retire_owned_bindings(
        &self,
    ) -> Result<crate::execution_host::runtime_paths::BindingRetirementReport, String> {
        Err(UNSUPPORTED_MESSAGE.to_string())
    }
}

#[derive(Debug)]
pub(crate) enum SshExecutionHostEvent {
    Status(ConnectionStatus),
    Worker(Box<WorkerMessage>),
    Diagnostic(String),
    Tested(Result<(), String>),
}

pub(crate) struct SshExecutionHost {
    profile: SshConnectionProfile,
    next_request_id: Arc<AtomicU64>,
    pending_diagnostic: bool,
}

impl SshExecutionHost {
    pub(crate) fn with_authentication_channel(
        profile: SshConnectionProfile,
        _installation_id: CoordinatorInstallationId,
        _session_namespace_id: SessionNamespaceId,
        _authentication: Arc<AuthenticationChallengeChannel>,
    ) -> Self {
        Self {
            profile,
            next_request_id: Arc::new(AtomicU64::new(1)),
            pending_diagnostic: false,
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
        ConnectionStatus::Disconnected
    }

    pub(crate) fn capabilities(&self) -> Option<&[WorkerCapability]> {
        None
    }

    pub(crate) fn sender(&self) -> Option<WorkerSender> {
        None
    }

    pub(crate) fn next_request_id(&self) -> RequestId {
        allocate_request_id(&self.next_request_id)
    }

    pub(crate) fn request_connect_for(&mut self, _owner: AuthenticationOwner) {
        self.pending_diagnostic = true;
    }

    pub(crate) fn request_test_for(
        &mut self,
        _owner: AuthenticationOwner,
    ) -> Option<Result<(), String>> {
        Some(Err(UNSUPPORTED_MESSAGE.to_string()))
    }

    pub(crate) fn request_disconnect_for(&mut self, _owner: AuthenticationOwner) {
        self.pending_diagnostic = false;
    }

    pub(crate) fn poll(&mut self, _now: Instant) -> Vec<SshExecutionHostEvent> {
        if std::mem::take(&mut self.pending_diagnostic) {
            vec![SshExecutionHostEvent::Diagnostic(
                UNSUPPORTED_MESSAGE.to_string(),
            )]
        } else {
            Vec::new()
        }
    }

    #[cfg(test)]
    pub(crate) fn set_next_request_id_for_test(&self, value: u64) {
        self.next_request_id.store(value, Ordering::Relaxed);
    }
}

fn allocate_request_id(next_request_id: &AtomicU64) -> RequestId {
    RequestId::new(
        next_request_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_add(1))
            })
            .unwrap_or_else(|current| current),
    )
}
