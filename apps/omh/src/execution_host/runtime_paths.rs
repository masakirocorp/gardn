use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::protocol::{
    CoordinatorInstallationId, HostBindingGeneration, SessionNamespaceId, PROTOCOL_VERSION,
};
use super::ExecutionHostId;

/// Mirrors `lifecycle::DAEMON_LIFECYCLE_VERSION` without importing that module
/// (lifecycle already depends on runtime_paths for binding digests).
const OWNERSHIP_DAEMON_LIFECYCLE_VERSION: u16 = 2;

const PRODUCT_COMPONENT: &str = "omh";
const ROLES_COMPONENT: &str = "roles";
const EXECUTION_WORKER_ROLE: &str = "execution-worker";
const EXECUTION_WORKER_RUNTIME_PREFIX: &str = "omh-ew";
const SOCKET_NAME: &str = "worker.sock";
const HOOK_SOCKET_NAME: &str = "hooks.sock";
const LOCK_NAME: &str = "worker.lock";
const OWNERSHIP_MANIFEST_NAME: &str = "ownership.json";

/// Durable ownership record for one execution-worker binding.
///
/// Written when a daemon binding is created/parsed and preserved across ordinary
/// daemon exit so retirement can inventory abandoned owned artifacts. Never
/// authorizes process signals by itself — only discovers Oh My Herdr-owned paths.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BindingOwnershipManifest {
    pub(crate) coordinator_installation_id: String,
    pub(crate) session_namespace_id: String,
    pub(crate) execution_host_id: String,
    pub(crate) host_binding_generation: u64,
    pub(crate) worker_instance_id: String,
    pub(crate) pid: u32,
    pub(crate) app_version: String,
    pub(crate) worker_protocol: u32,
    pub(crate) daemon_lifecycle_version: u16,
}

impl BindingOwnershipManifest {
    pub(crate) fn new(
        installation: &CoordinatorInstallationId,
        namespace: &SessionNamespaceId,
        host: &ExecutionHostId,
        generation: HostBindingGeneration,
        worker_instance_id: impl AsRef<str>,
        app_version: impl Into<String>,
    ) -> Self {
        Self {
            coordinator_installation_id: installation.to_string(),
            session_namespace_id: namespace.to_string(),
            execution_host_id: host.to_string(),
            host_binding_generation: generation.get(),
            worker_instance_id: worker_instance_id.as_ref().to_string(),
            pid: std::process::id(),
            app_version: app_version.into(),
            worker_protocol: PROTOCOL_VERSION,
            daemon_lifecycle_version: OWNERSHIP_DAEMON_LIFECYCLE_VERSION,
        }
    }

    pub(crate) fn matches_owner(&self, installation: &str, execution_host: &str) -> bool {
        self.coordinator_installation_id == installation && self.execution_host_id == execution_host
    }
}

