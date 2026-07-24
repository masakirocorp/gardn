use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::protocol::{CoordinatorInstallationId, HostBindingGeneration, SessionNamespaceId};
use super::ExecutionHostId;

const PRODUCT_COMPONENT: &str = "omh";
const ROLES_COMPONENT: &str = "roles";
const EXECUTION_WORKER_ROLE: &str = "execution-worker";
const EXECUTION_WORKER_RUNTIME_PREFIX: &str = "omh-ew";
const SOCKET_NAME: &str = "worker.sock";
const LOCK_NAME: &str = "worker.lock";

/// Every filesystem artifact owned by one persistent execution-worker binding.
///
/// The binding scope deliberately excludes the worker incarnation: reconnecting
/// bridges for the same coordinator/session/host binding must discover the same
/// daemon. A changed host binding generation gets a disjoint scope.
///
/// Socket and lock paths are binding-scoped and unversioned. The lock inode is
/// stable across ordinary daemon exits so `flock` holders and late joiners keep
/// a durable rendezvous; only the lock owner may unlink the socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkerRolePaths {
    binding_root: PathBuf,
    runtime_dir: PathBuf,
    artifact_dir: PathBuf,
}

impl WorkerRolePaths {
    pub(crate) fn for_binding(
        installation: &CoordinatorInstallationId,
        namespace: &SessionNamespaceId,
        host: &ExecutionHostId,
        generation: HostBindingGeneration,
    ) -> Self {
        Self::under(
            &std::env::temp_dir(),
            installation,
            namespace,
            host,
            generation,
        )
    }

    fn under(
        base: &Path,
        installation: &CoordinatorInstallationId,
        namespace: &SessionNamespaceId,
        host: &ExecutionHostId,
        generation: HostBindingGeneration,
    ) -> Self {
        let scope = binding_scope(installation, namespace, host, generation);
        let binding_root = base
            .join(PRODUCT_COMPONENT)
            .join(ROLES_COMPONENT)
            .join(EXECUTION_WORKER_ROLE)
            .join(&scope);
        let runtime_dir = short_runtime_dir(base, &scope);
        Self {
            runtime_dir,
            artifact_dir: binding_root.join("artifacts"),
            binding_root,
        }
    }

    pub(crate) fn artifact_dir(&self) -> &Path {
        &self.artifact_dir
    }

    pub(crate) fn socket_path(&self) -> PathBuf {
        self.runtime_dir.join(SOCKET_NAME)
    }

    pub(crate) fn lock_path(&self) -> PathBuf {
        self.runtime_dir.join(LOCK_NAME)
    }

    pub(crate) fn prepare(&self) -> io::Result<()> {
        create_private_dir(&self.runtime_dir)?;
        create_private_dir(&self.artifact_dir)
    }

    /// Remove the binding socket if present. Intended for the lock owner only.
    ///
    /// Does not touch the lock inode or the runtime directory so an exiting
    /// daemon never races a successor that still holds or is about to take the
    /// flock on the stable lock path.
    pub(crate) fn remove_socket_if_present(&self) -> io::Result<()> {
        remove_file_if_exists(&self.socket_path())
    }

    /// Ordinary daemon-exit cleanup. Removes the socket (lock owner only) and
    /// owned artifacts, but never the stable lock inode or runtime directory.
    pub(crate) fn cleanup(&self) -> io::Result<()> {
        self.remove_socket_if_present()?;
        remove_dir_if_exists(&self.artifact_dir)?;
        match fs::remove_dir(&self.binding_root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => return Err(error),
        }
        // Intentionally do not remove runtime_dir / lock inode on ordinary paths.
        // Full retirement of an abandoned runtime dir is a separate operator action.
        let _ = &self.runtime_dir;
        Ok(())
    }
}

