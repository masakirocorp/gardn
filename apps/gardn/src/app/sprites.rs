use sha2::{Digest, Sha256};
use std::path::Path;

use gardn_local_api::{
    PaneTarget, ResponseResult, SpriteCommand, SpriteConnection, SpriteCreateParams, SpriteError,
    SpriteOperation, SpriteOperationStatus, SpriteReply, SpriteRequest, SpriteResult, SpriteSource,
    SpriteTarget, SpritesConfig,
};

use super::{
    api::responses::{encode_error, encode_success},
    App, ClientViewState,
};

impl App {
    pub(crate) fn workspaces_without_sprite_transports(
        &self,
    ) -> std::borrow::Cow<'_, [crate::workspace::Workspace]> {
        if self.state.sprite_panes.is_empty() {
            return std::borrow::Cow::Borrowed(&self.state.workspaces);
        }
        let mut workspaces = self.state.workspaces.clone();
        for workspace in &mut workspaces {
            for pane in self.state.sprite_panes.keys() {
                if workspace.close_pane(*pane) {
                    workspace.tabs.clear();
                    workspace.public_pane_numbers.clear();
                }
            }
        }
        std::borrow::Cow::Owned(workspaces)
    }

    pub(crate) fn initialize_sprites(&mut self) -> Result<(), SpriteError> {
        self.sprites_runtime.set_waker(self.event_tx.clone());
        let configured = self
            .sprites_runtime
            .configure(self.state.sprites_config.clone());
        if !self.state.sprites_config.enabled {
            let panes: Vec<_> = self.sprite_connections.keys().copied().collect();
            for pane in panes {
                self.detach_sprite_pane(pane);
            }
            self.sprite_request_views.clear();
        }
        self.sync_sprite_snapshot();
        configured
    }

    pub(crate) fn save_sprites_config(&mut self, config: SpritesConfig) -> Result<(), String> {
        crate::config::validate_sprites_config(&config)?;
        if !self.update_config_file("Sprites settings", |content| {
            crate::config::upsert_sprites_config(content, &config)
        }) {
            return Err(self
                .state
                .config_diagnostic
                .clone()
                .unwrap_or_else(|| "Could not save Sprites settings".into()));
        }
        self.reload_config();
        if self.state.sprites_config != config
            || self.sprites_runtime.snapshot().enabled != config.enabled
        {
            return Err(
                "Sprites settings were not applied; inspect configuration diagnostics".into(),
            );
        }
        if let Some(error) = &self.sprites_runtime.snapshot().observation_error {
            return Err(error.message.clone());
        }
        Ok(())
    }

    pub(crate) fn poll_sprites(&mut self) -> bool {
        let mut changed = self.sprites_runtime.poll();
        if !self.state.sprites_config.enabled {
            return changed;
        }
        let completed = self.sprites_runtime.take_completed();
        changed |= !completed.is_empty();
        for operation in completed {
            self.present_sprite_operation(operation);
        }
        changed |= self
            .sprite_connections
            .keys()
            .any(|pane| self.find_pane(*pane).is_none());
        if changed {
            self.sync_sprite_snapshot();
        } else if !self.sprite_connections.is_empty() {
            let mut snapshot = std::mem::take(&mut self.state.sprites_snapshot);
            for resource in &mut snapshot.resources {
                let status = self.sprite_agent_status(&resource.id);
                changed |= status != resource.agent_status;
                resource.agent_status = status;
            }
            self.state.sprites_snapshot = snapshot;
        }
        changed
    }

    fn present_sprite_operation(&mut self, operation: SpriteOperation) {
        let workspace = operation.request.open_in_workspace.clone();
        let parent = operation
            .request
            .request_id
            .strip_suffix(":launch")
            .filter(|id| {
                self.sprites_runtime.snapshot().operations.iter().any(|op| {
                    op.id == *id && matches!(op.request.command, SpriteCommand::Create(_))
                })
            })
            .map(str::to_owned);
        if let Some(error) = operation.error.clone() {
            if operation.stage == "connection_canceled" {
                if let Some(SpriteResult::Connection(connection)) = &operation.result {
                    if let Some((_, pane)) = connection
                        .pane_id
                        .as_deref()
                        .and_then(|id| self.parse_pane_id(id))
                    {
                        self.detach_sprite_pane(pane);
                    }
                }
            }
            if let Some(parent) = parent {
                self.finish_sprite_presentation(&parent, Err(error));
            }
            self.sprite_request_views.remove(&operation.id);
            return;
        }
        if operation.status != SpriteOperationStatus::Succeeded {
            return;
        }
        if matches!(
            operation.stage.as_str(),
            "session_started" | "transport_open"
        ) {
            if let Some(SpriteResult::Connection(connection)) = operation.result.clone() {
                if let Some((_, pane)) = connection
                    .pane_id
                    .as_deref()
                    .and_then(|id| self.parse_pane_id(id))
                {
                    self.sprite_connections.insert(pane, connection.clone());
                }
                if let Some(view_id) = self.sprite_request_views.remove(&operation.id) {
                    self.focus_sprite_connection(view_id, &connection);
                }
                if let Some(parent) = parent {
                    self.finish_sprite_presentation(
                        &parent,
                        Ok(SpriteResult::Connection(connection)),
                    );
                }
            }
            return;
        }
        match (operation.result.clone(), workspace) {
            (Some(SpriteResult::Resource(resource)), Some(workspace))
                if matches!(operation.request.command, SpriteCommand::Create(_)) =>
            {
                if let Err(error) = self
                    .sprites_runtime
                    .begin_presentation(&operation.id, "opening_agent_terminal")
                {
                    self.finish_sprite_presentation(&operation.id, Err(error));
                    return;
                }
                let request = SpriteRequest {
                    request_id: format!("{}:launch", operation.id),
                    command: SpriteCommand::Start(SpriteTarget {
                        sprite_id: resource.id,
                        session_id: None,
                        checkpoint_id: None,
                        approval: None,
                    }),
                    open_in_workspace: Some(workspace),
                    focus: operation.request.focus,
                };
                match self.submit_sprite_request(request) {
                    Ok(SpriteReply::Operation(child)) => {
                        if let Some(view) = self.sprite_request_views.remove(&operation.id) {
                            self.sprite_request_views.insert(child.id.clone(), view);
                        }
                        match child.status {
                            SpriteOperationStatus::Failed | SpriteOperationStatus::Interrupted => {
                                let retry = SpriteRequest {
                                    request_id: String::new(),
                                    command: SpriteCommand::Retry {
                                        operation_id: child.id.clone(),
                                    },
                                    open_in_workspace: None,
                                    focus: false,
                                };
                                if let Err(error) = self.submit_sprite_request(retry) {
                                    self.finish_sprite_presentation(&operation.id, Err(error));
                                }
                            }
                            SpriteOperationStatus::Succeeded => {
                                if let Some(result) = child.result {
                                    self.finish_sprite_presentation(&operation.id, Ok(result));
                                }
                            }
                            SpriteOperationStatus::Canceled => {
                                self.finish_sprite_presentation(&operation.id, Err(sprite_error("canceled", "The agent launch was canceled. Use Start explicitly when ready.")));
                            }
                            SpriteOperationStatus::Queued | SpriteOperationStatus::Running => {}
                        }
                    }
                    Ok(_) => self.finish_sprite_presentation(
                        &operation.id,
                        Err(sprite_error(
                            "launch_failed",
                            "The agent launch did not produce an operation",
                        )),
                    ),
                    Err(error) => self.finish_sprite_presentation(&operation.id, Err(error)),
                }
            }
            (Some(SpriteResult::Connection(connection)), Some(workspace)) => {
                match self.open_sprite_connection(&workspace, connection) {
                    Ok((connection, newly_opened)) => {
                        if newly_opened {
                            if let Err(error) = self
                                .sprites_runtime
                                .watch_connection(&operation.id, &connection)
                            {
                                self.finish_sprite_presentation(&operation.id, Err(error.clone()));
                                if let Some(parent) = parent {
                                    self.finish_sprite_presentation(&parent, Err(error));
                                }
                            }
                        } else {
                            if let Some(view_id) = self.sprite_request_views.remove(&operation.id) {
                                self.focus_sprite_connection(view_id, &connection);
                            }
                            let result = Ok(SpriteResult::Connection(connection));
                            self.finish_sprite_presentation(&operation.id, result.clone());
                            if let Some(parent) = parent {
                                self.finish_sprite_presentation(&parent, result);
                            }
                        }
                    }
                    Err(error) => {
                        self.finish_sprite_presentation(&operation.id, Err(error.clone()));
                        if let Some(parent) = parent {
                            self.finish_sprite_presentation(&parent, Err(error));
                        }
                    }
                }
            }
            _ => {
                self.sprite_request_views.remove(&operation.id);
            }
        }
    }

    fn finish_sprite_presentation(&mut self, id: &str, result: Result<SpriteResult, SpriteError>) {
        if let Err(error) = self.sprites_runtime.finish_presentation(id, result) {
            self.state.config_diagnostic = Some(format!(
                "Could not record Sprite terminal outcome: {}",
                error.message
            ));
        }
    }

    pub(crate) fn submit_sprite_request(
        &mut self,
        mut request: SpriteRequest,
    ) -> Result<SpriteReply, SpriteError> {
        if !self.state.sprites_config.enabled {
            return Err(sprite_error(
                "disabled",
                "Sprites are disabled. Enable Settings > Integrations > Sprites.",
            ));
        }
        if matches!(request.command, SpriteCommand::Create(_))
            && request.request_id.trim().is_empty()
        {
            return Err(sprite_error("request_id_required", "Create requires a stable request_id. Reuse it to inspect the same creation intent after a timeout."));
        }
        if let Some(workspace) = &request.open_in_workspace {
            self.sprite_workspace(workspace)?;
        }
        if request.focus && request.open_in_workspace.is_none() {
            return Err(sprite_error(
                "invalid_target",
                "focus requires open_in_workspace",
            ));
        }
        match &mut request.command {
            SpriteCommand::Retry { operation_id } => {
                let original = self
                    .sprites_runtime
                    .snapshot()
                    .operations
                    .iter()
                    .find(|op| op.id == *operation_id)
                    .ok_or_else(|| {
                        sprite_error(
                            "operation_not_found",
                            "The original operation was not found",
                        )
                    })?;
                if let Some(workspace) = &original.request.open_in_workspace {
                    self.sprite_workspace(workspace)?;
                }
            }
            SpriteCommand::Create(params) | SpriteCommand::Preflight(params) => {
                self.validate_sprite_creation(params)?
            }
            SpriteCommand::Reassociate {
                workspace_id,
                source,
                ..
            } => {
                self.sprite_workspace(workspace_id)?;
                validate_sprite_source(source)?;
            }
            SpriteCommand::Start(_) | SpriteCommand::Resume { .. } | SpriteCommand::Shell(_)
                if request.open_in_workspace.is_none() =>
            {
                return Err(sprite_error("presentation_required", "Starting a remote session requires an explicit open_in_workspace (CLI: --workspace ID --open)."));
            }
            SpriteCommand::Disconnect(target) => {
                let panes: Vec<_> = self
                    .sprite_connections
                    .iter()
                    .filter(|(_, connection)| {
                        connection.sprite_id == target.sprite_id
                            && target
                                .session_id
                                .as_ref()
                                .is_none_or(|id| connection.session_id.as_ref() == Some(id))
                    })
                    .map(|(pane, _)| *pane)
                    .collect();
                for pane in panes {
                    self.detach_sprite_pane(pane);
                }
                self.sync_sprite_snapshot();
                return Ok(SpriteReply::Snapshot(self.state.sprites_snapshot.clone()));
            }
            _ => {}
        }
        let reply = self.sprites_runtime.submit(request)?;
        self.sync_sprite_snapshot();
        Ok(match reply {
            SpriteReply::Snapshot(_) => SpriteReply::Snapshot(self.state.sprites_snapshot.clone()),
            reply => reply,
        })
    }

    fn validate_sprite_creation(&self, params: &mut SpriteCreateParams) -> Result<(), SpriteError> {
        self.sprite_workspace(&params.workspace_id)?;
        validate_sprite_source(&params.source)?;
        if !params.agent.profile_id.is_empty() {
            let profile = self
                .state
                .agent_profiles
                .get(&params.agent.profile_id)
                .ok_or_else(|| {
                    sprite_error(
                        "profile_not_found",
                        "The selected agent profile no longer exists",
                    )
                })?;
            if let Some(reason) = profile.sprite_unavailable_reason() {
                return Err(sprite_error("profile_unsupported", reason));
            }
            params.agent.kind = profile.kind.as_str().into();
            params.agent.command = profile.argv.clone();
        }
        if params.agent.kind.is_empty()
            || params
                .agent
                .command
                .first()
                .is_none_or(|part| part.is_empty())
        {
            return Err(sprite_error(
                "invalid_agent",
                "An agent kind and nonempty remote command argv are required",
            ));
        }
        Ok(())
    }

    fn sprite_workspace(&self, id: &str) -> Result<usize, SpriteError> {
        self.state.workspaces.iter().position(|workspace| workspace.id == id)
            .ok_or_else(|| sprite_error("workspace_not_found", "The explicit target Space no longer exists. Choose a stable workspace ID; numeric focus aliases are not accepted."))
    }

    fn open_sprite_connection(
        &mut self,
        workspace_id: &str,
        mut connection: SpriteConnection,
    ) -> Result<(SpriteConnection, bool), SpriteError> {
        let ws_idx = self.sprite_workspace(workspace_id)?;
        if !connection.starts_session {
            if let Some((pane_id, _)) = self.sprite_connections.iter().find(|(pane, existing)| {
                existing.sprite_id == connection.sprite_id
                    && existing.session_id == connection.session_id
                    && self.find_pane(**pane).is_some()
            }) {
                if let Some((existing_ws, _)) = self.find_pane(*pane_id) {
                    connection.pane_id = self.public_pane_id(existing_ws, *pane_id);
                    connection.tab_id = self.state.workspaces[existing_ws]
                        .find_tab_index_for_pane(*pane_id)
                        .and_then(|tab| self.public_tab_id(existing_ws, tab));
                }
                let ready = self.sprites_runtime.snapshot().operations.iter().any(|operation| {
                    operation.status == SpriteOperationStatus::Succeeded
                        && matches!(operation.stage.as_str(), "session_started" | "transport_open" | "presentation_succeeded")
                        && matches!(&operation.result, Some(SpriteResult::Connection(ready)) if ready.pane_id.is_some() && ready.pane_id == connection.pane_id)
                });
                if !ready {
                    return Err(SpriteError {
                        code: "connection_pending".into(),
                        message: "The existing local terminal has not completed startup. Inspect its operation before reconnecting.".into(),
                        retryable: true,
                    });
                }
                return Ok((connection, false));
            }
        }
        let execution_host = crate::execution_host::ExecutionHostId::new(format!(
            "sprite:{}",
            crate::checksum::to_lower_hex(&Sha256::digest(connection.sprite_id.as_bytes()))
        ))
        .map_err(|error| sprite_error("invalid_connection", error.to_string()))?;
        let remote_path = crate::execution_host::HostPath::new(&connection.remote_cwd)
            .map_err(|error| sprite_error("invalid_connection", error.to_string()))?;
        let mut argv = Vec::with_capacity(connection.args.len() + 1);
        argv.push(connection.program.clone());
        argv.extend(connection.args.iter().cloned());
        let launch_env = connection
            .agent_kind
            .as_deref()
            .filter(|kind| crate::detect::parse_agent_label(kind).is_some())
            .map(|kind| {
                vec![(
                    crate::agent_profiles::AGENT_HINT_ENV_VAR.to_owned(),
                    kind.to_owned(),
                )]
            })
            .unwrap_or_default();
        let (rows, cols) = self.state.estimate_pane_size();
        let cwd = crate::config::state_dir().join("sprites");
        let ws = &mut self.state.workspaces[ws_idx];
        let (tab_idx, mut terminal, mut runtime) = ws
            .create_tab_argv_command(
                rows.max(4),
                cols.max(10),
                cwd,
                &argv,
                launch_env,
                self.state.pane_scrollback_limit_bytes,
                self.state.host_terminal_theme,
                self.event_tx.clone(),
                self.render_notify.clone(),
                self.render_dirty.clone(),
            )
            .map_err(|error| sprite_error("terminal_open_failed", error.to_string()))?;
        let tab = ws
            .terminal_tab_mut(tab_idx)
            .map_err(|error| sprite_error("terminal_open_failed", error.to_string()))?;
        let pane = tab.root_pane;
        tab.set_custom_name(format!("Sprite {}", connection.sprite_id));
        // The bridge is not a resumable coordinator-side agent command.
        terminal.launch_argv = None;
        terminal.respawn_shell_on_exit = false;
        terminal.persisted_agent_session = None;
        terminal.location =
            crate::execution_host::ResourceLocation::new(execution_host, remote_path);
        terminal.cwd = Path::new(&connection.remote_cwd).to_path_buf();
        runtime.disable_local_cwd_inspection();
        self.terminal_runtimes.insert(terminal.id.clone(), runtime);
        self.state.terminals.insert(terminal.id.clone(), terminal);
        self.state.remove_alias_shadowed_by_new_pane(pane);
        connection.pane_id = self.public_pane_id(ws_idx, pane);
        connection.tab_id = self.public_tab_id(ws_idx, tab_idx);
        self.state
            .sprite_panes
            .insert(pane, connection.sprite_id.clone());
        self.sprite_connections.insert(pane, connection.clone());
        self.schedule_session_save();
        Ok((connection, true))
    }

    fn focus_sprite_connection(&mut self, client_view_id: u64, connection: &SpriteConnection) {
        let Some(effect) = self.sprite_focus_effect(client_view_id, connection) else {
            return;
        };
        if self.default_client_view.apply_client_view_effect(&effect) {
            self.default_client_view.reconcile(&self.state);
        } else {
            self.pending_client_view_effects.push(effect);
        }
    }

    fn sprite_focus_effect(
        &self,
        client_view_id: u64,
        connection: &SpriteConnection,
    ) -> Option<super::view_state::ClientViewEffect> {
        let (ws_idx, pane_id) = connection
            .pane_id
            .as_deref()
            .and_then(|id| self.parse_pane_id(id))?;
        let ws = &self.state.workspaces[ws_idx];
        let (_, tab) = ws
            .terminal_tabs()
            .find(|(_, tab)| tab.panes.contains_key(&pane_id))?;
        let group_index = self
            .state
            .groups
            .iter()
            .position(|group| group.id == ws.group_id)?;
        Some(super::view_state::ClientViewEffect::FocusSpritePane {
            client_view_id,
            workspace_id: ws.id.clone(),
            tab_number: tab.number,
            pane_id,
            group_index,
        })
    }

    fn detach_sprite_pane(&mut self, pane: crate::layout::PaneId) {
        if let Some((ws_idx, pane_state)) = self.find_pane(pane) {
            let terminal_id = pane_state.attached_terminal_id.clone();
            let ws = &self.state.workspaces[ws_idx];
            if ws.tabs.len() == 1
                && ws
                    .terminal_tabs()
                    .next()
                    .is_some_and(|(_, tab)| tab.panes.len() == 1)
            {
                let ws = &mut self.state.workspaces[ws_idx];
                ws.tabs.clear();
                ws.public_pane_numbers.clear();
                self.state.remove_unattached_terminal_ids(Some(terminal_id));
                self.shutdown_detached_terminal_runtimes();
                self.default_client_view.reconcile(&self.state);
                self.sprite_connections.remove(&pane);
                self.state.sprite_panes.remove(&pane);
                self.schedule_session_save();
                return;
            }
        }
        let public_id = self
            .find_pane(pane)
            .and_then(|(ws, _)| self.public_pane_id(ws, pane));
        if let Some(pane_id) = public_id {
            if let Err(error) =
                self.close_pane("sprites:disconnect".into(), &PaneTarget { pane_id })
            {
                tracing::warn!(%error, "could not close Sprite transport pane");
                return;
            }
        }
        self.sprite_connections.remove(&pane);
        self.state.sprite_panes.remove(&pane);
    }

    fn sync_sprite_snapshot(&mut self) {
        let dead: Vec<_> = self
            .sprite_connections
            .keys()
            .filter(|pane| self.find_pane(**pane).is_none())
            .copied()
            .collect();
        for pane in dead {
            self.sprite_connections.remove(&pane);
            self.state.sprite_panes.remove(&pane);
        }
        let mut snapshot = self.sprites_runtime.snapshot().clone();
        if snapshot.enabled {
            for resource in &mut snapshot.resources {
                resource.attached_panes.clear();
                resource.agent_status = self.sprite_agent_status(&resource.id);
                for (pane_id, connection) in &self.sprite_connections {
                    if connection.sprite_id != resource.id {
                        continue;
                    }
                    let Some((ws_idx, _)) = self.find_pane(*pane_id) else {
                        continue;
                    };
                    if let Some(id) = self.public_pane_id(ws_idx, *pane_id) {
                        resource.attached_panes.push(id);
                    }
                }
            }
        }
        self.state.sprites_snapshot = snapshot;
    }

    fn sprite_agent_status(&self, resource_id: &str) -> Option<gardn_local_api::AgentStatus> {
        self.sprite_connections
            .iter()
            .find_map(|(pane_id, connection)| {
                if connection.sprite_id != resource_id || connection.agent_kind.is_none() {
                    return None;
                }
                let (_, pane) = self.find_pane(*pane_id)?;
                let terminal = self.state.terminals.get(&pane.attached_terminal_id)?;
                Some(super::api_helpers::pane_agent_status(
                    terminal.state,
                    pane.seen,
                ))
            })
    }

    pub(crate) fn handle_sprites_request(&mut self, id: String, request: SpriteRequest) -> String {
        self.with_default_client_view(|app, view| {
            app.handle_sprites_request_for_view(view, id, request)
        })
    }

    pub(crate) fn handle_sprites_request_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        request: SpriteRequest,
    ) -> String {
        let focus = request.focus;
        match self.submit_sprite_request(request) {
            Ok(reply) => {
                if focus {
                    if let SpriteReply::Operation(operation) = &reply {
                        if let Some(SpriteResult::Connection(connection)) = &operation.result {
                            if let Some(effect) = self.sprite_focus_effect(view.id(), connection) {
                                view.apply_client_view_effect(&effect);
                                view.reconcile(&self.state);
                            }
                        } else if matches!(
                            operation.status,
                            SpriteOperationStatus::Queued | SpriteOperationStatus::Running
                        ) {
                            self.sprite_request_views
                                .insert(operation.id.clone(), view.id());
                        }
                    }
                }
                encode_success(id, ResponseResult::Sprites { reply })
            }
            Err(error) => encode_error(id, &error.code, error.message),
        }
    }

    pub(crate) fn handle_sprites_capabilities(&self, id: String) -> String {
        let enabled = self.state.sprites_config.enabled;
        let actions = if enabled {
            [
                "list",
                "inspect",
                "preflight",
                "create",
                "connect",
                "disconnect",
                "start",
                "resume",
                "shell",
                "stop",
                "pull_preview",
                "pull",
                "checkpoint",
                "checkpoints",
                "restore",
                "destroy",
                "forget",
                "reassociate",
                "operation",
                "cancel",
                "retry",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        } else {
            Vec::new()
        };
        encode_success(id, ResponseResult::SpritesCapabilities { enabled, actions })
    }
}

