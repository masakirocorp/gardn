//! Coordinator-owned SSH Connection Profile catalog.
//!
//! Persisted globally at `~/.config/omh/ssh-profiles.json`. Profiles hold only
//! OpenSSH destination metadata — never credentials, private keys, or
//! passphrases. System OpenSSH owns authentication.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tracing::warn;

use crate::execution_host::{ExecutionHostId, HostPath};

const CATALOG_FILE: &str = "ssh-profiles.json";
const CATALOG_LOCK_FILE: &str = ".ssh-profiles.lock";

fn catalog_path() -> PathBuf {
    crate::config::config_dir().join(CATALOG_FILE)
}

fn catalog_lock_path() -> PathBuf {
    crate::config::config_dir().join(CATALOG_LOCK_FILE)
}

/// Validation failure for an SSH connection profile field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SshConnectionProfileError {
    EmptyId,
    EmptyName,
    EmptyTarget,
    NulInId,
    NulInName,
    NulInTarget,
    InvalidIdCharacter,
    ZeroHostBindingGeneration,
    HostBindingGenerationOverflow,
}

impl std::fmt::Display for SshConnectionProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyId => "ssh connection profile id must not be empty",
            Self::EmptyName => "ssh connection profile name must not be empty",
            Self::EmptyTarget => "ssh connection profile target must not be empty",
            Self::NulInId => "ssh connection profile id must not contain NUL",
            Self::NulInName => "ssh connection profile name must not contain NUL",
            Self::NulInTarget => "ssh connection profile target must not contain NUL",
            Self::InvalidIdCharacter => "ssh connection profile id contains an invalid character",
            Self::ZeroHostBindingGeneration => {
                "ssh connection profile host-binding generation must be nonzero"
            }
            Self::HostBindingGenerationOverflow => {
                "ssh connection profile host-binding generation overflowed"
            }
        })
    }
}

impl std::error::Error for SshConnectionProfileError {}

/// Coordinator-owned SSH connection configuration.
///
/// Identity (`id`) is stable across rename and suggested-directory edits.
/// Changing the raw OpenSSH target increments `host_binding_generation` so
/// derived [`ExecutionHostId`] values never silently reinterpret old placements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SshConnectionProfile {
    id: String,
    name: String,
    /// Raw OpenSSH destination preserved for `ssh` argv (outer whitespace trimmed only).
    target: String,
    suggested_directory: Option<HostPath>,
    /// Nonzero generation of the authenticated target binding behind this profile.
    host_binding_generation: u64,
}

impl SshConnectionProfile {
    /// Construct a new profile with host-binding generation `1`.
    pub(crate) fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        target: impl Into<String>,
        suggested_directory: Option<HostPath>,
    ) -> Result<Self, SshConnectionProfileError> {
        Self::from_parts(id, name, target, suggested_directory, 1)
    }

    fn from_parts(
        id: impl Into<String>,
        name: impl Into<String>,
        target: impl Into<String>,
        suggested_directory: Option<HostPath>,
        host_binding_generation: u64,
    ) -> Result<Self, SshConnectionProfileError> {
        let id = validate_id(id.into())?;
        let name = validate_name(name.into())?;
        let target = validate_target(target.into())?;
        if host_binding_generation == 0 {
            return Err(SshConnectionProfileError::ZeroHostBindingGeneration);
        }
        Ok(Self {
            id,
            name,
            target,
            suggested_directory,
            host_binding_generation,
        })
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn suggested_directory(&self) -> Option<&HostPath> {
        self.suggested_directory.as_ref()
    }

    pub(crate) fn host_binding_generation(&self) -> u64 {
        self.host_binding_generation
    }

    /// Deterministic Execution Host identity for the current host binding.
    ///
    /// Format: `ssh:{profile_id}:{generation}`. Rename and suggested-directory
    /// edits preserve this value; target changes bump the generation.
    pub(crate) fn execution_host_id(&self) -> ExecutionHostId {
        ExecutionHostId::new(format!("ssh:{}:{}", self.id, self.host_binding_generation))
            .expect("validated profile id yields a valid execution host id")
    }

    /// Update the user-visible name without changing host binding identity.
    pub(crate) fn rename(
        &mut self,
        name: impl Into<String>,
    ) -> Result<(), SshConnectionProfileError> {
        self.name = validate_name(name.into())?;
        Ok(())
    }

    /// Update the optional suggested directory without changing host binding identity.
    pub(crate) fn set_suggested_directory(&mut self, suggested_directory: Option<HostPath>) {
        self.suggested_directory = suggested_directory;
    }

    /// Replace the raw OpenSSH target and increment the host-binding generation.
    ///
    /// Outer whitespace is trimmed; interior syntax is preserved for `ssh`.
    pub(crate) fn set_target(
        &mut self,
        target: impl Into<String>,
    ) -> Result<(), SshConnectionProfileError> {
        let target = validate_target(target.into())?;
        if target == self.target {
            return Ok(());
        }
        let next_generation = self
            .host_binding_generation
            .checked_add(1)
            .ok_or(SshConnectionProfileError::HostBindingGenerationOverflow)?;
        self.target = target;
        self.host_binding_generation = next_generation;
        Ok(())
    }
}

