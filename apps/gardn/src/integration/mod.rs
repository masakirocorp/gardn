use std::fs;
use std::io;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

use portable_pty::CommandBuilder;
use serde_json::{json, Map, Value};

use crate::layout::PaneId;

mod claude_settings;
pub(crate) mod host;
mod opencode_config;

pub(crate) const GARDN_PANE_ID_ENV_VAR: &str = "GARDN_PANE_ID";
const PI_EXTENSION_INSTALL_NAME: &str = "gardn-pi-agent-state.ts";
const PI_EXTENSION_ASSET: &str = include_str!("assets/pi/gardn-agent-state.ts");
const PI_INTEGRATION_VERSION: u32 = 7;
const OMP_EXTENSION_INSTALL_NAME: &str = "gardn-omp-agent-state.ts";
const OMP_EXTENSION_ASSET: &str = include_str!("assets/omp/gardn-agent-state.ts");
const OMP_INTEGRATION_VERSION: u32 = 9;
const PI_CODING_AGENT_DIR_ENV_VAR: &str = "PI_CODING_AGENT_DIR";
const OMP_CONFIG_DIR_ENV_VAR: &str = "PI_CONFIG_DIR";
const CLAUDE_HOOK_INSTALL_NAME: &str = "gardn-agent-state.sh";
const CLAUDE_HOOK_ASSET: &str = include_str!("assets/claude/gardn-agent-state.sh");
const CLAUDE_INTEGRATION_VERSION: u32 = 4;
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
const CODEX_HOOK_INSTALL_NAME: &str = "gardn-agent-state.sh";
const CODEX_HOOK_ASSET: &str = include_str!("assets/codex/gardn-agent-state.sh");
const CODEX_INTEGRATION_VERSION: u32 = 3;
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
const KIMI_HOOK_INSTALL_NAME: &str = "gardn-agent-state.sh";
const KIMI_HOOK_ASSET: &str = include_str!("assets/kimi/gardn-agent-state.sh");
const KIMI_INTEGRATION_VERSION: u32 = 3;
const KIMI_CODE_HOME_ENV_VAR: &str = "KIMI_CODE_HOME";
const KIMI_CONFIG_BLOCK_BEGIN: &str = "# >>> gardn kimi integration";
const KIMI_CONFIG_BLOCK_END: &str = "# <<< gardn kimi integration";
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
const COPILOT_HOOK_INSTALL_NAME: &str = "gardn-agent-state.sh";
const COPILOT_HOOK_ASSET: &str = include_str!("assets/copilot/gardn-agent-state.sh");
const COPILOT_INTEGRATION_VERSION: u32 = 1;
const COPILOT_HOME_ENV_VAR: &str = "COPILOT_HOME";
const DEVIN_HOOK_INSTALL_NAME: &str = if cfg!(windows) {
    "gardn-agent-state.ps1"
} else {
    "gardn-agent-state.sh"
};
const DEVIN_HOOK_ASSET: &str = if cfg!(windows) {
    include_str!("assets/devin/gardn-agent-state.ps1")
} else {
    include_str!("assets/devin/gardn-agent-state.sh")
};
const DEVIN_INTEGRATION_VERSION: u32 = 2;
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
const DROID_HOOK_INSTALL_NAME: &str = "gardn-agent-state.sh";
const DROID_HOOK_ASSET: &str = include_str!("assets/droid/gardn-agent-state.sh");
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
const OPENCODE_PLUGIN_INSTALL_NAME: &str = "gardn-agent-state.js";
const OPENCODE_PLUGIN_ASSET: &str = include_str!("assets/opencode/gardn-agent-state.js");
const OPENCODE_TUI_PLUGIN_INSTALL_NAME: &str = "gardn-tui-session.js";
const OPENCODE_TUI_PLUGIN_SPEC: &str = "./gardn-tui-session.js";
const OPENCODE_TUI_PLUGIN_ASSET: &str = include_str!("assets/opencode/gardn-tui-session.js");
const OPENCODE_INTEGRATION_VERSION: u32 = 8;
const KILO_PLUGIN_INSTALL_NAME: &str = "gardn-agent-state.js";
const KILO_PLUGIN_ASSET: &str = include_str!("assets/kilo/gardn-agent-state.js");
const KILO_INTEGRATION_VERSION: u32 = 4;
const HERMES_PLUGIN_INSTALL_NAME: &str = "gardn-agent-state";
const HERMES_PLUGIN_MANIFEST_INSTALL_NAME: &str = "plugin.yaml";
const HERMES_PLUGIN_INIT_INSTALL_NAME: &str = "__init__.py";
const HERMES_PLUGIN_MANIFEST_ASSET: &str = include_str!("assets/hermes/plugin.yaml");
const HERMES_PLUGIN_INIT_ASSET: &str = include_str!("assets/hermes/__init__.py");
const HERMES_INTEGRATION_VERSION: u32 = 2;
const QODERCLI_HOOK_INSTALL_NAME: &str = "gardn-agent-state.sh";
const QODERCLI_HOOK_ASSET: &str = include_str!("assets/qodercli/gardn-agent-state.sh");
const QODERCLI_INTEGRATION_VERSION: u32 = 1;
const QODERCLI_CONFIG_DIR_ENV_VAR: &str = "QODER_CONFIG_DIR";
const QWEN_HOOK_INSTALL_NAME: &str = if cfg!(windows) {
    "gardn-agent-session.ps1"
} else {
    "gardn-agent-session.sh"
};
const QWEN_HOOK_ASSET: &str = if cfg!(windows) {
    include_str!("assets/qwen/gardn-agent-session.ps1")
} else {
    include_str!("assets/qwen/gardn-agent-session.sh")
};
const QWEN_INTEGRATION_VERSION: u32 = 1;
const QWEN_HOOK_EVENTS: [(&str, &str); 1] = [("SessionStart", "session")];
const QWEN_HOME_ENV_VAR: &str = "QWEN_HOME";
const MASTRACODE_HOOK_INSTALL_NAME: &str = if cfg!(windows) {
    "gardn-agent-state.ps1"
} else {
    "gardn-agent-state.sh"
};
const MASTRACODE_HOOK_ASSET: &str = if cfg!(windows) {
    include_str!("assets/mastracode/gardn-agent-state.ps1")
} else {
    include_str!("assets/mastracode/gardn-agent-state.sh")
};
const MASTRACODE_INTEGRATION_VERSION: u32 = 2;
const MASTRACODE_HOOK_TIMEOUT_MS: u64 = 10_000;
const MASTRACODE_HOOK_EVENTS: [(&str, &str); 11] = [
    ("SessionStart", "session"),
    ("UserPromptSubmit", "working"),
    ("AgentStart", "working"),
    ("PreToolUse", "working"),
    ("PermissionRequest", "blocked"),
    ("PermissionResult", "working"),
    ("SubagentStart", "working"),
    ("SubagentEnd", "working"),
    ("Interrupt", "idle"),
    ("AgentEnd", "idle"),
    ("Stop", "idle"),
];
const ANTIGRAVITY_CLI_SESSION_INSTALL_NAME: &str = if cfg!(windows) {
    "gardn-agent-session.ps1"
} else {
    "gardn-agent-session.sh"
};
const ANTIGRAVITY_CLI_HOOK_ASSET: &str = if cfg!(windows) {
    include_str!("assets/antigravity_cli/gardn-agent-session.ps1")
} else {
    include_str!("assets/antigravity_cli/gardn-agent-session.sh")
};
const ANTIGRAVITY_CLI_INTEGRATION_VERSION: u32 = 2;
const ANTIGRAVITY_CLI_HOOK_BLOCK_NAME: &str = "gardn";
const ANTIGRAVITY_CLI_HOOK_TIMEOUT_SEC: u64 = 10;
const ANTIGRAVITY_CLI_HOOK_EVENTS: [(&str, &str); 1] = [("PreInvocation", "session")];
const ANTIGRAVITY_CLI_CONFIG_DIR_ENV_VAR: &str = "ANTIGRAVITY_CLI_CONFIG_DIR";
const CURSOR_HOOK_INSTALL_NAME: &str = if cfg!(windows) {
    "gardn-agent-state.ps1"
} else {
    "gardn-agent-state.sh"
};
const CURSOR_HOOK_ASSET: &str = if cfg!(windows) {
    include_str!("assets/cursor/gardn-agent-state.ps1")
} else {
    include_str!("assets/cursor/gardn-agent-state.sh")
};
const CURSOR_INTEGRATION_VERSION: u32 = 3;
const CURSOR_CONFIG_DIR_ENV_VAR: &str = "CURSOR_CONFIG_DIR";
const GROK_HOOK_INSTALL_NAME: &str = if cfg!(windows) {
    "gardn-agent-state.ps1"
} else {
    "gardn-agent-state.sh"
};
const GROK_HOOK_CONFIG_INSTALL_NAME: &str = "gardn.json";
const GROK_HOOK_ASSET: &str = if cfg!(windows) {
    include_str!("assets/grok/gardn-agent-state.ps1")
} else {
    include_str!("assets/grok/gardn-agent-state.sh")
};
const GROK_INTEGRATION_VERSION: u32 = 1;
const GROK_HOME_ENV_VAR: &str = "GROK_HOME";
const HERMES_HOME_ENV_VAR: &str = "HERMES_HOME";
const INTEGRATION_VERSION_MARKER: &str = "GARDN_INTEGRATION_VERSION=";
const INTEGRATION_ID_MARKER: &str = "GARDN_INTEGRATION_ID=";

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
    pub tui_plugin_path: PathBuf,
    pub tui_config_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct OmpInstallPaths {
    pub extension_paths: Vec<PathBuf>,
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
pub(crate) struct QwenInstallPaths {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
}
#[derive(Debug)]
pub(crate) struct KiloInstallPaths {
    pub plugin_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct MastracodeInstallPaths {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct AntigravityCliInstallPaths {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CursorInstallPaths {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct GrokInstallPaths {
    pub hook_path: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct QodercliUninstallResult {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_settings: bool,
}
#[derive(Debug)]
pub(crate) struct QwenUninstallResult {
    pub hook_path: PathBuf,
    pub settings_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_settings: bool,
}
#[derive(Debug)]
pub(crate) struct KiloUninstallResult {
    pub plugin_path: PathBuf,
    pub removed_plugin: bool,
}

#[derive(Debug)]
pub(crate) struct MastracodeUninstallResult {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_hooks: bool,
}

#[derive(Debug)]
pub(crate) struct AntigravityCliUninstallResult {
    pub hook_path: PathBuf,
    pub hooks_path: PathBuf,
    pub removed_hook_file: bool,
    pub updated_hooks: bool,
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

#[derive(Debug)]
pub(crate) struct GrokUninstallResult {
    pub hook_path: PathBuf,
    pub config_path: PathBuf,
    pub removed_hook_file: bool,
    pub removed_config_file: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntegrationStatus {
    pub target: crate::api::schema::IntegrationTarget,
    pub path: PathBuf,
    pub state: IntegrationStatusKind,
    pub installed_version: Option<u32>,
    pub expected_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
            (_, IntegrationStatusKind::Current) => "Installed",
            (_, IntegrationStatusKind::Outdated) => "Update Available",
            (true, IntegrationStatusKind::NotInstalled) => "Available",
            (false, IntegrationStatusKind::NotInstalled) => "Not Found",
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
    pub tui_plugin_path: PathBuf,
    pub tui_config_path: PathBuf,
    pub removed_plugin: bool,
    pub removed_tui_plugin: bool,
    pub updated_tui_config: bool,
}

#[derive(Debug)]
pub(crate) struct HermesUninstallResult {
    pub plugin_dir: PathBuf,
    pub config_path: PathBuf,
    pub removed_plugin_dir: bool,
    pub updated_config: bool,
}

pub(crate) fn apply_pane_base_env(cmd: &mut CommandBuilder) {
    crate::product_env::apply(
        cmd,
        crate::api::SOCKET_PATH_ENV_VAR,
        crate::api::socket_path(),
    );
    if let Ok(executable) = std::env::current_exe() {
        crate::product_env::apply(cmd, "GARDN_BIN_PATH", executable);
    }
}

pub(crate) fn apply_pane_env(cmd: &mut CommandBuilder, pane_id: PaneId) {
    apply_pane_base_env(cmd);
    crate::product_env::apply(cmd, GARDN_PANE_ID_ENV_VAR, format!("p_{}", pane_id.raw()));
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
    let output = match crate::noninteractive_process::command(requirement.binary)
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
            "{label} {}.{}.{} is too old: gardn hooks require {label} {min} or newer. upgrade {label}, then re-run install",
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

pub(crate) fn install_target_for_agent_profiles(
    target: crate::api::schema::IntegrationTarget,
    agent_profiles: &crate::agent_profiles::AgentProfileCatalog,
) -> io::Result<Vec<String>> {
    let profiles = host::ProfileIntegrationContext::from_catalog(agent_profiles);
    install_target_for_profile_contexts(target, &profiles)
}

pub(crate) fn install_target_for_profile_contexts(
    target: crate::api::schema::IntegrationTarget,
    profiles: &[host::ProfileIntegrationContext],
) -> io::Result<Vec<String>> {
    if target != crate::api::schema::IntegrationTarget::Codex {
        return install_target(target);
    }

    let result = install_codex_for_profile_contexts_inner(profiles);
    let outcome = if result.is_ok() { "ok" } else { "error" };
    crate::logging::integration_action("install", integration_target_label(target), outcome);
    result
}

pub(crate) fn uninstall_target_for_agent_profiles(
    target: crate::api::schema::IntegrationTarget,
    agent_profiles: &crate::agent_profiles::AgentProfileCatalog,
) -> io::Result<Vec<String>> {
    let profiles = host::ProfileIntegrationContext::from_catalog(agent_profiles);
    uninstall_target_for_profile_contexts(target, &profiles)
}

pub(crate) fn uninstall_target_for_profile_contexts(
    target: crate::api::schema::IntegrationTarget,
    profiles: &[host::ProfileIntegrationContext],
) -> io::Result<Vec<String>> {
    if target != crate::api::schema::IntegrationTarget::Codex {
        return uninstall_target(target);
    }

    let result = uninstall_codex_for_profile_contexts_inner(profiles);
    let outcome = if result.is_ok() { "ok" } else { "error" };
    crate::logging::integration_action("uninstall", integration_target_label(target), outcome);
    result
}

fn codex_home_dir_for_profile(
    profile: &crate::agent_profiles::AgentProfile,
) -> io::Result<Option<PathBuf>> {
    let codex_home = profile
        .env
        .iter()
        .find_map(|(key, value)| (key == CODEX_HOME_ENV_VAR).then_some(value.as_str()));
    let command_name = profile.argv.first().map(|command| {
        Path::new(command)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(command)
    });
    resolve_codex_home_dir(codex_home, command_name)
}

pub(crate) fn agent_profile_integration_warning(
    profile: &crate::agent_profiles::AgentProfile,
) -> Option<String> {
    let target = profile.kind.integration_target()?;
    if target != crate::api::schema::IntegrationTarget::Codex {
        return None;
    }
    let dir = match codex_home_dir_for_profile(profile) {
        Ok(Some(dir)) => dir,
        Ok(None) => {
            return Some(format!(
                "{} uses `{}`, but Gardn cannot determine its Codex home. Set CODEX_HOME on this profile, or use a codex-* command name.",
                profile.name, profile.command
            ));
        }
        Err(err) => {
            return Some(format!(
                "{} uses `{}`, but Gardn could not inspect its Codex home: {err}",
                profile.name, profile.command
            ));
        }
    };
    let status = integration_status_at(
        target,
        dir.join(CODEX_HOOK_INSTALL_NAME),
        CODEX_INTEGRATION_VERSION,
    );
    match status.state {
        IntegrationStatusKind::Current => None,
        IntegrationStatusKind::NotInstalled => Some(format!(
            "{} uses {}, but Gardn's codex hook is missing for {}. Run `gardn integration install codex`, then restart the pane.",
            profile.name,
            profile.command,
            dir.display()
        )),
        IntegrationStatusKind::Outdated => Some(format!(
            "{} uses {}, but Gardn's codex hook is outdated for {}. Run `gardn integration install codex`, then restart the pane.",
            profile.name,
            profile.command,
            dir.display()
        )),
    }
}

pub(crate) fn agent_profile_integration_badge(
    profile: &crate::agent_profiles::AgentProfile,
) -> Option<&'static str> {
    let target = profile.kind.integration_target()?;
    if target != crate::api::schema::IntegrationTarget::Codex {
        return None;
    }
    let dir = match codex_home_dir_for_profile(profile) {
        Ok(Some(dir)) => dir,
        Ok(None) | Err(_) => return Some("codex home unknown"),
    };
    let status = integration_status_at(
        target,
        dir.join(CODEX_HOOK_INSTALL_NAME),
        CODEX_INTEGRATION_VERSION,
    );
    match status.state {
        IntegrationStatusKind::Current => None,
        IntegrationStatusKind::NotInstalled => Some("Hook Missing"),
        IntegrationStatusKind::Outdated => Some("Hook Outdated"),
    }
}

fn codex_home_dir_for_profile_context(
    profile: &host::ProfileIntegrationContext,
) -> io::Result<Option<PathBuf>> {
    resolve_codex_home_dir(
        profile.codex_home.as_deref(),
        profile.command_name.as_deref(),
    )
}

fn resolve_codex_home_dir(
    codex_home: Option<&str>,
    command_name: Option<&str>,
) -> io::Result<Option<PathBuf>> {
    if let Some(codex_home) = codex_home {
        return expand_tilde_path(PathBuf::from(codex_home)).map(Some);
    }
    let Some(command_name) = command_name else {
        return Ok(None);
    };
    if command_name == "codex" {
        return codex_dir().map(Some);
    }
    let Some(profile_suffix) = command_name.strip_prefix("codex-") else {
        return Ok(None);
    };
    if profile_suffix.is_empty() {
        return Ok(None);
    }
    Ok(Some(home_dir()?.join(format!(".codex-{profile_suffix}"))))
}

fn codex_dirs_for_profile_contexts(
    profiles: &[host::ProfileIntegrationContext],
) -> io::Result<Vec<PathBuf>> {
    let mut dirs = vec![codex_dir()?];
    for profile in profiles {
        if profile.kind != crate::agent_profiles::AgentKind::Codex {
            continue;
        }
        let Some(dir) = codex_home_dir_for_profile_context(profile)? else {
            continue;
        };
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    Ok(dirs)
}

fn install_codex_for_profile_contexts_inner(
    profiles: &[host::ProfileIntegrationContext],
) -> io::Result<Vec<String>> {
    if !integration_target_supported(crate::api::schema::IntegrationTarget::Codex) {
        return Err(io::Error::other(
            "codex integration is not supported on Windows",
        ));
    }

    let dirs = codex_dirs_for_profile_contexts(profiles)?;

    let mut messages = Vec::new();
    let mut skipped_missing_dirs = Vec::new();
    for dir in dirs {
        if dir.is_dir() {
            messages.extend(codex_install_messages(install_codex_at(&dir)?));
        } else {
            skipped_missing_dirs.push(dir);
        }
    }

    if messages.is_empty() {
        let dir = skipped_missing_dirs
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("<unknown>"));
        return Err(io::Error::other(format!(
            "codex config directory not found at {}. install codex first",
            dir.display()
        )));
    }

    messages.extend(skipped_missing_dirs.into_iter().map(|dir| {
        format!(
            "{INSTALL_WARNING_PREFIX} skipped missing codex config directory at {}",
            dir.display()
        )
    }));
    Ok(messages)
}

fn uninstall_codex_for_profile_contexts_inner(
    profiles: &[host::ProfileIntegrationContext],
) -> io::Result<Vec<String>> {
    if !integration_target_supported(crate::api::schema::IntegrationTarget::Codex) {
        return Err(io::Error::other(
            "codex integration is not supported on Windows",
        ));
    }

    let dirs = codex_dirs_for_profile_contexts(profiles)?;
    let mut messages = Vec::new();
    for (index, dir) in dirs.into_iter().enumerate() {
        if index > 0 && !dir.is_dir() {
            messages.push(format!(
                "{INSTALL_WARNING_PREFIX} skipped missing codex config directory at {}",
                dir.display()
            ));
            continue;
        }
        messages.extend(codex_uninstall_messages(uninstall_codex_at(&dir)?));
    }
    Ok(messages)
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
            installed
                .extension_paths
                .into_iter()
                .map(|path| format!("installed omp integration to {}", path.display()))
                .collect()
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
        crate::api::schema::IntegrationTarget::Codex => codex_install_messages(install_codex()?),
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
                    "removed legacy gardn droid hook entries from {}",
                    installed.hooks_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Opencode => {
            let installed = install_opencode()?;
            vec![
                format!(
                    "installed opencode integration plugin to {}",
                    installed.plugin_path.display()
                ),
                format!(
                    "installed opencode TUI session plugin to {}",
                    installed.tui_plugin_path.display()
                ),
                format!(
                    "enabled opencode TUI session plugin in {}",
                    installed.tui_config_path.display()
                ),
            ]
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
        crate::api::schema::IntegrationTarget::Qwen => {
            let installed = install_qwen()?;
            vec![
                format!(
                    "installed qwen integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured qwen settings at {}",
                    installed.settings_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::Kilo => {
            let installed = install_kilo()?;
            vec![format!(
                "installed kilo integration plugin to {}",
                installed.plugin_path.display()
            )]
        }
        crate::api::schema::IntegrationTarget::Mastracode => {
            let installed = install_mastracode()?;
            vec![
                format!(
                    "installed mastracode integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured mastracode hooks at {}",
                    installed.hooks_path.display()
                ),
            ]
        }
        crate::api::schema::IntegrationTarget::AntigravityCli => {
            let installed = install_antigravity_cli()?;
            vec![
                format!(
                    "installed antigravity cli integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "ensured antigravity cli hooks at {}",
                    installed.hooks_path.display()
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
        crate::api::schema::IntegrationTarget::Grok => {
            let installed = install_grok()?;
            vec![
                format!(
                    "installed grok integration hook to {}",
                    installed.hook_path.display()
                ),
                format!(
                    "installed grok hook config to {}",
                    installed.config_path.display()
                ),
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
                    "removed gardn claude hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn claude hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Codex => {
            codex_uninstall_messages(uninstall_codex()?)
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
                    "removed gardn kimi hook entries from {}",
                    result.config_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn kimi hook entries found in {}",
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
                    "removed legacy gardn droid hook entries from {}",
                    result.hooks_path.display()
                ));
            } else {
                messages.push(format!(
                    "no legacy gardn droid hook entries found in {}",
                    result.hooks_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed gardn droid hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn droid hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Opencode => {
            let result = uninstall_opencode()?;
            let mut messages = vec![if result.removed_plugin {
                format!(
                    "removed opencode integration plugin at {}",
                    result.plugin_path.display()
                )
            } else {
                format!(
                    "no opencode integration plugin found at {}",
                    result.plugin_path.display()
                )
            }];
            messages.push(if result.removed_tui_plugin {
                format!(
                    "removed opencode TUI session plugin at {}",
                    result.tui_plugin_path.display()
                )
            } else {
                format!(
                    "no opencode TUI session plugin found at {}",
                    result.tui_plugin_path.display()
                )
            });
            messages.push(if result.updated_tui_config {
                format!(
                    "removed opencode TUI session plugin from {}",
                    result.tui_config_path.display()
                )
            } else {
                format!(
                    "no opencode TUI session plugin entry found in {}",
                    result.tui_config_path.display()
                )
            });
            messages
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
                    "removed gardn copilot hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn copilot hook entries found in {}",
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
                    "removed gardn devin hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn devin hook entries found in {}",
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
                    "removed gardn qodercli hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn qodercli hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Qwen => {
            let result = uninstall_qwen()?;
            let mut messages = Vec::new();
            if result.removed_hook_file {
                messages.push(format!(
                    "removed qwen hook at {}",
                    result.hook_path.display()
                ));
            } else {
                messages.push(format!(
                    "no qwen hook found at {}",
                    result.hook_path.display()
                ));
            }
            if result.updated_settings {
                messages.push(format!(
                    "removed gardn qwen hook entries from {}",
                    result.settings_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn qwen hook entries found in {}",
                    result.settings_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Kilo => {
            let result = uninstall_kilo()?;
            vec![if result.removed_plugin {
                format!("removed kilo plugin at {}", result.plugin_path.display())
            } else {
                format!("no kilo plugin found at {}", result.plugin_path.display())
            }]
        }
        crate::api::schema::IntegrationTarget::Mastracode => {
            let result = uninstall_mastracode()?;
            vec![
                if result.removed_hook_file {
                    format!("removed mastracode hook at {}", result.hook_path.display())
                } else {
                    format!("no mastracode hook found at {}", result.hook_path.display())
                },
                if result.updated_hooks {
                    format!(
                        "removed gardn mastracode hook entries from {}",
                        result.hooks_path.display()
                    )
                } else {
                    format!(
                        "no gardn mastracode hook entries found in {}",
                        result.hooks_path.display()
                    )
                },
            ]
        }
        crate::api::schema::IntegrationTarget::AntigravityCli => {
            let result = uninstall_antigravity_cli()?;
            vec![
                if result.removed_hook_file {
                    format!(
                        "removed antigravity cli hook at {}",
                        result.hook_path.display()
                    )
                } else {
                    format!(
                        "no antigravity cli hook found at {}",
                        result.hook_path.display()
                    )
                },
                if result.updated_hooks {
                    format!(
                        "removed gardn antigravity cli hook entries from {}",
                        result.hooks_path.display()
                    )
                } else {
                    format!(
                        "no gardn antigravity cli hook entries found in {}",
                        result.hooks_path.display()
                    )
                },
            ]
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
                    "removed gardn cursor hook entries from {}",
                    result.hooks_path.display()
                ));
            } else {
                messages.push(format!(
                    "no gardn cursor hook entries found in {}",
                    result.hooks_path.display()
                ));
            }
            messages
        }
        crate::api::schema::IntegrationTarget::Grok => {
            let result = uninstall_grok()?;
            let mut messages = Vec::new();
            messages.push(if result.removed_hook_file {
                format!("removed grok hook at {}", result.hook_path.display())
            } else {
                format!("no grok hook found at {}", result.hook_path.display())
            });
            messages.push(if result.removed_config_file {
                format!(
                    "removed grok hook config at {}",
                    result.config_path.display()
                )
            } else {
                format!(
                    "no grok hook config found at {}",
                    result.config_path.display()
                )
            });
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
        crate::api::schema::IntegrationTarget::Qwen => "qwen",
        crate::api::schema::IntegrationTarget::Kilo => "kilo",
        crate::api::schema::IntegrationTarget::Mastracode => "mastracode",
        crate::api::schema::IntegrationTarget::AntigravityCli => "antigravity-cli",
        crate::api::schema::IntegrationTarget::Cursor => "cursor",
        crate::api::schema::IntegrationTarget::Grok => "grok",
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
        crate::api::schema::IntegrationTarget::Qwen => "qwen",
        crate::api::schema::IntegrationTarget::Kilo => "kilo",
        crate::api::schema::IntegrationTarget::Mastracode => "mastracode",
        crate::api::schema::IntegrationTarget::AntigravityCli => "agy",
        crate::api::schema::IntegrationTarget::Cursor => "cursor-agent",
        crate::api::schema::IntegrationTarget::Grok => "grok",
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
        crate::api::schema::IntegrationTarget::Pi
            | crate::api::schema::IntegrationTarget::Omp
            | crate::api::schema::IntegrationTarget::Claude
            | crate::api::schema::IntegrationTarget::Codex
            | crate::api::schema::IntegrationTarget::Copilot
            | crate::api::schema::IntegrationTarget::Devin
            | crate::api::schema::IntegrationTarget::Droid
            | crate::api::schema::IntegrationTarget::Kimi
            | crate::api::schema::IntegrationTarget::Opencode
            | crate::api::schema::IntegrationTarget::Hermes
            | crate::api::schema::IntegrationTarget::Qodercli
            | crate::api::schema::IntegrationTarget::Qwen
            | crate::api::schema::IntegrationTarget::Kilo
            | crate::api::schema::IntegrationTarget::Mastracode
            | crate::api::schema::IntegrationTarget::AntigravityCli
            | crate::api::schema::IntegrationTarget::Cursor
            | crate::api::schema::IntegrationTarget::Grok
    )
}

fn integration_target_available(target: crate::api::schema::IntegrationTarget) -> bool {
    if !integration_target_supported(target) {
        return false;
    }
    if target == crate::api::schema::IntegrationTarget::Hermes && hermes_install_layout_available()
    {
        return true;
    }
    if target == crate::api::schema::IntegrationTarget::Kilo {
        return command_available("kilo") || command_available("kilo-code");
    }
    command_available(integration_target_command(target))
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

pub(crate) fn missing_profile_hook_count_for_target(
    target: crate::api::schema::IntegrationTarget,
    agent_profiles: &crate::agent_profiles::AgentProfileCatalog,
) -> usize {
    if target != crate::api::schema::IntegrationTarget::Codex {
        return 0;
    }
    agent_profiles
        .profiles()
        .iter()
        .filter(|profile| codex_profile_needs_hook_install(profile))
        .count()
}

pub(crate) fn missing_profile_hook_count_for_contexts(
    target: crate::api::schema::IntegrationTarget,
    profiles: &[host::ProfileIntegrationContext],
) -> usize {
    if target != crate::api::schema::IntegrationTarget::Codex {
        return 0;
    }
    profiles
        .iter()
        .filter(|profile| codex_profile_context_needs_hook_install(profile))
        .count()
}

fn codex_profile_needs_hook_install(profile: &crate::agent_profiles::AgentProfile) -> bool {
    if !profile.available()
        || profile.kind.integration_target() != Some(crate::api::schema::IntegrationTarget::Codex)
    {
        return false;
    }

    let Ok(Some(dir)) = codex_home_dir_for_profile(profile) else {
        return false;
    };
    integration_status_at(
        crate::api::schema::IntegrationTarget::Codex,
        dir.join(CODEX_HOOK_INSTALL_NAME),
        CODEX_INTEGRATION_VERSION,
    )
    .state
        != IntegrationStatusKind::Current
}

fn codex_profile_context_needs_hook_install(profile: &host::ProfileIntegrationContext) -> bool {
    if profile.kind.integration_target() != Some(crate::api::schema::IntegrationTarget::Codex)
        || profile.command_name.is_none()
    {
        return false;
    }

    let Ok(Some(dir)) = codex_home_dir_for_profile_context(profile) else {
        return false;
    };
    integration_status_at(
        crate::api::schema::IntegrationTarget::Codex,
        dir.join(CODEX_HOOK_INSTALL_NAME),
        CODEX_INTEGRATION_VERSION,
    )
    .state
        != IntegrationStatusKind::Current
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
); 17] {
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
            crate::api::schema::IntegrationTarget::Qwen,
            qwen_dir().map(|dir| dir.join("hooks").join(QWEN_HOOK_INSTALL_NAME)),
            QWEN_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Kilo,
            kilo_dir().map(|dir| dir.join("plugin").join(KILO_PLUGIN_INSTALL_NAME)),
            KILO_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Mastracode,
            mastracode_dir().map(|dir| dir.join("hooks").join(MASTRACODE_HOOK_INSTALL_NAME)),
            MASTRACODE_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::AntigravityCli,
            antigravity_cli_dir()
                .map(|dir| dir.join("hooks").join(ANTIGRAVITY_CLI_SESSION_INSTALL_NAME)),
            ANTIGRAVITY_CLI_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Cursor,
            cursor_dir().map(|dir| dir.join(CURSOR_HOOK_INSTALL_NAME)),
            CURSOR_INTEGRATION_VERSION,
        ),
        (
            crate::api::schema::IntegrationTarget::Grok,
            grok_dir().map(|dir| dir.join("hooks").join(GROK_HOOK_INSTALL_NAME)),
            GROK_INTEGRATION_VERSION,
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
                "`gardn integration install {}`",
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
        "installed Gardn integrations need updating; {}.",
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
    let mut state = if installed_id_matches
        && installed_version == Some(expected_version)
        && installed_content_matches
    {
        IntegrationStatusKind::Current
    } else {
        IntegrationStatusKind::Outdated
    };
    if target == crate::api::schema::IntegrationTarget::Opencode
        && state == IntegrationStatusKind::Current
        && !opencode_tui_integration_is_valid(&path, expected_version)
    {
        state = IntegrationStatusKind::Outdated;
    }

    IntegrationStatus {
        target,
        path,
        state,
        installed_version,
        expected_version,
    }
}

fn opencode_tui_integration_is_valid(plugin_path: &Path, expected_version: u32) -> bool {
    let Some(config_dir) = plugin_path.parent().and_then(Path::parent) else {
        return false;
    };
    let tui_plugin_path = config_dir.join(OPENCODE_TUI_PLUGIN_INSTALL_NAME);
    let tui_plugin_current = fs::read_to_string(tui_plugin_path)
        .ok()
        .and_then(|content| parse_integration_version(&content))
        .is_some_and(|version| version >= expected_version);
    tui_plugin_current
        && opencode_config::tui_plugin_is_configured(config_dir, OPENCODE_TUI_PLUGIN_SPEC)
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
        crate::api::schema::IntegrationTarget::Qwen => QWEN_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Kilo => KILO_PLUGIN_ASSET,
        crate::api::schema::IntegrationTarget::Mastracode => MASTRACODE_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::AntigravityCli => ANTIGRAVITY_CLI_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Cursor => CURSOR_HOOK_ASSET,
        crate::api::schema::IntegrationTarget::Grok => GROK_HOOK_ASSET,
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

fn ensure_pi_extension_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if dir.parent().is_some_and(|parent| parent.is_dir()) {
        return fs::create_dir_all(dir);
    }
    Err(io::Error::other(format!(
        "pi extension directory not found at {}. install pi first",
        dir.display()
    )))
}

pub(crate) fn install_pi() -> io::Result<PathBuf> {
    let dir = pi_extension_dir()?;
    ensure_pi_extension_dir(&dir)?;
    let path = dir.join(PI_EXTENSION_INSTALL_NAME);
    fs::write(&path, PI_EXTENSION_ASSET)?;
    Ok(path)
}

pub(crate) fn install_omp() -> io::Result<OmpInstallPaths> {
    let dirs = omp_install_extension_dirs()?;
    let mut extension_paths = Vec::with_capacity(dirs.len());

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

        let extension_path = dir.join(OMP_EXTENSION_INSTALL_NAME);
        fs::write(&extension_path, OMP_EXTENSION_ASSET)?;
        extension_paths.push(extension_path);
    }

    Ok(OmpInstallPaths { extension_paths })
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
    let existing_settings = if settings_path.is_file() {
        fs::read_to_string(&settings_path)?
    } else {
        "{}".to_string()
    };
    let mut updated_settings =
        claude_settings::install(&existing_settings, &settings_path, &hook_path)?;
    updated_settings =
        ensure_gardn_claude_lifecycle_hooks(&updated_settings, &settings_path, &hook_path)?;
    if updated_settings != existing_settings {
        fs::write(&settings_path, updated_settings)?;
    }

    Ok(ClaudeInstallPaths {
        hook_path,
        settings_path,
    })
}

pub(crate) fn install_codex() -> io::Result<CodexInstallPaths> {
    let dir = codex_dir()?;
    install_codex_at(&dir)
}

fn install_codex_at(dir: &Path) -> io::Result<CodexInstallPaths> {
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

fn codex_install_messages(installed: CodexInstallPaths) -> Vec<String> {
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

fn codex_uninstall_messages(result: CodexUninstallResult) -> Vec<String> {
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
            "removed gardn codex hook entries from {}",
            result.hooks_path.display()
        ));
    } else {
        messages.push(format!(
            "no gardn codex hook entries found in {}",
            result.hooks_path.display()
        ));
    }
    messages.push(format!(
        "left codex config unchanged at {}",
        result.config_path.display()
    ));
    messages
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

    opencode_config::validate_tui_plugin_config(&dir)?;
    let plugins_dir = dir.join("plugins");
    fs::create_dir_all(&plugins_dir)?;

    let plugin_path = plugins_dir.join(OPENCODE_PLUGIN_INSTALL_NAME);
    fs::write(&plugin_path, OPENCODE_PLUGIN_ASSET)?;
    let tui_plugin_path = dir.join(OPENCODE_TUI_PLUGIN_INSTALL_NAME);
    fs::write(&tui_plugin_path, OPENCODE_TUI_PLUGIN_ASSET)?;
    let tui_config_path = opencode_config::add_tui_plugin(&dir, OPENCODE_TUI_PLUGIN_SPEC)?;

    Ok(OpenCodeInstallPaths {
        plugin_path,
        tui_plugin_path,
        tui_config_path,
    })
}
pub(crate) fn install_kilo() -> io::Result<KiloInstallPaths> {
    let dir = kilo_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "kilo config directory not found at {}. install kilo first",
            dir.display()
        )));
    }
    let plugins_dir = dir.join("plugin");
    fs::create_dir_all(&plugins_dir)?;
    let plugin_path = plugins_dir.join(KILO_PLUGIN_INSTALL_NAME);
    fs::write(&plugin_path, KILO_PLUGIN_ASSET)?;
    Ok(KiloInstallPaths { plugin_path })
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
        let existing_settings = fs::read_to_string(&settings_path)?;
        let new_settings =
            claude_settings::uninstall(&existing_settings, &settings_path, &hook_path)?;
        updated_settings = new_settings != existing_settings;
        if updated_settings {
            fs::write(&settings_path, new_settings)?;
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
    let dir = codex_dir()?;
    uninstall_codex_at(&dir)
}

fn uninstall_codex_at(dir: &Path) -> io::Result<CodexUninstallResult> {
    let hook_path = dir.join(CODEX_HOOK_INSTALL_NAME);
    let hooks_path = dir.join("hooks.json");
    let config_path = dir.join("config.toml");
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
    let dir = opencode_dir()?;
    let tui_config_path = opencode_config::tui_config_path(&dir);
    let plugin_path = dir.join("plugins").join(OPENCODE_PLUGIN_INSTALL_NAME);
    let tui_plugin_path = dir.join(OPENCODE_TUI_PLUGIN_INSTALL_NAME);
    let mut errors = Vec::new();
    let updated_tui_config = opencode_config::remove_tui_plugin(&dir, OPENCODE_TUI_PLUGIN_SPEC)
        .unwrap_or_else(|err| {
            errors.push(err.to_string());
            false
        });
    let removed_plugin = remove_file_if_exists(&plugin_path).unwrap_or_else(|err| {
        errors.push(format!("failed to remove {}: {err}", plugin_path.display()));
        false
    });
    let removed_tui_plugin = remove_file_if_exists(&tui_plugin_path).unwrap_or_else(|err| {
        errors.push(format!(
            "failed to remove {}: {err}",
            tui_plugin_path.display()
        ));
        false
    });
    if !errors.is_empty() {
        return Err(io::Error::other(errors.join("; ")));
    }

    Ok(OpenCodeUninstallResult {
        plugin_path,
        tui_plugin_path,
        tui_config_path,
        removed_plugin,
        removed_tui_plugin,
        updated_tui_config,
    })
}
pub(crate) fn uninstall_kilo() -> io::Result<KiloUninstallResult> {
    let plugin_path = kilo_dir()?.join("plugin").join(KILO_PLUGIN_INSTALL_NAME);
    let removed_plugin = remove_file_if_exists(&plugin_path)?;
    Ok(KiloUninstallResult {
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
    // returns early on it (mirroring assets/claude/gardn-agent-state.sh) so
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
pub(crate) fn install_qwen() -> io::Result<QwenInstallPaths> {
    let dir = qwen_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "qwen code config directory not found at {}. install qwen code first",
            dir.display()
        )));
    }

    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join(QWEN_HOOK_INSTALL_NAME);
    fs::write(&hook_path, QWEN_HOOK_ASSET)?;
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
        "qwen settings",
        "qwen settings hooks",
    )?;
    for (event, action) in QWEN_HOOK_EVENTS {
        remove_hook_commands(hooks, event, &hook_path, Some(action))?;
        ensure_command_hook(
            hooks,
            event,
            hook_command(&hook_path, Some(action)),
            10_000,
            Some("*"),
        )?;
    }

    fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;

    Ok(QwenInstallPaths {
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
    let session_command = hook_command(&hook_path, Some("working"));
    let working_command = hook_command(&hook_path, Some("working"));
    let idle_command = hook_command(&hook_path, Some("idle"));
    let release_command = hook_command(&hook_path, Some("release"));
    for event in [
        "sessionStart",
        "beforeSubmitPrompt",
        "beforeShellExecution",
        "beforeMCPExecution",
        "stop",
        "sessionEnd",
    ] {
        for command in hook_command_variants(&hook_path, Some("session")) {
            remove_simple_command_hook(hooks, event, &command)?;
        }
        remove_simple_command_hook(hooks, event, &session_command)?;
        remove_simple_command_hook(hooks, event, &working_command)?;
        remove_simple_command_hook(hooks, event, &idle_command)?;
        remove_simple_command_hook(hooks, event, &release_command)?;
    }
    ensure_simple_command_hook(hooks, "sessionStart", session_command)?;
    ensure_simple_command_hook(hooks, "beforeSubmitPrompt", working_command.clone())?;
    ensure_simple_command_hook(hooks, "beforeShellExecution", working_command.clone())?;
    ensure_simple_command_hook(hooks, "beforeMCPExecution", working_command)?;
    ensure_simple_command_hook(hooks, "stop", idle_command)?;
    ensure_simple_command_hook(hooks, "sessionEnd", release_command)?;
    fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
    Ok(CursorInstallPaths {
        hook_path,
        hooks_path,
    })
}

pub(crate) fn install_grok() -> io::Result<GrokInstallPaths> {
    let dir = grok_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "grok config directory not found at {}. install grok build first",
            dir.display()
        )));
    }

    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(GROK_HOOK_INSTALL_NAME);
    fs::write(&hook_path, GROK_HOOK_ASSET)?;
    make_executable(&hook_path)?;

    let command = |action: &str| {
        json!({
            "type": "command",
            "command": hook_command(&hook_path, Some(action)),
            "timeout": 10
        })
    };
    let group = |action: &str| json!({ "hooks": [command(action)] });
    let working = || vec![group("working")];
    let hook_config = json!({
        "description": "Gardn Grok Build lifecycle integration",
        "hooks": {
            "SessionStart": [{
                "hooks": [command("session"), command("idle")]
            }],
            "UserPromptSubmit": working(),
            "SubagentStart": working(),
            "PreCompact": working(),
            "PostCompact": working(),
            "PreToolUse": working(),
            "PostToolUse": working(),
            "PostToolUseFailure": working(),
            "PermissionDenied": working(),
            "Notification": [
                {
                    "matcher": "permission_prompt|elicitation_dialog",
                    "hooks": [command("blocked")]
                },
                {
                    "matcher": "idle_prompt",
                    "hooks": [command("idle")]
                }
            ],
            "Stop": [group("idle")],
            "StopFailure": [group("idle")],
            "SessionEnd": [group("release")]
        }
    });
    let config_path = hooks_dir.join(GROK_HOOK_CONFIG_INSTALL_NAME);
    fs::write(&config_path, serde_json::to_string_pretty(&hook_config)?)?;

    Ok(GrokInstallPaths {
        hook_path,
        config_path,
    })
}

pub(crate) fn uninstall_grok() -> io::Result<GrokUninstallResult> {
    let hooks_dir = grok_dir()?.join("hooks");
    let hook_path = hooks_dir.join(GROK_HOOK_INSTALL_NAME);
    let config_path = hooks_dir.join(GROK_HOOK_CONFIG_INSTALL_NAME);
    let removed_hook_file = remove_file_if_exists(&hook_path)?;
    let removed_config_file = remove_file_if_exists(&config_path)?;
    Ok(GrokUninstallResult {
        hook_path,
        config_path,
        removed_hook_file,
        removed_config_file,
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
pub(crate) fn uninstall_qwen() -> io::Result<QwenUninstallResult> {
    let hook_path = qwen_dir()?.join("hooks").join(QWEN_HOOK_INSTALL_NAME);
    let settings_path = qwen_dir()?.join("settings.json");
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
            "qwen settings",
            "qwen settings hooks",
        )? {
            for (event, action) in QWEN_HOOK_EVENTS {
                updated_settings |= remove_hook_commands(hooks, event, &hook_path, Some(action))?;
            }
        }

        if updated_settings {
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        }
    }

    let removed_hook_file = remove_file_if_exists(&hook_path)?;

    Ok(QwenUninstallResult {
        hook_path,
        settings_path,
        removed_hook_file,
        updated_settings,
    })
}
pub(crate) fn install_mastracode() -> io::Result<MastracodeInstallPaths> {
    let home = mastracode_dir()?;
    let hooks_dir = home.join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(MASTRACODE_HOOK_INSTALL_NAME);
    fs::write(&hook_path, MASTRACODE_HOOK_ASSET)?;
    make_executable(&hook_path)?;
    let hooks_path = home.join("hooks.json");
    let mut hooks_file = if hooks_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?).map_err(|err| {
            io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
        })?
    } else {
        json!({})
    };
    let hooks = hooks_file.as_object_mut().ok_or_else(|| {
        io::Error::other(format!(
            "mastracode hooks file at {} must be a JSON object",
            hooks_path.display()
        ))
    })?;
    for (event, action) in MASTRACODE_HOOK_EVENTS {
        let command = hook_command(&hook_path, Some(action));
        remove_flat_command_hook(hooks, event, &command)?;
        ensure_flat_command_hook(hooks, event, command, MASTRACODE_HOOK_TIMEOUT_MS)?;
    }
    fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
    Ok(MastracodeInstallPaths {
        hook_path,
        hooks_path,
    })
}

pub(crate) fn uninstall_mastracode() -> io::Result<MastracodeUninstallResult> {
    let home = mastracode_dir()?;
    let hook_path = home.join("hooks").join(MASTRACODE_HOOK_INSTALL_NAME);
    let hooks_path = home.join("hooks.json");
    let mut updated_hooks = false;
    if hooks_path.is_file() {
        let mut hooks_file = serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?)
            .map_err(|err| {
                io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
            })?;
        let hooks = hooks_file.as_object_mut().ok_or_else(|| {
            io::Error::other(format!(
                "mastracode hooks file at {} must be a JSON object",
                hooks_path.display()
            ))
        })?;
        for (event, action) in MASTRACODE_HOOK_EVENTS {
            updated_hooks |=
                remove_flat_command_hook(hooks, event, &hook_command(&hook_path, Some(action)))?;
        }
        if updated_hooks {
            fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
        }
    }
    let removed_hook_file = remove_file_if_exists(&hook_path)?;
    Ok(MastracodeUninstallResult {
        hook_path,
        hooks_path,
        removed_hook_file,
        updated_hooks,
    })
}

pub(crate) fn install_antigravity_cli() -> io::Result<AntigravityCliInstallPaths> {
    let dir = antigravity_cli_dir()?;
    if !dir.is_dir() {
        return Err(io::Error::other(format!(
            "antigravity cli config directory not found at {}. install antigravity cli first",
            dir.display()
        )));
    }
    let hooks_dir = dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;
    let hook_path = hooks_dir.join(ANTIGRAVITY_CLI_SESSION_INSTALL_NAME);
    fs::write(&hook_path, ANTIGRAVITY_CLI_HOOK_ASSET)?;
    make_executable(&hook_path)?;
    let hooks_path = dir.join("hooks.json");
    let mut hooks_file = if hooks_path.is_file() {
        serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?).map_err(|err| {
            io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
        })?
    } else {
        json!({})
    };
    let hooks = hooks_file.as_object_mut().ok_or_else(|| {
        io::Error::other(format!(
            "antigravity cli hooks file at {} must be a JSON object",
            hooks_path.display()
        ))
    })?;
    let mut block = Map::new();
    for (event, action) in ANTIGRAVITY_CLI_HOOK_EVENTS {
        block.insert(
            event.to_string(),
            json!([{
                "type": "command",
                "command": hook_command(&hook_path, Some(action)),
                "timeout": ANTIGRAVITY_CLI_HOOK_TIMEOUT_SEC,
            }]),
        );
    }
    hooks.insert(
        ANTIGRAVITY_CLI_HOOK_BLOCK_NAME.to_string(),
        Value::Object(block),
    );
    fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
    Ok(AntigravityCliInstallPaths {
        hook_path,
        hooks_path,
    })
}

pub(crate) fn uninstall_antigravity_cli() -> io::Result<AntigravityCliUninstallResult> {
    let dir = antigravity_cli_dir()?;
    let hook_path = dir.join("hooks").join(ANTIGRAVITY_CLI_SESSION_INSTALL_NAME);
    let hooks_path = dir.join("hooks.json");
    let mut updated_hooks = false;
    if hooks_path.is_file() {
        let mut hooks_file = serde_json::from_str::<Value>(&fs::read_to_string(&hooks_path)?)
            .map_err(|err| {
                io::Error::other(format!("failed to parse {}: {err}", hooks_path.display()))
            })?;
        let hooks = hooks_file.as_object_mut().ok_or_else(|| {
            io::Error::other(format!(
                "antigravity cli hooks file at {} must be a JSON object",
                hooks_path.display()
            ))
        })?;
        updated_hooks = hooks.remove(ANTIGRAVITY_CLI_HOOK_BLOCK_NAME).is_some();
        if updated_hooks {
            fs::write(&hooks_path, serde_json::to_string_pretty(&hooks_file)?)?;
        }
    }
    let removed_hook_file = remove_file_if_exists(&hook_path)?;
    Ok(AntigravityCliUninstallResult {
        hook_path,
        hooks_path,
        removed_hook_file,
        updated_hooks,
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
            for action in ["session", "working", "idle", "release"] {
                for command in hook_command_variants(&hook_path, Some(action)) {
                    updated_hooks |= remove_simple_command_hook(hooks, "sessionStart", &command)?;
                    updated_hooks |=
                        remove_simple_command_hook(hooks, "beforeSubmitPrompt", &command)?;
                    updated_hooks |=
                        remove_simple_command_hook(hooks, "beforeShellExecution", &command)?;
                    updated_hooks |=
                        remove_simple_command_hook(hooks, "beforeMCPExecution", &command)?;
                    updated_hooks |= remove_simple_command_hook(hooks, "stop", &command)?;
                    updated_hooks |= remove_simple_command_hook(hooks, "sessionEnd", &command)?;
                }
            }
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
fn ensure_flat_command_hook(
    hooks: &mut Map<String, Value>,
    event: &str,
    command: String,
    timeout_ms: u64,
) -> io::Result<()> {
    let entries = hooks
        .entry(event.to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| io::Error::other(format!("hook entries for {event} must be an array")))?;
    if entries.iter().any(|entry| {
        entry.get("type").and_then(Value::as_str) == Some("command")
            && entry.get("command").and_then(Value::as_str) == Some(command.as_str())
    }) {
        return Ok(());
    }
    entries.push(json!({
        "type": "command",
        "command": command,
        "timeout": timeout_ms,
        "description": "Report MastraCode agent state to Gardn",
    }));
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

fn remove_flat_command_hook(
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
        result.push_str("plugins:\n  enabled:\n    - gardn-agent-state\n");
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
        if line == "enabled: []" || line == "enabled: [] # gardn" {
            if enabled {
                lines[enabled_index] = "  enabled:".to_string();
                lines.insert(enabled_index + 1, "    - gardn-agent-state".to_string());
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
            (true, None) => lines.insert(list_start, "    - gardn-agent-state".to_string()),
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
            (true, None) => lines.insert(flat_list_start, "  - gardn-agent-state".to_string()),
            (false, Some(index)) => {
                lines.remove(index);
            }
        }
        return join_yaml_lines(lines, trailing_newline);
    }

    if enabled {
        lines.insert(plugins_index + 1, "  enabled:".to_string());
        lines.insert(plugins_index + 2, "    - gardn-agent-state".to_string());
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

fn ensure_gardn_claude_lifecycle_hooks(
    content: &str,
    settings_path: &Path,
    hook_path: &Path,
) -> io::Result<String> {
    let mut settings: Value = serde_json::from_str(content).map_err(|err| {
        io::Error::other(format!(
            "failed to parse {}: {err}",
            settings_path.display()
        ))
    })?;
    let hooks = ensure_hooks_object(
        &mut settings,
        settings_path,
        "claude settings",
        "claude settings hooks",
    )?;
    for (event, action, matcher) in CLAUDE_HOOK_EVENTS {
        ensure_command_hook(
            hooks,
            event,
            hook_command(hook_path, Some(action)),
            10,
            matcher,
        )?;
    }
    ensure_command_hook(
        hooks,
        "Stop",
        hook_command(hook_path, Some("idle")),
        10,
        None,
    )?;
    ensure_command_hook(
        hooks,
        "SessionEnd",
        hook_command(hook_path, Some("release")),
        10,
        None,
    )?;
    let serialized = serde_json::to_string(&settings)?;
    let original: Value = serde_json::from_str(content).map_err(|err| {
        io::Error::other(format!(
            "failed to parse {}: {err}",
            settings_path.display()
        ))
    })?;
    if original == settings {
        Ok(content.to_string())
    } else if content.contains('\n') || content.contains('\r') {
        Ok(serde_json::to_string_pretty(&settings)?)
    } else {
        Ok(serialized)
    }
}

fn hook_command(hook_path: &Path, action: Option<&str>) -> String {
    let path = hook_path.display().to_string();
    #[cfg(windows)]
    {
        let mut command = format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -File {}",
            windows_command_quote(&path)
        );
        if let Some(action) = action {
            command.push(' ');
            command.push_str(action);
        }
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = format!("bash {}", shell_single_quote(&path));
        if let Some(action) = action {
            command.push(' ');
            command.push_str(action);
        }
        command
    }
}

fn legacy_bash_hook_command(hook_path: &Path, action: Option<&str>) -> String {
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

fn hook_command_variants(hook_path: &Path, action: Option<&str>) -> Vec<String> {
    let mut commands = vec![hook_command(hook_path, action)];
    push_unique_command(&mut commands, legacy_bash_hook_command(hook_path, action));
    #[cfg(windows)]
    {
        push_unique_command(
            &mut commands,
            legacy_bash_hook_command(&legacy_bash_hook_path(hook_path), action),
        );
    }
    commands
}

fn push_unique_command(commands: &mut Vec<String>, command: String) {
    if !commands.iter().any(|existing| existing == &command) {
        commands.push(command);
    }
}

#[cfg(windows)]
fn windows_command_quote(value: &str) -> String {
    let escaped = value.replace('"', r#"\""#);
    format!("{0}{1}{0}", '"', escaped)
}

#[cfg(windows)]
fn legacy_bash_hook_path(hook_path: &Path) -> PathBuf {
    hook_path.with_extension("sh")
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

fn kilo_dir() -> io::Result<PathBuf> {
    Ok(home_dir()?.join(".config/kilo"))
}

fn mastracode_dir() -> io::Result<PathBuf> {
    Ok(home_dir()?.join(".mastracode"))
}

fn antigravity_cli_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(ANTIGRAVITY_CLI_CONFIG_DIR_ENV_VAR, &[".gemini", "config"])
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
    if let Some(value) = std::env::var_os(HERMES_HOME_ENV_VAR).filter(|value| !value.is_empty()) {
        return expand_tilde_path(PathBuf::from(value));
    }

    #[cfg(windows)]
    {
        let explicit_home = std::env::var_os("HOME").filter(|value| !value.is_empty());
        let profile = std::env::var_os("USERPROFILE").filter(|value| !value.is_empty());
        if let Some(home) = explicit_home.filter(|home| profile.as_ref() != Some(home)) {
            return Ok(PathBuf::from(home).join(".hermes"));
        }
        if let Some(local_app_data) =
            std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())
        {
            return Ok(PathBuf::from(local_app_data).join("hermes"));
        }
    }

    Ok(home_dir()?.join(".hermes"))
}

fn hermes_install_layout_available() -> bool {
    let Ok(dir) = hermes_dir() else {
        return false;
    };
    #[cfg(windows)]
    {
        return [dir.join("hermes.exe"), dir.join("bin").join("hermes.exe")]
            .into_iter()
            .any(|path| path.is_file());
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        false
    }
}

fn hermes_plugin_dir() -> io::Result<PathBuf> {
    Ok(hermes_dir()?
        .join("plugins")
        .join(HERMES_PLUGIN_INSTALL_NAME))
}

fn qodercli_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(QODERCLI_CONFIG_DIR_ENV_VAR, &[".qoder"])
}
fn qwen_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(QWEN_HOME_ENV_VAR, &[".qwen"])
}
fn cursor_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(CURSOR_CONFIG_DIR_ENV_VAR, &[".cursor"])
}
fn grok_dir() -> io::Result<PathBuf> {
    config_dir_from_env_or_home(GROK_HOME_ENV_VAR, &[".grok"])
}

fn home_dir() -> io::Result<PathBuf> {
    if let Some(value) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    #[cfg(windows)]
    {
        if let Some(value) = std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(value));
        }
    }
    Err(io::Error::other(
        "HOME is not set; cannot locate home directory",
    ))
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
    #[cfg(unix)]
    use std::io::Write;

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
            binary: "gardn-test-binary-that-does-not-exist",
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
            binary: "/bin/echo",
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
            binary: "/bin/echo",
            args: &[KIMI_MIN_VERSION],
            min_version: KIMI_MIN_VERSION,
        };

        assert_eq!(enforce_agent_version(&requirement).unwrap(), None);
    }

    fn clear_integration_path_env() -> [TestEnvVar; 12] {
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
            TestEnvVar::remove(QWEN_HOME_ENV_VAR),
            TestEnvVar::remove(ANTIGRAVITY_CLI_CONFIG_DIR_ENV_VAR),
            TestEnvVar::remove("HOME"),
        ]
    }

    fn unique_base() -> PathBuf {
        std::env::temp_dir().join(format!(
            "gardn-integration-install-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn codex_profile_catalog_with_home(
        profile_id: &str,
        command: &str,
        codex_home: &Path,
    ) -> crate::agent_profiles::AgentProfileCatalog {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            CODEX_HOME_ENV_VAR.to_string(),
            codex_home.to_string_lossy().to_string(),
        );
        crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec![format!("user:{profile_id}")],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: profile_id.into(),
                    name: profile_id.into(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: command.into(),
                    env,
                    enabled: true,
                }],
            },
        )
    }

    fn add_non_gardn_codex_hook(codex_dir: &Path, command: &str) {
        let hooks_path = codex_dir.join("hooks.json");
        let mut hooks_file: Value =
            serde_json::from_str(&fs::read_to_string(&hooks_path).unwrap()).unwrap();
        hooks_file["hooks"]["UserPromptSubmit"][0]["hooks"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "type": "command",
                "command": command,
                "timeout": 10
            }));
        fs::write(
            &hooks_path,
            serde_json::to_string_pretty(&hooks_file).unwrap(),
        )
        .unwrap();
    }

    fn codex_hook_commands(codex_dir: &Path) -> Vec<String> {
        let hooks_file: Value =
            serde_json::from_str(&fs::read_to_string(codex_dir.join("hooks.json")).unwrap())
                .unwrap();
        let mut commands = Vec::new();
        if let Some(hooks) = hooks_file.get("hooks").and_then(Value::as_object) {
            for entries in hooks.values().filter_map(Value::as_array) {
                for entry in entries {
                    if let Some(command_hooks) = entry.get("hooks").and_then(Value::as_array) {
                        for hook in command_hooks {
                            if let Some(command) = hook.get("command").and_then(Value::as_str) {
                                commands.push(command.to_string());
                            }
                        }
                    }
                }
            }
        }
        commands
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
    fn windows_supports_javascript_integrations() {
        use crate::api::schema::IntegrationTarget;

        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Pi,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Omp,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Opencode,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Hermes,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Cursor,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Devin,
            true
        ));
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Grok,
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
        assert!(integration_target_supported_for_platform(
            IntegrationTarget::Qwen,
            true
        ));
    }

    #[test]
    #[cfg(windows)]
    fn hermes_layout_makes_target_available() {
        let _lock = integration_env_lock();
        let base = unique_base();
        let local_app_data = base.join("local-app-data");
        let hermes_bin = local_app_data.join("hermes").join("bin");
        fs::create_dir_all(&hermes_bin).unwrap();
        fs::write(hermes_bin.join("hermes.exe"), "").unwrap();
        let _hermes_home = TestEnvVar::remove(HERMES_HOME_ENV_VAR);
        let _home = TestEnvVar::remove("HOME");
        let _local = TestEnvVar::set("LOCALAPPDATA", &local_app_data);
        let _path = TestEnvVar::set("PATH", "");

        assert!(hermes_install_layout_available());
        assert!(integration_target_available(
            crate::api::schema::IntegrationTarget::Hermes
        ));

        let _ = fs::remove_dir_all(base);
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
            IntegrationTarget::Qwen,
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
            path: PathBuf::from("/tmp/gardn-agent-state.sh"),
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
        assert!(content.contains("GARDN_INTEGRATION_VERSION=7"));
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
        assert_eq!(content, OMP_EXTENSION_ASSET);
        assert!(content.contains("GARDN_INTEGRATION_ID=omp"));
        assert!(content.contains("GARDN_INTEGRATION_VERSION=9"));
        assert!(content.contains("agent: \"omp\""));
        assert!(!content.contains("agent: \"pi\""));

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
        fs::write(&extension_path, "// installed by Gardn\n").unwrap();
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
    fn outdated_integrations_detect_previous_omp_version() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let ext_dir = home.join(".omp/agent/extensions");
        fs::create_dir_all(&ext_dir).unwrap();
        let extension_path = ext_dir.join(OMP_EXTENSION_INSTALL_NAME);
        fs::write(
            &extension_path,
            "// GARDN_INTEGRATION_ID=omp\n// GARDN_INTEGRATION_VERSION=4\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let outdated = outdated_installed_integrations();

        assert_eq!(outdated.len(), 1);
        assert_eq!(
            outdated[0].target,
            crate::api::schema::IntegrationTarget::Omp
        );
        assert_eq!(outdated[0].path, extension_path);
        assert_eq!(outdated[0].installed_version, Some(4));
        assert_eq!(outdated[0].expected_version, OMP_INTEGRATION_VERSION);

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
            "// installed by Gardn\n// GARDN_INTEGRATION_ID=pi\n// GARDN_INTEGRATION_VERSION=1\n",
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
    fn install_pi_creates_extensions_dir_when_agent_dir_exists() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let agent_dir = home.join(".pi/agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let path = install_pi().unwrap();

        assert_eq!(
            path,
            agent_dir.join("extensions").join(PI_EXTENSION_INSTALL_NAME)
        );
        assert!(path.is_file());

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
    fn install_claude_preserves_settings_formatting_and_crlf() {
        let settings_path = Path::new("/home/test/.claude/settings.json");
        let hook_path = Path::new("/home/test/.claude/hooks/gardn-agent-state.sh");
        let input = concat!(
            "{\r\n",
            "    \"permissions\" : {\"allow\":[\"Read\"]},\r\n",
            "    \"alpha\" : 1\r\n",
            "}\r\n\r\n",
        );
        let updated = claude_settings::install(input, settings_path, hook_path).unwrap();
        assert!(updated.starts_with("{\r\n    \"permissions\" : {\"allow\":[\"Read\"]},"));
        assert!(updated.contains("\"alpha\" : 1"));
        assert!(updated.contains("SessionStart"));
        assert!(!updated.replace("\r\n", "").contains('\n'));
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
            "#!/bin/sh\n# GARDN_INTEGRATION_ID=claude\n# GARDN_INTEGRATION_VERSION=1\n",
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
            "#!/bin/sh\n# GARDN_INTEGRATION_ID=codex\n# GARDN_INTEGRATION_VERSION=1\n",
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
    fn devin_v1_integration_status_is_outdated_after_python39_hook_update() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let devin_dir = base.join("devin");
        fs::create_dir_all(&devin_dir).unwrap();
        let hook_path = devin_dir.join(DEVIN_HOOK_INSTALL_NAME);
        fs::write(
            &hook_path,
            "#!/bin/sh\n# GARDN_INTEGRATION_ID=devin\n# GARDN_INTEGRATION_VERSION=1\n",
        )
        .unwrap();
        let _devin_config_dir_env = TestEnvVar::set(DEVIN_CONFIG_DIR_ENV_VAR, &devin_dir);

        let statuses = installed_integration_statuses();
        let devin = statuses
            .iter()
            .find(|status| status.target == crate::api::schema::IntegrationTarget::Devin)
            .unwrap();

        assert_eq!(devin.path, hook_path);
        assert_eq!(devin.installed_version, Some(1));
        assert_eq!(devin.expected_version, DEVIN_INTEGRATION_VERSION);
        assert_eq!(devin.state, IntegrationStatusKind::Outdated);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_claude_removes_gardn_hooks_and_preserves_others() {
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
    fn install_codex_for_agent_profiles_installs_each_configured_home() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let default_codex_dir = home.join(".codex");
        let custom_codex_dir = base.join("codex-mk");
        fs::create_dir_all(&default_codex_dir).unwrap();
        fs::create_dir_all(&custom_codex_dir).unwrap();
        fs::write(
            default_codex_dir.join("config.toml"),
            "model = \"gpt-5.4\"\n",
        )
        .unwrap();
        fs::write(
            custom_codex_dir.join("config.toml"),
            "model = \"gpt-5.5\"\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            CODEX_HOME_ENV_VAR.to_string(),
            custom_codex_dir.to_string_lossy().to_string(),
        );
        let catalog = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-mk".into()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-mk".into(),
                    name: "codex mk".into(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-mk".into(),
                    env,
                    enabled: true,
                }],
            },
        );

        let messages = install_target_for_agent_profiles(
            crate::api::schema::IntegrationTarget::Codex,
            &catalog,
        )
        .unwrap();

        assert!(default_codex_dir.join(CODEX_HOOK_INSTALL_NAME).is_file());
        assert!(default_codex_dir.join("hooks.json").is_file());
        assert!(custom_codex_dir.join(CODEX_HOOK_INSTALL_NAME).is_file());
        assert!(custom_codex_dir.join("hooks.json").is_file());
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.starts_with("installed codex integration hook"))
                .count(),
            2
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_for_agent_profiles_skips_missing_default_when_profile_home_exists() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let custom_codex_dir = base.join("codex-mk");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&custom_codex_dir).unwrap();
        fs::write(
            custom_codex_dir.join("config.toml"),
            "model = \"gpt-5.5\"\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            CODEX_HOME_ENV_VAR.to_string(),
            custom_codex_dir.to_string_lossy().to_string(),
        );
        let catalog = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-mk".into()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-mk".into(),
                    name: "codex mk".into(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-mk".into(),
                    env,
                    enabled: true,
                }],
            },
        );

        let messages = install_target_for_agent_profiles(
            crate::api::schema::IntegrationTarget::Codex,
            &catalog,
        )
        .unwrap();

        assert!(custom_codex_dir.join(CODEX_HOOK_INSTALL_NAME).is_file());
        assert!(messages.iter().any(
            |message| message.starts_with(INSTALL_WARNING_PREFIX) && message.contains(".codex")
        ));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn codex_mk_profile_warning_clears_when_profile_home_hook_is_current() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let custom_codex_dir = home.join(".codex-mk");
        fs::create_dir_all(&custom_codex_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let catalog = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-mk".into()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-mk".into(),
                    name: "codex mk".into(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-mk".into(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        let profile = catalog.get("user:codex-mk").unwrap();

        let warning = agent_profile_integration_warning(profile)
            .expect("missing profile-specific codex hook should warn");

        assert!(warning.contains("codex mk"), "{warning}");
        assert!(warning.contains(".codex-mk"), "{warning}");
        assert!(
            warning.contains("gardn integration install codex"),
            "{warning}"
        );

        fs::write(
            custom_codex_dir.join(CODEX_HOOK_INSTALL_NAME),
            CODEX_HOOK_ASSET,
        )
        .unwrap();

        assert_eq!(agent_profile_integration_warning(profile), None);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_codex_for_agent_profiles_installs_codex_mk_home_without_codex_home_env() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let default_codex_dir = home.join(".codex");
        let custom_codex_dir = home.join(".codex-mk");
        fs::create_dir_all(&custom_codex_dir).unwrap();
        fs::write(
            custom_codex_dir.join("config.toml"),
            "model = \"gpt-5.5\"\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let catalog = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:codex-mk".into()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "codex-mk".into(),
                    name: "codex mk".into(),
                    kind: crate::agent_profiles::AgentKind::Codex,
                    command: "codex-mk".into(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );

        let messages = install_target_for_agent_profiles(
            crate::api::schema::IntegrationTarget::Codex,
            &catalog,
        )
        .unwrap();

        assert!(!default_codex_dir.exists());
        assert_eq!(
            fs::read_to_string(custom_codex_dir.join(CODEX_HOOK_INSTALL_NAME)).unwrap(),
            CODEX_HOOK_ASSET
        );
        assert!(custom_codex_dir.join("hooks.json").is_file());
        assert!(messages.iter().any(|message| {
            message.starts_with("installed codex integration hook") && message.contains(".codex-mk")
        }));

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
    fn uninstall_codex_removes_gardn_hooks_and_leaves_config_alone() {
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
    fn uninstall_codex_for_agent_profiles_removes_default_and_custom_hooks() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let default_codex_dir = home.join(".codex");
        let custom_codex_dir = base.join("codex-mk");
        fs::create_dir_all(&default_codex_dir).unwrap();
        fs::create_dir_all(&custom_codex_dir).unwrap();
        fs::write(
            default_codex_dir.join("config.toml"),
            "model = \"gpt-5.4\"\n",
        )
        .unwrap();
        fs::write(
            custom_codex_dir.join("config.toml"),
            "model = \"gpt-5.5\"\n[features]\nother = true\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let catalog = codex_profile_catalog_with_home("codex-mk", "codex-mk", &custom_codex_dir);

        install_target_for_agent_profiles(crate::api::schema::IntegrationTarget::Codex, &catalog)
            .unwrap();
        add_non_gardn_codex_hook(&default_codex_dir, "echo keep default");
        add_non_gardn_codex_hook(&custom_codex_dir, "echo keep custom");
        let custom_config_before =
            fs::read_to_string(custom_codex_dir.join("config.toml")).unwrap();

        let messages = uninstall_target_for_agent_profiles(
            crate::api::schema::IntegrationTarget::Codex,
            &catalog,
        )
        .unwrap();
        let default_hook_path = default_codex_dir.join(CODEX_HOOK_INSTALL_NAME);
        let custom_hook_path = custom_codex_dir.join(CODEX_HOOK_INSTALL_NAME);
        let default_hook_path_text = default_hook_path.display().to_string();
        let custom_hook_path_text = custom_hook_path.display().to_string();
        let default_commands = codex_hook_commands(&default_codex_dir);
        let custom_commands = codex_hook_commands(&custom_codex_dir);

        assert!(!default_hook_path.exists());
        assert!(!custom_hook_path.exists());
        assert!(
            default_commands
                .iter()
                .all(|command| !command.contains(&default_hook_path_text)),
            "{default_commands:?}"
        );
        assert!(
            custom_commands
                .iter()
                .all(|command| !command.contains(&custom_hook_path_text)),
            "{custom_commands:?}"
        );
        assert!(default_commands
            .iter()
            .any(|command| command == "echo keep default"));
        assert!(custom_commands
            .iter()
            .any(|command| command == "echo keep custom"));
        assert_eq!(
            fs::read_to_string(custom_codex_dir.join("config.toml")).unwrap(),
            custom_config_before
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.starts_with("removed codex hook at "))
                .count(),
            2
        );
        assert_eq!(
            messages
                .iter()
                .filter(|message| { message.starts_with("removed gardn codex hook entries from ") })
                .count(),
            2
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_codex_for_agent_profiles_skips_missing_custom_home() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let default_codex_dir = home.join(".codex");
        let missing_custom_codex_dir = base.join("missing-codex-mk");
        fs::create_dir_all(&default_codex_dir).unwrap();
        fs::write(
            default_codex_dir.join("config.toml"),
            "model = \"gpt-5.4\"\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        install_codex().unwrap();
        let catalog =
            codex_profile_catalog_with_home("codex-mk", "codex-mk", &missing_custom_codex_dir);

        let messages = uninstall_target_for_agent_profiles(
            crate::api::schema::IntegrationTarget::Codex,
            &catalog,
        )
        .unwrap();

        assert!(!default_codex_dir.join(CODEX_HOOK_INSTALL_NAME).exists());
        assert!(!missing_custom_codex_dir.exists());
        assert!(messages.iter().any(|message| {
            message.starts_with("removed codex hook at ")
                && message.contains(&default_codex_dir.display().to_string())
        }));
        assert!(messages.iter().any(|message| {
            message.starts_with(INSTALL_WARNING_PREFIX)
                && message.contains(&missing_custom_codex_dir.display().to_string())
        }));

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
    fn uninstall_copilot_removes_gardn_hooks_and_preserves_others() {
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
    fn install_opencode_writes_server_and_tui_plugins() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let installed = install_opencode().unwrap();

        assert_eq!(
            installed.plugin_path,
            opencode_dir
                .join("plugins")
                .join(OPENCODE_PLUGIN_INSTALL_NAME)
        );
        assert_eq!(
            fs::read_to_string(&installed.plugin_path).unwrap(),
            OPENCODE_PLUGIN_ASSET
        );
        assert_eq!(
            installed.tui_plugin_path,
            opencode_dir.join(OPENCODE_TUI_PLUGIN_INSTALL_NAME)
        );
        assert_eq!(
            fs::read_to_string(&installed.tui_plugin_path).unwrap(),
            OPENCODE_TUI_PLUGIN_ASSET
        );
        assert_eq!(installed.tui_config_path, opencode_dir.join("tui.jsonc"));
        let tui_config: Value =
            serde_json::from_str(&fs::read_to_string(&installed.tui_config_path).unwrap()).unwrap();
        assert_eq!(tui_config["plugin"], json!([OPENCODE_TUI_PLUGIN_SPEC]));
        let plugin_content = fs::read_to_string(&installed.plugin_path).unwrap();
        assert!(plugin_content.contains("GARDN_INTEGRATION_VERSION=8"));
        assert!(plugin_content.contains("Math.max(reportSeq + 1, Date.now() * 1000)"));
        assert!(plugin_content.contains("pane.report_agent_session"));
        assert!(plugin_content.contains("pane.report_agent"));
        assert!(plugin_content.contains("permission.asked"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn opencode_status_requires_the_tui_plugin_and_config_entry() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let installed = install_opencode().unwrap();
        let status = || {
            integration_status_at(
                crate::api::schema::IntegrationTarget::Opencode,
                installed.plugin_path.clone(),
                OPENCODE_INTEGRATION_VERSION,
            )
            .state
        };

        assert_eq!(status(), IntegrationStatusKind::Current);
        fs::remove_file(&installed.tui_plugin_path).unwrap();
        assert_eq!(status(), IntegrationStatusKind::Outdated);
        fs::write(&installed.tui_plugin_path, OPENCODE_TUI_PLUGIN_ASSET).unwrap();
        opencode_config::remove_tui_plugin(&opencode_dir, OPENCODE_TUI_PLUGIN_SPEC).unwrap();
        assert_eq!(status(), IntegrationStatusKind::Outdated);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_opencode_removes_plugins_and_managed_tui_config_entry() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);
        let installed = install_opencode().unwrap();

        let result = uninstall_opencode().unwrap();

        assert!(result.removed_plugin);
        assert!(result.removed_tui_plugin);
        assert!(result.updated_tui_config);
        assert!(!result.plugin_path.exists());
        assert!(!result.tui_plugin_path.exists());
        assert!(result.tui_config_path.exists());
        let tui_config: Value =
            serde_json::from_str(&fs::read_to_string(&result.tui_config_path).unwrap()).unwrap();
        assert_eq!(tui_config, json!({}));
        assert_eq!(installed.plugin_path, result.plugin_path);

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn install_opencode_invalid_tui_config_does_not_write_plugins() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode");
        fs::create_dir_all(&opencode_dir).unwrap();
        fs::write(opencode_dir.join("tui.jsonc"), r#"{"plugin":{}}"#).unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = install_opencode().unwrap_err().to_string();

        assert!(err.contains("plugin list"));
        assert!(!opencode_dir
            .join("plugins")
            .join(OPENCODE_PLUGIN_INSTALL_NAME)
            .exists());
        assert!(!opencode_dir.join(OPENCODE_TUI_PLUGIN_INSTALL_NAME).exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn uninstall_opencode_removes_plugins_when_tui_config_is_invalid() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let opencode_dir = home.join(".config/opencode");
        let plugins_dir = opencode_dir.join("plugins");
        fs::create_dir_all(&plugins_dir).unwrap();
        let plugin_path = plugins_dir.join(OPENCODE_PLUGIN_INSTALL_NAME);
        let tui_plugin_path = opencode_dir.join(OPENCODE_TUI_PLUGIN_INSTALL_NAME);
        fs::write(&plugin_path, OPENCODE_PLUGIN_ASSET).unwrap();
        fs::write(&tui_plugin_path, OPENCODE_TUI_PLUGIN_ASSET).unwrap();
        fs::write(opencode_dir.join("tui.jsonc"), "{\"plugin\":").unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let err = uninstall_opencode().unwrap_err().to_string();

        assert!(err.contains("failed to parse OpenCode TUI config"));
        assert!(!plugin_path.exists());
        assert!(!tui_plugin_path.exists());

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
        assert!(config.contains("plugins:\n  enabled:\n    - gardn-agent-state"));

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
            "plugins:\n  enabled:\n    - gardn-agent-state\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        install_hermes().unwrap();
        install_hermes().unwrap();

        let config = fs::read_to_string(hermes_dir.join("config.yaml")).unwrap();
        assert_eq!(config.matches("gardn-agent-state").count(), 1);

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
  - gardn-agent-state
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
  - gardn-agent-state
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
            "plugins:\n  enabled:\n    - other-plugin\n    - gardn-agent-state\n",
        )
        .unwrap();
        let _home_env = TestEnvVar::set("HOME", &home);

        let result = uninstall_hermes().unwrap();
        let config = fs::read_to_string(hermes_dir.join("config.yaml")).unwrap();

        assert!(result.removed_plugin_dir);
        assert!(result.updated_config);
        assert!(!plugin_dir.exists());
        assert!(config.contains("    - other-plugin"));
        assert!(!config.contains("gardn-agent-state"));

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
    fn kilo_installer_manages_only_its_plugin() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let kilo_home = home.join(".config/kilo");
        fs::create_dir_all(kilo_home.join("plugin")).unwrap();
        fs::write(kilo_home.join("plugin/foreign.js"), "foreign").unwrap();
        let _home = TestEnvVar::set("HOME", &home);

        let installed = install_kilo().unwrap();
        install_kilo().unwrap();
        assert_eq!(
            fs::read_to_string(&installed.plugin_path).unwrap(),
            KILO_PLUGIN_ASSET
        );
        let result = uninstall_kilo().unwrap();
        assert!(result.removed_plugin);
        assert!(kilo_home.join("plugin/foreign.js").is_file());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn mastracode_installer_is_idempotent_and_preserves_foreign_hooks() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let home = base.join("home");
        let mastra_home = home.join(".mastracode");
        fs::create_dir_all(&mastra_home).unwrap();
        fs::write(
            mastra_home.join("hooks.json"),
            serde_json::to_string_pretty(&json!({
                "UserPromptSubmit": [{
                    "type": "command",
                    "command": "foreign-hook",
                    "timeout": 1
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let _home = TestEnvVar::set("HOME", &home);

        let installed = install_mastracode().unwrap();
        install_mastracode().unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(&installed.hooks_path).unwrap()).unwrap();
        let entries = hooks["UserPromptSubmit"].as_array().unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(
                    |entry| entry["command"] == hook_command(&installed.hook_path, Some("working"))
                )
                .count(),
            1
        );
        assert!(entries
            .iter()
            .any(|entry| entry["command"] == "foreign-hook"));
        assert!(hooks["PermissionRequest"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["command"] == hook_command(&installed.hook_path, Some("blocked"))));

        let result = uninstall_mastracode().unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(&result.hooks_path).unwrap()).unwrap();
        assert!(result.removed_hook_file);
        assert!(result.updated_hooks);
        assert!(hooks["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["command"] == "foreign-hook"));

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn antigravity_installer_rewrites_only_the_gardn_owned_block() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let config_home = base.join("antigravity");
        fs::create_dir_all(&config_home).unwrap();
        fs::write(
            config_home.join("hooks.json"),
            serde_json::to_string_pretty(&json!({
                "foreign": {"PreInvocation": [{"command": "foreign-hook"}]},
                "gardn": {"stale": true}
            }))
            .unwrap(),
        )
        .unwrap();
        let _config = TestEnvVar::set(ANTIGRAVITY_CLI_CONFIG_DIR_ENV_VAR, &config_home);

        let installed = install_antigravity_cli().unwrap();
        install_antigravity_cli().unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(&installed.hooks_path).unwrap()).unwrap();
        assert_eq!(
            hooks["foreign"]["PreInvocation"][0]["command"],
            "foreign-hook"
        );
        assert!(hooks["gardn"].get("stale").is_none());
        assert_eq!(
            hooks["gardn"]["PreInvocation"][0]["command"],
            hook_command(&installed.hook_path, Some("session"))
        );

        let result = uninstall_antigravity_cli().unwrap();
        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(&result.hooks_path).unwrap()).unwrap();
        assert!(result.removed_hook_file);
        assert!(result.updated_hooks);
        assert!(hooks.get("gardn").is_none());
        assert_eq!(
            hooks["foreign"]["PreInvocation"][0]["command"],
            "foreign-hook"
        );

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn qwen_install_is_idempotent_and_uninstall_preserves_foreign_settings() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let qwen_home = base.join(".qwen");
        fs::create_dir_all(&qwen_home).unwrap();
        fs::write(
            qwen_home.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "theme": "dark",
                "hooks": {
                    "SessionStart": [{
                        "matcher": "foreign",
                        "hooks": [{
                            "type": "command",
                            "command": "foreign-hook",
                            "timeout": 3
                        }]
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let _qwen_home = TestEnvVar::set(QWEN_HOME_ENV_VAR, &qwen_home);

        let installed = install_qwen().unwrap();
        install_qwen().unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&installed.settings_path).unwrap()).unwrap();
        let entries = settings["hooks"]["SessionStart"].as_array().unwrap();
        let managed_command = hook_command(&installed.hook_path, Some("session"));

        assert_eq!(
            fs::read_to_string(&installed.hook_path).unwrap(),
            QWEN_HOOK_ASSET
        );
        assert_eq!(settings["theme"], "dark");
        assert_eq!(
            entries
                .iter()
                .flat_map(|entry| entry["hooks"].as_array().into_iter().flatten())
                .filter(|hook| hook["command"] == managed_command)
                .count(),
            1
        );
        assert!(entries.iter().any(|entry| {
            entry["hooks"]
                .as_array()
                .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == "foreign-hook"))
        }));

        let result = uninstall_qwen().unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&result.settings_path).unwrap()).unwrap();
        assert!(result.removed_hook_file);
        assert!(result.updated_settings);
        assert_eq!(settings["theme"], "dark");
        assert!(settings["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| {
                entry["hooks"]
                    .as_array()
                    .is_some_and(|hooks| hooks.iter().any(|hook| hook["command"] == "foreign-hook"))
            }));

        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn qwen_unix_hook_reports_session_identity_and_filters_start_source() {
        use std::os::unix::fs::PermissionsExt;

        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let qwen_home = base.join(".qwen");
        fs::create_dir_all(&qwen_home).unwrap();
        let _qwen_home = TestEnvVar::set(QWEN_HOME_ENV_VAR, &qwen_home);
        let installed = install_qwen().unwrap();
        let capture_path = base.join("argv.txt");
        let fake_gardn = base.join("gardn");
        fs::write(
            &fake_gardn,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n",
                shell_single_quote(&capture_path.display().to_string())
            ),
        )
        .unwrap();
        fs::set_permissions(&fake_gardn, fs::Permissions::from_mode(0o755)).unwrap();

        let mut child = std::process::Command::new(&installed.hook_path)
            .arg("session")
            .env("GARDN_ENV", "1")
            .env("GARDN_PANE_ID", "test:p1")
            .env("GARDN_SOCKET_PATH", "/tmp/gardn.sock")
            .env("GARDN_BIN_PATH", &fake_gardn)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"session_id":"qwen-session","source":"resume"}"#)
            .unwrap();
        assert!(child.wait().unwrap().success());
        let argv = fs::read_to_string(&capture_path).unwrap();
        assert!(argv.contains("pane\nreport-agent-session\ntest:p1\n"));
        assert!(argv.contains("--source\ngardn:qwen\n--agent\nqwen\n"));
        assert!(argv.contains("--agent-session-id\nqwen-session\n"));
        assert!(argv.contains("--session-start-source\nresume\n"));
        let mut child = std::process::Command::new(&installed.hook_path)
            .arg("session")
            .env("GARDN_ENV", "1")
            .env("GARDN_PANE_ID", "test:p1")
            .env("GARDN_SOCKET_PATH", "/tmp/gardn.sock")
            .env("GARDN_BIN_PATH", &fake_gardn)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(br#"{"session_id":"qwen-session","source":"external"}"#)
            .unwrap();
        assert!(child.wait().unwrap().success());
        let argv = fs::read_to_string(&capture_path).unwrap();
        assert!(!argv.contains("--session-start-source"));

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
    fn install_and_uninstall_grok_manage_lifecycle_hooks() {
        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let grok_dir = base.join(".grok");
        fs::create_dir_all(&grok_dir).unwrap();
        let _grok_home_env = TestEnvVar::set(GROK_HOME_ENV_VAR, &grok_dir);

        let installed = install_grok().unwrap();
        install_grok().unwrap();
        let config: Value =
            serde_json::from_str(&fs::read_to_string(&installed.config_path).unwrap()).unwrap();

        assert_eq!(
            fs::read_to_string(&installed.hook_path).unwrap(),
            GROK_HOOK_ASSET
        );
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "SubagentStart",
            "PreCompact",
            "PostCompact",
            "PreToolUse",
            "PostToolUse",
            "PostToolUseFailure",
            "PermissionDenied",
            "Notification",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ] {
            assert!(config["hooks"].get(event).is_some(), "missing {event}");
        }
        assert_eq!(
            config["hooks"]["SessionStart"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .last(),
            Some("session")
        );
        assert_eq!(
            config["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .last(),
            Some("release")
        );

        let removed = uninstall_grok().unwrap();
        assert!(removed.removed_hook_file);
        assert!(removed.removed_config_file);
        assert!(!removed.hook_path.exists());
        assert!(!removed.config_path.exists());
        let already_removed = uninstall_grok().unwrap();
        assert!(!already_removed.removed_hook_file);
        assert!(!already_removed.removed_config_file);
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn grok_hook_reports_parent_lifecycle_and_ignores_subagent_stop() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::process::{Command, Stdio};

        fn run_hook(hook_path: &Path, action: &str, payload: &str) -> Value {
            let socket_path = std::env::temp_dir()
                .join(format!("gardn-grok-{}-{action}.sock", std::process::id()));
            let _ = fs::remove_file(&socket_path);
            let listener = UnixListener::bind(&socket_path).unwrap();
            let request = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                stream.write_all(b"{\"ok\":true}\n").unwrap();
                serde_json::from_str::<Value>(&line).unwrap()
            });
            let mut child = Command::new("sh")
                .arg(hook_path)
                .arg(action)
                .env("GARDN_ENV", "1")
                .env("GARDN_PANE_ID", "pane-7")
                .env("GARDN_SOCKET_PATH", &socket_path)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(payload.as_bytes())
                .unwrap();
            assert!(child.wait().unwrap().success());
            let request = request.join().unwrap();
            let _ = fs::remove_file(socket_path);
            request
        }

        let _lock = integration_env_lock();
        let _path_env = clear_integration_path_env();
        let base = unique_base();
        let grok_dir = base.join(".grok");
        fs::create_dir_all(&grok_dir).unwrap();
        let _grok_home_env = TestEnvVar::set(GROK_HOME_ENV_VAR, &grok_dir);
        let installed = install_grok().unwrap();

        let session = run_hook(
            &installed.hook_path,
            "session",
            r#"{"hookEventName":"SessionStart","sessionId":"grok-session"}"#,
        );
        assert_eq!(session["method"], "pane.report_agent_session");
        assert_eq!(session["params"]["source"], "gardn:grok");
        assert_eq!(session["params"]["agent"], "grok");
        assert_eq!(session["params"]["agent_session_id"], "grok-session");

        for (action, expected_method, expected_state) in [
            ("working", "pane.report_agent", Some("working")),
            ("blocked", "pane.report_agent", Some("blocked")),
            ("idle", "pane.report_agent", Some("idle")),
            ("release", "pane.release_agent", None),
        ] {
            let request = run_hook(
                &installed.hook_path,
                action,
                r#"{"hookEventName":"Stop","sessionId":"grok-session"}"#,
            );
            assert_eq!(request["method"], expected_method);
            assert_eq!(request["params"]["state"].as_str(), expected_state);
        }

        let ignored_socket_path =
            std::env::temp_dir().join(format!("gardn-grok-{}-ignored.sock", std::process::id()));
        let _ = fs::remove_file(&ignored_socket_path);
        let ignored_listener = UnixListener::bind(&ignored_socket_path).unwrap();
        ignored_listener.set_nonblocking(true).unwrap();
        let mut ignored = Command::new("sh")
            .arg(&installed.hook_path)
            .arg("idle")
            .env("GARDN_ENV", "1")
            .env("GARDN_PANE_ID", "pane-7")
            .env("GARDN_SOCKET_PATH", &ignored_socket_path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        ignored
            .stdin
            .take()
            .unwrap()
            .write_all(
                br#"{"hookEventName":"SubagentStop","agentId":"child","sessionId":"grok-session"}"#,
            )
            .unwrap();
        assert!(ignored.wait().unwrap().success());
        assert!(matches!(
            ignored_listener.accept(),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock
        ));
        drop(ignored_listener);
        let _ = fs::remove_file(ignored_socket_path);
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
        assert!(PI_EXTENSION_ASSET.contains("pi.on(\"agent_settled\""));
        assert!(PI_EXTENSION_ASSET.contains("!sessionIsIdle(ctx)"));
        assert!(!PI_EXTENSION_ASSET.contains("pi.on(\"agent_end\""));
        assert!(!PI_EXTENSION_ASSET.contains("GARDN_PI_IDLE_DEBOUNCE_MS"));
        assert!(!PI_EXTENSION_ASSET.contains("GARDN_PI_RETRY_GRACE_MS"));
        assert!(PI_EXTENSION_ASSET.contains("ctx.mode !== \"tui\""));
        assert!(OMP_EXTENSION_ASSET.contains("agent_session_path: currentAgentSessionPath"));
        assert!(OMP_EXTENSION_ASSET.contains("agent_session_id: currentAgentSessionId"));
        assert!(OMP_EXTENSION_ASSET.contains("publishState(true)"));
        assert!(OMP_EXTENSION_ASSET.contains("new Set<string>()"));
        assert!(OMP_EXTENSION_ASSET.contains("event?.toolName === \"ask\""));
        assert!(OMP_EXTENSION_ASSET.contains("tool_execution_start"));
        assert!(OMP_EXTENSION_ASSET.contains("tool_execution_end"));
        assert!(PI_EXTENSION_ASSET.contains("function sendRequestAttempt"));
        assert!(PI_EXTENSION_ASSET.contains("await sendRequestAttempt(request, 1500)"));

        assert!(PI_EXTENSION_ASSET.contains("let rootSession = false"));
        assert!(OMP_EXTENSION_ASSET.contains("let requestQueue = Promise.resolve()"));
        assert!(OMP_EXTENSION_ASSET
            .contains("function reportSession(sessionStartSource = \"startup\")"));
        assert!(OMP_EXTENSION_ASSET.contains("let rootSession = false"));
        assert!(OMP_EXTENSION_ASSET.contains("pi.on(\"session_switch\""));
        assert!(OMP_EXTENSION_ASSET.contains("tool_approval_requested"));

        assert!(CLAUDE_HOOK_ASSET.contains("agent_session_path"));
        assert!(CLAUDE_HOOK_ASSET.contains("session_start_source"));
        let stale_session_ref_freeze = "if (currentAgentSessionPath || currentAgentSessionId)";
        assert!(
            !PI_EXTENSION_ASSET.contains(stale_session_ref_freeze),
            "PI extension must refresh session refs on later session_start after /resume or a session switch"
        );
        assert!(
            !OMP_EXTENSION_ASSET.contains(stale_session_ref_freeze),
            "OMP extension must refresh session refs on later session_start after /resume or a session switch"
        );
        assert!(
            !OPENCODE_PLUGIN_ASSET.contains("if (!primarySessionID && !parentID)"),
            "OpenCode plugin must not freeze the first top-level session forever"
        );
        assert!(OPENCODE_PLUGIN_ASSET.contains("setPrimarySession(sessionID)"));
        assert!(OPENCODE_PLUGIN_ASSET
            .contains("type === \"session.created\" || type === \"session.updated\""));
        assert!(CLAUDE_HOOK_ASSET.contains("agent_session_id"));
        assert!(CODEX_HOOK_ASSET.contains("GARDN_HOOK_INPUT_FILE"));
        assert!(CODEX_HOOK_ASSET.contains("agent_session_id"));
        assert!(CODEX_HOOK_ASSET.contains("CODEX_THREAD_ID"));
        assert!(COPILOT_HOOK_ASSET.contains("GARDN_HOOK_INPUT_FILE"));
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
        assert!(OPENCODE_TUI_PLUGIN_ASSET.contains("session_start_source: \"select\""));
        assert!(HERMES_PLUGIN_INIT_ASSET.contains("pane.report_agent_session"));
        assert!(HERMES_PLUGIN_INIT_ASSET.contains("\"session_start_source\": start_source"));
        assert!(HERMES_PLUGIN_INIT_ASSET.contains("agent_session_id"));
        assert!(!HERMES_PLUGIN_INIT_ASSET.contains("pane.report_agent\""));
        assert!(!HERMES_PLUGIN_INIT_ASSET.contains("on_session_finalize"));
        assert!(!HERMES_PLUGIN_INIT_ASSET.contains("pane.release_agent"));
        // Qoder hook reads the event from the stdin JSON payload (per
        // https://docs.qoder.com/zh/cli/hooks). Make sure the bundled script
        // never reaches for a QODER_HOOK_EVENT environment variable.
        assert!(QODERCLI_HOOK_ASSET.contains("GARDN_HOOK_INPUT_FILE"));
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
    fn uninstall_qodercli_removes_gardn_hooks_and_preserves_others() {
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
