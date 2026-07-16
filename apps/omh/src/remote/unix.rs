//! Remote thin-client launcher over SSH command stdio.

use std::fs::{self, File};
use std::io::{self, IsTerminal, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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
    let prepared_remote = prepare_remote_omh(&remote_ssh, remote.live_handoff)?;
    ensure_remote_server_ready(
        &remote_ssh,
        &prepared_remote.remote_omh,
        prepared_remote.installed_or_replaced,
        prepared_remote.stop_after_install_approved,
        remote.live_handoff,
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
        }
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
        command
    }

    fn sh_output(&self, script: &str) -> io::Result<Output> {
        let mut child = self
            .command()
            .arg("/bin/sh -s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let write_result = if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(script.as_bytes())
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "ssh bootstrap stdin missing",
            ))
        };
        let output = child.wait_with_output()?;
        write_result?;
        Ok(output)
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
        .arg("ControlPersist=yes");
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
) -> io::Result<PreparedRemoteOmh> {
    let platform = detect_remote_platform(ssh)?;
    let remote_omh = RemoteOmh::for_platform(platform);
    let override_binary = remote_binary_override_path()?;
    let path_remote_omh = remote_binary_on_path_any(ssh, &remote_omh)?;

    if override_binary.is_none() {
        if let Some(path_remote_omh) = path_remote_omh
            .as_ref()
            .filter(|candidate| remote_binary_matches(ssh, candidate).unwrap_or(false))
        {
            return Ok(PreparedRemoteOmh {
                remote_omh: path_remote_omh.clone(),
                installed_or_replaced: false,
                stop_after_install_approved: false,
            });
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
    if let Some(status_probe_omh) = path_remote_omh.as_ref().or_else(|| {
        remote_binary_exists(ssh, &remote_omh)
            .ok()
            .and_then(|exists| exists.then_some(&remote_omh))
    }) {
        let approved = confirm_remote_install_with_running_server(
            ssh,
            status_probe_omh,
            live_handoff_enabled,
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
    let output = ssh.sh_output("uname -s\nuname -m\n")?;
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

fn remote_binary_on_path_any(
    ssh: &RemoteSsh,
    remote_omh: &RemoteOmh,
) -> io::Result<Option<RemoteOmh>> {
    let output = ssh.user_shell_output("command -v omh")?;
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(remote_omh_from_path_discovery(remote_omh, &stdout))
}

fn remote_omh_from_path_discovery(remote_omh: &RemoteOmh, stdout: &str) -> Option<RemoteOmh> {
    let mut lines = stdout.lines();
    let path = lines.next()?;
    if !path.starts_with('/') {
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
) -> io::Result<()> {
    let status = remote_server_status(ssh, remote_omh)?;
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
        match live_handoff_remote_server(ssh, remote_omh) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!("remote live handoff failed: {err}");
                eprintln!("falling back to remote server restart.");
            }
        }
    }

    if stop_after_install_approved {
        stop_remote_server(ssh, remote_omh)?;
        return Ok(());
    }

    if confirm_remote_server_stop(ssh.target(), version.as_deref(), protocol, reason)? {
        stop_remote_server(ssh, remote_omh)?;
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
) -> io::Result<bool> {
    let target = ssh.target();
    let status = match remote_server_status(ssh, remote_omh) {
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

fn remote_server_status(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<RemoteServerStatus> {
    let command = format!("{} status server --json", remote_omh.shell_path);
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

fn live_handoff_remote_server(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<()> {
    let command = format!(
        "{} server live-handoff --import-exe {} --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}",
        remote_omh.shell_path,
        remote_omh.shell_path
    );
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

fn stop_remote_server(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<()> {
    let command = format!("{} server stop", remote_omh.shell_path);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server stop failed", &output));
    }

    wait_for_remote_server_shutdown(ssh, remote_omh)?;
    eprintln!(
        "stopped the remote Oh My Herdr server on {}; it will restart when the remote client bridge attaches.",
        ssh.target()
    );
    Ok(())
}

fn wait_for_remote_server_shutdown(ssh: &RemoteSsh, remote_omh: &RemoteOmh) -> io::Result<()> {
    let deadline = Instant::now() + REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT;
    loop {
        if remote_server_status(ssh, remote_omh)? == RemoteServerStatus::NotRunning {
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

fn download_release_asset(platform: &RemotePlatform) -> io::Result<InstallSource> {
    let release_output = Command::new("curl")
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

    let asset_key = platform.asset_key();
    let asset_name = format!("omh-{asset_key}");
    let url = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or_else(|| {
            io::Error::other(format!(
                "no {asset_name} binary in the latest GitHub release for omh {CURRENT_VERSION}"
            ))
        })?;

    let dir = private_download_dir(&asset_key)?;
    let path = dir.join("omh.tmp");
    let status = Command::new("curl")
        .args(["-sfL", "--max-time", "120", "-o"])
        .arg(&path)
        .arg(url)
        .status()
        .map_err(|err| io::Error::new(err.kind(), format!("download failed: {err}")))?;
    if !status.success() {
        let _ = fs::remove_dir_all(&dir);
        return Err(io::Error::other("download failed"));
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

fn remote_bridge_command(remote_omh: &RemoteOmh, session_name: &str) -> String {
    let mut command = format!("exec {}", remote_omh.shell_path);
    if session_name != crate::session::DEFAULT_SESSION_NAME {
        command.push_str(" --session ");
        command.push_str(&shell_quote(session_name));
    }
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
    if session_name != crate::session::DEFAULT_SESSION_NAME {
        command.push_str(" --session ");
        command.push_str(&shell_quote(session_name));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
                "ControlPersist=yes".to_string(),
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
}
