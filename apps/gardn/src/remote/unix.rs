//! Remote thin-client launcher over SSH command stdio.
use super::{ConnectCancel, WorkerInstallKind, WorkerInstallPreview, WorkerInstallReport};

use std::fmt;
use std::fs::{self, File};
use std::io::{self, IsTerminal, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{
    Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio,
};

use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BRIDGE_ACCEPT_POLL: Duration = Duration::from_millis(50);
const BRIDGE_SOCKET_PERMISSION_MODE: u32 = 0o600;
const REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);

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
const GITHUB_RELEASE_BY_TAG_API_URL: &str =
    "https://api.github.com/repos/masakirocorp/gardn/releases/tags";
const REMOTE_BINARY_ENV_VAR: &str = "GARDN_REMOTE_BINARY";
const DEV_WORKER_DATA_DIR_ENV_VAR: &str = "GARDN_DEV_WORKER_DIR";
const SSH_CONTROL_SOCKET_NAME: &str = "ctl";
pub(crate) const REATTACH_COMMAND_ENV_VAR: &str = "GARDN_REATTACH_COMMAND";

pub(crate) const REMOTE_KEYBINDINGS_ENV_VAR: &str = "GARDN_REMOTE_KEYBINDINGS";

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
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "gardn".to_string());
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
    let prepared_remote = prepare_remote_gardn(&remote_ssh, remote.live_handoff, &session_name)?;
    ensure_remote_server_ready(
        &remote_ssh,
        &prepared_remote.remote_gardn,
        prepared_remote.installed_or_replaced,
        prepared_remote.stop_after_install_approved,
        remote.live_handoff,
        &session_name,
    )?;

    let _bridge = SshStdioBridge::start(
        remote.target,
        prepared_remote.remote_gardn,
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
                "failed to connect to remote Gardn client socket {}: {err}",
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
            "the remote Gardn server must restart before this bridge can attach; rerun `gardn --remote` from an interactive terminal to approve stopping it",
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
struct RemoteGardn {
    install_suffix: String,
    shell_path: String,
    platform: RemotePlatform,
}

#[derive(Debug, Serialize)]
struct ArtifactManifest<'a> {
    schema_version: u32,
    sha256: &'a str,
    platform: &'a str,
    app_version: &'a str,
    build_channel: &'a str,
    build_cohort: &'a str,
    target: &'a str,
    client_protocol: u32,
    worker_protocol: u32,
    daemon_lifecycle_version: u16,
    capabilities: &'a [String],
    source: &'a str,
    installed_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkerArtifactInventoryEntry {
    path: String,
    last_used_unix_seconds: u64,
}

const WORKER_ARTIFACT_RETAIN_RECENT: usize = 2;
const WORKER_ARTIFACT_LEASE_GRACE_SECONDS: u64 = 7 * 24 * 60 * 60;

impl RemoteGardn {
    fn for_platform(platform: RemotePlatform) -> Self {
        let install_suffix = ".local/bin/gardn".to_string();
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

struct PreparedRemoteGardn {
    remote_gardn: RemoteGardn,
    installed_or_replaced: bool,
    stop_after_install_approved: bool,
}
#[derive(Clone)]
struct ManagedSshOptions {
    config_path: PathBuf,
    control_path: Option<PathBuf>,
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

    fn dedicated_command(&self) -> Command {
        let mut command = Command::new("ssh");
        if let Some(options) = self.options() {
            command.arg("-F").arg(&options.config_path);
        }
        command
            .arg("-o")
            .arg("ControlMaster=no")
            .arg("-o")
            .arg("ControlPath=none")
            .arg("-o")
            .arg("ControlPersist=no");
        if let Some(askpass) = &self.askpass {
            askpass.configure(&mut command);
        }
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

    fn install_gardn(
        &self,
        remote_gardn: &RemoteGardn,
        source_path: &Path,
        expected_checksum: &str,
        source_description: &str,
    ) -> io::Result<()> {
        let output = self.sh_output(&remote_install_prepare_script(remote_gardn))?;
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

            let staged_path = shell_quote(&tmp_path);
            let output = self.sh_output(&format!(
                "chmod 755 {staged_path} && {}",
                worker_build_info_command(&staged_path)
            ))?;
            let identity = parse_worker_build_identity(&output)?;
            validate_worker_build_identity(&identity, &remote_gardn.platform)?;
            let manifest = artifact_manifest(expected_checksum, source_description, &identity)?;
            let output = self.sh_output(&remote_install_commit_script(
                &tmp_path,
                &dest_path,
                expected_checksum,
                &manifest,
            ))?;
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
        let Some(_options) = self
            .managed_config
            .as_ref()
            .map(|config| &config.options)
            .filter(|options| options.control_path.is_some())
        else {
            return;
        };

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

    command.arg("-F").arg(&options.config_path);
    if let Some(control_path) = &options.control_path {
        command
            .arg("-S")
            .arg(control_path)
            .arg("-o")
            .arg("ControlMaster=auto")
            .arg("-o")
            .arg("ControlPersist=60");
    }
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

fn prepare_remote_gardn(
    ssh: &RemoteSsh,
    live_handoff_enabled: bool,
    session_name: &str,
) -> io::Result<PreparedRemoteGardn> {
    let platform = detect_remote_platform(ssh)?;
    let remote_gardn = RemoteGardn::for_platform(platform);
    let override_binary = remote_binary_override_path()?;
    let candidates = remote_binary_candidates(ssh, &remote_gardn)?;

    if override_binary.is_none() {
        for candidate in &candidates {
            if remote_binary_matches(ssh, candidate).unwrap_or(false) {
                return Ok(PreparedRemoteGardn {
                    remote_gardn: candidate.clone(),
                    installed_or_replaced: false,
                    stop_after_install_approved: false,
                });
            }
        }
        if remote_binary_matches(ssh, &remote_gardn)? {
            return Ok(PreparedRemoteGardn {
                remote_gardn,
                installed_or_replaced: false,
                stop_after_install_approved: false,
            });
        }
    }

    let mut stop_after_install_approved = false;
    if let Some(status_probe_gardn) = candidates.first().or_else(|| {
        remote_binary_exists(ssh, &remote_gardn)
            .ok()
            .and_then(|exists| exists.then_some(&remote_gardn))
    }) {
        let approved = confirm_remote_install_with_running_server(
            ssh,
            status_probe_gardn,
            live_handoff_enabled,
            session_name,
        )?;
        stop_after_install_approved = approved;
    }
    let source_description =
        install_source_description(&remote_gardn.platform, override_binary.as_deref());
    confirm_remote_install(ssh.target(), &remote_gardn, &source_description)?;
    let source = resolve_install_source(&remote_gardn.platform, override_binary)?;
    let checksum = crate::checksum::file_sha256(&source.path)?;
    let install_result =
        ssh.install_gardn(&remote_gardn, &source.path, &checksum, &source_description);
    source.cleanup();
    install_result?;

    if !remote_binary_matches(ssh, &remote_gardn)? {
        return Err(io::Error::other(format!(
            "installed remote Gardn at {}, but it did not report version {CURRENT_VERSION}",
            remote_gardn.shell_path
        )));
    }
    warn_if_remote_bin_not_on_path(ssh)?;

    Ok(PreparedRemoteGardn {
        remote_gardn,
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

fn remote_binary_candidates(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
) -> io::Result<Vec<RemoteGardn>> {
    let mut candidates = Vec::new();

    if let Some(path_candidate) = remote_binary_on_path_any(ssh, remote_gardn)? {
        candidates.push(path_candidate);
    }

    let output = ssh.sh_output(&known_remote_binary_candidate_script())?;
    if !output.status.success() {
        return Err(command_failed("remote binary discovery failed", &output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for candidate in remote_gardns_from_path_discovery(remote_gardn, &stdout) {
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
    emit "$home/.local/share/mise/installs/gardn/$version/bin/gardn"
    emit "$home/.local/share/mise/installs/gardn/$version/gardn"
fi
"#,
    );
    script
}

fn remote_binary_on_path_any(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
) -> io::Result<Option<RemoteGardn>> {
    let primary_output = ssh.user_shell_output("command -v gardn")?;
    if primary_output.status.success() {
        let stdout = String::from_utf8_lossy(&primary_output.stdout);
        if let Some(candidate) = remote_gardn_from_path_discovery(remote_gardn, &stdout) {
            return Ok(Some(candidate));
        }
    }

    // Non-POSIX login shells such as xonsh reject `command -v`; retry through
    // /bin/sh while retaining the login-shell probe for shell-initialized PATHs.
    let fallback_output = ssh.sh_output("command -v gardn\n")?;
    if fallback_output.status.success() {
        let stdout = String::from_utf8_lossy(&fallback_output.stdout);
        return Ok(remote_gardn_from_path_discovery(remote_gardn, &stdout));
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

fn remote_gardns_from_path_discovery(remote_gardn: &RemoteGardn, stdout: &str) -> Vec<RemoteGardn> {
    stdout
        .lines()
        .filter_map(|path| remote_gardn_from_path(remote_gardn, path))
        .collect()
}

fn remote_gardn_from_path_discovery(
    remote_gardn: &RemoteGardn,
    stdout: &str,
) -> Option<RemoteGardn> {
    stdout
        .lines()
        .find_map(|path| remote_gardn_from_path(remote_gardn, path))
}

fn remote_gardn_from_path(remote_gardn: &RemoteGardn, path: &str) -> Option<RemoteGardn> {
    let path = path.trim();
    if !path.starts_with('/') || path.ends_with("/mise/shims/gardn") {
        return None;
    }
    Some(remote_gardn.clone().with_shell_path(shell_quote(path)))
}

fn worker_build_info_command(shell_path: &str) -> String {
    format!(
        "test -x {0} && {0} execution-worker --build-info",
        shell_path
    )
}

fn parse_worker_build_identity(
    output: &Output,
) -> io::Result<crate::build_info::WorkerBuildIdentity> {
    if !output.status.success() {
        return Err(command_failed(
            "execution worker build identity probe failed",
            output,
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid execution worker build identity: {error}"),
        )
    })
}

fn validate_worker_build_identity(
    identity: &crate::build_info::WorkerBuildIdentity,
    platform: &RemotePlatform,
) -> io::Result<()> {
    let expected = crate::build_info::worker_identity();
    let expected_platform = platform.asset_key();
    let target_platform = crate::build_info::platform_for_target(&identity.target);
    let mut mismatches = Vec::new();
    if identity.app_version != expected.app_version {
        mismatches.push(format!(
            "app_version expected {}, got {}",
            expected.app_version, identity.app_version
        ));
    }
    if identity.build_channel != expected.build_channel {
        mismatches.push(format!(
            "build_channel expected {}, got {}",
            expected.build_channel, identity.build_channel
        ));
    }
    if identity.build_cohort != expected.build_cohort {
        mismatches.push(format!(
            "build_cohort expected {}, got {}",
            expected.build_cohort, identity.build_cohort
        ));
    }
    if identity.platform != expected_platform || target_platform != Some(expected_platform.as_str())
    {
        mismatches.push(format!(
            "target expected {expected_platform}, got {} ({})",
            identity.platform, identity.target
        ));
    }
    if identity.client_protocol != expected.client_protocol {
        mismatches.push(format!(
            "client_protocol expected {}, got {}",
            expected.client_protocol, identity.client_protocol
        ));
    }
    if identity.worker_protocol != expected.worker_protocol {
        mismatches.push(format!(
            "worker_protocol expected {}, got {}",
            expected.worker_protocol, identity.worker_protocol
        ));
    }
    if identity.daemon_lifecycle_version != expected.daemon_lifecycle_version {
        mismatches.push(format!(
            "daemon_lifecycle_version expected {}, got {}",
            expected.daemon_lifecycle_version, identity.daemon_lifecycle_version
        ));
    }
    if identity.capabilities != expected.capabilities {
        mismatches.push(format!(
            "capabilities expected {:?}, got {:?}",
            expected.capabilities, identity.capabilities
        ));
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "execution worker build identity mismatch: {}",
                mismatches.join("; ")
            ),
        ))
    }
}

fn remote_binary_matches(ssh: &RemoteSsh, remote_gardn: &RemoteGardn) -> io::Result<bool> {
    let output = ssh.sh_output(&worker_build_info_command(&remote_gardn.shell_path))?;
    let Ok(identity) = parse_worker_build_identity(&output) else {
        return Ok(false);
    };
    Ok(validate_worker_build_identity(&identity, &remote_gardn.platform).is_ok())
}

fn remote_binary_exists(ssh: &RemoteSsh, remote_gardn: &RemoteGardn) -> io::Result<bool> {
    let command = format!("test -x {}", remote_gardn.shell_path);
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

fn development_worker_bundle_path(platform: &RemotePlatform) -> PathBuf {
    let root = std::env::var_os(DEV_WORKER_DATA_DIR_ENV_VAR)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .map(|path| path.join(crate::config::app_dir_name()).join("workers"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                PathBuf::from(home)
                    .join(".local/share")
                    .join(crate::config::app_dir_name())
                    .join("workers")
            })
        })
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join(crate::config::app_dir_name())
                .join("workers")
        });
    root.join(crate::build_info::BUILD_COHORT)
        .join(format!("gardn-{}", platform.asset_key()))
}

fn install_source_description(platform: &RemotePlatform, override_binary: Option<&Path>) -> String {
    if let Some(path) = override_binary {
        return format!("{REMOTE_BINARY_ENV_VAR} ({})", path.display());
    }
    if *platform == RemotePlatform::local() {
        return "the current local Gardn binary".to_string();
    }
    if crate::build_info::is_official_release() {
        return format!(
            "the {} release asset for {}",
            crate::build_info::RELEASE_TAG,
            platform.asset_key()
        );
    }
    format!(
        "the matching development worker sidecar at {}",
        development_worker_bundle_path(platform).display()
    )
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
    if crate::build_info::is_official_release() {
        return download_release_asset(platform);
    }

    let path = development_worker_bundle_path(platform);
    let metadata = fs::metadata(&path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "matching {} development worker is not installed; run `just install-dev`, then retry",
                platform.asset_key()
            ),
        )
    })?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "development worker sidecar is not a file: {}",
                path.display()
            ),
        ));
    }
    Ok(InstallSource::persistent(path))
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
    remote_gardn: &RemoteGardn,
    remote_binary_changed: bool,
    stop_after_install_approved: bool,
    live_handoff_enabled: bool,
    session_name: &str,
) -> io::Result<()> {
    let status = remote_server_status(ssh, remote_gardn, session_name)?;
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
        match live_handoff_remote_server(ssh, remote_gardn, session_name) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!("remote live handoff failed: {err}");
                eprintln!("falling back to remote server restart.");
            }
        }
    }

    if stop_after_install_approved {
        stop_remote_server(ssh, remote_gardn, session_name)?;
        return Ok(());
    }

    if confirm_remote_server_stop(ssh.target(), version.as_deref(), protocol, reason)? {
        stop_remote_server(ssh, remote_gardn, session_name)?;
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
    remote_gardn: &RemoteGardn,
    live_handoff_enabled: bool,
    session_name: &str,
) -> io::Result<bool> {
    let target = ssh.target();
    let status = match remote_server_status(ssh, remote_gardn, session_name) {
        Ok(status) => status,
        Err(err) => {
            if !io::stdin().is_terminal() {
                return Err(io::Error::other(format!(
                    "could not inspect the running remote Gardn server on {target} before installing: {err}; run from an interactive terminal to approve updating the remote binary"
                )));
            }
            eprintln!(
                "could not inspect the running remote Gardn server on {target} before installing: {err}"
            );
            eprint!("continue installing the remote Gardn binary? [y/N] ");
            io::stderr().flush()?;

            let mut answer = String::new();
            io::stdin().read_line(&mut answer)?;
            let answer = answer.trim().to_ascii_lowercase();
            if answer != "y" && answer != "yes" {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "remote Gardn install cancelled",
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
            "remote Gardn server on {target} is running v{}; run from an interactive terminal to approve stopping it for the update",
            version_label(version.as_deref())
        )));
    }

    if live_handoff_enabled && live_handoff {
        eprintln!("remote Gardn server on {target} is currently running:");
        eprintln!("  server: v{}", version_label(version.as_deref()));
        eprintln!(
            "Gardn will install v{CURRENT_VERSION} and hand off live pane processes to the prepared server."
        );
        return Ok(false);
    }

    eprintln!("remote Gardn server on {target} is currently running:");
    eprintln!("  server: v{}", version_label(version.as_deref()));
    eprintln!(
        "To complete the remote update, Gardn must stop the running remote server after installing."
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
            "remote Gardn install cancelled",
        ));
    }

    Ok(true)
}

