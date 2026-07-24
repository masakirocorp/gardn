//! Remote thin-client launcher over SSH command stdio.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, IsTerminal, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{
    Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio,
};

use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BRIDGE_ACCEPT_POLL: Duration = Duration::from_millis(50);
const BRIDGE_SOCKET_PERMISSION_MODE: u32 = 0o600;
const REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);

/// Cooperative cancellation for blocking SSH probe/spawn work owned by a connect attempt.
#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectCancel {
    cancelled: Arc<AtomicBool>,
}

impl ConnectCancel {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> io::Result<()> {
        if self.is_cancelled() {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ssh connection attempt cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

fn wait_child_cancellable(
    child: &mut Child,
    cancel: Option<&ConnectCancel>,
) -> io::Result<ExitStatus> {
    if cancel.is_none() {
        return child.wait();
    }
    loop {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        match child.try_wait()? {
            Some(status) => return Ok(status),
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn kill_child_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
const REMOTE_SERVER_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const CURRENT_PROTOCOL: u32 = crate::protocol::PROTOCOL_VERSION;
const GITHUB_LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/masakirocorp/oh-my-herdr/releases/latest";
const REMOTE_BINARY_ENV_VAR: &str = "OMH_REMOTE_BINARY";
const SSH_CONTROL_SOCKET_NAME: &str = "ctl";
pub(crate) const REATTACH_COMMAND_ENV_VAR: &str = "OMH_REATTACH_COMMAND";

pub(crate) const REMOTE_KEYBINDINGS_ENV_VAR: &str = "OMH_REMOTE_KEYBINDINGS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemoteKeybindings {
    Local,
    Server,
}

impl RemoteKeybindings {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "local" => Ok(Self::Local),
            "server" => Ok(Self::Server),
            _ => Err("--remote-keybindings must be 'local' or 'server'".to_string()),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Server => "server",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteLaunch {
    pub(crate) target: String,
    pub(crate) keybindings: RemoteKeybindings,
    pub(crate) live_handoff: bool,
}

pub(crate) fn extract_remote_args(
    args: &[String],
) -> Result<(Vec<String>, Option<RemoteLaunch>), String> {
    let mut cleaned = Vec::with_capacity(args.len());
    if let Some(program) = args.first() {
        cleaned.push(program.clone());
    }

    let mut remote_target = None;
    let mut keybindings = RemoteKeybindings::Local;
    let mut keybindings_seen = false;
    let mut live_handoff = false;
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--" {
            cleaned.extend_from_slice(&args[index..]);
            break;
        }
        if arg == "--handoff" {
            live_handoff = true;
            index += 1;
            continue;
        }
        if arg == "--remote" {
            if remote_target.is_some() {
                return Err("--remote can only be specified once".to_string());
            }
            let Some(value) = args.get(index + 1) else {
                return Err("missing value for --remote".to_string());
            };
            remote_target = Some(validate_remote_target(value)?.to_owned());
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--remote=") {
            if remote_target.is_some() {
                return Err("--remote can only be specified once".to_string());
            }
            remote_target = Some(validate_remote_target(value)?.to_owned());
            index += 1;
            continue;
        }
        if arg == "--remote-keybindings" {
            if keybindings_seen {
                return Err("--remote-keybindings can only be specified once".to_string());
            }
            let Some(value) = args.get(index + 1) else {
                return Err("missing value for --remote-keybindings".to_string());
            };
            keybindings = RemoteKeybindings::parse(value)?;
            keybindings_seen = true;
            index += 2;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--remote-keybindings=") {
            if keybindings_seen {
                return Err("--remote-keybindings can only be specified once".to_string());
            }
            keybindings = RemoteKeybindings::parse(value)?;
            keybindings_seen = true;
            index += 1;
            continue;
        }

        cleaned.push(arg.clone());
        index += 1;
    }

    let remote = remote_target.map(|target| RemoteLaunch {
        target,
        keybindings,
        live_handoff,
    });
    if remote.is_none() && keybindings_seen {
        return Err("--remote-keybindings requires --remote".to_string());
    }
    if remote.is_none() && live_handoff {
        cleaned.push("--handoff".to_string());
    }

    Ok((cleaned, remote))
}

fn validate_remote_target(target: &str) -> Result<&str, String> {
    if target.is_empty() {
        return Err("missing value for --remote".to_string());
    }
    if target.starts_with('-') {
        return Err("--remote target must not start with '-'".to_string());
    }
    Ok(target)
}

pub(crate) fn run_remote(remote: RemoteLaunch) -> io::Result<()> {
    let session_name = crate::session::active_name()
        .unwrap_or_else(|| crate::session::DEFAULT_SESSION_NAME.to_string());
    let local_socket = local_forward_socket_path(&remote.target, &session_name);
    let program = std::env::args().next().unwrap_or_else(|| "omh".to_string());
    let reattach_command = reattach_command(
        &program,
        &remote.target,
        &session_name,
        remote.keybindings,
        remote.live_handoff,
    );
    let manage_ssh_config = crate::config::Config::load()
        .config
        .remote
        .manage_ssh_config;
    let remote_ssh = RemoteSsh::new(remote.target.clone(), manage_ssh_config);
    let prepared_remote = prepare_remote_omh(&remote_ssh, remote.live_handoff, &session_name)?;
    ensure_remote_server_ready(
        &remote_ssh,
        &prepared_remote.remote_omh,
        prepared_remote.installed_or_replaced,
        prepared_remote.stop_after_install_approved,
        remote.live_handoff,
        &session_name,
    )?;

    let _bridge = SshStdioBridge::start(
        remote.target,
        prepared_remote.remote_omh,
        local_socket.clone(),
        session_name,
        remote_ssh.options(),
    )?;

    run_client_process(&local_socket, &reattach_command, remote.keybindings)
}

pub(crate) fn run_remote_client_bridge() -> io::Result<()> {
    ensure_remote_server_running()?;

    let socket_path = crate::server::socket_paths::client_socket_path();
    let stream = UnixStream::connect(&socket_path).map_err(|err| {
        io::Error::new(
            err.kind(),
            format!(
                "failed to connect to remote Oh My Herdr client socket {}: {err}",
                socket_path.display()
            ),
        )
    })?;

    let mut stdout = io::stdout().lock();
    let mut socket_to_stdout = stream.try_clone()?;
    let mut stdin_to_socket = stream;

    let _upload = thread::spawn(move || {
        let mut stdin = io::stdin();
        let _ = copy_flush(&mut stdin, &mut stdin_to_socket);
        let _ = stdin_to_socket.shutdown(std::net::Shutdown::Write);
    });

    copy_flush(&mut socket_to_stdout, &mut stdout).map(|_| ())
}

fn ensure_remote_server_running() -> io::Result<()> {
    let socket_path = crate::server::socket_paths::client_socket_path();
    if crate::server::autodetect::is_server_listening() {
        let status = crate::api::read_runtime_status_at(
            &crate::api::socket_path(),
            Duration::from_millis(500),
        )?
        .ok_or_else(|| io::Error::other("remote server status API is unavailable"))?;
        if status.protocol == Some(CURRENT_PROTOCOL) {
            return Ok(());
        }
        return Err(io::Error::other(
            "the remote Oh My Herdr server must restart before this bridge can attach; rerun `omh --remote` from an interactive terminal to approve stopping it",
        ));
    }

    crate::server::autodetect::spawn_server_daemon()?;
    crate::server::autodetect::wait_for_server_socket(&socket_path, Duration::from_secs(5))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemotePlatform {
    os: &'static str,
    arch: &'static str,
}

impl RemotePlatform {
    fn from_uname(os: &str, arch: &str) -> Option<Self> {
        let os = match os.trim() {
            "Linux" => "linux",
            "Darwin" => "macos",
            _ => return None,
        };
        let arch = match arch.trim() {
            "x86_64" | "amd64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            _ => return None,
        };
        Some(Self { os, arch })
    }

    fn local() -> Self {
        let os = if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unknown"
        };

        let arch = if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "unknown"
        };

        Self { os, arch }
    }

    fn asset_key(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

#[derive(Debug, Clone)]
struct RemoteOmh {
    install_suffix: String,
    shell_path: String,
    platform: RemotePlatform,
}

impl RemoteOmh {
    fn for_platform(platform: RemotePlatform) -> Self {
        let install_suffix = ".local/bin/omh".to_string();
        let shell_path = format!("\"$HOME/{install_suffix}\"");
        Self {
            install_suffix,
            shell_path,
            platform,
        }
    }

    fn with_shell_path(mut self, shell_path: String) -> Self {
        self.shell_path = shell_path;
        self
    }
}

#[derive(Deserialize)]
struct RemoteGitHubRelease {
    tag_name: String,
    assets: Vec<RemoteGitHubReleaseAsset>,
}

#[derive(Deserialize)]
struct RemoteGitHubReleaseAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

struct InstallSource {
    path: PathBuf,
    temporary_dir: Option<PathBuf>,
}

struct PreparedRemoteOmh {
    remote_omh: RemoteOmh,
    installed_or_replaced: bool,
    stop_after_install_approved: bool,
}
#[derive(Clone)]
struct ManagedSshOptions {
    config_path: PathBuf,
    control_path: PathBuf,
}

struct ManagedSshConfig {
    options: ManagedSshOptions,
}

impl Drop for ManagedSshConfig {
    fn drop(&mut self) {
        if let Some(dir) = self.options.config_path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

struct RemoteSsh {
    target: String,
    managed_config: Option<ManagedSshConfig>,
    askpass: Option<crate::execution_host::auth::AskpassCommandConfig>,
}

impl RemoteSsh {
    fn new(target: String, manage_ssh_config: bool) -> Self {
        let managed_config = if manage_ssh_config {
            write_managed_ssh_config()
                .inspect_err(|err| {
                    tracing::debug!(%err, "could not write managed ssh config; using plain ssh");
                })
                .ok()
        } else {
            None
        };

        Self {
            target,
            managed_config,
            askpass: None,
        }
    }

    fn with_askpass(
        target: String,
        manage_ssh_config: bool,
        askpass: crate::execution_host::auth::AskpassCommandConfig,
    ) -> Self {
        let mut ssh = Self::new(target, manage_ssh_config);
        ssh.askpass = Some(askpass);
        ssh
    }

    fn target(&self) -> &str {
        &self.target
    }

    fn options(&self) -> Option<&ManagedSshOptions> {
        self.managed_config.as_ref().map(|config| &config.options)
    }

    fn command(&self) -> Command {
        let mut command = self.base_command();
        command.arg("-T").arg(&self.target);
        command
    }

    fn base_command(&self) -> Command {
        let mut command = Command::new("ssh");
        apply_managed_ssh_options(&mut command, self.options());
        if let Some(askpass) = &self.askpass {
            askpass.configure(&mut command);
        }
        command
    }

    fn sh_output(&self, script: &str) -> io::Result<Output> {
        self.sh_output_cancellable(script, None)
    }

    fn sh_output_cancellable(
        &self,
        script: &str,
        cancel: Option<&ConnectCancel>,
    ) -> io::Result<Output> {
        if let Some(cancel) = cancel {
            cancel.check()?;
        }
        let mut child = self
            .command()
            .arg("/bin/sh -s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let write_result = if let Some(mut stdin) = child.stdin.take() {
            let result = stdin.write_all(script.as_bytes());
            drop(stdin);
            result
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "ssh bootstrap stdin missing",
            ))
        };

        let status = match wait_child_cancellable(&mut child, cancel) {
            Ok(status) => status,
            Err(err) => {
                kill_child_tree(&mut child);
                return Err(err);
            }
        };
        // Child has exited; collect remaining stdio. `wait_with_output` would re-wait, so
        // read pipes directly after a successful cancellable wait.
        let stdout = {
            let mut buf = Vec::new();
            if let Some(mut out) = child.stdout.take() {
                use std::io::Read as _;
                let _ = out.read_to_end(&mut buf);
            }
            buf
        };
        let stderr = {
            let mut buf = Vec::new();
            if let Some(mut err) = child.stderr.take() {
                use std::io::Read as _;
                let _ = err.read_to_end(&mut buf);
            }
            buf
        };
        write_result?;
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }

    fn user_shell_output(&self, command: &str) -> io::Result<Output> {
        self.command().arg(command).output()
    }

    fn install_omh(&self, remote_omh: &RemoteOmh, source_path: &Path) -> io::Result<()> {
        let output = self.sh_output(&remote_install_prepare_script(remote_omh))?;
        if !output.status.success() {
            return Err(command_failed("remote install preparation failed", &output));
        }
        let (tmp_path, dest_path) = parse_remote_install_paths(&output.stdout)?;

        let result = (|| {
            let mut child = self
                .command()
                .arg(remote_install_stream_command(&tmp_path))
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|err| {
                    io::Error::new(err.kind(), format!("failed to start ssh install: {err}"))
                })?;

            let mut source = File::open(source_path)?;
            let copy_result = if let Some(mut stdin) = child.stdin.take() {
                io::copy(&mut source, &mut stdin).map(|_| ())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ssh install stdin missing",
                ))
            };
            let status = child.wait()?;
            copy_result?;
            if !status.success() {
                return Err(io::Error::other(format!(
                    "remote install exited with {status}"
                )));
            }

            let output = self.sh_output(&remote_install_commit_script(&tmp_path, &dest_path))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(command_failed("remote install commit failed", &output))
            }
        })();
        if result.is_err() {
            let _ = self.sh_output(&format!("rm -f -- {}\n", shell_quote(&tmp_path)));
        }
        result
    }
}

impl Drop for RemoteSsh {
    fn drop(&mut self) {
        if self.managed_config.is_none() {
            return;
        }

        let _ = self
            .base_command()
            .arg("-O")
            .arg("exit")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(&self.target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn apply_managed_ssh_options(command: &mut Command, options: Option<&ManagedSshOptions>) {
    let Some(options) = options else {
        return;
    };

    command
        .arg("-F")
        .arg(&options.config_path)
        .arg("-S")
        .arg(&options.control_path)
        .arg("-o")
        .arg("ControlMaster=auto")
        .arg("-o")
        .arg("ControlPersist=60");
}

impl InstallSource {
    fn persistent(path: PathBuf) -> Self {
        Self {
            path,
            temporary_dir: None,
        }
    }

    fn temporary(path: PathBuf, temporary_dir: PathBuf) -> Self {
        Self {
            path,
            temporary_dir: Some(temporary_dir),
        }
    }

    fn cleanup(&self) {
        if let Some(dir) = &self.temporary_dir {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

fn prepare_remote_omh(
    ssh: &RemoteSsh,
    live_handoff_enabled: bool,
    session_name: &str,
) -> io::Result<PreparedRemoteOmh> {
    let platform = detect_remote_platform(ssh)?;
    let remote_omh = RemoteOmh::for_platform(platform);
    let override_binary = remote_binary_override_path()?;
    let candidates = remote_binary_candidates(ssh, &remote_omh)?;

    if override_binary.is_none() {
        for candidate in &candidates {
            if remote_binary_matches(ssh, candidate).unwrap_or(false) {
                return Ok(PreparedRemoteOmh {
                    remote_omh: candidate.clone(),
                    installed_or_replaced: false,
                    stop_after_install_approved: false,
                });
            }
        }
        if remote_binary_matches(ssh, &remote_omh)? {
            return Ok(PreparedRemoteOmh {
                remote_omh,
                installed_or_replaced: false,
                stop_after_install_approved: false,
            });
        }
    }

    let mut stop_after_install_approved = false;
    if let Some(status_probe_omh) = candidates.first().or_else(|| {
        remote_binary_exists(ssh, &remote_omh)
            .ok()
            .and_then(|exists| exists.then_some(&remote_omh))
    }) {
        let approved = confirm_remote_install_with_running_server(
            ssh,
            status_probe_omh,
            live_handoff_enabled,
            session_name,
        )?;
        stop_after_install_approved = approved;
    }
    confirm_remote_install(
        ssh.target(),
        &remote_omh,
        &install_source_description(&remote_omh.platform, override_binary.as_deref()),
    )?;
    let source = resolve_install_source(&remote_omh.platform, override_binary)?;
    let install_result = ssh.install_omh(&remote_omh, &source.path);
    source.cleanup();
    install_result?;

    if !remote_binary_matches(ssh, &remote_omh)? {
        return Err(io::Error::other(format!(
            "installed remote Oh My Herdr at {}, but it did not report version {CURRENT_VERSION}",
            remote_omh.shell_path
        )));
    }
    warn_if_remote_bin_not_on_path(ssh)?;

    Ok(PreparedRemoteOmh {
        remote_omh,
        installed_or_replaced: true,
        stop_after_install_approved,
    })
}

fn detect_remote_platform(ssh: &RemoteSsh) -> io::Result<RemotePlatform> {
    detect_remote_platform_cancellable(ssh, None)
}

fn detect_remote_platform_cancellable(
    ssh: &RemoteSsh,
    cancel: Option<&ConnectCancel>,
) -> io::Result<RemotePlatform> {
    let output = ssh.sh_output_cancellable("uname -s\nuname -m\n", cancel)?;
    if !output.status.success() {
        return Err(command_failed("remote platform detection failed", &output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let os = lines.next().unwrap_or_default();
    let arch = lines.next().unwrap_or_default();
    RemotePlatform::from_uname(os, arch).ok_or_else(|| {
        io::Error::other(format!(
            "unsupported remote platform: {} {}",
            os.trim(),
            arch.trim()
        ))
    })
}

fn remote_binary_candidates(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<Vec<RemoteOmh>> {
    let mut candidates = Vec::new();

    if let Some(path_candidate) = remote_binary_on_path_any(ssh, remote_omh)? {
        candidates.push(path_candidate);
    }

    let output = ssh.sh_output(&known_remote_binary_candidate_script())?;
    if !output.status.success() {
        return Err(command_failed("remote binary discovery failed", &output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for candidate in remote_omhs_from_path_discovery(remote_omh, &stdout) {
        if !candidates
            .iter()
            .any(|existing| existing.shell_path == candidate.shell_path)
        {
            candidates.push(candidate);
        }
    }

    Ok(candidates)
}

fn known_remote_binary_candidate_script() -> String {
    let mut script = String::from(
        r#"home=${HOME:-}
version="#,
    );
    script.push_str(&shell_quote(CURRENT_VERSION));
    script.push_str(
        r#"
emit() {
    path=$1
    if [ -n "$path" ] && [ -x "$path" ]; then
        printf '%s\n' "$path"
    fi
}
if [ -n "$home" ]; then
    emit "$home/.local/share/mise/installs/omh/$version/bin/omh"
    emit "$home/.local/share/mise/installs/omh/$version/omh"
fi
"#,
    );
    script
}

fn remote_binary_on_path_any(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
) -> io::Result<Option<RemoteOmh>> {
    let primary_output = ssh.user_shell_output("command -v omh")?;
    if primary_output.status.success() {
        let stdout = String::from_utf8_lossy(&primary_output.stdout);
        if let Some(candidate) = remote_omh_from_path_discovery(remote_omh, &stdout) {
            return Ok(Some(candidate));
        }
    }

    // Non-POSIX login shells such as xonsh reject `command -v`; retry through
    // /bin/sh while retaining the login-shell probe for shell-initialized PATHs.
    let fallback_output = ssh.sh_output("command -v omh\n")?;
    if fallback_output.status.success() {
        let stdout = String::from_utf8_lossy(&fallback_output.stdout);
        return Ok(remote_omh_from_path_discovery(remote_omh, &stdout));
    }

    tracing::debug!(
        primary = %command_output_diagnostic(&primary_output),
        fallback = %command_output_diagnostic(&fallback_output),
        "remote binary path discovery failed in user shell and /bin/sh"
    );
    Ok(None)
}

fn command_output_diagnostic(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        output.status.to_string()
    } else {
        stderr.to_string()
    }
}

fn remote_omhs_from_path_discovery(remote_omh: &RemoteOmh, stdout: &str) -> Vec<RemoteOmh> {
    stdout
        .lines()
        .filter_map(|path| remote_omh_from_path(remote_omh, path))
        .collect()
}

fn remote_omh_from_path_discovery(remote_omh: &RemoteOmh, stdout: &str) -> Option<RemoteOmh> {
    stdout
        .lines()
        .find_map(|path| remote_omh_from_path(remote_omh, path))
}

fn remote_omh_from_path(remote_omh: &RemoteOmh, path: &str) -> Option<RemoteOmh> {
    let path = path.trim();
    if !path.starts_with('/') || path.ends_with("/mise/shims/omh") {
        return None;
    }
    Some(remote_omh.clone().with_shell_path(shell_quote(path)))
}

fn remote_binary_matches(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<bool> {
    let command = format!(
        "test -x {0} && {0} --version && {0} status client --json",
        remote_omh.shell_path
    );
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let version = lines.next().unwrap_or_default().trim();
    let status = lines.next().unwrap_or_default();
    Ok(version == format!("omh {CURRENT_VERSION}")
        && parse_client_status_json(status)
            .map(|status| status.protocol == CURRENT_PROTOCOL)
            .unwrap_or(false))
}

fn remote_binary_exists(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<bool> {
    let command = format!("test -x {}", remote_omh.shell_path);
    Ok(ssh.sh_output(&command)?.status.success())
}

fn remote_binary_override_path() -> io::Result<Option<PathBuf>> {
    let Some(value) = std::env::var_os(REMOTE_BINARY_ENV_VAR) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{REMOTE_BINARY_ENV_VAR} must not be empty"),
        ));
    }

    let path = PathBuf::from(value);
    let metadata = fs::metadata(&path).map_err(|err| {
        io::Error::new(
            err.kind(),
            format!(
                "failed to inspect {REMOTE_BINARY_ENV_VAR} path {}: {err}",
                path.display()
            ),
        )
    })?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{REMOTE_BINARY_ENV_VAR} path is not a file: {}",
                path.display()
            ),
        ));
    }

    Ok(Some(path))
}

fn install_source_description(platform: &RemotePlatform, override_binary: Option<&Path>) -> String {
    if let Some(path) = override_binary {
        return format!("{REMOTE_BINARY_ENV_VAR} ({})", path.display());
    }

    if *platform == RemotePlatform::local() {
        "the current local Oh My Herdr binary".to_string()
    } else {
        format!(
            "the {CURRENT_VERSION} release asset for {}",
            platform.asset_key()
        )
    }
}

fn resolve_install_source(
    platform: &RemotePlatform,
    override_binary: Option<PathBuf>,
) -> io::Result<InstallSource> {
    if let Some(path) = override_binary {
        return Ok(InstallSource::persistent(path));
    }

    if *platform == RemotePlatform::local() {
        let path = std::env::current_exe()?;
        return Ok(InstallSource::persistent(path));
    }

    download_release_asset(platform)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteServerStatus {
    Running {
        version: Option<String>,
        protocol: Option<u32>,
        live_handoff: bool,
    },
    NotRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteServerRestartReason {
    ProtocolMismatch,
    BinaryUpdated,
    VersionMismatch,
}

fn ensure_remote_server_ready(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    remote_binary_changed: bool,
    stop_after_install_approved: bool,
    live_handoff_enabled: bool,
    session_name: &str,
) -> io::Result<()> {
    let status = remote_server_status(ssh, remote_omh, session_name)?;
    let RemoteServerStatus::Running {
        version,
        protocol,
        live_handoff,
    } = status
    else {
        return Ok(());
    };

    let Some(reason) =
        remote_server_restart_reason(version.as_deref(), protocol, remote_binary_changed)
    else {
        return Ok(());
    };

    if live_handoff_enabled && live_handoff {
        match live_handoff_remote_server(ssh, remote_omh, session_name) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!("remote live handoff failed: {err}");
                eprintln!("falling back to remote server restart.");
            }
        }
    }

    if stop_after_install_approved {
        stop_remote_server(ssh, remote_omh, session_name)?;
        return Ok(());
    }

    if confirm_remote_server_stop(ssh.target(), version.as_deref(), protocol, reason)? {
        stop_remote_server(ssh, remote_omh, session_name)?;
    }
    Ok(())
}

fn remote_server_restart_reason(
    version: Option<&str>,
    protocol: Option<u32>,
    remote_binary_changed: bool,
) -> Option<RemoteServerRestartReason> {
    if protocol != Some(CURRENT_PROTOCOL) {
        return Some(RemoteServerRestartReason::ProtocolMismatch);
    }
    if remote_binary_changed {
        return Some(RemoteServerRestartReason::BinaryUpdated);
    }
    if version != Some(CURRENT_VERSION) {
        return Some(RemoteServerRestartReason::VersionMismatch);
    }
    None
}

fn confirm_remote_install_with_running_server(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    live_handoff_enabled: bool,
    session_name: &str,
) -> io::Result<bool> {
    let target = ssh.target();
    let status = match remote_server_status(ssh, remote_omh, session_name) {
        Ok(status) => status,
        Err(err) => {
            if !io::stdin().is_terminal() {
                return Err(io::Error::other(format!(
                    "could not inspect the running remote Oh My Herdr server on {target} before installing: {err}; run from an interactive terminal to approve updating the remote binary"
                )));
            }
            eprintln!(
                "could not inspect the running remote Oh My Herdr server on {target} before installing: {err}"
            );
            eprint!("continue installing the remote Oh My Herdr binary? [y/N] ");
            io::stderr().flush()?;

            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            let answer = answer.trim().to_ascii_lowercase();
            if answer != "y" && answer != "yes" {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "remote Oh My Herdr install cancelled",
                ));
            }
            return Ok(false);
        }
    };
    let RemoteServerStatus::Running {
        version,
        protocol: _,
        live_handoff,
    } = status
    else {
        return Ok(false);
    };

    if !io::stdin().is_terminal() {
        if live_handoff_enabled && live_handoff {
            return Ok(false);
        }
        return Err(io::Error::other(format!(
            "remote Oh My Herdr server on {target} is running v{}; run from an interactive terminal to approve stopping it for the update",
            version_label(version.as_deref())
        )));
    }

    if live_handoff_enabled && live_handoff {
        eprintln!("remote Oh My Herdr server on {target} is currently running:");
        eprintln!("  server: v{}", version_label(version.as_deref()));
        eprintln!(
            "Oh My Herdr will install v{CURRENT_VERSION} and hand off live pane processes to the prepared server."
        );
        return Ok(false);
    }

    eprintln!("remote Oh My Herdr server on {target} is currently running:");
    eprintln!("  server: v{}", version_label(version.as_deref()));
    eprintln!(
        "To complete the remote update, Oh My Herdr must stop the running remote server after installing."
    );
    eprintln!("This stops active remote pane processes, including shells, dev servers, and tests.");
    eprintln!();
    eprint!("Install v{CURRENT_VERSION} and stop the remote server now? [y/N] ");
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    if answer != "y" && answer != "yes" {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "remote Oh My Herdr install cancelled",
        ));
    }

    Ok(true)
}

fn remote_server_status(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    session_name: &str,
) -> io::Result<RemoteServerStatus> {
    let command = remote_server_status_command(remote_omh, session_name);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server status failed", &output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_remote_server_status_json(stdout.trim())
}

#[derive(Debug, Deserialize)]
struct RemoteClientStatusJson {
    protocol: u32,
}

#[derive(Debug, Deserialize)]
struct RemoteServerStatusJson {
    running: bool,
    version: Option<String>,
    protocol: Option<u32>,
    capabilities: Option<RemoteServerCapabilitiesJson>,
}

#[derive(Debug, Deserialize)]
struct RemoteServerCapabilitiesJson {
    live_handoff: bool,
}

fn parse_client_status_json(status: &str) -> Option<RemoteClientStatusJson> {
    serde_json::from_str(status).ok()
}

fn parse_remote_server_status_json(status: &str) -> io::Result<RemoteServerStatus> {
    let parsed: RemoteServerStatusJson = serde_json::from_str(status).map_err(|err| {
        io::Error::other(format!(
            "could not parse remote server status JSON from `{status}`: {err}"
        ))
    })?;
    if !parsed.running {
        return Ok(RemoteServerStatus::NotRunning);
    }

    Ok(RemoteServerStatus::Running {
        version: parsed.version,
        protocol: parsed.protocol,
        live_handoff: parsed
            .capabilities
            .is_some_and(|capabilities| capabilities.live_handoff),
    })
}

fn confirm_remote_server_stop(
    target: &str,
    version: Option<&str>,
    _protocol: Option<u32>,
    reason: RemoteServerRestartReason,
) -> io::Result<bool> {
    if !io::stdin().is_terminal() {
        if reason == RemoteServerRestartReason::ProtocolMismatch {
            return Err(io::Error::other(format!(
                "remote Oh My Herdr server on {target} must stop before this client can attach; run from an interactive terminal to approve stopping it"
            )));
        }

        eprintln!(
            "remote Oh My Herdr server on {target} is still running v{}; it will use v{CURRENT_VERSION} after it restarts.",
            version_label(version)
        );
        return Ok(false);
    }

    eprintln!("remote Oh My Herdr server on {target} is currently running:");
    eprintln!("  server: v{}", version_label(version));
    eprintln!("  prepared binary: v{CURRENT_VERSION}");
    eprintln!();

    match reason {
        RemoteServerRestartReason::ProtocolMismatch => {
            eprintln!("the remote server must stop before this client can attach.");
        }
        RemoteServerRestartReason::BinaryUpdated => {
            eprintln!(
                "the remote Oh My Herdr binary was installed or replaced. restart the remote server so it uses the prepared binary."
            );
        }
        RemoteServerRestartReason::VersionMismatch => {
            eprintln!(
                "the remote server is still running a different omh version. restart it so it uses the prepared binary."
            );
        }
    }

    let prompt = if reason == RemoteServerRestartReason::ProtocolMismatch {
        "stop the remote server and continue attaching? [Y/n] "
    } else {
        "restart the remote server now? [y/N] "
    };
    eprint!("{prompt}");
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    if answer == "y" || answer == "yes" {
        return Ok(true);
    }
    if answer.is_empty() && reason == RemoteServerRestartReason::ProtocolMismatch {
        return Ok(true);
    }
    if reason == RemoteServerRestartReason::ProtocolMismatch {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "remote Oh My Herdr server stop cancelled",
        ));
    }

    Ok(false)
}

fn live_handoff_remote_server(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    session_name: &str,
) -> io::Result<()> {
    let command = remote_server_live_handoff_command(remote_omh, session_name);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server live handoff failed", &output));
    }

