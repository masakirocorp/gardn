use std::fs;
use std::io;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

use portable_pty::CommandBuilder;
use serde_json::{json, Map, Value};

use crate::layout::PaneId;

pub(crate) const HAKO_PANE_ID_ENV_VAR: &str = "HAKO_PANE_ID";
const LEGACY_PI_OMP_EXTENSION_INSTALL_NAME: &str = "hako-agent-state.ts";
const PI_EXTENSION_INSTALL_NAME: &str = "hako-pi-agent-state.ts";
const PI_EXTENSION_ASSET: &str = include_str!("assets/pi/hako-agent-state.ts");
const PI_INTEGRATION_VERSION: u32 = 3;
const OMP_EXTENSION_INSTALL_NAME: &str = "hako-omp-agent-state.ts";
const OMP_EXTENSION_ASSET: &str = include_str!("assets/omp/hako-agent-state.ts");
const OMP_INTEGRATION_VERSION: u32 = 3;
const PI_CODING_AGENT_DIR_ENV_VAR: &str = "PI_CODING_AGENT_DIR";
const OMP_CONFIG_DIR_ENV_VAR: &str = "PI_CONFIG_DIR";
const CLAUDE_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const CLAUDE_HOOK_ASSET: &str = include_str!("assets/claude/hako-agent-state.sh");
const CLAUDE_INTEGRATION_VERSION: u32 = 2;
const CLAUDE_CONFIG_DIR_ENV_VAR: &str = "CLAUDE_CONFIG_DIR";
const CLAUDE_HOOK_EVENTS: [(&str, &str, Option<&str>); 13] = [
    ("SessionStart", "session", Some("*")),
    ("SessionStart", "working", Some("compact")),
    ("UserPromptSubmit", "working", None),
    ("SubagentStart", "working", Some("*")),
    ("TaskCreated", "working", None),
    ("PreCompact", "working", Some("*")),
    ("PostCompact", "working", Some("*")),
    ("PreToolUse", "working", None),
    ("PostToolUse", "working", None),
    ("PostToolUseFailure", "working", None),
    ("PermissionRequest", "blocked", None),
    (
        "Notification",
        "blocked",
        Some("permission_prompt|elicitation_dialog"),
    ),
    ("Notification", "idle", Some("idle_prompt")),
];
const CODEX_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const CODEX_HOOK_ASSET: &str = include_str!("assets/codex/hako-agent-state.sh");
const CODEX_INTEGRATION_VERSION: u32 = 2;
const CODEX_HOOK_EVENTS: [(&str, &str, Option<&str>); 10] = [
    ("SessionStart", "session", None),
    ("SessionStart", "working", Some("compact")),
    ("UserPromptSubmit", "working", None),
    ("SubagentStart", "working", None),
    ("PreCompact", "working", Some("*")),
    ("PostCompact", "working", Some("*")),
    ("PreToolUse", "working", None),
    ("PostToolUse", "working", None),
    ("PostToolUseFailure", "working", None),
    ("PermissionRequest", "blocked", None),
];
const CODEX_HOME_ENV_VAR: &str = "CODEX_HOME";
const KIMI_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const KIMI_HOOK_ASSET: &str = include_str!("assets/kimi/hako-agent-state.sh");
const KIMI_INTEGRATION_VERSION: u32 = 3;
const KIMI_CODE_HOME_ENV_VAR: &str = "KIMI_CODE_HOME";
const KIMI_CONFIG_BLOCK_BEGIN: &str = "# >>> hako kimi integration";
const KIMI_CONFIG_BLOCK_END: &str = "# <<< hako kimi integration";
const KIMI_MIN_VERSION: &str = "0.8.0";
const KIMI_HOOK_EVENTS: [(&str, &str); 9] = [
    ("SessionStart", "idle"),
    ("UserPromptSubmit", "working"),
    ("PreToolUse", "working"),
    ("PermissionRequest", "blocked"),
    ("PreCompact", "working"),
    ("PostCompact", "working"),
    ("Stop", "idle"),
    ("SessionEnd", "idle"),
    ("SessionEnd", "release"),
];
const COPILOT_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const COPILOT_HOOK_ASSET: &str = include_str!("assets/copilot/hako-agent-state.sh");
const COPILOT_INTEGRATION_VERSION: u32 = 1;
const COPILOT_HOME_ENV_VAR: &str = "COPILOT_HOME";
const DEVIN_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const DEVIN_HOOK_ASSET: &str = include_str!("assets/devin/hako-agent-state.sh");
const DEVIN_INTEGRATION_VERSION: u32 = 1;
const DEVIN_CONFIG_DIR_ENV_VAR: &str = "DEVIN_CONFIG_DIR";
const DEVIN_HOOK_EVENTS: [(&str, &str); 6] = [
    ("SessionStart", "session"),
    ("UserPromptSubmit", "session"),
    ("PreToolUse", "session"),
    ("PostToolUse", "session"),
    ("PermissionRequest", "session"),
    ("Stop", "session"),
];
const DEVIN_REMOVED_LIFECYCLE_HOOK_EVENTS: [(&str, &str); 6] = [
    ("UserPromptSubmit", "working"),
    ("PreToolUse", "working"),
    ("PostToolUse", "working"),
    ("PermissionRequest", "blocked"),
    ("Stop", "idle"),
    ("SessionEnd", "release"),
];
const DROID_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const DROID_HOOK_ASSET: &str = include_str!("assets/droid/hako-agent-state.sh");
const DROID_INTEGRATION_VERSION: u32 = 3;
const DROID_HOOK_EVENTS: [(&str, &str); 10] = [
    ("SessionStart", "idle"),
    ("UserPromptSubmit", "working"),
    ("PreToolUse", "working"),
    ("PermissionRequest", "blocked"),
    ("PostToolUse", "working"),
    ("PostToolUseFailure", "working"),
    ("PreCompact", "working"),
    ("PostCompact", "working"),
    ("Stop", "idle"),
    ("SessionEnd", "release"),
];
const DROID_REMOVED_LIFECYCLE_HOOK_EVENTS: [(&str, &str); 9] = [
    ("SessionStart", "idle"),
    ("UserPromptSubmit", "working"),
    ("PreToolUse", "working"),
    ("PermissionRequest", "blocked"),
    ("PostToolUse", "working"),
    ("PostToolUseFailure", "working"),
    ("SubagentStop", "working"),
    ("Stop", "idle"),
    ("SessionEnd", "release"),
];
const OPENCODE_PLUGIN_INSTALL_NAME: &str = "hako-agent-state.js";
const OPENCODE_PLUGIN_ASSET: &str = include_str!("assets/opencode/hako-agent-state.js");
const OPENCODE_INTEGRATION_VERSION: u32 = 5;
const HERMES_PLUGIN_INSTALL_NAME: &str = "hako-agent-state";
const HERMES_PLUGIN_MANIFEST_INSTALL_NAME: &str = "plugin.yaml";
const HERMES_PLUGIN_INIT_INSTALL_NAME: &str = "__init__.py";
const HERMES_PLUGIN_MANIFEST_ASSET: &str = include_str!("assets/hermes/plugin.yaml");
const HERMES_PLUGIN_INIT_ASSET: &str = include_str!("assets/hermes/__init__.py");
const HERMES_INTEGRATION_VERSION: u32 = 1;
const QODERCLI_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const QODERCLI_HOOK_ASSET: &str = include_str!("assets/qodercli/hako-agent-state.sh");
const QODERCLI_INTEGRATION_VERSION: u32 = 1;
const QODERCLI_CONFIG_DIR_ENV_VAR: &str = "QODER_CONFIG_DIR";
const CURSOR_HOOK_INSTALL_NAME: &str = "hako-agent-state.sh";
const CURSOR_HOOK_ASSET: &str = include_str!("assets/cursor/hako-agent-state.sh");
const CURSOR_INTEGRATION_VERSION: u32 = 2;
const CURSOR_CONFIG_DIR_ENV_VAR: &str = "CURSOR_CONFIG_DIR";
const INTEGRATION_VERSION_MARKER: &str = "HAKO_INTEGRATION_VERSION=";
const INTEGRATION_ID_MARKER: &str = "HAKO_INTEGRATION_ID=";

