use std::collections::BTreeMap;
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
    pub env: Vec<(String, String)>,
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

pub fn launch_env_from_report(
    source: &str,
    agent: &str,
    launch_env: BTreeMap<String, String>,
) -> Vec<(String, String)> {
    if !is_official_agent_source(source, agent) {
        return Vec::new();
    }

    let allowed = match (source, agent) {
        ("hako:claude", "claude") => &["CLAUDE_CONFIG_DIR"][..],
        ("hako:codex", "codex") => &["CODEX_HOME"][..],
        ("hako:copilot", "copilot") => &["COPILOT_HOME"][..],
        ("hako:devin", "devin") => &["DEVIN_CONFIG_DIR"][..],
        ("hako:kimi", "kimi") => &["KIMI_CODE_HOME"][..],
        ("hako:cursor", "cursor") => &["CURSOR_CONFIG_DIR"][..],
        ("hako:pi", "pi") | ("hako:omp", "omp") => &["PI_CONFIG_DIR", "PI_CODING_AGENT_DIR"][..],
        ("hako:hermes", "hermes") => &["HERMES_HOME"][..],
        ("hako:opencode", "opencode") => &["OPENCODE_CONFIG", "XDG_DATA_HOME"][..],
        _ => &[],
    };

    allowed
        .iter()
        .filter_map(|key| {
            launch_env
                .get(*key)
                .filter(|value| valid_launch_env_value(value))
                .map(|value| ((*key).to_string(), value.clone()))
        })
        .collect()
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
        ("hako:devin", "devin", AgentSessionRefKind::Id) => {
            vec!["devin".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("hako:droid", "droid", AgentSessionRefKind::Id) => {
            vec!["droid".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("hako:kimi", "kimi", AgentSessionRefKind::Id) => {
            vec!["kimi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("hako:pi", "pi", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            path_agent_resume_argv("pi", session_ref)
        }
        ("hako:omp", "omp", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            path_agent_resume_argv("omp", session_ref)
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
        ("hako:cursor", "cursor", AgentSessionRefKind::Id) => {
            vec![
                "cursor-agent".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        _ => return None,
    };

    Some(AgentResumePlan {
        agent: agent.to_string(),
        argv,
        env: Vec::new(),
        dedupe_key: dedupe_key(source, agent, session_ref),
    })
}

fn path_agent_resume_argv(command: &str, session_ref: &AgentSessionRef) -> Vec<String> {
    let mut argv = vec![
        command.to_string(),
        "--session".to_string(),
        session_ref.value.clone(),
    ];
    if session_ref.kind == AgentSessionRefKind::Path {
        if let Some(session_dir) = canonical_project_session_dir(&session_ref.value) {
            argv.push("--session-dir".to_string());
            argv.push(session_dir);
        }
    }
    argv
}

fn canonical_project_session_dir(session_path: &str) -> Option<String> {
    let path = Path::new(session_path);
    let components = path.components().collect::<Vec<_>>();
    let sessions_idx = components.windows(2).position(|window| {
        window[0].as_os_str() == std::ffi::OsStr::new("agent")
            && window[1].as_os_str() == std::ffi::OsStr::new("sessions")
    })? + 1;
    let project_idx = sessions_idx + 1;
    if components.len() <= project_idx + 1 {
        return None;
    }

    let mut session_dir = std::path::PathBuf::new();
    for component in components.iter().take(project_idx + 1) {
        session_dir.push(component.as_os_str());
    }
    session_dir.to_str().map(str::to_string)
}

pub fn plan_with_launch_context(
    source: &str,
    agent: &str,
    session_ref: &AgentSessionRef,
    launch_argv: Option<&[String]>,
    launch_env: &[(String, String)],
) -> Option<AgentResumePlan> {
    let mut plan = plan_with_launch_argv(source, agent, session_ref, launch_argv)?;
    plan.env = launch_env
        .iter()
        .filter(|(key, value)| valid_launch_env_key(key) && valid_launch_env_value(value))
        .cloned()
        .collect();
    if !plan.env.is_empty() {
        plan.dedupe_key = dedupe_key_with_env(&plan.dedupe_key, &plan.env);
    }
    Some(plan)
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

fn dedupe_key_with_env(base: &str, env: &[(String, String)]) -> String {
    let mut key = base.to_string();
    for (name, value) in env {
        key.push('\0');
        key.push_str(name);
        key.push('=');
        key.push_str(value);
    }
    key
}

fn is_official_agent_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("hako:claude", "claude")
            | ("hako:codex", "codex")
            | ("hako:copilot", "copilot")
            | ("hako:devin", "devin")
            | ("hako:droid", "droid")
            | ("hako:kimi", "kimi")
            | ("hako:pi", "pi")
            | ("hako:omp", "omp")
            | ("hako:hermes", "hermes")
            | ("hako:opencode", "opencode")
            | ("hako:cursor", "cursor")
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

fn valid_launch_env_key(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_launch_env_value(value: &str) -> bool {
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
                "hako:devin",
                "devin",
                &AgentSessionRef::id("devin-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["devin", "--resume", "devin-session"]
        );
        assert_eq!(
            plan(
                "hako:kimi",
                "kimi",
                &AgentSessionRef::id("kimi-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["kimi", "--session", "kimi-session"]
        );
        assert_eq!(
            plan(
                "hako:droid",
                "droid",
                &AgentSessionRef::id("droid-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["droid", "--resume", "droid-session"]
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
        assert_eq!(
            plan(
                "hako:cursor",
                "cursor",
                &AgentSessionRef::id("cursor-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["cursor-agent", "--resume", "cursor-session"]
        );
    }

    #[test]
    fn planner_preserves_launch_command_for_every_resumable_agent() {
        let id_cases = [
            (
                "hako:claude",
                "claude",
                "claude-session",
                "custom-claude",
                vec!["custom-claude", "--resume", "claude-session"],
            ),
            (
                "hako:codex",
                "codex",
                "codex-session",
                "custom-codex",
                vec!["custom-codex", "resume", "codex-session"],
            ),
            (
                "hako:copilot",
                "copilot",
                "copilot-session",
                "custom-copilot",
                vec!["custom-copilot", "--resume=copilot-session"],
            ),
            (
                "hako:devin",
                "devin",
                "devin-session",
                "custom-devin",
                vec!["custom-devin", "--resume", "devin-session"],
            ),
            (
                "hako:hermes",
                "hermes",
                "hermes-session",
                "custom-hermes",
                vec!["custom-hermes", "--resume", "hermes-session"],
            ),
            (
                "hako:opencode",
                "opencode",
                "opencode-session",
                "custom-opencode",
                vec!["custom-opencode", "--session", "opencode-session"],
            ),
        ];

        for (source, agent, session_id, launch_command, expected) in id_cases {
            let session_ref = AgentSessionRef::id(session_id).unwrap();
            assert_eq!(
                plan_with_launch_argv(
                    source,
                    agent,
                    &session_ref,
                    Some(&[launch_command.to_string()])
                )
                .unwrap()
                .argv,
                expected
            );
        }

        let pi_ref = AgentSessionRef::path("/tmp/pi-session.jsonl").unwrap();
        assert_eq!(
            plan_with_launch_argv("hako:pi", "pi", &pi_ref, Some(&["custom-pi".to_string()]))
                .unwrap()
                .argv,
            vec!["custom-pi", "--session", "/tmp/pi-session.jsonl"]
        );

        let omp_ref = AgentSessionRef::path("/tmp/omp-session.jsonl").unwrap();
        assert_eq!(
            plan_with_launch_argv(
                "hako:omp",
                "omp",
                &omp_ref,
                Some(&["custom-omp".to_string()])
            )
            .unwrap()
            .argv,
            vec!["custom-omp", "--session", "/tmp/omp-session.jsonl"]
        );
    }

    #[test]
    fn planner_infers_omp_profile_command_from_session_path() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path = format!("{home}/.omp-profile/agent/sessions/project/session.jsonl");
        let session_dir = format!("{home}/.omp-profile/agent/sessions/project");
        let omp_profile_ref = AgentSessionRef::path(session_path.clone()).unwrap();

        assert_eq!(
            plan_with_launch_argv("hako:omp", "omp", &omp_profile_ref, None)
                .unwrap()
                .argv,
            vec![
                "omp-profile".to_string(),
                "--session".to_string(),
                session_path,
                "--session-dir".to_string(),
                session_dir,
            ]
        );
    }

    #[test]
    fn omp_child_session_restore_keeps_project_session_dir() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path = format!(
            "{home}/.omp-profile/agent/sessions/-projects-masakiro-hako/2026-06-03T17-52-01-399Z_019e8e9d-1b77-7000-875f-206076643bdf/RightSidebarHierarchyReview.jsonl"
        );
        let project_session_dir =
            format!("{home}/.omp-profile/agent/sessions/-projects-masakiro-hako");
        let omp_ref = AgentSessionRef::path(session_path.clone()).unwrap();

        assert_eq!(
            plan_with_launch_argv(
                "hako:omp",
                "omp",
                &omp_ref,
                Some(&["custom-omp".to_string()])
            )
            .unwrap()
            .argv,
            vec![
                "custom-omp".to_string(),
                "--session".to_string(),
                session_path,
                "--session-dir".to_string(),
                project_session_dir,
            ]
        );
    }

    #[test]
    fn planner_preserves_profile_environment_for_manual_starts() {
        let session_ref = AgentSessionRef::id("codex-session").unwrap();
        let env = vec![("CODEX_HOME".to_string(), "/profiles/codex".to_string())];

        let plan =
            plan_with_launch_context("hako:codex", "codex", &session_ref, None, &env).unwrap();

        assert_eq!(plan.argv, vec!["codex", "resume", "codex-session"]);
        assert_eq!(plan.env, env);
        let other_env = vec![("CODEX_HOME".to_string(), "/profiles/other".to_string())];
        let other_plan =
            plan_with_launch_context("hako:codex", "codex", &session_ref, None, &other_env)
                .unwrap();
        assert_ne!(plan.dedupe_key, other_plan.dedupe_key);
    }

    #[test]
    fn launch_env_report_keeps_only_supported_profile_vars() {
        let env = BTreeMap::from([
            ("CODEX_HOME".to_string(), "/profiles/codex".to_string()),
            ("PATH".to_string(), "/bin".to_string()),
            ("OPENCODE_CONFIG".to_string(), "/wrong/tool".to_string()),
        ]);

        assert_eq!(
            launch_env_from_report("hako:codex", "codex", env),
            vec![("CODEX_HOME".to_string(), "/profiles/codex".to_string())]
        );
        let env = BTreeMap::from([
            (
                "DEVIN_CONFIG_DIR".to_string(),
                "/profiles/devin".to_string(),
            ),
            ("PATH".to_string(), "/bin".to_string()),
        ]);
        assert_eq!(
            launch_env_from_report("hako:devin", "devin", env),
            vec![(
                "DEVIN_CONFIG_DIR".to_string(),
                "/profiles/devin".to_string()
            )]
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
        let codex_plan = plan("hako:codex", "codex", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(codex_plan.argv, vec!["codex", "resume", id]);
        let devin_plan = plan("hako:devin", "devin", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(devin_plan.argv, vec!["devin", "--resume", id]);
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
        assert!(plan(
            "hako:devin",
            "devin",
            &AgentSessionRef::path("/tmp/devin-session").unwrap()
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
        assert!(session_ref_from_snapshot(
            "hako:devin",
            "devin",
            AgentSessionRefKind::Id,
            "devin-session"
        )
        .is_some());
    }
}