fn validate_id(id: String) -> Result<String, SshConnectionProfileError> {
    if id.is_empty() {
        return Err(SshConnectionProfileError::EmptyId);
    }
    if id.contains('\0') {
        return Err(SshConnectionProfileError::NulInId);
    }
    // Keep the profile segment unambiguous in `ssh:{profile_id}:{generation}`.
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(SshConnectionProfileError::InvalidIdCharacter);
    }
    // Ensure the derived host id stays within ExecutionHostId limits.
    ExecutionHostId::new(format!("ssh:{id}:1")).map_err(|err| match err {
        crate::execution_host::ExecutionHostIdError::Empty => SshConnectionProfileError::EmptyId,
        crate::execution_host::ExecutionHostIdError::TooLong
        | crate::execution_host::ExecutionHostIdError::InvalidCharacter => {
            SshConnectionProfileError::InvalidIdCharacter
        }
    })?;
    Ok(id)
}

fn validate_name(name: String) -> Result<String, SshConnectionProfileError> {
    if name.is_empty() {
        return Err(SshConnectionProfileError::EmptyName);
    }
    if name.contains('\0') {
        return Err(SshConnectionProfileError::NulInName);
    }
    Ok(name)
}

fn validate_target(target: String) -> Result<String, SshConnectionProfileError> {
    // Trim outer whitespace only — do not parse or normalize OpenSSH syntax.
    let target = target.trim().to_string();
    if target.is_empty() {
        return Err(SshConnectionProfileError::EmptyTarget);
    }
    if target.contains('\0') {
        return Err(SshConnectionProfileError::NulInTarget);
    }
    Ok(target)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SshConnectionProfileSerde {
    id: String,
    name: String,
    target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    suggested_directory: Option<HostPath>,
    host_binding_generation: u64,
}

impl Serialize for SshConnectionProfile {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SshConnectionProfileSerde {
            id: self.id.clone(),
            name: self.name.clone(),
            target: self.target.clone(),
            suggested_directory: self.suggested_directory.clone(),
            host_binding_generation: self.host_binding_generation,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SshConnectionProfile {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = SshConnectionProfileSerde::deserialize(deserializer)?;
        Self::from_parts(
            raw.id,
            raw.name,
            raw.target,
            raw.suggested_directory,
            raw.host_binding_generation,
        )
        .map_err(serde::de::Error::custom)
    }
}

fn with_catalog_lock<T>(operation: impl FnOnce() -> std::io::Result<T>) -> std::io::Result<T> {
    let lock_path = catalog_lock_path();
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

fn save_json_to_path<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    #[cfg(windows)]
    if path.exists() {
        if let Err(err) = std::fs::remove_file(path) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(err);
        }
    }
    if let Err(err) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

/// Atomically write the profile catalog to `path` (test and internal helper).
pub(crate) fn save_to_path(path: &Path, profiles: &[SshConnectionProfile]) -> std::io::Result<()> {
    ensure_unique_ids(profiles)?;
    let mut ordered = profiles.to_vec();
    sort_profiles(&mut ordered);
    save_json_to_path(path, &ordered)
}

fn sort_profiles(profiles: &mut [SshConnectionProfile]) {
    profiles.sort_by(|left, right| left.id.cmp(&right.id));
}

fn ensure_unique_ids(profiles: &[SshConnectionProfile]) -> std::io::Result<()> {
    let mut seen = HashSet::with_capacity(profiles.len());
    for profile in profiles {
        if !seen.insert(profile.id.clone()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("duplicate ssh connection profile id: {}", profile.id),
            ));
        }
    }
    Ok(())
}

fn load_from_path_strict(path: &Path) -> std::io::Result<Vec<SshConnectionProfile>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path)?;
    let profiles = serde_json::from_str::<Vec<SshConnectionProfile>>(&content)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    ensure_unique_ids(&profiles)?;
    let mut profiles = profiles;
    sort_profiles(&mut profiles);
    Ok(profiles)
}