fn remote_server_status(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
    session_name: &str,
) -> io::Result<RemoteServerStatus> {
    let command = remote_server_status_command(remote_gardn, session_name);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server status failed", &output));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_remote_server_status_json(stdout.trim())
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
                "remote Gardn server on {target} must stop before this client can attach; run from an interactive terminal to approve stopping it"
            )));
        }

        eprintln!(
            "remote Gardn server on {target} is still running v{}; it will use v{CURRENT_VERSION} after it restarts.",
            version_label(version)
        );
        return Ok(false);
    }

    eprintln!("remote Gardn server on {target} is currently running:");
    eprintln!("  server: v{}", version_label(version));
    eprintln!("  prepared binary: v{CURRENT_VERSION}");
    eprintln!();

    match reason {
        RemoteServerRestartReason::ProtocolMismatch => {
            eprintln!("the remote server must stop before this client can attach.");
        }
        RemoteServerRestartReason::BinaryUpdated => {
            eprintln!(
                "the remote Gardn binary was installed or replaced. restart the remote server so it uses the prepared binary."
            );
        }
        RemoteServerRestartReason::VersionMismatch => {
            eprintln!(
                "the remote server is still running a different gardn version. restart it so it uses the prepared binary."
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
            "remote Gardn server stop cancelled",
        ));
    }

    Ok(false)
}

fn live_handoff_remote_server(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
    session_name: &str,
) -> io::Result<()> {
    let command = remote_server_live_handoff_command(remote_gardn, session_name);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server live handoff failed", &output));
    }

    eprintln!(
        "handed off the remote Gardn server on {}; reconnecting to the prepared server.",
        ssh.target()
    );
    Ok(())
}

fn stop_remote_server(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
    session_name: &str,
) -> io::Result<()> {
    let command = remote_server_stop_command(remote_gardn, session_name);
    let output = ssh.sh_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("remote server stop failed", &output));
    }

    wait_for_remote_server_shutdown(ssh, remote_gardn, session_name)?;
    eprintln!(
        "stopped the remote Gardn server on {}; it will restart when the remote client bridge attaches.",
        ssh.target()
    );
    Ok(())
}