#[derive(Debug)]
pub(crate) struct ClaudeInstallPaths {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CodexInstallPaths {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CopilotInstallPaths {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct DevinInstallPaths {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct KimiInstallPaths {
    pub hook_path: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct DroidInstallPaths {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub settings_path: PathBuf,
    pub updated_legacy_hooks: bool,
}

#[derive(Debug)]
pub(crate) struct OpenCodeInstallPaths {
    pub plugin_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct OmpInstallPaths {
    pub extension_paths: Vec<PathBuf>,
    pub removed_legacy_pi_extensions: Vec<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct HermesInstallPaths {
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct QodercliInstallPaths {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CursorInstallPaths {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct QodercliUninstallResult {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_settings: bool,
}

#[derive(Debug)]
pub(crate) struct CopilotUninstallResult {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_settings: bool,
}

#[derive(Debug)]
pub(crate) struct DevinUninstallResult {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_settings: bool,
}

#[derive(Debug)]
pub(crate) struct KimiUninstallResult {
    pub hook_path: PathBuf,
    pub config_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_config: bool,
}

#[derive(Debug)]
pub(crate) struct DroidUninstallResult {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_hooks: bool,
    pub updated_settings: bool,
}

#[derive(Debug)]
pub(crate) struct CursorUninstallResult {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_hooks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntegrationStatus {
    pub target: crate::api::schema::IntegrationTarget,
    pub path: PathBuf,
    pub state: IntegrationStatusKind,
    pub installed_version: Option<u32>,
    pub expected_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntegrationStatusKind {
    NotInstalled,
    Current,
    Outdated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntegrationRecommendation {
    pub target: crate::api::schema::IntegrationTarget,
    pub label: &'static str,
    pub command: &'static str,
    pub available: bool,
    pub path: PathBuf,
    pub state: IntegrationStatusKind,
}

impl IntegrationRecommendation {
    pub fn needs_install(&self) -> bool {
        self.state == IntegrationStatusKind::Outdated
            || (self.available && self.state == IntegrationStatusKind::NotInstalled)
    }

    pub fn status_label(&self) -> &'static str {
        match (self.available, self.state) {
            (_, IntegrationStatusKind::Current) => "installed",
            (_, IntegrationStatusKind::Outdated) => "update available",
            (true, IntegrationStatusKind::NotInstalled) => "available",
            (false, IntegrationStatusKind::NotInstalled) => "not found",
        }
    }
}

#[derive(Debug)]
pub(crate) struct PiUninstallResult {
    pub extension_path: PathBuf,
    pub removed_extension: bool,
}

#[derive(Debug)]
pub(crate) struct OmpUninstallResult {
    pub extension_paths: Vec<PathBuf>,
    pub removed_extension_paths: Vec<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct ClaudeUninstallResult {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_settings: bool,
}

#[derive(Debug)]
pub(crate) struct CodexUninstallResult {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub config_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_hooks: bool,
}

#[derive(Debug)]
pub(crate) struct OpenCodeUninstallResult {
    pub plugin_path: PathBuf,
    pub removed_plugin: bool,
}

#[derive(Debug)]
pub(crate) struct HermesUninstallResult {
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub removed_plugin_dir: bool,
    pub updated_config: bool,
}

pub(crate) fn apply_pane_base_env(cmd: &mut CommandBuilder) {
    cmd.env(crate::api::SOCKET_PATH_ENV_VAR, crate::api::socket_path());
}

pub(crate) fn apply_pane_env(cmd: &mut CommandBuilder, pane_id: PaneId) {
    apply_pane_base_env(cmd);
    cmd.env(HAKO_PANE_ID_ENV_VAR, format!("p_{}", pane_id.raw()));
}

pub(crate) const INSTALL_WARNING_PREFIX: &str = "warning:";

struct AgentVersionRequirement {
    label: &'static str,
    binary: &'static str,
    args: &'static [&'static str],
    min_version: &'static str,
}

fn agent_version_requirement(
    target: crate::api::schema::IntegrationTarget,
) -> Option<AgentVersionRequirement> {
    match target {
        crate::api::schema::IntegrationTarget::Kimi => Some(AgentVersionRequirement {
            label: "kimi code",
            binary: "kimi",
            args: &["--version"],
            min_version: KIMI_MIN_VERSION,
        }),
        _ => None,
    }
}

fn extract_version_triple(text: &str) -> Option<(u64, u64, u64)> {
    text.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches('v');
        let (major, rest) = token.split_once('.')?;
        let (minor, patch) = match rest.split_once('.') {
            Some((minor, patch)) => (minor, patch),
            None => (rest, "0"),
        };
        let patch_digits = patch
            .char_indices()
            .find_map(|(index, c)| (!c.is_ascii_digit()).then_some(index))
            .unwrap_or(patch.len());

        Some((
            major.parse().ok()?,
            minor.parse().ok()?,
            patch[..patch_digits].parse().ok()?,
        ))
    })
}

/// Returns `Ok(None)` when the installed agent satisfies the requirement,
/// `Ok(Some(warning))` when the version cannot be determined and installation
/// can continue, and `Err` when the installed agent is too old.
fn enforce_agent_version(requirement: &AgentVersionRequirement) -> io::Result<Option<String>> {
    let probe = format!("{} {}", requirement.binary, requirement.args.join(" "));
    let output = match std::process::Command::new(requirement.binary)
        .args(requirement.args)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            return Ok(Some(format!(
                "{INSTALL_WARNING_PREFIX} could not run `{probe}` to verify the installed version; hooks require {} {} or newer",
                requirement.label, requirement.min_version
            )));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(found) = extract_version_triple(&stdout) else {
        return Ok(Some(format!(
            "{INSTALL_WARNING_PREFIX} could not parse the {} version from `{probe}` output; hooks require {} {} or newer",
            requirement.label, requirement.label, requirement.min_version
        )));
    };
    let required = extract_version_triple(requirement.min_version)
        .expect("static min version must be a valid version triple");

    if found < required {
        return Err(io::Error::other(format!(
            "{label} {}.{}.{} is too old: hako hooks require {label} {min} or newer. upgrade {label}, then re-run install",
            found.0,
            found.1,
            found.2,
            label = requirement.label,
            min = requirement.min_version
        )));
    }
    Ok(None)
}

pub(crate) fn install_target(
    target: crate::api::schema::IntegrationTarget,
) -> io::Result<Vec<String>> {
    let result = install_target_inner(target);
    let outcome = if result.is_ok() { "ok" } else { "error" };
    crate::logging::integration_action("install", integration_target_label(target), outcome);
    result
}

fn install_target_inner(target: crate::api::schema::IntegrationTarget) -> io::Result<Vec<String>> {
    if !integration_target_supported(target) {
        return Err(io::Error::other(format!(
            "{} integration is not supported on Windows",
            integration_target_label(target)
        )));
    }

    let version_warning = match agent_version_requirement(target) {
        Some(requirement) => enforce_agent_version(&requirement)?,
        None => None,
    };

    let mut messages = match target {
        crate::api::schema::IntegrationTarget::Pi => {
            let path = install_pi()?;
            vec![format!("installed pi integration to {}", path.display())]
        }
        crate::api::schema::IntegrationTarget::Omp => {
            let installed = install_omp()?;
            let mut messages = installed
                .removed_legacy_pi_extensions
                .into_iter()
                .map(|path| {
                    format!(
                        "removed legacy pi/omp integration from omp extension directory at {}",
                        path.display()
                    )
                })
                .collect::<Vec<_>>();
            messages.extend(
                installed
                    .extension_paths
                    .into_iter()
                    .map(|path| format!("installed omp integration to {}", path.display())),
            );
            messages
        }
        crate::api::schema::IntegrationTarget::Claude => {
            let installed = install_claude()?;
            vec![
                format!(
                    "installed claude integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured claude settings at {}",
                    installed.settings_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Codex => {
            let installed = install_codex()?;
            vec![
                format!(
                    "installed codex integration hook to {}",
                    installed.hook_path.display()
                ),
                format!("ensured codex hooks at {}", installed.hooks_path.display()),
                format!(
                    "ensured codex config at {}",
                    installed.config_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Copilot => {
            let installed = install_copilot()?;
            vec![
                format!(
                    "installed copilot integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured copilot settings at {}",
                    installed.settings_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Devin => {
            let installed = install_devin()?;
            vec![
                format!(
                    "installed devin integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured devin settings at {}",
                    installed.settings_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Kimi => {
            let installed = install_kimi()?;
            vec![
                format!(
                    "installed kimi integration hook to {}",
                    installed.hook_path.display()
                ),
                format!("ensured kimi config at {}", installed.config_path.display()),
            ]
        }
        crate::api::schema::IntegrationTarget::Droid => {
            let installed = install_droid()?;
            let mut messages = vec![
                format!(
                    "installed droid integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured droid hooks at {}",
                    installed.settings_path.display()
                ),
            ];
            if installed.updated_legacy_hooks {
                messages.push(format!(
                    "removed legacy hako droid hook entries from {}",
                    installed.hooks_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Opencode => {
            let installed = install_opencode()?;
            vec![format!(
                "installed opencode integration plugin to {}",
                installed.plugin_path.display()
            )]
        }
        crate::api::schema::IntegrationTarget::Hermes => {
            let installed = install_hermes()?;
            vec![
                format!(
                    "installed hermes integration plugin to {}",
                    installed.plugin_dir.display()
                ),
                format!(
                    "enabled hermes plugin in {}",
                    installed.config_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Qodercli => {
            let installed = install_qodercli()?;
            vec![
                format!(
                    "installed qodercli integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured qodercli settings at {}",
                    installed.settings_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Cursor => {
            let installed = install_cursor()?;
            vec![
                format!(
                    "installed cursor integration hook to {}",
                    installed.hook_path.display()
                ),
                format!("updated cursor hooks at {}", installed.hooks_path.display()),
            ]
        }
    };

    if let Some(warning) = version_warning {
        messages.push(warning);
    }

    Ok(messages)
}

pub(crate) fn uninstall_target(
    target: crate::api::schema::IntegrationTarget,
) -> io::Result<Vec<String>> {
    let messages = match target {
        crate::api::schema::IntegrationTarget::Pi => {
            let result = uninstall_pi()?;
            if result.removed_extension {
                vec![format!(
                    "removed pi integration extension at {}",
                    result.extension_path.display()
                )]
            } else {
                vec![format!(
                    "no pi integration extension found at {}",
                    result.extension_path.display()
                )]
            }
        }
        crate::api::schema::IntegrationTarget::Omp => {
            let result = uninstall_omp()?;
            if result.removed_extension_paths.is_empty() {
                result
                    .extension_paths
                    .into_iter()
                    .map(|path| format!("no omp integration extension found at {}", path.display()))
                    .collect()
            } else {
                result
                    .removed_extension_paths
                    .into_iter()
                    .map(|path| format!("removed omp integration extension at {}", path.display()))
                    .collect()
            }
        }
        crate::api::schema::IntegrationTarget::Claude => {
            let result = uninstall_claude()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed claude hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no claude hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed hako claude hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako claude hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Codex => {
            let result = uninstall_codex()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed codex hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no codex hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_hooks {
                messages.push(format!(
                    "removed hako codex hook entries from {}",
                    result.hooks_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako codex hook entries found in {}",
                    result.hooks_path.display()
                ));
            }
            messages.push(format!(
                "left codex config unchanged at {}",
                result.config_path.display()
            ));
            messages
        }
        crate::api::schema::IntegrationTarget::Kimi => {
            let result = uninstall_kimi()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed kimi hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no kimi hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_config {
                messages.push(format!(
                    "removed hako kimi hook entries from {}",
                    result.config_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako kimi hook entries found in {}",
                    result.config_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Droid => {
            let result = uninstall_droid()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed droid hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no droid hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_hooks {
                messages.push(format!(
                    "removed legacy hako droid hook entries from {}",
                    result.hooks_path.display()
                ));
            } else {
                messages.push(format!(
                    "no legacy hako droid hook entries found in {}",
                    result.hooks_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed hako droid hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako droid hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Opencode => {
            let result = uninstall_opencode()?;
            if result.removed_plugin {
                vec![format!(
                    "removed opencode integration plugin at {}",
                    result.plugin_path.display()
                )]
            } else {
                vec![format!(
                    "no opencode integration plugin found at {}",
                    result.plugin_path.display()
                )]
            }
        }
        crate::api::schema::IntegrationTarget::Copilot => {
            let result = uninstall_copilot()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed copilot hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no copilot hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed hako copilot hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako copilot hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Devin => {
            let result = uninstall_devin()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed devin hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no devin hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed hako devin hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako devin hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Hermes => {
            let result = uninstall_hermes()?;
            let mut messages = Vec::new();
            if result.removed_plugin_dir {
                messages.push(format!(
                    "removed hermes integration plugin at {}",
                    result.plugin_dir.display()
                ));
            } else {
                messages.push(format!(
                    "no hermes integration plugin found at {}",
                    result.plugin_dir.display()
                ));
            }
            if result.updated_config {
                messages.push(format!(
                    "disabled hermes plugin in {}",
                    result.config_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hermes plugin entry found in {}",
                    result.config_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Qodercli => {
            let result = uninstall_qodercli()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed qodercli hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no qodercli hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed hako qodercli hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako qodercli hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Cursor => {
            let result = uninstall_cursor()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed cursor hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no cursor hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_hooks {
                messages.push(format!(
                    "removed hako cursor hook entries from {}",
                    result.hooks_path.display()
                ));
            } else {
                messages.push(format!(
                    "no hako cursor hook entries found in {}",
                    result.hooks_path.display()
                ));
            }
            messages
        }
    };

    crate::logging::integration_action("uninstall", integration_target_label(target), "ok");
    Ok(messages)
}

pub(crate) fn integration_target_label(
    target: crate::api::schema::IntegrationTarget,
) -> &'static str {
    match target {
        crate::api::schema::IntegrationTarget::Pi => "pi",
        crate::api::schema::IntegrationTarget::Omp => "omp",
        crate::api::schema::IntegrationTarget::Claude => "claude",
        crate::api::schema::IntegrationTarget::Codex => "codex",
        crate::api::schema::IntegrationTarget::Copilot => "copilot",
        crate::api::schema::IntegrationTarget::Devin => "devin",
        crate::api::schema::IntegrationTarget::Kimi => "kimi",
        crate::api::schema::IntegrationTarget::Droid => "droid",
        crate::api::schema::IntegrationTarget::Opencode => "opencode",
        crate::api::schema::IntegrationTarget::Hermes => "hermes",
        crate::api::schema::IntegrationTarget::Qodercli => "qodercli",
        crate::api::schema::IntegrationTarget::Cursor => "cursor",
    }
}

pub(crate) fn integration_target_command(
    target: crate::api::schema::IntegrationTarget,
) -> &'static str {
    match target {
        crate::api::schema::IntegrationTarget::Pi => "pi",
        crate::api::schema::IntegrationTarget::Omp => "omp",
        crate::api::schema::IntegrationTarget::Claude => "claude",
        crate::api::schema::IntegrationTarget::Codex => "codex",
        crate::api::schema::IntegrationTarget::Copilot => "copilot",
        crate::api::schema::IntegrationTarget::Devin => "devin",
        crate::api::schema::IntegrationTarget::Kimi => "kimi",
        crate::api::schema::IntegrationTarget::Droid => "droid",
        crate::api::schema::IntegrationTarget::Opencode => "opencode",
        crate::api::schema::IntegrationTarget::Hermes => "hermes",
        crate::api::schema::IntegrationTarget::Qodercli => "qodercli",
        crate::api::schema::IntegrationTarget::Cursor => "cursor-agent",
    }
}

fn integration_target_supported(target: crate::api::schema::IntegrationTarget) -> bool {
    integration_target_supported_for_platform(target, cfg!(windows))
}

fn integration_target_supported_for_platform(
    target: crate::api::schema::IntegrationTarget,
    is_windows: bool,
) -> bool {
    if !is_windows {
        return true;
    }

    matches!(
        target,
        crate::api::schema::IntegrationTarget::Claude
            | crate::api::schema::IntegrationTarget::Codex
            | crate::api::schema::IntegrationTarget::Copilot
            | crate::api::schema::IntegrationTarget::Droid
            | crate::api::schema::IntegrationTarget::Kimi
            | crate::api::schema::IntegrationTarget::Qodercli
    )
}

fn integration_target_available(target: crate::api::schema::IntegrationTarget) -> bool {
    integration_target_supported(target) && command_available(integration_target_command(target))
}

fn command_available(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| executable_file_exists(&dir.join(command)))
}

fn executable_file_exists(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

pub(crate) fn installed_integration_statuses() -> Vec<IntegrationStatus> {
    integration_specs()
        .into_iter()
        .filter_map(|(target, path, expected_version)| {
            if !integration_target_supported(target) {
                return None;
            }
            Some(integration_status_at(target, path.ok()?, expected_version))
        })
        .collect()
}

pub(crate) fn integration_recommendations() -> Vec<IntegrationRecommendation> {
    integration_specs()
        .into_iter()
        .filter_map(|(target, path, expected_version)| {
            if !integration_target_supported(target) {
                return None;
            }
            let path = path.ok()?;
            let status = integration_status_at(target, path.clone(), expected_version);
            Some(IntegrationRecommendation {
                target,
                label: integration_target_label(target),
                command: integration_target_command(target),
                available: integration_target_available(target)
                    || status.state != IntegrationStatusKind::NotInstalled,
                path,
                state: status.state,
            })
        })
        .collect()
}

fn outdated_installed_integrations() -> Vec<IntegrationStatus> {
    installed_integration_statuses()
        .into_iter()
        .filter(|status| status.state == IntegrationStatusKind::Outdated)
        .collect()
}

fn integration_specs() -> [(
    crate::api::schema::IntegrationTarget,
    io::Result<PathBuf>,
    u32,
); 12] {
    [
        (
            crate::api::schema::IntegrationTarget::Pi,
            pi_extension_dir().map(|dir| dir.join(PI_EXTENSION_INSTALL_NAME)),
            PI_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Omp,
            omp_extension_dir().map(|dir| dir.join(OMP_EXTENSION_INSTALL_NAME)),
            OMP_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Claude,
            claude_dir().map(|dir| dir.join("hooks").join(CLAUDE_HOOK_INSTALL_NAME)),
            CLAUDE_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Codex,
            codex_dir().map(|dir| dir.join(CODEX_HOOK_INSTALL_NAME)),
            CODEX_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Copilot,
            copilot_dir().map(|dir| dir.join("hooks").join(COPILOT_HOOK_INSTALL_NAME)),
            COPILOT_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Devin,
            devin_dir().map(|dir| dir.join(DEVIN_HOOK_INSTALL_NAME)),
            DEVIN_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Kimi,
            kimi_dir().map(|dir| dir.join("hooks").join(KIMI_HOOK_INSTALL_NAME)),
            KIMI_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Droid,
            droid_dir().map(|dir| dir.join("hooks").join(DROID_HOOK_INSTALL_NAME)),
            DROID_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Opencode,
            opencode_dir().map(|dir| dir.join("plugins").join(OPENCODE_PLUGIN_INSTALL_NAME)),
            OPENCODE_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Hermes,
            hermes_plugin_dir().map(|dir| dir.join(HERMES_PLUGIN_INIT_INSTALL_NAME)),
            HERMES_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Qodercli,
            qodercli_dir().map(|dir| dir.join("hooks").join(QODERCLI_HOOK_INSTALL_NAME)),
            QODERCLI_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Cursor,
            cursor_dir().map(|dir| dir.join(CURSOR_HOOK_INSTALL_NAME)),
            CURSOR_INTEGRATION_VERSION,
        ),
    ]
}

pub(crate) fn integration_update_instructions(
    targets: &[crate::api::schema::IntegrationTarget],
) -> String {
    let commands: Vec<String> = targets
        .iter()
        .map(|target| {
            format!(
                "`hako integration install {}`",
                integration_target_label(*target)
            )
        })
        .collect();

    match commands.as_slice() {
        [] => String::new(),
        [command] => format!("run {command}"),
        [rest @ .., last] => format!("run {} and {last}", rest.join(", ")),
    }
}

pub(crate) fn print_outdated_update_notice() -> bool {
    let outdated = outdated_installed_integrations();
    if outdated.is_empty() {
        return false;
    }

    let targets = outdated
        .iter()
        .map(|integration| integration.target)
        .collect::<Vec<_>>();
    eprintln!(
        "installed hako integrations need updating; {}.",
        integration_update_instructions(&targets).replace('`', "")
    );
    true
}

fn integration_status_at(
    target: crate::api::schema::IntegrationTarget,
    path: PathBuf,
    expected_version: u32,
) -> IntegrationStatus {
    if !path.is_file() {
        return IntegrationStatus {
            target,
            path,
            state: IntegrationStatusKind::NotInstalled,
            installed_version: None,
            expected_version,
        };
    }

    let content = fs::read_to_string(&path).ok();
    let installed_version = content.as_deref().and_then(parse_integration_version);
    let installed_id_matches = content
        .as_deref()
        .and_then(parse_integration_id)
        .is_some_and(|id| id == integration_target_label(target));
    let installed_content_matches = content
        .as_deref()
        .is_some_and(|content| content == integration_asset_for_target(target));
    let state = if installed_id_matches
        && installed_version == Some(expected_version)
        && installed_content_matches
    {
        IntegrationStatusKind::Current
    } else {
        IntegrationStatusKind::Outdated
    };

    IntegrationStatus {
        target,
        path,
        state,
        installed_version,
        expected_version,
    }
}

fn integration_asset_for_target(target: crate::api::schema::IntegrationTarget) -> &'static str {
    match target {
        crate::api::schema::IntegrationTarget::Pi => PI_EXTENSION_ASSET,
        crate::api::schema::IntegrationTarget::Omp => OMP_EXTENSION_ASSET,
        crate::api::schema::IntegrationTarget::Claude => CLAUDE_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Codex => CODEX_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Copilot => COPILOT_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Devin => DEVIN_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Kimi => KIMI_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Droid => DROID_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Opencode => OPENCODE_PLUGIN_ASSET,
        crate::api::schema::IntegrationTarget::Hermes => HERMES_PLUGIN_INIT_ASSET,
        crate::api::schema::IntegrationTarget::Qodercli => QODERCLI_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Cursor => CURSOR_HOOK_ASSET,
    }
}

fn parse_integration_id(content: &str) -> Option<&str> {
    parse_integration_marker(content, INTEGRATION_ID_MARKER)
}

fn parse_integration_version(content: &str) -> Option<u32> {
    parse_integration_marker(content, INTEGRATION_VERSION_MARKER)?
        .parse()
        .ok()
}

fn parse_integration_marker<'a>(content: &'a str, marker: &str) -> Option<&'a str> {
    content.lines().find_map(|line| {
        let marker_line = line
            .trim()
            .trim_start_matches('/')
            .trim_start_matches('#')
            .trim();
        Some(marker_line.strip_prefix(marker)?.trim())
    })
}

pub(crate) fn install_pi() -> io::Result<PathBuf> {
    let dir = pi_extension_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "pi extension directory not found at {}. install pi and create the extensions directory first",
            dir.display()
        )));
    }
    fs::create_dir_all(&dir)?;
    remove_legacy_pi_omp_extension_from_dir(&dir)?;
    let path = dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(&path, PI_EXTENSION_ASSET)?;
    Ok(path)
}

pub(crate) fn install_omp() -> io::Result<OmpInstallPaths> {
    let dirs = omp_install_extension_dirs()?;
    let mut extension_paths = Vec::with_capacity(dirs.len());
    let mut removed_legacy_pi_extensions = Vec::new();

    for dir in dirs {
        let Some(agent_dir) = dir.parent() else {
            return Err(io::Error::other(format!(
                "omp extension directory has no parent at {}",
                dir.display()
            )));
        };
        if !agent_dir.is_dir() {
            return Err(io::Error::other(format!(
                "omp agent directory not found at {}. install omp first",
                agent_dir.display()
            )));
        }
        fs::create_dir_all(&dir)?;

        if remove_legacy_pi_omp_extension_from_dir(&dir)? {
            removed_legacy_pi_extensions.push(dir.join(LEGACY_PI_OMP_EXTENSION_INSTALL_NAME));
        }
        let extension_path = dir.join(OMP_EXTENSION_INSTALL_NAME);
        fs::write(&extension_path, OMP_EXTENSION_ASSET)?;
        extension_paths.push(extension_path);
    }

    Ok(OmpInstallPaths {
        extension_paths,
        removed_legacy_pi_extensions,
    })
}
fn remove_legacy_pi_omp_extension_from_dir(dir: &Path) -> io::Result<bool> {
    let legacy_path = dir.join(LEGACY_PI_OMP_EXTENSION_INSTALL_NAME);
    if !legacy_path.is_file() {
        return Ok(false);
    }

    let content = fs::read_to_string(&legacy_path)?;
    if matches!(parse_integration_id(&content), Some("pi" | "omp")) {
        fs::remove_file(legacy_path)?;
        return Ok(true);
    }

    Ok(false)
}

pub(crate) fn install_claude() -> io::Result<ClaudeInstallPaths> {
    let dir = claude_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "claude directory not found at {}. install claude code first",
            dir.display()
        )));
    }

    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join(CLAUDE_HOOK_INSTALL_NAME);
    fs::write(&hook_path, CLAUDE_HOOK_ASSET)?;
    make_executable(&hook_path)?;

    let settings_path = dir.join("settings.json");
    let mut settings = if settings_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?).map_err(|err| {
            io::Error::other(format!(
                "failed to parse {}: {err}",
                settings_path.display()
            ))
        })?
    } else {
        json!({})
    };

    let hooks = ensure_hooks_object(
        &mut settings,
        &settings_path,
        "claude settings",
        "claude settings hooks",
    )?;
    let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
    for (event, action) in [
        ("SessionStart", "idle"),
        ("UserPromptSubmit", "working"),
        ("PreToolUse", "working"),
        ("PermissionRequest", "blocked"),
        ("Stop", "idle"),
        ("SessionEnd", "release"),
        ("PostToolUse", "working"),
        ("PostToolUseFailure", "working"),
        ("SubagentStop", "working"),
    ] {
        remove_command_hook(hooks, event, &format!("bash {quoted_hook_path} {action}"))?;
    }
    for (event, action, matcher) in CLAUDE_HOOK_EVENTS {
        ensure_command_hook(
            hooks,
            event,
            format!("bash {quoted_hook_path} {action}"),
            10,
            matcher,
        )?;
    }
    ensure_command_hook(
        hooks,
        "Stop",
        format!("bash {quoted_hook_path} idle"),
        10,
        None,
    )?;
    ensure_command_hook(
        hooks,
        "SessionEnd",
        format!("bash {quoted_hook_path} release"),
        10,
        None,
    )?;

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(ClaudeInstallPaths {
        hook_path,
        settings_path,
    })
}

pub(crate) fn install_codex() -> io::Result<CodexInstallPaths> {
    let dir = codex_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "codex config directory not found at {}. install codex first",
            dir.display()
        )));
    }

    let hook_path = dir.join(CODEX_HOOK_INSTALL_NAME);
    fs::write(&hook_path, CODEX_HOOK_ASSET)?;
    make_executable(&hook_path)?;

    let hooks_path = dir.join("hooks.json");
    let mut hooks_file = if hooks_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?).map_err(|err| {
            io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
        })?
    } else {
        json!({})
    };

    let hooks = ensure_hooks_object(
        &mut hooks_file,
        &hooks_path,
        "codex hooks file",
        "codex hooks file hooks",
    )?;
    let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
    for (event, action) in [
        ("SessionStart", "idle"),
        ("UserPromptSubmit", "working"),
        ("PreToolUse", "working"),
        ("PermissionRequest", "blocked"),
        ("Stop", "idle"),
    ] {
        remove_command_hook(hooks, event, &format!("bash {quoted_hook_path} {action}"))?;
    }
    for (event, action, matcher) in CODEX_HOOK_EVENTS {
        ensure_command_hook(
            hooks,
            event,
            format!("bash {quoted_hook_path} {action}"),
            10,
            matcher,
        )?;
    }
    ensure_command_hook(
        hooks,
        "Stop",
        format!("bash {quoted_hook_path} idle"),
        10,
        None,
    )?;

    fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;

    let config_path = dir.join("config.toml");
    let existing_config = if config_path.is_file() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };
    let new_config = build_codex_config_with_hooks(&existing_config);
    if new_config != existing_config {
        fs::write(&config_path, new_config)?;
    }

    Ok(CodexInstallPaths {
        hook_path,
        hooks_path,
        config_path,
    })
}

pub(crate) fn install_kimi() -> io::Result<KimiInstallPaths> {
    let dir = kimi_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "kimi code config directory not found at {}. install kimi code first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(KIMI_HOOK_INSTALL_NAME);
    fs::write(&hook_path, KIMI_HOOK_ASSET)?;
    make_executable(&hook_path)?;
    let config_path = dir.join("config.toml");
    let existing_config = if config_path.is_file() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };
    let new_config = build_kimi_config_with_hooks(&existing_config, &hook_path);
    if new_config != existing_config {
        fs::write(&config_path, new_config)?;
    }
    Ok(KimiInstallPaths {
        hook_path,
        config_path,
    })
}

pub(crate) fn install_droid() -> io::Result<DroidInstallPaths> {
    let dir = droid_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "droid config directory not found at {}. install droid first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(DROID_HOOK_INSTALL_NAME);
    fs::write(&hook_path, DROID_HOOK_ASSET)?;
    make_executable(&hook_path)?;
    let settings_path = dir.join("settings.json");
    let mut settings = if settings_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?).map_err(|err| {
            io::Error::other(format!(
                "failed to parse {}: {err}",
                settings_path.display()
            ))
        })?
    } else {
        json!({})
    };
    let hooks = ensure_hooks_object(
        &mut settings,
        &settings_path,
        "droid settings",
        "droid settings hooks",
    )?;
    remove_hook_commands(hooks, "SessionStart", &hook_path, None)?;
    for (event, action) in DROID_REMOVED_LIFECYCLE_HOOK_EVENTS {
        remove_hook_commands(hooks, event, &hook_path, Some(action))?;
    }
    for (event, action) in DROID_HOOK_EVENTS {
        remove_hook_commands(hooks, event, &hook_path, Some(action))?;
        ensure_command_hook(
            hooks,
            event,
            hook_command(&hook_path, Some(action)),
            10,
            None,
        )?;
    }
    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    let hooks_path = dir.join("hooks.json");
    let mut updated_legacy_hooks = false;
    if hooks_path.is_file() {
        let mut hooks_file = serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?)
            .map_err(|err| {
                io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
            })?;
        if let Some(hooks) = hooks_object_if_present(
            &mut hooks_file,
            &hooks_path,
            "droid hooks file",
            "droid hooks file hooks",
        )? {
            updated_legacy_hooks |= remove_hook_commands(hooks, "SessionStart", &hook_path, None)?;
            for (event, action) in DROID_REMOVED_LIFECYCLE_HOOK_EVENTS {
                updated_legacy_hooks |=
                    remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
            for (event, action) in DROID_HOOK_EVENTS {
                updated_legacy_hooks |=
                    remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
        }
        if updated_legacy_hooks {
            fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
        }
    }
    Ok(DroidInstallPaths {
        hook_path,
        hooks_path,
        settings_path,
        updated_legacy_hooks,
    })
}

pub(crate) fn install_copilot() -> io::Result<CopilotInstallPaths> {
    let dir = copilot_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "copilot config directory not found at {}. install github copilot cli first",
            dir.display()
        )));
    }

    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join(COPILOT_HOOK_INSTALL_NAME);
    fs::write(&hook_path, COPILOT_HOOK_ASSET)?;
    make_executable(&hook_path)?;

    let settings_path = dir.join("settings.json");
    let mut settings = if settings_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?).map_err(|err| {
            io::Error::other(format!(
                "failed to parse {}: {err}",
                settings_path.display()
            ))
        })?
    } else {
        json!({})
    };

    let hooks = ensure_hooks_object(
        &mut settings,
        &settings_path,
        "copilot settings",
        "copilot settings hooks",
    )?;
    let command = format!(
        "bash {}",
        shell_single_quote(&hook_path.display().to_string())
    );
    ensure_direct_command_hook(hooks, "SessionStart", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "UserPromptSubmit", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "PreToolUse", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "PostToolUse", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "PostToolUseFailure", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "Stop", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "agentStop", command.clone(), 10, None)?;
    ensure_direct_command_hook(hooks, "SessionEnd", command.clone(), 10, None)?;
    ensure_direct_command_hook(
        hooks,
        "notification",
        command,
        10,
        Some("permission_prompt|elicitation_dialog|agent_idle"),
    )?;

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(CopilotInstallPaths {
        hook_path,
        settings_path,
    })
}

pub(crate) fn install_devin() -> io::Result<DevinInstallPaths> {
    let dir = devin_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "devin config directory not found at {}. install devin cli first",
            dir.display()
        )));
    }

    let hook_path = dir.join(DEVIN_HOOK_INSTALL_NAME);
    fs::write(&hook_path, DEVIN_HOOK_ASSET)?;
    make_executable(&hook_path)?;

    let settings_path = dir.join("config.json");
    let mut settings = if settings_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?).map_err(|err| {
            io::Error::other(format!(
                "failed to parse {}: {err}",
                settings_path.display()
            ))
        })?
    } else {
        json!({})
    };

    let hooks = ensure_hooks_object(
        &mut settings,
        &settings_path,
        "devin settings",
        "devin settings hooks",
    )?;
    for (event, action) in DEVIN_REMOVED_LIFECYCLE_HOOK_EVENTS {
        remove_command_hook(hooks, event, &hook_command(&hook_path, Some(action)))?;
    }
    for (event, action) in DEVIN_HOOK_EVENTS {
        remove_command_hook(hooks, event, &hook_command(&hook_path, Some(action)))?;
    }
    for (event, action) in DEVIN_HOOK_EVENTS {
        ensure_command_hook(
            hooks,
            event,
            hook_command(&hook_path, Some(action)),
            10,
            None,
        )?;
    }

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(DevinInstallPaths {
        hook_path,
        settings_path,
    })
}

pub(crate) fn install_opencode() -> io::Result<OpenCodeInstallPaths> {
    let dir = opencode_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "opencode config directory not found at {}. install opencode first",
            dir.display()
        )));
    }

    let plugins_dir = dir.join("plugins");
    fs::create_dir_all(&plugins_dir)?;

    let plugin_path = plugins_dir.join(OPENCODE_PLUGIN_INSTALL_NAME);
    fs::write(&plugin_path, OPENCODE_PLUGIN_ASSET)?;

    Ok(OpenCodeInstallPaths { plugin_path })
}

pub(crate) fn install_hermes() -> io::Result<HermesInstallPaths> {
    let dir = hermes_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "hermes config directory not found at {}. install hermes agent first",
            dir.display()
        )));
    }

