//! Windows thin-client launcher that attaches over SSH to a Unix Gardn host.

use std::fs::{self, File};
use std::io::{self, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use interprocess::local_socket::traits::Listener as _;
use interprocess::local_socket::traits::Stream as _;
use interprocess::local_socket::ListenerNonblockingMode;
use interprocess::TryClone as _;
use serde::{Deserialize, Serialize};

const BRIDGE_ACCEPT_POLL: Duration = Duration::from_millis(50);
const BRIDGE_IO_POLL: Duration = Duration::from_millis(1);
const BRIDGE_SOCKET_PERMISSION_MODE: u32 = 0o600;
const REMOTE_SERVER_SHUTDOWN_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5);
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
    Err(io::Error::other(
        "remote Windows hosts are not supported yet",
    ))
}

pub(crate) fn run_remote_api_bridge() -> io::Result<()> {
    Err(io::Error::other(
        "remote Windows hosts are not supported yet",
    ))
}

pub(crate) fn run_extra_api_connect(
    _target: &str,
    _session_name: &str,
    _json: bool,
) -> io::Result<()> {
    Err(io::Error::other(
        "remote extra coordinators are not supported on Windows yet",
    ))
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
        Self {
            os: "unknown",
            arch: if cfg!(target_arch = "x86_64") {
                "x86_64"
            } else if cfg!(target_arch = "aarch64") {
                "aarch64"
            } else {
                "unknown"
            },
        }
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

            let staged_path = posix_shell_quote(&tmp_path);
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
            let _ = self.sh_output(&format!("rm -f -- {}\n", posix_shell_quote(&tmp_path)));
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
            .arg("ControlPersist=yes");
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
        stop_after_install_approved = confirm_remote_install_with_running_server(
            ssh,
            status_probe_gardn,
            live_handoff_enabled,
            session_name,
        )?;
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
    script.push_str(&posix_shell_quote(CURRENT_VERSION));
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

    let fallback_output = ssh.sh_output("command -v gardn\n")?;
    if fallback_output.status.success() {
        let stdout = String::from_utf8_lossy(&fallback_output.stdout);
        return Ok(remote_gardn_from_path_discovery(remote_gardn, &stdout));
    }

    Ok(None)
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
    Some(
        remote_gardn
            .clone()
            .with_shell_path(posix_shell_quote(path)),
    )
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
    let release_output = crate::noninteractive_process::curl_command()
        .args([
            "-sfL",
            "--max-time",
            "30",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: gardn",
        ])
        .arg(format!("{GITHUB_RELEASE_BY_TAG_API_URL}/{release_tag}"))
        .output()
        .map_err(|err| {
            io::Error::new(err.kind(), format!("GitHub release lookup failed: {err}"))
        })?;
    if !release_output.status.success() {
        return Err(command_failed(
            "GitHub release lookup failed",
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
    format!("tee {}", posix_shell_quote(tmp_path))
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
        tmp_path = posix_shell_quote(tmp_path),
        dest_path = posix_shell_quote(dest_path),
        expected_checksum = posix_shell_quote(expected_checksum),
        artifact_manifest = posix_shell_quote(artifact_manifest),
    )
}

fn append_remote_session_flag(command: &mut String, session_name: &str) {
    if session_name != crate::session::DEFAULT_SESSION_NAME {
        command.push_str(" --session ");
        command.push_str(&posix_shell_quote(session_name));
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

fn posix_shell_quote(value: &str) -> String {
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

struct SshStdioBridge {
    local_socket: PathBuf,
    socket_identity: crate::ipc::SocketFileIdentity,
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
        crate::ipc::prepare_socket_path(&local_socket, |path| {
            format!("remote bridge is already listening at {}", path.display())
        })?;
        let listener = crate::ipc::bind_private_local_listener(&local_socket)?;
        let socket_identity = crate::ipc::socket_file_identity(&local_socket)?;
        if let Err(err) =
            crate::ipc::restrict_socket_permissions(&local_socket, BRIDGE_SOCKET_PERMISSION_MODE)
        {
            let _ = crate::ipc::remove_socket_file_if_owned(&local_socket, &socket_identity);
            return Err(err);
        }
        if let Err(err) = listener.set_nonblocking(ListenerNonblockingMode::Accept) {
            let _ = crate::ipc::remove_socket_file_if_owned(&local_socket, &socket_identity);
            return Err(err);
        }

        let should_stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&should_stop);
        let thread_ssh_options = ssh_options.cloned();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok(stream) => {
                        if let Err(err) = bridge_connection(
                            stream,
                            &target,
                            &remote_gardn,
                            &session_name,
                            thread_ssh_options.as_ref(),
                            &thread_stop,
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
            socket_identity,
            should_stop,
            thread: Some(thread),
        })
    }
}

impl Drop for SshStdioBridge {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = crate::ipc::remove_socket_file_if_owned(&self.local_socket, &self.socket_identity);
    }
}

fn ssh_config_quote(path: &str) -> String {
    format!("\"{path}\"")
}

fn ssh_config_include_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '\\' {
        ssh_config_quote(&path.replace('\\', "/"))
    } else {
        ssh_config_quote(&path)
    }
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
            ssh_config_include_path(&user_config)
        ));
    }
    if let Some(system_config) = paths.system_config.filter(|path| path.is_file()) {
        contents.push_str(&format!(
            "Include {}\n",
            ssh_config_include_path(&system_config)
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
    stream: crate::ipc::LocalStream,
    target: &str,
    remote_gardn: &RemoteGardn,
    session_name: &str,
    ssh_options: Option<&ManagedSshOptions>,
    bridge_stop: &Arc<AtomicBool>,
) -> io::Result<()> {
    let mut command = Command::new("ssh");
    apply_managed_ssh_options(&mut command, ssh_options);
    command
        .arg("-T")
        .arg(target)
        .arg(remote_bridge_command(remote_gardn, session_name))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .map_err(|err| io::Error::new(err.kind(), format!("failed to start ssh bridge: {err}")))?;
    let mut child_stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => return terminate_bridge_child(child, "ssh bridge stdin missing"),
    };
    let mut child_stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return terminate_bridge_child(child, "ssh bridge stdout missing"),
    };
    let stream_to_child = match stream.try_clone() {
        Ok(stream) => stream,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
    };
    if let Err(err) = stream.set_nonblocking(true) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }
    let mut child_to_stream = stream;

    let connection_stop = Arc::new(AtomicBool::new(false));
    let upload_stop = Arc::new(AtomicBool::new(false));
    let upload_failed = Arc::new(AtomicBool::new(false));
    let download_done = Arc::new(AtomicBool::new(false));
    let client_closed = Arc::new(AtomicBool::new(false));
    let upload_cancel = Arc::clone(&upload_stop);
    let upload_bridge_stop = Arc::clone(bridge_stop);
    let upload_failed_worker = Arc::clone(&upload_failed);
    let upload_client_closed = Arc::clone(&client_closed);
    let upload = thread::spawn(move || {
        let result = copy_local_stream_to_writer(
            stream_to_child,
            &mut child_stdin,
            &upload_cancel,
            &upload_bridge_stop,
            &upload_client_closed,
        );
        upload_failed_worker.store(result.is_err(), Ordering::Release);
        result
    });
    let download_stop = Arc::clone(&connection_stop);
    let download_bridge_stop = Arc::clone(bridge_stop);
    let download_done_worker = Arc::clone(&download_done);
    let download_upload_stop = Arc::clone(&upload_stop);
    let download = thread::spawn(move || {
        let result = copy_reader_to_local_stream(
            &mut child_stdout,
            &mut child_to_stream,
            &download_stop,
            &download_bridge_stop,
        );
        download_done_worker.store(true, Ordering::Release);
        download_upload_stop.store(true, Ordering::Release);
        result
    });

    let mut stopped_at = None;
    let (status_result, child_exited) = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                upload_stop.store(true, Ordering::Release);
                break (Ok(status), true);
            }
            Ok(None) => {}
            Err(err) => {
                connection_stop.store(true, Ordering::Release);
                upload_stop.store(true, Ordering::Release);
                let _ = child.kill();
                let _ = child.wait();
                break (Err(err), false);
            }
        }
        if bridge_stop.load(Ordering::Acquire) {
            connection_stop.store(true, Ordering::Release);
            upload_stop.store(true, Ordering::Release);
            let _ = child.kill();
            break (child.wait(), false);
        }
        if client_closed.load(Ordering::Acquire)
            || upload_failed.load(Ordering::Acquire)
            || download_done.load(Ordering::Acquire)
        {
            upload_stop.store(true, Ordering::Release);
            let stopped_at = stopped_at.get_or_insert_with(Instant::now);
            if stopped_at.elapsed() >= Duration::from_millis(250) {
                connection_stop.store(true, Ordering::Release);
                let _ = child.kill();
                break (child.wait(), false);
            }
        }
        thread::sleep(BRIDGE_ACCEPT_POLL);
    };
    upload_stop.store(true, Ordering::Release);
    if !child_exited {
        connection_stop.store(true, Ordering::Release);
    }
    let upload_result = upload
        .join()
        .map_err(|_| io::Error::other("remote bridge upload worker panicked"))?;
    let download_result = download
        .join()
        .map_err(|_| io::Error::other("remote bridge download worker panicked"))?;
    let status = status_result?;

    let stopping = bridge_stop.load(Ordering::Acquire);
    let client_closed = client_closed.load(Ordering::Acquire);
    if !stopping && !client_closed {
        upload_result.map_err(|err| {
            io::Error::new(err.kind(), format!("remote bridge upload failed: {err}"))
        })?;
        download_result.map_err(|err| {
            io::Error::new(err.kind(), format!("remote bridge download failed: {err}"))
        })?;
    }

    if status.success() || stopping || client_closed {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            format!("ssh bridge exited with {status}"),
        ))
    }
}

