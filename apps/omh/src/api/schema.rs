//! Local API schema facade.
//!
//! Wire DTOs and JSON Schema live in `omh_local_api`. This module re-exports
//! that contract and hosts omh-only adapters (sound, resource locations,
//! toast/config/agent-session mappings, and build/protocol identity injection).

pub use omh_local_api::*;

use crate::execution_host::{ExecutionHostId, HostPath, ResourceLocation};
use crate::sound::Sound;

/// Inject product and protocol versions into the contract schema document.
pub fn generated_schema() -> serde_json::Value {
    omh_local_api::generated_schema(
        crate::build_info::BASE_VERSION,
        crate::protocol::PROTOCOL_VERSION,
    )
}

/// Convert a runtime resource location into the wire DTO.
pub fn resource_location_params_from(location: &ResourceLocation) -> ResourceLocationParams {
    ResourceLocationParams {
        execution_host_id: location.execution_host_id.as_str().to_string(),
        path: location.path.as_path().display().to_string(),
    }
}

impl TryFrom<ResourceLocationParams> for ResourceLocation {
    type Error = String;

    fn try_from(location: ResourceLocationParams) -> Result<Self, Self::Error> {
        let host_id =
            ExecutionHostId::new(location.execution_host_id).map_err(|error| error.to_string())?;
        let path = HostPath::new(location.path).map_err(|error| error.to_string())?;
        Ok(Self::new(host_id, path))
    }
}

/// Map a wire notification sound to the runtime sound engine value.
pub fn notification_show_sound_to_sound(sound: NotificationShowSound) -> Option<Sound> {
    match sound {
        NotificationShowSound::None => None,
        NotificationShowSound::Done => Some(Sound::Done),
        NotificationShowSound::Request => Some(Sound::Request),
    }
}

/// Map wire toast position onto the config toast position.
pub fn toast_position_to_config(position: ToastPosition) -> crate::config::ToastOmhPosition {
    match position {
        ToastPosition::TopLeft => crate::config::ToastOmhPosition::TopLeft,
        ToastPosition::TopRight => crate::config::ToastOmhPosition::TopRight,
        ToastPosition::BottomLeft => crate::config::ToastOmhPosition::BottomLeft,
        ToastPosition::BottomRight => crate::config::ToastOmhPosition::BottomRight,
    }
}

/// Map config reload status onto the Local API wire enum.
pub fn config_reload_status_from_config(
    status: crate::config::ConfigReloadStatus,
) -> ConfigReloadStatus {
    match status {
        crate::config::ConfigReloadStatus::Applied => ConfigReloadStatus::Applied,
        crate::config::ConfigReloadStatus::Partial => ConfigReloadStatus::Partial,
        crate::config::ConfigReloadStatus::Failed => ConfigReloadStatus::Failed,
    }
}

/// Map agent-resume session ref kind onto the Local API wire enum.
pub fn agent_session_ref_kind_from_resume(
    kind: crate::agent_resume::AgentSessionRefKind,
) -> AgentSessionRefKind {
    match kind {
        crate::agent_resume::AgentSessionRefKind::Id => AgentSessionRefKind::Id,
        crate::agent_resume::AgentSessionRefKind::Path => AgentSessionRefKind::Path,
    }
}