/// One owned binding discovered by inventory or retirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OwnedBindingInventoryEntry {
    #[serde(flatten)]
    pub(crate) ownership: BindingOwnershipManifest,
    pub(crate) binding_root: String,
    pub(crate) runtime_dir: String,
    pub(crate) lock_live: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BindingInventoryReport {
    pub(crate) bindings: Vec<OwnedBindingInventoryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BindingRetirementReport {
    pub(crate) removed_bindings: Vec<OwnedBindingInventoryEntry>,
    pub(crate) blocked_bindings: Vec<OwnedBindingInventoryEntry>,
}

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

    pub(crate) fn hook_socket_path(&self) -> PathBuf {
        self.runtime_dir.join(HOOK_SOCKET_NAME)
    }

    pub(crate) fn lock_path(&self) -> PathBuf {
        self.runtime_dir.join(LOCK_NAME)
    }

    pub(crate) fn ownership_manifest_path(&self) -> PathBuf {
        self.binding_root.join(OWNERSHIP_MANIFEST_NAME)
    }

    pub(crate) fn prepare(&self) -> io::Result<()> {
        create_private_dir(&self.runtime_dir)?;
        create_private_dir(&self.binding_root)?;
        create_private_dir(&self.artifact_dir)
    }

    /// Atomically persist ownership metadata for this binding.
    ///
    /// Preserved across ordinary daemon exit so later retirement can inventory
    /// abandoned owned roots. Not process-kill authority.
    pub(crate) fn write_ownership_manifest(
        &self,
        manifest: &BindingOwnershipManifest,
    ) -> io::Result<()> {
        create_private_dir(&self.binding_root)?;
        write_ownership_manifest_atomic(&self.ownership_manifest_path(), manifest)
    }

    #[cfg(test)]
    pub(crate) fn read_ownership_manifest(&self) -> io::Result<Option<BindingOwnershipManifest>> {
        read_ownership_manifest_at(&self.ownership_manifest_path())
    }

    /// Remove the binding socket if present. Intended for the lock owner only.
    ///
    /// Does not touch the lock inode or the runtime directory so an exiting
    /// daemon never races a successor that still holds or is about to take the
    /// flock on the stable lock path.
    pub(crate) fn remove_socket_if_present(&self) -> io::Result<()> {
        remove_file_if_exists(&self.socket_path())
    }

    pub(crate) fn remove_hook_socket_if_present(&self) -> io::Result<()> {
        remove_file_if_exists(&self.hook_socket_path())
    }

    /// Ordinary daemon-exit cleanup. Removes the socket (lock owner only) and
    /// owned artifacts, but never the stable lock inode, runtime directory, or
    /// durable ownership manifest.
    pub(crate) fn cleanup(&self) -> io::Result<()> {
        self.remove_socket_if_present()?;
        self.remove_hook_socket_if_present()?;
        remove_dir_if_exists(&self.artifact_dir)?;
        match fs::remove_dir(&self.binding_root) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            // Ownership manifest (and any other retained metadata) keeps the root.
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => return Err(error),
        }
        // Intentionally do not remove runtime_dir / lock inode / ownership.json
        // on ordinary paths. Full retirement of abandoned roots is separate.
        let _ = &self.runtime_dir;
        Ok(())
    }

    /// Full retirement of this binding's roots after the lock is proven idle.
    pub(crate) fn retire_owned_paths(&self) -> io::Result<()> {
        self.remove_socket_if_present()?;
        self.remove_hook_socket_if_present()?;
        remove_dir_if_exists(&self.artifact_dir)?;
        remove_file_if_exists(&self.ownership_manifest_path())?;
        remove_dir_if_exists(&self.runtime_dir)?;
        match fs::remove_dir(&self.binding_root) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => Err(error),
            Err(error) => Err(error),
        }
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

/// Inventory owned bindings for one coordinator installation and execution host.
pub(crate) fn inventory_owned_bindings(
    installation: &str,
    execution_host: &str,
) -> io::Result<BindingInventoryReport> {
    inventory_owned_bindings_under(&std::env::temp_dir(), installation, execution_host)
}

/// Retire idle owned bindings for one coordinator installation and execution host.
///
/// Uses non-blocking flock on each binding lock. Live locks are reported in
/// `blocked_bindings` and never removed. Never kills by process name or PID.
pub(crate) fn retire_owned_bindings(
    installation: &str,
    execution_host: &str,
) -> io::Result<BindingRetirementReport> {
    retire_owned_bindings_under(&std::env::temp_dir(), installation, execution_host)
}

pub(crate) fn inventory_owned_bindings_under(
    base: &Path,
    installation: &str,
    execution_host: &str,
) -> io::Result<BindingInventoryReport> {
    let mut bindings = discover_owned_bindings(base, installation, execution_host)?;
    bindings.sort_by(|left, right| {
        left.ownership
            .host_binding_generation
            .cmp(&right.ownership.host_binding_generation)
            .then_with(|| left.binding_root.cmp(&right.binding_root))
    });
    Ok(BindingInventoryReport { bindings })
}

