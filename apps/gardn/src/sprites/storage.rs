use gardn_local_api::sprites::{
    SpriteError, SpriteOperation, SpriteOperationStatus, SpriteRecord, SpriteResult, SpritesConfig,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn error(code: &str, message: impl Into<String>, retryable: bool) -> SpriteError {
    SpriteError {
        code: code.into(),
        message: message.into(),
        retryable,
    }
}
pub(super) fn io_error(cause: std::io::Error) -> SpriteError {
    error(
        "persistence",
        format!("Sprites local storage error: {cause}"),
        true,
    )
}
pub(super) fn hash_id(id: &str) -> String {
    crate::checksum::to_lower_hex(&Sha256::digest(id.as_bytes()))
}
pub(super) fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
pub(super) fn is_terminal(status: &SpriteOperationStatus) -> bool {
    matches!(
        status,
        SpriteOperationStatus::Succeeded
            | SpriteOperationStatus::Failed
            | SpriteOperationStatus::Interrupted
            | SpriteOperationStatus::Canceled
    )
}

pub(super) fn create_private_dir(path: &Path) -> Result<(), SpriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => {
            return Err(error(
                "unsafe_storage",
                format!("Sprites storage is not a directory: {}", path.display()),
                false,
            ))
        }
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
        Err(cause) => return Err(io_error(cause)),
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_private_dir(parent)?;
    }
    match crate::platform::create_remote_private_dir(path) {
        Ok(()) => Ok(()),
        Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => create_private_dir(path),
        Err(cause) => Err(io_error(cause)),
    }
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SpriteError> {
    let parent = path
        .parent()
        .ok_or_else(|| error("persistence", "Sprites storage path has no parent", false))?;
    create_private_dir(parent)?;
    let temp = parent.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = crate::platform::create_remote_ssh_config_file(&temp).map_err(io_error)?;
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(&temp, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// OS-released lock serializes local intent admission, including separate Gardn sessions.
pub(super) fn lock_intents(root: &Path) -> Result<fs::File, SpriteError> {
    create_private_dir(root)?;
    let path = root.join("intents.lock");
    let file = match crate::platform::create_remote_ssh_config_file(&path) {
        Ok(file) => file,
        Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(io_error)?,
        Err(cause) => return Err(io_error(cause)),
    };
    file.try_lock().map_err(|cause| {
        error(
            "operation_busy",
            format!("Another Gardn session is admitting a Sprites operation: {cause}"),
            true,
        )
    })?;
    Ok(file)
}

pub(super) fn persist_operation(
    root: &Path,
    operation: &SpriteOperation,
) -> Result<(), SpriteError> {
    let bytes = serde_json::to_vec(operation).map_err(|cause| {
        error(
            "persistence",
            format!("Could not encode Sprites operation: {cause}"),
            false,
        )
    })?;
    atomic_write(
        &root
            .join("operations")
            .join(format!("{}.json", hash_id(&operation.id))),
        &bytes,
    )
}
pub(super) fn read_operation(
    root: &Path,
    id: &str,
) -> Result<Option<SpriteOperation>, SpriteError> {
    read_optional_json(
        &root
            .join("operations")
            .join(format!("{}.json", hash_id(id))),
    )
}
pub(super) fn read_optional_json<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Option<T>, SpriteError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|cause| {
            error(
                "state_corrupt",
                format!("Could not read Sprites state {}: {cause}", path.display()),
                false,
            )
        }),
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(cause) => Err(io_error(cause)),
    }
}
pub(super) fn load_operations(root: &Path) -> Result<Vec<SpriteOperation>, SpriteError> {
    let directory = root.join("operations");
    create_private_dir(&directory)?;
    let mut operations = Vec::new();
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            if let Some(operation) = read_optional_json(&path)? {
                operations.push(refresh_operation(root, operation)?);
            }
        }
    }
    operations.sort_by_key(|operation| operation.created_unix_ms);
    Ok(operations)
}
pub(super) fn load_resources(root: &Path) -> Result<Vec<SpriteRecord>, SpriteError> {
    let entries = match fs::read_dir(root.join("resources")) {
        Ok(entries) => entries,
        Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(cause) => return Err(io_error(cause)),
    };
    let mut records = Vec::new();
    for entry in entries {
        if let Some(record) =
            read_optional_json(&entry.map_err(io_error)?.path().join("record.json"))?
        {
            records.push(record);
        }
    }
    records.sort_by(|left: &SpriteRecord, right| left.id.cmp(&right.id));
    Ok(records)
}
fn lease_path(root: &Path, id: &str) -> PathBuf {
    root.join("workers").join(format!("{}.json", hash_id(id)))
}
pub(super) fn persist_lease(root: &Path, id: &str) -> Result<(), SpriteError> {
    let lease = serde_json::json!({"owner_pid": std::process::id(), "operation_id": id, "started_unix_ms": unix_ms()});
    atomic_write(&lease_path(root, id), lease.to_string().as_bytes())
}
pub(super) fn remove_lease(root: &Path, id: &str) {
    let _ = fs::remove_file(lease_path(root, id));
}
pub(super) fn has_live_owner(root: &Path, id: &str) -> Result<bool, SpriteError> {
    let Some(lease): Option<serde_json::Value> = read_optional_json(&lease_path(root, id))? else {
        return Ok(false);
    };
    let pid = lease
        .get("owner_pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    Ok(pid.is_some_and(crate::platform::process_exists))
}
pub(super) fn needs_presentation(operation: &SpriteOperation) -> bool {
    operation.status == SpriteOperationStatus::Succeeded
        && operation.request.open_in_workspace.is_some()
        && !matches!(
            operation.stage.as_str(),
            "session_started" | "transport_open" | "presentation_succeeded"
        )
        && (matches!(operation.result, Some(SpriteResult::Connection(_)))
            || (matches!(operation.result, Some(SpriteResult::Resource(_)))
                && matches!(
                    operation.request.command,
                    gardn_local_api::sprites::SpriteCommand::Create(_)
                )))
}
pub(super) fn refresh_operation(
    root: &Path,
    mut operation: SpriteOperation,
) -> Result<SpriteOperation, SpriteError> {
    let pending_presentation = needs_presentation(&operation);
    let unfinished = !is_terminal(&operation.status) || pending_presentation;
    let live_owner = unfinished && has_live_owner(root, &operation.id)?;
    if pending_presentation && live_owner {
        operation.status = SpriteOperationStatus::Running;
        operation.stage = "awaiting_presentation".into();
    }
    if unfinished && !live_owner {
        operation.status = SpriteOperationStatus::Interrupted;
        operation.stage = "interrupted".into();
        operation.updated_unix_ms = unix_ms();
        operation.error = Some(error(
            "interrupted",
            "The owning Gardn coordinator is no longer active; retry explicitly to reconcile",
            true,
        ));
        persist_operation(root, &operation)?;
        remove_lease(root, &operation.id);
    }
    Ok(operation)
}
pub(super) fn write_backend_fence(root: &Path, config: &SpritesConfig) -> Result<(), SpriteError> {
    let value = serde_json::json!({"enabled": config.enabled, "org": config.org, "sprite_bin": config.sprite_bin});
    atomic_write(
        &root.join("backend-config.json"),
        value.to_string().as_bytes(),
    )
}