fn wait_for_remote_server_shutdown(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
    session_name: &str,
) -> io::Result<()> {
    let deadline = Instant::now() + REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT;
    loop {
        if remote_server_status(ssh, remote_gardn, session_name)? == RemoteServerStatus::NotRunning
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "shutdown was requested, but the old remote Gardn server on {} is still responding after {} seconds",
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
    let output = ssh.user_shell_output("command -v gardn")?;
    if output.status.success()
        && remote_shell_resolves_managed_install(&String::from_utf8_lossy(&output.stdout))
    {
        return Ok(());
    }

    eprintln!(
        "gardn: installed remote binary to ~/.local/bin/gardn, but the remote shell does not resolve `gardn` to that path"
    );
    Ok(())
}

fn remote_shell_resolves_managed_install(stdout: &str) -> bool {
    stdout
        .lines()
        .next()
        .map(str::trim)
        .is_some_and(|path| path.ends_with("/.local/bin/gardn"))
}

fn release_worker_asset(platform: &RemotePlatform) -> io::Result<(String, String)> {
    if !crate::build_info::is_official_release() {
        return Err(io::Error::other(
            "development builds must use a matching local worker sidecar",
        ));
    }
    let release_tag = crate::build_info::RELEASE_TAG;
    let release_url = format!("{GITHUB_RELEASE_BY_TAG_API_URL}/{release_tag}");
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
            "User-Agent: gardn-remote-installer",
        ])
        .arg(&release_url)
        .output()
        .map_err(|err| io::Error::new(err.kind(), format!("curl failed: {err}")))?;
    if !release_output.status.success() {
        return Err(command_failed(
            &format!("failed to fetch GitHub release {release_tag}"),
            &release_output,
        ));
    }
    let release: RemoteGitHubRelease =
        serde_json::from_slice(&release_output.stdout).map_err(|err| {
            io::Error::other(format!(
                "failed to parse GitHub release {release_tag} JSON: {err}"
            ))
        })?;
    if release.tag_name != release_tag {
        return Err(io::Error::other(format!(
            "GitHub returned release {} for requested tag {release_tag}",
            release.tag_name
        )));
    }
    let asset_name = format!("gardn-{}", platform.asset_key());
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| {
            io::Error::other(format!(
                "no {asset_name} binary in GitHub release {release_tag}"
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
    let path = dir.join("gardn.tmp");
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
    let base = crate::platform::remote_private_temp_base();
    fs::create_dir_all(&base)?;
    for attempt in 0..100 {
        let dir = base.join(format!(
            "gardn-remote-{}-{}-{attempt}",
            std::process::id(),
            asset_key
        ));
        match crate::platform::create_remote_private_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to create private gardn remote download directory",
    ))
}

fn confirm_remote_install(
    target: &str,
    remote_gardn: &RemoteGardn,
    source_description: &str,
) -> io::Result<()> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::other(format!(
            "matching remote Gardn {CURRENT_VERSION} is not installed at {}; run from an interactive terminal to approve installation",
            remote_gardn.shell_path
        )));
    }

    eprintln!(
        "matching Gardn {CURRENT_VERSION} is not installed on {target} for {}.",
        remote_gardn.platform.asset_key()
    );
    eprint!(
        "Install {} to {}? [Y/n] ",
        source_description, remote_gardn.shell_path
    );
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim().to_ascii_lowercase();
    if answer == "n" || answer == "no" {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "remote Gardn installation cancelled",
        ));
    }

    Ok(())
}

fn remote_install_prepare_script(remote_gardn: &RemoteGardn) -> String {
    format!(
        r#"set -eu
dest="$HOME/{install_suffix}"
dir="${{dest%/*}}"
mkdir -p "$dir"
tmp="${{dest}}.tmp.$$"
printf '%s\0%s\0' "$tmp" "$dest"
"#,
        install_suffix = remote_gardn.install_suffix
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

fn remote_install_commit_script(
    tmp_path: &str,
    dest_path: &str,
    expected_checksum: &str,
    artifact_manifest: &str,
) -> String {
    format!(
        r#"set -eu
tmp={tmp_path}
dest={dest_path}
expected={expected_checksum}
lock="${{dest}}.install-lock"
manifest="${{dest}}.sha256"
metadata="${{dest}}.manifest.json"
metadata_json={artifact_manifest}
if [ -d "$lock" ] && find "$lock" -prune -mmin +5 | grep -q .; then
  rm -rf -- "$lock"
fi
if ! mkdir "$lock" 2>/dev/null; then
  printf '%s\n' "another Gardn worker install is publishing $dest" >&2
  exit 75
fi
cleanup() {{
  rm -f -- "$tmp"
  rm -rf -- "$lock"
}}
trap cleanup EXIT HUP INT TERM
checksum_file() {{
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    openssl dgst -sha256 "$1" | sed 's/^.*= //'
  fi
}}
actual=$(checksum_file "$tmp")
if [ "$actual" != "$expected" ]; then
  printf '%s\n' "uploaded Gardn worker checksum mismatch" >&2
  exit 65
fi
if [ -e "$dest" ] || [ -L "$dest" ]; then
  if [ -L "$dest" ] || [ ! -f "$dest" ] || [ ! -f "$manifest" ] || [ "$(cat "$manifest")" != "$expected" ]; then
    printf '%s\n' "refusing to replace unowned Gardn worker artifact $dest" >&2
    exit 73
  fi
  if [ "$(checksum_file "$dest")" = "$expected" ]; then
    printf '%s\n' "$metadata_json" > "${{metadata}}.tmp.$$"
    mv "${{metadata}}.tmp.$$" "$metadata"
    exit 0
  fi
fi
chmod 755 "$tmp"
mv -f "$tmp" "$dest"
printf '%s\n' "$expected" > "${{manifest}}.tmp.$$"
mv "${{manifest}}.tmp.$$" "$manifest"
printf '%s\n' "$metadata_json" > "${{metadata}}.tmp.$$"
mv "${{metadata}}.tmp.$$" "$metadata"
"#,
        tmp_path = shell_quote(tmp_path),
        dest_path = shell_quote(dest_path),
        expected_checksum = shell_quote(expected_checksum),
        artifact_manifest = shell_quote(artifact_manifest),
    )
}

fn append_remote_session_flag(command: &mut String, session_name: &str) {
    if session_name != crate::session::DEFAULT_SESSION_NAME {
        command.push_str(" --session ");
        command.push_str(&shell_quote(session_name));
    }
}

fn remote_server_status_command(remote_gardn: &RemoteGardn, session_name: &str) -> String {
    let mut command = remote_gardn.shell_path.clone();
    append_remote_session_flag(&mut command, session_name);
    command.push_str(" status server --json");
    command
}

fn remote_server_stop_command(remote_gardn: &RemoteGardn, session_name: &str) -> String {
    let mut command = remote_gardn.shell_path.clone();
    append_remote_session_flag(&mut command, session_name);
    command.push_str(" server stop");
    command
}

fn remote_server_live_handoff_command(remote_gardn: &RemoteGardn, session_name: &str) -> String {
    let mut command = remote_gardn.shell_path.clone();
    append_remote_session_flag(&mut command, session_name);
    command.push_str(&format!(
        " server live-handoff --import-exe {} --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}",
        remote_gardn.shell_path
    ));
    command
}

fn remote_bridge_command(remote_gardn: &RemoteGardn, session_name: &str) -> String {
    let mut command = format!("exec {}", remote_gardn.shell_path);
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
    let program = crate::platform::remote_reattach_program(program);
    let target = crate::platform::remote_reattach_argument(target);
    let mut command = format!("{program} --remote {target}");
    if keybindings != RemoteKeybindings::Local {
        command.push_str(" --remote-keybindings ");
        command.push_str(keybindings.as_str());
    }
    if live_handoff {
        command.push_str(" --handoff");
    }
    if session_name != crate::session::DEFAULT_SESSION_NAME {
        command.push_str(" --session ");
        command.push_str(&crate::platform::remote_reattach_argument(session_name));
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
        remote_gardn: RemoteGardn,
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
                        let stream = match prepare_remote_bridge_stream(stream) {
                            Ok(stream) => stream,
                            Err(err) => {
                                eprintln!(
                                    "gardn: remote bridge failed to prepare client socket: {err}"
                                );
                                continue;
                            }
                        };
                        if let Err(err) = bridge_connection(
                            stream,
                            &target,
                            &remote_gardn,
                            &session_name,
                            thread_ssh_options.as_ref(),
                        ) {
                            eprintln!("gardn: remote bridge failed: {err}");
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(BRIDGE_ACCEPT_POLL);
                    }
                    Err(err) => {
                        eprintln!("gardn: remote bridge listener failed: {err}");
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

fn prepare_remote_bridge_stream(stream: UnixStream) -> io::Result<UnixStream> {
    stream.set_nonblocking(false)?;
    Ok(stream)
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

fn ssh_config_quote(path: &str) -> String {
    format!("\"{path}\"")
}

fn write_managed_ssh_config() -> io::Result<ManagedSshConfig> {
    let paths = crate::platform::remote_ssh_config_paths();
    let dir = crate::platform::create_remote_ssh_config_dir(SSH_CONTROL_SOCKET_NAME)?;
    let path = dir.join("config");
    let control_path = paths
        .multiplexing
        .then(|| dir.join(SSH_CONTROL_SOCKET_NAME));

    let mut contents = String::new();
    if let Some(user_config) = paths.user_config.filter(|path| path.is_file()) {
        contents.push_str(&format!(
            "Include {}\n",
            ssh_config_quote(&user_config.to_string_lossy())
        ));
    }
    if let Some(system_config) = paths.system_config.filter(|path| path.is_file()) {
        contents.push_str(&format!(
            "Include {}\n",
            ssh_config_quote(&system_config.to_string_lossy())
        ));
    }
    contents.push_str("Host *\n");
    contents.push_str("  ServerAliveInterval 15\n");
    contents.push_str("  ServerAliveCountMax 4\n");

    let write_result = (|| {
        let mut file = crate::platform::create_remote_ssh_config_file(&path)?;
        file.write_all(contents.as_bytes())
    })();
    if let Err(err) = write_result {
        let _ = fs::remove_dir_all(&dir);
        return Err(err);
    }
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
    remote_gardn: &RemoteGardn,
    session_name: &str,
    ssh_options: Option<&ManagedSshOptions>,
) -> io::Result<()> {
    let mut command = Command::new("ssh");
    apply_managed_ssh_options(&mut command, ssh_options);
    command
        .arg("-T")
        .arg(target)
        .arg(remote_bridge_command(remote_gardn, session_name));
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
        .env("GARDN_RENDER_ENCODING", "terminal-ansi")
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
    let readable_name = format!("gardn-remote-{pid}-{target_clean}-{session_clean}.sock");
    let target_prefix: String = target_clean.chars().take(8).collect();
    let hash = short_socket_hash(target, session_name);
    let short_name = format!("gardn-r-{pid}-{target_prefix}-{hash}.sock");
    crate::platform::remote_bridge_endpoint_path(&readable_name, &short_name)
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
    if *platform == RemotePlatform::local() || !crate::build_info::is_official_release() {
        let source = resolve_install_source(platform, None)?;
        let checksum = crate::checksum::file_sha256(&source.path)?;
        return Ok((install_source_description(platform, None), checksum));
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
    let (source, checksum) = worker_install_source_metadata(&platform)?;
    let remote_gardn = execution_worker_remote_gardn(platform.clone(), &checksum)?;
    let already_current = remote_worker_binary_matches(ssh, &remote_gardn)?;
    let target_exists = remote_binary_exists(ssh, &remote_gardn)?;
    let has_previous = !remote_binary_candidates(ssh, &remote_gardn)?.is_empty();
    Ok(WorkerInstallPreview {
        kind: if target_exists || has_previous {
            WorkerInstallKind::Update
        } else {
            WorkerInstallKind::Install
        },
        source,
        target_path: format!("$HOME/{}", remote_gardn.install_suffix),
        checksum,
        version: CURRENT_VERSION.to_string(),
        commands: vec![
            "gardn execution-worker --build-info".to_string(),
            "gardn execution-worker --protocol-version".to_string(),
            "gardn execution-worker --daemon-lifecycle-version".to_string(),
            "gardn execution-worker --daemon <binding>".to_string(),
            "gardn execution-worker".to_string(),
        ],
        capabilities: crate::execution_host::worker::CAPABILITY_NAMES
            .iter()
            .map(|capability| (*capability).to_string())
            .collect(),
        already_current,
    })
}

pub(crate) fn inventory_execution_worker_bindings(
    target: &str,
    askpass: crate::execution_host::auth::AskpassCommandConfig,
    installation_id: &str,
    execution_host_id: &str,
) -> io::Result<crate::execution_host::runtime_paths::BindingInventoryReport> {
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    let platform = detect_remote_platform(&ssh)?;
    let (_, checksum) = worker_install_source_metadata(&platform)?;
    let remote_gardn = execution_worker_remote_gardn(platform, &checksum)?;
    if !remote_worker_binary_matches(&ssh, &remote_gardn)? {
        ensure_execution_worker_with_ssh(&ssh)?;
    }
    if !remote_worker_binary_matches(&ssh, &remote_gardn)? {
        return Err(io::Error::other(format!(
            "managed execution worker repair failed at {}",
            remote_gardn.shell_path
        )));
    }
    let command =
        execution_worker_inventory_command(&remote_gardn, installation_id, execution_host_id);
    let output = ssh.user_shell_output(&command)?;
    if !output.status.success() {
        return Err(command_failed("execution worker inventory failed", &output));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid execution worker inventory report: {error}"),
        )
    })
}

pub(crate) fn retire_execution_worker_bindings(
    target: &str,
    askpass: crate::execution_host::auth::AskpassCommandConfig,
    installation_id: &str,
    execution_host_id: &str,
) -> io::Result<crate::execution_host::runtime_paths::BindingRetirementReport> {
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    let platform = detect_remote_platform(&ssh)?;
    let (_, checksum) = worker_install_source_metadata(&platform)?;
    let remote_gardn = execution_worker_remote_gardn(platform, &checksum)?;
    if !remote_worker_binary_matches(&ssh, &remote_gardn)? {
        ensure_execution_worker_with_ssh(&ssh)?;
    }
    if !remote_worker_binary_matches(&ssh, &remote_gardn)? {
        return Err(io::Error::other(format!(
            "managed execution worker repair failed at {}",
            remote_gardn.shell_path
        )));
    }
    let command =
        execution_worker_retirement_command(&remote_gardn, installation_id, execution_host_id);
    let output = ssh.user_shell_output(&command)?;
    serde_json::from_slice(&output.stdout).map_err(|error| {
        if output.status.success() {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid execution worker retirement report: {error}"),
            )
        } else {
            command_failed("execution worker retirement failed", &output)
        }
    })
}

pub(crate) fn ensure_execution_worker(
    target: &str,
    askpass: crate::execution_host::auth::AskpassCommandConfig,
) -> io::Result<WorkerInstallReport> {
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    ensure_execution_worker_with_ssh(&ssh)
}
fn ensure_execution_worker_with_ssh(ssh: &RemoteSsh) -> io::Result<WorkerInstallReport> {
    let preview = preview_execution_worker_install_with_ssh(ssh)?;
    install_execution_worker_with_ssh(ssh, &preview)
}

fn artifact_manifest(
    checksum: &str,
    source: &str,
    identity: &crate::build_info::WorkerBuildIdentity,
) -> io::Result<String> {
    let installed_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_millis();
    serde_json::to_string(&ArtifactManifest {
        schema_version: 3,
        sha256: checksum,
        platform: &identity.platform,
        app_version: &identity.app_version,
        build_channel: &identity.build_channel,
        build_cohort: &identity.build_cohort,
        target: &identity.target,
        client_protocol: identity.client_protocol,
        worker_protocol: identity.worker_protocol,
        daemon_lifecycle_version: identity.daemon_lifecycle_version,
        capabilities: &identity.capabilities,
        source,
        installed_unix_ms,
    })
    .map_err(io::Error::other)
}

fn parse_worker_artifact_inventory(bytes: &[u8]) -> io::Result<Vec<WorkerArtifactInventoryEntry>> {
    let fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    if fields.len() % 2 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "execution worker artifact inventory was truncated",
        ));
    }
    fields
        .as_chunks::<2>()
        .0
        .iter()
        .map(|fields| {
            let path = std::str::from_utf8(fields[0])
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                .to_string();
            let last_used_unix_seconds = std::str::from_utf8(fields[1])
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                .parse::<u64>()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            Ok(WorkerArtifactInventoryEntry {
                path,
                last_used_unix_seconds,
            })
        })
        .collect()
}

fn worker_artifacts_to_prune(
    mut entries: Vec<WorkerArtifactInventoryEntry>,
    current_checksum: &str,
    now_unix_seconds: u64,
) -> Vec<WorkerArtifactInventoryEntry> {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.last_used_unix_seconds));
    let cutoff = now_unix_seconds.saturating_sub(WORKER_ARTIFACT_LEASE_GRACE_SECONDS);
    let mut retained_recent = 0;
    entries
        .into_iter()
        .filter(|entry| {
            let path = Path::new(&entry.path);
            let checksum = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str());
            if checksum == Some(current_checksum) {
                return false;
            }
            if retained_recent < WORKER_ARTIFACT_RETAIN_RECENT {
                retained_recent += 1;
                return false;
            }
            entry.last_used_unix_seconds <= cutoff
                && path.file_name().and_then(|name| name.to_str()) == Some("gardn")
                && checksum.is_some_and(|checksum| {
                    checksum.len() == 64 && checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
        })
        .collect()
}