pub(crate) fn retire_owned_bindings_under(
    base: &Path,
    installation: &str,
    execution_host: &str,
) -> io::Result<BindingRetirementReport> {
    let discovered = discover_owned_bindings(base, installation, execution_host)?;
    let mut removed_bindings = Vec::new();
    let mut blocked_bindings = Vec::new();

    for entry in discovered {
        if entry.lock_live {
            blocked_bindings.push(entry);
            continue;
        }
        let paths = WorkerRolePaths {
            binding_root: PathBuf::from(&entry.binding_root),
            runtime_dir: PathBuf::from(&entry.runtime_dir),
            artifact_dir: PathBuf::from(&entry.binding_root).join("artifacts"),
        };
        // Re-check under an exclusive flock before unlinking. If the lock is
        // taken between inventory and removal, fail closed for that binding.
        match try_acquire_binding_lock(&paths.lock_path()) {
            Ok(_guard) => {
                paths.retire_owned_paths()?;
                removed_bindings.push(OwnedBindingInventoryEntry {
                    lock_live: false,
                    ..entry
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                blocked_bindings.push(OwnedBindingInventoryEntry {
                    lock_live: true,
                    ..entry
                });
            }
            Err(error) => return Err(error),
        }
    }

    Ok(BindingRetirementReport {
        removed_bindings,
        blocked_bindings,
    })
}

fn discover_owned_bindings(
    base: &Path,
    installation: &str,
    execution_host: &str,
) -> io::Result<Vec<OwnedBindingInventoryEntry>> {
    let role_root = base
        .join(PRODUCT_COMPONENT)
        .join(ROLES_COMPONENT)
        .join(EXECUTION_WORKER_ROLE);
    let entries = match fs::read_dir(&role_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut owned = Vec::new();
    for entry in entries {
        let entry = entry?;
        let binding_root = entry.path();
        if !binding_root.is_dir() {
            continue;
        }
        let Some(manifest) =
            read_ownership_manifest_at(&binding_root.join(OWNERSHIP_MANIFEST_NAME))?
        else {
            continue;
        };
        if !manifest.matches_owner(installation, execution_host) {
            continue;
        }
        let Ok(installation_id) =
            CoordinatorInstallationId::new(manifest.coordinator_installation_id.clone())
        else {
            continue;
        };
        let Ok(namespace_id) = SessionNamespaceId::new(manifest.session_namespace_id.clone())
        else {
            continue;
        };
        let Ok(host_id) = ExecutionHostId::new(manifest.execution_host_id.clone()) else {
            continue;
        };
        let paths = WorkerRolePaths::under(
            base,
            &installation_id,
            &namespace_id,
            &host_id,
            HostBindingGeneration::new(manifest.host_binding_generation),
        );
        if paths.binding_root != binding_root {
            // Manifest lives outside the scope-derived root — fail closed.
            continue;
        }
        let lock_live = binding_lock_is_live(&paths.lock_path())?;
        owned.push(OwnedBindingInventoryEntry {
            ownership: manifest,
            binding_root: paths.binding_root.display().to_string(),
            runtime_dir: paths.runtime_dir.display().to_string(),
            lock_live,
        });
    }
    Ok(owned)
}

fn read_ownership_manifest_at(path: &Path) -> io::Result<Option<BindingOwnershipManifest>> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            let manifest = serde_json::from_str(&raw).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid ownership manifest at {}: {error}", path.display()),
                )
            })?;
            Ok(Some(manifest))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_ownership_manifest_atomic(
    path: &Path,
    manifest: &BindingOwnershipManifest,
) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|error| io::Error::other(format!("serialize ownership manifest: {error}")))?;
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "ownership manifest path has no parent",
        )
    })?;
    create_private_dir(parent)?;

    let tmp_path = path.with_extension(format!(
        "json.tmp.{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(&json)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(&tmp_path, path)?;
        #[cfg(unix)]
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    write_result
}

fn binding_lock_is_live(path: &Path) -> io::Result<bool> {
    match try_acquire_binding_lock(path) {
        Ok(_guard) => Ok(false),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Non-blocking exclusive flock probe. Dropping the returned file releases the lock.
fn try_acquire_binding_lock(path: &Path) -> io::Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd as _;

        if let Some(parent) = path.parent() {
            match fs::create_dir_all(parent) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        let file = fs::OpenOptions::new()
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
                    "execution worker binding lock is held: {}",
                    io::Error::last_os_error()
                ),
            ))
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "execution-worker binding locks require unix",
        ))
    }
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

    fn paths_for_host(
        base: &Path,
        installation: &str,
        session: &str,
        host: &str,
        generation: u64,
    ) -> WorkerRolePaths {
        WorkerRolePaths::under(
            base,
            &CoordinatorInstallationId::new(installation).expect("valid installation id"),
            &SessionNamespaceId::new(session).expect("valid session namespace"),
            &ExecutionHostId::new(host).expect("valid execution host id"),
            HostBindingGeneration::new(generation),
        )
    }

    fn ownership_for(
        installation: &str,
        session: &str,
        host: &str,
        generation: u64,
        worker: &str,
    ) -> BindingOwnershipManifest {
        BindingOwnershipManifest::new(
            &CoordinatorInstallationId::new(installation).unwrap(),
            &SessionNamespaceId::new(session).unwrap(),
            &ExecutionHostId::new(host).unwrap(),
            HostBindingGeneration::new(generation),
            worker,
            "1.2.3-test",
        )
    }

    fn unique_base(_label: &str) -> PathBuf {
        static NEXT_BASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        #[cfg(unix)]
        let temp_root = PathBuf::from("/tmp");
        #[cfg(not(unix))]
        let temp_root = std::env::temp_dir();

        temp_root.join(format!(
            "omh-t-{}-{}",
            std::process::id(),
            NEXT_BASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    #[test]
    fn worker_binding_paths_include_role_and_full_binding_scope() {
        let base = unique_base("paths");
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
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn standalone_coordinator_and_execution_worker_bind_without_interference() {
        use std::os::unix::net::UnixListener;

        // Keep this under the portable Unix socket path limit; unique_base under
        // a long TMPDIR can otherwise exceed SUN_LEN for the coordinator socket.
        let base = Path::new("/tmp").join(format!(
            "omh-rb-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
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
        let base = unique_base("cleanup");
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
        let base = unique_base("lock-inode");
        let worker = paths(&base, "install-lock", "session-lock", 1);
        worker.prepare().expect("prepare");
        fs::write(worker.lock_path(), b"lock-holder").expect("create lock inode");
        fs::write(worker.socket_path(), b"socket").expect("create socket");

        worker.cleanup().expect("ordinary daemon exit cleanup");

        assert!(worker.lock_path().exists());
        assert!(worker.runtime_dir.exists());
        assert!(!worker.socket_path().exists());

        fs::write(worker.socket_path(), b"socket-again").expect("recreate socket");
        worker
            .remove_socket_if_present()
            .expect("lock owner removes socket");
        assert!(!worker.socket_path().exists());
        assert!(worker.lock_path().exists());

        fs::remove_dir_all(&base).expect("remove test tree");
    }

    #[test]
    fn ordinary_cleanup_preserves_ownership_manifest() {
        let base = unique_base("manifest-preserve");
        let worker = paths(&base, "install-keep", "session-keep", 1);
        worker.prepare().expect("prepare");
        let manifest = ownership_for(
            "install-keep",
            "session-keep",
            "ssh:workbox:1",
            1,
            "worker-keep",
        );
        worker
            .write_ownership_manifest(&manifest)
            .expect("write ownership");
        fs::write(worker.socket_path(), b"socket").expect("socket");
        fs::write(worker.artifact_dir().join("blob"), b"blob").expect("artifact");

        worker.cleanup().expect("ordinary cleanup");

        let retained = worker
            .read_ownership_manifest()
            .expect("read retained")
            .expect("manifest must survive ordinary exit");
        assert_eq!(retained.worker_instance_id, "worker-keep");
        assert!(worker.ownership_manifest_path().exists());
        assert!(!worker.socket_path().exists());
        assert!(!worker.artifact_dir().exists());
        fs::remove_dir_all(&base).expect("remove test tree");
    }

    #[test]
    fn inventory_filters_by_installation_and_execution_host() {
        let base = unique_base("inventory-filter");
        let owned = paths_for_host(&base, "install-a", "session-a", "ssh:workbox:1", 1);
        let other_install = paths_for_host(&base, "install-b", "session-a", "ssh:workbox:1", 1);
        let other_host = paths_for_host(&base, "install-a", "session-a", "ssh:other:2", 1);

        for (paths, installation, host, worker) in [
            (&owned, "install-a", "ssh:workbox:1", "worker-a"),
            (&other_install, "install-b", "ssh:workbox:1", "worker-b"),
            (&other_host, "install-a", "ssh:other:2", "worker-c"),
        ] {
            paths.prepare().expect("prepare");
            paths
                .write_ownership_manifest(&ownership_for(
                    installation,
                    "session-a",
                    host,
                    1,
                    worker,
                ))
                .expect("write ownership");
        }

        let report = inventory_owned_bindings_under(&base, "install-a", "ssh:workbox:1")
            .expect("inventory owned");
        assert_eq!(report.bindings.len(), 1);
        assert_eq!(report.bindings[0].ownership.worker_instance_id, "worker-a");
        assert_eq!(
            report.bindings[0].ownership.coordinator_installation_id,
            "install-a"
        );
        assert_eq!(
            report.bindings[0].ownership.execution_host_id,
            "ssh:workbox:1"
        );
        assert!(!report.bindings[0].lock_live);

        let encoded = serde_json::to_value(&report).expect("encode inventory");
        assert!(encoded
            .get("bindings")
            .and_then(|value| value.as_array())
            .is_some());
        assert!(encoded["bindings"][0].get("worker_instance_id").is_some());
        assert!(encoded["bindings"][0].get("app_version").is_some());
        assert!(encoded["bindings"][0].get("worker_protocol").is_some());
        assert!(encoded["bindings"][0]
            .get("daemon_lifecycle_version")
            .is_some());

        fs::remove_dir_all(&base).expect("remove test tree");
    }

    #[cfg(unix)]
    #[test]
    fn retirement_refuses_live_lock_and_is_idempotent_when_idle() {
        use std::os::fd::AsRawFd as _;

        let base = unique_base("retire-live");
        let worker = paths(&base, "install-a", "session-a", 3);
        worker.prepare().expect("prepare");
        worker
            .write_ownership_manifest(&ownership_for(
                "install-a",
                "session-a",
                "ssh:workbox:1",
                3,
                "worker-live",
            ))
            .expect("write ownership");
        fs::write(worker.socket_path(), b"socket").expect("socket");
        fs::write(worker.artifact_dir().join("blob"), b"blob").expect("artifact");

        let lock_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(worker.lock_path())
            .expect("open lock");
        let locked = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        assert_eq!(locked, 0, "test must hold the binding lock");

        let blocked = retire_owned_bindings_under(&base, "install-a", "ssh:workbox:1")
            .expect("retire while live");
        assert!(blocked.removed_bindings.is_empty());
        assert_eq!(blocked.blocked_bindings.len(), 1);
        assert!(blocked.blocked_bindings[0].lock_live);
        assert!(worker.ownership_manifest_path().exists());
        assert!(worker.runtime_dir.exists());

        drop(lock_file);

        let removed = retire_owned_bindings_under(&base, "install-a", "ssh:workbox:1")
            .expect("retire after idle");
        assert_eq!(removed.removed_bindings.len(), 1);
        assert!(removed.blocked_bindings.is_empty());
        assert!(!worker.ownership_manifest_path().exists());
        assert!(!worker.runtime_dir.exists());
        assert!(!worker.binding_root.exists());

        let again = retire_owned_bindings_under(&base, "install-a", "ssh:workbox:1")
            .expect("idempotent retire");
        assert!(again.removed_bindings.is_empty());
        assert!(again.blocked_bindings.is_empty());

        let encoded = serde_json::to_value(&removed).expect("encode retirement");
        assert!(encoded
            .get("removed_bindings")
            .and_then(|value| value.as_array())
            .is_some());
        assert!(encoded
            .get("blocked_bindings")
            .and_then(|value| value.as_array())
            .is_some());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn retirement_never_touches_foreign_installation_or_host() {
        let base = unique_base("retire-foreign");
        let owned = paths_for_host(&base, "install-a", "session-a", "ssh:workbox:1", 1);
        let foreign_install = paths_for_host(&base, "install-b", "session-a", "ssh:workbox:1", 1);
        let foreign_host = paths_for_host(&base, "install-a", "session-a", "ssh:other:9", 1);

        for (paths, installation, host, worker) in [
            (&owned, "install-a", "ssh:workbox:1", "worker-owned"),
            (
                &foreign_install,
                "install-b",
                "ssh:workbox:1",
                "worker-foreign-install",
            ),
            (
                &foreign_host,
                "install-a",
                "ssh:other:9",
                "worker-foreign-host",
            ),
        ] {
            paths.prepare().expect("prepare");
            paths
                .write_ownership_manifest(&ownership_for(
                    installation,
                    "session-a",
                    host,
                    1,
                    worker,
                ))
                .expect("write ownership");
            fs::write(paths.artifact_dir().join("keep"), b"keep").expect("artifact");
        }

        let report = retire_owned_bindings_under(&base, "install-a", "ssh:workbox:1")
            .expect("retire owned only");
        assert_eq!(report.removed_bindings.len(), 1);
        assert_eq!(
            report.removed_bindings[0].ownership.worker_instance_id,
            "worker-owned"
        );
        assert!(!owned.ownership_manifest_path().exists());
        assert!(foreign_install.ownership_manifest_path().exists());
        assert!(foreign_host.ownership_manifest_path().exists());
        assert!(foreign_install.artifact_dir().join("keep").exists());
        assert!(foreign_host.artifact_dir().join("keep").exists());

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
