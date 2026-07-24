use crate::api::schema::{
    ConnectionAction, ConnectionInstallKind, ConnectionInstallParams, ConnectionInstallPreview,
    ConnectionInstallReport, ConnectionProfileInfo, ConnectionSaveParams, ConnectionStatusKind,
    ConnectionTarget, ResponseResult,
};
use crate::execution_host::{ConnectionStatus, HostPath};

use super::responses::{encode_error, encode_success};
use crate::app::App;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteTerminationInfo {
    pub(crate) terminal_id: crate::terminal::TerminalId,
    pub(crate) profile_id: String,
    pub(crate) execution_host_id: crate::execution_host::ExecutionHostId,
    pub(crate) path: std::path::PathBuf,
    pub(crate) termination_pending: bool,
}

#[derive(Debug)]
pub(crate) enum ConnectionProfileMutationError {
    Referenced(String),
    Persistence(std::io::Error),
}

impl std::fmt::Display for ConnectionProfileMutationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Referenced(message) => formatter.write_str(message),
            Self::Persistence(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ConnectionProfileMutationError {}

impl crate::app::state::AppState {
    pub(crate) fn remote_termination_tombstones_for_profile(
        &self,
        profile_id: &str,
    ) -> Vec<RemoteTerminationInfo> {
        let Some(profile) = self
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == profile_id)
        else {
            return Vec::new();
        };
        let host_id = profile.execution_host_id();
        self.remote_termination_tombstones
            .iter()
            .filter(|tombstone| tombstone.location.execution_host_id == host_id)
            .map(|tombstone| RemoteTerminationInfo {
                terminal_id: tombstone.terminal_id.clone(),
                profile_id: profile_id.to_string(),
                execution_host_id: host_id.clone(),
                path: tombstone.location.path.as_path().to_path_buf(),
                termination_pending: true,
            })
            .collect()
    }
}

impl App {
    pub(super) fn handle_connection_list(&self, id: String) -> String {
        encode_success(
            id,
            ResponseResult::ConnectionList {
                profiles: self
                    .state
                    .ssh_connection_profiles
                    .iter()
                    .map(|profile| self.connection_profile_info(profile))
                    .collect(),
            },
        )
    }

