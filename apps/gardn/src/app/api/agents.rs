use std::time::Duration;

use bytes::Bytes;

use crate::api::schema::{
    AgentPromptParams, AgentRenameParams, AgentSendKeysParams, AgentStartParams, AgentTarget,
    PaneReadResult, ResponseResult,
};
use crate::app::App;

use super::responses::{encode_error, encode_error_body, encode_success};

const AGENT_PROMPT_SUBMIT_DELAY: Duration = Duration::from_millis(300);

impl App {
    pub(super) fn handle_agent_list(&mut self, id: String) -> String {
        encode_success(
            id,
            ResponseResult::AgentList {
                agents: self.collect_agent_infos(),
            },
        )
    }

    pub(super) fn handle_agent_get(&mut self, id: String, target: AgentTarget) -> String {
        let agent = match self.agent_info_for_target(&target.target) {
            Ok(agent) => agent,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };

        encode_success(id, ResponseResult::AgentInfo { agent })
    }

    pub(super) fn handle_agent_focus(&mut self, id: String, target: AgentTarget) -> String {
        let agent = match self.focus_agent_target(&target.target) {
            Ok(agent) => agent,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };

        encode_success(id, ResponseResult::AgentInfo { agent })
    }

    pub(super) fn handle_agent_follow_up_add(&mut self, id: String, target: AgentTarget) -> String {
        let resolved = match self.resolve_agent_target(&target.target) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };
        self.state
            .insert_agent_follow_up(resolved.ws_idx, resolved.pane_id);
        let agent = match self.agent_info(resolved.ws_idx, resolved.pane_id) {
            Some(agent) => agent,
            None => return agent_not_found(id, &target.target),
        };
        encode_success(id, ResponseResult::AgentInfo { agent })
    }

    pub(super) fn handle_agent_follow_up_remove(
        &mut self,
        id: String,
        target: AgentTarget,
    ) -> String {
        let resolved = match self.resolve_agent_target(&target.target) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };
        let workspace_id = match self.state.workspaces.get(resolved.ws_idx) {
            Some(workspace) => workspace.id.clone(),
            None => return agent_not_found(id, &target.target),
        };
        self.state
            .clear_agent_follow_up_for_pane(&workspace_id, resolved.pane_id);
        let agent = match self.agent_info(resolved.ws_idx, resolved.pane_id) {
            Some(agent) => agent,
            None => return agent_not_found(id, &target.target),
        };
        encode_success(id, ResponseResult::AgentInfo { agent })
    }

    pub(super) fn handle_agent_rename(&mut self, id: String, params: AgentRenameParams) -> String {
        let agent = match self.rename_agent_target(&params.target, params.name) {
            Ok(agent) => agent,
            Err(err) => return encode_error_body(id, self.agent_rename_error_body(err)),
        };

        encode_success(id, ResponseResult::AgentInfo { agent })
    }

    pub(super) fn handle_agent_start_disposition(
        &mut self,
        id: String,
        params: AgentStartParams,
    ) -> crate::api::ApiRequestDisposition {
        self.handle_agent_start_with(None, id, params)
    }

    pub(super) fn handle_agent_start_disposition_for_view(
        &mut self,
        view: &crate::app::ClientViewState,
        id: String,
        params: AgentStartParams,
    ) -> crate::api::ApiRequestDisposition {
        self.handle_agent_start_with(Some(view), id, params)
    }

    fn handle_agent_start_with(
        &mut self,
        view: Option<&crate::app::ClientViewState>,
        id: String,
        params: AgentStartParams,
    ) -> crate::api::ApiRequestDisposition {
        let argv = params.argv.clone();
        let focus = params.focus;
        let extra_env = match super::env::normalize_launch_env(params.env.clone()) {
            Ok(env) => env,
            Err((code, message)) => {
                return crate::api::ApiRequestDisposition::Respond(encode_error(id, &code, message))
            }
        };
        let outcome = match view {
            Some(view) => self.start_agent_for_view(view, params, extra_env),
            None => self.start_agent(params, extra_env),
        };
        match outcome {
            Ok(crate::app::agents::AgentStartOutcome::Committed { agent, argv }) => {
                crate::api::ApiRequestDisposition::Respond(encode_success(
                    id,
                    ResponseResult::AgentStarted {
                        agent: *agent,
                        argv,
                    },
                ))
            }
            Ok(crate::app::agents::AgentStartOutcome::Pending(terminal_id)) => {
                crate::api::ApiRequestDisposition::Deferred(crate::api::DeferredRemoteCreate {
                    terminal_id,
                    request_id: id,
                    kind: crate::api::DeferredRemoteCreateKind::AgentStart { argv },
                    focus,
                    client_view_id: view.map(|view| view.id()),
                    // Agent start may stamp tab/workspace markers via its own path;
                    // failure cleanup is exact only when a marker was installed.
                    pending_focus: None,
                })
            }
            Err(err) => crate::api::ApiRequestDisposition::Respond(encode_error_body(
                id,
                self.agent_start_error_body(err),
            )),
        }
    }

    pub(super) fn handle_agent_prompt(&mut self, id: String, params: AgentPromptParams) -> String {
        if params.text.is_empty() {
            return encode_error(id, "empty_agent_prompt", "agent prompt must not be empty");
        }
        let resolved = match self.resolve_agent_target(&params.target) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };
        let Some(terminal_id) = self
            .state
            .workspaces
            .get(resolved.ws_idx)
            .and_then(|workspace| workspace.terminal_id(resolved.pane_id))
            .cloned()
        else {
            return agent_not_found(id, &params.target);
        };
        let Some(terminal) = self.state.terminals.get(&terminal_id) else {
            return agent_not_found(id, &params.target);
        };
        if terminal.state == crate::detect::AgentState::Blocked {
            return encode_error(
                id,
                "agent_blocked",
                format!(
                    "agent {} is blocked and requires interactive input",
                    params.target
                ),
            );
        }
        let Some(expected_agent) = terminal.effective_known_agent() else {
            return agent_not_ready(id, &params.target);
        };
        let Some(runtime) = self.lookup_runtime_sender(resolved.ws_idx, resolved.pane_id) else {
            return agent_not_found(id, &params.target);
        };
        if !super::super::agents::runtime_hosts_agent(runtime, expected_agent) {
            return agent_not_ready(id, &params.target);
        }
        if expected_agent == crate::detect::Agent::GithubCopilot {
            // Copilot ignores synthetic Enter after focus loss until it receives focus gained.
            let focus = match crate::ghostty::encode_focus(crate::ghostty::FocusEvent::Gained) {
                Ok(focus) => focus,
                Err(err) => return encode_error(id, "agent_prompt_failed", err.to_string()),
            };
            if let Err(err) = runtime.try_send_bytes(Bytes::from(focus)) {
                return encode_error(id, "agent_prompt_failed", err.to_string());
            }
        }
        let (text, enter) =
            super::super::api_helpers::encode_api_submission_parts(runtime, &params.text);
        if let Err(err) = runtime.try_send_bytes(Bytes::from(text)) {
            return encode_error(id, "agent_prompt_failed", err.to_string());
        }
        runtime.send_bytes_after(Bytes::from(enter), AGENT_PROMPT_SUBMIT_DELAY);
        let Some(agent) = self.agent_info(resolved.ws_idx, resolved.pane_id) else {
            return agent_not_found(id, &params.target);
        };
        encode_success(id, ResponseResult::AgentPrompted { agent })
    }

    pub(super) fn handle_agent_read(
        &mut self,
        id: String,
        params: crate::api::schema::AgentReadParams,
    ) -> String {
        let resolved = match self.resolve_terminal_target(&params.target) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };
        let Some((pane, workspace_id)) = self.lookup_runtime(resolved.ws_idx, resolved.pane_id)
        else {
            return agent_not_found(id, &params.target);
        };
        let snapshot = crate::app::api_helpers::read_terminal_snapshot(
            pane,
            params.source,
            params.format,
            params.lines,
        );

        encode_success(
            id,
            ResponseResult::PaneRead {
                read: PaneReadResult {
                    pane_id: self
                        .public_pane_id(resolved.ws_idx, resolved.pane_id)
                        .unwrap_or_else(|| params.target.clone()),
                    workspace_id,
                    tab_id: self
                        .public_tab_id(resolved.ws_idx, resolved.tab_idx)
                        .unwrap(),
                    source: params.source,
                    format: params.format,
                    text: snapshot.text,
                    revision: 0,
                    truncated: snapshot.truncated,
                },
            },
        )
    }

    pub(super) fn handle_agent_explain(&mut self, id: String, target: AgentTarget) -> String {
        let resolved = match self.resolve_terminal_target(&target.target) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };
        let Some((pane, _workspace_id)) = self.lookup_runtime(resolved.ws_idx, resolved.pane_id)
        else {
            return agent_not_found(id, &target.target);
        };
        let Some(terminal_id) = self
            .state
            .workspaces
            .get(resolved.ws_idx)
            .and_then(|workspace| workspace.terminal_id(resolved.pane_id))
        else {
            return agent_not_found(id, &target.target);
        };
        let Some(terminal) = self.state.terminals.get(terminal_id) else {
            return agent_not_found(id, &target.target);
        };
        if terminal.full_lifecycle_hook_authority_active() {
            let explain = serde_json::json!({
                "agent": terminal.effective_agent_label().unwrap_or("unknown"),
                "state": crate::detect::manifest::agent_state_label(terminal.state),
                "manifest_source": null,
                "manifest_version": null,
                "cached_remote_version": null,
                "local_override_shadowing_remote": false,
                "remote_update_status": null,
                "remote_update_error": null,
                "matched_rule": null,
                "visible_idle": false,
                "visible_blocker": false,
                "visible_working": false,
                "screen_detection_skipped": true,
                "screen_detection_skip_reason": "full_lifecycle_hook_authority",
                "skip_state_update": false,
                "skipped_update_reason": null,
                "fallback_reason": null,
                "warning": null,
                "evaluated_rules": [],
            });
            return encode_success(id, ResponseResult::AgentExplain { explain });
        }
        let Some(agent) = terminal.effective_known_agent().or(terminal.detected_agent) else {
            return encode_error(
                id,
                "agent_explain_unavailable",
                format!(
                    "agent target {} does not have a detected agent label",
                    target.target
                ),
            );
        };

        let explain = crate::detect::manifest::explain_with_input(
            agent,
            crate::detect::manifest::DetectionInput {
                screen: &pane.detection_text(),
                osc_title: &pane.agent_osc_title(),
                osc_progress: &pane.agent_osc_progress(),
            },
        );
        let value = crate::detect::manifest::explain_to_json_value(&explain);

        encode_success(id, ResponseResult::AgentExplain { explain: value })
    }

    pub(super) fn handle_agent_send_keys(
        &mut self,
        id: String,
        params: AgentSendKeysParams,
    ) -> String {
        let resolved = match self.resolve_agent_target(&params.target) {
            Ok(resolved) => resolved,
            Err(err) => return encode_error_body(id, self.agent_target_error_body(err)),
        };
        let Some(terminal_id) = self
            .state
            .workspaces
            .get(resolved.ws_idx)
            .and_then(|workspace| workspace.terminal_id(resolved.pane_id))
            .cloned()
        else {
            return agent_not_found(id, &params.target);
        };
        let Some(expected_agent) = self
            .state
            .terminals
            .get(&terminal_id)
            .and_then(|terminal| terminal.effective_known_agent())
        else {
            return agent_not_ready(id, &params.target);
        };
        let Some(runtime) = self.lookup_runtime_sender(resolved.ws_idx, resolved.pane_id) else {
            return agent_not_found(id, &params.target);
        };
        if !super::super::agents::runtime_hosts_agent(runtime, expected_agent) {
            return agent_not_ready(id, &params.target);
        }
        let encoded_keys = match super::super::api_helpers::encode_api_keys(runtime, &params.keys) {
            Ok(encoded_keys) => encoded_keys,
            Err(key) => return encode_error(id, "invalid_key", format!("unsupported key {key}")),
        };
        let bytes: Vec<u8> = encoded_keys.into_iter().flatten().collect();
        if let Err(err) = runtime.try_send_bytes(Bytes::from(bytes)) {
            return encode_error(id, "agent_send_keys_failed", err.to_string());
        }

        encode_success(id, ResponseResult::Ok {})
    }
}