/// Load from an explicit path. Missing or corrupt files yield an empty catalog.
#[cfg(test)]
pub(crate) fn load_from_path(path: &Path) -> Vec<SshConnectionProfile> {
    match load_from_path_strict(path) {
        Ok(profiles) => profiles,
        Err(err) => {
            warn!(
                path = %path.display(),
                err = %err,
                "failed to read ssh connection profile catalog"
            );
            Vec::new()
        }
    }
}

/// Load the global catalog under the catalog lock.
pub(crate) fn try_load() -> std::io::Result<Vec<SshConnectionProfile>> {
    with_catalog_lock(|| load_from_path_strict(&catalog_path()))
}

/// Load the global catalog. Returns empty on failure so corrupt/missing files
/// never block server startup; mutations still use strict reads.
pub(crate) fn load() -> Vec<SshConnectionProfile> {
    match try_load() {
        Ok(profiles) => profiles,
        Err(err) => {
            warn!(
                path = %catalog_path().display(),
                err = %err,
                "failed to load ssh connection profile catalog"
            );
            Vec::new()
        }
    }
}

/// Mutate the global catalog under an exclusive lock, then write atomically.
pub(crate) fn update<T>(
    mutation: impl FnOnce(&mut Vec<SshConnectionProfile>) -> T,
) -> std::io::Result<(T, Vec<SshConnectionProfile>)> {
    with_catalog_lock(|| {
        let mut profiles = load_from_path_strict(&catalog_path())?;
        let result = mutation(&mut profiles);
        ensure_unique_ids(&profiles)?;
        sort_profiles(&mut profiles);
        save_to_path(&catalog_path(), &profiles)?;
        Ok((result, profiles))
    })
}

/// Insert or replace a profile by stable id.
pub(crate) fn upsert(profile: SshConnectionProfile) -> std::io::Result<Vec<SshConnectionProfile>> {
    let (_, profiles) = update(|profiles| {
        if let Some(existing) = profiles.iter_mut().find(|entry| entry.id == profile.id) {
            *existing = profile;
        } else {
            profiles.push(profile);
        }
    })?;
    Ok(profiles)
}

