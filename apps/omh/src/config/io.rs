use std::path::{Path, PathBuf};

use tracing::warn;

use super::{model::LoadedConfig, Config, CONFIG_PATH_ENV_VAR};

const KNOWN_TOP_LEVEL_CONFIG_KEYS: &[&str] = &[
    "advanced",
    "agent_profiles",
    "experimental",
    "git",
    "keys",
    "onboarding",
    "remote",
    "session",
    "terminal",
    "theme",
    "ui",
    "update",
];

pub fn app_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "omh-dev"
    } else {
        "omh"
    }
}

pub fn config_dir() -> PathBuf {
    config_dir_from_env(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),
        cfg!(windows),
    )
}

pub fn state_dir() -> PathBuf {
    state_dir_from_env(
        std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        cfg!(windows),
    )
}

fn config_dir_from_env(
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
    appdata: Option<PathBuf>,
    target_is_windows: bool,
) -> PathBuf {
    if let Some(dir) = xdg_config_home {
        return dir.join(app_dir_name());
    }
    if target_is_windows {
        if let Some(dir) = appdata {
            return dir.join(app_dir_name());
        }
    }
    if let Some(home) = home {
        return home.join(format!(".config/{}", app_dir_name()));
    }
    std::env::temp_dir().join(app_dir_name())
}

fn state_dir_from_env(
    xdg_state_home: Option<PathBuf>,
    home: Option<PathBuf>,
    local_appdata: Option<PathBuf>,
    target_is_windows: bool,
) -> PathBuf {
    if let Some(dir) = xdg_state_home {
        return dir.join(app_dir_name());
    }
    if target_is_windows {
        if let Some(dir) = local_appdata {
            return dir.join(app_dir_name()).join("state");
        }
    }
    if let Some(home) = home {
        return home.join(format!(".local/state/{}", app_dir_name()));
    }
    std::env::temp_dir().join(format!("{}-state", app_dir_name()))
}

fn read_optional_config(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

impl Config {
    pub fn load() -> LoadedConfig {
        let path = config_path();
        let content = match read_optional_config(&path) {
            Ok(Some(content)) => content,
            Ok(None) => {
                return LoadedConfig {
                    config: Self::default(),
                    diagnostics: Vec::new(),
                    invalid_sections: Vec::new(),
                };
            }
            Err(err) => {
                warn!(err = %err, "config read error, using defaults");
                return LoadedConfig {
                    config: Self::default(),
                    diagnostics: vec![format!("config read error: {err}; using defaults")],
                    invalid_sections: Vec::new(),
                };
            }
        };

        match toml::from_str::<Config>(&content) {
            Ok(config) => {
                let mut diagnostics = unknown_top_level_section_diagnostics_from_str(&content);
                diagnostics.extend(config.collect_diagnostics());
                LoadedConfig {
                    config,
                    diagnostics,
                    invalid_sections: Vec::new(),
                }
            }
            Err(err) => {
                warn!(err = %err, "config parse error, using defaults");
                LoadedConfig {
                    config: Self::default(),
                    diagnostics: vec![format!("config parse error: {err}; using defaults")],
                    invalid_sections: Vec::new(),
                }
            }
        }
    }
}

pub(super) fn resolve_config_relative_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    config_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(path)
}

pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var(CONFIG_PATH_ENV_VAR) {
        return PathBuf::from(path);
    }
    config_dir().join("config.toml")
}

pub fn config_diagnostic_summary(diagnostics: &[String]) -> Option<String> {
    (!diagnostics.is_empty()).then(|| diagnostics.join("\n"))
}

pub fn load_live_config() -> Result<LoadedConfig, Vec<String>> {
    let path = config_path();
    let content = match read_optional_config(&path) {
        Ok(Some(content)) => content,
        Ok(None) => {
            return Ok(LoadedConfig {
                config: Config::default(),
                diagnostics: Vec::new(),
                invalid_sections: Vec::new(),
            });
        }
        Err(err) => {
            return Err(vec![format!(
                "config read error: {err}; keeping current config"
            )]);
        }
    };
    load_live_config_from_str(&content)
}

