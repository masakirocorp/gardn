use super::App;
use crate::app::state::{AppState, SettingsState};
use crate::execution_host::ExecutionHostId;
#[cfg(test)]
use crate::persist::ssh_profiles::SshConnectionProfile;

pub(crate) enum IntegrationHostSelection {
    Local,
    Remote { host_id: ExecutionHostId },
}

impl IntegrationHostSelection {
    pub(crate) fn label<'a>(&'a self, app: &'a AppState) -> crate::app::host_label::HostLabel<'a> {
        match self {
            Self::Local => app.host_label(crate::app::host_label::HostLabelTarget::Coordinator),
            Self::Remote { host_id } => app.host_label(
                crate::app::host_label::HostLabelTarget::ExecutionHost(host_id),
            ),
        }
    }

    pub(crate) fn host_id(&self) -> Option<&ExecutionHostId> {
        match self {
            Self::Local => None,
            Self::Remote { host_id } => Some(host_id),
        }
    }
}

pub(crate) fn resolve(app: &AppState, settings: &SettingsState) -> IntegrationHostSelection {
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
        assert_eq!(selection.label(&state).to_string(), "Work box");
        assert_eq!(selection.host_id(), Some(&expected_host_id));
    }
}