fn prune_execution_worker_artifacts(ssh: &RemoteSsh, current: &RemoteGardn) -> io::Result<()> {
    let artifact_root = Path::new(&current.install_suffix)
        .ancestors()
        .nth(4)
        .ok_or_else(|| io::Error::other("execution worker artifact path has no v2 root"))?;
    let inventory_command = format!(
        r#"set -eu
dir="$HOME/{}"
for artifact in "$dir"/*/p*-l*/*/gardn; do
  [ -f "$artifact" ] || continue
  [ -f "${{artifact}}.sha256" ] || continue
  [ -f "${{artifact}}.manifest.json" ] || continue
  lease="${{artifact}}.last-used"
  [ -f "$lease" ] || lease="${{artifact}}.manifest.json"
  last=$(stat -c %Y "$lease" 2>/dev/null || stat -f %m "$lease" 2>/dev/null || printf 0)
  printf '%s\0%s\0' "$artifact" "$last"
done
"#,
        artifact_root.display()
    );
    let output = ssh.sh_output(&inventory_command)?;
    if !output.status.success() {
        return Err(command_failed(
            "execution worker artifact inventory failed",
            &output,
        ));
    }
    let current_checksum = Path::new(&current.install_suffix)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("execution worker artifact checksum is invalid"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs();
    let candidates = worker_artifacts_to_prune(
        parse_worker_artifact_inventory(&output.stdout)?,
        current_checksum,
        now,
    );
    if candidates.is_empty() {
        return Ok(());
    }
    let cutoff = now.saturating_sub(WORKER_ARTIFACT_LEASE_GRACE_SECONDS);
    let mut command = format!("set -eu\ncutoff={cutoff}\n");
    for candidate in candidates {
        let artifact = shell_quote(&candidate.path);
        command.push_str(&format!(
            "artifact={artifact}\nlease=\"${{artifact}}.last-used\"\n\
             [ -f \"$lease\" ] || lease=\"${{artifact}}.manifest.json\"\n\
             last=$(stat -c %Y \"$lease\" 2>/dev/null || stat -f %m \"$lease\" 2>/dev/null || printf 0)\n\
             if [ \"$last\" -le \"$cutoff\" ]; then rm -f -- \"$artifact\" \"${{artifact}}.sha256\" \"${{artifact}}.manifest.json\" \"${{artifact}}.last-used\" && rmdir \"$(dirname \"$artifact\")\" 2>/dev/null || true; fi\n"
        ));
    }
    let output = ssh.sh_output(&command)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(
            "execution worker artifact cleanup failed",
            &output,
        ))
    }
}

