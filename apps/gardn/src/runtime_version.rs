use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Current,
    ServerRestartRequired,
    ClientUpdateRequired,
    VersionSkew,
    ProtocolIncompatible,
    Unknown,
    ServerNotRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAction {
    None,
    RestartServer,
    UpdateClient,
    InspectStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVersion {
    pub state: RuntimeState,
    pub action: RuntimeAction,
    pub client_version: String,
    pub client_protocol: u32,
    pub server_version: Option<String>,
    pub server_protocol: Option<u32>,
    pub live_handoff: Option<bool>,
}

pub(crate) const ENV_VAR: &str = "GARDN_RUNTIME_VERSION";

impl RuntimeVersion {
    pub fn classify(
        client_version: &str,
        client_protocol: u32,
        server: Option<&crate::api::RuntimeStatus>,
    ) -> Self {
        let server_version = server.and_then(|server| server.version.as_deref());
        let server_protocol = server.and_then(|server| server.protocol);
        let state = match server {
            None => RuntimeState::ServerNotRunning,
            Some(_) => match server_protocol {
                None => RuntimeState::Unknown,
                Some(protocol) if protocol != client_protocol => RuntimeState::ProtocolIncompatible,
                Some(_) => match server_version {
                    None => RuntimeState::Unknown,
                    Some(version) if version == client_version => RuntimeState::Current,
                    Some(version) => match (
                        crate::update::Version::parse(client_version),
                        crate::update::Version::parse(version),
                    ) {
                        (Some(client), Some(server)) => match client.cmp(&server) {
                            Ordering::Greater => RuntimeState::ServerRestartRequired,
                            Ordering::Less => RuntimeState::ClientUpdateRequired,
                            Ordering::Equal => RuntimeState::VersionSkew,
                        },
                        _ => RuntimeState::VersionSkew,
                    },
                },
            },
        };
        let action = match state {
            RuntimeState::Current | RuntimeState::ServerNotRunning => RuntimeAction::None,
            RuntimeState::ServerRestartRequired => RuntimeAction::RestartServer,
            RuntimeState::ClientUpdateRequired => RuntimeAction::UpdateClient,
            RuntimeState::VersionSkew
            | RuntimeState::ProtocolIncompatible
            | RuntimeState::Unknown => RuntimeAction::InspectStatus,
        };
        Self {
            state,
            action,
            client_version: client_version.to_owned(),
            client_protocol,
            server_version: server_version.map(str::to_owned),
            server_protocol,
            live_handoff: server.and_then(|server| {
                server
                    .capabilities
                    .as_ref()
                    .map(|capabilities| capabilities.live_handoff)
            }),
        }
    }

    pub fn attachment_allowed(&self) -> bool {
        self.server_protocol == Some(self.client_protocol)
    }

    pub fn message(&self) -> Option<String> {
        let server = self.server_version.as_deref().unwrap_or("unknown");
        let client = &self.client_version;
        match self.state {
            RuntimeState::Current | RuntimeState::ServerNotRunning => None,
            RuntimeState::ServerRestartRequired => Some(format!(
                "Server v{server} is still running. Restart it to use v{client}."
            )),
            RuntimeState::ClientUpdateRequired => Some(format!(
                "Server v{server} is newer than client v{client}. Update the client."
            )),
            RuntimeState::VersionSkew => Some(format!(
                "Server v{server} and client v{client} differ. Inspect `gardn status`."
            )),
            RuntimeState::ProtocolIncompatible => Some(format!(
                "Server protocol {} is incompatible with client protocol {}. Inspect `gardn status` before attaching.",
                self.server_protocol
                    .map_or_else(|| "unknown".to_owned(), |protocol| protocol.to_string()),
                self.client_protocol
            )),
            RuntimeState::Unknown => Some(
                "Server runtime metadata is incomplete. Inspect `gardn status`.".to_owned()
            ),
        }
    }

    pub fn title_suffix(&self) -> Option<&'static str> {
        if !self.attachment_allowed() {
            return None;
        }
        match self.action {
            RuntimeAction::None => None,
            RuntimeAction::RestartServer => Some("Restart server"),
            RuntimeAction::UpdateClient => Some("Update client"),
            RuntimeAction::InspectStatus => Some("Inspect status"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_stale_server_can_attach_and_requests_restart() {
        let server = crate::api::RuntimeStatus {
            version: Some("0.10.12".into()),
            protocol: Some(13),
            capabilities: None,
        };
        let runtime = RuntimeVersion::classify("0.10.16", 13, Some(&server));
        assert!(runtime.attachment_allowed());
        assert_eq!(runtime.state, RuntimeState::ServerRestartRequired);
        assert_eq!(runtime.action, RuntimeAction::RestartServer);
        assert_eq!(
            runtime.message().as_deref(),
            Some("Server v0.10.12 is still running. Restart it to use v0.10.16.")
        );
        assert_eq!(runtime.title_suffix(), Some("Restart server"));
    }

    #[test]
    fn unknown_and_unequal_protocols_block_attachment_even_with_matching_versions() {
        for (protocol, expected) in [
            (None, RuntimeState::Unknown),
            (Some(12), RuntimeState::ProtocolIncompatible),
        ] {
            let server = crate::api::RuntimeStatus {
                version: Some("0.10.16".into()),
                protocol,
                capabilities: None,
            };
            let runtime = RuntimeVersion::classify("0.10.16", 13, Some(&server));
            assert_eq!(runtime.state, expected);
            assert!(!runtime.attachment_allowed());
        }
    }

    #[test]
    fn newer_server_requests_client_update() {
        let server = crate::api::RuntimeStatus {
            version: Some("0.10.17".into()),
            protocol: Some(13),
            capabilities: None,
        };
        let runtime = RuntimeVersion::classify("0.10.16", 13, Some(&server));
        assert_eq!(runtime.action, RuntimeAction::UpdateClient);
        assert!(runtime.attachment_allowed());
        assert_eq!(runtime.title_suffix(), Some("Update client"));
    }

    #[test]
    fn equivalent_release_spellings_are_version_skew() {
        let server = crate::api::RuntimeStatus {
            version: Some("v0.10.16".into()),
            protocol: Some(13),
            capabilities: None,
        };
        let runtime = RuntimeVersion::classify("0.10.16", 13, Some(&server));
        assert_eq!(runtime.state, RuntimeState::VersionSkew);
        assert_eq!(runtime.action, RuntimeAction::InspectStatus);
        assert!(runtime.attachment_allowed());
    }

    #[test]
    fn malformed_versions_are_version_skew() {
        let server = crate::api::RuntimeStatus {
            version: Some("dev-server".into()),
            protocol: Some(13),
            capabilities: None,
        };
        let runtime = RuntimeVersion::classify("dev-client", 13, Some(&server));
        assert_eq!(runtime.state, RuntimeState::VersionSkew);
        assert_eq!(runtime.action, RuntimeAction::InspectStatus);
        assert!(runtime.attachment_allowed());
    }

    #[test]
    fn current_runtime_is_silent() {
        let server = crate::api::RuntimeStatus {
            version: Some("0.10.16".into()),
            protocol: Some(13),
            capabilities: None,
        };
        let runtime = RuntimeVersion::classify("0.10.16", 13, Some(&server));
        assert_eq!(runtime.state, RuntimeState::Current);
        assert_eq!(runtime.message(), None);
        assert_eq!(runtime.title_suffix(), None);
    }

    #[test]
    fn absent_server_has_no_action_and_null_metadata() {
        let runtime = RuntimeVersion::classify("0.10.16", 13, None);
        assert_eq!(
            serde_json::to_value(runtime).unwrap(),
            serde_json::json!({
                "state": "server_not_running", "action": "none", "client_version": "0.10.16",
                "client_protocol": 13, "server_version": null, "server_protocol": null, "live_handoff": null
            })
        );
    }
}
