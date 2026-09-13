use std::{ffi::OsString, io, path::Path};

#[cfg(unix)]
use std::{
    io::Read,
    os::fd::AsRawFd,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::agent_profiles::{AgentKind, AgentProfileCatalog};
use crate::api::schema::IntegrationTarget;

use super::{IntegrationStatusKind, INSTALL_WARNING_PREFIX};

#[cfg(unix)]
const LOGIN_SHELL_PATH_PREFIX: &[u8] = b"\x1egardn-login-path\x1f";
#[cfg(unix)]
const LOGIN_SHELL_PATH_SUFFIX: &[u8] = b"\x1egardn-login-path-end\x1f";
#[cfg(unix)]
const LOGIN_SHELL_PATH_COMMAND: &str =
    "printf '\\n\\036gardn-login-path\\037%s\\036gardn-login-path-end\\037\\n' \"$PATH\"";
#[cfg(unix)]
const LOGIN_SHELL_PATH_OUTPUT_LIMIT: usize = 16 * 1024;
#[cfg(unix)]
const LOGIN_SHELL_PATH_TIMEOUT: Duration = Duration::from_secs(2);

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
            activity_unix_secs: None,
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
    let paths = inspection_paths();
    let entries = super::integration_recommendations_for_paths(&paths)
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

fn inspection_paths() -> Vec<OsString> {
    #[cfg(unix)]
    {
        inspection_paths_with_limits(LOGIN_SHELL_PATH_TIMEOUT, LOGIN_SHELL_PATH_OUTPUT_LIMIT)
    }
    #[cfg(not(unix))]
    {
        std::env::var_os("PATH").into_iter().collect()
    }
}

#[cfg(unix)]
fn inspection_paths_with_limits(timeout: Duration, max_output_bytes: usize) -> Vec<OsString> {
    let mut paths = Vec::with_capacity(2);
    if let Some(path) = login_shell_path_with_limits(timeout, max_output_bytes) {
        paths.push(path);
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.push(path);
    }
    paths
}

#[cfg(unix)]
fn login_shell_path_with_limits(timeout: Duration, max_output_bytes: usize) -> Option<OsString> {
    let shell = std::env::var_os("SHELL")?;
    let mut command = Command::new(shell);
    command
        .args(["-lc", LOGIN_SHELL_PATH_COMMAND])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::platform::configure_cancellable_command(&mut command);

    let mut child = command.spawn().ok()?;
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        crate::platform::terminate_cancellable_child(&mut child);
        return None;
    };
    if set_nonblocking(&stdout).is_err() || set_nonblocking(&stderr).is_err() {
        crate::platform::terminate_cancellable_child(&mut child);
        return None;
    }

    let deadline = Instant::now() + timeout;
    let mut status = None;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut stdout_eof = false;
    let mut stderr_eof = false;

    loop {
        match drain_available(&mut stdout, &mut stdout_bytes, max_output_bytes) {
            Ok(PipeState::Eof) => stdout_eof = true,
            Ok(PipeState::Open) => {}
            Ok(PipeState::Oversized) | Err(_) => {
                crate::platform::terminate_cancellable_child(&mut child);
                return None;
            }
        }
        match drain_available(&mut stderr, &mut stderr_bytes, max_output_bytes) {
            Ok(PipeState::Eof) => stderr_eof = true,
            Ok(PipeState::Open) => {}
            Ok(PipeState::Oversized) | Err(_) => {
                crate::platform::terminate_cancellable_child(&mut child);
                return None;
            }
        }

        if status.is_none() {
            match child.try_wait() {
                Ok(next_status) => status = next_status,
                Err(_) => {
                    crate::platform::terminate_cancellable_child(&mut child);
                    return None;
                }
            }
        }
        if let Some(status) = status {
            if !status.success() {
                return None;
            }
            if let Some(path) = parse_login_shell_path(&stdout_bytes) {
                return Some(path);
            }
            if stdout_eof && stderr_eof {
                return None;
            }
        }
        if Instant::now() >= deadline {
            crate::platform::terminate_cancellable_child(&mut child);
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipeState {
    Open,
    Eof,
    Oversized,
}

#[cfg(unix)]
fn set_nonblocking(reader: &impl AsRawFd) -> io::Result<()> {
    let fd = reader.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(unix)]
fn drain_available(
    reader: &mut impl Read,
    output: &mut Vec<u8>,
    max_output_bytes: usize,
) -> io::Result<PipeState> {
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(PipeState::Eof),
            Ok(read) => {
                let remaining = max_output_bytes.saturating_sub(output.len());
                let retained = remaining.min(read);
                output.extend_from_slice(&buffer[..retained]);
                if retained != read {
                    return Ok(PipeState::Oversized);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return Ok(PipeState::Open);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(unix)]
fn parse_login_shell_path(stdout: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;

    let mut paths = stdout.split(|byte| *byte == b'\n').filter_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let path = line
            .strip_prefix(LOGIN_SHELL_PATH_PREFIX)?
            .strip_suffix(LOGIN_SHELL_PATH_SUFFIX)?;
        (!path.is_empty() && !path.contains(&0)).then(|| OsString::from_vec(path.to_vec()))
    });
    let path = paths.next()?;
    paths.next().is_none().then_some(path)
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
    #[cfg(unix)]
    use crate::config::TestEnvVar;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[cfg(unix)]
    fn unique_test_base(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "gardn-host-integration-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock must be after the Unix epoch")
                .as_nanos()
        ))
    }

    #[cfg(unix)]
    #[test]
    fn remote_inspection_combines_login_shell_and_inherited_paths() {
        let _lock = super::super::integration_env_lock();
        let base = unique_test_base("login-path");
        let inherited_bin = base.join("inherited-bin");
        let login_bin = base.join("login-bin");
        let home = base.join("home");
        std::fs::create_dir_all(&inherited_bin).expect("create inherited bin");
        std::fs::create_dir_all(&login_bin).expect("create login bin");
        std::fs::create_dir_all(&home).expect("create home");

        let login_agent = login_bin.join(super::super::integration_target_command(
            IntegrationTarget::Claude,
        ));
        std::fs::write(&login_agent, "").expect("write fake login-path agent command");
        std::fs::set_permissions(&login_agent, std::fs::Permissions::from_mode(0o755))
            .expect("make fake login-path agent command executable");
        let inherited_agent = inherited_bin.join(super::super::integration_target_command(
            IntegrationTarget::Pi,
        ));
        std::fs::write(&inherited_agent, "").expect("write fake inherited-path agent command");
        std::fs::set_permissions(&inherited_agent, std::fs::Permissions::from_mode(0o755))
            .expect("make fake inherited-path agent command executable");

        let shell = base.join("fake-login-shell");
        std::fs::write(
            &shell,
            r#"#!/bin/sh
if [ "$1" != "-lc" ]; then
    exit 64
fi
printf 'called\n' >> "$GARDN_TEST_SHELL_CALLS"
PATH="$GARDN_TEST_LOGIN_PATH"
export PATH
printf '/profile/chatter/that/is/not/PATH\n'
exec /bin/sh -c "$2"
"#,
        )
        .expect("write fake login shell");
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755))
            .expect("make fake login shell executable");

        let shell_calls = base.join("shell-calls");
        let _path = TestEnvVar::set("PATH", &inherited_bin);
        let _shell = TestEnvVar::set("SHELL", &shell);
        let _login_path = TestEnvVar::set("GARDN_TEST_LOGIN_PATH", &login_bin);
        let _shell_calls = TestEnvVar::set("GARDN_TEST_SHELL_CALLS", &shell_calls);
        let _home = TestEnvVar::set("HOME", &home);
        let _claude_config =
            TestEnvVar::set(super::super::CLAUDE_CONFIG_DIR_ENV_VAR, base.join("claude"));

        let snapshot = inspect(&[]);
        let claude = snapshot
            .entries
            .iter()
            .find(|entry| entry.target == IntegrationTarget::Claude)
            .expect("Claude recommendation");

        assert_eq!(claude.state, IntegrationStatusKind::NotInstalled);
        assert_eq!(claude.status_label(), "Available");
        let pi = snapshot
            .entries
            .iter()
            .find(|entry| entry.target == IntegrationTarget::Pi)
            .expect("Pi recommendation");
        assert_eq!(pi.status_label(), "Available");
        assert_eq!(
            std::fs::read_to_string(&shell_calls).expect("read shell call count"),
            "called\n"
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn remote_inspection_falls_back_when_login_shell_output_is_invalid() {
        let _lock = super::super::integration_env_lock();
        let base = unique_test_base("path-fallback");
        let inherited_bin = base.join("inherited-bin");
        let home = base.join("home");
        std::fs::create_dir_all(&inherited_bin).expect("create inherited bin");
        std::fs::create_dir_all(&home).expect("create home");

        let agent = inherited_bin.join(super::super::integration_target_command(
            IntegrationTarget::Claude,
        ));
        std::fs::write(&agent, "").expect("write fake agent command");
        std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755))
            .expect("make fake agent command executable");

        let shell = base.join("invalid-login-shell");
        std::fs::write(&shell, "#!/bin/sh\nprintf '/profile/chatter/bin\\n'\n")
            .expect("write invalid login shell output");
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755))
            .expect("make invalid login shell executable");

        let _path = TestEnvVar::set("PATH", &inherited_bin);
        let _shell = TestEnvVar::set("SHELL", &shell);
        let _home = TestEnvVar::set("HOME", &home);
        let _claude_config =
            TestEnvVar::set(super::super::CLAUDE_CONFIG_DIR_ENV_VAR, base.join("claude"));

        let snapshot = inspect(&[]);
        let claude = snapshot
            .entries
            .iter()
            .find(|entry| entry.target == IntegrationTarget::Claude)
            .expect("Claude recommendation");

        assert_eq!(claude.state, IntegrationStatusKind::NotInstalled);
        assert_eq!(claude.status_label(), "Available");

        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_timeout_falls_back_to_inherited_path() {
        let _lock = super::super::integration_env_lock();
        let base = unique_test_base("path-timeout");
        let inherited_bin = base.join("inherited-bin");
        std::fs::create_dir_all(&inherited_bin).expect("create inherited bin");

        let shell = base.join("hanging-login-shell");
        std::fs::write(&shell, "#!/bin/sh\n/bin/sleep 60\n").expect("write hanging login shell");
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755))
            .expect("make hanging login shell executable");

        let _path = TestEnvVar::set("PATH", &inherited_bin);
        let _shell = TestEnvVar::set("SHELL", &shell);
        let started = Instant::now();

        assert_eq!(
            inspection_paths_with_limits(Duration::from_millis(100), 1024),
            vec![inherited_bin.clone().into_os_string()]
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "login shell timeout did not terminate the process group"
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_output_limit_falls_back_to_inherited_path() {
        let _lock = super::super::integration_env_lock();
        let base = unique_test_base("path-output-limit");
        let inherited_bin = base.join("inherited-bin");
        std::fs::create_dir_all(&inherited_bin).expect("create inherited bin");

        let shell = base.join("noisy-login-shell");
        std::fs::write(
            &shell,
            r#"#!/bin/sh
i=0
while [ "$i" -lt 2048 ]; do
    printf x
    i=$((i + 1))
done
"#,
        )
        .expect("write noisy login shell");
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755))
            .expect("make noisy login shell executable");

        let _path = TestEnvVar::set("PATH", &inherited_bin);
        let _shell = TestEnvVar::set("SHELL", &shell);

        assert_eq!(
            inspection_paths_with_limits(Duration::from_secs(1), 128),
            vec![inherited_bin.clone().into_os_string()]
        );

        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn login_shell_timeout_ignores_escaped_pipe_holder() {
        let _lock = super::super::integration_env_lock();
        let base = unique_test_base("escaped-pipe-holder");
        let inherited_bin = base.join("inherited-bin");
        let login_bin = base.join("login-bin");
        std::fs::create_dir_all(&inherited_bin).expect("create inherited bin");
        std::fs::create_dir_all(&login_bin).expect("create login bin");
        for (target, directory) in [
            (IntegrationTarget::Claude, &login_bin),
            (IntegrationTarget::Pi, &inherited_bin),
        ] {
            let agent = directory.join(super::super::integration_target_command(target));
            std::fs::write(&agent, "").expect("write fake agent command");
            std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755))
                .expect("make fake agent command executable");
        }

        let shell = base.join("escaping-login-shell");
        std::fs::write(
            &shell,
            r#"#!/bin/sh
"$GARDN_TEST_HELPER" --exact integration::host::tests::escaped_pipe_holder_helper --nocapture &
printf '%s\n' "$!" > "$GARDN_TEST_HELPER_PID"
PATH="$GARDN_TEST_LOGIN_PATH"
export PATH
exec /bin/sh -c "$2"
"#,
        )
        .expect("write escaping login shell");
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755))
            .expect("make escaping login shell executable");

        let helper_pid = base.join("helper-pid");
        let _path = TestEnvVar::set("PATH", &inherited_bin);
        let _shell = TestEnvVar::set("SHELL", &shell);
        let _login_path = TestEnvVar::set("GARDN_TEST_LOGIN_PATH", &login_bin);
        let _helper = TestEnvVar::set(
            "GARDN_TEST_HELPER",
            std::env::current_exe().expect("current test executable"),
        );
        let _helper_pid = TestEnvVar::set("GARDN_TEST_HELPER_PID", &helper_pid);
        let _escape = TestEnvVar::set("GARDN_TEST_ESCAPE_PIPE", "1");
        let started = Instant::now();

        let paths = inspection_paths_with_limits(Duration::from_millis(100), 1024);
        assert_eq!(
            paths,
            vec![
                login_bin.clone().into_os_string(),
                inherited_bin.clone().into_os_string()
            ]
        );
        let recommendations = super::super::integration_recommendations_for_paths(&paths);
        for target in [IntegrationTarget::Claude, IntegrationTarget::Pi] {
            assert!(
                recommendations
                    .iter()
                    .find(|recommendation| recommendation.target == target)
                    .expect("integration recommendation")
                    .available,
                "{target:?} should remain available"
            );
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "escaped pipe holder blocked past the probe deadline"
        );

        if let Ok(pid) = std::fs::read_to_string(&helper_pid) {
            if let Ok(pid) = pid.trim().parse::<i32>() {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
        let _ = std::fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn escaped_pipe_holder_helper() {
        let _lock = super::super::integration_env_lock();
        if std::env::var_os("GARDN_TEST_ESCAPE_PIPE").is_none() {
            return;
        }
        unsafe {
            libc::setsid();
        }
        std::thread::sleep(Duration::from_secs(2));
    }

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
