use std::path::PathBuf;

use ratatui::layout::Direction;

use super::super::responses::{encode_error, encode_success};
use crate::api::schema::{
    InstalledPluginInfo, PluginInvocationContext, PluginManifestPane, PluginPaneInfo,
    PluginPaneOpenParams, PluginPanePlacement, ResponseResult,
};
use crate::app::App;

/// Local plugin pane launch plan. Remote plugin panes are rejected by
/// [`resolve_plugin_pane_launch`] before any terminal create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginPaneLaunchPlan {
    pub(crate) cwd: PathBuf,
}

/// Resolve cwd for a plugin pane launch.
///
/// ADR 0057 v1: plugin code is not installed or executed on workers. Local
/// targets keep prior semantics (`override` as-is, else `plugin_root`). Remote
/// targets return `unsupported_execution_host` and never fall back to the
/// remote project path (which would run same-named project files).
pub(crate) fn resolve_plugin_pane_launch(
    plugin: &InstalledPluginInfo,
    override_cwd: Option<&str>,
    location: &crate::execution_host::ResourceLocation,
) -> Result<PluginPaneLaunchPlan, (String, String)> {
    if !location.is_local() {
        return Err((
            "unsupported_execution_host".to_string(),
            "plugin panes cannot run on remote execution hosts in v1; \
plugin code is not installed or executed on workers"
                .to_string(),
        ));
    }
    // Exact prior local behavior: raw override PathBuf, else plugin_root.
    let cwd = if let Some(cwd) = override_cwd {
        PathBuf::from(cwd)
    } else {
        PathBuf::from(&plugin.plugin_root)
    };
    Ok(PluginPaneLaunchPlan { cwd })
}