    eprintln!(
        "handed off the remote Oh My Herdr server on {}; reconnecting to the prepared server.",
        ssh.target()
    );
    Ok(())
}

fn stop_remote_server(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    session_name: &str,
) -> io::Result<()> {
    let command = remote_server_stop_command(remote_omh, session_name);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server stop failed", &output));
    }

    wait_for_remote_server_shutdown(ssh, remote_omh, session_name)?;
    eprintln!(
        "stopped the remote Oh My Herdr server on {}; it will restart when the remote client bridge attaches.",
        ssh.target()
    );
    Ok(())
}

fn wait_for_remote_server_shutdown(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    session_name: &str,
) -> io::Result<()> {
    let deadline = Instant::now() + REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT;
    loop {
        if remote_server_status(ssh, remote_omh, session_name)? == RemoteServerStatus::NotRunning {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "shutdown was requested, but the old remote Oh My Herdr server on {} is still responding after {} seconds",
                    ssh.target(),
                    REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT.as_secs()
                ),
            ));
        }
        thread::sleep(REMOTE_SERVER_SHUTDOWN_POLL_INTERVAL);
    }
}

fn version_label(version: Option<&str>) -> &str {
    version.unwrap_or("unknown")
}

fn warn_if_remote_bin_not_on_path(ssh: &RemoteSsh) -> io::Result<()> {
    let output = ssh.user_shell_output("command -v omh")?;
    if output.status.success()
        && remote_shell_resolves_managed_install(&String::from_utf8_lossy(&output.stdout))
    {
        return Ok(());
    }

    eprintln!(
        "omh: installed remote binary to ~/.local/bin/omh, but the remote shell does not resolve `omh` to that path"
    );
    Ok(())
}