fn install_execution_worker_with_ssh(
    ssh: &RemoteSsh,
    approved: &WorkerInstallPreview,
) -> io::Result<WorkerInstallReport> {
    let platform = detect_remote_platform(ssh)?;
    let remote_gardn = execution_worker_remote_gardn(platform.clone(), &approved.checksum)?;
    let current = preview_execution_worker_install_with_ssh(ssh)?;
    if &current != approved {
        return Err(io::Error::other(
            "execution worker install plan changed; review and approve the new plan",
        ));
    }
    if current.already_current {
        if let Err(error) = prune_execution_worker_artifacts(ssh, &remote_gardn) {
            tracing::debug!(%error, "could not prune stale execution worker artifacts");
        }
        return Ok(WorkerInstallReport::AlreadyCurrent(current));
    }
    let source = resolve_install_source(&platform, remote_binary_override_path()?)?;
    let verified = crate::checksum::verify_sha256(&source.path, &current.checksum);
    if let Err(error) = verified {
        source.cleanup();
        return Err(io::Error::other(format!(
            "execution worker source checksum verification failed: {error}"
        )));
    }
    // Immutable addressed artifact only. Never touch an incumbent daemon's
    // socket, lock, or process; activation is a separate lifecycle step.
    let install_result = ssh.install_gardn(
        &remote_gardn,
        &source.path,
        &current.checksum,
        &current.source,
    );
    source.cleanup();
    install_result?;
    if !remote_worker_binary_matches(ssh, &remote_gardn)? {
        return Err(io::Error::other(
            "staged execution worker failed version/protocol/lifecycle verification",
        ));
    }
    if let Err(error) = prune_execution_worker_artifacts(ssh, &remote_gardn) {
        tracing::debug!(%error, "could not prune stale execution worker artifacts");
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
    worker: &WorkerInstallPreview,
    cancel: Option<&ConnectCancel>,
) -> Result<ExecutionWorkerTransport, ExecutionWorkerTransportError> {
    if let Some(cancel) = cancel {
        cancel.check()?;
    }
    let ssh = RemoteSsh::with_askpass(target.to_string(), true, askpass);
    let platform = detect_remote_platform_cancellable(&ssh, cancel)?;
    let remote_gardn = execution_worker_remote_gardn(platform, &worker.checksum)?;

    if !remote_worker_binary_matches_cancellable(&ssh, &remote_gardn, cancel)? {
        return Err(ExecutionWorkerTransportError::BootstrapRequired {
            target: target.to_string(),
            expected_version: CURRENT_VERSION.to_string(),
            versioned_remote_path: remote_gardn.install_suffix.clone(),
        });
    }

    if let Some(cancel) = cancel {
        cancel.check()?;
    }

    let mut child = ssh
        .dedicated_command()
        .arg(execution_worker_command(&remote_gardn))
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

fn execution_worker_remote_gardn(
    platform: RemotePlatform,
    checksum: &str,
) -> io::Result<RemoteGardn> {
    if checksum.len() != 64
        || !checksum
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "execution worker checksum must be 64 hexadecimal characters",
        ));
    }
    let install_suffix = format!(
        ".local/share/gardn/execution-workers/v2/{}/p{}-l{}/{}/gardn",
        platform.asset_key(),
        crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION,
        crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION,
        checksum.to_ascii_lowercase(),
    );
    Ok(RemoteGardn {
        shell_path: format!("\"$HOME/{install_suffix}\""),
        install_suffix,
        platform,
    })
}

fn execution_worker_command(remote_gardn: &RemoteGardn) -> String {
    format!("{} execution-worker", remote_gardn.shell_path)
}

fn remote_worker_binary_matches(ssh: &RemoteSsh, remote_gardn: &RemoteGardn) -> io::Result<bool> {
    remote_worker_binary_matches_cancellable(ssh, remote_gardn, None)
}

fn remote_worker_binary_matches_cancellable(
    ssh: &RemoteSsh,
    remote_gardn: &RemoteGardn,
    cancel: Option<&ConnectCancel>,
) -> io::Result<bool> {
    let output =
        ssh.sh_output_cancellable(&execution_worker_probe_command(remote_gardn), cancel)?;
    let Ok(identity) = parse_worker_build_identity(&output) else {
        return Ok(false);
    };
    Ok(validate_worker_build_identity(&identity, &remote_gardn.platform).is_ok())
}

fn execution_worker_probe_command(remote_gardn: &RemoteGardn) -> String {
    worker_build_info_command(&remote_gardn.shell_path)
}

fn execution_worker_inventory_command(
    remote_gardn: &RemoteGardn,
    installation_id: &str,
    execution_host_id: &str,
) -> String {
    format!(
        "{} execution-worker --inventory --installation {} --execution-host {}",
        remote_gardn.shell_path,
        shell_quote(installation_id),
        shell_quote(execution_host_id),
    )
}

