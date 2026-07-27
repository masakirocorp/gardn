//! Daemon binding identity and path derivation.

use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::execution_host::lifecycle::binding_digest_for;
use crate::execution_host::protocol::{
    validate_first_coordinator_message, CoordinatorInstallationId, CoordinatorMessage,
    HostBindingGeneration, SessionNamespaceId, WorkerInstanceId, PROTOCOL_VERSION,
};
use crate::execution_host::runtime_paths::{BindingOwnershipManifest, WorkerRolePaths};
use crate::execution_host::ExecutionHostId;

use super::util::{required, WORKER_APP_VERSION};

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DaemonBinding {
    pub(super) installation_id: CoordinatorInstallationId,
    pub(super) session_namespace_id: SessionNamespaceId,
    pub(super) execution_host_id: ExecutionHostId,
    pub(super) host_binding_generation: HostBindingGeneration,
    pub(super) worker_instance_id: WorkerInstanceId,
    pub(super) socket_path: PathBuf,
}

#[cfg(unix)]
impl DaemonBinding {
    pub(super) fn from_hello(message: &CoordinatorMessage) -> io::Result<Self> {
        validate_first_coordinator_message(message).map_err(io::Error::other)?;
        let CoordinatorMessage::Hello {
            coordinator_installation_id,
            session_namespace_id,
            execution_host_id,
            host_binding_generation,
            ..
        } = message
        else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "first worker message is not Hello",
            ));
        };
        let worker_instance_id = new_worker_instance_id()?;
        let role_paths = WorkerRolePaths::for_binding(
            coordinator_installation_id,
            session_namespace_id,
            execution_host_id,
            *host_binding_generation,
        );
        role_paths.prepare()?;
        let binding = Self {
            installation_id: coordinator_installation_id.clone(),
            session_namespace_id: session_namespace_id.clone(),
            execution_host_id: execution_host_id.clone(),
            host_binding_generation: *host_binding_generation,
            worker_instance_id,
            socket_path: role_paths.socket_path(),
        };
        binding.write_ownership_manifest()?;
        Ok(binding)
    }

    pub(super) fn parse(args: &[String]) -> io::Result<Self> {
        let mut installation = None;
        let mut namespace = None;
        let mut host = None;
        let mut generation = None;
        let mut worker = None;
        let mut socket = None;
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            let value = args.get(index + 1).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("missing value for {flag}"),
                )
            })?;
            match flag {
                "--installation" => installation = Some(value.clone()),
                "--session-namespace" => namespace = Some(value.clone()),
                "--execution-host" => host = Some(value.clone()),
                "--host-binding-generation" => {
                    generation = Some(value.parse::<u64>().map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidInput, err.to_string())
                    })?)
                }
                "--worker-instance" => worker = Some(value.clone()),
                "--socket" => socket = Some(PathBuf::from(value)),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unknown execution-worker daemon argument: {flag}"),
                    ));
                }
            }
            index += 2;
        }
        let installation_id =
            CoordinatorInstallationId::new(required(installation, "installation")?)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err.to_string()))?;
        let session_namespace_id =
            SessionNamespaceId::new(required(namespace, "session namespace")?)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err.to_string()))?;
        let execution_host_id = ExecutionHostId::new(required(host, "execution host")?)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err.to_string()))?;
        let host_binding_generation =
            HostBindingGeneration::new(required(generation, "binding generation")?);
        let worker_instance_id = WorkerInstanceId::new(required(worker, "worker instance")?)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err.to_string()))?;
        let socket_path = required(socket, "socket")?;
        let role_paths = WorkerRolePaths::for_binding(
            &installation_id,
            &session_namespace_id,
            &execution_host_id,
            host_binding_generation,
        );
        let expected_socket = role_paths.socket_path();
        if socket_path != expected_socket {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "execution-worker socket does not match its binding",
            ));
        }
        role_paths.prepare()?;
        let binding = Self {
            installation_id,
            session_namespace_id,
            execution_host_id,
            host_binding_generation,
            worker_instance_id,
            socket_path,
        };
        binding.write_ownership_manifest()?;
        Ok(binding)
    }

    pub(super) fn daemon_args(&self) -> Vec<String> {
        vec![
            "execution-worker".into(),
            "--daemon".into(),
            "--installation".into(),
            self.installation_id.to_string(),
            "--session-namespace".into(),
            self.session_namespace_id.to_string(),
            "--execution-host".into(),
            self.execution_host_id.to_string(),
            "--host-binding-generation".into(),
            self.host_binding_generation.to_string(),
            "--worker-instance".into(),
            self.worker_instance_id.to_string(),
            "--socket".into(),
            self.socket_path.display().to_string(),
        ]
    }

    pub(super) fn matches_hello(&self, hello: &CoordinatorMessage) -> bool {
        matches!(
            hello,
            CoordinatorMessage::Hello {
                version,
                coordinator_installation_id,
                session_namespace_id,
                execution_host_id,
                host_binding_generation,
                ..
            } if *version == PROTOCOL_VERSION
                && coordinator_installation_id == &self.installation_id
                && session_namespace_id == &self.session_namespace_id
                && execution_host_id == &self.execution_host_id
                && host_binding_generation == &self.host_binding_generation
        )
    }

    pub(super) fn role_paths(&self) -> WorkerRolePaths {
        WorkerRolePaths::for_binding(
            &self.installation_id,
            &self.session_namespace_id,
            &self.execution_host_id,
            self.host_binding_generation,
        )
    }

    pub(super) fn binding_digest(&self) -> [u8; 16] {
        binding_digest_for(
            &self.installation_id,
            &self.session_namespace_id,
            &self.execution_host_id,
            self.host_binding_generation,
        )
    }

    pub(super) fn write_ownership_manifest(&self) -> io::Result<()> {
        let role_paths = self.role_paths();
        let manifest = BindingOwnershipManifest::new(
            &self.installation_id,
            &self.session_namespace_id,
            &self.execution_host_id,
            self.host_binding_generation,
            self.worker_instance_id.to_string(),
            WORKER_APP_VERSION,
        );
        role_paths.write_ownership_manifest(&manifest)
    }
}

#[cfg(unix)]
pub(super) fn new_worker_instance_id() -> io::Result<WorkerInstanceId> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    WorkerInstanceId::new(format!("worker-{}-{nanos:x}", std::process::id()))
        .map_err(|err| io::Error::other(err.to_string()))
}
