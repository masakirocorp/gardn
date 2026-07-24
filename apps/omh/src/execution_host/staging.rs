use std::collections::HashMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::HostPath;

pub(crate) const MAX_STAGED_FILE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const DEFAULT_STAGED_FILE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_STAGED_FILE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Private, execution-host-local files accepted from an authenticated coordinator.
///
/// Only paths created by this store can be removed through it. Expiry is enforced both
/// during the worker loop and whenever a new file is staged; `Drop` covers orderly worker
/// shutdown while the TTL covers a lost coordinator or client.
pub(crate) struct StagedFileStore {
    root: PathBuf,
    files: HashMap<PathBuf, SystemTime>,
}

impl StagedFileStore {
    pub(crate) fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        ensure_private_root(&root)?;
        cleanup_stale_files(&root, SystemTime::now());
        Ok(Self {
            root,
            files: HashMap::new(),
        })
    }

    pub(crate) fn stage(
        &mut self,
        extension: &str,
        data: &[u8],
        ttl_secs: u32,
        now: SystemTime,
    ) -> io::Result<HostPath> {
        if data.len() > MAX_STAGED_FILE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "staged file is {} bytes; limit is {MAX_STAGED_FILE_BYTES} bytes",
                    data.len()
                ),
            ));
        }
        self.cleanup_expired(now);

        let extension = sanitize_extension(extension);
        let unique = now
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }

        for attempt in 0..100 {
            let path = self.root.join(format!(
                "stage-{}-{unique}-{attempt}.{extension}",
                std::process::id()
            ));
            let mut file = match options.open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            if let Err(error) = file.write_all(data).and_then(|()| file.sync_all()) {
                let _ = fs::remove_file(&path);
                return Err(error);
            }
            let ttl = Duration::from_secs(u64::from(ttl_secs))
                .max(Duration::from_secs(1))
                .min(MAX_STAGED_FILE_TTL);
            self.files.insert(path.clone(), now + ttl);
            return HostPath::new(path).map_err(|error| io::Error::other(error.to_string()));
        }

        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "failed to allocate a unique staged file path",
        ))
    }

    pub(crate) fn remove(&mut self, path: &HostPath) -> io::Result<bool> {
        let path = path.as_path();
        if self.files.remove(path).is_none() {
            return Ok(false);
        }
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn cleanup_expired(&mut self, now: SystemTime) {
        self.files.retain(|path, expires_at| {
            if now < *expires_at {
                return true;
            }
            let _ = fs::remove_file(path);
            false
        });
    }
}

impl Drop for StagedFileStore {
    fn drop(&mut self) {
        for path in self.files.keys() {
            let _ = fs::remove_file(path);
        }
    }
}

fn sanitize_extension(extension: &str) -> &'static str {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "png",
        "jpg" | "jpeg" => "jpg",
        "gif" => "gif",
        "webp" => "webp",
        "bmp" => "bmp",
        _ => "png",
    }
}

fn ensure_private_root(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;
    if !root.is_dir() {
        return Err(io::Error::other(format!(
            "staging path is not a directory: {}",
            root.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn cleanup_stale_files(root: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().starts_with("stage-") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() > MAX_STAGED_FILE_TTL {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "omh-staging-test-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn staged_files_are_private_and_removed_by_owner() {
        let root = test_root("private");
        let mut store = StagedFileStore::new(&root).unwrap();
        let path = store
            .stage("png", b"image bytes", 60, SystemTime::now())
            .unwrap();
        assert_eq!(fs::read(path.as_path()).unwrap(), b"image bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(path.as_path()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(store.remove(&path).unwrap());
        assert!(!path.as_path().exists());
        drop(store);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expired_staged_files_are_removed() {
        let root = test_root("expired");
        let mut store = StagedFileStore::new(&root).unwrap();
        let now = SystemTime::now();
        let path = store.stage("png", b"image bytes", 1, now).unwrap();
        store.cleanup_expired(now + Duration::from_secs(2));
        assert!(!path.as_path().exists());
        drop(store);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn staged_file_removal_rejects_unowned_paths() {
        let root = test_root("ownership");
        let mut store = StagedFileStore::new(&root).unwrap();
        let unrelated = HostPath::new(store.root.join("not-owned.png")).unwrap();
        fs::write(unrelated.as_path(), b"keep").unwrap();
        assert!(!store.remove(&unrelated).unwrap());
        assert!(unrelated.as_path().exists());
        let _ = fs::remove_file(unrelated.as_path());
        drop(store);
        let _ = fs::remove_dir_all(root);
    }
}
