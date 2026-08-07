use crate::api::schema::{
    ConnectionAction, ConnectionProfileInfo, ConnectionRetireParams, ConnectionSaveParams,
    ConnectionStatusKind, ConnectionTarget, ResponseResult,
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

    pub(crate) fn ensure_ssh_connection_profile_is_unreferenced(
        &self,
        profile_id: &str,
    ) -> Result<(), ConnectionProfileMutationError> {
        let Some(profile) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == profile_id)
        else {
            return Ok(());
        };
        let host_id = profile.execution_host_id();
        if let Some(reference) = self.ssh_host_reference(&host_id)? {
            return Err(ConnectionProfileMutationError::Referenced(format!(
                "connection profile {} cannot be deleted while referenced by {reference}",
                profile.name()
            )));
        }
        Ok(())
    }

    pub(crate) fn remove_ssh_connection_profile_if_unreferenced(
        &mut self,
        profile_id: &str,
    ) -> Result<bool, ConnectionProfileMutationError> {
        self.ensure_ssh_connection_profile_is_unreferenced(profile_id)?;
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

    pub(super) fn handle_connection_retire_start(
        &mut self,
        id: String,
        params: ConnectionRetireParams,
    ) -> String {
        let host_id = match self.resolve_connection_retire_host(&id, &params) {
            Ok(host_id) => host_id,
            Err(response) => return response,
        };

        let terminal_ids = params.local_only.then(|| {
            self.state
                .terminals
                .iter()
                .filter(|(_, terminal)| terminal.location.execution_host_id == host_id)
                .map(|(terminal_id, _)| terminal_id.clone())
                .chain(
                    self.state
                        .remote_termination_tombstones
                        .iter()
                        .filter(|tombstone| tombstone.location.execution_host_id == host_id)
                        .map(|tombstone| tombstone.terminal_id.clone()),
                )
                .collect::<Vec<_>>()
        });
        let Some(hosts) = self.execution_hosts.as_mut() else {
            return encode_error(
                id,
                "execution_hosts_unavailable",
                "execution host manager is unavailable",
            );
        };
        // Idempotent fence: re-start is safe once the host is already retiring.
        let _ = hosts.begin_host_retirement(host_id.clone());
        if let Some(terminal_ids) = &terminal_ids {
            for terminal_id in terminal_ids {
                let _ = hosts.forget_terminal(terminal_id);
            }
        }
        if params.local_only {
            self.state
                .remote_termination_tombstones
                .retain(|tombstone| tombstone.location.execution_host_id != host_id);
        }

        if self
            .state
            .pending_workspace_create_location
            .as_ref()
            .is_some_and(|location| location.execution_host_id == host_id)
        {
            self.state.pending_workspace_create_location = None;
            self.state.requested_new_workspace_name = None;
            self.state.name_input.clear();
            self.state.name_input_replace_on_type = false;
            if self.state.mode == crate::app::state::Mode::RenameWorkspace {
                self.state.mode = if self.state.active.is_some() {
                    crate::app::state::Mode::Terminal
                } else {
                    crate::app::state::Mode::Navigate
                };
            }
        }
        if self
            .default_client_view
            .pending_workspace_create_location
            .as_ref()
            .is_some_and(|location| location.execution_host_id == host_id)
        {
            self.default_client_view.pending_workspace_create_location = None;
            self.default_client_view.pending_workspace_create_group = None;
            self.default_client_view.name_input.clear();
            self.default_client_view.name_input_replace_on_type = false;
            if self.default_client_view.mode == crate::app::state::Mode::RenameWorkspace {
                self.default_client_view.mode =
                    if self.default_client_view.active_workspace.is_some() {
                        crate::app::state::Mode::Terminal
                    } else {
                        crate::app::state::Mode::Navigate
                    };
            }
        }

        self.rewrite_live_session_defaults_for_host(&host_id);

        let pane_ids = self.public_pane_ids_on_host(&host_id);
        for pane_id in pane_ids {
            if let Err(response) = self.close_pane(
                id.clone(),
                &crate::api::schema::PaneTarget {
                    pane_id: pane_id.clone(),
                },
            ) {
                // A pane may disappear between inventory and close (race with
                // another client). Continue closing the rest so retirement stays
                // progress-oriented; fail closed only on unexpected codes.
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                    if parsed["error"]["code"] == "pane_not_found" {
                        continue;
                    }
                }
                return response;
            }
        }

        // Durability before accept: schedule/persist so a crash after acceptance
        // still sees fenced cleanup state (defaults rewritten, panes closed).
        self.schedule_session_save();
        if let Err(error) = self.try_save_session_now() {
            return encode_error(id, "connection_retire_persist_failed", error.to_string());
        }

        let counts = self.connection_retire_counts(&host_id);
        encode_success(
            id,
            ResponseResult::ConnectionRetireStart {
                profile_id: params.profile_id,
                execution_host_id: host_id.as_str().to_string(),
                accepted: true,
                remaining_panes: counts.remaining_panes,
                remaining_terminals: counts.remaining_terminals,
                pending_terminations: counts.pending_terminations,
            },
        )
    }

    pub(super) fn handle_connection_retire_status(
        &mut self,
        id: String,
        params: ConnectionRetireParams,
    ) -> String {
        let host_id = match self.resolve_connection_retire_host(&id, &params) {
            Ok(host_id) => host_id,
            Err(response) => return response,
        };

        let counts = self.connection_retire_counts(&host_id);
        let (manager_ready, transport_active) =
            self.execution_hosts
                .as_ref()
                .map_or((false, false), |hosts| {
                    (
                        hosts.host_retirement_ready(&host_id),
                        hosts.host_has_transport(&host_id),
                    )
                });
        let local_cleanup_complete = counts.is_clear() && manager_ready;

        if local_cleanup_complete && transport_active {
            // A worker can reject the first shutdown request while its final
            // runtime is still exiting. Keep status pending so polling retries
            // shutdown until the transport actually closes.
            if let Some(hosts) = self.execution_hosts.as_mut() {
                match hosts.request_worker_shutdown(&host_id) {
                    Ok(_) => {}
                    Err(crate::execution_host::HostOperationError::Unavailable { .. }) => {}
                    Err(error) => {
                        tracing::debug!(
                            host_id = %host_id.as_str(),
                            error = %error,
                            "connection retire status: worker shutdown request skipped"
                        );
                    }
                }
            }
        }
        let ready = local_cleanup_complete && !transport_active;

        encode_success(
            id,
            ResponseResult::ConnectionRetireStatus {
                profile_id: params.profile_id,
                execution_host_id: host_id.as_str().to_string(),
                ready,
                remaining_panes: counts.remaining_panes,
                remaining_terminals: counts.remaining_terminals,
                pending_terminations: counts.pending_terminations,
            },
        )
    }

    fn resolve_connection_retire_host(
        &self,
        id: &str,
        params: &ConnectionRetireParams,
    ) -> Result<crate::execution_host::ExecutionHostId, String> {
        let Some(profile) = self
            .state
            .ssh_connection_profiles
            .iter()
            .find(|profile| profile.id() == params.profile_id)
        else {
            return Err(encode_error(
                id.to_string(),
                "connection_profile_not_found",
                format!("connection profile {} not found", params.profile_id),
            ));
        };

        let requested =
            match crate::execution_host::ExecutionHostId::new(params.execution_host_id.clone()) {
                Ok(host_id) => host_id,
                Err(error) => {
                    return Err(encode_error(
                        id.to_string(),
                        "connection_retire_host_invalid",
                        error.to_string(),
                    ));
                }
            };

        let profile_host = profile.execution_host_id();
        if profile_host != requested {
            return Err(encode_error(
                id.to_string(),
                "connection_retire_host_mismatch",
                format!(
                    "connection profile {} maps to execution host {}, not {}",
                    params.profile_id,
                    profile_host.as_str(),
                    requested.as_str()
                ),
            ));
        }

        Ok(profile_host)
    }

    fn rewrite_live_session_defaults_for_host(
        &mut self,
        host_id: &crate::execution_host::ExecutionHostId,
    ) {
        let group_indexes: Vec<usize> = self
            .state
            .groups
            .iter()
            .enumerate()
            .filter_map(|(idx, group)| {
                group
                    .default_location
                    .as_ref()
                    .is_some_and(|location| &location.execution_host_id == host_id)
                    .then_some(idx)
            })
            .collect();
        for group_idx in group_indexes {
            let _ = self.state.set_group_default_location(group_idx, None);
        }

        let workspace_rewrites: Vec<(usize, crate::execution_host::ResourceLocation)> = self
            .state
            .workspaces
            .iter()
            .enumerate()
            .filter(|(_, workspace)| &workspace.default_location.execution_host_id == host_id)
            .map(|(idx, _)| {
                let replacement =
                    crate::execution_host::connection_retirement::local_retirement_replacement();
                (idx, replacement)
            })
            .collect();
        for (ws_idx, location) in workspace_rewrites {
            let _ = self.state.set_workspace_default_location(ws_idx, location);
        }
    }

    fn public_pane_ids_on_host(
        &self,
        host_id: &crate::execution_host::ExecutionHostId,
    ) -> Vec<String> {
        let mut pane_ids = Vec::new();
        for (ws_idx, workspace) in self.state.workspaces.iter().enumerate() {
            for tab in &workspace.tabs {
                let mut panes: Vec<_> = tab.panes.iter().collect();
                panes.sort_by_key(|(pane_id, _)| pane_id.raw());
                for (pane_id, pane) in panes {
                    let Some(terminal) = self.state.terminals.get(&pane.attached_terminal_id)
                    else {
                        continue;
                    };
                    if &terminal.location.execution_host_id != host_id {
                        continue;
                    }
                    if let Some(public_id) = self.public_pane_id(ws_idx, *pane_id) {
                        pane_ids.push(public_id);
                    }
                }
            }
        }
        pane_ids
    }

    fn connection_retire_counts(
        &self,
        host_id: &crate::execution_host::ExecutionHostId,
    ) -> ConnectionRetireCounts {
        let remaining_panes = self.public_pane_ids_on_host(host_id).len();
        let remaining_terminals = self
            .state
            .terminals
            .values()
            .filter(|terminal| &terminal.location.execution_host_id == host_id)
            .count();
        let pending_terminations = self
            .state
            .remote_termination_tombstones
            .iter()
            .filter(|tombstone| &tombstone.location.execution_host_id == host_id)
            .count();
        ConnectionRetireCounts {
            remaining_panes,
            remaining_terminals,
            pending_terminations,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConnectionRetireCounts {
    remaining_panes: usize,
    remaining_terminals: usize,
    pending_terminations: usize,
}

impl ConnectionRetireCounts {
    fn is_clear(self) -> bool {
        self.remaining_panes == 0 && self.remaining_terminals == 0 && self.pending_terminations == 0
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

    fn retire_params(profile_id: &str, execution_host_id: &str) -> ConnectionRetireParams {
        ConnectionRetireParams {
            profile_id: profile_id.into(),
            execution_host_id: execution_host_id.into(),
            local_only: false,
        }
    }

    fn seed_mixed_local_remote_session(app: &mut App) -> crate::execution_host::ExecutionHostId {
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.state.ssh_connection_profiles = vec![profile];
        app.state.workspaces = vec![crate::workspace::Workspace::test_new("retire-ws")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.ensure_test_terminals();

        let remote_location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/work").expect("valid host path"),
        );
        app.state.groups[0].default_location = Some(remote_location.clone());
        assert!(app
            .state
            .set_workspace_default_location(0, remote_location.clone()));

        // Keep a local pane and add a remote pane on the same workspace.
        let local_root = app.state.workspaces[0].tabs[0].root_pane;
        let remote_pane =
            app.state.workspaces[0].test_split(ratatui::layout::Direction::Horizontal);
        app.state.ensure_test_terminals();
        let remote_terminal = app.state.workspaces[0].tabs[0].panes[&remote_pane]
            .attached_terminal_id
            .clone();
        let terminal = app
            .state
            .terminals
            .get_mut(&remote_terminal)
            .expect("remote terminal");
        terminal.location = remote_location;
        terminal.cwd = std::path::PathBuf::from("/srv/work");

        // Local root remains local via ensure_test_terminals defaults.
        let local_terminal = app.state.workspaces[0].tabs[0].panes[&local_root]
            .attached_terminal_id
            .clone();
        assert!(app
            .state
            .terminals
            .get(&local_terminal)
            .expect("local terminal")
            .location
            .is_local());

        if let Some(hosts) = app.execution_hosts.as_mut() {
            hosts.connect_test_host(host_id.clone());
        }

        host_id
    }

    #[test]
    fn connection_retire_rejects_host_profile_mismatch() {
        let mut app = app();
        app.state.ssh_connection_profiles = vec![profile()];
        let response = app.handle_connection_retire_start(
            "retire-mismatch".into(),
            retire_params("workbox", "ssh:other:1"),
        );
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(body["error"]["code"], "connection_retire_host_mismatch");
        assert_eq!(app.state.ssh_connection_profiles.len(), 1);
    }

    #[test]
    fn connection_retire_start_closes_remote_panes_and_rewrites_defaults() {
        let (_lock, _env, _base) = isolated_session_home("retire-start-ok");
        let mut app = session_app();
        app.no_session = false;
        if app.execution_hosts.is_none() {
            return;
        }
        let host_id = seed_mixed_local_remote_session(&mut app);
        let local_root = app.state.workspaces[0].tabs[0].root_pane;
        let local_terminal = app.state.workspaces[0].tabs[0].panes[&local_root]
            .attached_terminal_id
            .clone();

        let pending_location = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/pending").expect("path"),
        );
        app.state.pending_workspace_create_location = Some(pending_location.clone());
        app.state.mode = crate::app::state::Mode::RenameWorkspace;
        app.default_client_view.pending_workspace_create_location = Some(pending_location);
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.mode = crate::app::state::Mode::RenameWorkspace;

        let response = app.handle_connection_retire_start(
            "retire-start".into(),
            retire_params("workbox", host_id.as_str()),
        );
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");
        assert_eq!(body["result"]["type"], "connection_retire_start");
        assert_eq!(body["result"]["accepted"], true);
        assert_eq!(body["result"]["remaining_panes"], 0);
        assert_eq!(body["result"]["remaining_terminals"], 0);
        // No tombstones in this pure-state close path.
        assert_eq!(body["result"]["pending_terminations"], 0);

        assert!(app.state.pending_workspace_create_location.is_none());
        assert_eq!(app.state.mode, crate::app::state::Mode::Terminal);
        assert!(app
            .default_client_view
            .pending_workspace_create_location
            .is_none());
        assert_eq!(
            app.default_client_view.mode,
            crate::app::state::Mode::Terminal
        );

        assert!(app.state.groups[0].default_location.is_none());
        assert!(app.state.workspaces[0].default_location.is_local());
        assert_eq!(
            app.state.workspaces[0].default_location,
            crate::execution_host::connection_retirement::local_retirement_replacement()
        );
        assert_eq!(app.state.workspaces[0].tabs[0].panes.len(), 1);
        assert!(app.state.terminals.contains_key(&local_terminal));
        assert!(app.state.terminals.values().all(|terminal| {
            terminal.location.is_local() || terminal.location.execution_host_id != host_id
        }));
        assert_eq!(app.state.ssh_connection_profiles.len(), 1);

        // Fence blocks new host work via a public operation.
        let probe = crate::execution_host::ResourceLocation::new(
            host_id.clone(),
            crate::execution_host::HostPath::new("/srv/new").expect("path"),
        );
        let err = app
            .execution_hosts
            .as_mut()
            .expect("hosts")
            .request_git_status(probe)
            .expect_err("retiring host must reject new work");
        assert!(
            err.to_string().contains("retiring"),
            "unexpected fence error: {err}"
        );
    }
    #[test]
    fn successful_retirement_allows_recreating_the_same_connection() {
        let (_lock, _env, _base) = isolated_session_home("retire-recreate");
        let mut app = session_app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.commit_ssh_connection_profile(profile.clone())
            .expect("persist profile");
        let hosts = app.execution_hosts.as_mut().expect("hosts");
        hosts.connect_test_host(host_id.clone());
        assert!(hosts.begin_host_retirement(host_id.clone()));

        let approved =
            crate::execution_host::connection_retirement::ApprovedConnectionRetirement::LocalOnly {
                plan: crate::execution_host::connection_retirement::ConnectionRetirementPlan {
                    host_id: host_id.clone(),
                    sessions: Vec::new(),
                },
            };
        let journal =
            crate::execution_host::connection_retirement::begin_connection_retirement_journal(
                profile.id(),
                approved,
            )
            .expect("retirement journal");
        let result = app
            .finalize_connection_retirement(
                profile.id(),
                &Ok("retired".to_string()),
                &Some(journal),
            )
            .expect("finalize retirement");
        assert_eq!(result, "retired");

        app.commit_ssh_connection_profile(profile)
            .expect("recreate profile");
        let hosts = app.execution_hosts.as_mut().expect("hosts");
        hosts.connect_test_host(host_id.clone());
        let probe = crate::execution_host::ResourceLocation::new(
            host_id,
            crate::execution_host::HostPath::new("/srv/new").expect("path"),
        );
        hosts
            .request_git_status(probe)
            .expect("recreated host should accept work");
    }

    #[test]
    fn failed_profile_removal_keeps_retirement_journal_for_retry() {
        let (_lock, _env, _base) = isolated_session_home("retire-finalize-retry");
        let mut app = session_app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.commit_ssh_connection_profile(profile.clone())
            .expect("persist profile");
        app.state.pending_workspace_create_location =
            Some(crate::execution_host::ResourceLocation::new(
                host_id.clone(),
                crate::execution_host::HostPath::new("/srv/pending").expect("path"),
            ));
        let approved =
            crate::execution_host::connection_retirement::ApprovedConnectionRetirement::LocalOnly {
                plan: crate::execution_host::connection_retirement::ConnectionRetirementPlan {
                    host_id: host_id.clone(),
                    sessions: Vec::new(),
                },
            };
        let journal =
            crate::execution_host::connection_retirement::begin_connection_retirement_journal(
                profile.id(),
                approved,
            )
            .expect("retirement journal");

        let error = app
            .finalize_connection_retirement(
                profile.id(),
                &Ok("retired".to_string()),
                &Some(journal),
            )
            .expect_err("pending create must block profile removal");
        assert!(error.contains("pending terminal create"));
        assert_eq!(app.state.ssh_connection_profiles.len(), 1);
        assert_eq!(
            crate::execution_host::connection_retirement::pending_connection_retirement_host()
                .expect("journal state")
                .as_ref(),
            Some(&host_id)
        );
    }

    #[test]
    fn pending_local_forget_resumes_and_completes_after_restart() {
        let (_lock, _env, _base) = isolated_session_home("retire-resume");
        let mut app = session_app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        app.commit_ssh_connection_profile(profile.clone())
            .expect("persist profile");
        let plan =
            crate::execution_host::connection_retirement::plan_connection_retirement(&host_id)
                .expect("retirement plan");
        let approved =
            crate::execution_host::connection_retirement::ApprovedConnectionRetirement::LocalOnly {
                plan,
            };
        let journal =
            crate::execution_host::connection_retirement::begin_connection_retirement_journal(
                profile.id(),
                approved,
            )
            .expect("retirement journal");

        drop(journal);

        app.resume_pending_connection_retirement()
            .expect("resume retirement");
        let completion = app.event_rx.blocking_recv().expect("retirement completion");
        assert!(matches!(
            &completion,
            crate::events::AppEvent::ConnectionRetired {
                result: Ok(_),
                journal: Some(_),
                ..
            }
        ));
        app.handle_internal_event(completion);

        assert!(app.state.ssh_connection_profiles.is_empty());
        assert_eq!(
            crate::execution_host::connection_retirement::pending_connection_retirement_host()
                .expect("journal state"),
            None
        );
    }

    #[test]
    fn restart_finishes_journal_after_profile_was_already_removed() {
        let (_lock, _env, _base) = isolated_session_home("retire-profile-removed");
        let mut app = session_app();
        let profile = profile();
        let host_id = profile.execution_host_id();
        let approved =
            crate::execution_host::connection_retirement::ApprovedConnectionRetirement::LocalOnly {
                plan: crate::execution_host::connection_retirement::ConnectionRetirementPlan {
                    host_id: host_id.clone(),
                    sessions: Vec::new(),
                },
            };
        let journal =
            crate::execution_host::connection_retirement::begin_connection_retirement_journal(
                profile.id(),
                approved,
            )
            .expect("retirement journal");
        drop(journal);
        let hosts = app.execution_hosts.as_mut().expect("hosts");
        hosts.connect_test_host(host_id.clone());
        assert!(hosts.begin_host_retirement(host_id.clone()));

        app.resume_pending_connection_retirement()
            .expect("finish pending journal");
        assert_eq!(
            crate::execution_host::connection_retirement::pending_connection_retirement_host()
                .expect("journal state"),
            None
        );

        app.commit_ssh_connection_profile(profile)
            .expect("recreate profile");
        let hosts = app.execution_hosts.as_mut().expect("hosts");
        hosts.connect_test_host(host_id.clone());
        hosts
            .request_git_status(crate::execution_host::ResourceLocation::new(
                host_id,
                crate::execution_host::HostPath::new("/srv/new").expect("path"),
            ))
            .expect("recreated host should accept work");
    }

    #[test]
    fn local_only_forget_drops_coordinator_state_without_remote_cleanup() {
        let (_lock, _env, _base) = isolated_session_home("retire-local-forget");
        let mut app = session_app();
        app.no_session = false;
        if app.execution_hosts.is_none() {
            return;
        }
        let host_id = seed_mixed_local_remote_session(&mut app);
        let tombstone_id = crate::terminal::TerminalId::alloc();
        app.state
            .remote_termination_tombstones
            .push(remote_tombstone(tombstone_id.clone(), host_id.clone()));
        app.execution_hosts
            .as_mut()
            .expect("hosts")
            .restore_termination_pending(
                tombstone_id,
                app.state.remote_termination_tombstones[0].location.clone(),
                app.state.remote_termination_tombstones[0]
                    .remote_runtime_identity
                    .clone(),
            );

        let mut params = retire_params("workbox", host_id.as_str());
        params.local_only = true;
        let response = app.handle_connection_retire_start("forget-local".into(), params);
        let body: serde_json::Value = serde_json::from_str(&response).expect("json");

        assert_eq!(body["result"]["accepted"], true);
        assert_eq!(body["result"]["remaining_panes"], 0);
        assert_eq!(body["result"]["remaining_terminals"], 0);
        assert_eq!(body["result"]["pending_terminations"], 0);
        assert!(app.state.remote_termination_tombstones.is_empty());
        assert!(!app
            .execution_hosts
            .as_ref()
            .expect("hosts")
            .has_host_references(&host_id));
        assert_eq!(app.state.ssh_connection_profiles.len(), 1);
    }

    #[test]
    fn connection_retire_start_is_idempotent() {
        let (_lock, _env, _base) = isolated_session_home("retire-idempotent");
        let mut app = session_app();
        app.no_session = false;
        if app.execution_hosts.is_none() {
            return;
        }
        let host_id = seed_mixed_local_remote_session(&mut app);
        let first = app.handle_connection_retire_start(
            "retire-1".into(),
            retire_params("workbox", host_id.as_str()),
        );
        let second = app.handle_connection_retire_start(
            "retire-2".into(),
            retire_params("workbox", host_id.as_str()),
        );
        let first_body: serde_json::Value = serde_json::from_str(&first).expect("json");
        let second_body: serde_json::Value = serde_json::from_str(&second).expect("json");
        assert_eq!(first_body["result"]["accepted"], true);
        assert_eq!(second_body["result"]["accepted"], true);
        assert_eq!(second_body["result"]["remaining_panes"], 0);
        assert!(app.state.groups[0].default_location.is_none());
        assert_eq!(app.state.workspaces[0].tabs[0].panes.len(), 1);
    }

    #[test]
    fn connection_retire_status_not_ready_while_pending_termination() {
        let (_lock, _env, _base) = isolated_session_home("retire-status-pending");
        let mut app = session_app();
        app.no_session = false;
        if app.execution_hosts.is_none() {
            return;
        }
        let host_id = seed_mixed_local_remote_session(&mut app);
        let _ = app.handle_connection_retire_start(
            "retire-start".into(),
            retire_params("workbox", host_id.as_str()),
        );

        let terminal_id = crate::terminal::TerminalId::alloc();
        let tombstone = remote_tombstone(terminal_id.clone(), host_id.clone());
        app.state
            .remote_termination_tombstones
            .push(tombstone.clone());
        app.execution_hosts
            .as_mut()
            .expect("hosts")
            .restore_termination_pending(
                terminal_id,
                tombstone.location.clone(),
                tombstone.remote_runtime_identity.clone(),
            );

        let status = app.handle_connection_retire_status(
            "retire-status".into(),
            retire_params("workbox", host_id.as_str()),
        );
        let body: serde_json::Value = serde_json::from_str(&status).expect("json");
        assert_eq!(body["result"]["type"], "connection_retire_status");
        assert_eq!(body["result"]["ready"], false);
        assert_eq!(body["result"]["pending_terminations"], 1);
        assert_eq!(body["result"]["remaining_panes"], 0);
    }

    #[test]
    fn connection_retire_status_ready_when_clear_and_fenced() {
        let (_lock, _env, _base) = isolated_session_home("retire-status-ready");
        let mut app = session_app();
        app.no_session = false;
        if app.execution_hosts.is_none() {
            return;
        }
        let host_id = seed_mixed_local_remote_session(&mut app);
        let _ = app.handle_connection_retire_start(
            "retire-start".into(),
            retire_params("workbox", host_id.as_str()),
        );

        // First status may not be ready if manager still tracks host refs from
        // connect_test_host alone; with no panes/terminals/tombstones and fence
        // set, host_retirement_ready requires no manager host references.
        // Ensure manager refs are empty.
        assert!(!app
            .execution_hosts
            .as_ref()
            .expect("hosts")
            .has_host_references(&host_id));

        let status = app.handle_connection_retire_status(
            "retire-status".into(),
            retire_params("workbox", host_id.as_str()),
        );
        let body: serde_json::Value = serde_json::from_str(&status).expect("json");
        assert_eq!(
            body["result"]["ready"], false,
            "retirement must wait for the managed worker transport to close"
        );
        assert_eq!(body["result"]["remaining_panes"], 0);
        assert_eq!(body["result"]["remaining_terminals"], 0);
        assert_eq!(body["result"]["pending_terminations"], 0);

        app.execution_hosts
            .as_mut()
            .expect("hosts")
            .disconnect_test_host(&host_id);

        // Repeated status remains safe when transport is already gone.
        let again = app.handle_connection_retire_status(
            "retire-status-2".into(),
            retire_params("workbox", host_id.as_str()),
        );
        let again_body: serde_json::Value = serde_json::from_str(&again).expect("json");
        assert_eq!(again_body["result"]["ready"], true);
    }
}