/// Remove a profile by stable id. Returns whether a profile was removed.
pub(crate) fn remove(id: &str) -> std::io::Result<(bool, Vec<SshConnectionProfile>)> {
    update(|profiles| {
        let before = profiles.len();
        profiles.retain(|profile| profile.id != id);
        before != profiles.len()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_catalog_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "omh-ssh-profiles-{name}-{}-{nanos}",
                std::process::id()
            ))
            .join(CATALOG_FILE)
    }

    fn sample_profile(id: &str, target: &str) -> SshConnectionProfile {
        SshConnectionProfile::new(id, format!("Name {id}"), target, None).unwrap()
    }

    #[test]
    fn save_and_load_roundtrip() {
        let path = temp_catalog_path("roundtrip");
        let profiles = vec![
            sample_profile("workbox", "workbox"),
            sample_profile("lab", "user@lab.example.com"),
        ];

        save_to_path(&path, &profiles).unwrap();
        let loaded = load_from_path(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id(), "lab");
        assert_eq!(loaded[1].id(), "workbox");
        assert_eq!(loaded[1].target(), "workbox");
        assert_eq!(loaded[0].target(), "user@lab.example.com");
        assert_eq!(loaded[0].host_binding_generation(), 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_file_returns_empty() {
        let path = temp_catalog_path("missing");
        assert!(load_from_path(&path).is_empty());
        assert!(load_from_path_strict(&path).unwrap().is_empty());
    }

    #[test]
    fn corrupt_file_returns_empty_without_panic() {
        let path = temp_catalog_path("corrupt");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, b"this is not valid json {{{{").unwrap();
        assert!(load_from_path(&path).is_empty());
        assert!(load_from_path_strict(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let path = temp_catalog_path("duplicates");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            &path,
            r#"[
              {"id":"workbox","name":"A","target":"a","host_binding_generation":1},
              {"id":"workbox","name":"B","target":"b","host_binding_generation":1}
            ]"#,
        )
        .unwrap();
        let err = load_from_path_strict(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(load_from_path(&path).is_empty());

        let dup = vec![sample_profile("same", "a"), sample_profile("same", "b")];
        let err = save_to_path(&path, &dup).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rename_and_suggested_directory_preserve_host_id() {
        let mut profile = SshConnectionProfile::new(
            "workbox",
            "Workbox",
            "workbox",
            Some(HostPath::new("/srv").unwrap()),
        )
        .unwrap();
        let host_id = profile.execution_host_id();
        let generation = profile.host_binding_generation();

        profile.rename("Work box").unwrap();
        profile.set_suggested_directory(Some(HostPath::new("~/projects").unwrap()));
        assert_eq!(profile.name(), "Work box");
        assert_eq!(
            profile.suggested_directory().map(HostPath::as_path),
            Some(std::path::Path::new("~/projects"))
        );
        assert_eq!(profile.host_binding_generation(), generation);
        assert_eq!(profile.execution_host_id(), host_id);
    }

    #[test]
    fn target_change_increments_generation_and_host_id() {
        let mut profile = sample_profile("workbox", "workbox");
        let before = profile.execution_host_id();
        assert_eq!(before.as_str(), "ssh:workbox:1");

        profile.set_target("other-host").unwrap();
        assert_eq!(profile.target(), "other-host");
        assert_eq!(profile.host_binding_generation(), 2);
        let after = profile.execution_host_id();
        assert_eq!(after.as_str(), "ssh:workbox:2");
        assert_ne!(before, after);

        // Identical target is a no-op.
        profile.set_target("other-host").unwrap();
        assert_eq!(profile.host_binding_generation(), 2);

        let mut exhausted =
            SshConnectionProfile::from_parts("full", "Full", "old", None, u64::MAX).unwrap();
        assert_eq!(
            exhausted.set_target("new"),
            Err(SshConnectionProfileError::HostBindingGenerationOverflow)
        );
        assert_eq!(exhausted.target(), "old");
    }

    #[test]
    fn target_string_preserves_interior_syntax_after_outer_trim() {
        let profile =
            SshConnectionProfile::new("jump", "Jump", "  user@host -J bastion  ", None).unwrap();
        assert_eq!(profile.target(), "user@host -J bastion");

        let path = temp_catalog_path("target-preserve");
        save_to_path(&path, &[profile]).unwrap();
        let loaded = load_from_path_strict(&path).unwrap();
        assert_eq!(loaded[0].target(), "user@host -J bastion");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_empty_and_nul_fields() {
        assert_eq!(
            SshConnectionProfile::new("", "n", "t", None).unwrap_err(),
            SshConnectionProfileError::EmptyId
        );
        assert_eq!(
            SshConnectionProfile::new("id", "", "t", None).unwrap_err(),
            SshConnectionProfileError::EmptyName
        );
        assert_eq!(
            SshConnectionProfile::new("id", "n", "   ", None).unwrap_err(),
            SshConnectionProfileError::EmptyTarget
        );
        assert_eq!(
            SshConnectionProfile::new("id\0x", "n", "t", None).unwrap_err(),
            SshConnectionProfileError::NulInId
        );
        assert_eq!(
            SshConnectionProfile::new("id", "n\0", "t", None).unwrap_err(),
            SshConnectionProfileError::NulInName
        );
        assert_eq!(
            SshConnectionProfile::new("id", "n", "t\0", None).unwrap_err(),
            SshConnectionProfileError::NulInTarget
        );
        assert_eq!(
            SshConnectionProfile::new("ambiguous:id", "n", "t", None).unwrap_err(),
            SshConnectionProfileError::InvalidIdCharacter
        );
        assert_eq!(
            SshConnectionProfile::from_parts("id", "n", "t", None, 0).unwrap_err(),
            SshConnectionProfileError::ZeroHostBindingGeneration
        );
    }

    #[test]
    fn deserialize_rejects_invalid_profiles() {
        let err = serde_json::from_str::<SshConnectionProfile>(
            r#"{"id":"","name":"n","target":"t","host_binding_generation":1}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("empty"));

        let err = serde_json::from_str::<SshConnectionProfile>(
            r#"{"id":"id","name":"n","target":"t","host_binding_generation":0}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("nonzero"));
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let path = temp_catalog_path("cleanup");
        save_to_path(&path, &[sample_profile("a", "a")]).unwrap();
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists());
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn catalog_has_no_credential_fields() {
        let profile = sample_profile("workbox", "workbox");
        let json = serde_json::to_value(&profile).unwrap();
        let obj = json.as_object().unwrap();
        for banned in [
            "password",
            "passphrase",
            "private_key",
            "privateKey",
            "identity_file",
            "credential",
            "secret",
        ] {
            assert!(!obj.contains_key(banned), "unexpected field {banned}");
        }
        assert!(obj.contains_key("id"));
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("target"));
        assert!(obj.contains_key("host_binding_generation"));
    }
}