    pub(crate) fn commit_ssh_connection_profile(
        &mut self,
        profile: crate::persist::ssh_profiles::SshConnectionProfile,
    ) -> Result<(), ConnectionProfileMutationError> {
        if let Some(existing) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|existing| existing.id() == profile.id())
        {
            let old_host_id = existing.execution_host_id();
            if old_host_id != profile.execution_host_id() {
                if let Some(reference) = self.ssh_host_reference(&old_host_id)? {
                    return Err(ConnectionProfileMutationError::Referenced(format!(
                        "connection profile {} target cannot change while referenced by {reference}",
                        profile.name()
                    )));
                }
            }
        }
        let catalog = crate::persist::ssh_profiles::upsert(profile)
            .map_err(ConnectionProfileMutationError::Persistence)?;
        self.state.ssh_connection_profiles = catalog;
        if let Some(hosts) = &mut self.execution_hosts {
            hosts.sync_profiles(&self.state.ssh_connection_profiles);
        }
        Ok(())
    }

    pub(crate) fn remove_ssh_connection_profile_if_unreferenced(
        &mut self,
        profile_id: &str,
    ) -> Result<bool, ConnectionProfileMutationError> {
        if let Some(profile) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == profile_id)
        {
            let host_id = profile.execution_host_id();
            if let Some(reference) = self.ssh_host_reference(&host_id)? {
                return Err(ConnectionProfileMutationError::Referenced(format!(
                    "connection profile {} cannot be deleted while referenced by {reference}",
                    profile.name()
                )));
            }
        }
        let (removed, catalog) = crate::persist::ssh_profiles::remove(profile_id)
            .map_err(ConnectionProfileMutationError::Persistence)?;
        self.state.ssh_connection_profiles = catalog;
        if let Some(hosts) = &mut self.execution_hosts {
            hosts.sync_profiles(&self.state.ssh_connection_profiles);
        }
        Ok(removed)
    }

    pub(crate) fn forget_remote_termination(
        &mut self,
        terminal_id: &crate::terminal::TerminalId,
    ) -> Result<bool, std::io::Error> {
        let tombstone_index = self
            .state
            .remote_termination_tombstones
            .iter()
            .position(|tombstone| &tombstone.terminal_id == terminal_id);

        if let Some(index) = tombstone_index {
            // Write-ahead: durable snapshot must drop the tombstone before the
            // in-memory manager mapping is forgotten. Otherwise a crash between
            // mapping removal and debounced save restores the tombstone and
            // resumes remote process termination after the user chose forget.
            let removed = self.state.remote_termination_tombstones.remove(index);
            if let Err(err) = self.try_save_session_now() {
                self.state
                    .remote_termination_tombstones
                    .insert(index, removed);
                return Err(err);
            }
            let _ = self
                .execution_hosts
                .as_mut()
                .is_some_and(|hosts| hosts.forget_terminal(terminal_id));
            return Ok(true);
        }

        let runtime_mapping_removed = self
            .execution_hosts
            .as_mut()
            .is_some_and(|hosts| hosts.forget_terminal(terminal_id));
        Ok(runtime_mapping_removed)
    }

    fn ssh_host_reference(
        &self,
        host_id: &crate::execution_host::ExecutionHostId,
    ) -> Result<Option<&'static str>, ConnectionProfileMutationError> {
        if self.state.groups.iter().any(|group| {
            group
                .default_location
                .as_ref()
                .is_some_and(|location| &location.execution_host_id == host_id)
        }) {
            return Ok(Some("a group default"));
        }
        if self
            .state
            .workspaces
            .iter()
            .any(|workspace| &workspace.default_location.execution_host_id == host_id)
        {
            return Ok(Some("a workspace default"));
        }
        if self
            .state
            .terminals
            .values()
            .any(|terminal| &terminal.location.execution_host_id == host_id)
        {
            return Ok(Some("a terminal"));
        }
        if self
            .state
            .pending_workspace_create_location
            .as_ref()
            .is_some_and(|location| &location.execution_host_id == host_id)
            || self
                .pending_remote_creations
                .values()
                .any(|pending| &pending.requested_location().execution_host_id == host_id)
        {
            return Ok(Some("a pending terminal create"));
        }
        if self
            .state
            .remote_termination_tombstones
            .iter()
            .any(|tombstone| &tombstone.location.execution_host_id == host_id)
        {
            return Ok(Some("a pending remote termination"));
        }
        if self
            .execution_hosts
            .as_ref()
            .is_some_and(|hosts| hosts.has_host_references(host_id))
        {
            return Ok(Some("a pending terminal create or termination"));
        }
        if crate::persist::snapshots_reference_host(host_id)
            .map_err(ConnectionProfileMutationError::Persistence)?
        {
            return Ok(Some("a persisted session"));
        }
        Ok(None)
    }

    pub(super) fn handle_connection_save(
        &mut self,
        id: String,
        params: ConnectionSaveParams,
    ) -> String {
        let suggested_directory = match params.suggested_directory {
            Some(path) => match HostPath::new(path) {
                Ok(path) => Some(path),
                Err(error) => {
                    return encode_error(id, "connection_profile_invalid", error.to_string())
                }
            },
            None => None,
        };
        let profile = if let Some(existing) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == params.profile_id)
        {
            let mut profile = existing.clone();
            if let Err(error) = profile.rename(params.name) {
                return encode_error(id, "connection_profile_invalid", error.to_string());
            }
            profile.set_suggested_directory(suggested_directory);
            if let Err(error) = profile.set_target(params.target) {
                return encode_error(id, "connection_profile_invalid", error.to_string());
            }
            profile
        } else {
            match crate::persist::ssh_profiles::SshConnectionProfile::new(
                params.profile_id,
                params.name,
                params.target,
                suggested_directory,
            ) {
                Ok(profile) => profile,
                Err(error) => {
                    return encode_error(id, "connection_profile_invalid", error.to_string());
                }
            }
        };

        match self.commit_ssh_connection_profile(profile.clone()) {
            Ok(()) => encode_success(
                id,
                ResponseResult::ConnectionProfile {
                    profile: self.connection_profile_info(&profile),
                },
            ),
            Err(ConnectionProfileMutationError::Referenced(error)) => {
                encode_error(id, "connection_profile_referenced", error)
            }
            Err(error) => encode_error(id, "connection_profile_save_failed", error.to_string()),
        }
    }

    pub(super) fn handle_connection_delete(
        &mut self,
        id: String,
        target: ConnectionTarget,
    ) -> String {
        match self.remove_ssh_connection_profile_if_unreferenced(&target.profile_id) {
            Ok(removed) => encode_success(
                id,
                ResponseResult::ConnectionDeleted {
                    profile_id: target.profile_id,
                    removed,
                },
            ),
            Err(ConnectionProfileMutationError::Referenced(error)) => {
                encode_error(id, "connection_profile_referenced", error)
            }
            Err(error) => encode_error(id, "connection_profile_delete_failed", error.to_string()),
        }
    }

    pub(super) fn handle_connection_action(
        &mut self,
        id: String,
        target: ConnectionTarget,
        action: ConnectionAction,
    ) -> String {
        // Monolithic / non-view Local API calls still need an interactive owner so
        // OpenSSH challenges can be answered by the default client view.
        let owner =
            crate::execution_host::auth::AuthenticationOwner::new(self.default_client_view.id());
        self.handle_connection_action_for(id, target, action, owner)
    }

    pub(super) fn handle_connection_action_for(
        &mut self,
        id: String,
        target: ConnectionTarget,
        action: ConnectionAction,
        authentication_owner: crate::execution_host::auth::AuthenticationOwner,
    ) -> String {
        let Some(profile) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == target.profile_id)
            .cloned()
        else {
            return encode_error(
                id,
                "connection_profile_not_found",
                format!("connection profile {} not found", target.profile_id),
            );
        };
        let request_action = match action {
            ConnectionAction::Test => crate::execution_host::HostConnectionAction::Test,
            ConnectionAction::Connect => crate::execution_host::HostConnectionAction::Connect,
            ConnectionAction::Disconnect => crate::execution_host::HostConnectionAction::Disconnect,
        };
        self.state.queue_ssh_connection_request(
            target.profile_id,
            request_action,
            authentication_owner,
        );
        encode_success(
            id,
            ResponseResult::ConnectionActionQueued {
                profile: self.connection_profile_info(&profile),
                action,
            },
        )
    }

    pub(crate) fn spawn_connection_install_response(
        &self,
        respond_to: std::sync::mpsc::Sender<String>,
        request_id: String,
        profile_id: String,
        profile: ConnectionProfileInfo,
        confirm: bool,
        authentication_owner: crate::execution_host::auth::AuthenticationOwner,
    ) {
        let Some(hosts) = self.execution_hosts.as_ref() else {
            let _ = respond_to.send(encode_error(
                request_id,
                "execution_hosts_unavailable",
                "execution host manager is unavailable",
            ));
            return;
        };
        let installer = match hosts.worker_installer_for(authentication_owner, &profile_id) {
            Ok(installer) => installer,
            Err(error) => {
                let _ =
                    respond_to.send(encode_error(request_id, "connection_install_failed", error));
                return;
            }
        };
        std::thread::spawn(move || {
            let response = match installer.preview() {
                Ok(remote_preview) => {
                    let preview = connection_install_preview_from_remote(remote_preview);
                    if !confirm {
                        encode_success(
                            request_id,
                            ResponseResult::ConnectionInstall {
                                profile,
                                preview,
                                report: None,
                            },
                        )
                    } else {
                        let approved = crate::remote::WorkerInstallPreview {
                            kind: match preview.kind {
                                ConnectionInstallKind::Install => {
                                    crate::remote::WorkerInstallKind::Install
                                }
                                ConnectionInstallKind::Update => {
                                    crate::remote::WorkerInstallKind::Update
                                }
                            },
                            source: preview.source.clone(),
                            target_path: preview.target_path.clone(),
                            checksum: preview.checksum.clone(),
                            version: preview.version.clone(),
                            commands: preview.commands.clone(),
                            capabilities: preview.capabilities.clone(),
                            already_current: preview.already_current,
                        };
                        match installer.install(&approved) {
                            Ok(report) => encode_success(
                                request_id,
                                ResponseResult::ConnectionInstall {
                                    profile,
                                    preview,
                                    report: Some(connection_install_report_from_remote(report)),
                                },
                            ),
                            Err(error) => {
                                encode_error(request_id, "connection_install_failed", error)
                            }
                        }
                    }
                }
                Err(error) => encode_error(request_id, "connection_install_failed", error),
            };
            let _ = respond_to.send(response);
        });
    }

    pub(super) fn handle_connection_install_disposition(
        &mut self,
        id: String,
        params: ConnectionInstallParams,
    ) -> crate::api::ApiRequestDisposition {
        let owner =
            crate::execution_host::auth::AuthenticationOwner::new(self.default_client_view.id());
        self.handle_connection_install_disposition_for(id, params, owner)
    }

    pub(super) fn handle_connection_install_disposition_for(
        &mut self,
        id: String,
        params: ConnectionInstallParams,
        authentication_owner: crate::execution_host::auth::AuthenticationOwner,
    ) -> crate::api::ApiRequestDisposition {
        let Some(profile) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == params.profile_id)
            .cloned()
        else {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "connection_profile_not_found",
                format!("connection profile {} not found", params.profile_id),
            ));
        };
        if self.execution_hosts.is_none() {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "execution_hosts_unavailable",
                "execution host manager is unavailable",
            ));
        }
        // Never run SSH preview/install on the API handler thread: askpass prompts
        // must remain serviceable by the owning client view on the event loop.
        crate::api::ApiRequestDisposition::DeferredConnectionInstall {
            request_id: id,
            profile_id: params.profile_id,
            profile: self.connection_profile_info(&profile),
            confirm: params.confirm,
            authentication_owner,
        }
    }
    fn connection_profile_info(
        &self,
        profile: &crate::persist::ssh_profiles::SshConnectionProfile,
    ) -> ConnectionProfileInfo {
        let (status, error) = match self.state.ssh_connection_status(profile) {
            ConnectionStatus::Disconnected => (ConnectionStatusKind::Disconnected, None),
            ConnectionStatus::Connecting => (ConnectionStatusKind::Connecting, None),
            ConnectionStatus::Connected => (ConnectionStatusKind::Connected, None),
            ConnectionStatus::Reconnecting { error } => {
                (ConnectionStatusKind::Reconnecting, Some(error))
            }
            ConnectionStatus::Disconnecting => (ConnectionStatusKind::Disconnecting, None),
            ConnectionStatus::AuthenticationRequired => {
                (ConnectionStatusKind::AuthenticationRequired, None)
            }
        };
        ConnectionProfileInfo {
            profile_id: profile.id().to_string(),
            name: profile.name().to_string(),
            target: profile.target().to_string(),
            suggested_directory: profile
                .suggested_directory()
                .map(|path| path.as_path().display().to_string()),
            execution_host_id: profile.execution_host_id().as_str().to_string(),
            status,
            error,
        }
    }
}

