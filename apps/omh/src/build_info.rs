//! Build provenance and runtime compatibility identity.

use serde::{Deserialize, Serialize};

pub const BASE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_CHANNEL: &str = env!("OMH_BUILD_CHANNEL_EMBEDDED");
pub const BUILD_COHORT: &str = env!("OMH_BUILD_COHORT_EMBEDDED");
pub const BUILD_TARGET: &str = env!("OMH_BUILD_TARGET_EMBEDDED");
pub const RELEASE_TAG: &str = env!("OMH_RELEASE_TAG_EMBEDDED");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkerBuildIdentity {
    pub(crate) app_version: String,
    pub(crate) build_channel: String,
    pub(crate) build_cohort: String,
    pub(crate) target: String,
    pub(crate) platform: String,
    pub(crate) client_protocol: u32,
    pub(crate) worker_protocol: u32,
    pub(crate) daemon_lifecycle_version: u16,
    pub(crate) capabilities: Vec<String>,
}

pub fn version() -> String {
    BASE_VERSION.to_string()
}

pub(crate) fn is_official_release() -> bool {
    BUILD_CHANNEL == "release"
}

pub(crate) fn platform_for_target(target: &str) -> Option<&'static str> {
    match target {
        "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl" => Some("linux-x86_64"),
        "aarch64-unknown-linux-gnu" | "aarch64-unknown-linux-musl" => Some("linux-aarch64"),
        "x86_64-apple-darwin" => Some("macos-x86_64"),
        "aarch64-apple-darwin" => Some("macos-aarch64"),
        "x86_64-pc-windows-msvc" => Some("windows-x86_64"),
        _ => None,
    }
}

pub(crate) fn worker_identity() -> WorkerBuildIdentity {
    WorkerBuildIdentity {
        app_version: BASE_VERSION.to_string(),
        build_channel: BUILD_CHANNEL.to_string(),
        build_cohort: BUILD_COHORT.to_string(),
        target: BUILD_TARGET.to_string(),
        platform: platform_for_target(BUILD_TARGET)
            .unwrap_or("unknown")
            .to_string(),
        client_protocol: crate::protocol::PROTOCOL_VERSION,
        worker_protocol: crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION,
        daemon_lifecycle_version: crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION,
        capabilities: crate::execution_host::worker::CAPABILITY_NAMES
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
    }
}

pub(crate) fn worker_identity_json() -> String {
    serde_json::to_string(&worker_identity()).expect("worker build identity should serialize")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_cargo_version() {
        assert_eq!(super::version(), super::BASE_VERSION);
    }
}
