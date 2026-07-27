use crate::app::state::{AppState, SettingsState};
use crate::execution_host::ExecutionHostId;
use crate::persist::ssh_profiles::SshConnectionProfile;

use super::App;

pub(crate) enum IntegrationHostSelection<'a> {
    Local,
    Remote {
        profile: &'a SshConnectionProfile,
        host_id: ExecutionHostId,
    },
}

impl IntegrationHostSelection<'_> {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Local => "Local",
            Self::Remote { profile, .. } => profile.name(),
        }
    }

    pub(crate) fn host_id(&self) -> Option<&ExecutionHostId> {
        match self {
            Self::Local => None,
            Self::Remote { host_id, .. } => Some(host_id),
        }
    }
}

pub(crate) fn resolve<'a>(
    app: &'a AppState,
    settings: &SettingsState,
) -> IntegrationHostSelection<'a> {
    let Some(profile) = settings
        .integration_host_profile_id
        .as_deref()
        .and_then(|profile_id| {
            app.ssh_connection_profiles
                .iter()
                .find(|profile| profile.id() == profile_id)
        })
    else {
        return IntegrationHostSelection::Local;
    };

    IntegrationHostSelection::Remote {
        host_id: profile.execution_host_id(),
        profile,
    }
}

impl App {
    pub(super) fn apply_host_integration_update(
        &mut self,
        host_id: ExecutionHostId,
        request_id: crate::execution_host::protocol::RequestId,
        result: Result<
            crate::integration::host::HostIntegrationResult,
            crate::execution_host::protocol::WorkerError,
        >,
    ) -> bool {
        if self.state.host_integration_request_ids.get(&host_id) != Some(&request_id) {
            return false;
        }
        self.state.host_integration_request_ids.remove(&host_id);
        let observation = match result {
            Ok(result) => {
                self.state
                    .host_integration_install_messages
                    .insert(host_id.clone(), result.messages);
                crate::integration::host::HostIntegrationObservation::Ready(result.snapshot)
            }
            Err(error) => {
                crate::integration::host::HostIntegrationObservation::Failed(error.message)
            }
        };
        self.state
            .host_integration_observations
            .insert(host_id, observation);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::HostPath;

    #[test]
    fn stale_profile_selection_resolves_to_local() {
        let mut state = AppState::test_new();
        state.settings.integration_host_profile_id = Some("deleted".to_string());

        assert!(matches!(
            resolve(&state, &state.settings),
            IntegrationHostSelection::Local
        ));
    }

    #[test]
    fn configured_profile_selection_resolves_host_identity_and_label() {
        let mut state = AppState::test_new();
        let profile = SshConnectionProfile::new(
            "workbox",
            "Work box",
            "build.example",
            Some(HostPath::new("/srv/work").unwrap()),
        )
        .unwrap();
        let expected_host_id = profile.execution_host_id();
        state.ssh_connection_profiles.push(profile);
        state.settings.integration_host_profile_id = Some("workbox".to_string());

        let selection = resolve(&state, &state.settings);
        assert_eq!(selection.label(), "Work box");
        assert_eq!(selection.host_id(), Some(&expected_host_id));
    }
}
