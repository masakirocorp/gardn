use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Pi,
    Omp,
    Claude,
    Codex,
    Copilot,
    Kimi,
    Droid,
    Opencode,
    Hermes,
    Qodercli,
    Cursor,
    Custom,
}

impl AgentKind {
    pub const ALL: [Self; 12] = [
        Self::Codex,
        Self::Claude,
        Self::Cursor,
        Self::Opencode,
        Self::Copilot,
        Self::Pi,
        Self::Omp,
        Self::Kimi,
        Self::Droid,
        Self::Hermes,
        Self::Qodercli,
        Self::Custom,
    ];

    pub const SYSTEM: [Self; 11] = [
        Self::Codex,
        Self::Claude,
        Self::Cursor,
        Self::Opencode,
        Self::Copilot,
        Self::Pi,
        Self::Omp,
        Self::Kimi,
        Self::Droid,
        Self::Hermes,
        Self::Qodercli,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Kimi => "kimi",
            Self::Droid => "droid",
            Self::Opencode => "opencode",
            Self::Hermes => "hermes",
            Self::Qodercli => "qodercli",
            Self::Cursor => "cursor",
            Self::Custom => "custom",
        }
    }

    pub fn system_command(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Omp => "omp",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Copilot => "copilot",
            Self::Kimi => "kimi",
            Self::Droid => "droid",
            Self::Opencode => "opencode",
            Self::Hermes => "hermes",
            Self::Qodercli => "qoder",
            Self::Cursor => "cursor-agent",
            Self::Custom => "custom",
        }
    }

    pub fn system_id(self) -> String {
        format!("system:{}", self.as_str())
    }

    pub fn is_supported(self) -> bool {
        self != Self::Custom
    }

    pub fn integration_target(self) -> Option<crate::api::schema::IntegrationTarget> {
        match self {
            Self::Pi => Some(crate::api::schema::IntegrationTarget::Pi),
            Self::Omp => Some(crate::api::schema::IntegrationTarget::Omp),
            Self::Claude => Some(crate::api::schema::IntegrationTarget::Claude),
            Self::Codex => Some(crate::api::schema::IntegrationTarget::Codex),
            Self::Copilot => Some(crate::api::schema::IntegrationTarget::Copilot),
            Self::Kimi => Some(crate::api::schema::IntegrationTarget::Kimi),
            Self::Droid => Some(crate::api::schema::IntegrationTarget::Droid),
            Self::Opencode => Some(crate::api::schema::IntegrationTarget::Opencode),
            Self::Hermes => Some(crate::api::schema::IntegrationTarget::Hermes),
            Self::Qodercli => Some(crate::api::schema::IntegrationTarget::Qodercli),
            Self::Cursor => Some(crate::api::schema::IntegrationTarget::Cursor),
            Self::Custom => None,
        }
    }
}