    let plugin_dir = hermes_plugin_dir()?;
    fs::create_dir_all(&plugin_dir)?;
    fs::write(
        plugin_dir.join(HERMES_PLUGIN_MANIFEST_INSTALL_NAME),
        HERMES_PLUGIN_MANIFEST_ASSET,
    )?;
    fs::write(
        plugin_dir.join(HERMES_PLUGIN_INIT_INSTALL_NAME),
        HERMES_PLUGIN_INIT_ASSET,
    )?;

    let config_path = dir.join("config.yaml");
    let existing_config = if config_path.is_file() {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };
    let new_config = ensure_hermes_plugin_enabled(&existing_config);
    if new_config != existing_config {
        fs::write(&config_path, new_config)?;
    }

    Ok(HermesInstallPaths {
        plugin_dir,
        config_path,
    })
}

pub(crate) fn uninstall_pi() -> io::Result<PiUninstallResult> {
    let extension_path = pi_extension_dir()?.join(PI_EXTENSION_INSTALL_NAME);
    let removed_extension = remove_matching_integration_file(&extension_path, "pi")?;

    Ok(PiUninstallResult {
        extension_path,
        removed_extension,
    })
}

pub(crate) fn uninstall_omp() -> io::Result<OmpUninstallResult> {
    let mut extension_paths = Vec::new();
    let mut removed_extension_paths = Vec::new();

    for dir in omp_install_extension_dirs()? {
        let extension_path = dir.join(OMP_EXTENSION_INSTALL_NAME);
        if remove_matching_integration_file(&extension_path, "omp")? {
            removed_extension_paths.push(extension_path.clone());
        }
        extension_paths.push(extension_path);
    }

    Ok(OmpUninstallResult {
        extension_paths,
        removed_extension_paths,
    })
}

pub(crate) fn uninstall_claude() -> io::Result<ClaudeUninstallResult> {
    let hook_path = claude_dir()?.join("hooks").join(CLAUDE_HOOK_INSTALL_NAME);
    let settings_path = claude_dir()?.join("settings.json");
    let mut updated_settings = false;

    if settings_path.is_file() {
        let mut settings = serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?)
            .map_err(|err| {
                io::Error::other(format!(
                    "failed to parse {}: {err}",
                    settings_path.display()
                ))
            })?;

        if let Some(hooks) = hooks_object_if_present(
            &mut settings,
            &settings_path,
            "claude settings",
            "claude settings hooks",
        )? {
            let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
            for (event, action) in [
                ("SessionStart", "idle"),
                ("Stop", "idle"),
                ("SessionEnd", "release"),
                ("SubagentStop", "working"),
            ] {
                updated_settings |= remove_command_hook(
                    hooks,
                    event,
                    &format!("bash {quoted_hook_path} {action}"),
                )?;
            }
            for (event, action, _matcher) in CLAUDE_HOOK_EVENTS {
                updated_settings |= remove_command_hook(
                    hooks,
                    event,
                    &format!("bash {quoted_hook_path} {action}"),
                )?;
            }
        }

        if updated_settings {
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }

    let removed_hook_file = remove_file_if_exists(&hook_path)?;

    Ok(ClaudeUninstallResult {
        hook_path,
        settings_path,
        removed_hook_file,
        updated_settings,
    })
}

pub(crate) fn uninstall_codex() -> io::Result<CodexUninstallResult> {
    let codex_dir = codex_dir()?;
    let hook_path = codex_dir.join(CODEX_HOOK_INSTALL_NAME);
    let hooks_path = codex_dir.join("hooks.json");
    let config_path = codex_dir.join("config.toml");
    let mut updated_hooks = false;

    if hooks_path.is_file() {
        let mut hooks_file = serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?)
            .map_err(|err| {
                io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
            })?;

        if let Some(hooks) = hooks_object_if_present(
            &mut hooks_file,
            &hooks_path,
            "codex hooks file",
            "codex hooks file hooks",
        )? {
            let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
            for (event, action) in [("SessionStart", "idle"), ("Stop", "idle")] {
                updated_hooks |= remove_command_hook(
                    hooks,
                    event,
                    &format!("bash {quoted_hook_path} {action}"),
                )?;
            }
            for (event, action, _matcher) in CODEX_HOOK_EVENTS {
                updated_hooks |= remove_command_hook(
                    hooks,
                    event,
                    &format!("bash {quoted_hook_path} {action}"),
                )?;
            }
        }

        if updated_hooks {
            fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
        }
    }

    let removed_hook_file = remove_file_if_exists(&hook_path)?;

