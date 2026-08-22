use serde::{Deserialize, Serialize};

const DEFAULT_DARK_THEME_NAME: &str = "catppuccin";
const DEFAULT_LIGHT_THEME_NAME: &str = "catppuccin-latte";

const VALID_THEME_NAMES: &[&str] = &[
    "system",
    "terminal",
    "catppuccin",
    "catppuccin-latte",
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "dracula",
    "ethereal",
    "everforest",
    "flexoki",
    "flexoki-light",
    "gruvbox",
    "gruvbox-light",
    "hackerman",
    "kanagawa",
    "kanagawa-lotus",
    "last-horizon",
    "lumon",
    "matte-black",
    "miasma",
    "monokai-classic",
    "monokai-pro",
    "monokai-pro-light",
    "monokai-pro-light-sun",
    "monokai-pro-machine",
    "monokai-pro-octagon",
    "monokai-pro-ristretto",
    "monokai-pro-spectrum",
    "nord",
    "one-dark",
    "one-light",
    "osaka-jade",
    "retro-82",
    "rose-pine",
    "rose-pine-dawn",
    "solarized",
    "solarized-light",
    "solitude",
    "tokyo-night",
    "tokyo-night-day",
    "vantablack",
    "vesper",
    "white",
];

fn normalize_theme_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_'], "-")
}

fn known_theme_name(name: &str) -> bool {
    match normalize_theme_name(name).as_str() {
        "system" | "terminal" => true,
        "catppuccin" | "catppuccin-mocha" | "mocha" => true,
        "catppuccin-latte" | "latte" | "light" => true,
        "catppuccin-frappe" | "frappe" => true,
        "catppuccin-macchiato" | "macchiato" => true,
        "tokyo-night" | "tokyonight" => true,
        "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => true,
        "dracula" | "nord" => true,
        "gruvbox" | "gruvbox-dark" => true,
        "gruvbox-light" => true,
        "one-dark" | "onedark" => true,
        "one-light" | "onelight" => true,
        "solarized" | "solarized-dark" => true,
        "solarized-light" => true,
        "kanagawa" => true,
        "kanagawa-lotus" | "lotus" => true,
        "rose-pine" | "rosepine" => true,
        "rose-pine-dawn" | "rosepine-dawn" | "dawn" => true,
        "vesper" | "ethereal" | "everforest" | "flexoki" | "flexoki-light" => true,
        "hackerman" | "last-horizon" | "lumon" | "matte-black" | "miasma" => true,
        "monokai-pro" | "monokai" => true,
        "monokai-pro-light" | "monokai-light" => true,
        "monokai-pro-light-sun" | "monokai-pro-sun" | "monokai-sun" | "sun" => true,
        "monokai-pro-spectrum" | "monokai-spectrum" | "spectrum" => true,
        "monokai-pro-ristretto" | "monokai-ristretto" | "ristretto" => true,
        "monokai-pro-octagon" | "monokai-octagon" | "octagon" => true,
        "monokai-pro-machine" | "monokai-machine" | "machine" => true,
        "monokai-classic" | "classic" => true,
        "osaka-jade" | "retro-82" | "solitude" | "vantablack" | "white" => true,
        _ => VALID_THEME_NAMES.contains(&normalize_theme_name(name).as_str()),
    }
}

/// Theme configuration: pick a built-in palette and terminal accents.
///
/// ```toml
/// [theme]
/// mode = "system"               # system, light, dark
/// light = "catppuccin-latte"    # used in light appearance
/// dark = "catppuccin"           # used in dark appearance
/// terminal_accent = "magenta"   # fallback for terminal_light_accent/dark_accent
/// terminal_light_accent = "blue"
/// terminal_dark_accent = "magenta"
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Built-in theme name. Default: "catppuccin".
    pub name: Option<String>,
    /// Built-in light theme used when mode resolves to light.
    pub light: Option<String>,
    /// Built-in dark theme used when mode resolves to dark.
    pub dark: Option<String>,
    /// Light/dark resolution mode. Default: "system".
    pub mode: ThemeMode,
    /// ANSI color used for Gardn's accent when following terminal colors.
    pub terminal_accent: TerminalAccent,
    /// ANSI color used for Gardn's accent when terminal colors resolve light.
    pub terminal_light_accent: Option<TerminalAccent>,
    /// ANSI color used for Gardn's accent when terminal colors resolve dark.
    pub terminal_dark_accent: Option<TerminalAccent>,
}

impl ThemeConfig {
    pub fn resolved_terminal_light_accent(&self) -> TerminalAccent {
        self.terminal_light_accent.unwrap_or(self.terminal_accent)
    }

    pub fn resolved_terminal_dark_accent(&self) -> TerminalAccent {
        self.terminal_dark_accent.unwrap_or(self.terminal_accent)
    }

