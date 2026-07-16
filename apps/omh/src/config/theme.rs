use serde::{Deserialize, Serialize};
use tracing::warn;

/// Theme configuration: pick built-ins or override individual tokens.
///
/// ```toml
/// [theme]
/// mode = "system"               # system, light, dark
/// light = "catppuccin-latte"    # used in light appearance
/// dark = "catppuccin"           # used in dark appearance
/// terminal_accent = "magenta"   # fallback for terminal_light_accent/dark_accent
/// terminal_light_accent = "blue"
/// terminal_dark_accent = "magenta"
///
/// [theme.custom]                # override individual tokens on top of the base
/// accent = "#f5c2e7"
/// red = "#ff6188"
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
    /// Custom overrides — applied on top of the selected base theme.
    pub custom: Option<CustomThemeColors>,
    /// ANSI color used for Oh My Herdr's accent when following terminal colors.
    pub terminal_accent: TerminalAccent,
    /// ANSI color used for Oh My Herdr's accent when terminal colors resolve light.
    pub terminal_light_accent: Option<TerminalAccent>,
    /// ANSI color used for Oh My Herdr's accent when terminal colors resolve dark.
    pub terminal_dark_accent: Option<TerminalAccent>,
}

impl ThemeConfig {
    pub fn resolved_terminal_light_accent(&self) -> TerminalAccent {
        self.terminal_light_accent.unwrap_or(self.terminal_accent)
    }

    pub fn resolved_terminal_dark_accent(&self) -> TerminalAccent {
        self.terminal_dark_accent.unwrap_or(self.terminal_accent)
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

/// Per-token color overrides. All fields optional — only set what you want to change.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CustomThemeColors {
    pub accent: Option<String>,
    pub panel_bg: Option<String>,
    pub surface0: Option<String>,
    pub surface1: Option<String>,
    pub surface_dim: Option<String>,
    pub overlay0: Option<String>,
    pub overlay1: Option<String>,
    pub text: Option<String>,
    pub subtext0: Option<String>,
    pub mauve: Option<String>,
    pub green: Option<String>,
    pub yellow: Option<String>,
    pub red: Option<String>,
    pub blue: Option<String>,
    pub teal: Option<String>,
    pub peach: Option<String>,
}

/// Parse a color string into a ratatui Color.
/// Supports: hex (#rrggbb, #rgb), named colors, rgb(r,g,b), and reset aliases.
pub fn parse_color(s: &str) -> ratatui::style::Color {
    use ratatui::style::Color;
    let s = s.trim().to_lowercase();

    match s.as_str() {
        "reset" | "default" | "none" | "transparent" => return Color::Reset,
        _ => {}
    }

    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        } else if hex.len() == 3 {
            let chars: Vec<u8> = hex
                .chars()
                .filter_map(|c| u8::from_str_radix(&c.to_string(), 16).ok())
                .collect();
            if chars.len() == 3 {
                return Color::Rgb(chars[0] * 17, chars[1] * 17, chars[2] * 17);
            }
        }
    }

    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].trim().parse::<u8>(),
                parts[1].trim().parse::<u8>(),
                parts[2].trim().parse::<u8>(),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
    }

    match s.as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "white" => Color::White,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        _ => {
            warn!(color = s, "unknown color, defaulting to cyan");
            Color::Cyan
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
    fn parse_color_accepts_reset_aliases() {
        use ratatui::style::Color;

        for value in ["reset", "default", "none", "transparent"] {
            assert_eq!(parse_color(value), Color::Reset, "value: {value}");
        }
    }

    #[test]
    fn theme_custom_overrides_parse() {
        let toml = r##"
[theme]
name = "nord"

[theme.custom]
panel_bg = "#1e1e2e"
accent = "#ff79c6"
red = "rgb(255, 85, 85)"
"##;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.theme.name.as_deref(), Some("nord"));
        let custom = config.theme.custom.as_ref().unwrap();
        assert_eq!(custom.panel_bg.as_deref(), Some("#1e1e2e"));
        assert_eq!(custom.accent.as_deref(), Some("#ff79c6"));
        assert_eq!(custom.red.as_deref(), Some("rgb(255, 85, 85)"));
        assert!(custom.green.is_none());
    }

    #[test]
    fn theme_defaults_when_missing() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.theme.name.is_none());
        assert_eq!(config.theme.mode, ThemeMode::System);
        assert!(config.theme.custom.is_none());
        assert_eq!(config.theme.terminal_accent, TerminalAccent::Blue);
        assert_eq!(config.theme.terminal_light_accent, None);
        assert_eq!(config.theme.terminal_dark_accent, None);
    }
}