fn terminate_bridge_child(mut child: std::process::Child, message: &'static str) -> io::Result<()> {
    let _ = child.kill();
    let _ = child.wait();
    Err(io::Error::new(io::ErrorKind::BrokenPipe, message))
}

fn copy_reader_to_local_stream<R: io::Read>(
    reader: &mut R,
    stream: &mut crate::ipc::LocalStream,
    connection_stop: &AtomicBool,
    bridge_stop: &AtomicBool,
) -> io::Result<u64> {
    let mut buffer = [0_u8; 16 * 1024];
    let mut total = 0;

    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => return Ok(total),
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        let mut written = 0;
        while written < read {
            if connection_stop.load(Ordering::Acquire) || bridge_stop.load(Ordering::Acquire) {
                return Ok(total);
            }
            let chunk_len = (read - written).min(4 * 1024);
            match stream.write(&buffer[written..written + chunk_len]) {
                Ok(0) => thread::sleep(BRIDGE_IO_POLL),
                Ok(count) => written += count,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(BRIDGE_IO_POLL);
                }
                Err(err) => return Err(err),
            }
        }
        stream.flush()?;
        total += read as u64;
    }
}

fn copy_local_stream_to_writer<W: io::Write>(
    mut stream: crate::ipc::LocalStream,
    writer: &mut W,
    connection_stop: &AtomicBool,
    bridge_stop: &AtomicBool,
    client_closed: &AtomicBool,
) -> io::Result<u64> {
    let mut buffer = [0_u8; 16 * 1024];
    let mut total = 0;

    while !connection_stop.load(Ordering::Acquire) && !bridge_stop.load(Ordering::Acquire) {
        match crate::ipc::poll_local_stream_read_count(&mut stream, &mut buffer)? {
            crate::ipc::LocalStreamReadCount::Data(read) => {
                writer.write_all(&buffer[..read])?;
                writer.flush()?;
                total += read as u64;
            }
            crate::ipc::LocalStreamReadCount::Pending => thread::sleep(BRIDGE_IO_POLL),
            crate::ipc::LocalStreamReadCount::Closed => {
                client_closed.store(true, Ordering::Release);
                break;
            }
        }
    }

    Ok(total)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn extract_remote_args_requires_remote_for_keybindings() {
        let args = vec![
            "gardn".into(),
            "--remote-keybindings".into(),
            "server".into(),
        ];
        let err = extract_remote_args(&args).unwrap_err();
        assert_eq!(err, "--remote-keybindings requires --remote");
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
    fn windows_reattach_command_uses_current_executable() {
        let executable = std::env::current_exe().expect("current test executable");
        assert_eq!(
            reattach_command(
                r"C:\Program Files\Gardn\gardn.exe",
                "host'name",
                "work'name",
                RemoteKeybindings::Local,
                false,
            ),
            format!(
                "& '{}' --remote 'host''name' --session 'work''name'",
                executable.display().to_string().replace('\'', "''")
            )
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
    fn windows_local_forward_endpoint_uses_private_state_dir() {
        let path = local_forward_socket_path("user@example.com", "work");
        assert!(path.starts_with(crate::platform::remote_private_temp_base()));
        assert!(path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("gardn-r-")));
    }

    #[test]
    fn windows_bridge_drop_while_waiting_for_client_is_bounded() {
        let socket = local_forward_socket_path("drop-test", "default");
        let remote_gardn = RemoteGardn::for_platform(RemotePlatform {
            os: "linux",
            arch: "x86_64",
        });
        let started = Instant::now();
        let bridge = SshStdioBridge::start(
            "example".to_string(),
            remote_gardn,
            socket.clone(),
            "default".to_string(),
            None,
        )
        .expect("start bridge listener");
        drop(bridge);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "dropping an idle Windows bridge must not block on accept"
        );
        let _ = std::fs::remove_file(socket);
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
    fn ssh_config_include_path_normalizes_windows_separators() {
        let path = PathBuf::from(r"C:\Users\me\.ssh\config");
        assert_eq!(
            ssh_config_include_path(&path),
            "\"C:/Users/me/.ssh/config\""
        );
    }
}
