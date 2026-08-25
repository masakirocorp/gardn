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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionRefKind {
    Id,
    Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentResumeCommandResolution {
    External,
    ShellWrapper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentResumePlan {
    pub agent: String,
    pub argv: Vec<String>,
    pub command_resolution: AgentResumeCommandResolution,
    pub preserved_launch_argv: Option<Vec<String>>,
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
    agent_session_path: Option<String>,
) -> Option<AgentSessionRef> {
    if !is_official_agent_source(source, agent) {
        return None;
    }

    if agent == "omp" {
        let path_ref = agent_session_path.and_then(AgentSessionRef::path);
        if path_ref
            .as_ref()
            .is_some_and(|session_ref| Path::new(&session_ref.value).is_file())
        {
            return path_ref;
        }
        return agent_session_id.and_then(AgentSessionRef::id);
    }

    if agent == "pi" {
        return agent_session_path
            .and_then(AgentSessionRef::path)
            .or_else(|| agent_session_id.and_then(AgentSessionRef::id));
    }

    agent_session_id.and_then(AgentSessionRef::id)
}
pub fn normalize_session_start_source(value: Option<String>) -> Option<String> {
    match value.as_deref().map(str::trim) {
        Some(source @ ("startup" | "resume" | "clear" | "compact" | "new" | "fork" | "select")) => {
            Some(source.to_string())
        }
        _ => None,
    }
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
        ("gardn:claude", "claude") => &["CLAUDE_CONFIG_DIR"][..],
        ("gardn:codex", "codex") => &["CODEX_HOME"][..],
        ("gardn:copilot", "copilot") => &["COPILOT_HOME"][..],
        ("gardn:devin", "devin") => &["DEVIN_CONFIG_DIR"][..],
        ("gardn:kimi", "kimi") => &["KIMI_CODE_HOME"][..],
        ("gardn:cursor", "cursor") => &["CURSOR_CONFIG_DIR"][..],
        ("gardn:pi", "pi") | ("gardn:omp", "omp") => &["PI_CONFIG_DIR", "PI_CODING_AGENT_DIR"][..],
        ("gardn:hermes", "hermes") => &["HERMES_HOME"][..],
        ("gardn:opencode", "opencode") => &["OPENCODE_CONFIG", "XDG_DATA_HOME"][..],
        ("gardn:grok", "grok") => &["GROK_HOME"][..],
        ("gardn:antigravity_cli", "agy") => &["ANTIGRAVITY_CLI_CONFIG_DIR"][..],
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
        ("gardn:claude", "claude", AgentSessionRefKind::Id) => {
            vec![
                "claude".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("gardn:codex", "codex", AgentSessionRefKind::Id) => {
            vec!["codex".into(), "resume".into(), session_ref.value.clone()]
        }
        ("gardn:copilot", "copilot", AgentSessionRefKind::Id) => {
            vec!["copilot".into(), format!("--resume={}", session_ref.value)]
        }
        ("gardn:devin", "devin", AgentSessionRefKind::Id) => {
            vec!["devin".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("gardn:droid", "droid", AgentSessionRefKind::Id) => {
            vec!["droid".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("gardn:kimi", "kimi", AgentSessionRefKind::Id) => {
            vec!["kimi".into(), "--session".into(), session_ref.value.clone()]
        }
        ("gardn:pi", "pi", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            path_agent_resume_argv("pi", "--session", session_ref)
        }
        ("gardn:omp", "omp", AgentSessionRefKind::Path | AgentSessionRefKind::Id) => {
            path_agent_resume_argv("omp", "--resume", session_ref)
        }
        ("gardn:hermes", "hermes", AgentSessionRefKind::Id) => {
            vec![
                "hermes".into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("gardn:opencode", "opencode", AgentSessionRefKind::Id) => {
            vec![
                "opencode".into(),
                "--session".into(),
                session_ref.value.clone(),
            ]
        }
        ("gardn:grok", "grok", AgentSessionRefKind::Id) => {
            vec!["grok".into(), "--resume".into(), session_ref.value.clone()]
        }
        ("gardn:cursor", "cursor", AgentSessionRefKind::Id) => {
            vec![
                if cfg!(windows) {
                    "cursor-agent.cmd"
                } else {
                    "cursor-agent"
                }
                .into(),
                "--resume".into(),
                session_ref.value.clone(),
            ]
        }
        ("gardn:mastracode", "mastracode", AgentSessionRefKind::Id) => {
            vec![
                "mastracode".into(),
                "--thread".into(),
                session_ref.value.clone(),
            ]
        }
        ("gardn:antigravity_cli", "agy", AgentSessionRefKind::Id) => {
            vec![
                "agy".into(),
                "--conversation".into(),
                session_ref.value.clone(),
            ]
        }
        _ => return None,
    };

    Some(AgentResumePlan {
        agent: agent.to_string(),
        argv,
        command_resolution: AgentResumeCommandResolution::External,
        preserved_launch_argv: None,
        env: Vec::new(),
        dedupe_key: dedupe_key(source, agent, session_ref),
    })
}

fn path_agent_resume_argv(
    command: &str,
    resume_flag: &str,
    session_ref: &AgentSessionRef,
) -> Vec<String> {
    let mut argv = vec![
        command.to_string(),
        resume_flag.to_string(),
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

    if source == "gardn:omp" && agent == "omp" {
        if let Some(profile) = omp_profile_from_session_ref(session_ref) {
            let saved_command = launch_argv
                .and_then(|argv| argv.first())
                .filter(|command| valid_launch_command(command));
            let preserved_command = saved_command
                .filter(|command| command.as_str() != "omp" || profile.command == "omp");
            let command = preserved_command
                .cloned()
                .unwrap_or_else(|| profile.command.clone());
            plan.command_resolution = preserved_command
                .map(|command| resolution_for_saved_command(command))
                .unwrap_or(AgentResumeCommandResolution::ShellWrapper);
            plan.preserved_launch_argv = preserved_command.cloned().map(|command| vec![command]);
            if let Some(planned_command) = plan.argv.first_mut() {
                *planned_command = command;
            }
            plan.env = profile.reconcile_env(&plan.env);
        }
    }

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
    {
        plan.command_resolution = resolution_for_saved_command(&command);
        plan.preserved_launch_argv = Some(vec![command.clone()]);
        if let Some(planned_command) = plan.argv.first_mut() {
            *planned_command = command;
        }
    } else if source == "gardn:omp" && agent == "omp" {
        if let Some(profile) = omp_profile_from_session_ref(session_ref) {
            if let Some(planned_command) = plan.argv.first_mut() {
                *planned_command = profile.command;
            }
            plan.command_resolution = AgentResumeCommandResolution::ShellWrapper;
        }
    }
    Some(plan)
}

struct OmpProfile {
    config_dir: String,
    agent_dir: String,
    command: String,
}

impl OmpProfile {
    fn reconcile_env(&self, env: &[(String, String)]) -> Vec<(String, String)> {
        let mut reconciled = env
            .iter()
            .filter(|(key, _)| key != "PI_CONFIG_DIR" && key != "PI_CODING_AGENT_DIR")
            .cloned()
            .collect::<Vec<_>>();
        reconciled.push(("PI_CONFIG_DIR".to_string(), self.config_dir.clone()));
        reconciled.push(("PI_CODING_AGENT_DIR".to_string(), self.agent_dir.clone()));
        reconciled
    }
}

fn omp_profile_from_session_ref(session_ref: &AgentSessionRef) -> Option<OmpProfile> {
    if session_ref.kind != AgentSessionRefKind::Path {
        return None;
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    let relative = Path::new(&session_ref.value).strip_prefix(&home).ok()?;
    let mut components = relative.components();
    let config_dir = components.next()?.as_os_str().to_str()?;
    if components.next()?.as_os_str() != std::ffi::OsStr::new("agent")
        || components.next()?.as_os_str() != std::ffi::OsStr::new("sessions")
    {
        return None;
    }

    let command = if config_dir == ".omp" {
        "omp".to_string()
    } else {
        let profile = config_dir.strip_prefix(".omp-")?;
        if profile.is_empty()
            || !profile
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return None;
        }
        format!("omp-{profile}")
    };
    Some(OmpProfile {
        config_dir: config_dir.to_string(),
        agent_dir: home
            .join(config_dir)
            .join("agent")
            .to_string_lossy()
            .into_owned(),
        command,
    })
}

pub fn dedupe_key(source: &str, agent: &str, session_ref: &AgentSessionRef) -> String {
    format!(
        "{source}\u{0}{agent}\u{0}{:?}\u{0}{}",
        session_ref.kind, session_ref.value
    )
}

pub(crate) fn host_qualified_resume_key(
    host_id: &crate::execution_host::ExecutionHostId,
    dedupe_key: &str,
) -> String {
    format!("{}\u{0}{dedupe_key}", host_id.as_str())
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

pub(crate) fn is_official_agent_source(source: &str, agent: &str) -> bool {
    matches!(
        (source, agent),
        ("gardn:claude", "claude")
            | ("gardn:codex", "codex")
            | ("gardn:copilot", "copilot")
            | ("gardn:devin", "devin")
            | ("gardn:droid", "droid")
            | ("gardn:kimi", "kimi")
            | ("gardn:pi", "pi")
            | ("gardn:omp", "omp")
            | ("gardn:hermes", "hermes")
            | ("gardn:opencode", "opencode")
            | ("gardn:cursor", "cursor")
            | ("gardn:grok", "grok")
            | ("gardn:mastracode", "mastracode")
            | ("gardn:antigravity_cli", "agy")
    )
}
pub(crate) fn releases_process_owned_agent(source: &str, agent: &str) -> bool {
    matches!((source, agent), ("gardn:pi", "pi") | ("gardn:omp", "omp"))
}

fn valid_session_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_SESSION_ID_LEN && !value.chars().any(char::is_control)
}

fn valid_session_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SESSION_PATH_LEN
        && !value.chars().any(char::is_control)
        && is_absolute_session_path(value)
}

fn is_absolute_session_path(value: &str) -> bool {
    Path::new(value).is_absolute() || is_windows_absolute_session_path(value)
}

fn is_windows_absolute_session_path(value: &str) -> bool {
    let mut chars = value.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(drive), Some(':'), Some('\\' | '/')) if drive.is_ascii_alphabetic() => true,
        _ => value.starts_with("\\\\"),
    }
}

fn valid_launch_command(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn resolution_for_saved_command(command: &str) -> AgentResumeCommandResolution {
    if Path::new(command).is_absolute() || command.contains(['/', '\\']) {
        AgentResumeCommandResolution::External
    } else {
        AgentResumeCommandResolution::ShellWrapper
    }
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
    fn normalize_session_start_source_accepts_known_lifecycle_values() {
        for source in [
            "startup", "resume", "clear", "compact", "new", "fork", "select",
        ] {
            assert_eq!(
                normalize_session_start_source(Some(format!(" {source} "))),
                Some(source.to_string())
            );
        }
        assert_eq!(normalize_session_start_source(Some("other".into())), None);
        assert_eq!(normalize_session_start_source(None), None);
    }

    #[test]
    fn planner_allows_supported_agents() {
        assert_eq!(
            plan(
                "gardn:claude",
                "claude",
                &AgentSessionRef::id("claude-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["claude", "--resume", "claude-session"]
        );
        assert_eq!(
            plan(
                "gardn:codex",
                "codex",
                &AgentSessionRef::id("codex-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["codex", "resume", "codex-session"]
        );
        assert_eq!(
            plan(
                "gardn:copilot",
                "copilot",
                &AgentSessionRef::id("copilot-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["copilot", "--resume=copilot-session"]
        );
        assert_eq!(
            plan(
                "gardn:devin",
                "devin",
                &AgentSessionRef::id("devin-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["devin", "--resume", "devin-session"]
        );
        assert_eq!(
            plan(
                "gardn:kimi",
                "kimi",
                &AgentSessionRef::id("kimi-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["kimi", "--session", "kimi-session"]
        );
        assert_eq!(
            plan(
                "gardn:droid",
                "droid",
                &AgentSessionRef::id("droid-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["droid", "--resume", "droid-session"]
        );
        assert_eq!(
            plan(
                "gardn:pi",
                "pi",
                &AgentSessionRef::path("/tmp/pi-session.jsonl").unwrap()
            )
            .unwrap()
            .argv,
            vec!["pi", "--session", "/tmp/pi-session.jsonl"]
        );
        assert_eq!(
            plan(
                "gardn:omp",
                "omp",
                &AgentSessionRef::path("/tmp/omp-session.jsonl").unwrap()
            )
            .unwrap()
            .argv,
            vec!["omp", "--resume", "/tmp/omp-session.jsonl"]
        );
        assert_eq!(
            plan(
                "gardn:hermes",
                "hermes",
                &AgentSessionRef::id("hermes-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["hermes", "--resume", "hermes-session"]
        );
        assert_eq!(
            plan(
                "gardn:opencode",
                "opencode",
                &AgentSessionRef::id("opencode-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["opencode", "--session", "opencode-session"]
        );
        assert_eq!(
            plan(
                "gardn:grok",
                "grok",
                &AgentSessionRef::id("grok-session").unwrap()
            )
            .unwrap()
            .argv,
            vec!["grok", "--resume", "grok-session"]
        );
        assert_eq!(
            plan(
                "gardn:cursor",
                "cursor",
                &AgentSessionRef::id("cursor-session").unwrap()
            )
            .unwrap()
            .argv,
            vec![
                if cfg!(windows) {
                    "cursor-agent.cmd"
                } else {
                    "cursor-agent"
                },
                "--resume",
                "cursor-session",
            ]
        );
    }
    #[test]
    fn remaining_family_resume_contracts_are_explicit() {
        let session_ref = AgentSessionRef::id("family-session").unwrap();
        assert_eq!(
            plan("gardn:mastracode", "mastracode", &session_ref)
                .unwrap()
                .argv,
            vec!["mastracode", "--thread", "family-session"]
        );
        assert_eq!(
            plan("gardn:antigravity_cli", "agy", &session_ref)
                .unwrap()
                .argv,
            vec!["agy", "--conversation", "family-session"]
        );
        assert!(plan("gardn:qwen", "qwen", &session_ref).is_none());
        assert!(plan("gardn:kilo", "kilo", &session_ref).is_none());
    }

    #[test]
    fn omp_report_uses_session_id_when_reported_path_is_not_a_file() {
        let missing_path = std::env::temp_dir()
            .join(format!(
                "gardn-missing-omp-session-{}-{}.jsonl",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .to_string_lossy()
            .to_string();

        let session_ref = session_ref_from_report(
            "gardn:omp",
            "omp",
            Some("stable-omp-session-id".into()),
            Some(missing_path),
        )
        .expect("report with durable id should remain resumable");

        assert_eq!(
            session_ref,
            AgentSessionRef {
                kind: AgentSessionRefKind::Id,
                value: "stable-omp-session-id".into()
            }
        );
    }

    #[test]
    fn omp_report_prefers_existing_session_path_over_session_id() {
        let existing_path = std::env::current_exe()
            .expect("test executable path should be available")
            .to_string_lossy()
            .to_string();

        let session_ref = session_ref_from_report(
            "gardn:omp",
            "omp",
            Some("stable-omp-session-id".into()),
            Some(existing_path.clone()),
        )
        .expect("existing OMP path should remain resumable");

        assert_eq!(
            session_ref,
            AgentSessionRef {
                kind: AgentSessionRefKind::Path,
                value: existing_path
            }
        );
    }

    #[test]
    fn planner_preserves_available_launch_command_for_every_resumable_agent() {
        let launch_command = std::env::current_exe()
            .expect("test executable path should be available")
            .to_string_lossy()
            .to_string();
        let id_cases = [
            (
                "gardn:claude",
                "claude",
                "claude-session",
                vec!["--resume", "claude-session"],
            ),
            (
                "gardn:codex",
                "codex",
                "codex-session",
                vec!["resume", "codex-session"],
            ),
            (
                "gardn:copilot",
                "copilot",
                "copilot-session",
                vec!["--resume=copilot-session"],
            ),
            (
                "gardn:devin",
                "devin",
                "devin-session",
                vec!["--resume", "devin-session"],
            ),
            (
                "gardn:hermes",
                "hermes",
                "hermes-session",
                vec!["--resume", "hermes-session"],
            ),
            (
                "gardn:grok",
                "grok",
                "grok-session",
                vec!["--resume", "grok-session"],
            ),
            (
                "gardn:opencode",
                "opencode",
                "opencode-session",
                vec!["--session", "opencode-session"],
            ),
        ];

        for (source, agent, session_id, expected_args) in id_cases {
            let session_ref = AgentSessionRef::id(session_id).unwrap();
            let mut expected = vec![launch_command.clone()];
            expected.extend(expected_args.iter().map(|arg| (*arg).to_string()));
            assert_eq!(
                plan_with_launch_argv(
                    source,
                    agent,
                    &session_ref,
                    std::slice::from_ref(&launch_command).into()
                )
                .unwrap()
                .argv,
                expected
            );
        }

        let pi_ref = AgentSessionRef::path("/tmp/pi-session.jsonl").unwrap();
        assert_eq!(
            plan_with_launch_argv(
                "gardn:pi",
                "pi",
                &pi_ref,
                std::slice::from_ref(&launch_command).into()
            )
            .unwrap()
            .argv,
            vec![
                launch_command.clone(),
                "--session".into(),
                "/tmp/pi-session.jsonl".into()
            ]
        );

        let omp_ref = AgentSessionRef::path("/tmp/omp-session.jsonl").unwrap();
        assert_eq!(
            plan_with_launch_argv(
                "gardn:omp",
                "omp",
                &omp_ref,
                std::slice::from_ref(&launch_command).into()
            )
            .unwrap()
            .argv,
            vec![
                launch_command,
                "--resume".into(),
                "/tmp/omp-session.jsonl".into()
            ]
        );
    }

    #[test]
    fn planner_uses_omp_launch_command_for_path_resume() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path =
            format!("{home}/.omp-mk/agent/sessions/-projects-masakiro-gardn/session.jsonl");
        let session_dir = format!("{home}/.omp-mk/agent/sessions/-projects-masakiro-gardn");
        let session_ref = AgentSessionRef::path(session_path.clone()).unwrap();
        let launch_argv = vec!["omp-mk".to_string(), "--ignored-launch-arg".to_string()];

        let plan = plan_with_launch_argv("gardn:omp", "omp", &session_ref, Some(&launch_argv))
            .expect("official OMP path ref should be resumable");

        assert_eq!(
            plan.argv,
            vec![
                "omp-mk",
                "--resume",
                &session_path,
                "--session-dir",
                &session_dir
            ]
        );
    }

    #[test]
    fn planner_preserves_shell_resolved_profile_command_and_launch_env() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path =
            format!("{home}/.omp-mk/agent/sessions/-projects-masakiro-gardn/session.jsonl");
        let session_dir = format!("{home}/.omp-mk/agent/sessions/-projects-masakiro-gardn");
        let agent_dir = format!("{home}/.omp-mk/agent");
        let omp_profile_ref = AgentSessionRef::path(session_path.clone()).unwrap();
        let shell_resolved_profile = "omp-profile-alias".to_string();

        let plan = plan_with_launch_context(
            "gardn:omp",
            "omp",
            &omp_profile_ref,
            Some(std::slice::from_ref(&shell_resolved_profile)),
            &[
                ("PI_CONFIG_DIR".to_string(), ".omp-mk".to_string()),
                ("PI_CODING_AGENT_DIR".to_string(), agent_dir.clone()),
            ],
        )
        .unwrap();

        assert_eq!(
            plan.argv,
            vec![
                &shell_resolved_profile,
                "--resume",
                &session_path,
                "--session-dir",
                &session_dir
            ]
        );
        assert_eq!(
            plan.env,
            vec![
                ("PI_CONFIG_DIR".to_string(), ".omp-mk".to_string()),
                ("PI_CODING_AGENT_DIR".to_string(), agent_dir),
            ]
        );
        assert_eq!(
            plan.command_resolution,
            AgentResumeCommandResolution::ShellWrapper
        );
        assert_eq!(
            plan.preserved_launch_argv,
            Some(vec![shell_resolved_profile])
        );
    }

    #[test]
    fn planner_repairs_poisoned_omp_profile_context_from_session_path() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path =
            format!("{home}/.omp-mk/agent/sessions/-projects-masakiro-gardn/session.jsonl");
        let session_ref = AgentSessionRef::path(session_path.clone()).unwrap();

        let plan = plan_with_launch_context(
            "gardn:omp",
            "omp",
            &session_ref,
            Some(&["omp".to_string()]),
            &[
                ("PI_CONFIG_DIR".to_string(), ".omp".to_string()),
                (
                    "PI_CODING_AGENT_DIR".to_string(),
                    format!("{home}/.omp/agent"),
                ),
            ],
        )
        .unwrap();

        assert_eq!(
            plan.argv,
            vec![
                "omp-mk",
                "--resume",
                &session_path,
                "--session-dir",
                &format!("{home}/.omp-mk/agent/sessions/-projects-masakiro-gardn"),
            ]
        );
        assert_eq!(
            plan.env,
            vec![
                ("PI_CONFIG_DIR".to_string(), ".omp-mk".to_string()),
                (
                    "PI_CODING_AGENT_DIR".to_string(),
                    format!("{home}/.omp-mk/agent"),
                ),
            ]
        );
        assert_eq!(
            plan.command_resolution,
            AgentResumeCommandResolution::ShellWrapper
        );
        assert!(plan.preserved_launch_argv.is_none());
    }

    #[test]
    fn omp_child_session_restore_keeps_project_session_dir() {
        let home = std::env::var("HOME").expect("HOME should be set in tests");
        let session_path = format!(
            "{home}/.omp-profile/agent/sessions/-projects-masakiro-gardn/2026-06-03T17-52-01-399Z_019e8e9d-1b77-7000-875f-206076643bdf/RightSidebarHierarchyReview.jsonl"
        );
        let project_session_dir =
            format!("{home}/.omp-profile/agent/sessions/-projects-masakiro-gardn");
        let omp_ref = AgentSessionRef::path(session_path.clone()).unwrap();

        assert_eq!(
            plan_with_launch_argv("gardn:omp", "omp", &omp_ref, None)
                .unwrap()
                .argv,
            vec![
                "omp-profile".to_string(),
                "--resume".to_string(),
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
            plan_with_launch_context("gardn:codex", "codex", &session_ref, None, &env).unwrap();

        assert_eq!(plan.argv, vec!["codex", "resume", "codex-session"]);
        assert_eq!(plan.env, env);
        let other_env = vec![("CODEX_HOME".to_string(), "/profiles/other".to_string())];
        let other_plan =
            plan_with_launch_context("gardn:codex", "codex", &session_ref, None, &other_env)
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
            launch_env_from_report("gardn:codex", "codex", env),
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
            launch_env_from_report("gardn:devin", "devin", env),
            vec![(
                "DEVIN_CONFIG_DIR".to_string(),
                "/profiles/devin".to_string()
            )]
        );
        let env = BTreeMap::from([
            ("GROK_HOME".to_string(), "/profiles/grok".to_string()),
            ("PATH".to_string(), "/bin".to_string()),
        ]);
        assert_eq!(
            launch_env_from_report("gardn:grok", "grok", env),
            vec![("GROK_HOME".to_string(), "/profiles/grok".to_string())]
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
            "gardn:claude",
            "claude",
            &AgentSessionRef::path("/tmp/claude-session").unwrap()
        )
        .is_none());
    }

    #[test]
    fn planner_accepts_windows_omp_session_paths() {
        let windows_path = r"C:\Users\User\.omp\agent\sessions\omp-session.jsonl";
        let session_ref = AgentSessionRef::path(windows_path).unwrap();
        assert_eq!(
            plan("gardn:omp", "omp", &session_ref).unwrap().argv,
            vec!["omp", "--resume", windows_path]
        );
        assert_eq!(
            session_ref_from_snapshot("gardn:omp", "omp", AgentSessionRefKind::Path, windows_path)
                .unwrap()
                .session_ref,
            session_ref
        );
        assert!(
            AgentSessionRef::path("C:/Users/User/.omp/agent/sessions/omp-session.jsonl").is_some()
        );
        assert!(AgentSessionRef::path(r"\\server\share\omp-session.jsonl").is_some());
        assert!(AgentSessionRef::path("relative/omp-session.jsonl").is_none());
    }

    #[test]
    fn report_ref_prefers_pi_path_and_validates_values() {
        let session_ref = session_ref_from_report(
            "gardn:pi",
            "pi",
            Some("pi-id".into()),
            Some("/tmp/pi-session.jsonl".into()),
        )
        .unwrap();
        assert_eq!(session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(session_ref.value, "/tmp/pi-session.jsonl");
        let omp_path = std::env::current_exe()
            .expect("test executable path should be available")
            .to_string_lossy()
            .to_string();
        let omp_session_ref = session_ref_from_report(
            "gardn:omp",
            "omp",
            Some("omp-id".into()),
            Some(omp_path.clone()),
        )
        .unwrap();
        assert_eq!(omp_session_ref.kind, AgentSessionRefKind::Path);
        assert_eq!(omp_session_ref.value, omp_path);

        assert!(session_ref_from_report("gardn:pi", "pi", Some("bad\nid".into()), None).is_none());
        assert!(
            session_ref_from_report("gardn:pi", "pi", None, Some("relative.jsonl".into()))
                .is_none()
        );
        assert!(session_ref_from_report("custom:pi", "pi", Some("pi-id".into()), None).is_none());
        assert!(session_ref_from_report(
            "gardn:claude",
            "claude",
            None,
            Some("/tmp/claude-session".into())
        )
        .is_none());
    }

    #[test]
    fn ids_are_data_not_shell_text() {
        let id = "abc; rm -rf /";
        let codex_plan = plan("gardn:codex", "codex", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(codex_plan.argv, vec!["codex", "resume", id]);
        let devin_plan = plan("gardn:devin", "devin", &AgentSessionRef::id(id).unwrap()).unwrap();
        assert_eq!(devin_plan.argv, vec!["devin", "--resume", id]);
    }

    #[test]
    fn planner_rejects_path_refs_for_id_only_agents() {
        assert!(plan(
            "gardn:hermes",
            "hermes",
            &AgentSessionRef::path("/tmp/hermes-session").unwrap()
        )
        .is_none());
        assert!(plan(
            "gardn:opencode",
            "opencode",
            &AgentSessionRef::path("/tmp/opencode-session").unwrap()
        )
        .is_none());
        assert!(plan(
            "gardn:copilot",
            "copilot",
            &AgentSessionRef::path("/tmp/copilot-session").unwrap()
        )
        .is_none());
        assert!(plan(
            "gardn:grok",
            "grok",
            &AgentSessionRef::path("/tmp/grok-session").unwrap()
        )
        .is_none());
        assert!(session_ref_from_snapshot(
            "gardn:grok",
            "grok",
            AgentSessionRefKind::Id,
            "grok-session"
        )
        .is_some());
        assert!(plan(
            "gardn:devin",
            "devin",
            &AgentSessionRef::path("/tmp/devin-session").unwrap()
        )
        .is_none());
        assert!(session_ref_from_snapshot(
            "gardn:hermes",
            "hermes",
            AgentSessionRefKind::Id,
            "hermes-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "gardn:opencode",
            "opencode",
            AgentSessionRefKind::Id,
            "opencode-session"
        )
        .is_some());
        assert!(session_ref_from_snapshot(
            "gardn:devin",
            "devin",
            AgentSessionRefKind::Id,
            "devin-session"
        )
        .is_some());
    }
}