fn load_live_config_from_str(content: &str) -> Result<LoadedConfig, Vec<String>> {
    let table = content
        .parse::<toml::Table>()
        .map_err(|err| vec![format!("config parse error: {err}; keeping current config")])?;

    let mut config = Config::default();
    let mut diagnostics = unknown_top_level_section_diagnostics(&table);
    let mut invalid_sections = Vec::new();

    if let Some(value) = table.get("onboarding") {
        match value.clone().try_into::<Option<bool>>() {
            Ok(onboarding) => config.onboarding = onboarding,
            Err(err) => diagnostics.push(format!(
                "invalid onboarding setting: {err}; keeping current onboarding state"
            )),
        }
    }

    load_live_section(
        &table,
        "theme",
        "theme config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.theme = section,
    );
    load_live_section(
        &table,
        "keys",
        "keybinding config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.keys = section,
    );
    load_live_section(
        &table,
        "terminal",
        "terminal config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.terminal = section,
    );
    load_live_section(
        &table,
        "session",
        "session config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.session = section,
    );
    load_live_section(
        &table,
        "ui",
        "ui config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.ui = section,
    );
    load_live_section(
        &table,
        "advanced",
        "advanced config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.advanced = section,
    );
    load_live_section(
        &table,
        "git",
        "git config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.git = section,
    );
    load_live_section(
        &table,
        "update",
        "update config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.update = section,
    );
    load_live_section(
        &table,
        "experimental",
        "experimental config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.experimental = section,
    );
    load_live_section(
        &table,
        "agent_profiles",
        "agent profile config",
        &mut diagnostics,
        &mut invalid_sections,
        |section| config.agent_profiles = section,
    );

    Ok(LoadedConfig {
        config,
        diagnostics,
        invalid_sections,
    })
}

fn unknown_top_level_section_diagnostics_from_str(content: &str) -> Vec<String> {
    content
        .parse::<toml::Value>()
        .ok()
        .and_then(|value| value.as_table().map(unknown_top_level_section_diagnostics))
        .unwrap_or_default()
}

fn unknown_top_level_section_diagnostics(
    table: &toml::map::Map<String, toml::Value>,
) -> Vec<String> {
    table
        .iter()
        .filter_map(|(key, value)| unknown_top_level_section_diagnostic(key, value))
        .collect()
}

fn unknown_top_level_section_diagnostic(key: &str, value: &toml::Value) -> Option<String> {
    if KNOWN_TOP_LEVEL_CONFIG_KEYS.contains(&key) {
        return None;
    }

    let header = if value.is_table() {
        format!("[{key}]")
    } else if value.as_array().is_some_and(|items| {
        !items.is_empty()
            && items
                .iter()
                .all(|item| matches!(item, toml::Value::Table(_)))
    }) {
        format!("[[{key}]]")
    } else {
        return None;
    };

    if key == "toast" {
        Some(format!(
            "unknown config section {header}; did you mean [ui.toast]? ignoring section"
        ))
    } else {
        Some(format!("unknown config section {header}; ignoring section"))
    }
}

fn load_live_section<T>(
    table: &toml::map::Map<String, toml::Value>,
    section: &'static str,
    label: &str,
    diagnostics: &mut Vec<String>,
    invalid_sections: &mut Vec<String>,
    apply: impl FnOnce(T),
) where
    T: serde::de::DeserializeOwned,
{
    let Some(value) = table.get(section) else {
        return;
    };

    match value.clone().try_into::<T>() {
        Ok(section_config) => apply(section_config),
        Err(err) => {
            diagnostics.push(format!(
                "invalid {label}: {err}; keeping current {section} settings"
            ));
            invalid_sections.push(section.to_string());
        }
    }
}

pub(crate) fn upsert_top_level_bool(content: &str, key: &str, value: bool) -> String {
    let replacement = format!("{key} = {value}");
    let mut lines: Vec<String> = content.lines().map(|line| line.to_string()).collect();
    let mut in_section = false;

    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = true;
            continue;
        }
        if in_section {
            continue;
        }
        if trimmed.starts_with(&format!("{key} ")) || trimmed.starts_with(&format!("{key}=")) {
            *line = replacement.clone();
            return lines.join("\n") + "\n";
        }
    }

    if lines.is_empty() {
        format!("{replacement}\n")
    } else {
        format!("{replacement}\n{}\n", lines.join("\n").trim_end())
    }
}

/// Write a key = value pair in a TOML section (creates section if missing).
pub fn upsert_section_value(content: &str, section: &str, key: &str, value: &str) -> String {
    upsert_section_raw(content, section, key, value)
}

pub fn upsert_section_bool(content: &str, section: &str, key: &str, value: bool) -> String {
    upsert_section_raw(content, section, key, &value.to_string())
}