    pub(crate) fn diagnostics(&self) -> Vec<String> {
        let valid = VALID_THEME_NAMES.join(", ");
        [
            ("theme.name", self.name.as_deref(), DEFAULT_DARK_THEME_NAME),
            ("theme.dark", self.dark.as_deref(), DEFAULT_DARK_THEME_NAME),
            (
                "theme.light",
                self.light.as_deref(),
                DEFAULT_LIGHT_THEME_NAME,
            ),
        ]
        .into_iter()
        .filter_map(|(field, value, fallback)| {
            let value = value?;
            (!known_theme_name(value)).then(|| {
                format!(
                    "unknown theme name {field} = {value:?}; using {fallback:?}; valid themes: {valid}"
                )
            })
        })
        .collect()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: &[Self] = &[Self::System, Self::Light, Self::Dark];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn resolve(
        self,
        host_theme: crate::terminal_theme::TerminalTheme,
    ) -> crate::terminal_theme::ThemeAppearance {
        match self {
            Self::Light => crate::terminal_theme::ThemeAppearance::Light,
            Self::Dark => crate::terminal_theme::ThemeAppearance::Dark,
            Self::System => host_theme
                .appearance()
                .unwrap_or(crate::terminal_theme::ThemeAppearance::Dark),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalAccent {
    #[default]
    Blue,
    Magenta,
    Cyan,
    Green,
    Yellow,
    Red,
}

impl TerminalAccent {
    pub const ALL: &[Self] = &[
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::Green,
        Self::Yellow,
        Self::Red,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Magenta => "magenta",
            Self::Cyan => "cyan",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Magenta => "Magenta",
            Self::Cyan => "Cyan",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
            Self::Red => "Red",
        }
    }

    pub fn ansi_index(self) -> usize {
        match self {
            Self::Blue => 4,
            Self::Magenta => 5,
            Self::Cyan => 6,
            Self::Green => 2,
            Self::Yellow => 3,
            Self::Red => 1,
        }
    }

    pub fn fallback_color(self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            Self::Blue => Color::Blue,
            Self::Magenta => Color::Magenta,
            Self::Cyan => Color::Cyan,
            Self::Green => Color::Green,
            Self::Yellow => Color::Yellow,
            Self::Red => Color::LightRed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn theme_name_parses() {
        let toml = r#"
[theme]
name = "dracula"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.theme.name.as_deref(), Some("dracula"));
        assert_eq!(config.theme.mode, ThemeMode::System);
    }

    #[test]
    fn theme_mode_parses() {
        let toml = r#"
[theme]
mode = "light"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.theme.mode, ThemeMode::Light);
    }

    #[test]
    fn theme_light_and_dark_names_parse() {
        let toml = r#"
[theme]
mode = "system"
light = "solarized-light"
dark = "rose-pine"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.theme.mode, ThemeMode::System);
        assert_eq!(config.theme.light.as_deref(), Some("solarized-light"));
        assert_eq!(config.theme.dark.as_deref(), Some("rose-pine"));
    }

    #[test]
    fn terminal_accent_parses() {
        let toml = r#"
[theme]
terminal_accent = "magenta"
terminal_light_accent = "cyan"
terminal_dark_accent = "red"
"#;
        let config: Config = toml::from_str(toml).unwrap();

        assert_eq!(config.theme.terminal_accent, TerminalAccent::Magenta);
        assert_eq!(
            config.theme.resolved_terminal_light_accent(),
            TerminalAccent::Cyan
        );
        assert_eq!(
            config.theme.resolved_terminal_dark_accent(),
            TerminalAccent::Red
        );
    }

    #[test]
    fn terminal_accent_falls_back_for_light_and_dark() {
        let toml = r#"
[theme]
terminal_accent = "yellow"
"#;
        let config: Config = toml::from_str(toml).unwrap();

        assert_eq!(
            config.theme.resolved_terminal_light_accent(),
            TerminalAccent::Yellow
        );
        assert_eq!(
            config.theme.resolved_terminal_dark_accent(),
            TerminalAccent::Yellow
        );
    }

    #[test]
    fn theme_defaults_when_missing() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.theme.name.is_none());
        assert_eq!(config.theme.mode, ThemeMode::System);
        assert_eq!(config.theme.terminal_accent, TerminalAccent::Blue);
        assert_eq!(config.theme.terminal_light_accent, None);
        assert_eq!(config.theme.terminal_dark_accent, None);
    }

    #[test]
    fn unknown_theme_names_are_diagnosed() {
        let config: Config = toml::from_str(
            r#"
[theme]
name = "catppucin"
dark = "tokio-night"
light = "lattee"
"#,
        )
        .unwrap();

        let diagnostics = config.theme.diagnostics();
        assert_eq!(diagnostics.len(), 3);
        assert!(diagnostics[0].contains("theme.name = \"catppucin\""));
        assert!(diagnostics[0].contains("using \"catppuccin\""));
        assert!(diagnostics[1].contains("theme.dark = \"tokio-night\""));
        assert!(diagnostics[2].contains("theme.light = \"lattee\""));
        assert!(diagnostics[2].contains("using \"catppuccin-latte\""));
    }

    #[test]
    fn theme_name_aliases_are_valid() {
        for name in ["catppuccin-mocha", "tokyonight", "gruvbox-dark", "dawn"] {
            assert!(known_theme_name(name), "alias: {name}");
        }
    }
}
