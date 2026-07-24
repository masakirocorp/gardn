use std::{fmt, path::PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

pub(crate) const LOCAL_EXECUTION_HOST_ID: &str = "local";
const MAX_EXECUTION_HOST_ID_LEN: usize = 128;

/// Stable coordinator-owned identity for one logical execution environment.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct ExecutionHostId(String);

impl ExecutionHostId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, ExecutionHostIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ExecutionHostIdError::Empty);
        }
        if value.len() > MAX_EXECUTION_HOST_ID_LEN {
            return Err(ExecutionHostIdError::TooLong);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        {
            return Err(ExecutionHostIdError::InvalidCharacter);
        }
        Ok(Self(value))
    }

    pub(crate) fn local() -> Self {
        Self(LOCAL_EXECUTION_HOST_ID.to_string())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_local(&self) -> bool {
        self.0 == LOCAL_EXECUTION_HOST_ID
    }
}

impl Default for ExecutionHostId {
    fn default() -> Self {
        Self::local()
    }
}

impl fmt::Display for ExecutionHostId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ExecutionHostId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionHostIdError {
    Empty,
    TooLong,
    InvalidCharacter,
}

impl fmt::Display for ExecutionHostIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "execution host id must not be empty",
            Self::TooLong => "execution host id is too long",
            Self::InvalidCharacter => "execution host id contains an invalid character",
        })
    }
}

/// A path interpreted only by its execution host.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub(crate) struct HostPath(PathBuf);

impl HostPath {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Result<Self, HostPathError> {
        let path = path.into();
        if path.as_os_str().is_empty() {
            return Err(HostPathError);
        }
        Ok(Self(path))
    }

    pub(crate) fn as_path(&self) -> &std::path::Path {
        &self.0
    }

    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl fmt::Display for HostPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(formatter)
    }
}

impl<'de> Deserialize<'de> for HostPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = PathBuf::deserialize(deserializer)?;
        Self::new(path).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostPathError;

impl fmt::Display for HostPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("host path must not be empty")
    }
}

/// An atomic execution host and host-interpreted path pair.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceLocation {
    pub(crate) execution_host_id: ExecutionHostId,
    pub(crate) path: HostPath,
}

impl ResourceLocation {
    pub(crate) fn new(execution_host_id: ExecutionHostId, path: HostPath) -> Self {
        Self {
            execution_host_id,
            path,
        }
    }

    pub(crate) fn local(path: impl Into<PathBuf>) -> Result<Self, HostPathError> {
        Ok(Self::new(ExecutionHostId::local(), HostPath::new(path)?))
    }

    pub(crate) fn is_local(&self) -> bool {
        self.execution_host_id.is_local()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_location_serializes_host_and_path_atomically() {
        let location = ResourceLocation::new(
            ExecutionHostId::new("ssh:workbox").unwrap(),
            HostPath::new("/srv/api").unwrap(),
        );

        let encoded = serde_json::to_string(&location).unwrap();

        assert_eq!(
            encoded,
            r#"{"execution_host_id":"ssh:workbox","path":"/srv/api"}"#
        );
        assert_eq!(
            serde_json::from_str::<ResourceLocation>(&encoded).unwrap(),
            location
        );
    }

    #[test]
    fn execution_host_id_rejects_ambiguous_values() {
        assert_eq!(ExecutionHostId::new(""), Err(ExecutionHostIdError::Empty));
        assert_eq!(
            ExecutionHostId::new("work box"),
            Err(ExecutionHostIdError::InvalidCharacter)
        );
        assert!(ExecutionHostId::new("ssh:workbox").is_ok());
    }

    #[test]
    fn local_location_is_explicit() {
        let location = ResourceLocation::local("/tmp/project").unwrap();

        assert!(location.is_local());
        assert_eq!(location.execution_host_id.as_str(), LOCAL_EXECUTION_HOST_ID);
        assert_eq!(
            location.path.as_path(),
            std::path::Path::new("/tmp/project")
        );
    }
}