fn validate_sprite_source(source: &SpriteSource) -> Result<(), SpriteError> {
    if source.execution_host_id != "local" {
        return Err(sprite_error("source_host_unsupported", format!("Sprite transfer from host '{}' is not available. Choose an explicit coordinator-local repository; the path will not be reinterpreted locally.", source.execution_host_id)));
    }
    if !Path::new(&source.path).is_absolute() {
        return Err(sprite_error(
            "invalid_source",
            "Sprite source must be an absolute host-qualified repository path",
        ));
    }
    Ok(())
}

fn sprite_error(code: &str, message: impl Into<String>) -> SpriteError {
    SpriteError {
        code: code.into(),
        message: message.into(),
        retryable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gardn_local_api::{EmptyParams, Method, Request, SpriteAgent};

    fn app() -> App {
        let (_, rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            true,
            None,
            rx,
            crate::api::EventHub::default(),
        )
    }

    fn request(command: SpriteCommand) -> Request {
        Request {
            id: "sprite-test".into(),
            method: Method::SpritesRequest(SpriteRequest {
                request_id: "stable-intent".into(),
                command,
                open_in_workspace: None,
                focus: false,
            }),
        }
    }

    #[test]
    fn disabled_sprites_reject_provider_refresh_and_advertise_no_actions() {
        let mut app = app();
        let capabilities: serde_json::Value =
            serde_json::from_str(&app.handle_api_request(Request {
                id: "caps".into(),
                method: Method::SpritesCapabilities(EmptyParams::default()),
            }))
            .unwrap();
        assert_eq!(capabilities["result"]["enabled"], false);
        assert_eq!(capabilities["result"]["actions"], serde_json::json!([]));
        let response: serde_json::Value = serde_json::from_str(
            &app.handle_api_request(request(SpriteCommand::List { refresh: true })),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], "disabled");
    }

    #[test]
    fn sprite_creation_rejects_remote_source_without_local_reinterpretation() {
        let mut app = app();
        let workspace = crate::workspace::Workspace::test_new("source");
        let workspace_id = workspace.id.clone();
        app.state.workspaces.push(workspace);
        app.state.sprites_config.enabled = true;
        let response: serde_json::Value = serde_json::from_str(&app.handle_api_request(request(
            SpriteCommand::Create(SpriteCreateParams {
                workspace_id,
                source: SpriteSource {
                    execution_host_id: "ssh:build:1".into(),
                    path: "/srv/project".into(),
                },
                agent: SpriteAgent {
                    profile_id: String::new(),
                    kind: "claude".into(),
                    command: vec!["claude".into()],
                    share_credentials: false,
                },
                name: None,
            }),
        )))
        .unwrap();
        assert_eq!(response["error"]["code"], "source_host_unsupported");
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ssh:build:1"));
    }

    #[test]
    fn sprite_disconnect_preserves_the_last_space_and_does_not_require_provider_access() {
        let mut app = app();
        let workspace = crate::workspace::Workspace::test_new("retained");
        let workspace_id = workspace.id.clone();
        let pane = workspace.terminal_tab(0).unwrap().root_pane;
        app.state.workspaces.push(workspace);
        app.state.sprites_config.enabled = true;
        app.state.sprite_panes.insert(pane, "org/work".into());
        app.sprite_connections.insert(
            pane,
            SpriteConnection {
                sprite_id: "org/work".into(),
                session_id: Some("session-7".into()),
                attempt_id: "test-attempt".into(),
                program: "node".into(),
                args: Vec::new(),
                remote_cwd: "/home/sprite/work".into(),
                agent_kind: Some("claude".into()),
                starts_session: false,
                pane_id: None,
                tab_id: None,
            },
        );
        let response: serde_json::Value = serde_json::from_str(&app.handle_api_request(request(
            SpriteCommand::Disconnect(SpriteTarget {
                sprite_id: "org/work".into(),
                session_id: Some("session-7".into()),
                checkpoint_id: None,
                approval: None,
            }),
        )))
        .unwrap();
        assert!(response.get("error").is_none(), "{response}");
        let spaces: serde_json::Value = serde_json::from_str(&app.handle_api_request(Request {
            id: "spaces".into(),
            method: Method::WorkspaceList(EmptyParams::default()),
        }))
        .unwrap();
        assert!(spaces["result"]["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|space| space["workspace_id"] == workspace_id));
        let tabs: serde_json::Value = serde_json::from_str(&app.handle_api_request(Request {
            id: "tabs".into(),
            method: Method::TabList(gardn_local_api::TabListParams {
                workspace_id: Some(workspace_id),
            }),
        }))
        .unwrap();
        assert_eq!(tabs["result"]["tabs"], serde_json::json!([]));
    }

    #[test]
    fn generic_agent_start_cannot_split_a_sprite_via_workspace_target() {
        let mut app = app();
        let workspace = crate::workspace::Workspace::test_new("sprite");
        let workspace_id = workspace.id.clone();
        let pane = workspace.terminal_tab(0).unwrap().root_pane;
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.state.sprite_panes.insert(pane, "org/work".into());
        let response: serde_json::Value = serde_json::from_str(&app.handle_api_request(Request {
            id: "local-agent".into(),
            method: Method::AgentStart(gardn_local_api::AgentStartParams {
                name: "wrong-host".into(),
                cwd: None,
                location: Some(gardn_local_api::ResourceLocationParams {
                    execution_host_id: "local".into(),
                    path: std::env::temp_dir().to_string_lossy().into_owned(),
                }),
                workspace_id: Some(workspace_id),
                tab_id: None,
                split: Some(gardn_local_api::SplitDirection::Right),
                focus: false,
                env: Default::default(),
                argv: vec!["must-not-be-spawned".into()],
            }),
        }))
        .unwrap();
        assert_eq!(response["error"]["code"], "sprite_operation_unavailable");
    }
}