    Ok(CodexUninstallResult {
        hook_path,
        hooks_path,
        config_path,
        removed_hook_file,
        updated_hooks,
    })
}

pub(crate) fn uninstall_kimi() -> io::Result<KimiUninstallResult> {
    let kimi_dir = kimi_dir()?;
    let hook_path = kimi_dir.join("hooks").join(KIMI_HOOK_INSTALL_NAME);
    let config_path = kimi_dir.join("config.toml");
    let mut updated_config = false;
    if config_path.is_file() {
        let existing_config = fs::read_to_string(&config_path)?;
        let new_config = remove_kimi_config_block(&existing_config);
        if new_config != existing_config {
            fs::write(&config_path, new_config)?;
            updated_config = true;
        }
    }
    let removed_hook_file = remove_matching_integration_file(&hook_path, "kimi")?;
    Ok(KimiUninstallResult {
        hook_path,
        config_path,
        removed_hook_file,
        updated_config,
    })
}

pub(crate) fn uninstall_droid() -> io::Result<DroidUninstallResult> {
    let droid_dir = droid_dir()?;
    let hook_path = droid_dir.join("hooks").join(DROID_HOOK_INSTALL_NAME);
    let hooks_path = droid_dir.join("hooks.json");
    let settings_path = droid_dir.join("settings.json");
    let mut updated_hooks = false;
    let mut updated_settings = false;
    if hooks_path.is_file() {
        let mut hooks_file = serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?)
            .map_err(|err| {
                io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
            })?;
        if let Some(hooks) = hooks_object_if_present(
            &mut hooks_file,
            &hooks_path,
            "droid hooks file",
            "droid hooks file hooks",
        )? {
            updated_hooks |= remove_hook_commands(hooks, "SessionStart", &hook_path, None)?;
            for (event, action) in DROID_REMOVED_LIFECYCLE_HOOK_EVENTS {
                updated_hooks |= remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
            for (event, action) in DROID_HOOK_EVENTS {
                updated_hooks |= remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
        }
        if updated_hooks {
            fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
        }
    }
    if settings_path.is_file() {
        let mut settings = serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?)
            .map_err(|err| {
                io::Error::other(format!(
                    "failed to parse {}: {err}",
                    settings_path.display()
                ))
            })?;
        if let Some(hooks) = hooks_object_if_present(
            &mut settings,
            &settings_path,
            "droid settings",
            "droid settings hooks",
        )? {
            updated_settings |= remove_hook_commands(hooks, "SessionStart", &hook_path, None)?;
            for (event, action) in DROID_REMOVED_LIFECYCLE_HOOK_EVENTS {
                updated_settings |= remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
            for (event, action) in DROID_HOOK_EVENTS {
                updated_settings |= remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
        }
        if updated_settings {
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }
    let removed_hook_file = remove_matching_integration_file(&hook_path, "droid")?;
    Ok(DroidUninstallResult {
        hook_path,
        hooks_path,
        settings_path,
        removed_hook_file,
        updated_hooks,
        updated_settings,
    })
}

pub(crate) fn uninstall_copilot() -> io::Result<CopilotUninstallResult> {
    let copilot_dir = copilot_dir()?;
    let hook_path = copilot_dir.join("hooks").join(COPILOT_HOOK_INSTALL_NAME);
    let settings_path = copilot_dir.join("settings.json");
    let mut updated_settings = false;

    if settings_path.is_file() {
        let mut settings = serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?)
            .map_err(|err| {
                io::Error::other(format!(
                    "failed to parse {}: {err}",
                    settings_path.display()
                ))
            })?;

        if let Some(hooks) = hooks_object_if_present(
            &mut settings,
            &settings_path,
            "copilot settings",
            "copilot settings hooks",
        )? {
            let command = format!(
                "bash {}",
                shell_single_quote(&hook_path.display().to_string())
            );
            updated_settings |= remove_direct_command_hook(hooks, "SessionStart", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "UserPromptSubmit", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "PreToolUse", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "PostToolUse", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "PostToolUseFailure", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "Stop", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "agentStop", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "SessionEnd", &command)?;
            updated_settings |= remove_direct_command_hook(hooks, "notification", &command)?;
        }

        if updated_settings {
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }

    let removed_hook_file = remove_file_if_exists(&hook_path)?;

    Ok(CopilotUninstallResult {
        hook_path,
        settings_path,
        removed_hook_file,
        updated_settings,
    })
}

pub(crate) fn uninstall_opencode() -> io::Result<OpenCodeUninstallResult> {
    let plugin_path = opencode_dir()?
        .join("plugins")
        .join(OPENCODE_PLUGIN_INSTALL_NAME);
    let removed_plugin = remove_file_if_exists(&plugin_path)?;

    Ok(OpenCodeUninstallResult {
        plugin_path,
        removed_plugin,
    })
}

pub(crate) fn uninstall_hermes() -> io::Result<HermesUninstallResult> {
    let dir = hermes_dir()?;
    let plugin_dir = hermes_plugin_dir()?;
    let config_path = dir.join("config.yaml");

    let removed_plugin_dir = remove_dir_all_if_exists(&plugin_dir)?;
    let mut updated_config = false;
    if config_path.is_file() {
        let existing_config = fs::read_to_string(&config_path)?;
        let new_config = remove_hermes_plugin_enabled(&existing_config);
        if new_config != existing_config {
            fs::write(&config_path, new_config)?;
            updated_config = true;
        }
    }

    Ok(HermesUninstallResult {
        plugin_dir,
        config_path,
        removed_plugin_dir,
        updated_config,
    })
}

pub(crate) fn install_qodercli() -> io::Result<QodercliInstallPaths> {
    let dir = qodercli_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "qodercli config directory not found at {}. install qodercli first",
            dir.display()
        )));
    }

    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join(QODERCLI_HOOK_INSTALL_NAME);
    fs::write(&hook_path, QODERCLI_HOOK_ASSET)?;
    make_executable(&hook_path)?;

    // Register the hook in ~/.qoder/settings.json. The schema mirrors claude
    // settings.json (per https://docs.qoder.com/zh/cli/hooks): a top-level
    // `hooks` object keyed by event name, each entry holding a matcher + a
    // list of `{type: "command", command, timeout?}` invocations. The hook
    // script reads the event payload from stdin via `hook_event_name` so the
    // installation never depends on a `QODER_HOOK_EVENT` environment
    // variable.
    let settings_path = dir.join("settings.json");
    let mut settings = if settings_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?).map_err(|err| {
            io::Error::other(format!(
                "failed to parse {}: {err}",
                settings_path.display()
            ))
        })?
    } else {
        json!({})
    };

    let hooks = ensure_hooks_object(
        &mut settings,
        &settings_path,
        "qodercli settings",
        "qodercli settings hooks",
    )?;
    let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());

    // SubagentStop is intentionally *not* mapped to working: the hook script
    // returns early on it (mirroring assets/claude/hako-agent-state.sh) so
    // that recap/away-summary frames cannot revive an idle pane.
    ensure_command_hook(
        hooks,
        "SessionStart",
        format!("bash {quoted_hook_path} idle"),
        10,
        Some("*"),
    )?;
    ensure_command_hook(
        hooks,
        "UserPromptSubmit",
        format!("bash {quoted_hook_path} working"),
        10,
        Some("*"),
    )?;
    ensure_command_hook(
        hooks,
        "PreToolUse",
        format!("bash {quoted_hook_path} working"),
        10,
        Some("*"),
    )?;
    ensure_command_hook(
        hooks,
        "PermissionRequest",
        format!("bash {quoted_hook_path} blocked"),
        10,
        Some("*"),
    )?;
    ensure_command_hook(
        hooks,
        "Stop",
        format!("bash {quoted_hook_path} idle"),
        10,
        Some("*"),
    )?;
    ensure_command_hook(
        hooks,
        "SessionEnd",
        format!("bash {quoted_hook_path} release"),
        10,
        Some("*"),
    )?;

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(QodercliInstallPaths {
        hook_path,
        settings_path,
    })
}

pub(crate) fn install_cursor() -> io::Result<CursorInstallPaths> {
    let dir = cursor_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "cursor config directory not found at {}. install cursor agent cli first",
            dir.display()
        )));
    }
    let hook_path = dir.join(CURSOR_HOOK_INSTALL_NAME);
    fs::write(&hook_path, CURSOR_HOOK_ASSET)?;
    make_executable(&hook_path)?;
    let hooks_path = dir.join("hooks.json");
    let mut hooks_file = if hooks_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?).map_err(|err| {
            io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
        })?
    } else {
        json!({ "version": 1 })
    };
    if hooks_file.get("version").is_none() {
        hooks_file
            .as_object_mut()
            .ok_or_else(|| {
                io::Error::other(format!(
                    "cursor hooks file at {} must be a JSON object",
                    hooks_path.display()
                ))
            })?
            .insert("version".to_string(), json!(1));
    }
    let hooks = ensure_hooks_object(
        &mut hooks_file,
        &hooks_path,
        "cursor hooks file",
        "cursor hooks file hooks",
    )?;
    let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
    let session_command = format!("bash {quoted_hook_path} working");
    let working_command = format!("bash {quoted_hook_path} working");
    let release_command = format!("bash {quoted_hook_path} release");
    for event in [
        "sessionStart",
        "beforeSubmitPrompt",
        "beforeShellExecution",
        "beforeMCPExecution",
        "stop",
        "sessionEnd",
    ] {
        remove_simple_command_hook(hooks, event, &format!("bash {quoted_hook_path} session"))?;
        remove_simple_command_hook(hooks, event, &session_command)?;
        remove_simple_command_hook(hooks, event, &working_command)?;
        remove_simple_command_hook(hooks, event, &release_command)?;
    }
    ensure_simple_command_hook(hooks, "sessionStart", session_command)?;
    ensure_simple_command_hook(hooks, "beforeSubmitPrompt", working_command.clone())?;
    ensure_simple_command_hook(hooks, "beforeShellExecution", working_command.clone())?;
    ensure_simple_command_hook(hooks, "beforeMCPExecution", working_command)?;
    ensure_simple_command_hook(
        hooks,
        "stop",
        "bash ".to_string() + &quoted_hook_path + " idle",
    )?;
    ensure_simple_command_hook(hooks, "sessionEnd", release_command)?;
    fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
    Ok(CursorInstallPaths {
        hook_path,
        hooks_path,
    })
}

pub(crate) fn uninstall_qodercli() -> io::Result<QodercliUninstallResult> {
    let hook_path = qodercli_dir()?
        .join("hooks")
        .join(QODERCLI_HOOK_INSTALL_NAME);
    let settings_path = qodercli_dir()?.join("settings.json");
    let mut updated_settings = false;

    if settings_path.is_file() {
        let mut settings = serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?)
            .map_err(|err| {
                io::Error::other(format!(
                    "failed to parse {}: {err}",
                    settings_path.display()
                ))
            })?;

        if let Some(hooks) = hooks_object_if_present(
            &mut settings,
            &settings_path,
            "qodercli settings",
            "qodercli settings hooks",
        )? {
            let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
            updated_settings |= remove_command_hook(
                hooks,
                "SessionStart",
                &format!("bash {quoted_hook_path} idle"),
            )?;
            updated_settings |= remove_command_hook(
                hooks,
                "UserPromptSubmit",
                &format!("bash {quoted_hook_path} working"),
            )?;
            updated_settings |= remove_command_hook(
                hooks,
                "PreToolUse",
                &format!("bash {quoted_hook_path} working"),
            )?;
            updated_settings |= remove_command_hook(
                hooks,
                "PermissionRequest",
                &format!("bash {quoted_hook_path} blocked"),
            )?;
            updated_settings |=
                remove_command_hook(hooks, "Stop", &format!("bash {quoted_hook_path} idle"))?;
            updated_settings |= remove_command_hook(
                hooks,
                "SessionEnd",
                &format!("bash {quoted_hook_path} release"),
            )?;
        }

        if updated_settings {
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }

    let removed_hook_file = remove_file_if_exists(&hook_path)?;

    Ok(QodercliUninstallResult {
        hook_path,
        settings_path,
        removed_hook_file,
        updated_settings,
    })
}

pub(crate) fn uninstall_devin() -> io::Result<DevinUninstallResult> {
    let devin_home = devin_dir()?;
    let hook_path = devin_home.join(DEVIN_HOOK_INSTALL_NAME);
    let settings_path = devin_home.join("config.json");
    let mut updated_settings = false;

    if settings_path.is_file() {
        let mut settings = serde_json::from_str::<Value>(&fs::read_to_string(&settings_path)?)
            .map_err(|err| {
                io::Error::other(format!(
                    "failed to parse {}: {err}",
                    settings_path.display()
                ))
            })?;

        if let Some(hooks) = hooks_object_if_present(
            &mut settings,
            &settings_path,
            "devin settings",
            "devin settings hooks",
        )? {
            for (event, action) in DEVIN_REMOVED_LIFECYCLE_HOOK_EVENTS {
                updated_settings |=
                    remove_command_hook(hooks, event, &hook_command(&hook_path, Some(action)))?;
            }
            for (event, action) in DEVIN_HOOK_EVENTS {
                updated_settings |=
                    remove_command_hook(hooks, event, &hook_command(&hook_path, Some(action)))?;
            }
        }

        if updated_settings {
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }

    let removed_hook_file = remove_matching_integration_file(&hook_path, "devin")?;

    Ok(DevinUninstallResult {
        hook_path,
        settings_path,
        removed_hook_file,
        updated_settings,
    })
}
pub(crate) fn uninstall_cursor() -> io::Result<CursorUninstallResult> {
    let cursor_home = cursor_dir()?;
    let hook_path = cursor_home.join(CURSOR_HOOK_INSTALL_NAME);
    let hooks_path = cursor_home.join("hooks.json");
    let mut updated_hooks = false;
    if hooks_path.is_file() {
        let mut hooks_file = serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?)
            .map_err(|err| {
                io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
            })?;
        if let Some(hooks) = hooks_object_if_present(
            &mut hooks_file,
            &hooks_path,
            "cursor hooks file",
            "cursor hooks file hooks",
        )? {
            let quoted_hook_path = shell_single_quote(&hook_path.display().to_string());
            let session_command = format!("bash {quoted_hook_path} session");
            updated_hooks |= remove_simple_command_hook(hooks, "sessionStart", &session_command)?;
            updated_hooks |=
                remove_simple_command_hook(hooks, "beforeSubmitPrompt", &session_command)?;
            updated_hooks |=
                remove_simple_command_hook(hooks, "beforeShellExecution", &session_command)?;
            updated_hooks |=
                remove_simple_command_hook(hooks, "beforeMCPExecution", &session_command)?;
            updated_hooks |= remove_simple_command_hook(hooks, "stop", &session_command)?;
            updated_hooks |= remove_simple_command_hook(hooks, "sessionEnd", &session_command)?;
        }
        if updated_hooks {
            fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
        }
    }
    let removed_hook_file = remove_matching_integration_file(&hook_path, "cursor")?;
    Ok(CursorUninstallResult {
        hook_path,
        hooks_path,
        removed_hook_file,
        updated_hooks,
    })
}

fn ensure_hooks_object<'a>(
    settings: &'a mut Value,
    settings_path: &Path,
    root_description: &str,
    hooks_description: &str,
) -> io::Result<&'a mut Map<String, Value>> {
    let root = settings.as_object_mut().ok_or_else(|| {
        io::Error::other(format!(
            "{root_description} at {} must be a JSON object",
            settings_path.display()
        ))
    })?;

    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    hooks.as_object_mut().ok_or_else(|| {
        io::Error::other(format!(
            "{hooks_description} at {} must be a JSON object",
            settings_path.display()
        ))
    })
}

fn hooks_object_if_present<'a>(
    settings: &'a mut Value,
    settings_path: &Path,
    root_description: &str,
    hooks_description: &str,
) -> io::Result<Option<&'a mut Map<String, Value>>> {
    let root = settings.as_object_mut().ok_or_else(|| {
        io::Error::other(format!(
            "{root_description} at {} must be a JSON object",
            settings_path.display()
        ))
    })?;

    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(None);
    };

    hooks.as_object_mut().map(Some).ok_or_else(|| {
        io::Error::other(format!(
            "{hooks_description} at {} must be a JSON object",
            settings_path.display()
        ))
    })
}

fn ensure_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: String,
    timeout: u64,
    matcher: Option<&str>,
) -> io::Result<()> {
    let entries = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;

    let already_installed = entries.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hook_entries| {
                hook_entries.iter().any(|hook| {
                    hook.get("type").and_then(Value::as_str) == Some("command")
                        && hook.get("command").and_then(Value::as_str) == Some(command.as_str())
                })
            })
    });
    if already_installed {
        return Ok(());
    }

    let mut entry = Map::new();
    if let Some(matcher) = matcher {
        entry.insert("matcher".to_string(), Value::String(matcher.to_string()));
    }
    entry.insert(
        "hooks".to_string(),
        json!([
            {
                "type": "command",
                "command": command,
                "timeout": timeout,
            }
        ]),
    );

    entries.push(Value::Object(entry));
    Ok(())
}

fn ensure_direct_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: String,
    timeout_sec: u64,
    matcher: Option<&str>,
) -> io::Result<()> {
    let entries = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;

    if let Some(entry) = entries.iter_mut().find(|entry| {
        entry.get("type").and_then(Value::as_str) == Some("command")
            && entry.get("command").and_then(Value::as_str) == Some(command.as_str())
    }) {
        let Some(entry_object) = entry.as_object_mut() else {
            return Ok(());
        };
        entry_object.insert("timeoutSec".to_string(), Value::Number(timeout_sec.into()));
        match matcher {
            Some(matcher) => {
                entry_object.insert("matcher".to_string(), Value::String(matcher.to_string()));
            }
            None => {
                entry_object.remove("matcher");
            }
        }
        return Ok(());
    }

    let mut entry = Map::new();
    entry.insert("type".to_string(), Value::String("command".to_string()));
    if let Some(matcher) = matcher {
        entry.insert("matcher".to_string(), Value::String(matcher.to_string()));
    }
    entry.insert("command".to_string(), Value::String(command));
    entry.insert("timeoutSec".to_string(), Value::Number(timeout_sec.into()));
    entries.push(Value::Object(entry));
    Ok(())
}

