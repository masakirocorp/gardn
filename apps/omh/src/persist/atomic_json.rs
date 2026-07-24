//! Canonical atomic JSON persistence for coordinator-owned files.
//!
//! Owns symlink resolution, exclusive locking, temp cleanup, fsync, rename, and
//! platform replacement semantics so session and SSH profile writers stay aligned.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Follow symlinks manually so a write through a (possibly dangling) symlink
/// lands on the target. `fs::canonicalize` requires the target to exist, which
/// excludes the dangling-symlink case stow users hit on the very first save.
pub(crate) fn resolve_write_target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..16 {
        let meta = match std::fs::symlink_metadata(&current) {
            Ok(meta) => meta,
            Err(_) => return Ok(current),
        };
        if !meta.file_type().is_symlink() {
            return Ok(current);
        }
        let link = std::fs::read_link(&current)?;
        current = if link.is_absolute() {
            link
        } else {
            current
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(link)
        };
    }
    Ok(current)
}

fn lock_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    path.with_file_name(format!(".{file_name}.lock"))
}

/// Run `operation` while holding an exclusive lock adjacent to `path`.
pub(crate) fn with_path_lock<T>(
    path: &Path,
    operation: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let lock_path = lock_path_for(path);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock()?;
    operation()
}

fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = resolve_write_target(path)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = target.with_extension("json.tmp");
    // Best-effort cleanup if a previous attempt left a temp file behind.
    let _ = std::fs::remove_file(&tmp_path);

    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    #[cfg(windows)]
    if target.exists() {
        if let Err(err) = std::fs::remove_file(&target) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }
    }

    if let Err(err) = std::fs::rename(&tmp_path, &target) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }

    // Best-effort directory fsync so the rename itself is durable on Unix.
    #[cfg(unix)]
    if let Some(parent) = target.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(())
}

/// Serialize `value` as pretty JSON while holding the canonical adjacent lock.
pub(crate) fn save_json<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> io::Result<()> {
    with_path_lock(path, || save_json_unlocked(path, value))
}

/// Serialize inside a caller-owned transaction that already holds `path`'s lock.
pub(crate) fn save_json_unlocked<T: serde::Serialize + ?Sized>(
    path: &Path,
    value: &T,
) -> io::Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    write_atomic_bytes(path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample {
        value: u32,
    }

    fn temp_json_path(name: &str) -> PathBuf {
        let unique = format!(
            "omh-atomic-json-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("data.json")
    }

    #[test]
    fn save_json_leaves_no_tmp_file() {
        let path = temp_json_path("cleanup");
        save_json(&path, &Sample { value: 7 }).unwrap();
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn save_json_writes_through_dangling_symlink() {
        let target = temp_json_path("dangling-target");
        let link = target.with_file_name("link.json");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        save_json(&link, &Sample { value: 3 }).unwrap();

        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(target.exists());
        let body = std::fs::read_to_string(&target).unwrap();
        assert!(body.contains("\"value\": 3"));
        let _ = std::fs::remove_dir_all(target.parent().unwrap());
    }
}
