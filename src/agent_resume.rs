use std::path::Path;

use serde::{Deserialize, Serialize};

const MAX_SESSION_ID_LEN: usize = 512;
const MAX_SESSION_PATH_LEN: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionRef {
    pub kind: AgentSessionRefKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionRefKind {
    Id,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentResumePlan {
    pub agent: String,
    pub argv: Vec<String>,
    pub dedupe_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedAgentSession {
    pub source: String,
    pub agent: String,
    pub session_ref: AgentSessionRef,
}

impl AgentSessionRef {
    pub fn id(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        valid_session_id(&value).then_some(Self {
            kind: AgentSessionRefKind::Id,
            value,
        })
    }

    pub fn path(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        valid_session_path(&value).then_some(Self {
            kind: AgentSessionRefKind::Path,
            value,
        })
    }
}

pub fn session_ref_from_report(
    source: &str,
    agent: &str,
    agent_session_id: Option<String>,
    _agent_session_path: Option<String>,
) -> Option<AgentSessionRef> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    if matches!(agent, "pi" | "omp") {
        return _agent_session_path
            .and_then(AgentSessionRef::path)
            .or_else(|| agent_session_id.and_then(AgentSessionRef::id));
    }

    agent_session_id.and_then(AgentSessionRef::id)
}

pub fn is_reserved_native_state_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("hako:claude", "claude") | ("hako:codex", "codex")
    )
}

pub fn session_ref_from_snapshot(
    source: &str,
    agent: &str,
    kind: AgentSessionRefKind,
    value: &str,
) -> Option<PersistedAgentSession> {
    if !is_official_agent_source(source, agent) {
        return None;
    }
    let session_ref = match (agent, kind) {
        ("pi" | "omp", AgentSessionRefKind::Path) => AgentSessionRef::path(value)?,
        (_, AgentSessionRefKind::Id) => AgentSessionRef::id(value)?,
        _ => return None,
    };
    Some(PersistedAgentSession {
        source: source.to_string(),
        agent: agent.to_string(),
        session_ref,
    })
}