impl App {
    pub(super) fn open_plugin_popup_pane_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        id: String,
        params: PluginPaneOpenParams,
        plugin: &InstalledPluginInfo,
        pane: PluginManifestPane,
        location: crate::execution_host::ResourceLocation,
    ) -> crate::api::ApiRequestDisposition {
        let ws_idx = match view.active_workspace {
            Some(ws_idx) => ws_idx,
            None => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "no_active_workspace",
                    "no active workspace",
                ))
            }
        };
        let launch = match resolve_plugin_pane_launch(plugin, params.cwd.as_deref(), &location) {
            Ok(plan) => plan,
            Err((code, message)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(id, &code, message))
            }
        };
        let context = self.plugin_context_for_workspace(ws_idx, "plugin-pane");
        let extra_env =
            match self.plugin_pane_launch_env(plugin, &pane.id, params.env.clone(), &context) {
                Ok(env) => env,
                Err((code, message)) => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id, &code, message,
                    ))
                }
            };

        let (pane_id, _) = match self.spawn_popup_argv_command_for_view(
            view,
            &pane.command,
            Some(launch.cwd),
            extra_env,
            crate::app::popup::PopupGeometry::default(),
        ) {
            Ok(result) => result,
            Err(err) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "plugin_pane_open_failed",
                    err.to_string(),
                ))
            }
        };
        let entrypoint = pane.id.clone();
        if let Some(terminal_id) = self
            .state
            .popup_panes
            .get(&pane_id)
            .map(|popup| popup.terminal_id.clone())
        {
            if let Some(terminal) = self.state.terminals.get_mut(&terminal_id) {
                terminal.set_manual_label(pane.title.clone());
            }
        }
        self.state.plugin_panes.insert(
            pane_id,
            crate::app::state::PluginPaneRecord {
                plugin_id: plugin.plugin_id.clone(),
                entrypoint: entrypoint.clone(),
            },
        );
        self.schedule_session_save();
        let Some(pane_info) = self.popup_pane_info_for_view(view, pane_id) else {
            self.close_popup_pane_for_view(view);
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "plugin_pane_open_failed",
                "popup pane disappeared",
            ));
        };
        self.emit_event(crate::api::schema::EventEnvelope {
            event: crate::api::schema::EventKind::PaneCreated,
            data: crate::api::schema::EventData::PaneCreated {
                pane: pane_info.clone(),
            },
        });
        crate::api::ApiRequestDisposition::Respond(encode_success(
            id,
            ResponseResult::PluginPaneOpened {
                plugin_pane: PluginPaneInfo {
                    plugin_id: plugin.plugin_id.clone(),
                    entrypoint,
                    pane: pane_info,
                },
            },
        ))
    }

    pub(crate) fn focus_plugin_popup_pane_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        id: String,
        params: crate::api::schema::PluginPaneFocusParams,
    ) -> String {
        let Some(pane_id) = self.parse_popup_public_pane_id(&params.pane_id) else {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        };
        let Some(popup) = self.state.popup_panes.get(&pane_id) else {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        };
        if popup.owner.is_some_and(|owner| owner != view.id()) {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        }
        view.popup_pane = Some(pane_id);
        view.mode = crate::app::Mode::Terminal;
        let Some(record) = self.state.plugin_panes.get(&pane_id).cloned() else {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        };
        let Some(pane) = self.popup_pane_info_for_view(view, pane_id) else {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        };
        encode_success(
            id,
            ResponseResult::PluginPaneFocused {
                plugin_pane: PluginPaneInfo {
                    plugin_id: record.plugin_id,
                    entrypoint: record.entrypoint,
                    pane,
                },
            },
        )
    }

    pub(crate) fn close_plugin_popup_pane_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        id: String,
        params: crate::api::schema::PluginPaneCloseParams,
    ) -> String {
        let Some(pane_id) = self.parse_popup_public_pane_id(&params.pane_id) else {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        };
        let Some(popup) = self.state.popup_panes.get(&pane_id) else {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        };
        if popup.owner.is_some_and(|owner| owner != view.id()) || view.popup_pane != Some(pane_id) {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        }
        if !self.close_popup_pane_for_view(view) {
            return encode_error(id, "plugin_pane_not_found", "plugin pane not found");
        }
        encode_success(
            id,
            ResponseResult::PluginPaneClosed {
                pane_id: params.pane_id,
            },
        )
    }

    pub(super) fn open_plugin_split_pane(
        &mut self,
        view: &mut crate::app::ClientViewState,
        id: String,
        params: PluginPaneOpenParams,
        plugin: &InstalledPluginInfo,
        pane: PluginManifestPane,
        placement: PluginPanePlacement,
        location: crate::execution_host::ResourceLocation,
    ) -> crate::api::ApiRequestDisposition {
        let target_pane_id = params.target_pane_id.clone();
        let Some(target_pane_id) = target_pane_id else {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "no_active_pane",
                "no active pane",
            ));
        };
        let Some((ws_idx, target_pane)) = self.parse_pane_id(&target_pane_id) else {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "pane_not_found",
                format!("pane {target_pane_id} not found"),
            ));
        };
        let launch = match resolve_plugin_pane_launch(plugin, params.cwd.as_deref(), &location) {
            Ok(plan) => plan,
            Err((code, message)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(id, &code, message))
            }
        };
        let context = self.plugin_context_for_pane(ws_idx, target_pane, "plugin-pane");
        let extra_env =
            match self.plugin_pane_launch_env(plugin, &pane.id, params.env.clone(), &context) {
                Ok(env) => env,
                Err((code, message)) => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id, &code, message,
                    ))
                }
            };
        let direction = match params
            .direction
            .unwrap_or(crate::api::schema::SplitDirection::Right)
        {
            crate::api::schema::SplitDirection::Right => Direction::Horizontal,
            crate::api::schema::SplitDirection::Down => Direction::Vertical,
        };
        let focus = params.focus || placement == PluginPanePlacement::Zoomed;

        let (rows, cols) = self.state.estimate_pane_size();
        let previous_focus = self.state.current_pane_focus_target();
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "workspace_not_found",
                "workspace not found",
            ));
        };
        let result = ws.split_pane_argv_command(
            target_pane,
            direction,
            rows.max(4),
            cols.max(10),
            Some(launch.cwd),
            &pane.command,
            extra_env,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
            focus,
        );
        let (tab_idx, new_pane) = match result {
            Some(Ok(result)) => result,
            Some(Err(err)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "plugin_pane_open_failed",
                    err.to_string(),
                ))
            }
            None => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "pane_not_found",
                    format!("pane {target_pane_id} not found"),
                ));
            }
        };
        if focus {
            self.state.switch_workspace_tab(ws_idx, tab_idx);
            self.state
                .record_pane_focus_change(previous_focus, ws_idx, new_pane.pane_id);
            self.state.mode = crate::app::Mode::Terminal;
            let _ = view;
        }
        if placement == PluginPanePlacement::Zoomed {
            if let Some(tab) = self
                .state
                .workspaces
                .get_mut(ws_idx)
                .and_then(|ws| ws.tabs.get_mut(tab_idx))
            {
                tab.zoomed = true;
            }
        }
        crate::api::ApiRequestDisposition::Respond(self.finish_plugin_pane_open(
            id,
            ws_idx,
            None,
            new_pane,
            plugin.plugin_id.clone(),
            pane,
        ))
    }

    pub(super) fn open_plugin_tab(
        &mut self,
        view: &mut crate::app::ClientViewState,
        id: String,
        params: PluginPaneOpenParams,
        plugin: &InstalledPluginInfo,
        pane: PluginManifestPane,
        location: crate::execution_host::ResourceLocation,
    ) -> crate::api::ApiRequestDisposition {
        let ws_idx = match params.workspace_id.as_deref() {
            Some(workspace_id) => match self.parse_workspace_id(workspace_id) {
                Some(ws_idx) => ws_idx,
                None => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "workspace_not_found",
                        "workspace not found",
                    ))
                }
            },
            None => match self.state.active {
                Some(ws_idx) => ws_idx,
                None => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id,
                        "no_active_workspace",
                        "no active workspace",
                    ))
                }
            },
        };
        let launch = match resolve_plugin_pane_launch(plugin, params.cwd.as_deref(), &location) {
            Ok(plan) => plan,
            Err((code, message)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(id, &code, message))
            }
        };
        let context = self.plugin_context_for_workspace(ws_idx, "plugin-pane");
        let extra_env =
            match self.plugin_pane_launch_env(plugin, &pane.id, params.env.clone(), &context) {
                Ok(env) => env,
                Err((code, message)) => {
                    return crate::api::ApiRequestDisposition::Respond(encode_error(
                        id, &code, message,
                    ))
                }
            };
        let _ = view;

        let (rows, cols) = self.state.estimate_pane_size();
        let Some(ws) = self.state.workspaces.get_mut(ws_idx) else {
            return crate::api::ApiRequestDisposition::Respond(encode_error(
                id,
                "workspace_not_found",
                "workspace not found",
            ));
        };
        let (tab_idx, terminal, runtime) = match ws.create_tab_argv_command(
            rows.max(4),
            cols.max(10),
            launch.cwd,
            &pane.command,
            extra_env,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
        ) {
            Ok(result) => result,
            Err(err) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(
                    id,
                    "plugin_pane_open_failed",
                    err.to_string(),
                ))
            }
        };
        let pane_id = ws.tabs[tab_idx].root_pane;
        if params.focus {
            self.state.switch_workspace_tab(ws_idx, tab_idx);
            self.state.mode = crate::app::Mode::Terminal;
        }
        let new_pane = crate::workspace::NewPane {
            pane_id,
            terminal,
            runtime,
        };
        crate::api::ApiRequestDisposition::Respond(self.finish_plugin_pane_open(
            id,
            ws_idx,
            Some(tab_idx),
            new_pane,
            plugin.plugin_id.clone(),
            pane,
        ))
    }

    fn plugin_pane_launch_env(
        &self,
        plugin: &InstalledPluginInfo,
        entrypoint: &str,
        env: std::collections::HashMap<String, String>,
        context: &PluginInvocationContext,
    ) -> Result<Vec<(String, String)>, (String, String)> {
        let mut env = super::super::env::normalize_launch_env(env)?;
        let context_json = serde_json::to_string(&context)
            .map_err(|err| ("invalid_plugin_context".to_string(), err.to_string()))?;
        env.retain(|(key, _)| !plugin_pane_protected_env_key(key));
        env.extend(super::env::plugin_path_env(plugin));
        env.push(("GARDN_PLUGIN_ID".to_string(), plugin.plugin_id.clone()));
        env.push((
            "GARDN_PLUGIN_ENTRYPOINT_ID".to_string(),
            entrypoint.to_string(),
        ));
        env.push(("GARDN_PLUGIN_CONTEXT_JSON".to_string(), context_json));
        if let Some(workspace_id) = context.workspace_id.as_ref() {
            env.push(("GARDN_WORKSPACE_ID".to_string(), workspace_id.clone()));
        }
        if let Some(tab_id) = context.tab_id.as_ref() {
            env.push(("GARDN_TAB_ID".to_string(), tab_id.clone()));
        }
        if let Some(pane_id) = context.focused_pane_id.as_ref() {
            env.push(("GARDN_PANE_ID".to_string(), pane_id.clone()));
        }
        if let Ok(current_exe) = std::env::current_exe() {
            env.push((
                "GARDN_BIN_PATH".to_string(),
                current_exe.display().to_string(),
            ));
        }
        Ok(env)
    }

    fn finish_plugin_pane_open(
        &mut self,
        id: String,
        ws_idx: usize,
        created_tab_idx: Option<usize>,
        new_pane: crate::workspace::NewPane,
        plugin_id: String,
        pane_manifest: PluginManifestPane,
    ) -> String {
        let entrypoint = pane_manifest.id.clone();
        let mut terminal = new_pane.terminal;
        terminal.set_manual_label(pane_manifest.title.clone());
        let terminal_id = terminal.id.clone();
        self.terminal_runtimes
            .insert(terminal_id.clone(), new_pane.runtime);
        self.state
            .remove_alias_shadowed_by_new_pane(new_pane.pane_id);
        self.state.terminals.insert(terminal_id, terminal);
        self.state.plugin_panes.insert(
            new_pane.pane_id,
            crate::app::state::PluginPaneRecord {
                plugin_id: plugin_id.clone(),
                entrypoint: entrypoint.clone(),
            },
        );
        if let Some(tab_idx) = created_tab_idx {
            if let Some(tab) = self.tab_info(ws_idx, tab_idx) {
                self.emit_event(crate::api::schema::EventEnvelope {
                    event: crate::api::schema::EventKind::TabCreated,
                    data: crate::api::schema::EventData::TabCreated { tab },
                });
            }
        }
        self.schedule_session_save();
        let Some(pane) = self.pane_info(ws_idx, new_pane.pane_id) else {
            return encode_error(id, "plugin_pane_open_failed", "plugin pane disappeared");
        };
        self.emit_event(crate::api::schema::EventEnvelope {
            event: crate::api::schema::EventKind::PaneCreated,
            data: crate::api::schema::EventData::PaneCreated { pane: pane.clone() },
        });
        encode_success(
            id,
            ResponseResult::PluginPaneOpened {
                plugin_pane: PluginPaneInfo {
                    plugin_id,
                    entrypoint,
                    pane,
                },
            },
        )
    }

    pub(super) fn plugin_pane_target_location_for_view(
        &self,
        view: &crate::app::ClientViewState,
        placement: PluginPanePlacement,
        params: &PluginPaneOpenParams,
    ) -> Result<crate::execution_host::ResourceLocation, (String, String)> {
        let pane_location = |ws_idx: usize, pane_id: crate::layout::PaneId| {
            let workspace = self.state.workspaces.get(ws_idx)?;
            let terminal_id = workspace
                .pane_state(pane_id)
                .map(|pane| &pane.attached_terminal_id)?;
            self.state
                .terminals
                .get(terminal_id)
                .map(|terminal| terminal.location.clone())
        };

        match placement {
            PluginPanePlacement::Overlay => {
                let ws_idx = view.active_workspace.ok_or_else(|| {
                    (
                        "no_active_workspace".to_string(),
                        "no active workspace".to_string(),
                    )
                })?;
                let (_, pane_id) = view
                    .focused_pane_for_workspace(&self.state, ws_idx)
                    .ok_or_else(|| ("no_active_pane".to_string(), "no active pane".to_string()))?;
                pane_location(ws_idx, pane_id).ok_or_else(|| {
                    (
                        "plugin_pane_target_unavailable".to_string(),
                        "plugin pane target terminal is unavailable".to_string(),
                    )
                })
            }
            PluginPanePlacement::Split | PluginPanePlacement::Zoomed => {
                let pane_id = params
                    .target_pane_id
                    .as_deref()
                    .ok_or_else(|| ("no_active_pane".to_string(), "no active pane".to_string()))?;
                let (ws_idx, pane_id) = self.parse_pane_id(pane_id).ok_or_else(|| {
                    (
                        "pane_not_found".to_string(),
                        format!("pane {pane_id} not found"),
                    )
                })?;
                pane_location(ws_idx, pane_id).ok_or_else(|| {
                    (
                        "plugin_pane_target_unavailable".to_string(),
                        "plugin pane target terminal is unavailable".to_string(),
                    )
                })
            }
            PluginPanePlacement::Tab => {
                let ws_idx = params
                    .workspace_id
                    .as_deref()
                    .and_then(|workspace_id| self.parse_workspace_id(workspace_id))
                    .or(self.state.active)
                    .ok_or_else(|| {
                        (
                            "workspace_not_found".to_string(),
                            "workspace not found".to_string(),
                        )
                    })?;
                if view.active_workspace == Some(ws_idx) {
                    if let Some((_, pane_id)) = view.focused_pane_for_workspace(&self.state, ws_idx)
                    {
                        if let Some(location) = pane_location(ws_idx, pane_id) {
                            return Ok(location);
                        }
                    }
                }
                let workspace = self.state.workspaces.get(ws_idx).ok_or_else(|| {
                    (
                        "workspace_not_found".to_string(),
                        "workspace not found".to_string(),
                    )
                })?;
                let pane_id = workspace
                    .tabs
                    .get(workspace.active_tab)
                    .map(|tab| tab.layout.focused())
                    .ok_or_else(|| ("no_active_pane".to_string(), "no active pane".to_string()))?;
                pane_location(ws_idx, pane_id).ok_or_else(|| {
                    (
                        "plugin_pane_target_unavailable".to_string(),
                        "plugin pane target terminal is unavailable".to_string(),
                    )
                })
            }
        }
    }
}