fn remote_shell_resolves_managed_install(stdout: &str) -> bool {
    stdout
        .lines()
        .next()
        .map(str::trim)
        .is_some_and(|path| path.ends_with("/.local/bin/omh"))
}

fn release_worker_asset(platform: &RemotePlatform) -> io::Result<(String, String)> {
    let release_output = crate::noninteractive_process::curl_command()
        .args([
            "-sfL",
            "--retry",
            "3",
            "--connect-timeout",
            "10",
            "--max-time",
            "20",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: omh-remote-installer",
            GITHUB_LATEST_RELEASE_API_URL,
        ])
        .output()
        .map_err(|err| io::Error::new(err.kind(), format!("curl failed: {err}")))?;
    if !release_output.status.success() {
        return Err(command_failed(
            "failed to fetch latest GitHub release",
            &release_output,
        ));
    }
    let release: RemoteGitHubRelease =
        serde_json::from_slice(&release_output.stdout).map_err(|err| {
            io::Error::other(format!("failed to parse latest GitHub release JSON: {err}"))
        })?;
    if release.tag_name.trim_start_matches('v') != CURRENT_VERSION {
        return Err(io::Error::other(format!(
            "remote host is {}, but this local Oh My Herdr is {CURRENT_VERSION} and the latest GitHub release is {}; build omh for the remote platform or install it there manually",
            platform.asset_key(),
            release.tag_name
        )));
    }
    let asset_name = format!("omh-{}", platform.asset_key());
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
            io::Error::other(format!(
                "no {asset_name} binary in the latest GitHub release for omh {CURRENT_VERSION}"
            ))
        })?;
    let checksum = asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| {
            digest.len() == 64
                && digest
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        })
        .ok_or_else(|| io::Error::other(format!("{asset_name} has no valid SHA-256 digest")))?;
    Ok((
        asset.browser_download_url.clone(),
        checksum.to_ascii_lowercase(),
    ))
}

fn download_release_asset(platform: &RemotePlatform) -> io::Result<InstallSource> {
    let (url, checksum) = release_worker_asset(platform)?;
    let asset_key = platform.asset_key();
    let dir = private_download_dir(&asset_key)?;
    let path = dir.join("omh.tmp");
    let status = crate::noninteractive_process::curl_command()
        .args(["-sfL", "--max-time", "120", "-o"])
        .arg(&path)
        .arg(&url)
        .status()
        .map_err(|err| io::Error::new(err.kind(), format!("download failed: {err}")))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&dir);
        return Err(io::Error::other("download failed"));
    }
    if let Err(error) = crate::checksum::verify_sha256(&path, &checksum) {
        let _ = fs::remove_dir_all(&dir);
        return Err(io::Error::other(format!(
            "downloaded remote worker checksum verification failed: {error}"
        )));
    }
    Ok(InstallSource::temporary(path, dir))
}

fn private_download_dir(asset_key: &str) -> io::Result<PathBuf> {
    let base = std::env::temp_dir();
    for attempt in 0..100 {
        let dir = base.join(format!(
            "omh-remote-{}-{}-{attempt}",
            std::process::id(),
            asset_key
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to create private omh remote download directory",
    ))
}

fn confirm_remote_install(
    target: &str,
    remote_omh: &RemoteOmh,
    source_description: &str,
) -> io::Result<()> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::other(format!(
            "matching remote Oh My Herdr {CURRENT_VERSION} is not installed at {}; run from an interactive terminal to approve installation",
            remote_omh.shell_path
        )));
    }

    eprintln!(
        "matching Oh My Herdr {CURRENT_VERSION} is not installed on {target} for {}.",
        remote_omh.platform.asset_key()
    );
    eprint!(
        "Install {} to {}? [Y/n] ",
        source_description, remote_omh.shell_path
    );
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    if answer == "n" || answer == "no" {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "remote Oh My Herdr installation cancelled",
        ));
    }

    Ok(())
}

fn remote_install_prepare_script(remote_omh: &RemoteOmh) -> String {
    format!(
        r#"set -eu
dest="$HOME/{install_suffix}"
dir="${{dest%/*}}"
mkdir -p "$dir"
tmp="${{dest}}.tmp.$$"
printf '%s\0%s\0' "$tmp" "$dest"
"#,
        install_suffix = remote_omh.install_suffix
    )
}

fn parse_remote_install_paths(stdout: &[u8]) -> io::Result<(String, String)> {
    let mut parts = stdout.split(|byte| *byte == 0);
    let tmp_path = parts.next().unwrap_or_default();
    let dest_path = parts.next().unwrap_or_default();
    if tmp_path.is_empty() || dest_path.is_empty() {
        return Err(io::Error::other(
            "remote install preparation did not return destination paths",
        ));
    }
    let tmp_path = String::from_utf8(tmp_path.to_vec()).map_err(|err| {
        io::Error::other(format!(
            "remote install temporary path is not valid UTF-8: {err}"
        ))
    })?;
    let dest_path = String::from_utf8(dest_path.to_vec()).map_err(|err| {
        io::Error::other(format!(
            "remote install destination path is not valid UTF-8: {err}"
        ))
    })?;
    Ok((tmp_path, dest_path))
}