fn remove_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: &str,
) -> io::Result<bool> {
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(false);
    };

    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;

    let mut removed = false;
    entries.retain_mut(|entry| {
        let Some(entry_object) = entry.as_object_mut() else {
            return true;
        };
        let Some(hook_entries) = entry_object.get_mut("hooks") else {
            return true;
        };
        let Some(hook_entries) = hook_entries.as_array_mut() else {
            return true;
        };

        let before = hook_entries.len();
        hook_entries.retain(|hook| !is_matching_command_hook(hook, command));
        if hook_entries.len() != before {
            removed = true;
        }

        !hook_entries.is_empty()
    });

    let remove_event = entries.is_empty();
    if remove_event {
        hooks.remove(event);
    }

    Ok(removed)
}

fn remove_direct_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: &str,
) -> io::Result<bool> {
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(false);
    };

    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;

    let before = entries.len();
    entries.retain(|entry| {
        !(entry.get("type").and_then(Value::as_str) == Some("command")
            && entry.get("command").and_then(Value::as_str) == Some(command))
    });
    let removed = entries.len() != before;
    if entries.is_empty() {
        hooks.remove(event);
    }
    Ok(removed)
}

fn ensure_simple_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: String,
) -> io::Result<()> {
    let entries = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;
    if entries
        .iter()
        .any(|entry| entry.get("command").and_then(Value::as_str) == Some(command.as_str()))
    {
        return Ok(());
    }
    entries.push(json!({ "command": command }));
    Ok(())
}
fn remove_simple_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: &str,
) -> io::Result<bool> {
    let Some(entries_value) = hooks.get_mut(event) else {
        return Ok(false);
    };
    let entries = entries_value
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;
    let before = entries.len();
    entries.retain(|entry| entry.get("command").and_then(Value::as_str) != Some(command));
    let removed = entries.len() != before;
    if entries.is_empty() {
        hooks.remove(event);
    }
    Ok(removed)
}
fn remove_hook_commands(
    hooks: &mut Map<String, Value>,
    event: &str,
    hook_path: &Path,
    action: Option<&str>,
) -> io::Result<bool> {
    let command = hook_command(hook_path, action);
    remove_command_hook(hooks, event, &command)
}
fn is_matching_command_hook(hook: &Value, command: &str) -> bool {
    hook.get("type").and_then(Value::as_str) == Some("command")
        && hook.get("command").and_then(Value::as_str) == Some(command)
}

fn remove_matching_integration_file(path: &Path, expected_id: &str) -> io::Result<bool> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    if parse_integration_id(&content) != Some(expected_id) {
        return Ok(false);
    }
    fs::remove_file(path)?;
    Ok(true)
}

fn remove_file_if_exists(path: &Path) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn remove_dir_all_if_exists(path: &Path) -> io::Result<bool> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn ensure_hermes_plugin_enabled(content: &str) -> String {
    update_hermes_enabled_plugin(content, true)
}

fn remove_hermes_plugin_enabled(content: &str) -> String {
    update_hermes_enabled_plugin(content, false)
}

fn update_hermes_enabled_plugin(content: &str, enabled: bool) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let Some(plugins_index) = top_level_yaml_key_index(&lines, "plugins") else {
        if !enabled {
            return content.to_string();
        }
        let mut result = content.trim_end_matches('\n').to_string();
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("plugins:\n  enabled:\n    - hako-agent-state\n");
        return result;
    };

    let plugins_end =
        next_top_level_yaml_key_index(&lines, plugins_index + 1).unwrap_or(lines.len());
    let plugins_inline_items = yaml_key_value_at_indent(&lines[plugins_index], 0, "plugins")
        .and_then(yaml_flow_sequence_items);
    let enabled_index = lines[plugins_index + 1..plugins_end]
        .iter()
        .position(|line| yaml_key_at_indent(line, 2) == Some("enabled"))
        .map(|offset| plugins_index + 1 + offset);
    let flat_list_start = lines[plugins_index + 1..plugins_end]
        .iter()
        .position(|line| yaml_list_item_value_at_indent(line, 2).is_some())
        .map(|offset| plugins_index + 1 + offset);

    if let Some(enabled_index) = enabled_index {
        let line = lines[enabled_index].trim();
        if line == "enabled: []" || line == "enabled: [] # hako" {
            if enabled {
                lines[enabled_index] = "  enabled:".to_string();
                lines.insert(enabled_index + 1, "    - hako-agent-state".to_string());
            }
            return join_yaml_lines(lines, trailing_newline);
        }

        let list_start = enabled_index + 1;
        let list_end = lines[list_start..plugins_end]
            .iter()
            .position(|line| {
                yaml_indent(line).is_some_and(|indent| indent <= 2) && yaml_key_name(line).is_some()
            })
            .map(|offset| list_start + offset)
            .unwrap_or(plugins_end);
        let existing_item_index = lines[list_start..list_end]
            .iter()
            .position(|line| yaml_list_item_matches(line, HERMES_PLUGIN_INSTALL_NAME))
            .map(|offset| list_start + offset);

        match (enabled, existing_item_index) {
            (true, Some(_)) | (false, None) => return content.to_string(),
            (true, None) => lines.insert(list_start, "    - hako-agent-state".to_string()),
            (false, Some(index)) => {
                lines.remove(index);
            }
        }
        return join_yaml_lines(lines, trailing_newline);
    }

    if let Some(mut items) = plugins_inline_items {
        let existing_item_index = items
            .iter()
            .position(|item| item == HERMES_PLUGIN_INSTALL_NAME);
        match (enabled, existing_item_index) {
            (true, Some(_)) | (false, None) => return content.to_string(),
            (true, None) => items.insert(0, HERMES_PLUGIN_INSTALL_NAME.to_string()),
            (false, Some(index)) => {
                items.remove(index);
            }
        }
        let replacement = hermes_flat_plugin_lines(&items);
        lines.splice(plugins_index..plugins_end, replacement);
        return join_yaml_lines(lines, trailing_newline);
    }

    if let Some(flat_list_start) = flat_list_start {
        let existing_item_index = lines[plugins_index + 1..plugins_end]
            .iter()
            .position(|line| yaml_list_item_matches_at_indent(line, 2, HERMES_PLUGIN_INSTALL_NAME))
            .map(|offset| plugins_index + 1 + offset);
        match (enabled, existing_item_index) {
            (true, Some(_)) | (false, None) => return content.to_string(),
            (true, None) => lines.insert(flat_list_start, "  - hako-agent-state".to_string()),
            (false, Some(index)) => {
                lines.remove(index);
            }
        }
        return join_yaml_lines(lines, trailing_newline);
    }

    if enabled {
        lines.insert(plugins_index + 1, "  enabled:".to_string());
        lines.insert(plugins_index + 2, "    - hako-agent-state".to_string());
        return join_yaml_lines(lines, trailing_newline);
    }

    content.to_string()
}

fn hermes_flat_plugin_lines(items: &[String]) -> Vec<String> {
    if items.is_empty() {
        return vec!["plugins: []".to_string()];
    }
    let mut lines = vec!["plugins:".to_string()];
    lines.extend(items.iter().map(|item| format!("  - {item}")));
    lines
}

fn top_level_yaml_key_index(lines: &[String], key: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| yaml_key_at_indent(line, 0) == Some(key))
}

fn next_top_level_yaml_key_index(lines: &[String], start: usize) -> Option<usize> {
    lines[start..]
        .iter()
        .position(|line| yaml_indent(line) == Some(0) && yaml_key_name(line).is_some())
        .map(|offset| start + offset)
}

fn yaml_key_at_indent(line: &str, indent: usize) -> Option<&str> {
    if yaml_indent(line)? != indent {
        return None;
    }
    yaml_key_name(line)
}

fn yaml_key_value_at_indent<'a>(line: &'a str, indent: usize, key: &str) -> Option<&'a str> {
    if yaml_indent(line)? != indent {
        return None;
    }
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
        return None;
    }
    let (line_key, value) = trimmed.split_once(':')?;
    (line_key.trim() == key).then_some(value.trim())
}

fn yaml_key_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
        return None;
    }
    let (key, _) = trimmed.split_once(':')?;
    let key = key.trim();
    (!key.is_empty()).then_some(key)
}

fn yaml_indent(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    Some(line.len() - trimmed.len())
}

fn yaml_list_item_value(line: &str) -> Option<&str> {
    line.trim().strip_prefix("- ").map(str::trim)
}

fn yaml_list_item_matches(line: &str, value: &str) -> bool {
    yaml_list_item_value(line).is_some_and(|item| yaml_scalar_value(item) == value)
}

fn yaml_list_item_value_at_indent(line: &str, indent: usize) -> Option<&str> {
    if yaml_indent(line)? != indent {
        return None;
    }
    yaml_list_item_value(line)
}

fn yaml_list_item_matches_at_indent(line: &str, indent: usize, value: &str) -> bool {
    yaml_list_item_value_at_indent(line, indent)
        .is_some_and(|item| yaml_scalar_value(item) == value)
}

fn yaml_flow_sequence_items(value: &str) -> Option<Vec<String>> {
    let value = strip_yaml_inline_comment(value).trim();
    let inner = value.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::new();
    for item in inner.split(',') {
        items.push(yaml_scalar_value(item));
    }
    Some(items)
}

fn yaml_scalar_value(value: &str) -> String {
    let value = strip_yaml_inline_comment(value).trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let quoted = (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'');
        if quoted {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn strip_yaml_inline_comment(value: &str) -> &str {
    let mut quote = None;
    for (index, ch) in value.char_indices() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '#') if index == 0 || value[..index].ends_with(char::is_whitespace) => {
                return value[..index].trim_end();
            }
            _ => {}
        }
    }
    value
}

fn join_yaml_lines(lines: Vec<String>, trailing_newline: bool) -> String {
    let mut result = lines.join("\n");
    if trailing_newline || result.is_empty() {
        result.push('\n');
    }
    result
}

fn build_kimi_config_with_hooks(content: &str, hook_path: &Path) -> String {
    let mut result = remove_kimi_config_block(content)
        .trim_end_matches('\n')
        .to_string();
    if !result.is_empty() {
        result.push('\n');
        result.push('\n');
    }
    result.push_str(KIMI_CONFIG_BLOCK_BEGIN);
    result.push('\n');
    for (event, action) in KIMI_HOOK_EVENTS {
        result.push_str(&kimi_hook_table(event, hook_path, action));
    }
    result.push_str(KIMI_CONFIG_BLOCK_END);
    result.push('\n');
    result
}
fn kimi_hook_table(event: &str, hook_path: &Path, action: &str) -> String {
    let command = hook_command(hook_path, Some(action));
    format!(
        "[[hooks]]\nevent = {}\ncommand = {}\ntimeout = 10\n\n",
        toml_basic_string(event),
        toml_basic_string(&command)
    )
}
fn remove_kimi_config_block(content: &str) -> String {
    let trailing_newline = content.ends_with('\n');
    let mut lines = Vec::new();
    let mut in_block = false;
    let mut removed_block = false;
    for line in content.lines() {
        if line.trim() == KIMI_CONFIG_BLOCK_BEGIN {
            in_block = true;
            removed_block = true;
            continue;
        }
        if in_block {
            if line.trim() == KIMI_CONFIG_BLOCK_END {
                in_block = false;
            }
            continue;
        }
        lines.push(line.to_string());
    }
    if !removed_block {
        return content.to_string();
    }
    let mut result = join_toml_lines(lines, trailing_newline);
    while result.ends_with("\n\n") {
        result.pop();
    }
    if result == "\n" {
        String::new()
    } else {
        result
    }
}
fn toml_basic_string(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 2);
    result.push('"');
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\t' => result.push_str("\\t"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            ch if ch <= '\u{1f}' || ch == '\u{7f}' => {
                result.push_str(&format!("\\u{:04X}", ch as u32))
            }
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}
fn build_codex_config_with_hooks(content: &str) -> String {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let trailing_newline = content.ends_with('\n');
    let mut in_top_level_features = false;
    let mut features_header_index = None;
    let mut hooks_index = None;
    let mut deprecated_hooks_indexes = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        if let Some(header) = toml_table_header(line) {
            in_top_level_features = header == "[features]";
            if in_top_level_features && features_header_index.is_none() {
                features_header_index = Some(index);
            }
            continue;
        }

        if !in_top_level_features {
            continue;
        }

        if is_toml_key(line, "codex_hooks") {
            deprecated_hooks_indexes.push(index);
        } else if is_toml_key(line, "hooks") {
            hooks_index = Some(index);
        }
    }

    if let Some(index) = hooks_index {
        lines[index] = "hooks = true".to_string();
    }

    for index in deprecated_hooks_indexes.into_iter().rev() {
        lines.remove(index);
    }

    if hooks_index.is_none() {
        if let Some(index) = features_header_index {
            lines.insert(index + 1, "hooks = true".to_string());
            return join_toml_lines(lines, trailing_newline);
        }

        let mut result = content.trim_end_matches('\n').to_string();
        if !result.is_empty() {
            result.push('\n');
            result.push('\n');
        }
        result.push_str("[features]\nhooks = true\n");
        return result;
    }

    join_toml_lines(lines, trailing_newline)
}

fn join_toml_lines(lines: Vec<String>, trailing_newline: bool) -> String {
    let mut result = lines.join("\n");
    if trailing_newline || result.is_empty() {
        result.push('\n');
    }
    result
}

fn toml_table_header(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') || !trimmed.starts_with('[') {
        return None;
    }

    let header_end = if trimmed.starts_with("[[") {
        trimmed.find("]]").map(|index| index + 2)?
    } else {
        trimmed.find(']').map(|index| index + 1)?
    };
    let header = &trimmed[..header_end];
    let rest = trimmed[header_end..].trim_start();
    if !rest.is_empty() && !rest.starts_with('#') {
        return None;
    }

    Some(header)
}

fn is_toml_key(line: &str, key: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') || !trimmed.starts_with(key) {
        return false;
    }

    trimmed[key.len()..].trim_start().starts_with('=')
}

fn hook_command(hook_path: &Path, action: Option<&str>) -> String {
    let mut command = format!(
        "bash {}",
        shell_single_quote(&hook_path.display().to_string())
    );
    if let Some(action) = action {
        command.push(' ');
        command.push_str(action);
    }
    command
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn make_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }

    #[cfg(not(unix))]
    let _ = path;

    Ok(())
}

fn pi_extension_dir() -> io::Result<PathBuf> {
    Ok(
        config_dir_from_env_or_home(PI_CODING_AGENT_DIR_ENV_VAR, &[".pi", "agent"])?
            .join("extensions"),
    )
}

fn omp_extension_dir() -> io::Result<PathBuf> {
    let agent_dir = if let Some(value) =
        std::env::var_os(PI_CODING_AGENT_DIR_ENV_VAR).filter(|value| !value.is_empty())
    {
        expand_tilde_path(PathBuf::from(value))?
    } else {
        omp_config_dir()?.join("agent")
    };

    Ok(agent_dir.join("extensions"))
}

fn omp_install_extension_dirs() -> io::Result<Vec<PathBuf>> {
    let mut explicit_extension_dirs = Vec::new();
    if std::env::var_os(PI_CODING_AGENT_DIR_ENV_VAR).is_some_and(|value| !value.is_empty()) {
        explicit_extension_dirs.push(omp_extension_dir()?);
    }

    let home = home_dir()?;
    let mut config_dirs = Vec::new();
    if std::env::var_os(OMP_CONFIG_DIR_ENV_VAR).is_some_and(|value| !value.is_empty()) {
        config_dirs.push(omp_config_dir()?);
    }
    config_dirs.push(home.join(".omp"));

    if let Ok(entries) = fs::read_dir(&home) {
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name == ".omp" || name.starts_with(".omp-") {
                config_dirs.push(path);
            }
        }
    }

    config_dirs.sort();
    config_dirs.dedup();

    let mut extension_dirs = explicit_extension_dirs;
    extension_dirs.extend(config_dirs.into_iter().filter_map(|config_dir| {
        let agent_dir = config_dir.join("agent");
        agent_dir.is_dir().then(|| agent_dir.join("extensions"))
    }));
    extension_dirs.sort();
    extension_dirs.dedup();

    if extension_dirs.is_empty() {
        Ok(vec![omp_extension_dir()?])
    } else {
        Ok(extension_dirs)
    }
}

