use std::fmt;

use crate::execution_host::ExecutionHostId;

const FALLBACK_COORDINATOR_NAME: &str = "this coordinator";
const RESERVED_LOCAL_NAME: &str = "Local";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostDisplayName(String);

impl HostDisplayName {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, HostDisplayNameError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(HostDisplayNameError::Empty);
        }
        if trimmed.chars().any(char::is_control) {
            return Err(HostDisplayNameError::ContainsControlCharacter);
        }
        if trimmed.eq_ignore_ascii_case(RESERVED_LOCAL_NAME) {
            return Err(HostDisplayNameError::ReservedLocal);
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostDisplayName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
impl AsRef<str> for HostDisplayName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostDisplayNameError {
    Empty,
    ContainsControlCharacter,
    ReservedLocal,
    SshProfileNameCollision(String),
}

impl fmt::Display for HostDisplayNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("host display name must not be empty"),
            Self::ContainsControlCharacter => {
                formatter.write_str("host display name must not contain control characters")
            }
            Self::ReservedLocal => formatter.write_str("host display name must not be Local"),
            Self::SshProfileNameCollision(name) => write!(
                formatter,
                "host display name conflicts with SSH connection profile name {name:?}"
            ),
        }
    }
}

impl std::error::Error for HostDisplayNameError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostLabelTarget<'a> {
    Coordinator,
    ExecutionHost(&'a ExecutionHostId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HostLabel<'a>(&'a str);

impl<'a> HostLabel<'a> {
    pub(crate) fn new(value: &'a str) -> Self {
        Self(value)
    }

    pub(crate) fn as_str(&self) -> &'a str {
        self.0
    }
}

impl fmt::Display for HostLabel<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for HostLabel<'_> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostDisplayNameOverlay {
    coordinator: HostDisplayName,
}

impl HostDisplayNameOverlay {
    pub(crate) fn from_config_or_hostname(configured: &str, hostname: Option<&str>) -> Self {
        let coordinator = HostDisplayName::new(configured)
            .or_else(|_| hostname.map_or(Err(HostDisplayNameError::Empty), HostDisplayName::new))
            .unwrap_or_else(|_| HostDisplayName(FALLBACK_COORDINATOR_NAME.to_owned()));
        Self { coordinator }
    }

    pub(crate) fn from_config_or_hostname_with_profile_names<'a, I>(
        configured: &str,
        hostname: Option<&str>,
        profile_names: I,
    ) -> (Self, Option<HostDisplayNameError>)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let configured_name = HostDisplayName::new(configured);
        let collision = configured_name.as_ref().ok().and_then(|name| {
            profile_names
                .into_iter()
                .find(|profile_name| names_case_fold_equal(name.as_str(), profile_name))
                .map(|profile_name| {
                    HostDisplayNameError::SshProfileNameCollision(profile_name.to_owned())
                })
        });
        if let Some(error) = collision {
            return (Self::from_config_or_hostname("", hostname), Some(error));
        }

        let error = if configured.trim().is_empty() {
            None
        } else {
            configured_name.as_ref().err().cloned()
        };
        (Self::from_config_or_hostname(configured, hostname), error)
    }

    pub(crate) fn coordinator(&self) -> HostLabel<'_> {
        HostLabel(self.coordinator.as_str())
    }
}

fn names_case_fold_equal(left: &str, right: &str) -> bool {
    left.chars()
        .flat_map(char::to_lowercase)
        .eq(right.chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_name_wins_over_hostname() {
        let overlay = HostDisplayNameOverlay::from_config_or_hostname("desktop", Some("host"));
        assert_eq!(overlay.coordinator().as_str(), "desktop");
    }

    #[test]
    fn blank_config_uses_hostname() {
        let overlay = HostDisplayNameOverlay::from_config_or_hostname("", Some("host"));
        assert_eq!(overlay.coordinator().as_str(), "host");
    }

    #[test]
    fn missing_names_use_coordinator_fallback() {
        let overlay = HostDisplayNameOverlay::from_config_or_hostname("", None);
        assert_eq!(overlay.coordinator().as_str(), FALLBACK_COORDINATOR_NAME);
    }
    #[test]
    fn reserved_local_name_uses_hostname_or_coordinator_fallback() {
        let overlay = HostDisplayNameOverlay::from_config_or_hostname("Local", Some("desktop"));
        assert_ne!(overlay.coordinator().as_str(), "Local");
        assert_eq!(overlay.coordinator().as_str(), "desktop");
    }

    #[test]
    fn configured_name_collision_falls_back_to_hostname() {
        let (overlay, error) = HostDisplayNameOverlay::from_config_or_hostname_with_profile_names(
            "Build",
            Some("desktop"),
            ["build"].into_iter(),
        );
        assert_eq!(overlay.coordinator().as_str(), "desktop");
        assert!(matches!(
            error,
            Some(HostDisplayNameError::SshProfileNameCollision(name)) if name == "build"
        ));
    }

    #[test]
    fn hostname_collision_is_allowed() {
        let (overlay, error) = HostDisplayNameOverlay::from_config_or_hostname_with_profile_names(
            "",
            Some("Build"),
            ["build"].into_iter(),
        );
        assert_eq!(overlay.coordinator().as_str(), "Build");
        assert!(error.is_none());
    }
}