impl From<crate::api::schema::IntegrationTarget> for AgentKind {
    fn from(value: crate::api::schema::IntegrationTarget) -> Self {
        match value {
            crate::api::schema::IntegrationTarget::Pi => Self::Pi,
            crate::api::schema::IntegrationTarget::Omp => Self::Omp,
            crate::api::schema::IntegrationTarget::Claude => Self::Claude,
            crate::api::schema::IntegrationTarget::Codex => Self::Codex,
            crate::api::schema::IntegrationTarget::Copilot => Self::Copilot,
            crate::api::schema::IntegrationTarget::Kimi => Self::Kimi,
            crate::api::schema::IntegrationTarget::Droid => Self::Droid,
            crate::api::schema::IntegrationTarget::Opencode => Self::Opencode,
            crate::api::schema::IntegrationTarget::Hermes => Self::Hermes,
            crate::api::schema::IntegrationTarget::Qodercli => Self::Qodercli,
            crate::api::schema::IntegrationTarget::Cursor => Self::Cursor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAgentProfileConfig {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub command: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentProfilesConfig {
    pub order: Vec<String>,
    pub custom: Vec<UserAgentProfileConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProfileSource {
    System,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub command: String,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub enabled: bool,
    pub source: AgentProfileSource,
    pub parse_error: Option<String>,
}

impl AgentProfile {
    pub fn available(&self) -> bool {
        self.enabled && self.parse_error.is_none() && !self.argv.is_empty()
    }

    pub fn is_system(&self) -> bool {
        self.source == AgentProfileSource::System
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfileCatalog {
    profiles: Vec<AgentProfile>,
}

impl AgentProfileCatalog {
    pub fn from_config(config: &AgentProfilesConfig) -> Self {
        let mut profiles = Vec::new();
        for kind in AgentKind::SYSTEM {
            let command = kind.system_command().to_string();
            profiles.push(AgentProfile {
                id: kind.system_id(),
                name: kind.as_str().to_string(),
                kind,
                argv: vec![command.clone()],
                command,
                env: Vec::new(),
                enabled: true,
                source: AgentProfileSource::System,
                parse_error: None,
            });
        }

        for custom in &config.custom {
            let (argv, parse_error) = parse_profile_command(&custom.command);
            profiles.push(AgentProfile {
                id: normalized_user_profile_id(&custom.id),
                name: custom.name.trim().to_string(),
                kind: custom.kind,
                command: custom.command.clone(),
                argv,
                env: custom
                    .env
                    .iter()
                    .filter(|(key, value)| valid_env_key(key) && valid_env_value(value))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                enabled: custom.enabled,
                source: AgentProfileSource::User,
                parse_error,
            });
        }

        let ordered_ids = effective_order_ids(config, &profiles);
        profiles.sort_by_key(|profile| {
            ordered_ids
                .iter()
                .position(|id| id == &profile.id)
                .unwrap_or(usize::MAX)
        });
        Self { profiles }
    }

    pub fn profiles(&self) -> &[AgentProfile] {
        &self.profiles
    }

    pub fn get(&self, id: &str) -> Option<&AgentProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    pub fn group_sections<'a>(
        &'a self,
        favorite_ids: &[String],
    ) -> (Vec<&'a AgentProfile>, Vec<&'a AgentProfile>) {
        let favorites: HashSet<&str> = favorite_ids.iter().map(String::as_str).collect();
        let mut favorite = Vec::new();
        let mut available = Vec::new();
        for profile in &self.profiles {
            if !profile.enabled {
                continue;
            }
            if favorites.contains(profile.id.as_str()) {
                favorite.push(profile);
            } else {
                available.push(profile);
            }
        }
        (favorite, available)
    }
}

fn effective_order_ids(config: &AgentProfilesConfig, profiles: &[AgentProfile]) -> Vec<String> {
    let mut ids = Vec::new();
    let known: HashSet<&str> = profiles.iter().map(|profile| profile.id.as_str()).collect();
    for id in &config.order {
        if known.contains(id.as_str()) && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    for profile in profiles {
        if !ids.contains(&profile.id) {
            ids.push(profile.id.clone());
        }
    }
    ids
}

fn parse_profile_command(command: &str) -> (Vec<String>, Option<String>) {
    match shell_words::split(command) {
        Ok(argv) if !argv.is_empty() => (argv, None),
        Ok(_) => (Vec::new(), Some("command is empty".to_string())),
        Err(err) => (Vec::new(), Some(err.to_string())),
    }
}

fn normalized_user_profile_id(id: &str) -> String {
    let trimmed = id.trim();
    if trimmed.starts_with("user:") {
        trimmed.to_string()
    } else {
        format!("user:{trimmed}")
    }
}

fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn valid_env_value(value: &str) -> bool {
    !value.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_system_profiles_start_with_popular_agents() {
        let catalog = AgentProfileCatalog::from_config(&AgentProfilesConfig::default());

        assert_eq!(
            catalog
                .profiles()
                .iter()
                .take(3)
                .map(|profile| profile.id.as_str())
                .collect::<Vec<_>>(),
            ["system:codex", "system:claude", "system:cursor"]
        );
    }

    #[test]
    fn catalog_layers_system_and_custom_profiles_in_config_order() {
        let config = AgentProfilesConfig {
            order: vec!["user:omp-mk".into(), "system:codex".into()],
            custom: vec![UserAgentProfileConfig {
                id: "omp-mk".into(),
                name: "omp mk".into(),
                kind: AgentKind::Omp,
                command: "omp-mk --profile main".into(),
                env: BTreeMap::new(),
                enabled: true,
            }],
        };

        let catalog = AgentProfileCatalog::from_config(&config);
        assert_eq!(catalog.profiles()[0].id, "user:omp-mk");
        assert_eq!(catalog.profiles()[0].argv, ["omp-mk", "--profile", "main"]);
        assert_eq!(catalog.profiles()[1].id, "system:codex");
        assert!(catalog.get("system:omp").unwrap().is_system());
        assert!(catalog.get("system:custom").is_none());
    }

    #[test]
    fn group_sections_promote_favorites_without_changing_global_order() {
        let config = AgentProfilesConfig {
            order: vec![
                "system:codex".into(),
                "system:omp".into(),
                "system:claude".into(),
            ],
            custom: Vec::new(),
        };
        let catalog = AgentProfileCatalog::from_config(&config);

        let (favorites, available) = catalog.group_sections(&["system:omp".into()]);

        assert_eq!(
            favorites.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["system:omp"]
        );
        assert_eq!(available[0].id, "system:codex");
        assert_eq!(available[1].id, "system:claude");
    }
}