fn omp_config_dir() -> io::Result<PathBuf> {
    let Some(value) = std::env::var_os(OMP_CONFIG_DIR_ENV_VAR).filter(|value| !value.is_empty())
    else {
        return Ok(home_dir()?.join(".omp"));
    };

    let path = expand_tilde_path(PathBuf::from(value))?;
    if path.is_relative() {
        Ok(home_dir()?.join(path))
    } else {
        Ok(path)
    }
}

fn claude_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(CLAUDE_CONFIG_DIR_ENV_VAR, &[".claude"])
}

fn codex_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(CODEX_HOME_ENV_VAR, &[".codex"])
}

fn copilot_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(COPILOT_HOME_ENV_VAR, &[".copilot"])
}
fn devin_dir() -> io::Result<PathBuf> {
    if let Some(value) =
        std::env::var_os(DEVIN_CONFIG_DIR_ENV_VAR).filter(|value| !value.is_empty())
    {
        return expand_tilde_path(PathBuf::from(value));
    }
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return expand_tilde_path(PathBuf::from(value)).map(|path| path.join("devin"));
    }
    Ok(home_dir()?.join(".config").join("devin"))
}

fn kimi_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(KIMI_CODE_HOME_ENV_VAR, &[".kimi-code"])
}
fn droid_dir() -> io::Result<PathBuf> {
    Ok(home_dir()?.join(".factory"))
}

fn config_dir_from_env_or_home(
    env_var: &str,
    home_relative_segments: &[&str],
) -> io::Result<PathBuf> {
    if let Some(value) = std::env::var_os(env_var).filter(|value| !value.is_empty()) {
        return expand_tilde_path(PathBuf::from(value));
    }

    let mut path = home_dir()?;
    for segment in home_relative_segments {
        path.push(segment);
    }
    Ok(path)
}

fn expand_tilde_path(path: PathBuf) -> io::Result<PathBuf> {
    let Some(raw) = path.to_str() else {
        return Ok(path);
    };

    if raw == "~" {
        return home_dir();
    }

    if let Some(rest) = raw
        .strip_prefix("~/")
        .or_else(|| raw.strip_prefix("~\\"))
        .or_else(|| raw.strip_prefix('~'))
    {
        return Ok(home_dir()?.join(rest));
    }

    Ok(path)
}

fn opencode_dir() -> io::Result<PathBuf> {
    Ok(home_dir()?.join(".config/opencode"))
}

fn hermes_dir() -> io::Result<PathBuf> {
    Ok(home_dir()?.join(".hermes"))
}

fn hermes_plugin_dir() -> io::Result<PathBuf> {
    Ok(hermes_dir()?
        .join("plugins")
        .join(HERMES_PLUGIN_INSTALL_NAME))
}

fn qodercli_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(QODERCLI_CONFIG_DIR_ENV_VAR, &[".qoder"])
}
fn cursor_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(CURSOR_CONFIG_DIR_ENV_VAR, &[".cursor"])
}

fn home_dir() -> io::Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| io::Error::other("HOME is not set; cannot locate home directory"))
}