fn remote_install_stream_command(tmp_path: &str) -> String {
    format!("tee {}", shell_quote(tmp_path))
}

fn remote_install_commit_script(tmp_path: &str, dest_path: &str) -> String {
    format!(
        "set -eu\nchmod 755 {tmp_path}\nmv {tmp_path} {dest_path}\n",
        tmp_path = shell_quote(tmp_path),
        dest_path = shell_quote(dest_path)
    )
}

fn append_remote_session_flag(command: &mut String, session_name: &str) {
    if session_name != crate::session::DEFAULT_SESSION_NAME {
        command.push_str(" --session ");
        command.push_str(&shell_quote(session_name));
    }
}

fn remote_server_status_command(remote_omh: &RemoteOmh, session_name: &str) -> String {
    let mut command = remote_omh.shell_path.clone();
    append_remote_session_flag(&mut command, session_name);
    command.push_str(" status server --json");
    command
}

fn remote_server_stop_command(remote_omh: &RemoteOmh, session_name: &str) -> String {
    let mut command = remote_omh.shell_path.clone();
    append_remote_session_flag(&mut command, session_name);
    command.push_str(" server stop");
    command
}

fn remote_server_live_handoff_command(remote_omh: &RemoteOmh, session_name: &str) -> String {
    let mut command = remote_omh.shell_path.clone();
    append_remote_session_flag(&mut command, session_name);
    command.push_str(&format!(
        " server live-handoff --import-exe {} --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}",
        remote_omh.shell_path
    ));
    command
}

fn remote_bridge_command(remote_omh: &RemoteOmh, session_name: &str) -> String {
    let mut command = format!("exec {}", remote_omh.shell_path);
    append_remote_session_flag(&mut command, session_name);
    command.push_str(" remote-client-bridge");
    command
}

fn reattach_command(
    program: &str,
    target: &str,
    session_name: &str,
    keybindings: RemoteKeybindings,
    live_handoff: bool,
) -> String {
    let program = if program.is_empty() { "omh" } else { program };
    let mut command = format!("{} --remote {}", shell_quote(program), shell_quote(target));
    if keybindings != RemoteKeybindings::Local {
        command.push_str(" --remote-keybindings ");
        command.push_str(keybindings.as_str());
    }
    if live_handoff {
        command.push_str(" --handoff");
    }
    append_remote_session_flag(&mut command, session_name);
    command
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(
                    ch,
                    '@' | '%' | '_' | '+' | '=' | ':' | ',' | '.' | '/' | '-'
                )
        })
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\\''"))
}

fn command_failed(context: &str, output: &Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        io::Error::other(format!("{context}: {}", output.status))
    } else {
        io::Error::other(format!("{context}: {stderr}"))
    }
}

struct SshStdioBridge {
    local_socket: PathBuf,
    should_stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SshStdioBridge {
    fn start(
        target: String,
        remote_omh: RemoteOmh,
        local_socket: PathBuf,
        session_name: String,
        ssh_options: Option<&ManagedSshOptions>,
    ) -> io::Result<Self> {
        let _ = std::fs::remove_file(&local_socket);
        let listener = UnixListener::bind(&local_socket)?;
        crate::ipc::restrict_socket_permissions(&local_socket, BRIDGE_SOCKET_PERMISSION_MODE)?;
        listener.set_nonblocking(true)?;

        let should_stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&should_stop);
        let thread_ssh_options = ssh_options.cloned();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        if let Err(err) = stream.set_nonblocking(false) {
                            eprintln!("omh: remote bridge failed to prepare client socket: {err}");
                            continue;
                        }
                        if let Err(err) = bridge_connection(
                            stream,
                            &target,
                            &remote_omh,
                            &session_name,
                            thread_ssh_options.as_ref(),
                        ) {
                            eprintln!("omh: remote bridge failed: {err}");
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(BRIDGE_ACCEPT_POLL);
                    }
                    Err(err) => {
                        eprintln!("omh: remote bridge listener failed: {err}");
                        break;
                    }
                }
            }
        });

        Ok(Self {
            local_socket,
            should_stop,
            thread: Some(thread),
        })
    }
}

impl Drop for SshStdioBridge {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Release);
        let _ = std::fs::remove_file(&self.local_socket);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn private_ssh_config_dir() -> io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;

    let mut bases = vec![std::env::temp_dir()];
    let short_tmp = PathBuf::from("/tmp");
    if bases.first() != Some(&short_tmp) {
        bases.push(short_tmp);
    }

    let mut last_error = None;
    for base in bases {
        for attempt in 0..100 {
            let dir = base.join(format!("omh-ssh-{}-{attempt}", std::process::id()));
            if !fits_unix_socket_path(&dir.join(SSH_CONTROL_SOCKET_NAME)) {
                continue;
            }
            match fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => return Ok(dir),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    last_error = Some(err);
                    break;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "failed to create private Oh My Herdr ssh config directory",
        )
    }))
}

fn ssh_config_quote(path: &str) -> String {
    format!("\"{path}\"")
}

fn write_managed_ssh_config() -> io::Result<ManagedSshConfig> {
    use std::os::unix::fs::OpenOptionsExt;

    let dir = private_ssh_config_dir()?;
    let path = dir.join("config");
    let control_path = dir.join(SSH_CONTROL_SOCKET_NAME);

    let mut contents = String::new();
    if let Some(home) = std::env::var_os("HOME") {
        let user_config = PathBuf::from(home).join(".ssh").join("config");
        if user_config.is_file() {
            contents.push_str(&format!(
                "Include {}\n",
                ssh_config_quote(&user_config.to_string_lossy())
            ));
        }
    }
    if Path::new("/etc/ssh/ssh_config").is_file() {
        contents.push_str("Include /etc/ssh/ssh_config\n");
    }
    contents.push_str("Host *\n");
    contents.push_str("  ServerAliveInterval 15\n");
    contents.push_str("  ServerAliveCountMax 4\n");

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(BRIDGE_SOCKET_PERMISSION_MODE)
        .open(&path)?;
    file.write_all(contents.as_bytes())?;
    Ok(ManagedSshConfig {
        options: ManagedSshOptions {
            config_path: path,
            control_path,
        },
    })
}

fn bridge_connection(
    stream: UnixStream,
    target: &str,
    remote_omh: &RemoteOmh,
    session_name: &str,
    ssh_options: Option<&ManagedSshOptions>,
) -> io::Result<()> {
    let mut command = Command::new("ssh");
    apply_managed_ssh_options(&mut command, ssh_options);
    command
        .arg("-T")
        .arg(target)
        .arg(remote_bridge_command(remote_omh, session_name));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .map_err(|err| io::Error::new(err.kind(), format!("failed to start ssh bridge: {err}")))?;
    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "ssh bridge stdin missing"))?;
    let mut child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "ssh bridge stdout missing"))?;
    let mut stream_to_child = stream.try_clone()?;
    let mut child_to_stream = stream;

    let upload = thread::spawn(move || {
        let _ = copy_flush(&mut stream_to_child, &mut child_stdin);
    });
    let download = thread::spawn(move || {
        let _ = copy_flush(&mut child_stdout, &mut child_to_stream);
        let _ = child_to_stream.shutdown(std::net::Shutdown::Write);
    });

    let status = child.wait()?;
    let _ = upload.join();
    let _ = download.join();

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            format!("ssh bridge exited with {status}"),
        ))
    }
}

fn copy_flush<R: io::Read, W: io::Write>(reader: &mut R, writer: &mut W) -> io::Result<u64> {
    let mut buffer = [0_u8; 16 * 1024];
    let mut total = 0;

    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(bytes_read) => bytes_read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };

        writer.write_all(&buffer[..bytes_read])?;
        writer.flush()?;
        total += bytes_read as u64;
    }
}

fn run_client_process(
    local_socket: &Path,
    reattach_command: &str,
    keybindings: RemoteKeybindings,
) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let status = Command::new(exe)
        .arg("client")
        .env(
            crate::server::socket_paths::CLIENT_SOCKET_PATH_ENV_VAR,
            local_socket,
        )
        .env("OMH_RENDER_ENCODING", "terminal-ansi")
        .env(REATTACH_COMMAND_ENV_VAR, reattach_command)
        .env(REMOTE_KEYBINDINGS_ENV_VAR, keybindings.as_str())
        .env_remove(crate::api::SOCKET_PATH_ENV_VAR)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            format!("remote client exited with {status}"),
        ))
    }
}

fn local_forward_socket_path(target: &str, session_name: &str) -> PathBuf {
    let pid = std::process::id();
    let target_clean = sanitize_path_component(target);
    let session_clean = sanitize_path_component(session_name);

    let tmpdir = std::env::temp_dir();
    let readable = tmpdir.join(format!(
        "omh-remote-{pid}-{target_clean}-{session_clean}.sock"
    ));
    if fits_unix_socket_path(&readable) {
        return readable;
    }

    // macOS' per-user TMPDIR (~49 chars under /var/folders/...) can push the
    // readable name past sun_path's 104-byte ceiling. Fall back to a hashed
    // short name in TMPDIR, then to /tmp as a last resort when TMPDIR itself
    // is longer than the budget. The hash covers the full unsanitized
    // target/session so uniqueness does not depend on the prefix truncation;
    // the prefix is kept only for debuggability.
    let target_prefix: String = target_clean.chars().take(8).collect();
    let hash = short_socket_hash(target, session_name);
    let short_name = format!("omh-r-{pid}-{target_prefix}-{hash}.sock");
    let short_in_tmp = tmpdir.join(&short_name);
    if fits_unix_socket_path(&short_in_tmp) {
        return short_in_tmp;
    }
    PathBuf::from("/tmp").join(short_name)
}

fn fits_unix_socket_path(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    // sun_path is byte-limited: 104 bytes on macOS, 108 on Linux. Reserve
    // 1 byte for the trailing NUL and use the smaller cap for portability.
    const MAX: usize = 103;
    path.as_os_str().as_bytes().len() <= MAX
}

fn short_socket_hash(target: &str, session: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    target.hash(&mut hasher);
    0u8.hash(&mut hasher);
    session.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn sanitize_path_component(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect();

    sanitized.trim_matches('-').chars().take(32).collect()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkerInstallKind {
    Install,
    Update,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkerInstallPreview {
    pub(crate) kind: WorkerInstallKind,
    pub(crate) source: String,
    pub(crate) target_path: String,
    pub(crate) checksum: String,
    pub(crate) version: String,
    pub(crate) commands: Vec<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) already_current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WorkerInstallReport {
    Installed(WorkerInstallPreview),
    AlreadyCurrent(WorkerInstallPreview),
}

#[derive(Debug)]
pub(crate) enum ExecutionWorkerTransportError {
    BootstrapRequired {
        target: String,
        expected_version: String,
        versioned_remote_path: String,
    },
    Io(io::Error),
}

impl fmt::Display for ExecutionWorkerTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BootstrapRequired {
                target,
                expected_version,
                versioned_remote_path,
            } => write!(
                formatter,
                "execution worker setup is required for {target}: approve installation of version {expected_version} at {versioned_remote_path}"
            ),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ExecutionWorkerTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BootstrapRequired { .. } => None,
            Self::Io(error) => Some(error),
        }
    }
}

impl From<io::Error> for ExecutionWorkerTransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) struct ExecutionWorkerTransport {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    _ssh: RemoteSsh,
}

impl ExecutionWorkerTransport {
    pub(crate) fn stdin_mut(&mut self) -> io::Result<&mut ChildStdin> {
        self.stdin
            .as_mut()
            .ok_or_else(|| io::Error::other("execution worker stdin is unavailable"))
    }

    pub(crate) fn take_stdin(&mut self) -> io::Result<ChildStdin> {
        self.stdin
            .take()
            .ok_or_else(|| io::Error::other("execution worker stdin is unavailable"))
    }

    pub(crate) fn take_stdout(&mut self) -> io::Result<ChildStdout> {
        self.stdout
            .take()
            .ok_or_else(|| io::Error::other("execution worker stdout is unavailable"))
    }