pub fn remove_section_key(content: &str, section: &str, key: &str) -> String {
    let header = format!("[{section}]");
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;
    let mut in_section = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == header;
            result.push(line.to_string());
            i += 1;
            continue;
        }

        if in_section
            && (trimmed.starts_with(&format!("{key} ")) || trimmed.starts_with(&format!("{key}=")))
        {
            i += 1;
            continue;
        }

        result.push(line.to_string());
        i += 1;
    }

    result.join("\n") + "\n"
}

pub fn remove_keybinding_config_sections(content: &str) -> (String, bool) {
    let mut result = Vec::new();
    let mut removed = false;
    let mut skipping_key_section = false;
    let mut in_table = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if let Some(table_name) = toml_table_header_name(trimmed) {
            in_table = true;
            skipping_key_section = is_keys_table_name(table_name);
            if skipping_key_section {
                removed = true;
                continue;
            }
        } else if skipping_key_section || (!in_table && is_top_level_keys_assignment(trimmed)) {
            removed = true;
            continue;
        }

        result.push(line.to_string());
    }

    let mut updated = result.join("\n");
    if content.ends_with('\n') || !updated.is_empty() {
        updated.push('\n');
    }
    (updated, removed)
}

fn toml_table_header_name(trimmed: &str) -> Option<&str> {
    if let Some(name) = trimmed
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
    {
        return Some(name.trim());
    }
    trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .map(str::trim)
}

fn is_keys_table_name(name: &str) -> bool {
    name == "keys" || name.starts_with("keys.")
}

fn is_top_level_keys_assignment(trimmed: &str) -> bool {
    trimmed.starts_with("keys ") || trimmed.starts_with("keys=") || trimmed.starts_with("keys.")
}

