#[cfg(unix)]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

#[cfg(unix)]
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt};

#[cfg(unix)]
const DIRECTORY_MODE: u32 = 0o700;
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

#[derive(Debug)]
pub(crate) struct FileStore {
    base: PathBuf,
    generation: OnceLock<Arc<Generation>>,
    next_fingerprint: AtomicU64,
}

#[derive(Debug)]
struct Generation {
    root: PathBuf,
    source: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct Lease {
    inner: Arc<LeaseInner>,
}

#[derive(Debug)]
struct LeaseInner {
    path: PathBuf,
    file: File,
    generation: Arc<Generation>,
    metadata: fs::Metadata,
    len: usize,
    fingerprint: u64,
}

impl Default for FileStore {
    fn default() -> Self {
        Self::new(runtime_base())
    }
}

impl FileStore {
    fn new(base: PathBuf) -> Self {
        Self {
            base,
            generation: OnceLock::new(),
            next_fingerprint: AtomicU64::new(1),
        }
    }

    pub(crate) fn source_directory(&self) -> io::Result<PathBuf> {
        Ok(self.generation()?.source.clone())
    }

    pub(crate) fn lease(&self, path: &Path, expected_len: usize) -> io::Result<Lease> {
        let generation = self.generation()?;
        validate_child(path, &generation.source)?;
        let file = open_no_follow(path)?;
        let metadata = file.metadata()?;
        validate_metadata(&metadata, expected_len)?;
        validate_path_identity(path, &metadata)?;
        let fingerprint = self.next_fingerprint.fetch_add(1, Ordering::Relaxed);
        Ok(Lease {
            inner: Arc::new(LeaseInner {
                path: path.to_owned(),
                file,
                generation,
                metadata,
                len: expected_len,
                fingerprint,
            }),
        })
    }

    fn generation(&self) -> io::Result<Arc<Generation>> {
        if let Some(generation) = self.generation.get() {
            return Ok(Arc::clone(generation));
        }
        let generation = Arc::new(create_generation(&self.base)?);
        match self.generation.set(Arc::clone(&generation)) {
            Ok(()) => Ok(generation),
            Err(_) => self
                .generation
                .get()
                .cloned()
                .ok_or_else(|| io::Error::other("pane graphics generation was lost")),
        }
    }
}

impl Lease {
    pub(crate) fn path(&self) -> &Path {
        &self.inner.path
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.len
    }

    pub(crate) fn fingerprint(&self) -> u64 {
        self.inner.fingerprint
    }

    pub(crate) fn copy_rgba(&self) -> io::Result<Vec<u8>> {
        let _keep_generation_alive = &self.inner.generation;
        validate_path_identity(&self.inner.path, &self.inner.metadata)?;
        let mut data = vec![0; self.inner.len];
        read_exact_at(&self.inner.file, &mut data)?;
        if has_byte_at(&self.inner.file, self.inner.len as u64)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "frame length changed while leased",
            ));
        }
        validate_metadata(&self.inner.file.metadata()?, self.inner.len)?;
        validate_path_identity(&self.inner.path, &self.inner.metadata)?;
        Ok(data)
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        if let Err(err) = fs::remove_dir_all(&self.root) {
            if err.kind() != io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %self.root.display(),
                    err = %err,
                    "failed to remove pane graphics directory"
                );
            }
        }
    }
}

#[cfg(unix)]
pub(crate) fn validate_direct_source(path: &Path, expected_len: usize) -> io::Result<()> {
    let source = path.parent().ok_or_else(invalid_path)?;
    let generation = source.parent().ok_or_else(invalid_path)?;
    if !path.is_absolute()
        || path.file_name().is_none()
        || source.file_name().and_then(|name| name.to_str()) != Some("source")
        || generation.parent() != Some(runtime_base().as_path())
        || !generation
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("server-"))
    {
        return Err(invalid_path());
    }
    for directory in [runtime_base(), generation.to_owned(), source.to_owned()] {
        validate_directory(&directory)?;
    }
    let file = open_no_follow(path)?;
    let metadata = file.metadata()?;
    validate_metadata(&metadata, expected_len)?;
    validate_path_identity(path, &metadata)
}

#[cfg(not(unix))]
pub(crate) fn validate_direct_source(_path: &Path, _expected_len: usize) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "file-backed pane graphics require Unix",
    ))
}

fn create_generation(base: &Path) -> io::Result<Generation> {
    #[cfg(not(unix))]
    {
        let _ = base;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "file-backed pane graphics require Unix",
        ))
    }
    #[cfg(unix)]
    {
        fs::create_dir_all(base)?;
        fs::set_permissions(base, fs::Permissions::from_mode(DIRECTORY_MODE))?;
        validate_directory(base)?;
        remove_stale_generations(base);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = base.join(format!("server-{}-{nonce}", std::process::id()));
        fs::create_dir(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(DIRECTORY_MODE))?;
        let source = root.join("source");
        fs::create_dir(&source)?;
        fs::set_permissions(&source, fs::Permissions::from_mode(DIRECTORY_MODE))?;
        validate_directory(&root)?;
        validate_directory(&source)?;
        Ok(Generation { root, source })
    }
}

fn runtime_base() -> PathBuf {
    #[cfg(unix)]
    {
        let root = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/var/tmp"));
        root.join(format!("gardn-pane-graphics-{}", effective_uid()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from("pane-graphics-unavailable")
    }
}

#[cfg(unix)]
fn read_exact_at(file: &File, data: &mut [u8]) -> io::Result<()> {
    file.read_exact_at(data, 0)
}

#[cfg(not(unix))]
fn read_exact_at(_file: &File, _data: &mut [u8]) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
}

#[cfg(unix)]
fn has_byte_at(file: &File, offset: u64) -> io::Result<bool> {
    Ok(file.read_at(&mut [0], offset)? != 0)
}

#[cfg(not(unix))]
fn has_byte_at(_file: &File, _offset: u64) -> io::Result<bool> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
}

fn invalid_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "invalid pane graphics path")
}

fn validate_child(path: &Path, source: &Path) -> io::Result<()> {
    if path.parent() != Some(source) || path.file_name().is_none() {
        return Err(invalid_path());
    }
    Ok(())
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_no_follow(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
}

#[cfg(unix)]
fn validate_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(invalid_path());
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_directory(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
}

#[cfg(unix)]
fn validate_metadata(metadata: &fs::Metadata, expected_len: usize) -> io::Result<()> {
    if !metadata.is_file()
        || metadata.len() != expected_len as u64
        || metadata.permissions().mode() & !FILE_MODE != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "pane graphics file metadata is invalid",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_metadata(_metadata: &fs::Metadata, _expected_len: usize) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
}

#[cfg(unix)]
fn validate_path_identity(path: &Path, expected: &fs::Metadata) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.dev() != expected.dev() || metadata.ino() != expected.ino() {
        return Err(invalid_path());
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_path_identity(_path: &Path, _expected: &fs::Metadata) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix only"))
}

#[cfg(unix)]
fn remove_stale_generations(base: &Path) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("server-"))
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}