    pub(crate) fn take_stderr(&mut self) -> io::Result<ChildStderr> {
        self.stderr
            .take()
            .ok_or_else(|| io::Error::other("execution worker stderr is unavailable"))
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    #[cfg(test)]
    pub(crate) fn blocked_for_test() -> io::Result<Self> {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        Ok(Self {
            child,
            stdin,
            stdout,
            stderr,
            _ssh: RemoteSsh::new("blocked-test".to_string(), false),
        })
    }
}
fn worker_install_source_metadata(platform: &RemotePlatform) -> io::Result<(String, String)> {
    let override_binary = remote_binary_override_path()?;
    if let Some(path) = override_binary {
        let checksum = crate::checksum::file_sha256(&path)?;
        return Ok((
            format!("{REMOTE_BINARY_ENV_VAR} ({})", path.display()),
            checksum,
        ));
    }
    if *platform == RemotePlatform::local() {
        let path = std::env::current_exe()?;
        let checksum = crate::checksum::file_sha256(&path)?;
        return Ok((format!("current executable ({})", path.display()), checksum));
    }
    let (url, checksum) = release_worker_asset(platform)?;
    Ok((url, checksum))
}

pub(crate) fn preview_execution_worker_install(
    target: &str,
    askpass: crate::execution_host::auth::AskpassCommandConfig,
) -> io::Result<WorkerInstallPreview> {
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    preview_execution_worker_install_with_ssh(&ssh)
}

fn preview_execution_worker_install_with_ssh(ssh: &RemoteSsh) -> io::Result<WorkerInstallPreview> {
    let platform = detect_remote_platform(ssh)?;
    let remote_omh = execution_worker_remote_omh(platform.clone());
    let already_current = remote_worker_binary_matches(ssh, &remote_omh)?;
    let target_exists = remote_binary_exists(ssh, &remote_omh)?;
    let has_previous = !remote_binary_candidates(ssh, &remote_omh)?.is_empty();
    let (source, checksum) = worker_install_source_metadata(&platform)?;
    Ok(WorkerInstallPreview {
        kind: if target_exists || has_previous {
            WorkerInstallKind::Update
        } else {
            WorkerInstallKind::Install
        },
        source,
        target_path: format!("$HOME/{}", remote_omh.install_suffix),
        checksum,
        version: CURRENT_VERSION.to_string(),
        commands: vec![
            "omh execution-worker --protocol-version".to_string(),
            "omh execution-worker --daemon-lifecycle-version".to_string(),
            "omh execution-worker --daemon <binding>".to_string(),
            "omh execution-worker".to_string(),
        ],
        capabilities: vec![
            "terminal".to_string(),
            "path_completion".to_string(),
            "process_observation".to_string(),
            "git".to_string(),
            "worktree".to_string(),
            "command".to_string(),
            "agent".to_string(),
            "ports".to_string(),
            "file_staging".to_string(),
            "daemon_lifecycle_v1".to_string(),
        ],
        already_current,
    })
}

pub(crate) fn install_execution_worker(
    target: &str,
    askpass: crate::execution_host::auth::AskpassCommandConfig,
    approved: &WorkerInstallPreview,
) -> io::Result<WorkerInstallReport> {
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    let platform = detect_remote_platform(&ssh)?;
    let remote_omh = execution_worker_remote_omh(platform.clone());
    let current = preview_execution_worker_install_with_ssh(&ssh)?;
    if &current != approved {
        return Err(io::Error::other(
            "execution worker install plan changed; review and approve the new plan",
        ));
    }
    if current.already_current {
        return Ok(WorkerInstallReport::AlreadyCurrent(current));
    }
    if remote_binary_exists(&ssh, &remote_omh)? {
        return Err(io::Error::other(format!(
            "refusing to replace incompatible execution worker at {}; stop it or choose a new versioned target",
            current.target_path
        )));
    }
    let source = resolve_install_source(&platform, remote_binary_override_path()?)?;
    let verified = crate::checksum::verify_sha256(&source.path, &current.checksum);
    if let Err(error) = verified {
        source.cleanup();
        return Err(io::Error::other(format!(
            "execution worker source checksum verification failed: {error}"
        )));
    }
    // Side-by-side versioned artifact only. Never touch an incumbent daemon's
    // socket, lock, or process; activation is a separate lifecycle step.
    let install_result = ssh.install_omh(&remote_omh, &source.path);
    source.cleanup();
    install_result?;
    if !remote_worker_binary_matches(&ssh, &remote_omh)? {
        return Err(io::Error::other(
            "staged execution worker failed version/protocol/lifecycle verification",
        ));
    }
    Ok(WorkerInstallReport::Installed(current))
}

impl ExecutionWorkerTransport {
    pub(crate) fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }
}

impl Drop for ExecutionWorkerTransport {
    fn drop(&mut self) {
        self.stdin = None;
        if !matches!(self.try_wait(), Ok(Some(_))) {
            let _ = self.kill();
        }
        let _ = self.child.wait();
    }
}

pub(crate) fn spawn_execution_worker_cancellable(
    target: &str,
    askpass: crate::execution_host::auth::AskpassCommandConfig,
    cancel: Option<&ConnectCancel>,
) -> Result<ExecutionWorkerTransport, ExecutionWorkerTransportError> {
    if let Some(cancel) = cancel {
        cancel.check()?;
    }
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    let platform = detect_remote_platform_cancellable(&ssh, cancel)?;
    let remote_omh = execution_worker_remote_omh(platform);

    if !remote_worker_binary_matches_cancellable(&ssh, &remote_omh, cancel)? {
        return Err(ExecutionWorkerTransportError::BootstrapRequired {
            target: target.to_string(),
            expected_version: CURRENT_VERSION.to_string(),
            versioned_remote_path: remote_omh.install_suffix.clone(),
        });
    }

    if let Some(cancel) = cancel {
        cancel.check()?;
    }

    let mut child = ssh
        .command()
        .arg(execution_worker_command(&remote_omh))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("failed to start remote execution worker: {err}"),
            )
        })?;

    if let Some(cancel) = cancel {
        if cancel.is_cancelled() {
            kill_child_tree(&mut child);
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "ssh connection attempt cancelled",
            )
            .into());
        }
    }

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if stdin.is_none() || stdout.is_none() || stderr.is_none() {
        kill_child_tree(&mut child);
        return Err(io::Error::other("remote execution worker stdio was not available").into());
    }

    Ok(ExecutionWorkerTransport {
        child,
        stdin,
        stdout,
        stderr,
        _ssh: ssh,
    })
}

fn execution_worker_remote_omh(platform: RemotePlatform) -> RemoteOmh {
    let install_suffix = format!(
        ".local/share/omh/execution-workers/{CURRENT_VERSION}/{}/omh",
        platform.asset_key()
    );
    RemoteOmh {
        shell_path: format!("\"$HOME/{install_suffix}\""),
        install_suffix,
        platform,
    }
}