fn plugin_pane_protected_env_key(key: &str) -> bool {
    matches!(
        key,
        "GARDN_PLUGIN_ID"
            | "GARDN_PLUGIN_ENTRYPOINT_ID"
            | "GARDN_PLUGIN_CONTEXT_JSON"
            | "GARDN_PLUGIN_ROOT"
            | "GARDN_PLUGIN_CONFIG_DIR"
            | "GARDN_PLUGIN_STATE_DIR"
            | "GARDN_WORKSPACE_ID"
            | "GARDN_TAB_ID"
            | "GARDN_PANE_ID"
            | "GARDN_BIN_PATH"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::InstalledPluginInfo;

    fn plugin_info(root: &str, id: &str) -> InstalledPluginInfo {
        InstalledPluginInfo {
            plugin_id: id.to_string(),
            name: id.to_string(),
            version: "0.1.0".into(),
            min_gardn_version: "0.2.0".into(),
            description: None,
            manifest_path: format!("{root}/gardn-plugin.toml"),
            plugin_root: root.to_string(),
            enabled: true,
            platforms: None,
            build: Vec::new(),
            startup: Vec::new(),
            actions: Vec::new(),
            events: Vec::new(),
            panes: Vec::new(),
            link_handlers: Vec::new(),
            source: Default::default(),
            warnings: Vec::new(),
        }
    }

    fn local_location() -> crate::execution_host::ResourceLocation {
        crate::execution_host::ResourceLocation::local("/tmp").expect("local")
    }

    fn remote_location() -> crate::execution_host::ResourceLocation {
        crate::execution_host::ResourceLocation::new(
            crate::execution_host::ExecutionHostId::new("ssh:plugin-reject").unwrap(),
            crate::execution_host::HostPath::new("/srv/project").unwrap(),
        )
    }

    #[test]
    fn local_relative_script_commands_keep_plugin_root_cwd() {
        // bun/node/lua/bash relative scripts share the same local cwd policy.
        for (label, _script) in [
            ("bun", "bootstrap.ts"),
            ("node", "toggle.mjs"),
            ("lua", "setup.lua"),
            ("bash", "open.sh"),
        ] {
            let plugin = plugin_info(&format!("/plugins/{label}"), &format!("example.{label}"));
            let plan = resolve_plugin_pane_launch(&plugin, None, &local_location()).unwrap();
            assert_eq!(plan.cwd, PathBuf::from(&plugin.plugin_root), "{label}");
            // Never the remote project path.
            assert_ne!(plan.cwd, PathBuf::from("/srv/project"), "{label}");

            // Prior local override semantics: raw PathBuf, not joined/canonicalized.
            let plan =
                resolve_plugin_pane_launch(&plugin, Some("/anywhere/else"), &local_location())
                    .unwrap();
            assert_eq!(plan.cwd, PathBuf::from("/anywhere/else"), "{label}");
            let plan = resolve_plugin_pane_launch(&plugin, Some("rel"), &local_location()).unwrap();
            assert_eq!(plan.cwd, PathBuf::from("rel"), "{label}");
        }
    }

    #[test]
    fn remote_rejects_before_create_for_all_relative_runtimes() {
        for (label, _script) in [
            ("bun", "bootstrap.ts"),
            ("node", "toggle.mjs"),
            ("lua", "setup.lua"),
            ("bash", "open.sh"),
        ] {
            let plugin = plugin_info(
                &format!("/plugins/{label}"),
                &format!("example.remote-{label}"),
            );
            let err = resolve_plugin_pane_launch(&plugin, None, &remote_location()).unwrap_err();
            assert_eq!(err.0, "unsupported_execution_host", "{label}");
            assert!(err.1.contains("cannot run on remote"), "{label}: {}", err.1);
            // Even with an override that looks like the project path, remote is rejected.
            let err = resolve_plugin_pane_launch(&plugin, Some("/srv/project"), &remote_location())
                .unwrap_err();
            assert_eq!(err.0, "unsupported_execution_host", "{label}");
        }
    }

    #[test]
    fn all_pane_variants_share_remote_rejection() {
        let plugin = plugin_info("/plugins/variants", "example.variants");
        for _placement in ["overlay", "split", "tab", "zoomed"] {
            let err = resolve_plugin_pane_launch(&plugin, None, &remote_location()).unwrap_err();
            assert_eq!(err.0, "unsupported_execution_host");
        }
        let local = resolve_plugin_pane_launch(&plugin, None, &local_location()).unwrap();
        assert_eq!(local.cwd, PathBuf::from(&plugin.plugin_root));
    }
}