/// First 16 bytes of the binding-scope SHA-256 digest.
pub(crate) fn binding_scope_digest(
    installation: &CoordinatorInstallationId,
    namespace: &SessionNamespaceId,
    host: &ExecutionHostId,
    generation: HostBindingGeneration,
) -> [u8; 16] {
    let mut digest = Sha256::new();
    for component in [
        installation.as_str(),
        namespace.as_str(),
        host.as_str(),
        &generation.get().to_string(),
    ] {
        digest.update(component.len().to_be_bytes());
        digest.update(component.as_bytes());
    }
    let digest = digest.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

/// Hex encoding of a binding digest (path component form).
pub(crate) fn binding_scope_hex(digest: &[u8; 16]) -> String {
    let mut scope = String::with_capacity(32);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(scope, "{byte:02x}");
    }
    scope
}

fn binding_scope(
    installation: &CoordinatorInstallationId,
    namespace: &SessionNamespaceId,
    host: &ExecutionHostId,
    generation: HostBindingGeneration,
) -> String {
    binding_scope_hex(&binding_scope_digest(
        installation,
        namespace,
        host,
        generation,
    ))
}

fn short_runtime_dir(base: &Path, scope: &str) -> PathBuf {
    let component = format!("{EXECUTION_WORKER_RUNTIME_PREFIX}-{scope}");
    let candidate = base.join(&component);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        const PORTABLE_UNIX_SOCKET_PATH_LIMIT: usize = 103;
        if candidate.join(SOCKET_NAME).as_os_str().as_bytes().len()
            > PORTABLE_UNIX_SOCKET_PATH_LIMIT
        {
            return Path::new("/tmp").join(component);
        }
    }
    candidate
}