fn connection_install_preview_from_remote(
    preview: crate::remote::WorkerInstallPreview,
) -> ConnectionInstallPreview {
    ConnectionInstallPreview {
        kind: match preview.kind {
            crate::remote::WorkerInstallKind::Install => ConnectionInstallKind::Install,
            crate::remote::WorkerInstallKind::Update => ConnectionInstallKind::Update,
        },
        source: preview.source,
        target_path: preview.target_path,
        checksum: preview.checksum,
        version: preview.version,
        commands: preview.commands,
        capabilities: preview.capabilities,
        already_current: preview.already_current,
    }
}

fn connection_install_report_from_remote(
    report: crate::remote::WorkerInstallReport,
) -> ConnectionInstallReport {
    match report {
        crate::remote::WorkerInstallReport::Installed(preview) => {
            ConnectionInstallReport::Installed {
                preview: connection_install_preview_from_remote(preview),
            }
        }
        crate::remote::WorkerInstallReport::AlreadyCurrent(preview) => {
            ConnectionInstallReport::AlreadyCurrent {
                preview: connection_install_preview_from_remote(preview),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    fn profile() -> crate::persist::ssh_profiles::SshConnectionProfile {
        crate::persist::ssh_profiles::SshConnectionProfile::new(
            "workbox",
            "Work box",
            "alice@workbox",
            None,
        )
        .expect("valid profile")
    }

    #[test]
    fn profile_delete_is_blocked_while_a_group_default_references_its_host() {
        let mut app = app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.state.ssh_connection_profiles = vec![profile];
        app.state.groups[0].default_location = Some(crate::execution_host::ResourceLocation::new(
            host_id,
            crate::execution_host::HostPath::new("/srv/work").expect("valid host path"),
        ));

        let response = app.handle_connection_delete(
            "delete".into(),
            ConnectionTarget {
                profile_id: "workbox".into(),
            },
        );
        let response: serde_json::Value = serde_json::from_str(&response).expect("valid response");

        assert_eq!(response["error"]["code"], "connection_profile_referenced");
        assert_eq!(app.state.ssh_connection_profiles.len(), 1);
        assert_eq!(app.state.ssh_connection_profiles[0].id(), "workbox");
    }

    #[test]
    fn pending_remote_workspace_create_blocks_profile_deletion() {
        let mut app = app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.state.ssh_connection_profiles = vec![profile];
        app.state.pending_workspace_create_location =
            Some(crate::execution_host::ResourceLocation::new(
                host_id,
                crate::execution_host::HostPath::new("/srv/work").expect("valid host path"),
            ));

        let response = app.handle_connection_delete(
            "delete-pending".into(),
            ConnectionTarget {
                profile_id: "workbox".into(),
            },
        );
        let response: serde_json::Value = serde_json::from_str(&response).expect("valid response");

        assert_eq!(response["error"]["code"], "connection_profile_referenced");
        assert_eq!(app.state.ssh_connection_profiles[0].id(), "workbox");
    }

    #[test]
    fn referenced_terminal_blocks_target_edit_before_generation_changes() {
        let mut app = app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.state.ssh_connection_profiles = vec![profile];
        let terminal_id = crate::terminal::TerminalId::alloc();
        app.state.terminals.insert(
            terminal_id.clone(),
            crate::terminal::TerminalState::new_at(
                terminal_id,
                crate::execution_host::ResourceLocation::new(
                    host_id,
                    crate::execution_host::HostPath::new("/srv/work").expect("valid host path"),
                ),
            ),
        );

        let response = app.handle_connection_save(
            "save".into(),
            ConnectionSaveParams {
                profile_id: "workbox".into(),
                name: "Renamed work box".into(),
                target: "bob@other-host".into(),
                suggested_directory: None,
            },
        );
        let response: serde_json::Value = serde_json::from_str(&response).expect("valid response");

        assert_eq!(response["error"]["code"], "connection_profile_referenced");
        assert_eq!(
            app.state.ssh_connection_profiles[0].host_binding_generation(),
            1
        );
        assert_eq!(
            app.state.ssh_connection_profiles[0].execution_host_id(),
            crate::execution_host::ExecutionHostId::new("ssh:workbox:1")
                .expect("valid execution host id")
        );
    }

    fn isolated_session_home(
        name: &str,
    ) -> (
        std::sync::MutexGuard<'static, ()>,
        crate::config::TestEnvVar,
        std::path::PathBuf,
    ) {
        let lock = match crate::config::test_config_env_lock().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let base = std::env::temp_dir().join(format!(
            "omh-forget-termination-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let guard = crate::config::TestEnvVar::set("XDG_CONFIG_HOME", &base);
        (lock, guard, base)
    }

    fn remote_tombstone(
        terminal_id: crate::terminal::TerminalId,
        host_id: crate::execution_host::ExecutionHostId,
    ) -> crate::app::state::RemoteTerminationTombstone {
        crate::app::state::RemoteTerminationTombstone {
            terminal_id,
            location: crate::execution_host::ResourceLocation::new(
                host_id,
                crate::execution_host::HostPath::new("/srv/work").expect("valid host path"),
            ),
            remote_runtime_identity: crate::execution_host::protocol::RuntimeIdentity::new(
                crate::execution_host::protocol::HostBindingGeneration::new(1),
                crate::execution_host::protocol::WorkerInstanceId::new("worker-a")
                    .expect("valid worker id"),
                crate::execution_host::protocol::WorkerRuntimeId::new("runtime-a")
                    .expect("valid runtime id"),
                crate::execution_host::protocol::RuntimeIncarnation::new(1),
            ),
        }
    }

    fn session_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            false,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    #[test]
    fn explicit_forget_removes_durable_tombstone_without_terminal_shutdown() {
        let (_lock, _env, _base) = isolated_session_home("forget-ok");
        let mut app = session_app();
        app.no_session = false;
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.state.ssh_connection_profiles = vec![profile];
        let terminal_id = crate::terminal::TerminalId::alloc();
        app.state
            .remote_termination_tombstones
            .push(remote_tombstone(terminal_id.clone(), host_id.clone()));
        if let Some(hosts) = app.execution_hosts.as_mut() {
            hosts.restore_termination_pending(
                terminal_id.clone(),
                app.state.remote_termination_tombstones[0].location.clone(),
                app.state.remote_termination_tombstones[0]
                    .remote_runtime_identity
                    .clone(),
            );
        }
        app.state.session_dirty = false;

        let pending = app
            .state
            .remote_termination_tombstones_for_profile("workbox");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].terminal_id, terminal_id);
        assert_eq!(pending[0].execution_host_id, host_id);
        assert!(pending[0].termination_pending);

        assert!(app.forget_remote_termination(&terminal_id).expect("forget"));
        assert!(app.state.remote_termination_tombstones.is_empty());
        assert!(!app.state.session_dirty);
        if let Some(hosts) = app.execution_hosts.as_ref() {
            assert!(!hosts.has_host_references(&host_id));
        }

        // Reference guard no longer treats this host as pending termination.
        assert_eq!(
            app.ssh_host_reference(&host_id).expect("reference check"),
            None
        );
    }

    #[test]
    fn forget_remote_termination_survives_crash_before_debounce() {
        let (_lock, _env, _base) = isolated_session_home("forget-reload");
        let terminal_id = {
            let mut app = session_app();
            app.no_session = false;
            let profile = profile();
            let host_id = profile.execution_host_id();
            app.state.ssh_connection_profiles = vec![profile];
            let terminal_id = crate::terminal::TerminalId::alloc();
            app.state
                .remote_termination_tombstones
                .push(remote_tombstone(terminal_id.clone(), host_id));
            // Seed a durable pre-forget snapshot that still contains the tombstone.
            app.try_save_session_now().expect("seed session");
            assert_eq!(
                crate::persist::load()
                    .expect("seeded session")
                    .remote_termination_tombstones
                    .len(),
                1
            );

            assert!(app.forget_remote_termination(&terminal_id).expect("forget"));
            assert!(app.state.remote_termination_tombstones.is_empty());
            // Simulate crash before any later debounced save by dropping this App.
            terminal_id
        };

        let restored = session_app();
        assert!(
            restored
                .state
                .remote_termination_tombstones
                .iter()
                .all(|tombstone| tombstone.terminal_id != terminal_id),
            "forgotten tombstone must not reload after crash"
        );
        assert!(restored.state.remote_termination_tombstones.is_empty());
    }

    #[test]
    fn forget_remote_termination_save_failure_keeps_in_memory_tombstone() {
        let (_lock, _env, base) = isolated_session_home("forget-save-fail");
        let mut app = session_app();
        app.no_session = false;
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.state.ssh_connection_profiles = vec![profile];
        let terminal_id = crate::terminal::TerminalId::alloc();
        let tombstone = remote_tombstone(terminal_id.clone(), host_id.clone());
        app.state
            .remote_termination_tombstones
            .push(tombstone.clone());
        if let Some(hosts) = app.execution_hosts.as_mut() {
            hosts.restore_termination_pending(
                terminal_id.clone(),
                tombstone.location.clone(),
                tombstone.remote_runtime_identity.clone(),
            );
        }
        app.try_save_session_now().expect("seed session");

        // Replace the session data directory with a file so atomic rename/write fails.
        let session_dir = crate::session::data_dir();
        let _ = std::fs::remove_dir_all(&session_dir);
        std::fs::write(&session_dir, b"not-a-directory").expect("block session dir");
        assert!(session_dir.starts_with(&base));

        let err = app
            .forget_remote_termination(&terminal_id)
            .expect_err("save failure must surface");
        assert!(
            matches!(
                err.kind(),
                std::io::ErrorKind::AlreadyExists
                    | std::io::ErrorKind::NotADirectory
                    | std::io::ErrorKind::PermissionDenied
            ),
            "unexpected save failure kind: {err:?}"
        );
        assert_eq!(app.state.remote_termination_tombstones, vec![tombstone]);
        if let Some(hosts) = app.execution_hosts.as_ref() {
            assert!(
                hosts.has_host_references(&host_id),
                "manager mapping must remain until durable forget succeeds"
            );
        }

        // Profile deletion stays blocked while the tombstone remains.
        let blocked = app
            .remove_ssh_connection_profile_if_unreferenced("workbox")
            .expect_err("profile still referenced");
        assert!(matches!(
            blocked,
            ConnectionProfileMutationError::Referenced(_)
        ));
    }

    #[test]
    fn non_view_connection_actions_own_auth_as_default_client_view() {
        let mut app = app();
        app.state.ssh_connection_profiles = vec![profile()];
        let default_owner =
            crate::execution_host::auth::AuthenticationOwner::new(app.default_client_view.id());
        assert_ne!(
            default_owner,
            crate::execution_host::auth::AuthenticationOwner::SYSTEM
        );

        for (request_id, action, expected_action) in [
            (
                "test-owner",
                ConnectionAction::Test,
                crate::execution_host::HostConnectionAction::Test,
            ),
            (
                "connect-owner",
                ConnectionAction::Connect,
                crate::execution_host::HostConnectionAction::Connect,
            ),
            (
                "disconnect-owner",
                ConnectionAction::Disconnect,
                crate::execution_host::HostConnectionAction::Disconnect,
            ),
        ] {
            app.state.pending_ssh_connection_requests.clear();
            let response = app.handle_connection_action(
                request_id.into(),
                ConnectionTarget {
                    profile_id: "workbox".into(),
                },
                action,
            );
            let body: serde_json::Value = serde_json::from_str(&response).expect("json");
            assert_eq!(body["result"]["type"], "connection_action_queued");
            assert_eq!(
                app.state.pending_ssh_connection_requests,
                vec![crate::app::state::SshConnectionRequest {
                    profile_id: "workbox".into(),
                    action: expected_action,
                    authentication_owner: default_owner,
                }]
            );
        }
    }

    #[test]
    fn non_view_connection_install_defers_with_default_client_owner() {
        let mut app = app();
        app.state.ssh_connection_profiles = vec![profile()];
        let default_owner =
            crate::execution_host::auth::AuthenticationOwner::new(app.default_client_view.id());

        let disposition = app.handle_connection_install_disposition(
            "install-owner".into(),
            ConnectionInstallParams {
                profile_id: "workbox".into(),
                confirm: true,
            },
        );
        match disposition {
            crate::api::ApiRequestDisposition::Respond(response) => {
                let body: serde_json::Value =
                    serde_json::from_str(&response).expect("json response");
                assert_eq!(body["id"], "install-owner");
                assert_eq!(body["error"]["code"], "execution_hosts_unavailable");
            }
            crate::api::ApiRequestDisposition::DeferredConnectionInstall {
                request_id,
                confirm,
                authentication_owner,
                ..
            } => {
                assert_eq!(request_id, "install-owner");
                assert!(confirm);
                assert_eq!(authentication_owner, default_owner);
                assert_ne!(
                    authentication_owner,
                    crate::execution_host::auth::AuthenticationOwner::SYSTEM
                );
            }
            crate::api::ApiRequestDisposition::Deferred { .. } => {
                panic!("connection.install must not use remote-create Deferred terminal path")
            }
        }
    }

    #[test]
    fn explicit_for_owner_connection_action_preserves_initiating_client() {
        let mut app = app();
        app.state.ssh_connection_profiles = vec![profile()];
        let initiator = crate::app::ClientViewState::from_default_client_state(&app.state);
        let other = crate::app::ClientViewState::from_default_client_state(&app.state);
        let initiator_owner = crate::execution_host::auth::AuthenticationOwner::new(initiator.id());
        let other_owner = crate::execution_host::auth::AuthenticationOwner::new(other.id());
        let default_owner =
            crate::execution_host::auth::AuthenticationOwner::new(app.default_client_view.id());
        assert_ne!(initiator_owner, other_owner);
        assert_ne!(initiator_owner, default_owner);

        let response = app.handle_connection_action_for(
            "connect-initiator".into(),
            ConnectionTarget {
                profile_id: "workbox".into(),
            },
            ConnectionAction::Connect,
            initiator_owner,
        );
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(body["result"]["type"], "connection_action_queued");
        assert_eq!(
            app.state.pending_ssh_connection_requests,
            vec![crate::app::state::SshConnectionRequest {
                profile_id: "workbox".into(),
                action: crate::execution_host::HostConnectionAction::Connect,
                authentication_owner: initiator_owner,
            }]
        );
    }

    #[test]
    fn only_initiating_client_sees_and_answers_authentication_challenge() {
        let mut app = app();
        if app.execution_hosts.is_none() {
            // Coordinator installation identity is unavailable in this environment.
            return;
        }
        let channel = app
            .execution_hosts
            .as_ref()
            .expect("hosts present")
            .authentication_channel_for_test();

        let mut initiator = crate::app::ClientViewState::from_default_client_state(&app.state);
        let mut other = crate::app::ClientViewState::from_default_client_state(&app.state);
        let initiator_owner = crate::execution_host::auth::AuthenticationOwner::new(initiator.id());
        let other_owner = crate::execution_host::auth::AuthenticationOwner::new(other.id());
        assert_ne!(initiator_owner, other_owner);

        let profile = profile();
        app.state.ssh_connection_profiles = vec![profile.clone()];
        app.state.queue_ssh_connection_request(
            profile.id(),
            crate::execution_host::HostConnectionAction::Connect,
            initiator_owner,
        );
        assert_eq!(
            app.state.pending_ssh_connection_requests[0].authentication_owner,
            initiator_owner
        );

        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox:1")
            .expect("valid execution host id");
        let waiter = channel.clone();
        let request = std::thread::spawn(move || {
            waiter.request(
                initiator_owner,
                1,
                host_id,
                "Password for alice@workbox:".to_string(),
            )
        });

        let challenge = {
            let mut found = None;
            for _ in 0..100 {
                if let Some(challenge) = channel.challenge_for(initiator_owner) {
                    found = Some(challenge);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            found.expect("initiator challenge should become visible")
        };
        assert_eq!(channel.challenge_for(other_owner), None);
        assert_eq!(
            channel.challenge_for(crate::execution_host::auth::AuthenticationOwner::SYSTEM),
            None
        );

        app.route_client_events_for_view(&mut initiator, Vec::new(), false);
        app.route_client_events_for_view(&mut other, Vec::new(), false);
        assert_eq!(
            initiator
                .authentication_prompt
                .as_ref()
                .map(|prompt| prompt.challenge_id),
            Some(challenge.id)
        );
        assert!(other.authentication_prompt.is_none());

        // Non-owner cannot resolve the challenge.
        assert_eq!(
            app.execution_hosts
                .as_ref()
                .expect("hosts present")
                .respond_to_authentication(
                    other_owner,
                    challenge.id,
                    crate::execution_host::auth::AuthenticationResponse::new(b"nope".to_vec()),
                ),
            Err(crate::execution_host::auth::AuthenticationResponseError::UnknownChallenge)
        );

        // Initiator answers through the client-view key path.
        if let Some(prompt) = initiator.authentication_prompt.as_mut() {
            prompt.response = "owner-secret".to_string();
        }
        app.route_client_events_for_view(
            &mut initiator,
            vec![crate::raw_input::RawInputEvent::Key(
                crate::input::TerminalKey::new(
                    crossterm::event::KeyCode::Enter,
                    crossterm::event::KeyModifiers::empty(),
                )
                .with_kind(crossterm::event::KeyEventKind::Press),
            )],
            false,
        );
        assert!(initiator.authentication_prompt.is_none());

        let mut response = request
            .join()
            .expect("challenge waiter")
            .expect("owner response")
            .into_bytes();
        assert_eq!(response, b"owner-secret");
        response.fill(0);

        // Disconnect cancels any remaining owner-scoped challenges.
        let cancel_host = crate::execution_host::ExecutionHostId::new("ssh:workbox:1")
            .expect("valid execution host id");
        let cancel_waiter = channel.clone();
        let cancelled = std::thread::spawn(move || {
            cancel_waiter.request(
                initiator_owner,
                2,
                cancel_host,
                "Password again:".to_string(),
            )
        });
        for _ in 0..100 {
            if channel.challenge_for(initiator_owner).is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.execution_hosts
            .as_ref()
            .expect("hosts present")
            .cancel_authentication_owner(initiator_owner);
        assert!(matches!(
            cancelled.join().expect("cancel waiter"),
            Err(crate::execution_host::auth::AuthenticationCancelled)
        ));
        assert_eq!(channel.challenge_for(initiator_owner), None);
    }
}
