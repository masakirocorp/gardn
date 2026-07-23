use crate::api::schema::{AgentViewClearParams, AgentViewSetParams, ResponseResult};
use crate::app::{App, ClientViewState};

use super::responses::{encode_error, encode_success};

impl App {
    pub(super) fn handle_agent_view_set_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        mut params: AgentViewSetParams,
    ) -> String {
        if let Err(message) = crate::app::agent_view::validate_agent_view(&mut params) {
            return encode_error(id, "invalid_agent_view", message);
        }
        if let Some(plugin_id) = params.source.strip_prefix("plugin:") {
            let Some(plugin_id) = super::plugins::normalize_plugin_id(plugin_id) else {
                return encode_error(
                    id,
                    "invalid_agent_view",
                    "plugin-owned agent view source has an invalid plugin id",
                );
            };
            let Some(plugin) = self.state.installed_plugins.get(&plugin_id) else {
                return encode_error(id, "plugin_not_found", "plugin not found");
            };
            if !plugin.enabled {
                return encode_error(id, "plugin_disabled", "plugin is disabled");
            }
        }
        let source = params.source.clone();
        let label = params.label.clone();
        replace_agent_view_override(view, Some(params));
        encode_success(
            id,
            ResponseResult::AgentView {
                active: true,
                source: Some(source),
                label,
            },
        )
    }

    pub(super) fn handle_agent_view_clear_for_view(
        &mut self,
        view: &mut ClientViewState,
        id: String,
        params: AgentViewClearParams,
    ) -> String {
        let source = match params.source {
            Some(source) => match crate::app::agent_view::validate_agent_view_source(&source) {
                Ok(source) => Some(source),
                Err(message) => return encode_error(id, "invalid_agent_view", message),
            },
            None => None,
        };
        if source.as_deref().is_none_or(|source| {
            view.agent_view_override
                .as_ref()
                .is_some_and(|active| active.source == source)
        }) {
            replace_agent_view_override(view, None);
        }
        let active = view.agent_view_override.as_ref();
        encode_success(
            id,
            ResponseResult::AgentView {
                active: active.is_some(),
                source: active.map(|view| view.source.clone()),
                label: active.and_then(|view| view.label.clone()),
            },
        )
    }

    pub(super) fn clear_agent_view_for_source(
        &mut self,
        view: &mut ClientViewState,
        source: &str,
    ) -> bool {
        if view
            .agent_view_override
            .as_ref()
            .is_some_and(|active| active.source == source)
        {
            replace_agent_view_override(view, None);
            true
        } else {
            false
        }
    }

    pub(super) fn handle_agent_view_set(
        &mut self,
        id: String,
        params: AgentViewSetParams,
    ) -> String {
        let mut view = self.default_client_view.clone_reconciled(&self.state);
        let response = self.handle_agent_view_set_for_view(&mut view, id, params);
        self.default_client_view = view;
        response
    }

    pub(super) fn handle_agent_view_clear(
        &mut self,
        id: String,
        params: AgentViewClearParams,
    ) -> String {
        let mut view = self.default_client_view.clone_reconciled(&self.state);
        let response = self.handle_agent_view_clear_for_view(&mut view, id, params);
        self.default_client_view = view;
        response
    }
}

fn replace_agent_view_override(view: &mut ClientViewState, next: Option<AgentViewSetParams>) {
    view.agent_view_override = next;
    view.agent_panel_scroll = 0;
    view.mobile_switcher_scroll = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::{
        AgentViewBuiltinField, AgentViewField, AgentViewFilter, AgentViewValue, Method, Request,
    };
    use crate::config::Config;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    fn working_view(source: &str) -> AgentViewSetParams {
        AgentViewSetParams {
            source: source.into(),
            label: Some("working".into()),
            filter: Some(AgentViewFilter::Eq {
                field: AgentViewField::Builtin(AgentViewBuiltinField::Status),
                value: AgentViewValue::String("working".into()),
            }),
            sort: Vec::new(),
        }
    }

    #[test]
    fn view_set_is_client_local_and_source_guarded_clear() {
        let mut app = test_app();
        let mut first = ClientViewState::from_default_client_state(&app.state);
        let second = ClientViewState::from_default_client_state(&app.state);

        let response = app.handle_api_request_for_view(
            &mut first,
            Request {
                id: "set".into(),
                method: Method::AgentViewSet(working_view("example.views")),
            },
        );
        let response: crate::api::schema::SuccessResponse =
            serde_json::from_str(&response).unwrap();
        assert_eq!(
            response.result,
            ResponseResult::AgentView {
                active: true,
                source: Some("example.views".into()),
                label: Some("working".into()),
            }
        );
        assert!(first.agent_view_override.is_some());
        assert!(second.agent_view_override.is_none());

        app.handle_agent_view_clear_for_view(
            &mut first,
            "wrong".into(),
            AgentViewClearParams {
                source: Some("other.views".into()),
            },
        );
        assert!(first.agent_view_override.is_some());
        app.handle_agent_view_clear_for_view(
            &mut first,
            "right".into(),
            AgentViewClearParams {
                source: Some("example.views".into()),
            },
        );
        assert!(first.agent_view_override.is_none());
    }

    #[test]
    fn invalid_view_does_not_replace_active_view() {
        let mut app = test_app();
        let mut view = ClientViewState::from_default_client_state(&app.state);
        app.handle_agent_view_set_for_view(&mut view, "set".into(), working_view("example.views"));

        let mut invalid = working_view("example.other");
        invalid.filter = Some(AgentViewFilter::Any {
            filters: Vec::new(),
        });
        let response = app.handle_api_request_for_view(
            &mut view,
            Request {
                id: "invalid".into(),
                method: Method::AgentViewSet(invalid),
            },
        );
        let response: crate::api::schema::ErrorResponse = serde_json::from_str(&response).unwrap();
        assert_eq!(response.error.code, "invalid_agent_view");
        assert_eq!(
            view.agent_view_override
                .as_ref()
                .map(|active| active.source.as_str()),
            Some("example.views")
        );
    }
}