fn agent_not_found(id: String, target: &str) -> String {
    encode_error(
        id,
        "agent_not_found",
        format!("agent target {target} not found"),
    )
}
fn agent_not_ready(id: String, target: &str) -> String {
    encode_error(
        id,
        "agent_not_ready",
        format!("agent target {target} is not ready for input"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::schema::{AgentStatus, SuccessResponse},
        config::Config,
        workspace::Workspace,
    };

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        );
        app.state.workspaces = vec![Workspace::test_new("agents")];
        app.state.ensure_test_terminals();
        app
    }

    #[test]
    fn agent_list_includes_follow_up_from_session_state() {
        let mut app = test_app();
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0]
            .pane_state(pane_id)
            .expect("root pane")
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&terminal_id)
            .expect("terminal")
            .agent_name = Some("omp".into());
        assert!(app.state.insert_agent_follow_up(0, pane_id));
        app.state.agent_follow_up[0].added_at_unix_secs = 1_700_000_000;

        let response = app.handle_agent_list("list".into());
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::AgentList { agents } = success.result else {
            panic!("expected agent list");
        };
        assert_eq!(agents.len(), 1);
        assert!(agents[0].follow_up);
        assert_eq!(agents[0].follow_up_added_at_unix_secs, Some(1_700_000_000));
        assert_eq!(agents[0].agent_status, AgentStatus::Unknown);
    }

    #[test]
    fn agent_list_keeps_follow_up_after_agent_identity_is_cleared() {
        let mut app = test_app();
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        assert!(app.state.insert_agent_follow_up(0, pane_id));

        let response = app.handle_agent_list("list".into());
        let success: SuccessResponse = serde_json::from_str(&response).unwrap();
        let ResponseResult::AgentList { agents } = success.result else {
            panic!("expected agent list");
        };
        assert_eq!(agents.len(), 1);
        assert!(agents[0].follow_up);
    }

}