fn execution_worker_retirement_command(
    remote_gardn: &RemoteGardn,
    installation_id: &str,
    execution_host_id: &str,
) -> String {
    format!(
        "{} execution-worker --retire --installation {} --execution-host {}",
        remote_gardn.shell_path,
        shell_quote(installation_id),
        shell_quote(execution_host_id),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_WORKER_CHECKSUM: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn test_worker_build_info(platform: &RemotePlatform) -> String {
        let mut identity = crate::build_info::worker_identity();
        identity.platform = platform.asset_key();
        identity.target = match (platform.os, platform.arch) {
            ("linux", "x86_64") => "x86_64-unknown-linux-musl",
            ("linux", "aarch64") => "aarch64-unknown-linux-musl",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            ("windows", "x86_64") => "x86_64-pc-windows-msvc",
            _ => "unknown",
        }
        .to_string();
        serde_json::to_string(&identity).expect("test worker identity should serialize")
    }

    fn test_execution_worker_remote_gardn(platform: RemotePlatform) -> RemoteGardn {
        execution_worker_remote_gardn(platform, TEST_WORKER_CHECKSUM)
            .expect("test checksum should be valid")
    }
    #[test]
    fn connect_cancel_interrupts_wait_loop() {
        let cancel = ConnectCancel::new();
        cancel.cancel();
        let err = cancel.check().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(err.to_string().contains("cancelled"));
    }

    #[test]
    fn worker_retirement_command_is_exact_and_shell_quotes_identity() {
        let remote_gardn = test_execution_worker_remote_gardn(RemotePlatform {
            os: "linux",
            arch: "aarch64",
        });
        assert_eq!(
            execution_worker_retirement_command(
                &remote_gardn,
                "installation 'air'",
                "ssh:robotbox:1",
            ),
            format!(
                "\"$HOME/.local/share/gardn/execution-workers/v2/linux-aarch64/p{}-l{}/{TEST_WORKER_CHECKSUM}/gardn\" execution-worker --retire --installation 'installation '\\''air'\\''' --execution-host ssh:robotbox:1",
                crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION,
                crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION,
            )
        );
    }

    #[test]
    fn execution_worker_uses_a_role_isolated_versioned_artifact() {
        let remote_gardn = test_execution_worker_remote_gardn(RemotePlatform {
            os: "linux",
            arch: "aarch64",
        });

        assert_eq!(
            remote_gardn.install_suffix,
            format!(
                ".local/share/gardn/execution-workers/v2/linux-aarch64/p{}-l{}/{TEST_WORKER_CHECKSUM}/gardn",
                crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION,
                crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION,
            )
        );
        assert_eq!(
            execution_worker_command(&remote_gardn),
            format!(
                "\"$HOME/.local/share/gardn/execution-workers/v2/linux-aarch64/p{}-l{}/{TEST_WORKER_CHECKSUM}/gardn\" execution-worker",
                crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION,
                crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION,
            )
        );
        let probe = execution_worker_probe_command(&remote_gardn);
        assert!(probe.contains("execution-worker --build-info"));
        assert!(!probe.contains("daemon-lifecycle-version"));
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
            "gardn-bridge-permissions-test-{}.sock",
            std::process::id()
        ));
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let bridge = SshStdioBridge::start(
            "example".to_string(),
            remote_gardn,
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
    fn accepted_bridge_stream_is_reset_to_blocking() {
        use std::os::fd::AsRawFd as _;

        fn is_nonblocking(stream: &UnixStream) -> bool {
            let fd = stream.as_raw_fd();
            // SAFETY: F_GETFL only reads flags from the live descriptor owned by `stream`.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(flags >= 0, "fcntl(F_GETFL): {}", io::Error::last_os_error());
            flags & libc::O_NONBLOCK != 0
        }

        let socket = std::env::temp_dir().join(format!(
            "gardn-bridge-blocking-test-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket).expect("bind listener");
        let client = UnixStream::connect(&socket).expect("connect client");
        let (server, _addr) = listener.accept().expect("accept client");

        server
            .set_nonblocking(true)
            .expect("force the macOS accepted-stream state");
        assert!(is_nonblocking(&server));
        let server = prepare_remote_bridge_stream(server).expect("prepare bridge stream");
        assert!(!is_nonblocking(&server));

        drop(server);
        drop(client);
        drop(listener);
        let _ = std::fs::remove_file(socket);
    }

    #[test]
    fn managed_ssh_config_includes_user_config_then_fallback() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let home =
            std::env::temp_dir().join(format!("gardn-keepalive-home-test-{}", std::process::id()));
        let ssh_dir = home.join(".ssh");
        let user_config = ssh_dir.join("config");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&ssh_dir).unwrap();
        std::fs::write(&user_config, "Host example\n  User gardn\n").unwrap();
        let _home = crate::config::TestEnvVar::set("HOME", home.as_os_str());

        let managed_config = write_managed_ssh_config().expect("write managed config");
        let path = managed_config.options.config_path.clone();
        let control_path = managed_config
            .options
            .control_path
            .clone()
            .expect("unix multiplexing should create a control socket");
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
            "config should end with Gardn's keepalive fallback block: {contents}"
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
        let control_path = managed_config
            .options
            .control_path
            .clone()
            .expect("unix multiplexing should create a control socket");
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
    fn execution_worker_command_uses_dedicated_ssh_transport() {
        let managed_config = write_managed_ssh_config().expect("write managed config");
        let config_path = managed_config.options.config_path.clone();
        let ssh = RemoteSsh {
            target: "example".to_string(),
            managed_config: Some(managed_config),
            askpass: None,
        };

        let command = ssh.dedicated_command();
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "-F".to_string(),
                config_path.to_string_lossy().into_owned(),
                "-o".to_string(),
                "ControlMaster=no".to_string(),
                "-o".to_string(),
                "ControlPath=none".to_string(),
                "-o".to_string(),
                "ControlPersist=no".to_string(),
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
            "gardn".into(),
            "--remote".into(),
            "dev".into(),
            "--help".into(),
        ];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["gardn", "--help"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "dev");
        assert_eq!(remote.keybindings, RemoteKeybindings::Local);
    }

    #[test]
    fn extract_remote_args_removes_equals_form() {
        let args = vec!["gardn".into(), "--remote=user@host".into()];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["gardn"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "user@host");
        assert_eq!(remote.keybindings, RemoteKeybindings::Local);
    }

    #[test]
    fn extract_remote_args_accepts_remote_keybindings_server() {
        let args = vec![
            "gardn".into(),
            "--remote".into(),
            "dev".into(),
            "--remote-keybindings=server".into(),
        ];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["gardn"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "dev");
        assert_eq!(remote.keybindings, RemoteKeybindings::Server);
    }

    #[test]
    fn extract_remote_args_accepts_remote_keybindings_space_form() {
        let args = vec![
            "gardn".into(),
            "--remote=dev".into(),
            "--remote-keybindings".into(),
            "server".into(),
        ];
        let (cleaned, remote) = extract_remote_args(&args).unwrap();
        assert_eq!(cleaned, vec!["gardn"]);
        assert_eq!(remote.unwrap().keybindings, RemoteKeybindings::Server);
    }

    #[test]
    fn extract_remote_args_accepts_explicit_handoff() {
        let args = vec!["gardn".into(), "--remote=dev".into(), "--handoff".into()];

        let (cleaned, remote) = extract_remote_args(&args).unwrap();

        assert_eq!(cleaned, vec!["gardn"]);
        let remote = remote.unwrap();
        assert_eq!(remote.target, "dev");
        assert!(remote.live_handoff);
    }

    #[test]
    fn extract_remote_args_preserves_child_remote_options_after_separator() {
        let args = vec![
            "gardn".into(),
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
        let args = vec!["gardn".into(), "update".into(), "--handoff".into()];

        let (cleaned, remote) = extract_remote_args(&args).unwrap();

        assert_eq!(cleaned, args);
        assert!(remote.is_none());
    }

    #[test]
    fn extract_remote_args_rejects_remote_keybindings_without_remote() {
        let args = vec!["gardn".into(), "--remote-keybindings=server".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote-keybindings requires --remote");
    }

    #[test]
    fn extract_remote_args_rejects_duplicate_remote_keybindings() {
        let args = vec![
            "gardn".into(),
            "--remote=dev".into(),
            "--remote-keybindings=local".into(),
            "--remote-keybindings=server".into(),
        ];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote-keybindings can only be specified once");
    }

    #[test]
    fn extract_remote_args_requires_value() {
        let args = vec!["gardn".into(), "--remote".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "missing value for --remote");
    }

    #[test]
    fn extract_remote_args_rejects_empty_value() {
        let args = vec!["gardn".into(), "--remote=".into()];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "missing value for --remote");
    }

    #[test]
    fn extract_remote_args_rejects_duplicate_values() {
        let args = vec![
            "gardn".into(),
            "--remote=dev".into(),
            "--remote=prod".into(),
        ];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote can only be specified once");
    }

    #[test]
    fn extract_remote_args_rejects_option_like_target() {
        let args = vec!["gardn".into(), "--remote".into(), "-oProxyCommand=x".into()];
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
                "target/release/gardn",
                "user@host",
                "work",
                RemoteKeybindings::Local,
                false,
            ),
            "target/release/gardn --remote user@host --session work"
        );
        assert_eq!(
            reattach_command(
                "gardn",
                "host name",
                crate::session::DEFAULT_SESSION_NAME,
                RemoteKeybindings::Local,
                false,
            ),
            "gardn --remote 'host name'"
        );
        assert_eq!(
            reattach_command(
                "gardn",
                "host",
                crate::session::DEFAULT_SESSION_NAME,
                RemoteKeybindings::Server,
                false,
            ),
            "gardn --remote host --remote-keybindings server"
        );
        assert_eq!(
            reattach_command(
                "gardn",
                "host",
                crate::session::DEFAULT_SESSION_NAME,
                RemoteKeybindings::Local,
                true,
            ),
            "gardn --remote host --handoff"
        );
    }

    #[test]
    fn remote_bridge_command_uses_installed_binary() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        assert_eq!(
            remote_bridge_command(&remote_gardn, crate::session::DEFAULT_SESSION_NAME),
            "exec \"$HOME/.local/bin/gardn\" remote-client-bridge"
        );
    }

    #[test]
    fn remote_lifecycle_commands_omit_session_for_default() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let session = crate::session::DEFAULT_SESSION_NAME;

        assert_eq!(
            remote_server_status_command(&remote_gardn, session),
            "\"$HOME/.local/bin/gardn\" status server --json"
        );
        assert_eq!(
            remote_server_stop_command(&remote_gardn, session),
            "\"$HOME/.local/bin/gardn\" server stop"
        );
        assert_eq!(
            remote_server_live_handoff_command(&remote_gardn, session),
            format!(
                "\"$HOME/.local/bin/gardn\" server live-handoff --import-exe \"$HOME/.local/bin/gardn\" --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}"
            )
        );
        assert_eq!(
            remote_bridge_command(&remote_gardn, session),
            "exec \"$HOME/.local/bin/gardn\" remote-client-bridge"
        );
    }

    #[test]
    fn remote_lifecycle_commands_qualify_named_session() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let session = "work";

        assert_eq!(
            remote_server_status_command(&remote_gardn, session),
            "\"$HOME/.local/bin/gardn\" --session work status server --json"
        );
        assert_eq!(
            remote_server_stop_command(&remote_gardn, session),
            "\"$HOME/.local/bin/gardn\" --session work server stop"
        );
        assert_eq!(
            remote_server_live_handoff_command(&remote_gardn, session),
            format!(
                "\"$HOME/.local/bin/gardn\" --session work server live-handoff --import-exe \"$HOME/.local/bin/gardn\" --expected-protocol {CURRENT_PROTOCOL} --expected-version {CURRENT_VERSION}"
            )
        );
        assert_eq!(
            remote_bridge_command(&remote_gardn, session),
            "exec \"$HOME/.local/bin/gardn\" --session work remote-client-bridge"
        );
    }

    #[test]
    fn remote_lifecycle_commands_quote_session_names() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_gardn = remote_gardn_from_path_discovery(&remote_gardn, "/usr/bin/gardn\n")
            .expect("path binary");
        let session = "my session";

        assert_eq!(
            remote_server_status_command(&remote_gardn, session),
            "/usr/bin/gardn --session 'my session' status server --json"
        );
        assert_eq!(
            remote_server_stop_command(&remote_gardn, session),
            "/usr/bin/gardn --session 'my session' server stop"
        );
        assert_eq!(
            remote_bridge_command(&remote_gardn, session),
            "exec /usr/bin/gardn --session 'my session' remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_uses_path_binary() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_gardn = remote_gardn_from_path_discovery(&remote_gardn, "/usr/bin/gardn\n")
            .expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_gardn, crate::session::DEFAULT_SESSION_NAME),
            "exec /usr/bin/gardn remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_quotes_discovered_binary() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_gardn =
            remote_gardn_from_path_discovery(&remote_gardn, "/opt/gardn bin/gardn\n")
                .expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_gardn, crate::session::DEFAULT_SESSION_NAME),
            "exec '/opt/gardn bin/gardn' remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_uses_macos_path_binary() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "macos",
            arch: "aarch64",
        });
        let remote_gardn =
            remote_gardn_from_path_discovery(&remote_gardn, "/opt/homebrew/bin/gardn\n")
                .expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_gardn, crate::session::DEFAULT_SESSION_NAME),
            "exec /opt/homebrew/bin/gardn remote-client-bridge"
        );
        assert_eq!(remote_gardn.platform.asset_key(), "macos-aarch64");
    }

    #[test]
    fn remote_path_discovery_quotes_single_quotes_in_discovered_binary() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_gardn =
            remote_gardn_from_path_discovery(&remote_gardn, "/opt/gardn's/bin/gardn\n")
                .expect("path binary");

        assert_eq!(
            remote_bridge_command(&remote_gardn, crate::session::DEFAULT_SESSION_NAME),
            "exec '/opt/gardn'\\''s/bin/gardn' remote-client-bridge"
        );
    }

    #[test]
    fn remote_path_discovery_ignores_relative_paths() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_gardn = remote_gardn_from_path_discovery(&remote_gardn, "bin/gardn\n");

        assert!(remote_gardn.is_none());
    }

    #[test]
    fn remote_path_discovery_ignores_empty_output() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let remote_gardn = remote_gardn_from_path_discovery(&remote_gardn, "\n");

        assert!(remote_gardn.is_none());
    }

    struct FakeSshResponse<'a> {
        status: i32,
        stdout: &'a str,
        stderr: &'a str,
    }

    fn fake_remote_path_probe<'a>(
        primary: FakeSshResponse<'a>,
        fallback: FakeSshResponse<'a>,
    ) -> (io::Result<Option<RemoteGardn>>, String) {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "gardn-remote-path-discovery-{}-{unique}",
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
if [ "$last" = "command -v gardn" ]; then
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
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let result = remote_binary_on_path_any(&ssh, &remote_gardn);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    fn fake_remote_binary_candidates_probe<'a>(
        primary: FakeSshResponse<'a>,
        fallback: FakeSshResponse<'a>,
        mise: FakeSshResponse<'a>,
    ) -> (io::Result<Vec<RemoteGardn>>, String) {
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
            "gardn-remote-mise-discovery-{}-{unique}",
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
if [ "$last" = "command -v gardn" ]; then
    printf '%s\n' primary >> {log}
    printf '%s' {primary_stdout}
    printf '%s' {primary_stderr} >&2
    exit {primary_status}
fi
if [ "$last" = "/bin/sh -s" ]; then
    request=$(cat)
    case "$request" in
        *mise/installs/gardn*)
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
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let result = remote_binary_candidates(&ssh, &remote_gardn);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    #[test]
    fn remote_binary_candidates_discovers_mise_install() {
        let mise_paths = format!(
            "/home/can/.local/share/mise/installs/gardn/{CURRENT_VERSION}/bin/gardn\n\
             /home/can/.local/share/mise/installs/gardn/{CURRENT_VERSION}/gardn\n"
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
                stderr: "sh: gardn: not found\n",
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
            format!("/home/can/.local/share/mise/installs/gardn/{CURRENT_VERSION}/bin/gardn")
        );
        assert_eq!(
            candidates[1].shell_path,
            format!("/home/can/.local/share/mise/installs/gardn/{CURRENT_VERSION}/gardn")
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
                stdout: "not-an-absolute-path\n./gardn\n/home/can/.local/share/mise/shims/gardn\n",
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
                stdout: "/fallback/bin/gardn\n",
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
        assert_eq!(candidates[0].shell_path, "/fallback/bin/gardn");
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

        assert!(
            script.contains("emit \"$home/.local/share/mise/installs/gardn/$version/bin/gardn\"")
        );
        assert!(script.contains("emit \"$home/.local/share/mise/installs/gardn/$version/gardn\""));
        assert!(script.contains(&format!("version={}", shell_quote(CURRENT_VERSION))));
        assert!(!script.contains("mise/shims/gardn"));
    }

    #[test]
    fn remote_path_discovery_prefers_primary_user_shell_probe() {
        let (result, invocations) = fake_remote_path_probe(
            FakeSshResponse {
                status: 0,
                stdout: "/primary/bin/gardn\n",
                stderr: "",
            },
            FakeSshResponse {
                status: 0,
                stdout: "/fallback/bin/gardn\n",
                stderr: "",
            },
        );
        let remote_gardn = result
            .expect("primary discovery should succeed")
            .expect("primary path should be returned");

        assert_eq!(remote_gardn.shell_path, "/primary/bin/gardn");
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
                stdout: "/fallback/bin/gardn\n",
                stderr: "",
            },
        );
        let remote_gardn = result
            .expect("fallback discovery should succeed")
            .expect("fallback path should be returned");

        assert_eq!(remote_gardn.shell_path, "/fallback/bin/gardn");
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
                stderr: "sh: gardn: not found\n",
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
            "/home/can/.local/bin/gardn\n"
        ));
        assert!(remote_shell_resolves_managed_install(
            "/Users/can/.local/bin/gardn\n"
        ));
        assert!(!remote_shell_resolves_managed_install(
            "/usr/local/bin/gardn\n"
        ));
        assert!(!remote_shell_resolves_managed_install(""));
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
            install_source_description(&platform, Some(Path::new("/tmp/gardn-aarch64"))),
            "GARDN_REMOTE_BINARY (/tmp/gardn-aarch64)"
        );
    }

    #[test]
    fn worker_install_override_preview_has_verified_checksum() {
        let _environment = crate::integration::integration_env_lock();
        let path = std::env::temp_dir().join(format!(
            "gardn-worker-install-source-{}",
            std::process::id()
        ));
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
        let source = resolve_install_source(&platform, Some(PathBuf::from("/tmp/gardn-aarch64")))
            .expect("override source");
        assert_eq!(source.path, PathBuf::from("/tmp/gardn-aarch64"));
        assert!(source.temporary_dir.is_none());
    }

    #[test]
    fn missing_non_local_development_worker_points_to_atomic_install_command() {
        let _environment = crate::integration::integration_env_lock();
        let worker_root = std::env::temp_dir().join(format!(
            "gardn-missing-worker-sidecar-{}",
            std::process::id()
        ));
        let _worker_dir = crate::config::TestEnvVar::set(DEV_WORKER_DATA_DIR_ENV_VAR, &worker_root);
        let platform = RemotePlatform {
            os: "linux",
            arch: if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
                "aarch64"
            } else {
                "x86_64"
            },
        };

        let error = match resolve_install_source(&platform, None) {
            Ok(_) => panic!("worker should be absent"),
            Err(error) => error,
        };

        assert_eq!(
            error.to_string(),
            format!(
                "matching {} development worker is not installed; run `just install-dev`, then retry",
                platform.asset_key()
            )
        );
    }

    #[test]
    fn remote_install_stream_command_avoids_shell_c_wrapper() {
        let command = remote_install_stream_command("/home/a b/.local/bin/gardn.tmp.123");

        assert_eq!(command, "tee '/home/a b/.local/bin/gardn.tmp.123'");
    }

    #[test]
    fn remote_install_prepare_and_commit_scripts_quote_paths() {
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let prepare = remote_install_prepare_script(&remote_gardn);

        assert!(prepare.contains("mkdir -p \"$dir\""));
        assert!(prepare.contains("printf '%s\\0%s\\0' \"$tmp\" \"$dest\""));
        assert_eq!(
            parse_remote_install_paths(b"/home/a b/gardn.tmp.42\0/home/a b/gardn\0").unwrap(),
            (
                "/home/a b/gardn.tmp.42".to_string(),
                "/home/a b/gardn".to_string()
            )
        );
        assert_eq!(
            parse_remote_install_paths(b"/home/a b\n/gardn.tmp.42\0/home/a b\n/gardn\0").unwrap(),
            (
                "/home/a b\n/gardn.tmp.42".to_string(),
                "/home/a b\n/gardn".to_string()
            )
        );
        let commit = remote_install_commit_script(
            "/home/a b/gardn.tmp.42",
            "/home/a b/gardn",
            "abc123",
            r#"{"schema_version":2}"#,
        );
        assert!(commit.contains("tmp='/home/a b/gardn.tmp.42'"));
        assert!(commit.contains("dest='/home/a b/gardn'"));
        assert!(commit.contains("expected=abc123"));
        assert!(commit.contains("mkdir \"$lock\""));
        assert!(commit.contains("refusing to replace unowned"));
    }

    fn remote_env_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn socket_path_byte_len(path: &Path) -> usize {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().len()
    }

    fn fits_unix_socket_path(path: &Path) -> bool {
        socket_path_byte_len(path) <= 103
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
            filename.starts_with("gardn-remote-"),
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
        let tmpdir = PathBuf::from("/tmp").join(format!("gardn-{}", "a".repeat(39)));
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
            filename.starts_with("gardn-r-"),
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
        let long_dir = PathBuf::from("/tmp").join(format!("gardn-{}", "a".repeat(80)));
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
            filename.starts_with("gardn-r-"),
            "expected hashed fallback, got {filename}"
        );
    }

    #[test]
    fn install_source_cleanup_removes_temporary_directory() {
        let dir = std::env::temp_dir().join(format!(
            "gardn-install-source-cleanup-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).expect("create temp dir");
        let path = dir.join("gardn.tmp");
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
        let dir = std::env::temp_dir().join(format!(
            "gardn-worker-probe-{}-{unique}",
            std::process::id()
        ));
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
        let remote_gardn = test_execution_worker_remote_gardn(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let result = remote_worker_binary_matches(&ssh, &remote_gardn);
        let invocations = fs::read_to_string(&log).expect("fake ssh should record invocations");
        drop(_path);
        let _ = fs::remove_dir_all(dir);
        (result, invocations)
    }

    #[test]
    fn remote_worker_probe_requires_matching_build_identity() {
        let platform = RemotePlatform {
            os: "linux",
            arch: "x86_64",
        };
        let good = test_worker_build_info(&platform);
        let (result, invocations) = fake_execution_worker_probe(0, &good, "");
        assert!(result.expect("probe should succeed"));
        assert!(invocations.contains("execution-worker --build-info"));
        assert!(!invocations.contains("worker.sock"));
        assert!(!invocations.contains("worker.lock"));
        assert!(!invocations.contains("flock"));
        assert!(!invocations.contains("kill"));

        let (result, _) = fake_execution_worker_probe(0, "{}", "");
        assert!(!result.expect("incomplete identity should be stale"));

        let mut wrong = crate::build_info::worker_identity();
        wrong.platform = platform.asset_key();
        wrong.target = "x86_64-unknown-linux-musl".to_string();
        wrong.build_cohort = "different-source-state".to_string();
        let wrong = serde_json::to_string(&wrong).expect("wrong identity should serialize");
        let (result, _) = fake_execution_worker_probe(0, &wrong, "");
        assert!(!result.expect("mismatched identity should be stale"));
    }

    #[test]
    fn execution_worker_install_scripts_are_side_by_side_only() {
        let remote_gardn = test_execution_worker_remote_gardn(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let prepare = remote_install_prepare_script(&remote_gardn);
        let commit = remote_install_commit_script(
            "/tmp/gardn.tmp",
            &format!("$HOME/{}", remote_gardn.install_suffix),
            "abc123",
            r#"{"schema_version":2}"#,
        );
        let probe = execution_worker_probe_command(&remote_gardn);

        assert!(
            prepare.contains(&remote_gardn.install_suffix),
            "prepare must target versioned artifact path"
        );
        assert!(prepare.contains("mkdir -p"));
        assert!(!prepare.contains("worker.sock"));
        assert!(!prepare.contains("worker.lock"));
        assert!(!prepare.contains("kill"));
        assert!(!prepare.contains("flock"));
        assert!(!prepare.contains("execution-worker --daemon"));

        assert!(commit.contains("mv "));
        assert!(commit.contains("sha256sum"));
        assert!(commit.contains("install-lock"));
        assert!(commit.contains("sha256"));
        assert!(!commit.contains("worker.sock"));
        assert!(!commit.contains("worker.lock"));
        assert!(!commit.contains("kill"));
        assert!(!commit.contains("flock"));
        assert!(!commit.contains("execution-worker --daemon"));

        assert!(probe.contains("--build-info"));
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
            "gardn-worker-preview-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("fake ssh directory should be created");
        let fake_ssh = dir.join("ssh");
        let log = dir.join("invocations");
        let local_bin = dir.join("local-gardn");
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
      *'execution-worker --build-info'*)
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
    fn execution_worker_preview_requires_matching_build_identity_and_stages_only() {
        let platform = RemotePlatform {
            os: "linux",
            arch: "x86_64",
        };
        let good = test_worker_build_info(&platform);
        let (result, invocations) = fake_execution_worker_preview_probe(&good, 0);
        let preview = result.expect("preview should succeed");
        assert!(preview.already_current);
        assert!(preview
            .commands
            .iter()
            .any(|command| command.contains("--build-info")));
        assert!(preview
            .capabilities
            .iter()
            .any(|capability| capability == "daemon_lifecycle_v2"));
        assert!(invocations.contains("execution-worker --build-info"));
        assert!(!invocations.contains("worker.sock"));
        assert!(!invocations.contains("worker.lock"));
        assert!(!invocations.contains("execution-worker --daemon "));

        let (result, _) = fake_execution_worker_preview_probe("{}", 0);
        let preview = result.expect("preview should succeed with stale worker");
        assert!(!preview.already_current);
    }

    #[test]
    fn worker_install_preview_reports_lifecycle_capability_without_activation() {
        let preview = WorkerInstallPreview {
            kind: WorkerInstallKind::Install,
            source: "test".into(),
            target_path: "$HOME/.local/share/gardn/execution-workers/x/gardn".into(),
            checksum: "abc".into(),
            version: CURRENT_VERSION.to_string(),
            commands: vec![
                "gardn execution-worker --protocol-version".into(),
                "gardn execution-worker --daemon-lifecycle-version".into(),
            ],
            capabilities: vec!["daemon_lifecycle_v2".into()],
            already_current: false,
        };
        assert!(preview
            .commands
            .iter()
            .any(|command| command.contains("--daemon-lifecycle-version")));
        assert!(preview
            .capabilities
            .iter()
            .any(|capability| capability == "daemon_lifecycle_v2"));
        assert!(!preview
            .commands
            .iter()
            .any(|command| command.contains("activate")));
    }

    #[test]
    fn worker_artifact_pruning_keeps_current_recent_and_leased_artifacts() {
        let now = WORKER_ARTIFACT_LEASE_GRACE_SECONDS + 1_000;
        let entry = |checksum_byte: char, last_used_unix_seconds: u64| {
            let checksum = checksum_byte.to_string().repeat(64);
            WorkerArtifactInventoryEntry {
                path: format!(
                    "/home/test/.local/share/gardn/execution-workers/v2/linux-x86_64/p1-l1/{checksum}/gardn"
                ),
                last_used_unix_seconds,
            }
        };
        let entries = vec![
            entry('c', 1),
            entry('a', now - 10),
            entry('b', now - 20),
            entry('d', now - 30),
            entry('e', 1),
        ];

        let pruned = worker_artifacts_to_prune(entries, &"c".repeat(64), now);

        assert_eq!(pruned, vec![entry('e', 1)]);
    }

    #[test]
    fn remote_install_commit_is_verified_repairable_and_idempotent() {
        use std::process::Command;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "gardn-install-commit-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("install test directory should exist");
        let destination = dir.join("gardn");
        let source = dir.join("first.tmp");
        fs::write(&source, b"verified worker").expect("first source should exist");
        let checksum = crate::checksum::file_sha256(&source).expect("first checksum");

        let output = Command::new("sh")
            .arg("-c")
            .arg(remote_install_commit_script(
                &source.to_string_lossy(),
                &destination.to_string_lossy(),
                &checksum,
                r#"{"schema_version":2}"#,
            ))
            .output()
            .expect("commit script should run");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(&destination).expect("installed artifact"),
            b"verified worker"
        );

        let same_source = dir.join("same.tmp");
        fs::write(&same_source, b"verified worker").expect("same source should exist");
        let output = Command::new("sh")
            .arg("-c")
            .arg(remote_install_commit_script(
                &same_source.to_string_lossy(),
                &destination.to_string_lossy(),
                &checksum,
                r#"{"schema_version":2}"#,
            ))
            .output()
            .expect("idempotent commit should run");
        assert!(output.status.success());
        assert!(!same_source.exists());

        fs::write(&destination, b"corrupt worker").expect("corrupt destination should be written");
        let repair_source = dir.join("repair.tmp");
        fs::write(&repair_source, b"verified worker").expect("repair source should exist");
        let output = Command::new("sh")
            .arg("-c")
            .arg(remote_install_commit_script(
                &repair_source.to_string_lossy(),
                &destination.to_string_lossy(),
                &checksum,
                r#"{"schema_version":2}"#,
            ))
            .output()
            .expect("repair commit should run");
        assert!(output.status.success());
        assert_eq!(
            fs::read(&destination).expect("repaired artifact"),
            b"verified worker"
        );

        let different_source = dir.join("different.tmp");
        fs::write(&different_source, b"different worker").expect("different source should exist");
        let different_checksum =
            crate::checksum::file_sha256(&different_source).expect("different checksum");
        let output = Command::new("sh")
            .arg("-c")
            .arg(remote_install_commit_script(
                &different_source.to_string_lossy(),
                &destination.to_string_lossy(),
                &different_checksum,
                r#"{"schema_version":2}"#,
            ))
            .output()
            .expect("immutable commit should run");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to replace unowned"));
        assert_eq!(
            fs::read(&destination).expect("original artifact"),
            b"verified worker"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_worker_installs_once_then_reuses_verified_artifact() {
        use std::ffi::OsString;
        use std::os::unix::fs::PermissionsExt;

        let _guard = remote_env_lock().lock().unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "gardn-worker-ensure-{}-{unique}",
            std::process::id()
        ));
        let bin_dir = dir.join("bin");
        let remote_home = dir.join("remote-home");
        fs::create_dir_all(&bin_dir).expect("fake ssh directory should exist");
        fs::create_dir_all(&remote_home).expect("fake remote home should exist");

        let fake_ssh = bin_dir.join("ssh");
        fs::write(
            &fake_ssh,
            format!(
                r#"#!/bin/sh
set -eu
last=''
for arg in "$@"; do last="$arg"; done
export HOME={}
if [ "$last" = "/bin/sh -s" ]; then
  exec /bin/sh -s
fi
exec /bin/sh -c "$last"
"#,
                shell_quote(&remote_home.to_string_lossy())
            ),
        )
        .expect("fake ssh should be written");
        let mut ssh_permissions = fs::metadata(&fake_ssh)
            .expect("fake ssh metadata")
            .permissions();
        ssh_permissions.set_mode(0o700);
        fs::set_permissions(&fake_ssh, ssh_permissions).expect("fake ssh should be executable");

        let source = dir.join("gardn-source");
        let valid_identity = shell_quote(&crate::build_info::worker_identity_json());
        let source_script = format!(
            r#"#!/bin/sh
case "${{1:-}}" in
  --version) printf '%s\n' 'gardn {CURRENT_VERSION}' ;;
  execution-worker)
    case "${{2:-}}" in
      --build-info) printf '%s\n' {valid_identity} ;;
      --protocol-version) printf '%s\n' '{}' ;;
      --daemon-lifecycle-version) printf '%s\n' '{}' ;;
      *) exit 2 ;;
    esac
    ;;
  *) exit 2 ;;