#[cfg(test)]
pub(crate) fn integration_env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestEnvVar;

    #[test]
    fn extract_version_triple_parses_common_outputs() {
        assert_eq!(extract_version_triple("0.14.0"), Some((0, 14, 0)));
        assert_eq!(extract_version_triple("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(
            extract_version_triple("kimi-code 0.14.0 (linux/aarch64)"),
            Some((0, 14, 0))
        );
        assert_eq!(extract_version_triple("0.14"), Some((0, 14, 0)));
        assert_eq!(extract_version_triple("0.14.1-beta.2"), Some((0, 14, 1)));
        assert_eq!(extract_version_triple("no version here"), None);
        assert_eq!(extract_version_triple(""), None);
    }

    #[test]
    fn agent_version_requirement_only_set_for_kimi() {
        let requirement = agent_version_requirement(crate::api::schema::IntegrationTarget::Kimi)
            .expect("kimi must have a version requirement");
        assert_eq!(requirement.binary, "kimi");
        assert_eq!(requirement.min_version, KIMI_MIN_VERSION);
        assert!(agent_version_requirement(crate::api::schema::IntegrationTarget::Claude).is_none());
        assert!(agent_version_requirement(crate::api::schema::IntegrationTarget::Codex).is_none());
    }

    #[test]
    fn enforce_agent_version_warns_when_binary_missing() {
        let requirement = AgentVersionRequirement {
            label: "kimi code",
            binary: "hako-test-binary-that-does-not-exist",
            args: &["--version"],
            min_version: "0.14.0",
        };
        let warning = enforce_agent_version(&requirement)
            .expect("missing binary should not fail install")
            .expect("missing binary should warn");
        assert!(warning.starts_with(INSTALL_WARNING_PREFIX));
        assert!(warning.contains("could not run"));
        assert!(warning.contains("0.14.0"));
    }

    #[cfg(unix)]
    #[test]
    fn enforce_agent_version_rejects_old_version() {
        let requirement = AgentVersionRequirement {
            label: "kimi code",
            binary: "echo",
            args: &["0.7.0"],
            min_version: KIMI_MIN_VERSION,
        };

        let err = enforce_agent_version(&requirement).expect_err("old version should fail");
        let message = err.to_string();
        assert!(message.contains("0.7.0"));
        assert!(message.contains(KIMI_MIN_VERSION));
        assert!(message.contains("upgrade"));
    }

    #[cfg(unix)]
    #[test]
    fn enforce_agent_version_accepts_current_version() {
        let requirement = AgentVersionRequirement {
            label: "kimi code",
            binary: "echo",
            args: &[KIMI_MIN_VERSION],
            min_version: KIMI_MIN_VERSION,
        };

        assert_eq!(enforce_agent_version(&requirement).unwrap(), None);
    }

    fn clear_integration_path_env() -> [TestEnvVar; 10] {
        [
            TestEnvVar::remove(PI_CODING_AGENT_DIR_ENV_VAR),
            TestEnvVar::remove(OMP_CONFIG_DIR_ENV_VAR),
            TestEnvVar::remove(CLAUDE_CONFIG_DIR_ENV_VAR),
            TestEnvVar::remove(CODEX_HOME_ENV_VAR),
            TestEnvVar::remove(COPILOT_HOME_ENV_VAR),
            TestEnvVar::remove(DEVIN_CONFIG_DIR_ENV_VAR),
            TestEnvVar::remove(KIMI_CODE_HOME_ENV_VAR),
            TestEnvVar::remove(CURSOR_CONFIG_DIR_ENV_VAR),
            TestEnvVar::remove(QODERCLI_CONFIG_DIR_ENV_VAR),
            TestEnvVar::remove("HOME"),
        ]
    }

    fn unique_base() -> PathBuf {
        std::env::temp_dir().join(format!(
            "hako-integration-install-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    #[cfg(unix)]
    fn command_available_requires_executable_file_on_path() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let bin = base.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let _path_env = TestEnvVar::set("PATH", &bin);

        let command = bin.join("claude");
        fs::write(&command, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!command_available("claude"));

        fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(command_available("claude"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn windows_supports_only_cli_hook_integrations() {
        use crate::api::schema::IntegrationTarget;

        assert!(!integration_target_supported_for_platform(
            IntegrationTarget::Pi,
            true
        ));
        assert!(!integration_target_supported_for_platform(
            IntegrationTarget::Omp,
            true
        ));
        assert!(!integration_target_supported_for_platform(
            IntegrationTarget::Opencode,
            true
        ));
        assert!(!integration_target_supported_for_platform(
            IntegrationTarget::Hermes,
            true
        ));
        assert!(!integration_target_supported_for_platform(
            IntegrationTarget::Cursor,
            true
        ));

        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Claude,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Codex,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Copilot,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Droid,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Kimi,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Qodercli,
            true
        ));
    }

    #[test]
    fn non_windows_supports_all_integrations() {
        use crate::api::schema::IntegrationTarget;

        for target in [
            IntegrationTarget::Pi,
            IntegrationTarget::Omp,
            IntegrationTarget::Claude,
            IntegrationTarget::Codex,
            IntegrationTarget::Copilot,
            IntegrationTarget::Kimi,
            IntegrationTarget::Droid,
            IntegrationTarget::Opencode,
            IntegrationTarget::Hermes,
            IntegrationTarget::Qodercli,
            IntegrationTarget::Cursor,
        ] {
            assert!(integration_target_supported_for_platform(target, false));
        }
    }

    #[test]
    fn integration_recommendation_installs_available_or_outdated_targets() {
        let mut recommendation = IntegrationRecommendation {
            target: crate::api::schema::IntegrationTarget::Claude,
            label: "claude",
            command: "claude",
            available: false,
            path: PathBuf::from("/tmp/hako-agent-state.sh"),
            state: IntegrationStatusKind::NotInstalled,
        };
        assert!(!recommendation.needs_install());

        recommendation.available = true;
        assert!(recommendation.needs_install());

        recommendation.available = false;
        recommendation.state = IntegrationStatusKind::Outdated;
        assert!(recommendation.needs_install());

        recommendation.available = true;
        recommendation.state = IntegrationStatusKind::Current;
        assert!(!recommendation.needs_install());
    }

    #[test]
    fn install_pi_writes_embedded_asset_to_pi_extensions_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".pi/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let path = install_pi().unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert_eq!(path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));
        assert_eq!(content, PI_EXTENSION_ASSET);
        assert!(content.contains("HAKO_INTEGRATION_VERSION=3"));
        assert!(content.contains("Math.max(reportSeq + 1, Date.now() * 1000)"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_pi_uses_pi_coding_agent_dir_env() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let agent_dir = base.join("custom-pi-agent");
        let ext_dir = agent_dir.join("extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);

        let path = install_pi().unwrap();

        assert_eq!(path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_pi_expands_tilde_in_pi_coding_agent_dir_env() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join("custom-pi-agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, "~/custom-pi-agent");

        let path = install_pi().unwrap();

        assert_eq!(path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_omp_writes_embedded_asset_to_omp_extensions_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".omp/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_omp().unwrap();
        let extension_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);
        let content = fs::read_to_string(&extension_path).unwrap();

        assert_eq!(installed.extension_paths, vec![extension_path]);
        assert!(installed.removed_legacy_pi_extensions.is_empty());
        assert_eq!(content, OMP_EXTENSION_ASSET);
        assert!(content.contains("HAKO_INTEGRATION_ID=omp"));
        assert!(content.contains("HAKO_INTEGRATION_VERSION=3"));
        assert!(content.contains("agent: \"omp\""));
        assert!(!content.contains("agent: \"pi\""));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_omp_removes_legacy_shared_pi_omp_integration_from_extensions_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".omp/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let legacy_path = ext_dir.join(LEGACY_PI_OMP_EXTENSION_INSTALL_NAME);
        fs::write(&legacy_path, PI_EXTENSION_ASSET).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_omp().unwrap();
        let extension_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);

        assert_eq!(installed.extension_paths, vec![extension_path.clone()]);
        assert_eq!(installed.removed_legacy_pi_extensions, vec![legacy_path]);
        assert!(extension_path.exists());
        assert_eq!(
            fs::read_to_string(&extension_path).unwrap(),
            OMP_EXTENSION_ASSET
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_pi_and_omp_write_distinct_files_in_same_extension_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let agent_dir = base.join("shared-agent");
        let ext_dir = agent_dir.join("extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);

        let pi_path = install_pi().unwrap();
        let installed_omp = install_omp().unwrap();
        let omp_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);

        assert_eq!(pi_path, ext_dir.join(PI_EXTENSION_INSTALL_NAME));
        assert_eq!(installed_omp.extension_paths, vec![omp_path.clone()]);
        assert_eq!(fs::read_to_string(pi_path).unwrap(), PI_EXTENSION_ASSET);
        assert_eq!(fs::read_to_string(omp_path).unwrap(), OMP_EXTENSION_ASSET);
        assert!(!ext_dir.join(LEGACY_PI_OMP_EXTENSION_INSTALL_NAME).exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_omp_uses_pi_config_dir_env_for_default_agent_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join("custom-omp/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _omp_config_dir_env = TestEnvVar::set(OMP_CONFIG_DIR_ENV_VAR, "custom-omp");

        let installed = install_omp().unwrap();
        let extension_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);

        assert_eq!(installed.extension_paths, vec![extension_path]);
        assert!(installed.removed_legacy_pi_extensions.is_empty());

        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn install_omp_writes_to_all_existing_omp_profiles() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let default_agent = home.join(".omp/agent");
        let mk_agent = home.join(".omp-mk/agent");
        let frs_agent = home.join(".omp-frs/agent");
        fs::create_dir_all(&default_agent).unwrap();
        fs::create_dir_all(&mk_agent).unwrap();
        fs::create_dir_all(&frs_agent).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _omp_config_dir_env = TestEnvVar::set(OMP_CONFIG_DIR_ENV_VAR, ".omp-mk");

        let installed = install_omp().unwrap();

        let mut expected = vec![
            default_agent
                .join("extensions")
                .join(OMP_EXTENSION_INSTALL_NAME),
            frs_agent
                .join("extensions")
                .join(OMP_EXTENSION_INSTALL_NAME),
            mk_agent.join("extensions").join(OMP_EXTENSION_INSTALL_NAME),
        ];
        expected.sort();
        let mut actual = installed.extension_paths;
        actual.sort();

        assert_eq!(actual, expected);
        for path in actual {
            assert_eq!(fs::read_to_string(path).unwrap(), OMP_EXTENSION_ASSET);
        }

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_omp_includes_pi_coding_agent_dir_env_without_skipping_existing_omp_profiles() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let env_agent_dir = base.join("custom-omp-agent");
        let env_ext_dir = env_agent_dir.join("extensions");
        let home_agent_dir = home.join(".omp/agent");
        let home_ext_dir = home_agent_dir.join("extensions");
        fs::create_dir_all(&env_ext_dir).unwrap();
        fs::create_dir_all(&home_agent_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, &env_agent_dir);

        let installed = install_omp().unwrap();
        let mut actual = installed.extension_paths;
        actual.sort();
        let mut expected = vec![
            env_ext_dir.join(OMP_EXTENSION_INSTALL_NAME),
            home_ext_dir.join(OMP_EXTENSION_INSTALL_NAME),
        ];
        expected.sort();

        assert_eq!(actual, expected);
        assert!(installed.removed_legacy_pi_extensions.is_empty());
        for path in actual {
            assert_eq!(fs::read_to_string(path).unwrap(), OMP_EXTENSION_ASSET);
        }

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_omp_removes_embedded_extension_when_present() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".omp/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(
            ext_dir.join(OMP_EXTENSION_INSTALL_NAME),
            OMP_EXTENSION_ASSET,
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_omp().unwrap();
        let extension_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);

        assert_eq!(result.extension_paths, vec![extension_path.clone()]);
        assert_eq!(result.removed_extension_paths, vec![extension_path.clone()]);
        assert!(!extension_path.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_omp_errors_when_extension_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_omp().unwrap_err().to_string();

        assert!(err.contains("omp agent directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_pi_removes_embedded_extension_when_present() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".pi/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(ext_dir.join(PI_EXTENSION_INSTALL_NAME), PI_EXTENSION_ASSET).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_pi().unwrap();

        assert_eq!(
            result.extension_path,
            ext_dir.join(PI_EXTENSION_INSTALL_NAME)
        );
        assert!(result.removed_extension);
        assert!(!result.extension_path.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn outdated_integrations_treat_missing_version_marker_as_legacy() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".pi/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let extension_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
        fs::write(&extension_path, "// installed by hako\n").unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let outdated = outdated_installed_integrations();

        assert_eq!(outdated.len(), 1);
        assert_eq!(
            outdated[0].target,
            crate::api::schema::IntegrationTarget::Pi
        );
        assert_eq!(outdated[0].path, extension_path);
        assert_eq!(outdated[0].installed_version, None);
        assert_eq!(outdated[0].expected_version, PI_INTEGRATION_VERSION);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn integration_status_treats_same_version_with_stale_content_as_outdated() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".pi/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let extension_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
        fs::write(
            &extension_path,
            "// installed by hako\n// HAKO_INTEGRATION_ID=pi\n// HAKO_INTEGRATION_VERSION=1\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let outdated = outdated_installed_integrations();

        assert_eq!(outdated.len(), 1);
        assert_eq!(
            outdated[0].target,
            crate::api::schema::IntegrationTarget::Pi
        );
        assert_eq!(outdated[0].path, extension_path);
        assert_eq!(outdated[0].installed_version, Some(1));
        assert_eq!(outdated[0].expected_version, PI_INTEGRATION_VERSION);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn outdated_integrations_accept_current_version_marker() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".pi/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::write(ext_dir.join(PI_EXTENSION_INSTALL_NAME), PI_EXTENSION_ASSET).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        assert!(outdated_installed_integrations().is_empty());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn pi_and_omp_statuses_can_be_current_in_same_extension_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let agent_dir = base.join("shared-agent");
        let ext_dir = agent_dir.join("extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);
        let pi_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
        let omp_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);
        fs::write(&pi_path, PI_EXTENSION_ASSET).unwrap();
        fs::write(&omp_path, OMP_EXTENSION_ASSET).unwrap();

        let pi_status = integration_status_at(
            crate::api::schema::IntegrationTarget::Pi,
            pi_path,
            PI_INTEGRATION_VERSION,
        );
        let omp_status = integration_status_at(
            crate::api::schema::IntegrationTarget::Omp,
            omp_path,
            OMP_INTEGRATION_VERSION,
        );

        assert_eq!(pi_status.state, IntegrationStatusKind::Current);
        assert_eq!(omp_status.state, IntegrationStatusKind::Current);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_pi_does_not_remove_distinct_omp_asset() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let agent_dir = base.join("shared-agent");
        let ext_dir = agent_dir.join("extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);
        let pi_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
        let omp_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);
        fs::write(&pi_path, PI_EXTENSION_ASSET).unwrap();
        fs::write(&omp_path, OMP_EXTENSION_ASSET).unwrap();

        let result = uninstall_pi().unwrap();

        assert!(result.removed_extension);
        assert!(!pi_path.exists());
        assert_eq!(fs::read_to_string(&omp_path).unwrap(), OMP_EXTENSION_ASSET);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_omp_does_not_remove_distinct_pi_asset() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let agent_dir = base.join("shared-agent");
        let ext_dir = agent_dir.join("extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let _pi_agent_dir_env = TestEnvVar::set(PI_CODING_AGENT_DIR_ENV_VAR, &agent_dir);
        let pi_path = ext_dir.join(PI_EXTENSION_INSTALL_NAME);
        let omp_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);
        fs::write(&pi_path, PI_EXTENSION_ASSET).unwrap();
        fs::write(&omp_path, OMP_EXTENSION_ASSET).unwrap();

        let result = uninstall_omp().unwrap();

        assert_eq!(result.removed_extension_paths, vec![omp_path.clone()]);
        assert!(!omp_path.exists());
        assert_eq!(fs::read_to_string(&pi_path).unwrap(), PI_EXTENSION_ASSET);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_pi_errors_when_extension_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_pi().unwrap_err().to_string();

        assert!(err.contains("pi extension directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_claude_writes_hook_and_updates_settings() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            r#"{"permissions":{"allow":["Read"]},"hooks":{}}"#,
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_claude().unwrap();
        let hook_content = fs::read_to_string(&installed.hook_path).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&installed.settings_path).unwrap()).unwrap();

        assert_eq!(
            installed.hook_path,
            claude_dir.join("hooks").join(CLAUDE_HOOK_INSTALL_NAME)
        );
        assert_eq!(hook_content, CLAUDE_HOOK_ASSET);
        assert!(settings["permissions"]["allow"].is_array());
        assert_eq!(
            settings["hooks"]["SessionStart"].as_array().unwrap().len(),
            2
        );
        assert!(settings["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["hooks"][0]["command"]
                .as_str()
                .is_some_and(|command| command.contains(" session"))));
        assert!(settings["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["matcher"] == "compact"
                && entry["hooks"][0]["command"]
                    .as_str()
                    .is_some_and(|command| command.contains(" working"))));
        assert!(
            settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(" working")
        );
        assert!(settings["hooks"]["SubagentStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" working"));
        assert_eq!(
            settings["hooks"]["Notification"].as_array().unwrap().len(),
            2
        );
        assert!(settings["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" idle"));
        assert!(settings["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" release"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_claude_uses_claude_config_dir_env() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let claude_dir = base.join("custom-claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let _claude_config_dir_env = TestEnvVar::set(CLAUDE_CONFIG_DIR_ENV_VAR, &claude_dir);

        let installed = install_claude().unwrap();

        assert_eq!(installed.settings_path, claude_dir.join("settings.json"));
        assert_eq!(
            installed.hook_path,
            claude_dir.join("hooks").join(CLAUDE_HOOK_INSTALL_NAME)
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_claude_is_idempotent_for_hook_entries() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        install_claude().unwrap();
        install_claude().unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(
            settings["hooks"]["SessionStart"].as_array().unwrap().len(),
            2
        );
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            settings["hooks"]["SubagentStart"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            settings["hooks"]["Notification"].as_array().unwrap().len(),
            2
        );
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(settings["hooks"]["SessionEnd"].as_array().unwrap().len(), 1);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_claude_removes_deprecated_completion_hooks_and_preserves_user_hooks() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let claude_dir = home.join(".claude");
        let hooks_dir = claude_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let hook_path = hooks_dir.join(CLAUDE_HOOK_INSTALL_NAME);
        fs::write(
            claude_dir.join("settings.json"),
            format!(
                r#"{{"hooks":{{"PostToolUse":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}},{{"type":"command","command":"echo keep-post","timeout":10}}]}}],"PostToolUseFailure":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}},{{"type":"command","command":"echo keep-failure","timeout":10}}]}}],"SubagentStop":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}},{{"type":"command","command":"echo keep-subagent","timeout":10}}]}}]}}}}"#,
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
            ),
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        install_claude().unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(
            settings["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "echo keep-post"
        );
        assert_eq!(
            settings["hooks"]["PostToolUseFailure"][0]["hooks"][0]["command"],
            "echo keep-failure"
        );
        assert_eq!(
            settings["hooks"]["SubagentStop"][0]["hooks"][0]["command"],
            "echo keep-subagent"
        );
        assert!(
            settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(" working")
        );
        assert!(settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" working"));
        assert!(
            settings["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(" blocked")
        );
        assert!(settings["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" idle"));
        assert!(settings["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" release"));
        assert!(settings["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["hooks"][0]["command"]
                .as_str()
                .is_some_and(|command| command.contains(" session"))));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn claude_v1_integration_status_is_outdated() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let claude_hooks_dir = home.join(".claude").join("hooks");
        fs::create_dir_all(&claude_hooks_dir).unwrap();
        let hook_path = claude_hooks_dir.join(CLAUDE_HOOK_INSTALL_NAME);
        fs::write(
            &hook_path,
            "#!/bin/sh\n# HAKO_INTEGRATION_ID=claude\n# HAKO_INTEGRATION_VERSION=1\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let statuses = installed_integration_statuses();
        let claude = statuses
            .iter()
            .find(|status| status.target == crate::api::schema::IntegrationTarget::Claude)
            .unwrap();

        assert_eq!(claude.path, hook_path);
        assert_eq!(claude.installed_version, Some(1));
        assert_eq!(claude.expected_version, CLAUDE_INTEGRATION_VERSION);
        assert_eq!(claude.state, IntegrationStatusKind::Outdated);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn codex_v1_integration_status_is_outdated() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        let hook_path = codex_dir.join(CODEX_HOOK_INSTALL_NAME);
        fs::write(
            &hook_path,
            "#!/bin/sh\n# HAKO_INTEGRATION_ID=codex\n# HAKO_INTEGRATION_VERSION=1\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let statuses = installed_integration_statuses();
        let codex = statuses
            .iter()
            .find(|status| status.target == crate::api::schema::IntegrationTarget::Codex)
            .unwrap();

        assert_eq!(codex.path, hook_path);
        assert_eq!(codex.installed_version, Some(1));
        assert_eq!(codex.expected_version, CODEX_INTEGRATION_VERSION);
        assert_eq!(codex.state, IntegrationStatusKind::Outdated);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_claude_removes_hako_hooks_and_preserves_others() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let claude_dir = home.join(".claude");
        let hooks_dir = claude_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let hook_path = hooks_dir.join(CLAUDE_HOOK_INSTALL_NAME);
        fs::write(&hook_path, CLAUDE_HOOK_ASSET).unwrap();
        fs::write(
            claude_dir.join("settings.json"),
            format!(
                r#"{{"hooks":{{"SessionStart":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' idle","timeout":10}}]}}],"UserPromptSubmit":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}},{{"type":"command","command":"echo keep","timeout":10}}]}}],"PermissionRequest":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' blocked","timeout":10}}]}}],"PostToolUse":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}}]}}],"PostToolUseFailure":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}}]}}],"SubagentStop":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}}]}}],"Stop":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' idle","timeout":10}}]}}],"SessionEnd":[{{"matcher":"*","hooks":[{{"type":"command","command":"bash '{}' release","timeout":10}}]}}]}}}}"#,
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
            ),
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_claude().unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.json")).unwrap())
                .unwrap();

        assert!(result.removed_hook_file);
        assert!(result.updated_settings);
        assert!(!result.hook_path.exists());
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            settings["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "echo keep"
        );
        assert!(settings["hooks"].get("PermissionRequest").is_none());
        assert!(settings["hooks"].get("SessionStart").is_none());
        assert!(settings["hooks"].get("PostToolUse").is_none());
        assert!(settings["hooks"].get("PostToolUseFailure").is_none());
        assert!(settings["hooks"].get("SubagentStop").is_none());
        assert!(settings["hooks"].get("Stop").is_none());
        assert!(settings["hooks"].get("SessionEnd").is_none());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_claude_errors_when_claude_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_claude().unwrap_err().to_string();

        assert!(err.contains("claude directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_writes_hook_and_updates_hooks_and_config() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "model = \"gpt-5.4\"\n").unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_codex().unwrap();
        let hook_content = fs::read_to_string(&installed.hook_path).unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(&installed.hooks_path).unwrap()).unwrap();
        let config = fs::read_to_string(&installed.config_path).unwrap();

        assert_eq!(installed.hook_path, codex_dir.join(CODEX_HOOK_INSTALL_NAME));
        assert_eq!(installed.hooks_path, codex_dir.join("hooks.json"));
        assert_eq!(installed.config_path, codex_dir.join("config.toml"));
        assert_eq!(hook_content, CODEX_HOOK_ASSET);
        assert_eq!(hooks["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert!(hooks["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["hooks"][0]["command"]
                .as_str()
                .is_some_and(|command| command.contains(" session"))));
        assert!(hooks["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["matcher"] == "compact"
                && entry["hooks"][0]["command"]
                    .as_str()
                    .is_some_and(|command| command.contains(" working"))));
        assert!(hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" working"));
        assert!(hooks["hooks"]["SubagentStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" working"));
        assert!(
            hooks["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(" blocked")
        );
        assert!(hooks["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" idle"));
        assert!(config.contains("model = \"gpt-5.4\""));
        assert!(config.contains("[features]"));
        assert!(config.contains("hooks = true"));
        assert!(!config.contains("codex_hooks"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_uses_codex_home_env() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let codex_dir = base.join("custom-codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("config.toml"), "model = \"gpt-5.4\"\n").unwrap();
        let _codex_home_env = TestEnvVar::set(CODEX_HOME_ENV_VAR, &codex_dir);

        let installed = install_codex().unwrap();

        assert_eq!(installed.hook_path, codex_dir.join(CODEX_HOOK_INSTALL_NAME));
        assert_eq!(installed.hooks_path, codex_dir.join("hooks.json"));
        assert_eq!(installed.config_path, codex_dir.join("config.toml"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_is_idempotent_for_hook_entries_and_feature_flag() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "[features]\ncodex_hooks = false\nother = true\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        install_codex().unwrap();
        install_codex().unwrap();

        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(codex_dir.join("hooks.json")).unwrap())
                .unwrap();
        let config = fs::read_to_string(codex_dir.join("config.toml")).unwrap();

        assert_eq!(hooks["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert_eq!(
            hooks["hooks"]["UserPromptSubmit"].as_array().unwrap().len(),
            1
        );
        assert_eq!(hooks["hooks"]["SubagentStart"].as_array().unwrap().len(), 1);
        assert_eq!(
            hooks["hooks"]["PermissionRequest"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(hooks["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(config.matches("hooks = true").count(), 1);
        assert!(!config.contains("codex_hooks"));
        assert!(config.contains("other = true"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_only_migrates_top_level_feature_flags() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "profile = \"work\"\n\n[profiles.work.features]\nhooks = false\ncodex_hooks = false\n\n[features]\ncodex_hooks = true\nother = true\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        install_codex().unwrap();

        let config = fs::read_to_string(codex_dir.join("config.toml")).unwrap();

        assert!(config.contains("[profiles.work.features]\nhooks = false\ncodex_hooks = false"));
        assert!(config.contains("[features]\nhooks = true\nother = true"));

        let _ = fs::remove_dir_all(base);
    }
    #[test]
    fn uninstall_codex_removes_hako_hooks_and_leaves_config_alone() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let codex_dir = home.join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        let hook_path = codex_dir.join(CODEX_HOOK_INSTALL_NAME);
        fs::write(&hook_path, CODEX_HOOK_ASSET).unwrap();
        fs::write(
            codex_dir.join("hooks.json"),
            format!(
                r#"{{"hooks":{{"SessionStart":[{{"hooks":[{{"type":"command","command":"bash '{}' idle","timeout":10}}]}}],"UserPromptSubmit":[{{"hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}},{{"type":"command","command":"echo keep","timeout":10}}]}}],"PreToolUse":[{{"hooks":[{{"type":"command","command":"bash '{}' working","timeout":10}}]}}],"PermissionRequest":[{{"hooks":[{{"type":"command","command":"bash '{}' blocked","timeout":10}}]}}],"Stop":[{{"hooks":[{{"type":"command","command":"bash '{}' idle","timeout":10}}]}}]}}}}"#,
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
                hook_path.display(),
            ),
        )
        .unwrap();
        fs::write(
            codex_dir.join("config.toml"),
            "[features]\nhooks = true\nother = true\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_codex().unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(codex_dir.join("hooks.json")).unwrap())
                .unwrap();
        let config = fs::read_to_string(codex_dir.join("config.toml")).unwrap();

        assert!(result.removed_hook_file);
        assert!(result.updated_hooks);
        assert!(!result.hook_path.exists());
        assert!(hooks["hooks"].get("SessionStart").is_none());
        assert!(hooks["hooks"].get("PreToolUse").is_none());
        assert!(hooks["hooks"].get("PermissionRequest").is_none());
        assert!(hooks["hooks"].get("Stop").is_none());
        assert_eq!(
            hooks["hooks"]["UserPromptSubmit"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "echo keep"
        );
        assert!(config.contains("hooks = true"));
        assert!(config.contains("other = true"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_errors_when_config_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_codex().unwrap_err().to_string();

        assert!(err.contains("codex config directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_copilot_writes_hook_and_updates_settings() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let copilot_dir = home.join(".copilot");
        fs::create_dir_all(&copilot_dir).unwrap();
        fs::write(
            copilot_dir.join("settings.json"),
            r#"{"permissions":{"allow":["Read"]},"hooks":{}}"#,
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_copilot().unwrap();
        let hook_content = fs::read_to_string(&installed.hook_path).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&installed.settings_path).unwrap()).unwrap();

        assert_eq!(
            installed.hook_path,
            copilot_dir.join("hooks").join(COPILOT_HOOK_INSTALL_NAME)
        );
        assert_eq!(installed.settings_path, copilot_dir.join("settings.json"));
        assert_eq!(hook_content, COPILOT_HOOK_ASSET);
        assert!(settings["permissions"]["allow"].is_array());
        let hooks = settings["hooks"].as_object().unwrap();
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
            "agentStop",
            "SessionEnd",
            "notification",
        ] {
            let entries = hooks.get(event).and_then(Value::as_array).unwrap();
            assert_eq!(entries.len(), 1, "expected one copilot hook for {event}");
            assert_eq!(entries[0]["type"], "command");
            assert!(entries[0]["command"].as_str().unwrap().contains("bash "));
            assert_eq!(entries[0]["timeoutSec"], 10);
        }
        assert_eq!(
            settings["hooks"]["notification"][0]["matcher"],
            "permission_prompt|elicitation_dialog|agent_idle"
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_copilot_uses_copilot_home_env_and_is_idempotent() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let copilot_dir = base.join("custom-copilot");
        fs::create_dir_all(&copilot_dir).unwrap();
        let _copilot_home_env = TestEnvVar::set(COPILOT_HOME_ENV_VAR, &copilot_dir);

        install_copilot().unwrap();
        let installed = install_copilot().unwrap();

        assert_eq!(
            installed.hook_path,
            copilot_dir.join("hooks").join(COPILOT_HOOK_INSTALL_NAME)
        );
        assert_eq!(installed.settings_path, copilot_dir.join("settings.json"));
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(copilot_dir.join("settings.json")).unwrap())
                .unwrap();
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
            "agentStop",
            "SessionEnd",
            "notification",
        ] {
            assert_eq!(
                settings["hooks"][event].as_array().unwrap().len(),
                1,
                "expected one copilot hook for {event}"
            );
        }

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_copilot_removes_hako_hooks_and_preserves_others() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let copilot_dir = base.join(".copilot");
        fs::create_dir_all(&copilot_dir).unwrap();
        let _copilot_home_env = TestEnvVar::set(COPILOT_HOME_ENV_VAR, &copilot_dir);

        install_copilot().unwrap();
        let mut settings: Value =
            serde_json::from_str(&fs::read_to_string(copilot_dir.join("settings.json")).unwrap())
                .unwrap();
        settings["hooks"]["UserPromptSubmit"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "type": "command",
                "command": "echo user-defined",
                "timeoutSec": 10
            }));
        fs::write(
            copilot_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        let result = uninstall_copilot().unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(copilot_dir.join("settings.json")).unwrap())
                .unwrap();

        assert!(result.removed_hook_file);
        assert!(result.updated_settings);
        assert!(!result.hook_path.exists());
        let hooks = settings["hooks"].as_object().unwrap();
        for event in [
            "SessionStart",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "Stop",
            "agentStop",
            "SessionEnd",
            "notification",
        ] {
            assert!(hooks.get(event).is_none(), "expected {event} removed");
        }
        let remaining = hooks
            .get("UserPromptSubmit")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0]["command"], "echo user-defined");

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_copilot_errors_when_config_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let missing = base.join(".copilot");
        let _copilot_home_env = TestEnvVar::set(COPILOT_HOME_ENV_VAR, &missing);

        let err = install_copilot().unwrap_err().to_string();

        assert!(err.contains("copilot config directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_opencode_writes_plugin_to_plugins_dir() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_opencode().unwrap();
        let plugin_content = fs::read_to_string(&installed.plugin_path).unwrap();

        assert_eq!(
            installed.plugin_path,
            opencode_dir
                .join("plugins")
                .join(OPENCODE_PLUGIN_INSTALL_NAME)
        );
        assert_eq!(plugin_content, OPENCODE_PLUGIN_ASSET);
        assert!(plugin_content.contains("HAKO_INTEGRATION_VERSION=5"));
        assert!(plugin_content.contains("Math.max(reportSeq + 1, Date.now() * 1000)"));
        assert!(plugin_content.contains("pane.report_agent_session"));
        assert!(plugin_content.contains("pane.report_agent"));
        assert!(plugin_content.contains("permission.asked"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_opencode_removes_plugin_when_present() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode/plugins");
        fs::create_dir_all(&opencode_dir).unwrap();
        fs::write(
            opencode_dir.join(OPENCODE_PLUGIN_INSTALL_NAME),
            OPENCODE_PLUGIN_ASSET,
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_opencode().unwrap();

        assert!(result.removed_plugin);
        assert!(!result.plugin_path.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_opencode_errors_when_config_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_opencode().unwrap_err().to_string();

        assert!(err.contains("opencode config directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_hermes_writes_plugin_and_enables_it() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let hermes_dir = home.join(".hermes");
        fs::create_dir_all(&hermes_dir).unwrap();
        fs::write(hermes_dir.join("config.yaml"), "model:\n  provider: auto\n").unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_hermes().unwrap();
        let manifest = fs::read_to_string(
            installed
                .plugin_dir
                .join(HERMES_PLUGIN_MANIFEST_INSTALL_NAME),
        )
        .unwrap();
        let init =
            fs::read_to_string(installed.plugin_dir.join(HERMES_PLUGIN_INIT_INSTALL_NAME)).unwrap();
        let config = fs::read_to_string(&installed.config_path).unwrap();

        assert_eq!(
            installed.plugin_dir,
            hermes_dir.join("plugins").join(HERMES_PLUGIN_INSTALL_NAME)
        );
        assert_eq!(manifest, HERMES_PLUGIN_MANIFEST_ASSET);
        assert_eq!(init, HERMES_PLUGIN_INIT_ASSET);
        assert!(config.contains("plugins:\n  enabled:\n    - hako-agent-state"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_hermes_is_idempotent_for_enabled_entry() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let hermes_dir = home.join(".hermes");
        fs::create_dir_all(&hermes_dir).unwrap();
        fs::write(
            hermes_dir.join("config.yaml"),
            "plugins:\n  enabled:\n    - hako-agent-state\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        install_hermes().unwrap();
        install_hermes().unwrap();

        let config = fs::read_to_string(hermes_dir.join("config.yaml")).unwrap();
        assert_eq!(config.matches("hako-agent-state").count(), 1);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn hermes_plugin_update_preserves_flat_plugin_lists() {
        let enabled = ensure_hermes_plugin_enabled(
            "plugins: [alpha, beta]
model: auto
",
        );
        assert_eq!(
            enabled,
            "plugins:
  - hako-agent-state
  - alpha
  - beta
model: auto
"
        );
        let removed = remove_hermes_plugin_enabled(&enabled);
        assert_eq!(
            removed,
            "plugins:
  - alpha
  - beta
model: auto
"
        );

        let enabled = ensure_hermes_plugin_enabled(
            "plugins:
  - alpha
  - beta
",
        );
        assert_eq!(
            enabled,
            "plugins:
  - hako-agent-state
  - alpha
  - beta
"
        );
    }

    #[test]
    fn uninstall_hermes_removes_plugin_and_enabled_entry() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let hermes_dir = home.join(".hermes");
        let plugin_dir = hermes_dir.join("plugins").join(HERMES_PLUGIN_INSTALL_NAME);
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join(HERMES_PLUGIN_INIT_INSTALL_NAME),
            HERMES_PLUGIN_INIT_ASSET,
        )
        .unwrap();
        fs::write(
            hermes_dir.join("config.yaml"),
            "plugins:\n  enabled:\n    - other-plugin\n    - hako-agent-state\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_hermes().unwrap();
        let config = fs::read_to_string(hermes_dir.join("config.yaml")).unwrap();

        assert!(result.removed_plugin_dir);
        assert!(result.updated_config);
        assert!(!plugin_dir.exists());
        assert!(config.contains("    - other-plugin"));
        assert!(!config.contains("hako-agent-state"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_hermes_errors_when_config_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_hermes().unwrap_err().to_string();

        assert!(err.contains("hermes config directory not found"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_kimi_writes_hook_and_updates_config() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let kimi_dir = base.join(".kimi-code");
        fs::create_dir_all(&kimi_dir).unwrap();
        fs::write(kimi_dir.join("config.toml"), "model = \"kimi\"\n").unwrap();
        let _kimi_home_env = TestEnvVar::set(KIMI_CODE_HOME_ENV_VAR, &kimi_dir);

        let installed = install_kimi().unwrap();
        let config = fs::read_to_string(&installed.config_path).unwrap();

        assert_eq!(
            installed.hook_path,
            kimi_dir.join("hooks").join(KIMI_HOOK_INSTALL_NAME)
        );
        assert_eq!(
            fs::read_to_string(&installed.hook_path).unwrap(),
            KIMI_HOOK_ASSET
        );
        assert!(config.contains(KIMI_CONFIG_BLOCK_BEGIN));
        assert!(config.contains("event = \"SessionStart\""));
        assert!(config.contains("event = \"UserPromptSubmit\""));
        assert!(config.contains("event = \"PermissionRequest\""));
        assert!(config.contains("event = \"PreCompact\""));
        assert!(config.contains(" release"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_droid_writes_hook_and_updates_settings() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let factory_dir = base.join(".factory");
        fs::create_dir_all(&factory_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &base);

        let installed = install_droid().unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&installed.settings_path).unwrap()).unwrap();

        assert_eq!(
            installed.hook_path,
            factory_dir.join("hooks").join(DROID_HOOK_INSTALL_NAME)
        );
        assert_eq!(
            fs::read_to_string(&installed.hook_path).unwrap(),
            DROID_HOOK_ASSET
        );
        assert!(settings["hooks"].get("SessionStart").is_some());
        assert!(settings["hooks"].get("UserPromptSubmit").is_some());
        assert!(settings["hooks"].get("PermissionRequest").is_some());
        assert!(settings["hooks"].get("PreCompact").is_some());
        assert!(settings["hooks"].get("SessionEnd").is_some());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_cursor_writes_hook_and_updates_hooks() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let cursor_dir = base.join(".cursor");
        fs::create_dir_all(&cursor_dir).unwrap();
        let _cursor_env = TestEnvVar::set(CURSOR_CONFIG_DIR_ENV_VAR, &cursor_dir);

        let installed = install_cursor().unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(&installed.hooks_path).unwrap()).unwrap();

        assert_eq!(
            installed.hook_path,
            cursor_dir.join(CURSOR_HOOK_INSTALL_NAME)
        );
        assert_eq!(
            fs::read_to_string(&installed.hook_path).unwrap(),
            CURSOR_HOOK_ASSET
        );
        assert_eq!(hooks["version"], 1);
        assert!(hooks["hooks"]["sessionStart"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" working"));
        assert!(hooks["hooks"]["beforeSubmitPrompt"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" working"));
        assert!(hooks["hooks"]["sessionEnd"][0]["command"]
            .as_str()
            .unwrap()
            .contains(" release"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn bundled_integration_assets_report_session_refs() {
        assert!(PI_EXTENSION_ASSET.contains("agent_session_path: currentAgentSessionPath"));
        assert!(PI_EXTENSION_ASSET.contains("agent_session_id: currentAgentSessionId"));
        assert!(PI_EXTENSION_ASSET.contains("publishState(true)"));
        assert!(PI_EXTENSION_ASSET.contains("new Set<string>()"));
        assert!(PI_EXTENSION_ASSET.contains("event?.toolName === \"ask\""));
        assert!(PI_EXTENSION_ASSET.contains("tool_execution_start"));
        assert!(PI_EXTENSION_ASSET.contains("tool_execution_end"));
        assert!(OMP_EXTENSION_ASSET.contains("agent_session_path: currentAgentSessionPath"));
        assert!(OMP_EXTENSION_ASSET.contains("agent_session_id: currentAgentSessionId"));
        assert!(OMP_EXTENSION_ASSET.contains("publishState(true)"));
        assert!(OMP_EXTENSION_ASSET.contains("new Set<string>()"));
        assert!(OMP_EXTENSION_ASSET.contains("event?.toolName === \"ask\""));
        assert!(OMP_EXTENSION_ASSET.contains("tool_execution_start"));
        assert!(OMP_EXTENSION_ASSET.contains("tool_execution_end"));
        assert!(CLAUDE_HOOK_ASSET.contains("agent_session_id"));
        assert!(CODEX_HOOK_ASSET.contains("HAKO_HOOK_INPUT_FILE"));
        assert!(CODEX_HOOK_ASSET.contains("agent_session_id"));
        assert!(COPILOT_HOOK_ASSET.contains("HAKO_HOOK_INPUT_FILE"));
        assert!(COPILOT_HOOK_ASSET.contains("agent_session_id"));
        assert!(KIMI_HOOK_ASSET.contains("agent_session_id"));
        assert!(DROID_HOOK_ASSET.contains("agent_session_id"));
        assert!(CURSOR_HOOK_ASSET.contains("agent_session_id"));
        assert!(COPILOT_HOOK_ASSET.contains("notification_type"));
        assert!(COPILOT_HOOK_ASSET.contains("ask_user"));
        assert!(COPILOT_HOOK_ASSET.contains("exit_plan_mode"));
        assert!(OPENCODE_PLUGIN_ASSET.contains("properties?.sessionID"));
        assert!(OPENCODE_PLUGIN_ASSET.contains("pane.report_agent_session"));
        assert!(OPENCODE_PLUGIN_ASSET.contains("agent_session_id: sessionID"));
        assert!(!OPENCODE_PLUGIN_ASSET.contains("pane.release_agent"));
        assert!(HERMES_PLUGIN_INIT_ASSET.contains("session_id = _session_id(kwargs)"));
        assert!(HERMES_PLUGIN_INIT_ASSET.contains("agent_session_id"));
        // Qoder hook reads the event from the stdin JSON payload (per
        // https://docs.qoder.com/zh/cli/hooks). Make sure the bundled script
        // never reaches for a QODER_HOOK_EVENT environment variable.
        assert!(QODERCLI_HOOK_ASSET.contains("HAKO_HOOK_INPUT_FILE"));
        assert!(QODERCLI_HOOK_ASSET.contains("hook_event_name"));
        assert!(QODERCLI_HOOK_ASSET.contains("agent_session_id"));
        assert!(!QODERCLI_HOOK_ASSET.contains("QODER_HOOK_EVENT"));
    }

    #[test]
    fn install_qodercli_writes_hook_and_updates_settings() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let qoder_dir = base.join(".qoder");
        fs::create_dir_all(&qoder_dir).unwrap();
        fs::write(
            qoder_dir.join("settings.json"),
            r#"{"permissions":{"allow":["Read"]},"hooks":{}}"#,
        )
        .unwrap();
        let _qodercli_config_dir_env = TestEnvVar::set(QODERCLI_CONFIG_DIR_ENV_VAR, &qoder_dir);

        let installed = install_qodercli().unwrap();

        assert_eq!(
            installed.hook_path,
            qoder_dir.join("hooks").join(QODERCLI_HOOK_INSTALL_NAME)
        );
        assert_eq!(installed.settings_path, qoder_dir.join("settings.json"));
        assert!(installed.hook_path.is_file());

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&installed.settings_path).unwrap()).unwrap();
        let hooks = settings
            .get("hooks")
            .and_then(Value::as_object)
            .expect("hooks should be present");
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "Stop",
            "SessionEnd",
        ] {
            assert!(
                hooks.contains_key(event),
                "expected hooks.{event} to be registered"
            );
        }
        // Pre-existing settings keys must be preserved.
        assert!(settings.get("permissions").is_some());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_qodercli_is_idempotent_for_hook_entries() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let qoder_dir = base.join(".qoder");
        fs::create_dir_all(&qoder_dir).unwrap();
        let _qodercli_config_dir_env = TestEnvVar::set(QODERCLI_CONFIG_DIR_ENV_VAR, &qoder_dir);

        install_qodercli().unwrap();
        install_qodercli().unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(qoder_dir.join("settings.json")).unwrap())
                .unwrap();
        let hooks = settings.get("hooks").and_then(Value::as_object).unwrap();
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "Stop",
            "SessionEnd",
        ] {
            let entries = hooks.get(event).and_then(Value::as_array).unwrap();
            assert_eq!(
                entries.len(),
                1,
                "expected hooks.{event} to contain exactly one entry, got {entries:?}"
            );
        }

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_qodercli_removes_hako_hooks_and_preserves_others() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let qoder_dir = base.join(".qoder");
        fs::create_dir_all(&qoder_dir).unwrap();
        let _qodercli_config_dir_env = TestEnvVar::set(QODERCLI_CONFIG_DIR_ENV_VAR, &qoder_dir);

        install_qodercli().unwrap();
        // Inject a foreign hook entry the user might have configured by hand.
        let mut settings: Value =
            serde_json::from_str(&fs::read_to_string(qoder_dir.join("settings.json")).unwrap())
                .unwrap();
        settings["hooks"]["UserPromptSubmit"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "matcher": "*",
                "hooks": [{"type": "command", "command": "echo user-defined"}],
            }));
        fs::write(
            qoder_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();

        let result = uninstall_qodercli().unwrap();
        assert!(result.removed_hook_file);
        assert!(result.updated_settings);

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(qoder_dir.join("settings.json")).unwrap())
                .unwrap();
        let hooks = settings.get("hooks").and_then(Value::as_object).unwrap();
        let remaining = hooks
            .get("UserPromptSubmit")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(remaining.len(), 1);
        let cmd = remaining[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(cmd, "echo user-defined");

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_qodercli_errors_when_config_dir_missing() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let missing = base.join(".qoder");
        let _qodercli_config_dir_env = TestEnvVar::set(QODERCLI_CONFIG_DIR_ENV_VAR, &missing);

        let err = install_qodercli().unwrap_err().to_string();
        assert!(
            err.contains("qodercli config directory not found"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(base);
    }
}