fn execution_worker_command(remote_omh: &RemoteOmh) -> String {
    format!("{} execution-worker", remote_omh.shell_path)
}
fn remote_worker_binary_matches(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<bool> {
    remote_worker_binary_matches_cancellable(ssh, remote_omh, None)
}

fn remote_worker_binary_matches_cancellable(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
    cancel: Option<&ConnectCancel>,
) -> io::Result<bool> {
    let output = ssh.sh_output_cancellable(&execution_worker_probe_command(remote_omh), cancel)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let expected_version = format!("omh {CURRENT_VERSION}");
    let expected_protocol = crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION.to_string();
    let expected_lifecycle = crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION.to_string();
    Ok(output.status.success()
        && lines.next() == Some(expected_version.as_str())
        && lines.next() == Some(expected_protocol.as_str())
        && lines.next() == Some(expected_lifecycle.as_str()))
}

fn execution_worker_probe_command(remote_omh: &RemoteOmh) -> String {
    format!(
        "test -x {0} && {0} --version && {0} execution-worker --protocol-version && {0} execution-worker --daemon-lifecycle-version",
        remote_omh.shell_path
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connect_cancel_interrupts_wait_loop() {
        let cancel = ConnectCancel::new();
        cancel.cancel();
        let err = cancel.check().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(err.to_string().contains("cancelled"));
    }

    #[test]
    fn execution_worker_uses_a_role_isolated_versioned_artifact() {
        let remote_omh = execution_worker_remote_omh(RemotePlatform {
            os: "linux",
            arch: "aarch64",
        });

        assert_eq!(
            remote_omh.install_suffix,
            format!(".local/share/omh/execution-workers/{CURRENT_VERSION}/linux-aarch64/omh")
        );
        assert_eq!(
            execution_worker_command(&remote_omh),
            format!(
                "\"$HOME/.local/share/omh/execution-workers/{CURRENT_VERSION}/linux-aarch64/omh\" execution-worker"
            )
        );
        let probe = execution_worker_probe_command(&remote_omh);
        assert!(probe.contains("execution-worker --protocol-version"));
        assert!(probe.contains("execution-worker --daemon-lifecycle-version"));
        assert!(!probe.contains("status"));
        assert!(!probe.contains("server"));
        assert!(!probe.contains("session"));
        assert!(
            !probe.contains("worker.sock") && !probe.contains("worker.lock"),
            "installer probe must not touch incumbent socket/lock paths"
        );
    }

    #[test]
    fn bridge_socket_is_user_only() {
        use std::os::unix::fs::PermissionsExt;

        let socket = std::env::temp_dir().join(format!(
            "omh-bridge-permissions-test-{}.sock",
            std::process::id()
        ));
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let bridge = SshStdioBridge::start(
            "example".to_string(),
            remote_omh,
            socket.clone(),
            "default".to_string(),
            None,
        )
        .expect("start bridge listener");

        let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, BRIDGE_SOCKET_PERMISSION_MODE);

        drop(bridge);
        let _ = std::fs::remove_file(socket);
    }

    #[test]
    fn managed_ssh_config_includes_user_config_then_fallback() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let home =
            std::env::temp_dir().join(format!("omh-keepalive-home-test-{}", std::process::id()));
        let ssh_dir = home.join(".ssh");
        let user_config = ssh_dir.join("config");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&ssh_dir).unwrap();
        std::fs::write(&user_config, "Host example\n  User omh\n").unwrap();
        let _home = crate::config::TestEnvVar::set("HOME", home.as_os_str());

        let managed_config = write_managed_ssh_config().expect("write managed config");
        let path = managed_config.options.config_path.clone();
        let control_path = managed_config.options.control_path.clone();
        let contents = std::fs::read_to_string(&path).expect("read managed config");

        let include = format!(
            "Include {}",
            ssh_config_quote(&user_config.to_string_lossy())
        );
        assert!(
            contents.starts_with(&format!("{include}\n")),
            "user config Include must be first: {contents}"
        );
        assert!(
            contents.ends_with("Host *\n  ServerAliveInterval 15\n  ServerAliveCountMax 4\n"),
            "config should end with Oh My Herdr's keepalive fallback block: {contents}"
        );
        assert!(!contents.contains("ControlMaster"));
        assert!(!contents.contains("ControlPersist"));
        assert!(!contents.contains("ControlPath"));

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, BRIDGE_SOCKET_PERMISSION_MODE);
        let dir = path.parent().expect("config has a parent dir");
        let dir_mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "ssh config dir must be user-only");
        assert!(fits_unix_socket_path(&control_path));

        drop(managed_config);
        assert!(!dir.exists(), "managed ssh directory should be cleaned up");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn remote_ssh_command_uses_managed_config_when_present() {
        let managed_config = write_managed_ssh_config().expect("write managed config");
        let config_path = managed_config.options.config_path.clone();
        let control_path = managed_config.options.control_path.clone();
        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: Some(managed_config),
            askpass: None,
        };

        let command = ssh.command();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "-F".to_string(),
                config_path.to_string_lossy().into_owned(),
                "-S".to_string(),
                control_path.to_string_lossy().into_owned(),
                "-o".to_string(),
                "ControlMaster=auto".to_string(),
                "-o".to_string(),
                "ControlPersist=60".to_string(),
                "-T".to_string(),
                "example".to_string(),
            ]
        );
    }

    #[test]
    fn remote_ssh_command_is_plain_without_managed_config() {
        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: None,
            askpass: None,
        };

        let command = ssh.command();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(args, vec!["-T".to_string(), "example".to_string()]);
    }

    #[test]
    fn ssh_config_quote_wraps_path_with_spaces() {
        assert_eq!(
            ssh_config_quote("/home/a b/.ssh/config"),
            "\"/home/a b/.ssh/config\""
        );
    }

    #[test]
    fn extract_remote_args_removes_space_form() {
        let args = vec![
            "omh".into(),
            "--remote".into(),
            "dev".into(),
            "--help".into(),
        ];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["omh", "--help"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "dev");
        assert_eq!(remote.keybindings, RemoteKeybindings::Local);
    }

    #[test]
    fn extract_remote_args_removes_equals_form() {
        let args = vec!["omh".into(), "--remote=user@host".into()];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["omh"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "user@host");
        assert_eq!(remote.keybindings, RemoteKeybindings::Local);
    }

    #[test]
    fn extract_remote_args_accepts_remote_keybindings_server() {
        let args = vec![
            "omh".into(),
            "--remote".into(),
            "dev".into(),
            "--remote-keybindings=server".into(),
        ];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["omh"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "dev");
        assert_eq!(remote.keybindings, RemoteKeybindings::Server);
    }

    #[test]
    fn extract_remote_args_accepts_remote_keybindings_space_form() {
        let args = vec![
            "omh".into(),
            "--remote=dev".into(),
            "--remote-keybindings".into(),
            "server".into(),
        ];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["omh"]);
        assert_eq!(remote.unwrap().keybindings, RemoteKeybindings::Server);
    }

    #[test]
    fn extract_remote_args_accepts_explicit_handoff() {
        let args = vec!["omh".into(), "--remote=dev".into(), "--handoff".into()];

        let (cleaned, remote) = extract_remote_args(&args).unwrap();

        assert_eq!(cleaned, vec!["omh"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "dev");
        assert!(remote.live_handoff);
    }

    #[test]
    fn extract_remote_args_preserves_child_remote_options_after_separator() {
        let args = vec![
            "omh".into(),
            "agent".into(),
            "start".into(),
            "repro".into(),
            "--".into(),
            "child".into(),
            "--remote".into(),
            "dev".into(),
            "--remote-keybindings=server".into(),
            "--handoff".into(),
        ];

        let (cleaned, remote) = extract_remote_args(&args).unwrap();

        assert_eq!(cleaned, args);
        assert!(remote.is_none());
    }

    #[test]
    fn extract_remote_args_preserves_handoff_without_remote() {
        let args = vec!["omh".into(), "update".into(), "--handoff".into()];

        let (cleaned, remote) = extract_remote_args(&args).unwrap();

        assert_eq!(cleaned, args);
        assert!(remote.is_none());
    }

    #[test]
    fn extract_remote_args_rejects_remote_keybindings_without_remote() {
        let args = vec!["omh".into(), "--remote-keybindings=server".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote-keybindings requires --remote");
    }

    #[test]
    fn extract_remote_args_rejects_duplicate_remote_keybindings() {
        let args = vec![
            "omh".into(),
            "--remote=dev".into(),
            "--remote-keybindings=local".into(),
            "--remote-keybindings=server".into(),
        ];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote-keybindings can only be specified once");
    }

    #[test]
    fn extract_remote_args_requires_value() {
        let args = vec!["omh".into(), "--remote".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "missing value for --remote");
    }

    #[test]
    fn extract_remote_args_rejects_empty_value() {
        let args = vec!["omh".into(), "--remote=".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "missing value for --remote");
    }

    #[test]
    fn extract_remote_args_rejects_duplicate_values() {
        let args = vec!["omh".into(), "--remote=dev".into(), "--remote=prod".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote can only be specified once");
    }

    #[test]
    fn extract_remote_args_rejects_option_like_target() {
        let args = vec!["omh".into(), "--remote".into(), "-oProxyCommand=x".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote target must not start with '-'");
    }

    #[test]
    fn sanitize_path_component_removes_shell_sensitive_chars() {
        assert_eq!(sanitize_path_component("user@host:22"), "user-host-22");
    }

    #[test]
    fn remote_platform_maps_uname_values() {
        assert_eq!(
            RemotePlatform::from_uname("Linux", "amd64")
                .unwrap()
                .asset_key(),
            "linux-x86_64"
        );
        assert_eq!(
            RemotePlatform::from_uname("Darwin", "arm64")
                .unwrap()
                .asset_key(),
            "macos-aarch64"
        );
        assert!(RemotePlatform::from_uname("FreeBSD", "x86_64").is_none());
    }

    #[test]
    fn reattach_command_includes_remote_and_session() {
        assert_eq!(
            reattach_command(
                "target/release/omh",
                "user@host",
                "work",
                RemoteKeybindings::Local,
                false,
            ),
            "target/release/omh --remote user@host --session work"
        );
        assert_eq!(
            reattach_command(
                "omh",
                "host name",
                crate::session::DEFAULT_SESSION_NAME,
                RemoteKeybindings::Local,
                false,
            ),
            "omh --remote 'host name'"
        );
        assert_eq!(
            reattach_command(
                "omh",
                "host",
                crate::session::DEFAULT_SESSION_NAME,
                RemoteKeybindings::Server,
                false,
            ),
            "omh --remote host --remote-keybindings server"
        );
        assert_eq!(
            reattach_command(
                "omh",
                "host",
                crate::session::DEFAULT_SESSION_NAME,
                RemoteKeybindings::Local,
                true,
            ),
            "omh --remote host --handoff"
        );
    }

    #[test]
    fn remote_bridge_command_uses_installed_binary() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        assert_eq!(
            remote_bridge_command(&remote_omh, crate::session::DEFAULT_SESSION_NAME),
            "exec \"$HOME/.local/bin/omh\" remote-client-bridge"
        );
    }

    #[test]
    fn remote_lifecycle_commands_omit_session_for_default() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let session = crate::session::DEFAULT_SESSION_NAME;

        assert_eq!(
            remote_server_status_command(&remote_omh, session),
            "\"$HOME/.local/bin/omh\" status server --json"
        );
        assert_eq!(
            remote_server_stop_command(&remote_omh, session),
            "\"$HOME/.local/bin/omh\" server stop"
        );
        assert_eq!(
            remote_server_live_handoff_command(&remote_omh, session),
            format!(
                "\"$HOME/.local/bin/omh\" server live-handoff --import-exe \"$HOME/.local/bin/omh\" --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}"
            )
        );
        assert_eq!(
            remote_bridge_command(&remote_omh, session),
            "exec \"$HOME/.local/bin/omh\" remote-client-bridge"
        );
    }

    #[test]
    fn remote_lifecycle_commands_qualify_named_session() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let session = "work";

        assert_eq!(
            remote_server_status_command(&remote_omh, session),
            "\"$HOME/.local/bin/omh\" --session work status server --json"
        );
        assert_eq!(
            remote_server_stop_command(&remote_omh, session),
            "\"$HOME/.local/bin/omh\" --session work server stop"
        );
        assert_eq!(
            remote_server_live_handoff_command(&remote_omh, session),
            format!(
                "\"$HOME/.local/bin/omh\" --session work server live-handoff --import-exe \"$HOME/.local/bin/omh\" --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}"
            )
        );
        assert_eq!(
            remote_bridge_command(&remote_omh, session),
            "exec \"$HOME/.local/bin/omh\" --session work remote-client-bridge"
        );
    }

    #[test]
    fn remote_lifecycle_commands_quote_session_names() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_omh =
            remote_omh_from_path_discovery(&remote_omh, "/usr/bin/omh\n").expect("path binary");
        let session = "my session";

        assert_eq!(
            remote_server_status_command(&remote_omh, session),
            "/usr/bin/omh --session 'my session' status server --json"
        );
        assert_eq!(
            remote_server_stop_command(&remote_omh, session),
            "/usr/bin/omh --session 'my session' server stop"
        );
        assert_eq!(
            remote_bridge_command(&remote_omh, session),
            "exec /usr/bin/omh --session 'my session' remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_uses_path_binary() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_omh =
            remote_omh_from_path_discovery(&remote_omh, "/usr/bin/omh\n").expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_omh, crate::session::DEFAULT_SESSION_NAME),
            "exec /usr/bin/omh remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_quotes_discovered_binary() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_omh =
            remote_omh_from_path_discovery(&remote_omh, "/opt/omh bin/omh\n").expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_omh, crate::session::DEFAULT_SESSION_NAME),
            "exec '/opt/omh bin/omh' remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_uses_macos_path_binary() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "macos",
            arch: "aarch64",
        });
        let remote_omh = remote_omh_from_path_discovery(&remote_omh, "/opt/homebrew/bin/omh\n")
            .expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_omh, crate::session::DEFAULT_SESSION_NAME),
            "exec /opt/homebrew/bin/omh remote-client-bridge"
        );
        assert_eq!(remote_omh.platform.asset_key(), "macos-aarch64");
    }

    #[test]
    fn remote_path_discovery_quotes_single_quotes_in_discovered_binary() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_omh = remote_omh_from_path_discovery(&remote_omh, "/opt/omh's/bin/omh\n")
            .expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_omh, crate::session::DEFAULT_SESSION_NAME),
            "exec '/opt/omh'\\''s/bin/omh' remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_ignores_relative_paths() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_omh = remote_omh_from_path_discovery(&remote_omh, "bin/omh\n");

        assert!(remote_omh.is_none());
    }

    #[test]
    fn remote_path_discovery_ignores_empty_output() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_omh = remote_omh_from_path_discovery(&remote_omh, "\n");

        assert!(remote_omh.is_none());
    }

    struct FakeSshResponse<'a> {
        status: i32,
        stdout: &'a str,
        stderr: &'a str,
    }

    fn fake_remote_path_probe<'a>(
        primary: FakeSshResponse<'a>,
        fallback: FakeSshResponse<'a>,
    ) -> (io::Result<Option<RemoteOmh>>, String) {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omh-remote-path-discovery-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("fake ssh directory should be created");
        let fake_ssh = dir.join("ssh");
        let log = dir.join("invocations");
        let script = format!(
            r#"#!/bin/sh
set -eu
last=''
for arg in "$@"; do
    last="$arg"
done
if [ "$last" = "command -v omh" ]; then
    printf '%s\n' primary >> {log}
    printf '%s' {primary_stdout}
    printf '%s' {primary_stderr} >&2
    exit {primary_status}
fi
if [ "$last" = "/bin/sh -s" ]; then
    while IFS= read -r _line; do :; done
    printf '%s\n' fallback >> {log}
    printf '%s' {fallback_stdout}
    printf '%s' {fallback_stderr} >&2
    exit {fallback_status}
fi
printf '%s\n' unexpected >> {log}
exit 99
"#,
            log = shell_quote(&log.to_string_lossy()),
            primary_stdout = shell_quote(primary.stdout),
            primary_stderr = shell_quote(primary.stderr),
            primary_status = primary.status,
            fallback_stdout = shell_quote(fallback.stdout),
            fallback_stderr = shell_quote(fallback.stderr),
            fallback_status = fallback.status,
        );
        fs::write(&fake_ssh, script).expect("fake ssh should be written");
        let mut permissions = fs::metadata(&fake_ssh)
            .expect("fake ssh should have metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake_ssh, permissions).expect("fake ssh should be executable");

        let mut path = OsString::from(dir.as_os_str());
        if let Some(existing) = std::env::var_os("PATH") {
            path.push(":");
            path.push(existing);
        }
        let _path = crate::config::TestEnvVar::set("PATH", path);
        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: None,
            askpass: None,
        };
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let result = remote_binary_on_path_any(&ssh, &remote_omh);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    fn fake_remote_binary_candidates_probe<'a>(
        primary: FakeSshResponse<'a>,
        fallback: FakeSshResponse<'a>,
        mise: FakeSshResponse<'a>,
    ) -> (io::Result<Vec<RemoteOmh>>, String) {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omh-remote-mise-discovery-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("fake ssh directory should be created");
        let fake_ssh = dir.join("ssh");
        let log = dir.join("invocations");
        let script = format!(
            r#"#!/bin/sh
set -eu
last=''
for arg in "$@"; do
    last="$arg"
done
if [ "$last" = "command -v omh" ]; then
    printf '%s\n' primary >> {log}
    printf '%s' {primary_stdout}
    printf '%s' {primary_stderr} >&2
    exit {primary_status}
fi
if [ "$last" = "/bin/sh -s" ]; then
    request=$(cat)
    case "$request" in
        *mise/installs/omh*)
            printf '%s\n' mise >> {log}
            printf '%s' {mise_stdout}
            printf '%s' {mise_stderr} >&2
            exit {mise_status}
            ;;
        *)
            printf '%s\n' fallback >> {log}
            printf '%s' {fallback_stdout}
            printf '%s' {fallback_stderr} >&2
            exit {fallback_status}
            ;;
    esac