pub fn plan(source: &str, agent: &str, session_ref: &AgentSessionRef) -> Option<AgentResumePlan> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    let argv = match (source, agent, session_ref.kind) {
        ("hako:claude", "claude", AgentSessionRefKind::Id) => {
            vec![
                "claude".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("hako:codex", "codex", AgentSessionRefKind::Id) => {
            vec!["codex".into(), "resume".into(), session_ref.value.clone()]
        }
        ("hako:copilot", "copilot", AgentSessionRefKind::Id) => {
            vec!["copilot".into(), format!("--resume={}", session_ref.value)]
        }
        ("hako:pi", "pi", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            vec!["pi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("hako:omp", "omp", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            vec!["omp".into(), "--session".into(), session_ref.value.clone()]
        }
        ("hako:hermes", "hermes", AgentSessionRefKind::Id) => {
            vec![
                "hermes".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("hako:opencode", "opencode", AgentSessionRefKind::Id) => {
            vec![
                "opencode".into(),
                "--session".into(),
                session_ref.value.clone(),
            ]
        }
        _ => return None,
    };

    Some(AgentResumePlan {
        agent: agent.to_string(),
        argv,
        dedupe_key: dedupe_key(source, agent, session_ref),
    })
}

pub fn plan_with_launch_argv(
    source: &str,
    agent: &str,
    session_ref: &AgentSessionRef,
    launch_argv: Option<&[String]>,
) -> Option<AgentResumePlan> {
    let mut plan = plan(source, agent, session_ref)?;
    if let Some(command) = launch_argv
        .and_then(|argv| argv.first())
        .filter(|command| valid_launch_command(command))
        .cloned()
        .or_else(|| inferred_launch_command(source, agent, session_ref))
    {
        if let Some(planned_command) = plan.argv.first_mut() {
            *planned_command = command;
        }
    }
    Some(plan)
}

fn inferred_launch_command(
    source: &str,
    agent: &str,
    session_ref: &AgentSessionRef,
) -> Option<String> {
    if !matches!(
        (source, agent, session_ref.kind),
        ("hako:omp", "omp", AgentSessionRefKind::Path)
    ) {
        return None;
    }

    let home = home_dir()?;
    let prefix = Path::new(&home);
    if !Path::new(&session_ref.value).starts_with(prefix) {
        return None;
    }

    let profile_dir = Path::new(&session_ref.value)
        .strip_prefix(prefix)
        .ok()?
        .components()
        .next()?
        .as_os_str()
        .to_str()?;
    let suffix = profile_dir.strip_prefix(".omp")?;
    if suffix.is_empty() {
        return Some("omp".to_string());
    }
    if suffix.starts_with('-')
        && suffix.len() > 1
        && suffix[1..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Some(format!("omp{suffix}"));
    }

    None
}

pub fn dedupe_key(source: &str, agent: &str, session_ref: &AgentSessionRef) -> String {
    format!(
        "{source}\u{0}{agent}\u{0}{:?}\u{0}{}",
        session_ref.kind, session_ref.value
    )
}

fn is_official_agent_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("hako:claude", "claude")
            | ("hako:codex", "codex")
            | ("hako:copilot", "copilot")
            | ("hako:pi", "pi")
            | ("hako:omp", "omp")
            | ("hako:hermes", "hermes")
            | ("hako:opencode", "opencode")
    )
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SESSION_ID_LEN && !value.chars().any(char::is_control)
}

fn valid_session_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_PATH_LEN
        && !value.chars().any(char::is_control)
        && Path::new(value).is_absolute()
}

fn valid_launch_command(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_allows_supported_agents() {
        assert_eq!(
            plan(
                "hako:claude",
                "claude",
                &AgentSessionRef::id("claude-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["claude", "--resume", "claude-session"]
        );
        assert_eq!(
            plan(
                "hako:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["codex", "resume", "codex-session"]
        );
        assert_eq!(
            plan(
                "hako:copilot",
                "copilot",
                &AgentSessionRef::id("copilot-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["copilot", "--resume=copilot-session"]
        );
        assert_eq!(
            plan(
                "hako:pi",
                "pi",
                &AgentSessionRef::path("/tmp/pi-session.jsonl").unwrap()
            )
            .unwrap()
            .argv,
            vec!["pi", "--session", "/tmp/pi-session.jsonl"]
        );
        assert_eq!(
            plan(
                "hako:omp",
                "omp",
                &AgentSessionRef::path("/tmp/omp-session.jsonl").unwrap()
            )
            .unwrap()
            .argv,
            vec!["omp", "--session", "/tmp/omp-session.jsonl"]
        );
        assert_eq!(
            plan(
                "hako:hermes",
                "hermes",
                &AgentSessionRef::id("hermes-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["hermes", "--resume", "hermes-session"]
        );
        assert_eq!(
            plan(
                "hako:opencode",
                "opencode",
                &AgentSessionRef::id("opencode-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["opencode", "--session", "opencode-session"]
        );
    }

    #[test]
    fn planner_preserves_launch_command_alias_for_resume() {
        let opencode_ref = AgentSessionRef::id("opencode-session").unwrap();
        assert_eq!(
            plan_with_launch_argv(
                "hako:opencode",
                "opencode",
                &opencode_ref,
                Some(&["oc-frs".to_string()])
            )
            .unwrap()
            .argv,
            vec!["oc-frs", "--session", "opencode-session"]
        );

        let omp_ref = AgentSessionRef::path("/tmp/omp-session.jsonl").unwrap();
        assert_eq!(
            plan_with_launch_argv("hako:omp", "omp", &omp_ref, Some(&["omp-mk".to_string()]))
                .unwrap()
                .argv,
            vec!["omp-mk", "--session", "/tmp/omp-session.jsonl"]
        );
    }

    #[test]
    fn planner_infers_omp_profile_command_from_session_path() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path = format!("{home}/.omp-mk/agent/sessions/project/session.jsonl");
        let omp_profile_ref = AgentSessionRef::path(session_path.clone()).unwrap();

        assert_eq!(
            plan_with_launch_argv("hako:omp", "omp", &omp_profile_ref, None)
                .unwrap()
                .argv,
            vec!["omp-mk".to_string(), "--session".to_string(), session_path]
        );
    }

    #[test]
    fn planner_rejects_custom_and_unsupported_path_refs() {
        assert!(plan(
            "custom:claude",
            "claude",
            &AgentSessionRef::id("session").unwrap()
        )
        .is_none());
        assert!(plan(
            "hako:claude",
            "claude",
            &AgentSessionRef::path("/tmp/claude-session").unwrap()
        )
        .is_none());
    }

    #[test]
    fn report_ref_prefers_pi_path_and_validates_values() {
        let session_ref = session_ref_from_report(
            "hako:pi",
            "pi",
            Some("pi-id".into()),
            Some("/tmp/pi-session.jsonl".into()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(session_ref.value, "/tmp/pi-session.jsonl");
        let omp_session_ref = session_ref_from_report(
            "hako:omp",
            "omp",
            Some("omp-id".into()),
            Some("/tmp/omp-session.jsonl".into()),
        )
        .unwrap();
        assert_eq!(omp_session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(omp_session_ref.value, "/tmp/omp-session.jsonl");

        assert!(session_ref_from_report("hako:pi", "pi", Some("bad\nid".into()), None).is_none());
        assert!(
            session_ref_from_report("hako:pi", "pi", None, Some("relative.jsonl".into())).is_none()
        );
        assert!(session_ref_from_report("custom:pi", "pi", Some("pi-id".into()), None).is_none());
        assert!(session_ref_from_report(
            "hako:claude",
            "claude",
            None,
            Some("/tmp/claude-session".into())
        )
        .is_none());
    }

    #[test]
    fn ids_are_data_not_shell_text() {
        let id = "abc; rm -rf /";
        let plan = plan("hako:codex", "codex", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(plan.argv, vec!["codex", "resume", id]);
    }

    #[test]
    fn planner_rejects_path_refs_for_id_only_agents() {
        assert!(plan(
            "hako:hermes",
            "hermes",
            &AgentSessionRef::path("/tmp/hermes-session").unwrap()
        )
        .is_none());
        assert!(plan(
            "hako:opencode",
            "opencode",
            &AgentSessionRef::path("/tmp/opencode-session").unwrap()
        )
        .is_none());
        assert!(plan(
            "hako:copilot",
            "copilot",
            &AgentSessionRef::path("/tmp/copilot-session").unwrap()
        )
        .is_none());
        assert!(session_ref_from_snapshot(
            "hako:hermes",
            "hermes",
            AgentSessionRefKind::Id,
            "hermes-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "hako:opencode",
            "opencode",
            AgentSessionRefKind::Id,
            "opencode-session"
        )
        .is_some());
    }
}
