use std::{io, path::Path};

use serde::{Deserialize, Serialize};

use crate::agent_profiles::{AgentKind, AgentProfileCatalog};
use crate::api::schema::IntegrationTarget;

use super::{IntegrationStatusKind, INSTALL_WARNING_PREFIX};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfileIntegrationContext {
    pub kind: AgentKind,
    pub command_name: Option<String>,
    pub codex_home: Option<String>,
}

impl ProfileIntegrationContext {
    pub(crate) fn from_catalog(catalog: &AgentProfileCatalog) -> Vec<Self> {
        catalog
            .profiles()
            .iter()
            .filter(|profile| {
                profile.enabled
                    && profile.kind.integration_target() == Some(IntegrationTarget::Codex)
            })
            .map(|profile| Self {
                kind: profile.kind,
                command_name: profile.argv.first().map(|command| {
                    Path::new(command)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(command)
                        .to_string()
                }),
                codex_home: profile
                    .env
                    .iter()
                    .find(|(key, _)| key == "CODEX_HOME")
                    .map(|(_, value)| value.clone()),
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HostIntegrationOperation {
    Inspect,
    EnsureCurrent { target: IntegrationTarget },
    UninstallOwned { target: IntegrationTarget },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostIntegrationRequest {
    pub operation: HostIntegrationOperation,
    pub profiles: Vec<ProfileIntegrationContext>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WorkerHookReport {
    Agent(WorkerAgentReport),
    Session(WorkerAgentSessionReport),
    Release(WorkerAgentReleaseReport),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkerAgentReport {
    pub source: String,
    pub agent: String,
    pub state: crate::api::schema::PaneAgentState,
    pub message: Option<String>,
    pub custom_status: Option<String>,
    pub seq: Option<u64>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub launch_env: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkerAgentSessionReport {
    pub source: String,
    pub agent: String,
    pub seq: Option<u64>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub session_start_source: Option<String>,
    pub launch_env: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WorkerAgentReleaseReport {
    pub source: String,
    pub agent: String,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub seq: Option<u64>,
}

impl From<crate::api::schema::PaneReportAgentParams> for WorkerAgentReport {
    fn from(params: crate::api::schema::PaneReportAgentParams) -> Self {
        Self {
            source: params.source,
            agent: params.agent,
            state: params.state,
            message: params.message,
            custom_status: params.custom_status,
            seq: params.seq,
            agent_session_id: params.agent_session_id,
            agent_session_path: params.agent_session_path,
            launch_env: params.launch_env,
        }
    }
}

impl From<crate::api::schema::PaneReportAgentSessionParams> for WorkerAgentSessionReport {
    fn from(params: crate::api::schema::PaneReportAgentSessionParams) -> Self {
        Self {
            source: params.source,
            agent: params.agent,
            seq: params.seq,
            agent_session_id: params.agent_session_id,
            agent_session_path: params.agent_session_path,
            session_start_source: params.session_start_source,
            launch_env: params.launch_env,
        }
    }
}

impl From<crate::api::schema::PaneReleaseAgentParams> for WorkerAgentReleaseReport {
    fn from(params: crate::api::schema::PaneReleaseAgentParams) -> Self {
        Self {
            source: params.source,
            agent: params.agent,
            agent_session_id: params.agent_session_id,
            agent_session_path: params.agent_session_path,
            seq: params.seq,
        }
    }
}

impl WorkerAgentReport {
    pub(crate) fn into_params(self, pane_id: String) -> crate::api::schema::PaneReportAgentParams {
        crate::api::schema::PaneReportAgentParams {
            pane_id,
            source: self.source,
            agent: self.agent,
            state: self.state,
            message: self.message,
            custom_status: self.custom_status,
            seq: self.seq,
            agent_session_id: self.agent_session_id,
            agent_session_path: self.agent_session_path,
            launch_env: self.launch_env,
        }
    }
}

impl WorkerAgentSessionReport {
    pub(crate) fn into_params(
        self,
        pane_id: String,
    ) -> crate::api::schema::PaneReportAgentSessionParams {
        crate::api::schema::PaneReportAgentSessionParams {
            pane_id,
            source: self.source,
            agent: self.agent,
            seq: self.seq,
            agent_session_id: self.agent_session_id,
            agent_session_path: self.agent_session_path,
            session_start_source: self.session_start_source,
            launch_env: self.launch_env,
        }
    }
}

impl WorkerAgentReleaseReport {
    pub(crate) fn into_params(self, pane_id: String) -> crate::api::schema::PaneReleaseAgentParams {
        crate::api::schema::PaneReleaseAgentParams {
            pane_id,
            source: self.source,
            agent: self.agent,
            agent_session_id: self.agent_session_id,
            agent_session_path: self.agent_session_path,
            seq: self.seq,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostIntegrationEntry {
    pub target: IntegrationTarget,
    pub available: bool,
    pub state: IntegrationStatusKind,
    pub missing_profile_hooks: usize,
}

impl HostIntegrationEntry {
    pub(crate) fn status_label(&self) -> &'static str {
        match (self.available, self.state, self.missing_profile_hooks) {
            (_, IntegrationStatusKind::Current, count) if count > 0 => "Profile Hooks Missing",
            (_, IntegrationStatusKind::Current, _) => "Installed",
            (_, IntegrationStatusKind::Outdated, _) => "Update Available",
            (true, IntegrationStatusKind::NotInstalled, _) => "Available",
            (false, IntegrationStatusKind::NotInstalled, _) => "Not Found",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostIntegrationSnapshot {
    pub entries: Vec<HostIntegrationEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostIntegrationObservation {
    Pending,
    Ready(HostIntegrationSnapshot),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostIntegrationResult {
    pub snapshot: HostIntegrationSnapshot,
    pub messages: Vec<String>,
}

pub(crate) fn request_for_catalog(
    operation: HostIntegrationOperation,
    catalog: &AgentProfileCatalog,
) -> HostIntegrationRequest {
    HostIntegrationRequest {
        operation,
        profiles: ProfileIntegrationContext::from_catalog(catalog),
    }
}

pub(crate) fn execute(request: &HostIntegrationRequest) -> io::Result<HostIntegrationResult> {
    let messages = match request.operation {
        HostIntegrationOperation::Inspect => Vec::new(),
        HostIntegrationOperation::EnsureCurrent { target } => {
            super::install_target_for_profile_contexts(target, &request.profiles)?
        }
        HostIntegrationOperation::UninstallOwned { target } => {
            super::uninstall_target_for_profile_contexts(target, &request.profiles)?
        }
    };
    let snapshot = inspect(&request.profiles);
    Ok(HostIntegrationResult { snapshot, messages })
}

pub(crate) fn inspect(profiles: &[ProfileIntegrationContext]) -> HostIntegrationSnapshot {
    let entries = super::integration_recommendations()
        .into_iter()
        .map(|recommendation| HostIntegrationEntry {
            target: recommendation.target,
            available: recommendation.available,
            state: recommendation.state,
            missing_profile_hooks: super::missing_profile_hook_count_for_contexts(
                recommendation.target,
                profiles,
            ),
        })
        .collect();
    HostIntegrationSnapshot { entries }
}

pub(crate) fn operation_failure_message(error: &io::Error) -> String {
    if error.to_string().starts_with(INSTALL_WARNING_PREFIX) {
        error.to_string()
    } else {
        format!("integration operation failed: {error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_profile_context_omits_arguments_and_unrelated_environment() {
        let mut codex_env = std::collections::BTreeMap::new();
        codex_env.insert("CODEX_HOME".to_string(), "/remote/codex-home".to_string());
        codex_env.insert("API_TOKEN".to_string(), "secret-codex-token".to_string());
        let mut omp_env = std::collections::BTreeMap::new();
        omp_env.insert(
            "AWS_SECRET_ACCESS_KEY".to_string(),
            "secret-aws-key".to_string(),
        );
        let catalog =
            AgentProfileCatalog::from_config(&crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-work".into(), "user:omp-work".into()],
                custom: vec![
                    crate::agent_profiles::UserAgentProfileConfig {
                        id: "codex-work".into(),
                        name: "Codex work".into(),
                        kind: AgentKind::Codex,
                        command: "/usr/local/bin/codex-work --profile production-secret".into(),
                        env: codex_env,
                        enabled: true,
                    },
                    crate::agent_profiles::UserAgentProfileConfig {
                        id: "omp-work".into(),
                        name: "OMP work".into(),
                        kind: AgentKind::Omp,
                        command: "omp --profile secret".into(),
                        env: omp_env,
                        enabled: true,
                    },
                ],
            });

        let contexts = ProfileIntegrationContext::from_catalog(&catalog);
        let encoded = serde_json::to_string(&contexts).expect("serialize profile contexts");

        assert!(contexts
            .iter()
            .all(|context| context.kind == AgentKind::Codex));
        assert!(contexts.iter().any(|context| {
            context.command_name.as_deref() == Some("codex-work")
                && context.codex_home.as_deref() == Some("/remote/codex-home")
        }));
        assert!(!encoded.contains("production-secret"), "{encoded}");
        assert!(!encoded.contains("secret-codex-token"), "{encoded}");
        assert!(!encoded.contains("secret-aws-key"), "{encoded}");
        assert!(!encoded.contains("omp-work"), "{encoded}");
    }
}
