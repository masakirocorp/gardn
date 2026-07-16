use super::App;

impl App {
    pub(super) fn update_config_file<F>(&mut self, error_context: &str, update: F) -> bool
    where
        F: FnOnce(&str) -> String,
    {
        #[cfg(test)]
        if std::env::var_os(crate::config::CONFIG_PATH_ENV_VAR).is_none() {
            return false;
        }

        let path = crate::config::config_path();
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                crate::logging::config_write_failed(&path, error_context, &err.to_string());
                self.state.config_diagnostic =
                    Some(format!("failed to save {error_context}: {err}"));
                self.config_diagnostic_deadline =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
                return false;
            }
        }

        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = update(&content);
        if let Err(err) = std::fs::write(&path, new_content) {
            crate::logging::config_write_failed(&path, error_context, &err.to_string());
            self.state.config_diagnostic = Some(format!("failed to save {error_context}: {err}"));
            self.config_diagnostic_deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            return false;
        }

        true
    }

    pub(super) fn mark_onboarding_complete(&mut self) {
        self.update_config_file("onboarding setting", |content| {
            crate::config::upsert_top_level_bool(content, "onboarding", false)
        });
    }

    pub(super) fn save_theme(
        &mut self,
        light: &str,
        dark: &str,
        mode: crate::config::ThemeMode,
        terminal_light_accent: crate::config::TerminalAccent,
        terminal_dark_accent: crate::config::TerminalAccent,
    ) {
        self.state.global_light_theme_name = light.to_string();
        self.state.global_dark_theme_name = dark.to_string();
        self.state.global_theme_mode = mode;
        self.state.global_terminal_light_accent = terminal_light_accent;
        self.state.global_terminal_dark_accent = terminal_dark_accent;
        self.state.refresh_global_palette();
        self.state.apply_effective_theme();
        self.state.settings.pending_light_theme_name = Some(light.to_string());
        self.state.settings.pending_dark_theme_name = Some(dark.to_string());
        self.state.settings.pending_theme_mode = Some(mode);
        self.state.settings.pending_terminal_light_accent = Some(terminal_light_accent);
        self.state.settings.pending_terminal_dark_accent = Some(terminal_dark_accent);
        if self.update_config_file("theme", |content| {
            let content = crate::config::remove_section_key(content, "theme", "name");
            let content = crate::config::upsert_section_value(
                &content,
                "theme",
                "light",
                &format!("\"{light}\""),
            );
            let content = crate::config::upsert_section_value(
                &content,
                "theme",
                "dark",
                &format!("\"{dark}\""),
            );
            let content = crate::config::upsert_section_value(
                &content,
                "theme",
                "mode",
                &format!("\"{}\"", mode.as_str()),
            );
            let content = crate::config::upsert_section_value(
                &content,
                "theme",
                "terminal_accent",
                &format!("\"{}\"", terminal_dark_accent.as_str()),
            );
            let content = crate::config::upsert_section_value(
                &content,
                "theme",
                "terminal_light_accent",
                &format!("\"{}\"", terminal_light_accent.as_str()),
            );
            crate::config::upsert_section_value(
                &content,
                "theme",
                "terminal_dark_accent",
                &format!("\"{}\"", terminal_dark_accent.as_str()),
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }

    pub(super) fn save_sound(&mut self, enabled: bool) {
        self.state.sound.enabled = enabled;
        self.state.settings.pending_sound_enabled = Some(enabled);
        if self.update_config_file("sound setting", |content| {
            crate::config::upsert_section_bool(content, "ui.sound", "enabled", enabled)
        }) {
            self.apply_config_from_disk(false);
        }
    }

    pub(super) fn save_new_terminal_cwd(&mut self, policy: &crate::config::NewTerminalCwdConfig) {
        self.state.new_terminal_cwd = policy.clone();
        self.state.settings.pending_new_terminal_cwd = Some(policy.clone());
        let value = match policy {
            crate::config::NewTerminalCwdConfig::Follow => "\"follow\"".to_string(),
            crate::config::NewTerminalCwdConfig::Home => "\"home\"".to_string(),
            crate::config::NewTerminalCwdConfig::Current => "\"current\"".to_string(),
            crate::config::NewTerminalCwdConfig::Path(path) => format!("{path:?}"),
        };
        if self.update_config_file("new terminal cwd", |content| {
            crate::config::upsert_section_value(content, "terminal", "new_cwd", &value)
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_mouse_scroll_lines(&mut self, lines: usize) {
        let lines = lines.max(1);
        self.state.mouse_scroll_lines = lines;
        self.state.settings.pending_mouse_scroll_lines = Some(lines);
        if self.update_config_file("mouse scroll lines", |content| {
            crate::config::upsert_section_value(
                content,
                "ui",
                "mouse_scroll_lines",
                &lines.to_string(),
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_sidebar_widths(&mut self, width: u16, min: u16, max: u16) {
        let (min, max) = crate::config::validated_sidebar_bounds(min, max)
            .unwrap_or((self.state.sidebar_min_width, self.state.sidebar_max_width));
        let width = width.clamp(min, max);
        self.state.default_sidebar_width = width;
        if self.state.sidebar_width_source == crate::app::state::SidebarWidthSource::ConfigDefault {
            self.state.sidebar_width = width;
        }
        self.state.sidebar_min_width = min;
        self.state.sidebar_max_width = max;
        self.state.sidebar_width = self.state.sidebar_width.clamp(min, max);
        self.state.settings.pending_sidebar_width = Some(width);
        self.state.settings.pending_sidebar_min_width = Some(min);
        self.state.settings.pending_sidebar_max_width = Some(max);
        if self.update_config_file("sidebar widths", |content| {
            let content = crate::config::upsert_section_value(
                content,
                "ui",
                "sidebar_width",
                &width.to_string(),
            );
            let content = crate::config::upsert_section_value(
                &content,
                "ui",
                "sidebar_min_width",
                &min.to_string(),
            );
            crate::config::upsert_section_value(
                &content,
                "ui",
                "sidebar_max_width",
                &max.to_string(),
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_sidebar_arrangement(
        &mut self,
        arrangement: crate::config::SidebarArrangementConfig,
    ) {
        self.state.sidebar_arrangement = arrangement;
        self.state.settings.pending_sidebar_arrangement = Some(arrangement);
        if self.update_config_file("sidebar arrangement", |content| {
            crate::config::upsert_section_value(
                content,
                "ui",
                "sidebar_arrangement",
                &format!("{:?}", arrangement.config_value()),
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_sidebar_initial_view(
        &mut self,
        initial_state: crate::config::SidebarInitialStateConfig,
        initial_agent_scope: crate::config::AgentPanelScopeConfig,
    ) {
        self.state.sidebar_config.initial_state = initial_state;
        self.state.sidebar_config.initial_agent_scope = initial_agent_scope;
        self.state.settings.pending_sidebar_initial_state = Some(initial_state);
        self.state.settings.pending_sidebar_initial_agent_scope = Some(initial_agent_scope);
        if self.update_config_file("initial sidebar view", |content| {
            let content = crate::config::upsert_section_value(
                content,
                "ui.sidebar",
                "initial_state",
                &format!("{:?}", initial_state.label()),
            );
            crate::config::upsert_section_value(
                &content,
                "ui.sidebar",
                "initial_agent_scope",
                &format!("{:?}", initial_agent_scope.config_value()),
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_worktree_directory(&mut self, directory: &str) {
        self.state.worktree_directory = crate::worktree::expand_tilde_absolute_path(directory);
        self.state.settings.pending_worktree_directory = Some(directory.to_string());
        if self.update_config_file("worktree directory", |content| {
            crate::config::upsert_section_value(
                content,
                "worktrees",
                "directory",
                &format!("{directory:?}"),
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_toast_delivery(&mut self, delivery: crate::config::ToastDelivery) {
        self.state.toast_config.delivery = delivery;
        self.state.settings.pending_toast_delivery = Some(delivery);
        let value = match delivery {
            crate::config::ToastDelivery::Off => "\"off\"",
            crate::config::ToastDelivery::Hako => "\"hako\"",
            crate::config::ToastDelivery::Terminal => "\"terminal\"",
            crate::config::ToastDelivery::System => "\"system\"",
        };
        if self.update_config_file("toast setting", |content| {
            let content =
                crate::config::upsert_section_value(content, "ui.toast", "delivery", value);
            crate::config::remove_section_key(&content, "ui.toast", "enabled")
        }) {
            self.apply_config_from_disk(false);
        }
    }
    pub(super) fn save_confirm_close(&mut self, enabled: bool) {
        self.state.confirm_close = enabled;
        self.state.settings.pending_confirm_close = Some(enabled);
        if self.update_config_file("close confirmation", |content| {
            crate::config::upsert_section_bool(content, "ui", "confirm_close", enabled)
        }) {
            self.apply_config_from_disk(false);
        }
    }

    pub(super) fn save_prompt_new_tab_name(&mut self, enabled: bool) {
        self.state.prompt_new_tab_name = enabled;
        self.state.settings.pending_prompt_new_tab_name = Some(enabled);
        if self.update_config_file("new tab name prompt", |content| {
            crate::config::upsert_section_bool(content, "ui", "prompt_new_tab_name", enabled)
        }) {
            self.apply_config_from_disk(false);
        }
    }

    pub(super) fn save_agent_border_labels(&mut self, enabled: bool) {
        self.state.show_agent_labels_on_pane_borders = enabled;
        self.state.settings.pending_agent_border_labels = Some(enabled);
        if self.update_config_file("agent border labels", |content| {
            crate::config::upsert_section_bool(
                content,
                "ui",
                "show_agent_labels_on_pane_borders",
                enabled,
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }

    pub(super) fn save_switch_ascii_input_source_in_prefix(&mut self, enabled: bool) {
        self.state.switch_ascii_input_source_in_prefix = enabled;
        self.state
            .settings
            .pending_switch_ascii_input_source_in_prefix = Some(enabled);
        if self.update_config_file("prefix ascii input source", |content| {
            crate::config::upsert_section_bool(
                content,
                "experimental",
                "switch_ascii_input_source_in_prefix",
                enabled,
            )
        }) {
            self.apply_config_from_disk(false);
        }
    }

    pub(super) fn save_agent_profile(
        &mut self,
        profile: crate::agent_profiles::UserAgentProfileConfig,
    ) {
        let mut config = self.current_agent_profiles_config();
        let profile_id = format!("user:{}", profile.id.trim_start_matches("user:"));
        if let Some(existing) = config.custom.iter_mut().find(|existing| {
            format!("user:{}", existing.id.trim_start_matches("user:")) == profile_id
        }) {
            *existing = profile;
        } else {
            config.custom.push(profile);
        }
        if !config.order.iter().any(|id| id == &profile_id) {
            config.order.push(profile_id);
        }
        self.save_agent_profiles_config(config);
    }

    pub(super) fn delete_agent_profile(&mut self, profile_id: &str) {
        let mut config = self.current_agent_profiles_config();
        config.custom.retain(|profile| {
            format!("user:{}", profile.id.trim_start_matches("user:")) != profile_id
        });
        config.order.retain(|id| id != profile_id);
        for group in &mut self.state.groups {
            group
                .favorite_agent_profile_ids
                .retain(|id| id != profile_id);
            if group.default_agent_profile_id.as_deref() == Some(profile_id) {
                group.default_agent_profile_id = None;
            }
        }
        self.state.mark_session_dirty();
        self.state.settings.pending_agent_profile_id = None;
        self.state.settings.pending_agent_profile_name = None;
        self.state.settings.pending_agent_profile_kind =
            Some(crate::agent_profiles::AgentKind::Omp);
        self.state.settings.pending_agent_profile_command = None;
        self.state.settings.list.selected = 0;
        self.state.settings.scroll = 0;
        self.save_agent_profiles_config(config);
    }

    fn current_agent_profiles_config(&self) -> crate::agent_profiles::AgentProfilesConfig {
        let custom = self
            .state
            .agent_profiles
            .profiles()
            .iter()
            .filter(|profile| !profile.is_system())
            .map(|profile| crate::agent_profiles::UserAgentProfileConfig {
                id: profile.id.trim_start_matches("user:").to_string(),
                name: profile.name.clone(),
                kind: profile.kind,
                command: profile.command.clone(),
                env: profile.env.iter().cloned().collect(),
                enabled: profile.enabled,
            })
            .collect();
        let order = self
            .state
            .agent_profiles
            .profiles()
            .iter()
            .map(|profile| profile.id.clone())
            .collect();
        crate::agent_profiles::AgentProfilesConfig { order, custom }
    }

    fn save_agent_profiles_config(&mut self, config: crate::agent_profiles::AgentProfilesConfig) {
        let next_catalog = crate::agent_profiles::AgentProfileCatalog::from_config(&config);
        if self.update_config_file("agent profiles", |content| {
            write_agent_profiles_section(content, &config)
        }) {
            self.apply_config_from_disk(false);
        } else {
            self.state.agent_profiles = next_catalog;
            self.refresh_integration_recommendations();
        }
    }
}

fn write_agent_profiles_section(
    content: &str,
    config: &crate::agent_profiles::AgentProfilesConfig,
) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(header) = toml_header_name(trimmed) {
            skipping = header == "agent_profiles" || header.starts_with("agent_profiles.");
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.trim().is_empty() {
        out.push('\n');
    }
    out.push_str("[agent_profiles]\n");
    if !config.order.is_empty() {
        out.push_str("order = [");
        for (idx, id) in config.order.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push('"');
            out.push_str(&escape_toml_string(id));
            out.push('"');
        }
        out.push_str("]\n");
    }
    for profile in &config.custom {
        out.push_str("\n[[agent_profiles.custom]]\n");
        out.push_str("id = \"");
        out.push_str(&escape_toml_string(&profile.id));
        out.push_str("\"\nname = \"");
        out.push_str(&escape_toml_string(&profile.name));
        out.push_str("\"\nkind = \"");
        out.push_str(profile.kind.as_str());
        out.push_str("\"\ncommand = \"");
        out.push_str(&escape_toml_string(&profile.command));
        out.push_str("\"\n");
        if !profile.enabled {
            out.push_str("enabled = false\n");
        }
        if !profile.env.is_empty() {
            out.push_str("\n[agent_profiles.custom.env]\n");
            for (key, value) in &profile.env {
                out.push_str(key);
                out.push_str(" = \"");
                out.push_str(&escape_toml_string(value));
                out.push_str("\"\n");
            }
        }
    }
    out
}

fn toml_header_name(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix("[[")
        .and_then(|value| value.strip_suffix("]]"))
        .or_else(|| {
            trimmed
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
        })
}

fn escape_toml_string(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    #[test]
    fn delete_agent_profile_closes_editor_and_updates_catalog() {
        let mut app = test_app();
        app.state.agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
            &crate::agent_profiles::AgentProfilesConfig {
                order: vec!["user:omp-mk".to_string()],
                custom: vec![crate::agent_profiles::UserAgentProfileConfig {
                    id: "omp-mk".to_string(),
                    name: "omp mk".to_string(),
                    kind: crate::agent_profiles::AgentKind::Omp,
                    command: "omp-mk".to_string(),
                    env: std::collections::BTreeMap::new(),
                    enabled: true,
                }],
            },
        );
        app.state.settings.pending_agent_profile_id = Some("user:omp-mk".to_string());
        app.state.settings.pending_agent_profile_name = Some("omp mk".to_string());
        app.state.settings.pending_agent_profile_command = Some("omp-mk".to_string());
        app.state.settings.list.selected = 12;

        app.delete_agent_profile("user:omp-mk");

        assert!(app.state.agent_profiles.get("user:omp-mk").is_none());
        assert_eq!(app.state.settings.pending_agent_profile_id, None);
        assert_eq!(app.state.settings.pending_agent_profile_name, None);
        assert_eq!(app.state.settings.pending_agent_profile_command, None);
        assert_eq!(app.state.settings.list.selected, 0);
        assert!(app.state.session_dirty);
    }
}