fi
printf '%s\n' unexpected >> {log}
exit 99
"#,
            log = shell_quote(&log.to_string_lossy()),
            primary_stdout = shell_quote(primary.stdout),
            primary_stderr = shell_quote(primary.stderr),
            primary_status = primary.status,
            fallback_stdout = shell_quote(fallback.stdout),
            fallback_stderr = shell_quote(fallback.stderr),
            fallback_status = fallback.status,
            mise_stdout = shell_quote(mise.stdout),
            mise_stderr = shell_quote(mise.stderr),
            mise_status = mise.status,
        );
        fs::write(&fake_ssh, script).expect("fake ssh should be written");
        let mut permissions = fs::metadata(&fake_ssh)
            .expect("fake ssh should have metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake_ssh, permissions).expect("fake ssh should be executable");

        let mut path = OsString::from(dir.as_os_str());
        if let Some(existing) = std::env::var_os("PATH") {
            path.push(":");
            path.push(existing);
        }
        let _path = crate::config::TestEnvVar::set("PATH", path);
        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: None,
            askpass: None,
        };
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let result = remote_binary_candidates(&ssh, &remote_omh);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    #[test]
    fn remote_binary_candidates_discovers_mise_install() {
        let mise_paths = format!(
            "/home/can/.local/share/mise/installs/omh/{CURRENT_VERSION}/bin/omh\n\
             /home/can/.local/share/mise/installs/omh/{CURRENT_VERSION}/omh\n"
        );
        let (result, invocations) = fake_remote_binary_candidates_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "xonsh: command -v is not supported\n",
            },
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "sh: omh: not found\n",
            },
            FakeSshResponse {
                status: 0,
                stdout: &mise_paths,
                stderr: "",
            },
        );
        let candidates = result.expect("mise discovery should succeed");

        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0].shell_path,
            format!("/home/can/.local/share/mise/installs/omh/{CURRENT_VERSION}/bin/omh")
        );
        assert_eq!(
            candidates[1].shell_path,
            format!("/home/can/.local/share/mise/installs/omh/{CURRENT_VERSION}/omh")
        );
        assert_eq!(invocations, "primary\nfallback\nmise\n");
    }

    #[test]
    fn remote_binary_candidates_accept_mise_absence() {
        let (result, invocations) = fake_remote_binary_candidates_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "",
            },
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "",
            },
            FakeSshResponse {
                status: 0,
                stdout: "",
                stderr: "",
            },
        );

        assert!(result
            .expect("missing mise install should not abort discovery")
            .is_empty());
        assert_eq!(invocations, "primary\nfallback\nmise\n");
    }

    #[test]
    fn remote_binary_candidates_ignore_malformed_mise_output() {
        let (result, invocations) = fake_remote_binary_candidates_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "",
            },
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "",
            },
            FakeSshResponse {
                status: 0,
                stdout: "not-an-absolute-path\n./omh\n/home/can/.local/share/mise/shims/omh\n",
                stderr: "",
            },
        );

        assert!(result
            .expect("malformed mise output should not abort discovery")
            .is_empty());
        assert_eq!(invocations, "primary\nfallback\nmise\n");
    }

    #[test]
    fn remote_binary_candidates_keep_user_shell_then_sh_fallback_order() {
        let (result, invocations) = fake_remote_binary_candidates_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "xonsh: command -v is not supported\n",
            },
            FakeSshResponse {
                status: 0,
                stdout: "/fallback/bin/omh\n",
                stderr: "",
            },
            FakeSshResponse {
                status: 0,
                stdout: "",
                stderr: "",
            },
        );
        let candidates = result.expect("fallback discovery should succeed");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].shell_path, "/fallback/bin/omh");
        assert_eq!(invocations, "primary\nfallback\nmise\n");
    }

    #[test]
    fn remote_binary_candidates_preserve_mise_probe_diagnostics() {
        let (result, _invocations) = fake_remote_binary_candidates_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "",
            },
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "",
            },
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "mise probe failed\n",
            },
        );

        let error = result.expect_err("failed mise discovery should report an error");
        assert!(error.to_string().contains("mise probe failed"));
    }

    #[test]
    fn known_mise_candidate_script_checks_both_install_layouts() {
        let script = known_remote_binary_candidate_script();

        assert!(script.contains("emit \"$home/.local/share/mise/installs/omh/$version/bin/omh\""));
        assert!(script.contains("emit \"$home/.local/share/mise/installs/omh/$version/omh\""));
        assert!(script.contains(&format!("version={}", shell_quote(CURRENT_VERSION))));
        assert!(!script.contains("mise/shims/omh"));
    }

    #[test]
    fn remote_path_discovery_prefers_primary_user_shell_probe() {
        let (result, invocations) = fake_remote_path_probe(
            FakeSshResponse {
                status: 0,
                stdout: "/primary/bin/omh\n",
                stderr: "",
            },
            FakeSshResponse {
                status: 0,
                stdout: "/fallback/bin/omh\n",
                stderr: "",
            },
        );
        let remote_omh = result
            .expect("primary discovery should succeed")
            .expect("primary path should be returned");

        assert_eq!(remote_omh.shell_path, "/primary/bin/omh");
        assert_eq!(invocations, "primary\n");
    }

    #[test]
    fn remote_path_discovery_falls_back_to_posix_shell_after_primary_failure() {
        let (result, invocations) = fake_remote_path_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "xonsh: command -v is not supported\n",
            },
            FakeSshResponse {
                status: 0,
                stdout: "/fallback/bin/omh\n",
                stderr: "",
            },
        );
        let remote_omh = result
            .expect("fallback discovery should succeed")
            .expect("fallback path should be returned");

        assert_eq!(remote_omh.shell_path, "/fallback/bin/omh");
        assert_eq!(invocations, "primary\nfallback\n");
    }

    #[test]
    fn remote_path_discovery_keeps_install_flow_when_both_shell_probes_fail() {
        let (result, invocations) = fake_remote_path_probe(
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "xonsh: command -v is not supported\n",
            },
            FakeSshResponse {
                status: 1,
                stdout: "",
                stderr: "sh: omh: not found\n",
            },
        );

        assert!(result
            .expect("dual discovery failure should not abort installation")
            .is_none());
        assert_eq!(invocations, "primary\nfallback\n");
    }

    #[test]
    fn remote_shell_path_warning_accepts_managed_install() {
        assert!(remote_shell_resolves_managed_install(
            "/home/can/.local/bin/omh\n"
        ));
        assert!(remote_shell_resolves_managed_install(
            "/Users/can/.local/bin/omh\n"
        ));
        assert!(!remote_shell_resolves_managed_install(
            "/usr/local/bin/omh\n"
        ));
        assert!(!remote_shell_resolves_managed_install(""));
    }

    #[test]
    fn parse_client_status_json_reads_protocol() {
        assert_eq!(
            parse_client_status_json(r#"{"version":"x","protocol":8,"binary":"/bin/omh"}"#)
                .map(|status| status.protocol),
            Some(8)
        );
        assert!(parse_client_status_json(r#"{"protocol":"unknown"}"#).is_none());
    }

    #[test]
    fn parse_remote_server_status_json_reads_running_server() {
        assert_eq!(
            parse_remote_server_status_json(
                r#"{"status":"running","running":true,"version":"0.6.0","protocol":8,"capabilities":{"live_handoff":true}}"#
            )
            .unwrap(),
            RemoteServerStatus::Running {
                version: Some("0.6.0".into()),
                protocol: Some(8),
                live_handoff: true
            }
        );
    }

    #[test]
    fn parse_remote_server_status_json_treats_missing_capability_as_no_handoff() {
        assert_eq!(
            parse_remote_server_status_json(
                r#"{"status":"running","running":true,"version":"0.6.0","protocol":8}"#
            )
            .unwrap(),
            RemoteServerStatus::Running {
                version: Some("0.6.0".into()),
                protocol: Some(8),
                live_handoff: false
            }
        );
    }

    #[test]
    fn parse_remote_server_status_json_reads_stopped_server() {
        assert_eq!(
            parse_remote_server_status_json(
                r#"{"status":"not_running","running":false,"version":null,"protocol":null}"#
            )
            .unwrap(),
            RemoteServerStatus::NotRunning
        );
    }

    #[test]
    fn remote_server_restart_reason_requires_stop_for_protocol_mismatch() {
        assert_eq!(
            remote_server_restart_reason(Some(CURRENT_VERSION), Some(0), false),
            Some(RemoteServerRestartReason::ProtocolMismatch)
        );
    }

    #[test]
    fn remote_server_restart_reason_offers_restart_after_binary_update() {
        assert_eq!(
            remote_server_restart_reason(Some(CURRENT_VERSION), Some(CURRENT_PROTOCOL), true),
            Some(RemoteServerRestartReason::BinaryUpdated)
        );
    }

    #[test]
    fn remote_server_restart_reason_offers_restart_for_version_mismatch() {
        assert_eq!(
            remote_server_restart_reason(Some("0.0.0"), Some(CURRENT_PROTOCOL), false),
            Some(RemoteServerRestartReason::VersionMismatch)
        );
        assert_eq!(
            remote_server_restart_reason(None, Some(CURRENT_PROTOCOL), false),
            Some(RemoteServerRestartReason::VersionMismatch)
        );
    }

    #[test]
    fn remote_server_restart_reason_allows_current_server() {
        assert_eq!(
            remote_server_restart_reason(Some(CURRENT_VERSION), Some(CURRENT_PROTOCOL), false),
            None
        );
    }

    #[test]
    fn install_source_description_uses_override_binary() {
        let platform = RemotePlatform {
            os: "linux",
            arch: "aarch64",
        };
        assert_eq!(
            install_source_description(&platform, Some(Path::new("/tmp/omh-aarch64"))),
            "OMH_REMOTE_BINARY (/tmp/omh-aarch64)"
        );
    }

    #[test]
    fn worker_install_override_preview_has_verified_checksum() {
        let _environment = crate::integration::integration_env_lock();
        let path =
            std::env::temp_dir().join(format!("omh-worker-install-source-{}", std::process::id()));
        std::fs::write(&path, b"worker-source").unwrap();
        let _override = crate::config::TestEnvVar::set(REMOTE_BINARY_ENV_VAR, &path);
        let platform = RemotePlatform {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        };

        let (source, checksum) = worker_install_source_metadata(&platform).unwrap();

        assert!(source.contains(path.to_string_lossy().as_ref()));
        assert_eq!(checksum, crate::checksum::file_sha256(&path).unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resolve_install_source_uses_override_binary_without_temporary_cleanup() {
        let platform = RemotePlatform {
            os: "linux",
            arch: "aarch64",
        };
        let source = resolve_install_source(&platform, Some(PathBuf::from("/tmp/omh-aarch64")))
            .expect("override source");
        assert_eq!(source.path, PathBuf::from("/tmp/omh-aarch64"));
        assert!(source.temporary_dir.is_none());
    }

    #[test]
    fn remote_install_stream_command_avoids_shell_c_wrapper() {
        let command = remote_install_stream_command("/home/a b/.local/bin/omh.tmp.123");

        assert_eq!(command, "tee '/home/a b/.local/bin/omh.tmp.123'");
    }

    #[test]
    fn remote_install_prepare_and_commit_scripts_quote_paths() {
        let remote_omh = RemoteOmh::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let prepare = remote_install_prepare_script(&remote_omh);

        assert!(prepare.contains("mkdir -p \"$dir\""));
        assert!(prepare.contains("printf '%s\\0%s\\0' \"$tmp\" \"$dest\""));
        assert_eq!(
            parse_remote_install_paths(b"/home/a b/omh.tmp.42\0/home/a b/omh\0").unwrap(),
            (
                "/home/a b/omh.tmp.42".to_string(),
                "/home/a b/omh".to_string()
            )
        );
        assert_eq!(
            parse_remote_install_paths(b"/home/a b\n/omh.tmp.42\0/home/a b\n/omh\0").unwrap(),
            (
                "/home/a b\n/omh.tmp.42".to_string(),
                "/home/a b\n/omh".to_string()
            )
        );
        assert_eq!(
            remote_install_commit_script("/home/a b/omh.tmp.42", "/home/a b/omh"),
            "set -eu\nchmod 755 '/home/a b/omh.tmp.42'\nmv '/home/a b/omh.tmp.42' '/home/a b/omh'\n"
        );
    }

    fn remote_env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn socket_path_byte_len(path: &Path) -> usize {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().len()
    }

    #[test]
    fn local_forward_socket_path_uses_readable_name_when_it_fits() {
        let _guard = remote_env_lock().lock().unwrap();
        // Short target + session leave plenty of room — keep the human-
        // readable form so the socket path stays grep-friendly.
        let path = local_forward_socket_path("dev", "default");
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        assert!(
            filename.starts_with("omh-remote-"),
            "expected readable name, got {filename}"
        );
        assert!(filename.contains("-dev-default."), "got {filename}");
        assert!(
            fits_unix_socket_path(&path),
            "socket path too long: {} ({} bytes)",
            path.display(),
            socket_path_byte_len(&path)
        );
    }

    #[test]
    fn local_forward_socket_path_fits_in_sun_path() {
        let _guard = remote_env_lock().lock().unwrap();
        // Worst case for the readable form: macOS-style 49-char TMPDIR +
        // max-length sanitized components. Should fall back to the hashed
        // short name, which fits under TMPDIR.
        let tmpdir = PathBuf::from("/tmp").join(format!("omh-{}", "a".repeat(39)));
        let _ = fs::remove_dir_all(&tmpdir);
        fs::create_dir_all(&tmpdir).unwrap();
        let _tmpdir = crate::config::TestEnvVar::set("TMPDIR", tmpdir.as_os_str());
        let target = "longish-host.example.com";
        let session = "a-fairly-long-session-name-here";
        let path = local_forward_socket_path(target, session);
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert!(
            fits_unix_socket_path(&path),
            "socket path too long for sun_path: {} ({} bytes)",
            path.display(),
            socket_path_byte_len(&path)
        );
        assert_eq!(path.parent(), Some(tmpdir.as_path()));
        assert!(
            filename.starts_with("omh-r-"),
            "expected hashed name under macOS-style TMPDIR, got {filename}"
        );
        drop(_tmpdir);
        let _ = fs::remove_dir_all(tmpdir);
    }

    #[test]
    fn local_forward_socket_path_falls_back_to_tmp_when_dir_is_long() {
        let _guard = remote_env_lock().lock().unwrap();
        // Force a TMPDIR long enough that even the hashed short name cannot
        // fit inside it. The fallback should drop to /tmp.
        let long_dir = PathBuf::from("/tmp").join(format!("omh-{}", "a".repeat(80)));
        let _ = fs::create_dir_all(&long_dir);
        let _tmpdir = crate::config::TestEnvVar::set("TMPDIR", long_dir.as_os_str());

        let path = local_forward_socket_path("longish-host.example.com", "default");
        let fits = fits_unix_socket_path(&path);
        let parent = path.parent().map(Path::to_path_buf);
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        drop(_tmpdir);
        let _ = fs::remove_dir_all(&long_dir);

        assert!(fits, "fallback path still overflows: {}", path.display());
        assert_eq!(parent.as_deref(), Some(Path::new("/tmp")));
        assert!(
            filename.starts_with("omh-r-"),
            "expected hashed fallback, got {filename}"
        );
    }

    #[test]
    fn install_source_cleanup_removes_temporary_directory() {
        let dir = std::env::temp_dir().join(format!(
            "omh-install-source-cleanup-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("create temp dir");
        let path = dir.join("omh.tmp");
        fs::write(&path, b"test").expect("write temp file");

        InstallSource::temporary(path, dir.clone()).cleanup();

        assert!(!dir.exists());
    }

    fn fake_execution_worker_probe(
        probe_status: i32,
        probe_stdout: &str,
        probe_stderr: &str,
    ) -> (io::Result<bool>, String) {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("omh-worker-probe-{}-{unique}", std::process::id()));
        fs::create_dir_all(&dir).expect("fake ssh directory should be created");
        let fake_ssh = dir.join("ssh");
        let log = dir.join("invocations");
        let script = format!(
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> {log}
last=''
for arg in "$@"; do
    last="$arg"
done
if [ "$last" = "/bin/sh -s" ]; then
    script=$(cat)
    printf '%s\n' "$script" >> {log}
    printf '%s' {probe_stdout}
    printf '%s' {probe_stderr} >&2
    exit {probe_status}
fi
printf '%s\n' unexpected >> {log}
exit 99
"#,
            log = shell_quote(&log.to_string_lossy()),
            probe_stdout = shell_quote(probe_stdout),
            probe_stderr = shell_quote(probe_stderr),
            probe_status = probe_status,
        );
        fs::write(&fake_ssh, script).expect("fake ssh should be written");
        let mut permissions = fs::metadata(&fake_ssh)
            .expect("fake ssh should have metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake_ssh, permissions).expect("fake ssh should be executable");

        let mut path = OsString::from(dir.as_os_str());
        if let Some(existing) = std::env::var_os("PATH") {
            path.push(":");
            path.push(existing);
        }
        let _path = crate::config::TestEnvVar::set("PATH", path);
        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: None,
            askpass: None,
        };
        let remote_omh = execution_worker_remote_omh(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let result = remote_worker_binary_matches(&ssh, &remote_omh);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    #[test]
    fn remote_worker_probe_requires_app_protocol_and_lifecycle_v1() {
        let protocol = crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION;
        let lifecycle = crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION;
        let good = format!("omh {CURRENT_VERSION}\n{protocol}\n{lifecycle}\n");
        let (result, invocations) = fake_execution_worker_probe(0, &good, "");
        assert!(result.expect("probe should succeed"));
        assert!(invocations.contains("execution-worker --protocol-version"));
        assert!(invocations.contains("execution-worker --daemon-lifecycle-version"));
        assert!(!invocations.contains("worker.sock"));
        assert!(!invocations.contains("worker.lock"));
        assert!(!invocations.contains("flock"));
        assert!(!invocations.contains("kill"));

        let missing_lifecycle = format!("omh {CURRENT_VERSION}\n{protocol}\n");
        let (result, _) = fake_execution_worker_probe(0, &missing_lifecycle, "");
        assert!(!result.expect("probe should parse"));

        let wrong_lifecycle = format!("omh {CURRENT_VERSION}\n{protocol}\n0\n");
        let (result, _) = fake_execution_worker_probe(0, &wrong_lifecycle, "");
        assert!(!result.expect("probe should parse"));
    }

    #[test]
    fn execution_worker_install_scripts_are_side_by_side_only() {
        let remote_omh = execution_worker_remote_omh(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let prepare = remote_install_prepare_script(&remote_omh);
        let commit = remote_install_commit_script(
            "/tmp/omh.tmp",
            &format!("$HOME/{}", remote_omh.install_suffix),
        );
        let probe = execution_worker_probe_command(&remote_omh);

        assert!(
            prepare.contains(&remote_omh.install_suffix),
            "prepare must target versioned artifact path"
        );
        assert!(prepare.contains("mkdir -p"));
        assert!(!prepare.contains("worker.sock"));
        assert!(!prepare.contains("worker.lock"));
        assert!(!prepare.contains("kill"));
        assert!(!prepare.contains("flock"));
        assert!(!prepare.contains("execution-worker --daemon"));

        assert!(commit.contains("mv "));
        assert!(!commit.contains("worker.sock"));
        assert!(!commit.contains("worker.lock"));
        assert!(!commit.contains("kill"));
        assert!(!commit.contains("flock"));
        assert!(!commit.contains("execution-worker --daemon"));

        assert!(probe.contains("--daemon-lifecycle-version"));
        assert!(!probe.contains("worker.sock"));
        assert!(!probe.contains("worker.lock"));
    }

    fn fake_execution_worker_preview_probe(
        probe_stdout: &str,
        probe_status: i32,
    ) -> (io::Result<WorkerInstallPreview>, String) {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "omh-worker-preview-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("fake ssh directory should be created");
        let fake_ssh = dir.join("ssh");
        let log = dir.join("invocations");
        let local_bin = dir.join("local-omh");
        fs::write(&local_bin, b"#!/bin/sh\necho local\n").expect("write local binary");
        let mut bin_permissions = fs::metadata(&local_bin)
            .expect("local binary metadata")
            .permissions();
        bin_permissions.set_mode(0o700);
        fs::set_permissions(&local_bin, bin_permissions).expect("local binary executable");

        let script = format!(
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> {log}
last=''
for arg in "$@"; do
    last="$arg"
done
if [ "$last" = "/bin/sh -s" ]; then
    script=$(cat)
    printf '%s\n' "$script" >> {log}
    case "$script" in
      *uname*)
        printf 'Linux\n'
        printf 'x86_64\n'
        exit 0
        ;;
      *daemon-lifecycle-version*)
        printf '%s' {probe_stdout}
        exit {probe_status}
        ;;
      *'test -x'*)
        # exists/candidates probes without version lines
        if printf '%s' "$script" | grep -q -- '--version'; then
          printf '%s' {probe_stdout}
          exit {probe_status}
        fi
        exit {probe_status}
        ;;
      *'emit()'*)
        # No previous versioned worker candidates.
        exit 0
        ;;
      *'command -v'*|*mise*|*printf*)
        exit 1
        ;;
    esac
    printf '%s\n' "unmatched-script:$script" >> {log}
    exit 1
fi
printf '%s\n' unexpected >> {log}
exit 99
"#,
            log = shell_quote(&log.to_string_lossy()),
            probe_stdout = shell_quote(probe_stdout),
            probe_status = probe_status,
        );
        fs::write(&fake_ssh, script).expect("fake ssh should be written");
        let mut permissions = fs::metadata(&fake_ssh)
            .expect("fake ssh should have metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake_ssh, permissions).expect("fake ssh should be executable");

        let mut path = OsString::from(dir.as_os_str());
        if let Some(existing) = std::env::var_os("PATH") {
            path.push(":");
            path.push(existing);
        }
        let _path = crate::config::TestEnvVar::set("PATH", path);
        let _override =
            crate::config::TestEnvVar::set(REMOTE_BINARY_ENV_VAR, local_bin.as_os_str());

        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: None,
            askpass: None,
        };
        let result = preview_execution_worker_install_with_ssh(&ssh);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_override);
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    #[test]
    fn execution_worker_preview_requires_lifecycle_and_stages_only() {
        let protocol = crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION;
        let lifecycle = crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION;
        let good = format!("omh {CURRENT_VERSION}\n{protocol}\n{lifecycle}\n");
        let (result, invocations) = fake_execution_worker_preview_probe(&good, 0);
        let preview = result.expect("preview should succeed");
        assert!(preview.already_current);
        assert!(preview
            .commands
            .iter()
            .any(|command| command.contains("--daemon-lifecycle-version")));
        assert!(preview
            .capabilities
            .iter()
            .any(|capability| capability == "daemon_lifecycle_v1"));
        assert!(invocations.contains("execution-worker --daemon-lifecycle-version"));
        assert!(!invocations.contains("worker.sock"));
        assert!(!invocations.contains("worker.lock"));
        assert!(!invocations.contains("execution-worker --daemon "));

        let missing = format!("omh {CURRENT_VERSION}\n{protocol}\n");
        let (result, _) = fake_execution_worker_preview_probe(&missing, 0);
        let preview = result.expect("preview should succeed with stale worker");
        assert!(!preview.already_current);
    }

    #[test]
    fn worker_install_preview_reports_lifecycle_capability_without_activation() {
        let preview = WorkerInstallPreview {
            kind: WorkerInstallKind::Install,
            source: "test".into(),
            target_path: "$HOME/.local/share/omh/execution-workers/x/omh".into(),
            checksum: "abc".into(),
            version: CURRENT_VERSION.to_string(),
            commands: vec![
                "omh execution-worker --protocol-version".into(),
                "omh execution-worker --daemon-lifecycle-version".into(),
            ],
            capabilities: vec!["daemon_lifecycle_v1".into()],
            already_current: false,
        };
        assert!(preview
            .commands
            .iter()
            .any(|command| command.contains("--daemon-lifecycle-version")));
        assert!(preview
            .capabilities
            .iter()
            .any(|capability| capability == "daemon_lifecycle_v1"));
        assert!(!preview
            .commands
            .iter()
            .any(|command| command.contains("activate")));
    }
}