esac
"#,
            crate::execution_host::EXECUTION_WORKER_PROTOCOL_VERSION,
            crate::execution_host::lifecycle::DAEMON_LIFECYCLE_VERSION,
        );
        fs::write(&source, &source_script).expect("worker source should be written");
        let mut source_permissions = fs::metadata(&source)
            .expect("worker source metadata")
            .permissions();
        source_permissions.set_mode(0o700);
        fs::set_permissions(&source, source_permissions)
            .expect("worker source should be executable");

        let mut path = OsString::from(bin_dir.as_os_str());
        if let Some(existing) = std::env::var_os("PATH") {
            path.push(":");
            path.push(existing);
        }
        let _path = crate::config::TestEnvVar::set("PATH", path);
        let _override = crate::config::TestEnvVar::set(REMOTE_BINARY_ENV_VAR, source.as_os_str());
        let ssh = RemoteSsh::new("example".to_string(), false);

        let platform = detect_remote_platform(&ssh).expect("fake remote platform");
        let mut wrong_identity = crate::build_info::worker_identity();
        wrong_identity.build_cohort = "wrong-build-cohort".to_string();
        let wrong_identity = shell_quote(
            &serde_json::to_string(&wrong_identity).expect("wrong identity should serialize"),
        );
        fs::write(
            &source,
            source_script.replace(&valid_identity, &wrong_identity),
        )
        .expect("mismatched worker source should be written");
        let bad_checksum = crate::checksum::file_sha256(&source).expect("bad source checksum");
        let rejected_path = remote_home.join(
            execution_worker_remote_gardn(platform.clone(), &bad_checksum)
                .expect("bad source checksum should be valid")
                .install_suffix,
        );
        let error = ensure_execution_worker_with_ssh(&ssh)
            .expect_err("mismatched worker must be rejected before publication");
        assert!(error.to_string().contains("build_cohort expected"));
        assert!(!rejected_path.exists());

        fs::write(&source, &source_script).expect("valid worker source should be restored");

        let first = ensure_execution_worker_with_ssh(&ssh).expect("first ensure should install");
        assert!(matches!(first, WorkerInstallReport::Installed(_)));
        let second = ensure_execution_worker_with_ssh(&ssh).expect("second ensure should reuse");
        assert!(matches!(second, WorkerInstallReport::AlreadyCurrent(_)));

        let checksum = crate::checksum::file_sha256(&source).expect("source checksum");
        let installed = remote_home.join(
            execution_worker_remote_gardn(platform, &checksum)
                .expect("source checksum should be valid")
                .install_suffix,
        );
        assert_eq!(
            crate::checksum::file_sha256(&installed).expect("installed checksum"),
            crate::checksum::file_sha256(&source).expect("source checksum")
        );
        assert_eq!(
            fs::read_to_string(format!("{}.sha256", installed.display()))
                .expect("artifact manifest")
                .trim(),
            crate::checksum::file_sha256(&source).expect("source checksum")
        );

        drop(_override);
        drop(_path);
        let _ = fs::remove_dir_all(dir);
    }
}
