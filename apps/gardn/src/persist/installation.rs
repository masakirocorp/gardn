use sha2::{Digest as _, Sha256};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::execution_host::protocol::{CoordinatorInstallationId, SessionNamespaceId};

const INSTALLATION_ID_FILE: &str = "installation-id";
const INSTALLATION_ID_LOCK_FILE: &str = ".installation-id.lock";
static NEXT_OPAQUE_ID: AtomicU64 = AtomicU64::new(1);

fn installation_id_path() -> PathBuf {
    crate::config::config_dir().join(INSTALLATION_ID_FILE)
}

fn installation_id_lock_path() -> PathBuf {
    crate::config::config_dir().join(INSTALLATION_ID_LOCK_FILE)
}

/// Load the durable coordinator installation id, creating and persisting one when
/// missing or when a legacy on-disk value is empty/invalid.
pub(crate) fn load_or_create_installation_id() -> std::io::Result<CoordinatorInstallationId> {
    load_or_create_installation_id_at(&installation_id_path(), &installation_id_lock_path())
}

fn load_or_create_installation_id_at(
    path: &Path,
    lock_path: &Path,
) -> std::io::Result<CoordinatorInstallationId> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;

    if let Ok(existing) = std::fs::read_to_string(path) {
        let existing = existing.trim();
        if let Ok(id) = CoordinatorInstallationId::new(existing) {
            return Ok(id);
        }
    }

    let id = generate_installation_id(path);
    write_atomic(path, format!("{id}\n").as_bytes())?;
    Ok(id)
}

/// Mint a fresh session-namespace identity for a new coordinator session.
pub(crate) fn new_session_namespace_id() -> SessionNamespaceId {
    generate_session_namespace_id(&crate::config::config_dir())
}

/// Restore a session namespace from a snapshot DTO string.
///
/// Valid legacy values are preserved; empty or protocol-invalid values regenerate.
pub(crate) fn session_namespace_from_snapshot(raw: &str) -> SessionNamespaceId {
    match SessionNamespaceId::new(raw.trim()) {
        Ok(id) => id,
        Err(_) => new_session_namespace_id(),
    }
}

#[cfg(test)]
pub(crate) fn is_valid_session_namespace_id(raw: &str) -> bool {
    SessionNamespaceId::new(raw.trim()).is_ok()
}

fn generate_installation_id(path: &Path) -> CoordinatorInstallationId {
    // Generated opaque ids are always protocol-valid hex-with-dashes tokens.
    CoordinatorInstallationId::new(generate_opaque_id("installation", path))
        .expect("generated installation id must be protocol-valid")
}

fn generate_session_namespace_id(path: &Path) -> SessionNamespaceId {
    SessionNamespaceId::new(generate_opaque_id("session-namespace", path))
        .expect("generated session namespace id must be protocol-valid")
}

fn generate_opaque_id(scope: &str, path: &Path) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut digest = Sha256::new();
    digest.update(b"gardn-opaque-id-v1\0");
    digest.update(scope.as_bytes());
    digest.update(b"\0");
    digest.update(path.as_os_str().as_encoded_bytes());
    digest.update(b"\0");
    digest.update(std::process::id().to_le_bytes());
    digest.update(timestamp.to_le_bytes());
    digest.update(NEXT_OPAQUE_ID.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let bytes = digest.finalize();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn write_atomic(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp)?;
        file.write_all(content)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(label: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "gardn-installation-id-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        (dir.join("installation-id"), dir.join(".lock"))
    }

    #[test]
    fn installation_id_is_created_once_and_reused() {
        let (path, lock) = temp_paths("reuse");
        let first = load_or_create_installation_id_at(&path, &lock).unwrap();
        let second = load_or_create_installation_id_at(&path, &lock).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            first.as_str()
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn valid_legacy_installation_id_is_preserved() {
        let (path, lock) = temp_paths("preserve");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy = "install-legacy-ok_v1";
        std::fs::write(&path, format!("{legacy}\n")).unwrap();

        let loaded = load_or_create_installation_id_at(&path, &lock).unwrap();

        assert_eq!(loaded.as_str(), legacy);
        assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), legacy);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_partial_identity_is_replaced_and_persisted() {
        let (path, lock) = temp_paths("repair");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "partial id with spaces").unwrap();

        let repaired = load_or_create_installation_id_at(&path, &lock).unwrap();

        assert_ne!(repaired.as_str(), "partial id with spaces");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            repaired.as_str()
        );
        // Second load must reuse the healed value, not regenerate again.
        let again = load_or_create_installation_id_at(&path, &lock).unwrap();
        assert_eq!(again, repaired);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn empty_installation_id_is_replaced() {
        let (path, lock) = temp_paths("empty");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "\n").unwrap();

        let repaired = load_or_create_installation_id_at(&path, &lock).unwrap();
        assert!(!repaired.as_str().is_empty());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            repaired.as_str()
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn session_namespace_ids_are_typed_and_protocol_valid() {
        let id = new_session_namespace_id();
        assert!(!id.as_str().is_empty());
        assert_eq!(
            SessionNamespaceId::new(id.as_str()).expect("round-trip"),
            id
        );
    }

    #[test]
    fn snapshot_namespace_preserves_valid_legacy_and_heals_invalid() {
        let valid = "session-legacy-ok";
        let preserved = session_namespace_from_snapshot(valid);
        assert_eq!(preserved.as_str(), valid);

        let empty = session_namespace_from_snapshot("");
        assert!(!empty.as_str().is_empty());
        assert_ne!(empty.as_str(), "");

        let invalid = session_namespace_from_snapshot("bad id with spaces");
        assert!(!invalid.as_str().is_empty());
        assert_ne!(invalid.as_str(), "bad id with spaces");
        assert!(is_valid_session_namespace_id(invalid.as_str()));
        assert!(!is_valid_session_namespace_id("bad id with spaces"));
        assert!(!is_valid_session_namespace_id(""));
    }
}