fn upsert_section_raw(content: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("[{section}]");
    let assignment = format!("{key} = {value}");
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut i = 0;
    let mut found_section = false;
    let mut inserted = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed == header {
            found_section = true;
            result.push(line.to_string());
            i += 1;

            while i < lines.len() {
                let current = lines[i];
                let current_trimmed = current.trim();
                if current_trimmed.starts_with('[') && current_trimmed.ends_with(']') {
                    if !inserted {
                        result.push(assignment.clone());
                        inserted = true;
                    }
                    break;
                }

                if current_trimmed.starts_with(&format!("{key} "))
                    || current_trimmed.starts_with(&format!("{key}="))
                {
                    result.push(assignment.clone());
                    inserted = true;
                } else {
                    result.push(current.to_string());
                }
                i += 1;
            }

            continue;
        }

        result.push(line.to_string());
        i += 1;
    }

    if !found_section {
        if !result.is_empty() && !result.last().is_some_and(|line| line.trim().is_empty()) {
            result.push(String::new());
        }
        result.push(header);
        result.push(assignment);
    } else if !inserted {
        result.push(assignment);
    }

    result.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_dir_uses_omh_namespace() {
        assert_eq!(app_dir_name(), "omh-dev");
    }

    #[test]
    fn config_dir_uses_windows_appdata_on_windows_target() {
        assert_eq!(
            config_dir_from_env(
                None,
                Some(PathBuf::from("C:/Users/alice")),
                Some(PathBuf::from("C:/Users/alice/AppData/Roaming")),
                true,
            ),
            PathBuf::from("C:/Users/alice/AppData/Roaming").join(app_dir_name())
        );
    }

    #[test]
    fn state_dir_uses_windows_local_appdata_on_windows_target() {
        assert_eq!(
            state_dir_from_env(
                None,
                Some(PathBuf::from("C:/Users/alice")),
                Some(PathBuf::from("C:/Users/alice/AppData/Local")),
                true,
            ),
            PathBuf::from("C:/Users/alice/AppData/Local")
                .join(app_dir_name())
                .join("state")
        );
    }

    #[test]
    fn upsert_top_level_bool_replaces_existing_value() {
        let content = "onboarding = true\n[keys]\nprefix = \"ctrl+b\"\n";
        let updated = upsert_top_level_bool(content, "onboarding", false);
        assert_eq!(updated, "onboarding = false\n[keys]\nprefix = \"ctrl+b\"\n");
        let parsed: toml::Table = toml::from_str(&updated).unwrap();
        assert_eq!(parsed["onboarding"].as_bool(), Some(false));
        assert_eq!(parsed["keys"]["prefix"].as_str(), Some("ctrl+b"));
    }

    #[test]
    fn upsert_section_bool_adds_missing_section() {
        let updated = upsert_section_bool("", "ui.toast", "enabled", true);
        assert_eq!(updated, "[ui.toast]\nenabled = true\n");
        let parsed: toml::Table = toml::from_str(&updated).unwrap();
        assert_eq!(parsed["ui"]["toast"]["enabled"].as_bool(), Some(true));
    }

    #[test]
    fn remove_section_key_removes_matching_key_from_section() {
        let content =
            "[ui.toast]\nenabled = true\ndelivery = \"omh\"\n[ui.sound]\nenabled = true\n";
        let updated = remove_section_key(content, "ui.toast", "enabled");
        assert_eq!(
            updated,
            "[ui.toast]\ndelivery = \"omh\"\n[ui.sound]\nenabled = true\n"
        );
        let parsed: toml::Table = toml::from_str(&updated).unwrap();
        assert!(parsed["ui"]["toast"].get("enabled").is_none());
        assert_eq!(parsed["ui"]["toast"]["delivery"].as_str(), Some("omh"));
        assert_eq!(parsed["ui"]["sound"]["enabled"].as_bool(), Some(true));
    }

    #[test]
    fn diagnostic_summary_joins_messages() {
        assert_eq!(
            config_diagnostic_summary(&["first".into(), "second".into()]),
            Some("first\nsecond".into())
        );
        assert_eq!(config_diagnostic_summary(&[]), None);
    }

    #[test]
    fn load_live_config_parses_session_section() {
        let loaded = load_live_config_from_str(
            r#"
[session]
resume_agents_on_restore = true
"#,
        )
        .unwrap();

        assert!(loaded.config.session.resume_agents_on_restore);
        assert!(loaded.diagnostics.is_empty());
        assert!(loaded.invalid_sections.is_empty());
    }

    #[test]
    fn load_live_config_parses_agent_profiles_section() {
        let loaded = load_live_config_from_str(
            r#"
[agent_profiles]
order = ["user:omp-mk"]

[[agent_profiles.custom]]
id = "omp-mk"
name = "omp mk"
kind = "omp"
command = "omp-mk"
"#,
        )
        .unwrap();

        assert_eq!(loaded.config.agent_profiles.order, ["user:omp-mk"]);
        assert_eq!(loaded.config.agent_profiles.custom[0].id, "omp-mk");
        assert_eq!(loaded.config.agent_profiles.custom[0].command, "omp-mk");
        assert!(loaded.diagnostics.is_empty());
        assert!(loaded.invalid_sections.is_empty());
    }

    #[test]
    fn load_live_config_warns_about_unknown_top_level_sections() {
        let loaded = load_live_config_from_str(
            r#"
[toast]
delivery = "system"

[ui.toast]
delivery = "omh"
"#,
        )
        .unwrap();

        assert_eq!(
            loaded.diagnostics,
            vec!["unknown config section [toast]; did you mean [ui.toast]? ignoring section"]
        );
        assert!(loaded.invalid_sections.is_empty());
        assert_eq!(
            loaded.config.ui.toast.delivery,
            super::super::ToastDelivery::Omh
        );
    }

    #[test]
    fn load_live_config_does_not_warn_about_unknown_top_level_scalar_values() {
        let loaded = load_live_config_from_str(
            r#"
plugin = []

[ui.toast]
delivery = "omh"
"#,
        )
        .unwrap();

        assert!(loaded.diagnostics.is_empty());
        assert_eq!(
            loaded.config.ui.toast.delivery,
            super::super::ToastDelivery::Omh
        );
    }

    #[test]
    fn remove_keybinding_config_sections_removes_keys_tables_only() {
        let content = r#"onboarding = false

[theme]
name = "catppuccin"

[keys]
prefix = "ctrl+a"
new_tab = "c"

[[keys.command]]
key = "g"
command = "lazygit"

[keys.indexed]
tabs = "ctrl"

[ui]
mouse_capture = false
"#;

        let (updated, removed) = remove_keybinding_config_sections(content);

        assert!(removed);
        assert_eq!(
            updated,
            r#"onboarding = false

[theme]
name = "catppuccin"

[ui]
mouse_capture = false
"#
        );
        let parsed: toml::Table = toml::from_str(&updated).unwrap();
        assert_eq!(parsed["onboarding"].as_bool(), Some(false));
        assert_eq!(parsed["theme"]["name"].as_str(), Some("catppuccin"));
        assert_eq!(parsed["ui"]["mouse_capture"].as_bool(), Some(false));
        assert!(parsed.get("keys").is_none());
    }

    #[test]
    fn remove_keybinding_config_sections_reports_noop_without_keys() {
        let content = "[ui]\nmouse_capture = true\n";
        let (updated, removed) = remove_keybinding_config_sections(content);
        assert!(!removed);
        assert_eq!(updated, content);
    }
}