fn remove_dir_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(base: &Path, installation: &str, session: &str, generation: u64) -> WorkerRolePaths {
        WorkerRolePaths::under(
            base,
            &CoordinatorInstallationId::new(installation).expect("valid installation id"),
            &SessionNamespaceId::new(session).expect("valid session namespace"),
            &ExecutionHostId::new("ssh:workbox:1").expect("valid execution host id"),
            HostBindingGeneration::new(generation),
        )
    }

    #[test]
    fn worker_binding_paths_include_role_and_full_binding_scope() {
        let base = std::env::temp_dir().join(format!(
            "omh-role-paths-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("binding")
        ));
        let first = paths(&base, "install-a", "session-a", 1);
        let other_installation = paths(&base, "install-b", "session-a", 1);
        let other_session = paths(&base, "install-a", "session-b", 1);
        let other_generation = paths(&base, "install-a", "session-a", 2);

        assert!(first
            .binding_root
            .components()
            .any(|component| component.as_os_str() == EXECUTION_WORKER_ROLE));
        assert_ne!(first.binding_root, other_installation.binding_root);
        assert_ne!(first.binding_root, other_session.binding_root);
        assert_ne!(first.binding_root, other_generation.binding_root);
        assert!(first.socket_path().starts_with(&first.runtime_dir));
        assert!(first.lock_path().starts_with(&first.runtime_dir));
        assert!(first.artifact_dir().starts_with(&first.binding_root));
    }

    #[cfg(unix)]
    #[test]
    fn standalone_coordinator_and_execution_worker_bind_without_interference() {
        use std::os::unix::net::UnixListener;

        let base = std::env::temp_dir().join(format!("omh-role-bind-{}", std::process::id()));
        let coordinator_root = base.join("coordinator");
        let coordinator_socket = coordinator_root.join("omh.sock");
        let worker = paths(&base, "install-socket", "session-socket", 1);
        fs::create_dir_all(&coordinator_root).expect("prepare coordinator path");
        worker.prepare().expect("prepare worker path");
        let coordinator_listener =
            UnixListener::bind(&coordinator_socket).expect("bind coordinator socket");
        let worker_listener =
            UnixListener::bind(worker.socket_path()).expect("bind execution-worker socket");

        assert_ne!(coordinator_socket, worker.socket_path());
        assert!(coordinator_socket.exists());
        assert!(worker.socket_path().exists());

        drop(worker_listener);
        fs::write(worker.lock_path(), b"lock").expect("create lock inode");
        worker.cleanup().expect("clean worker binding");
        assert!(coordinator_socket.exists());
        assert!(
            worker.lock_path().exists(),
            "ordinary cleanup must preserve stable lock inode"
        );
        assert!(!worker.socket_path().exists());
        drop(coordinator_listener);
        fs::remove_dir_all(&base).expect("remove test tree");
    }

    #[test]
    fn worker_cleanup_cannot_remove_coordinator_or_other_binding_state() {
        let base = std::env::temp_dir().join(format!(
            "omh-role-cleanup-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("cleanup")
        ));
        let worker = paths(&base, "install-a", "session-a", 1);
        let other_worker = paths(&base, "install-a", "session-b", 1);
        let coordinator_root = base.join(PRODUCT_COMPONENT).join("coordinator");
        let coordinator_marker = coordinator_root.join("omh.sock");
        let other_worker_marker = other_worker.artifact_dir().join("worker-marker");

        worker.prepare().expect("prepare worker paths");
        other_worker.prepare().expect("prepare other worker paths");
        fs::create_dir_all(&coordinator_root).expect("prepare coordinator paths");
        fs::write(worker.artifact_dir().join("worker-marker"), b"worker")
            .expect("write worker artifact");
        fs::write(&other_worker_marker, b"other worker").expect("write other worker state");
        fs::write(&coordinator_marker, b"coordinator").expect("write coordinator state");
        fs::write(worker.lock_path(), b"lock").expect("write lock inode");
        fs::write(worker.socket_path(), b"socket").expect("write socket");

        worker.cleanup().expect("clean owned artifacts");

        assert!(
            worker.runtime_dir.exists(),
            "ordinary cleanup must keep the runtime dir for the stable lock inode"
        );
        assert!(
            worker.lock_path().exists(),
            "ordinary cleanup must keep the lock inode"
        );
        assert!(!worker.socket_path().exists());
        assert!(!worker.artifact_dir().exists());
        assert!(other_worker_marker.exists());
        assert!(coordinator_marker.exists());
        other_worker.cleanup().expect("clean other worker binding");
        fs::remove_dir_all(&base).expect("remove test tree");
    }

    #[test]
    fn ordinary_cleanup_preserves_lock_inode_while_only_owner_removes_socket() {
        let base = std::env::temp_dir().join(format!(
            "omh-role-lock-inode-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("lock-inode")
        ));
        let worker = paths(&base, "install-lock", "session-lock", 1);
        worker.prepare().expect("prepare");
        fs::write(worker.lock_path(), b"lock-holder").expect("create lock inode");
        fs::write(worker.socket_path(), b"socket").expect("create socket");

        // Non-owner path: never call remove_socket / cleanup while another
        // process could still flock the lock. Ordinary exit uses owned cleanup.
        worker.cleanup().expect("ordinary daemon exit cleanup");

        assert!(worker.lock_path().exists());
        assert!(worker.runtime_dir.exists());
        assert!(!worker.socket_path().exists());

        // Lock owner is the only caller allowed to target the socket explicitly.
        fs::write(worker.socket_path(), b"socket-again").expect("recreate socket");
        worker
            .remove_socket_if_present()
            .expect("lock owner removes socket");
        assert!(!worker.socket_path().exists());
        assert!(worker.lock_path().exists());

        fs::remove_dir_all(&base).expect("remove test tree");
    }

    #[test]
    fn binding_scope_digest_is_stable_hex_for_paths() {
        let installation = CoordinatorInstallationId::new("install-a").unwrap();
        let namespace = SessionNamespaceId::new("session-a").unwrap();
        let host = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let generation = HostBindingGeneration::new(1);
        let digest = binding_scope_digest(&installation, &namespace, &host, generation);
        let hex = binding_scope_hex(&digest);
        assert_eq!(hex.len(), 32);
        assert_eq!(
            binding_scope(&installation, &namespace, &host, generation),
            hex
        );
        let again = binding_scope_digest(&installation, &namespace, &host, generation);
        assert_eq!(digest, again);
    }
}
