use crate::config::{
    CustomThemeColors, Keybinds, SoundConfig, ThemeMode, ToastConfig, ToastDelivery,
};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Direction, Rect};
use ratatui::style::Color;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::layout::{PaneId, PaneInfo, SplitBorder};
use crate::selection::Selection;
use crate::terminal_theme::{TerminalTheme, ThemeAppearance};
use crate::workspace::Workspace;

static NEXT_GROUP_ID: AtomicU64 = AtomicU64::new(1);

pub const DEFAULT_GROUP_ICON: &str = "●";
pub const GROUP_ICONS: &[&str] = &[
    "●", "◆", "■", "▲", "○", "◇", "□", "△", "✦", "✚", "*", "#", "@", "+", "~", "=", "$", "%", "&",
    "?",
];

pub(crate) fn generate_group_id() -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or(0);
    let counter = NEXT_GROUP_ID.fetch_add(1, Ordering::Relaxed);
    format!("g{micros:x}{counter:x}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub theme_name: Option<String>,
}

impl Group {
    pub fn default_group() -> Self {
        Self {
            id: crate::workspace::DEFAULT_GROUP_ID.to_string(),
            name: "group 1".to_string(),
            icon: DEFAULT_GROUP_ICON.to_string(),
            theme_name: None,
        }
    }
}

pub fn normalize_group_icon(icon: &str) -> String {
    GROUP_ICONS
        .iter()
        .copied()
        .find(|candidate| *candidate == icon)
        .unwrap_or(DEFAULT_GROUP_ICON)
        .to_string()
}

// ---------------------------------------------------------------------------
// Theme palette — all UI colors in one place, ready for theming
// ---------------------------------------------------------------------------

/// All colors used by the UI. Derived from a base accent color for now,
/// but structured so a full theme system can replace it later.
#[derive(Clone)]
#[allow(dead_code)] // all fields defined for theming — some used later
pub struct Palette {
    /// Primary accent (highlight, active borders).
    pub accent: Color,
    /// Background for floating panels, overlays, and modals.
    pub panel_bg: Color,
    /// Subtle surface background for selected/focused items.
    pub surface0: Color,
    /// Slightly lighter surface for hover/active states.
    pub surface1: Color,
    /// Very dim surface for separators.
    pub surface_dim: Color,
    /// Muted text (secondary info, numbers).
    pub overlay0: Color,
    /// Slightly brighter overlay text.
    pub overlay1: Color,
    /// Main text color — soft white.
    pub text: Color,
    /// Subdued text (workspace numbers, dim labels).
    pub subtext0: Color,
    /// Branch name / special label color.
    pub mauve: Color,
    /// Done / idle states.
    pub green: Color,
    /// Working / running states.
    pub yellow: Color,
    /// Needs attention / blocked states.
    pub red: Color,
    /// Unseen / done notification accent.
    pub blue: Color,
    /// Notification accent / unseen markers.
    pub teal: Color,
    /// Interrupted / warning states.
    pub peach: Color,
}

impl Palette {
    /// Catppuccin Mocha — the default.
    pub fn catppuccin() -> Self {
        Self {
            accent: Color::Rgb(137, 180, 250), // blue
            panel_bg: Color::Rgb(24, 24, 37),
            surface0: Color::Rgb(49, 50, 68),
            surface1: Color::Rgb(69, 71, 90),
            surface_dim: Color::Rgb(30, 30, 46),
            overlay0: Color::Rgb(108, 112, 134),
            overlay1: Color::Rgb(127, 132, 156),
            text: Color::Rgb(205, 214, 244),
            subtext0: Color::Rgb(166, 173, 200),
            mauve: Color::Rgb(203, 166, 247),
            green: Color::Rgb(166, 227, 161),
            yellow: Color::Rgb(249, 226, 175),
            red: Color::Rgb(243, 139, 168),
            blue: Color::Rgb(137, 180, 250),
            teal: Color::Rgb(148, 226, 213),
            peach: Color::Rgb(250, 179, 135),
        }
    }

    /// System — respect the host terminal defaults for chrome background and foreground.
    pub fn system(
        host_theme: TerminalTheme,
        appearance: crate::terminal_theme::ThemeAppearance,
    ) -> Self {
        let base = match appearance {
            ThemeAppearance::Light => Self::catppuccin_light(),
            ThemeAppearance::Dark => Self::catppuccin(),
        };
        let host_fg = host_theme.foreground.map(Self::terminal_color);
        let host_bg = host_theme.background.map(Self::terminal_color);

        Self {
            accent: host_fg.unwrap_or(base.accent),
            panel_bg: Color::Reset,
            surface0: host_bg
                .map(|color| Self::surface_from_background(color, appearance, 0.08))
                .unwrap_or(base.surface0),
            surface1: host_bg
                .map(|color| Self::surface_from_background(color, appearance, 0.14))
                .unwrap_or(base.surface1),
            surface_dim: host_bg
                .map(|color| Self::surface_from_background(color, appearance, 0.05))
                .unwrap_or(base.surface_dim),
            overlay0: base.overlay0,
            overlay1: base.overlay1,
            text: host_fg.unwrap_or(Color::Reset),
            subtext0: host_fg.unwrap_or(base.subtext0),
            mauve: base.mauve,
            green: base.green,
            yellow: base.yellow,
            red: base.red,
            blue: base.blue,
            teal: base.teal,
            peach: base.peach,
        }
    }

    fn terminal_color(color: crate::terminal_theme::RgbColor) -> Color {
        Color::Rgb(color.r, color.g, color.b)
    }

    fn surface_from_background(
        color: Color,
        appearance: crate::terminal_theme::ThemeAppearance,
        amount: f32,
    ) -> Color {
        let Color::Rgb(r, g, b) = color else {
            return color;
        };
        let adjust = |channel: u8| -> u8 {
            let channel = channel as f32;
            let value = match appearance {
                ThemeAppearance::Light => channel * (1.0 - amount),
                ThemeAppearance::Dark => channel + (255.0 - channel) * amount,
            };
            value.round().clamp(0.0, 255.0) as u8
        };
        Color::Rgb(adjust(r), adjust(g), adjust(b))
    }

    /// Catppuccin Latte.
    pub fn catppuccin_light() -> Self {
        Self {
            accent: Color::Rgb(30, 102, 245),
            panel_bg: Color::Rgb(239, 241, 245),
            surface0: Color::Rgb(204, 208, 218),
            surface1: Color::Rgb(188, 192, 204),
            surface_dim: Color::Rgb(220, 224, 232),
            overlay0: Color::Rgb(108, 111, 133),
            overlay1: Color::Rgb(92, 95, 119),
            text: Color::Rgb(76, 79, 105),
            subtext0: Color::Rgb(108, 111, 133),
            mauve: Color::Rgb(136, 57, 239),
            green: Color::Rgb(64, 160, 43),
            yellow: Color::Rgb(223, 142, 29),
            red: Color::Rgb(210, 15, 57),
            blue: Color::Rgb(30, 102, 245),
            teal: Color::Rgb(23, 146, 153),
            peach: Color::Rgb(254, 100, 11),
        }
    }

    pub fn catppuccin_latte() -> Self {
        Self::catppuccin_light()
    }

    /// Tokyo Night — blue-purple aesthetic.
    pub fn tokyo_night() -> Self {
        Self {
            accent: Color::Rgb(122, 162, 247), // blue
            panel_bg: Color::Rgb(26, 27, 38),
            surface0: Color::Rgb(36, 40, 59),
            surface1: Color::Rgb(65, 72, 104),
            surface_dim: Color::Rgb(26, 27, 38),
            overlay0: Color::Rgb(86, 95, 137),
            overlay1: Color::Rgb(105, 113, 150),
            text: Color::Rgb(192, 202, 245),
            subtext0: Color::Rgb(169, 177, 214),
            mauve: Color::Rgb(187, 154, 247),
            green: Color::Rgb(158, 206, 106),
            yellow: Color::Rgb(224, 175, 104),
            red: Color::Rgb(247, 118, 142),
            blue: Color::Rgb(122, 162, 247),
            teal: Color::Rgb(125, 207, 255),
            peach: Color::Rgb(255, 158, 100),
        }
    }

    /// Tokyo Night Day.
    pub fn tokyo_night_light() -> Self {
        Self {
            accent: Color::Rgb(52, 84, 138),
            panel_bg: Color::Rgb(213, 214, 219),
            surface0: Color::Rgb(188, 189, 194),
            surface1: Color::Rgb(172, 173, 178),
            surface_dim: Color::Rgb(203, 204, 209),
            overlay0: Color::Rgb(116, 124, 149),
            overlay1: Color::Rgb(97, 103, 125),
            text: Color::Rgb(52, 59, 88),
            subtext0: Color::Rgb(86, 95, 137),
            mauve: Color::Rgb(90, 74, 120),
            green: Color::Rgb(72, 94, 48),
            yellow: Color::Rgb(143, 94, 21),
            red: Color::Rgb(140, 67, 81),
            blue: Color::Rgb(52, 84, 138),
            teal: Color::Rgb(51, 99, 122),
            peach: Color::Rgb(150, 80, 39),
        }
    }

    pub fn tokyo_night_day() -> Self {
        Self::tokyo_night_light()
    }

    /// Dracula — purple/pink/green.
    pub fn dracula() -> Self {
        Self {
            accent: Color::Rgb(189, 147, 249), // purple
            panel_bg: Color::Rgb(40, 42, 54),
            surface0: Color::Rgb(68, 71, 90),
            surface1: Color::Rgb(98, 114, 164),
            surface_dim: Color::Rgb(40, 42, 54),
            overlay0: Color::Rgb(98, 114, 164),
            overlay1: Color::Rgb(130, 140, 180),
            text: Color::Rgb(248, 248, 242),
            subtext0: Color::Rgb(210, 210, 220),
            mauve: Color::Rgb(255, 121, 198), // pink
            green: Color::Rgb(80, 250, 123),
            yellow: Color::Rgb(241, 250, 140),
            red: Color::Rgb(255, 85, 85),
            blue: Color::Rgb(139, 233, 253), // cyan-ish
            teal: Color::Rgb(139, 233, 253),
            peach: Color::Rgb(255, 184, 108),
        }
    }

    /// Nord — frosty blue palette.
    pub fn nord() -> Self {
        Self {
            accent: Color::Rgb(136, 192, 208), // frost
            panel_bg: Color::Rgb(46, 52, 64),
            surface0: Color::Rgb(59, 66, 82),
            surface1: Color::Rgb(67, 76, 94),
            surface_dim: Color::Rgb(46, 52, 64),
            overlay0: Color::Rgb(76, 86, 106),
            overlay1: Color::Rgb(100, 110, 130),
            text: Color::Rgb(236, 239, 244),
            subtext0: Color::Rgb(216, 222, 233),
            mauve: Color::Rgb(180, 142, 173),
            green: Color::Rgb(163, 190, 140),
            yellow: Color::Rgb(235, 203, 139),
            red: Color::Rgb(191, 97, 106),
            blue: Color::Rgb(129, 161, 193),
            teal: Color::Rgb(143, 188, 187),
            peach: Color::Rgb(208, 135, 112),
        }
    }

    /// Gruvbox Dark — warm retro palette.
    pub fn gruvbox() -> Self {
        Self {
            accent: Color::Rgb(215, 153, 33), // yellow
            panel_bg: Color::Rgb(40, 40, 40),
            surface0: Color::Rgb(60, 56, 54),
            surface1: Color::Rgb(80, 73, 69),
            surface_dim: Color::Rgb(40, 40, 40),
            overlay0: Color::Rgb(146, 131, 116),
            overlay1: Color::Rgb(168, 153, 132),
            text: Color::Rgb(235, 219, 178),
            subtext0: Color::Rgb(213, 196, 161),
            mauve: Color::Rgb(211, 134, 155),
            green: Color::Rgb(184, 187, 38),
            yellow: Color::Rgb(250, 189, 47),
            red: Color::Rgb(251, 73, 52),
            blue: Color::Rgb(131, 165, 152),
            teal: Color::Rgb(142, 192, 124),
            peach: Color::Rgb(254, 128, 25),
        }
    }

    /// Gruvbox Light.
    pub fn gruvbox_light() -> Self {
        Self {
            accent: Color::Rgb(181, 118, 20),
            panel_bg: Color::Rgb(251, 241, 199),
            surface0: Color::Rgb(235, 219, 178),
            surface1: Color::Rgb(213, 196, 161),
            surface_dim: Color::Rgb(242, 229, 188),
            overlay0: Color::Rgb(146, 131, 116),
            overlay1: Color::Rgb(124, 111, 100),
            text: Color::Rgb(60, 56, 54),
            subtext0: Color::Rgb(80, 73, 69),
            mauve: Color::Rgb(177, 98, 134),
            green: Color::Rgb(121, 116, 14),
            yellow: Color::Rgb(181, 118, 20),
            red: Color::Rgb(157, 0, 6),
            blue: Color::Rgb(7, 102, 120),
            teal: Color::Rgb(66, 123, 88),
            peach: Color::Rgb(175, 58, 3),
        }
    }

    /// One Dark — Atom's classic dark theme.
    pub fn one_dark() -> Self {
        Self {
            accent: Color::Rgb(97, 175, 239), // blue
            panel_bg: Color::Rgb(40, 44, 52),
            surface0: Color::Rgb(44, 49, 58),
            surface1: Color::Rgb(62, 68, 81),
            surface_dim: Color::Rgb(40, 44, 52),
            overlay0: Color::Rgb(92, 99, 112),
            overlay1: Color::Rgb(115, 122, 135),
            text: Color::Rgb(171, 178, 191),
            subtext0: Color::Rgb(150, 156, 168),
            mauve: Color::Rgb(198, 120, 221),
            green: Color::Rgb(152, 195, 121),
            yellow: Color::Rgb(229, 192, 123),
            red: Color::Rgb(224, 108, 117),
            blue: Color::Rgb(97, 175, 239),
            teal: Color::Rgb(86, 182, 194),
            peach: Color::Rgb(209, 154, 102),
        }
    }

    /// Atom One Light.
    pub fn one_light() -> Self {
        Self {
            accent: Color::Rgb(64, 120, 242),
            panel_bg: Color::Rgb(250, 250, 250),
            surface0: Color::Rgb(230, 230, 230),
            surface1: Color::Rgb(210, 210, 210),
            surface_dim: Color::Rgb(238, 238, 238),
            overlay0: Color::Rgb(160, 161, 167),
            overlay1: Color::Rgb(105, 108, 117),
            text: Color::Rgb(56, 58, 66),
            subtext0: Color::Rgb(92, 99, 112),
            mauve: Color::Rgb(166, 38, 164),
            green: Color::Rgb(80, 161, 79),
            yellow: Color::Rgb(193, 132, 1),
            red: Color::Rgb(228, 86, 73),
            blue: Color::Rgb(64, 120, 242),
            teal: Color::Rgb(1, 132, 188),
            peach: Color::Rgb(152, 104, 1),
        }
    }

    /// Solarized Dark — Ethan Schoonover's classic.
    pub fn solarized() -> Self {
        Self {
            accent: Color::Rgb(38, 139, 210), // blue
            panel_bg: Color::Rgb(0, 43, 54),
            surface0: Color::Rgb(7, 54, 66),
            surface1: Color::Rgb(88, 110, 117),
            surface_dim: Color::Rgb(0, 43, 54),
            overlay0: Color::Rgb(88, 110, 117),
            overlay1: Color::Rgb(101, 123, 131),
            text: Color::Rgb(147, 161, 161),
            subtext0: Color::Rgb(131, 148, 150),
            mauve: Color::Rgb(211, 54, 130),
            green: Color::Rgb(133, 153, 0),
            yellow: Color::Rgb(181, 137, 0),
            red: Color::Rgb(220, 50, 47),
            blue: Color::Rgb(38, 139, 210),
            teal: Color::Rgb(42, 161, 152),
            peach: Color::Rgb(203, 75, 22),
        }
    }

    /// Solarized Light.
    pub fn solarized_light() -> Self {
        Self {
            accent: Color::Rgb(38, 139, 210),
            panel_bg: Color::Rgb(253, 246, 227),
            surface0: Color::Rgb(238, 232, 213),
            surface1: Color::Rgb(147, 161, 161),
            surface_dim: Color::Rgb(246, 239, 219),
            overlay0: Color::Rgb(147, 161, 161),
            overlay1: Color::Rgb(101, 123, 131),
            text: Color::Rgb(88, 110, 117),
            subtext0: Color::Rgb(101, 123, 131),
            mauve: Color::Rgb(211, 54, 130),
            green: Color::Rgb(133, 153, 0),
            yellow: Color::Rgb(181, 137, 0),
            red: Color::Rgb(220, 50, 47),
            blue: Color::Rgb(38, 139, 210),
            teal: Color::Rgb(42, 161, 152),
            peach: Color::Rgb(203, 75, 22),
        }
    }

    /// Kanagawa — inspired by Katsushika Hokusai.
    pub fn kanagawa() -> Self {
        Self {
            accent: Color::Rgb(126, 156, 216), // blue
            panel_bg: Color::Rgb(31, 31, 40),
            surface0: Color::Rgb(42, 42, 55),
            surface1: Color::Rgb(54, 54, 70),
            surface_dim: Color::Rgb(31, 31, 40),
            overlay0: Color::Rgb(114, 113, 105),
            overlay1: Color::Rgb(135, 134, 125),
            text: Color::Rgb(220, 215, 186),
            subtext0: Color::Rgb(200, 195, 170),
            mauve: Color::Rgb(149, 127, 184),
            green: Color::Rgb(118, 148, 106),
            yellow: Color::Rgb(192, 163, 110),
            red: Color::Rgb(195, 64, 67),
            blue: Color::Rgb(126, 156, 216),
            teal: Color::Rgb(127, 180, 202),
            peach: Color::Rgb(255, 160, 102),
        }
    }

    /// Kanagawa Lotus — the light Kanagawa variant.
    pub fn kanagawa_lotus() -> Self {
        Self {
            accent: Color::Rgb(77, 105, 155),
            panel_bg: Color::Rgb(242, 236, 188),
            surface0: Color::Rgb(220, 213, 172),
            surface1: Color::Rgb(201, 203, 209),
            surface_dim: Color::Rgb(213, 206, 163),
            overlay0: Color::Rgb(160, 156, 172),
            overlay1: Color::Rgb(138, 137, 128),
            text: Color::Rgb(84, 84, 100),
            subtext0: Color::Rgb(67, 67, 108),
            mauve: Color::Rgb(98, 76, 131),
            green: Color::Rgb(111, 137, 78),
            yellow: Color::Rgb(119, 113, 63),
            red: Color::Rgb(200, 64, 83),
            blue: Color::Rgb(77, 105, 155),
            teal: Color::Rgb(78, 140, 162),
            peach: Color::Rgb(204, 109, 0),
        }
    }

    /// Rosé Pine — muted, elegant.
    pub fn rose_pine() -> Self {
        Self {
            accent: Color::Rgb(196, 167, 231), // iris
            panel_bg: Color::Rgb(25, 23, 36),
            surface0: Color::Rgb(31, 29, 46),
            surface1: Color::Rgb(38, 35, 58),
            surface_dim: Color::Rgb(25, 23, 36),
            overlay0: Color::Rgb(110, 106, 134),
            overlay1: Color::Rgb(144, 140, 170),
            text: Color::Rgb(224, 222, 244),
            subtext0: Color::Rgb(200, 197, 220),
            mauve: Color::Rgb(196, 167, 231),  // iris
            green: Color::Rgb(49, 116, 143),   // pine
            yellow: Color::Rgb(246, 193, 119), // gold
            red: Color::Rgb(235, 111, 146),    // love
            blue: Color::Rgb(49, 116, 143),    // pine
            teal: Color::Rgb(156, 207, 216),   // foam
            peach: Color::Rgb(234, 154, 151),  // rose
        }
    }

    /// Rose Pine Dawn.
    pub fn rose_pine_light() -> Self {
        Self {
            accent: Color::Rgb(144, 122, 169),
            panel_bg: Color::Rgb(250, 244, 237),
            surface0: Color::Rgb(242, 233, 222),
            surface1: Color::Rgb(223, 218, 217),
            surface_dim: Color::Rgb(246, 238, 229),
            overlay0: Color::Rgb(152, 147, 165),
            overlay1: Color::Rgb(121, 117, 147),
            text: Color::Rgb(87, 82, 121),
            subtext0: Color::Rgb(110, 106, 134),
            mauve: Color::Rgb(144, 122, 169),
            green: Color::Rgb(40, 105, 131),
            yellow: Color::Rgb(234, 157, 52),
            red: Color::Rgb(180, 99, 122),
            blue: Color::Rgb(40, 105, 131),
            teal: Color::Rgb(86, 148, 159),
            peach: Color::Rgb(215, 130, 126),
        }
    }

    pub fn rose_pine_dawn() -> Self {
        Self::rose_pine_light()
    }

    /// Vesper — minimal high-contrast monochrome with peach and mint accents.
    pub fn vesper() -> Self {
        Self {
            accent: Color::Rgb(255, 199, 153),
            panel_bg: Color::Rgb(26, 26, 26),
            surface0: Color::Rgb(35, 35, 35),
            surface1: Color::Rgb(40, 40, 40),
            surface_dim: Color::Rgb(16, 16, 16),
            overlay0: Color::Rgb(92, 92, 92),
            overlay1: Color::Rgb(126, 126, 126),
            text: Color::Rgb(255, 255, 255),
            subtext0: Color::Rgb(160, 160, 160),
            mauve: Color::Rgb(255, 209, 168),
            green: Color::Rgb(153, 255, 228),
            yellow: Color::Rgb(255, 199, 153),
            red: Color::Rgb(255, 128, 128),
            blue: Color::Rgb(176, 176, 176),
            teal: Color::Rgb(102, 221, 204),
            peach: Color::Rgb(255, 199, 153),
        }
    }

    /// Resolve a theme by name. Returns None for unknown names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().replace([' ', '_'], "-").as_str() {
            "catppuccin" | "catppuccin-mocha" => Some(Self::catppuccin()),
            "catppuccin-latte" | "latte" | "light" => Some(Self::catppuccin_latte()),
            "tokyo-night" | "tokyonight" => Some(Self::tokyo_night()),
            "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => Some(Self::tokyo_night_day()),
            "dracula" => Some(Self::dracula()),
            "nord" => Some(Self::nord()),
            "gruvbox" | "gruvbox-dark" => Some(Self::gruvbox()),
            "gruvbox-light" => Some(Self::gruvbox_light()),
            "one-dark" | "onedark" => Some(Self::one_dark()),
            "one-light" | "onelight" => Some(Self::one_light()),
            "solarized" | "solarized-dark" => Some(Self::solarized()),
            "solarized-light" => Some(Self::solarized_light()),
            "kanagawa" => Some(Self::kanagawa()),
            "kanagawa-lotus" | "lotus" => Some(Self::kanagawa_lotus()),
            "rose-pine" | "rosepine" => Some(Self::rose_pine()),
            "rose-pine-dawn" | "rosepine-dawn" | "dawn" => Some(Self::rose_pine_dawn()),
            "vesper" => Some(Self::vesper()),
            _ => None,
        }
    }

    pub fn from_theme(
        name: &str,
        appearance: crate::terminal_theme::ThemeAppearance,
    ) -> Option<Self> {
        Self::from_theme_with_terminal(name, appearance, TerminalTheme::default())
    }

    pub fn from_theme_with_terminal(
        name: &str,
        appearance: crate::terminal_theme::ThemeAppearance,
        host_theme: TerminalTheme,
    ) -> Option<Self> {
        let normalized = name.to_lowercase().replace([' ', '_'], "-");
        if normalized == "system" {
            return Some(Self::system(host_theme, appearance));
        }
        if appearance == crate::terminal_theme::ThemeAppearance::Dark {
            return Self::from_name(&normalized);
        }

        match normalized.as_str() {
            "catppuccin" | "catppuccin-mocha" => Some(Self::catppuccin_light()),
            "tokyo-night" | "tokyonight" => Some(Self::tokyo_night_light()),
            "gruvbox" | "gruvbox-dark" => Some(Self::gruvbox_light()),
            "one-dark" | "onedark" => Some(Self::one_light()),
            "solarized" | "solarized-dark" => Some(Self::solarized_light()),
            "kanagawa" => Some(Self::kanagawa_lotus()),
            "rose-pine" | "rosepine" => Some(Self::rose_pine_light()),
            _ => Self::from_name(&normalized),
        }
    }

    /// Apply custom color overrides on top of this palette.
    pub fn with_overrides(mut self, custom: &crate::config::CustomThemeColors) -> Self {
        use crate::config::parse_color;
        if let Some(c) = &custom.accent {
            self.accent = parse_color(c);
        }
        if let Some(c) = &custom.panel_bg {
            self.panel_bg = parse_color(c);
        }
        if let Some(c) = &custom.surface0 {
            self.surface0 = parse_color(c);
        }
        if let Some(c) = &custom.surface1 {
            self.surface1 = parse_color(c);
        }
        if let Some(c) = &custom.surface_dim {
            self.surface_dim = parse_color(c);
        }
        if let Some(c) = &custom.overlay0 {
            self.overlay0 = parse_color(c);
        }
        if let Some(c) = &custom.overlay1 {
            self.overlay1 = parse_color(c);
        }
        if let Some(c) = &custom.text {
            self.text = parse_color(c);
        }
        if let Some(c) = &custom.subtext0 {
            self.subtext0 = parse_color(c);
        }
        if let Some(c) = &custom.mauve {
            self.mauve = parse_color(c);
        }
        if let Some(c) = &custom.green {
            self.green = parse_color(c);
        }
        if let Some(c) = &custom.yellow {
            self.yellow = parse_color(c);
        }
        if let Some(c) = &custom.red {
            self.red = parse_color(c);
        }
        if let Some(c) = &custom.blue {
            self.blue = parse_color(c);
        }
        if let Some(c) = &custom.teal {
            self.teal = parse_color(c);
        }
        if let Some(c) = &custom.peach {
            self.peach = parse_color(c);
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceCardArea {
    pub ws_idx: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceGroupHeaderArea {
    pub group_idx: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceGroupEmptyArea {
    pub group_idx: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceGroupDropArea {
    pub group_idx: usize,
    pub insert_idx: usize,
    pub rect: Rect,
}

/// Computed view geometry — derived from AppState + terminal size.
/// Updated before each render, consumed by render and mouse handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewLayout {
    Desktop,
    Mobile,
}

pub struct ViewState {
    pub layout: ViewLayout,
    pub sidebar_rect: Rect,
    pub right_sidebar_rect: Rect,
    pub workspace_card_areas: Vec<WorkspaceCardArea>,
    pub workspace_group_header_areas: Vec<WorkspaceGroupHeaderArea>,
    pub workspace_group_empty_areas: Vec<WorkspaceGroupEmptyArea>,
    pub workspace_group_drop_areas: Vec<WorkspaceGroupDropArea>,
    pub tab_bar_rect: Rect,
    pub tab_hit_areas: Vec<Rect>,
    pub tab_scroll_left_hit_area: Rect,
    pub tab_scroll_right_hit_area: Rect,
    pub new_tab_hit_area: Rect,
    pub terminal_area: Rect,
    pub mobile_header_rect: Rect,
    pub mobile_menu_hit_area: Rect,
    pub toast_hit_area: Rect,
    pub pane_infos: Vec<PaneInfo>,
    pub split_borders: Vec<SplitBorder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Onboarding,
    ReleaseNotes,
    Navigate,
    Terminal,
    RenameWorkspace,
    RenameGroup,
    RenameTab,
    RenamePane,
    Resize,
    ConfirmClose,
    ConfirmDeleteGroup,
    ContextMenu,
    Settings,
    GlobalMenu,
    GroupMenu,
    AgentMenu,
    KeybindHelp,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AgentPanelScope {
    #[default]
    CurrentWorkspace,
    CurrentGroup,
    AllWorkspaces,
}

// ---------------------------------------------------------------------------
// Settings UI state
// ---------------------------------------------------------------------------

/// Which section of the settings panel is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    ThemeMode,
    Theme,
    Sound,
    Toast,
    PaneLabels,
}

impl SettingsSection {
    pub const ALL: &[Self] = &[
        Self::ThemeMode,
        Self::Theme,
        Self::Sound,
        Self::Toast,
        Self::PaneLabels,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ThemeMode => "mode",
            Self::Theme => "theme",
            Self::Sound => "sound",
            Self::Toast => "toasts",
            Self::PaneLabels => "pane labels",
        }
    }
}

/// All built-in theme names in display order.
pub const THEME_NAMES: &[&str] = &[
    "system",
    "catppuccin",
    "tokyo-night",
    "dracula",
    "nord",
    "gruvbox",
    "one-dark",
    "solarized",
    "kanagawa",
    "rose-pine",
    "vesper",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuListState {
    pub highlighted: usize,
}

impl MenuListState {
    pub fn new(highlighted: usize) -> Self {
        Self { highlighted }
    }

    pub fn move_prev(&mut self) {
        self.highlighted = self.highlighted.saturating_sub(1);
    }

    pub fn move_next(&mut self, item_count: usize) {
        if item_count > 0 {
            self.highlighted = (self.highlighted + 1).min(item_count - 1);
        }
    }

    pub fn hover(&mut self, idx: Option<usize>) {
        if let Some(idx) = idx {
            self.highlighted = idx;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionListState {
    pub selected: usize,
}

impl SelectionListState {
    pub fn new(selected: usize) -> Self {
        Self { selected }
    }

    pub fn move_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_next(&mut self, item_count: usize) {
        if item_count > 0 {
            self.selected = (self.selected + 1).min(item_count - 1);
        }
    }

    pub fn select(&mut self, idx: usize) {
        self.selected = idx;
    }
}

pub struct SettingsState {
    /// Which section tab is active.
    pub section: SettingsSection,
    /// Selected item index within the current section.
    pub list: SelectionListState,
    /// First visible row for scrollable settings sections.
    pub scroll: usize,
    /// The palette before opening settings (for cancel/restore).
    pub original_palette: Option<Palette>,
    /// The theme name before opening settings.
    pub original_theme: Option<String>,
    /// Pending global theme family while settings is open.
    pub pending_theme_name: Option<String>,
    /// Pending global theme mode while settings is open.
    pub pending_theme_mode: Option<ThemeMode>,
    /// Group whose theme is being edited, if settings was opened from a group menu.
    pub group_theme_target: Option<usize>,
}

pub(crate) enum DragTarget {
    WorkspaceReorder {
        source_ws_idx: usize,
        insert_idx: Option<usize>,
        target_group_idx: Option<usize>,
        indicator_row: Option<u16>,
    },
    TabReorder {
        ws_idx: usize,
        source_tab_idx: usize,
        insert_idx: Option<usize>,
    },
    WorkspaceListScrollbar {
        grab_row_offset: u16,
    },
    AgentPanelScrollbar {
        grab_row_offset: u16,
    },
    PaneSplit {
        path: Vec<bool>,
        direction: Direction,
        area: Rect,
    },
    PaneScrollbar {
        pane_id: crate::layout::PaneId,
        grab_row_offset: u16,
    },
    ReleaseNotesScrollbar {
        grab_row_offset: u16,
    },
    KeybindHelpScrollbar {
        grab_row_offset: u16,
    },
    SettingsThemeScrollbar {
        grab_row_offset: u16,
    },
    CommandPaletteScrollbar {
        grab_row_offset: u16,
    },
    SidebarDivider,
    RightSidebarDivider,
    SidebarSectionDivider,
}

/// Active mouse drag on a split border or sidebar divider.
pub(crate) struct DragState {
    pub target: DragTarget,
}

pub(crate) struct WorkspacePressState {
    pub ws_idx: usize,
    pub start_col: u16,
    pub start_row: u16,
}

pub(crate) struct TabPressState {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub start_col: u16,
    pub start_row: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuKind {
    Group {
        group_idx: usize,
        can_delete: bool,
    },
    Workspace {
        ws_idx: usize,
    },
    Tab {
        ws_idx: usize,
        tab_idx: usize,
    },
    Pane {
        pane_id: PaneId,
        has_manual_label: bool,
    },
}

/// Right-click context menu state.
pub struct ContextMenuState {
    pub kind: ContextMenuKind,
    pub x: u16,
    pub y: u16,
    pub list: MenuListState,
}

impl ContextMenuState {
    pub fn items(&self) -> &'static [&'static str] {
        match self.kind {
            ContextMenuKind::Group {
                can_delete: true, ..
            } => &["rename", "theme", "delete"],
            ContextMenuKind::Group {
                can_delete: false, ..
            } => &["rename", "theme"],
            ContextMenuKind::Workspace { .. } => &["rename", "close"],
            ContextMenuKind::Tab { .. } => &["new tab", "rename", "close"],
            ContextMenuKind::Pane {
                has_manual_label: true,
                ..
            } => &[
                "rename pane",
                "clear pane name",
                "split vertical",
                "split horizontal",
                "fullscreen",
                "close pane",
            ],
            ContextMenuKind::Pane {
                has_manual_label: false,
                ..
            } => &[
                "rename pane",
                "split vertical",
                "split horizontal",
                "fullscreen",
                "close pane",
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    NeedsAttention,
    Finished,
    UpdateInstalled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastTarget {
    pub workspace_id: String,
    pub pane_id: PaneId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastNotification {
    pub kind: ToastKind,
    pub title: String,
    pub context: String,
    pub target: Option<ToastTarget>,
}

pub struct ReleaseNotesState {
    pub version: String,
    pub body: String,
    pub scroll: u16,
    pub preview: bool,
}

pub struct KeybindHelpState {
    pub scroll: u16,
}

pub struct CommandPaletteState {
    pub query: String,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarWidthSource {
    ConfigDefault,
    Persisted,
    Manual,
}

/// All application state — pure data, no channels or async runtime.
/// Testable without PTYs or a tokio runtime.
pub struct AppState {
    pub groups: Vec<Group>,
    pub active_group: usize,
    pub group_filter_enabled: bool,
    pub workspaces: Vec<Workspace>,
    pub active: Option<usize>,
    pub selected: usize,
    pub mode: Mode,
    pub should_quit: bool,
    /// In persistence mode, client quit actions detach instead of stopping the server.
    pub quit_detaches: bool,
    /// Set when the current client should detach from the persistent session.
    /// The server's event loop checks this and handles client detach.
    pub detach_requested: bool,
    pub request_new_workspace: bool,
    pub request_new_tab: bool,
    pub request_reload_config: bool,
    /// Set when the headless server should ask attached clients to reload
    /// their client-local sound config from disk.
    pub request_client_sound_config_reload: bool,
    /// Set when UI interaction requested a clipboard write that must be
    /// handled by the outer App/event loop instead of directly from AppState.
    pub request_clipboard_write: Option<Vec<u8>>,
    pub creating_new_tab: bool,
    pub creating_new_group: bool,
    pub group_icon_input: String,
    pub group_icon_picker_open: bool,
    pub rename_group_target: Option<usize>,
    pub requested_new_tab_name: Option<String>,
    pub rename_pane_target: Option<PaneId>,
    pub confirm_delete_group: Option<usize>,
    pub request_complete_onboarding: bool,
    pub name_input: String,
    pub name_input_replace_on_type: bool,
    pub release_notes: Option<ReleaseNotesState>,
    pub keybind_help: KeybindHelpState,
    pub command_palette: CommandPaletteState,
    pub port_registry: crate::ports::PortRegistry,
    pub workspace_scroll: usize,
    pub agent_panel_scroll: usize,
    pub tab_scroll: usize,
    pub tab_scroll_follow_active: bool,
    pub mobile_switcher_scroll: usize,
    // View geometry (computed before render, consumed by render + mouse)
    pub view: ViewState,
    pub(crate) drag: Option<DragState>,
    pub(crate) workspace_press: Option<WorkspacePressState>,
    pub(crate) tab_press: Option<TabPressState>,
    pub selection: Option<Selection>,
    pub context_menu: Option<ContextMenuState>,
    // Notifications
    pub update_available: Option<String>,
    pub latest_release_notes_available: bool,
    pub update_dismissed: bool,
    pub config_diagnostic: Option<String>,
    pub toast: Option<ToastNotification>,
    /// Last reported focus state for the outer terminal hosting herdr.
    /// None means unsupported or not yet reported, which preserves active-pane suppression.
    pub outer_terminal_focus: Option<bool>,
    // Config
    pub prefix_code: KeyCode,
    pub prefix_mods: KeyModifiers,
    pub default_sidebar_width: u16,
    pub sidebar_width: u16,
    pub sidebar_width_source: SidebarWidthSource,
    pub sidebar_width_auto: bool,
    pub sidebar_collapsed: bool,
    pub right_sidebar_width: u16,
    pub right_sidebar_collapsed: bool,
    /// Ratio of sidebar height allocated to the workspaces section.
    pub sidebar_section_split: f32,
    pub activity_agents_expanded: bool,
    pub activity_ports_expanded: bool,
    pub collapsed_workspace_groups: Vec<String>,
    pub agent_panel_scope: AgentPanelScope,
    /// Capture mouse input for Herdr's own mouse UI. When false, Herdr only
    /// captures mouse while the focused pane app requests mouse reporting.
    pub mouse_capture: bool,
    pub confirm_close: bool,
    pub show_agent_labels_on_pane_borders: bool,
    pub kitty_graphics_enabled: bool,
    pub pane_scrollback_limit_bytes: usize,
    #[allow(dead_code)] // kept for backward compat; palette.accent is the source of truth
    pub accent: Color,
    pub sound: SoundConfig,
    pub local_sound_playback: bool,
    pub toast_config: ToastConfig,
    pub keybinds: Keybinds,
    /// Frame counter for spinner animations (wraps around).
    pub spinner_tick: u32,
    /// UI color palette — all sidebar/UI colors centralized for theming.
    pub palette: Palette,
    /// Default app palette from config, used when the active group has no override.
    pub global_palette: Palette,
    /// Currently applied theme name (for settings UI).
    pub theme_name: String,
    /// Default app theme name from config.
    pub global_theme_name: String,
    /// Default app light/dark mode from config.
    pub global_theme_mode: ThemeMode,
    /// Custom color overrides from config, applied only to the global fallback theme.
    pub global_theme_custom: Option<CustomThemeColors>,
    /// Whether legacy `ui.accent` should override the global theme accent.
    pub global_theme_use_legacy_ui_accent: bool,
    /// Settings panel state.
    pub settings: SettingsState,
    /// Highlight state for the bottom-right global launcher menu.
    pub global_menu: MenuListState,
    /// Highlight state for the sidebar group switcher menu.
    pub group_menu: MenuListState,
    /// Highlight state for the right-sidebar agent scope menu.
    pub agent_menu: MenuListState,
    /// Resolved host terminal default colors for theming embedded panes.
    pub host_terminal_theme: TerminalTheme,
    /// Set when a persisted session snapshot would change.
    pub session_dirty: bool,
}

impl AppState {
    pub fn theme_appearance_for_mode(&self, mode: ThemeMode) -> ThemeAppearance {
        mode.resolve(self.host_terminal_theme)
    }

    pub fn palette_for_theme_mode(&self, theme_name: &str, mode: ThemeMode) -> Option<Palette> {
        Palette::from_theme_with_terminal(
            theme_name,
            self.theme_appearance_for_mode(mode),
            self.host_terminal_theme,
        )
    }

    pub fn palette_for_theme(&self, theme_name: &str) -> Option<Palette> {
        self.palette_for_theme_mode(theme_name, self.global_theme_mode)
    }

    pub fn configured_global_palette(&self, theme_name: &str, mode: ThemeMode) -> Option<Palette> {
        let mut palette = self.palette_for_theme_mode(theme_name, mode)?;
        if let Some(custom) = &self.global_theme_custom {
            palette = palette.with_overrides(custom);
        }
        if self.global_theme_use_legacy_ui_accent
            && self
                .global_theme_custom
                .as_ref()
                .and_then(|custom| custom.accent.as_ref())
                .is_none()
        {
            palette.accent = self.accent;
        }
        Some(palette)
    }

    pub fn refresh_global_palette(&mut self) {
        if let Some(palette) =
            self.configured_global_palette(&self.global_theme_name, self.global_theme_mode)
        {
            self.global_palette = palette;
        }
    }

    pub fn active_group_id(&self) -> &str {
        self.groups
            .get(self.active_group)
            .map(|group| group.id.as_str())
            .unwrap_or(crate::workspace::DEFAULT_GROUP_ID)
    }

    pub fn active_group_name(&self) -> &str {
        self.groups
            .get(self.active_group)
            .map(|group| group.name.as_str())
            .unwrap_or("group 1")
    }

    pub fn active_group_icon(&self) -> &str {
        self.groups
            .get(self.active_group)
            .map(|group| group.icon.as_str())
            .unwrap_or(DEFAULT_GROUP_ICON)
    }

    pub fn workspace_in_active_group(&self, ws_idx: usize) -> bool {
        if !self.group_filter_enabled {
            return self.workspaces.get(ws_idx).is_some();
        }

        self.workspaces
            .get(ws_idx)
            .is_some_and(|workspace| workspace.group_id == self.active_group_id())
    }

    pub fn visible_workspace_indices(&self) -> Vec<usize> {
        if !self.group_filter_enabled {
            return (0..self.workspaces.len()).collect();
        }

        let active_group_id = self.active_group_id();
        self.workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| (workspace.group_id == active_group_id).then_some(idx))
            .collect()
    }

    pub fn workspace_group_collapsed(&self, group_id: &str) -> bool {
        self.collapsed_workspace_groups
            .iter()
            .any(|id| id == group_id)
    }

    pub fn sidebar_visible_workspace_indices(&self) -> Vec<usize> {
        if self.sidebar_collapsed || self.group_filter_enabled {
            return self.visible_workspace_indices();
        }

        self.workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                (!self.workspace_group_collapsed(&workspace.group_id)).then_some(idx)
            })
            .collect()
    }

    pub fn toggle_workspace_group(&mut self, group_idx: usize) {
        let Some(group_id) = self.groups.get(group_idx).map(|group| group.id.clone()) else {
            return;
        };
        if let Some(idx) = self
            .collapsed_workspace_groups
            .iter()
            .position(|id| id == &group_id)
        {
            self.collapsed_workspace_groups.remove(idx);
        } else {
            self.collapsed_workspace_groups.push(group_id);
        }
    }

    pub fn first_visible_workspace(&self) -> Option<usize> {
        if !self.group_filter_enabled {
            return (!self.workspaces.is_empty()).then_some(0);
        }

        let active_group_id = self.active_group_id();
        self.workspaces
            .iter()
            .position(|workspace| workspace.group_id == active_group_id)
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.session_dirty = true;
    }

    pub fn sound_enabled(&self) -> bool {
        self.sound.enabled
    }

    pub fn toast_delivery(&self) -> ToastDelivery {
        self.toast_config.delivery
    }

    pub fn agent_border_labels_enabled(&self) -> bool {
        self.show_agent_labels_on_pane_borders
    }

    pub fn focused_pane_requests_mouse_capture(&self) -> bool {
        self.mode == Mode::Terminal
            && self
                .active
                .and_then(|idx| self.workspaces.get(idx))
                .and_then(crate::workspace::Workspace::focused_runtime)
                .and_then(crate::pane::PaneRuntime::input_state)
                .is_some_and(crate::pane::InputState::mouse_reporting_enabled)
    }

    pub fn should_capture_host_mouse(&self) -> bool {
        self.mouse_capture || self.focused_pane_requests_mouse_capture()
    }

    pub fn is_prefix(&self, key: &crossterm::event::KeyEvent) -> bool {
        key_matches(key, self.prefix_code, self.prefix_mods)
    }

    pub fn estimate_pane_size(&self) -> (u16, u16) {
        if let Some(info) = self.view.pane_infos.first() {
            (info.rect.height, info.rect.width)
        } else {
            (24, 80)
        }
    }

    /// Returns true when the given (workspace, tab, pane) refers to the
    /// currently focused pane in the active workspace's active tab.
    pub fn is_active_pane(
        &self,
        ws_idx: usize,
        tab_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> bool {
        let Some(active_ws_idx) = self.active else {
            return false;
        };
        if ws_idx != active_ws_idx {
            return false;
        }
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return false;
        };
        if tab_idx != ws.active_tab_index() {
            return false;
        }
        ws.active_tab().map(|tab| tab.layout.focused()) == Some(pane_id)
    }
}

pub fn key_matches(
    key: &crossterm::event::KeyEvent,
    expected_code: KeyCode,
    expected_mods: KeyModifiers,
) -> bool {
    if key.modifiers != expected_mods {
        return false;
    }

    match (key.code, expected_code) {
        (KeyCode::Char(actual), KeyCode::Char(expected))
            if actual.is_ascii_alphabetic() && expected.is_ascii_alphabetic() =>
        {
            actual.eq_ignore_ascii_case(&expected)
        }
        (actual, expected) => actual == expected,
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
impl AppState {
    /// Create an AppState for testing — no channels, no PTYs.
    pub fn test_new() -> Self {
        Self {
            groups: vec![Group::default_group()],
            active_group: 0,
            group_filter_enabled: true,
            workspaces: Vec::new(),
            active: None,
            selected: 0,
            mode: Mode::Navigate,
            should_quit: false,
            quit_detaches: false,
            detach_requested: false,
            request_new_workspace: false,
            request_new_tab: false,
            request_reload_config: false,
            request_client_sound_config_reload: false,
            request_clipboard_write: None,
            creating_new_tab: false,
            creating_new_group: false,
            group_icon_input: DEFAULT_GROUP_ICON.to_string(),
            group_icon_picker_open: false,
            rename_group_target: None,
            requested_new_tab_name: None,
            rename_pane_target: None,
            confirm_delete_group: None,
            request_complete_onboarding: false,
            name_input: String::new(),
            name_input_replace_on_type: false,
            release_notes: None,
            keybind_help: KeybindHelpState { scroll: 0 },
            command_palette: CommandPaletteState {
                query: String::new(),
                selected: 0,
                scroll: 0,
            },
            port_registry: crate::ports::PortRegistry::default(),
            workspace_scroll: 0,
            agent_panel_scroll: 0,
            tab_scroll: 0,
            tab_scroll_follow_active: true,
            mobile_switcher_scroll: 0,
            view: ViewState {
                layout: ViewLayout::Desktop,
                sidebar_rect: Rect::default(),
                right_sidebar_rect: Rect::default(),
                workspace_card_areas: Vec::new(),
                workspace_group_header_areas: Vec::new(),
                workspace_group_empty_areas: Vec::new(),
                workspace_group_drop_areas: Vec::new(),
                tab_bar_rect: Rect::default(),
                tab_hit_areas: Vec::new(),
                tab_scroll_left_hit_area: Rect::default(),
                tab_scroll_right_hit_area: Rect::default(),
                new_tab_hit_area: Rect::default(),
                terminal_area: Rect::default(),
                mobile_header_rect: Rect::default(),
                mobile_menu_hit_area: Rect::default(),
                toast_hit_area: Rect::default(),
                pane_infos: Vec::new(),
                split_borders: Vec::new(),
            },
            drag: None,
            workspace_press: None,
            tab_press: None,
            selection: None,
            context_menu: None,
            update_available: None,
            latest_release_notes_available: false,
            update_dismissed: false,
            config_diagnostic: None,
            toast: None,
            outer_terminal_focus: None,
            prefix_code: KeyCode::Char('b'),
            prefix_mods: KeyModifiers::CONTROL,
            default_sidebar_width: 26,
            sidebar_width: 26,
            sidebar_width_source: SidebarWidthSource::ConfigDefault,
            sidebar_width_auto: false,
            sidebar_collapsed: false,
            right_sidebar_width: 28,
            right_sidebar_collapsed: false,
            sidebar_section_split: 0.5,
            activity_agents_expanded: true,
            activity_ports_expanded: true,
            collapsed_workspace_groups: Vec::new(),
            agent_panel_scope: AgentPanelScope::CurrentWorkspace,
            mouse_capture: true,
            confirm_close: true,
            show_agent_labels_on_pane_borders: false,
            kitty_graphics_enabled: false,
            pane_scrollback_limit_bytes: crate::config::DEFAULT_SCROLLBACK_LIMIT_BYTES,
            accent: Color::Cyan,
            sound: SoundConfig {
                enabled: false,
                ..SoundConfig::default()
            },
            local_sound_playback: false,
            toast_config: ToastConfig::default(),
            keybinds: Keybinds {
                new_workspace: (KeyCode::Char('n'), KeyModifiers::empty()),
                new_workspace_label: "n".into(),
                rename_workspace: (KeyCode::Char('n'), KeyModifiers::SHIFT),
                rename_workspace_label: "shift+n".into(),
                close_workspace: (KeyCode::Char('d'), KeyModifiers::SHIFT),
                close_workspace_label: "shift+d".into(),
                detach: None,
                detach_label: None,
                reload_config: None,
                reload_config_label: None,
                open_notification_target: None,
                open_notification_target_label: None,
                command_palette: (KeyCode::Char('p'), KeyModifiers::empty()),
                command_palette_label: "p".into(),
                previous_workspace: None,
                previous_workspace_label: None,
                next_workspace: None,
                next_workspace_label: None,
                open_group_menu: None,
                open_group_menu_label: None,
                new_group: None,
                new_group_label: None,
                rename_group: None,
                rename_group_label: None,
                delete_group: None,
                delete_group_label: None,
                toggle_group_filter: None,
                toggle_group_filter_label: None,
                previous_group: None,
                previous_group_label: None,
                next_group: None,
                next_group_label: None,
                previous_agent: None,
                previous_agent_label: None,
                next_agent: None,
                next_agent_label: None,
                open_agent_menu: None,
                open_agent_menu_label: None,
                indexed_tabs: None,
                indexed_tabs_label: None,
                indexed_workspaces: None,
                indexed_workspaces_label: None,
                indexed_agents: None,
                indexed_agents_label: None,
                new_tab: (KeyCode::Char('c'), KeyModifiers::empty()),
                new_tab_label: "c".into(),
                rename_tab: None,
                rename_tab_label: None,
                previous_tab: None,
                previous_tab_label: None,
                next_tab: None,
                next_tab_label: None,
                close_tab: None,
                close_tab_label: None,
                rename_pane: None,
                rename_pane_label: None,
                focus_pane_left: None,
                focus_pane_left_label: None,
                focus_pane_down: None,
                focus_pane_down_label: None,
                focus_pane_up: None,
                focus_pane_up_label: None,
                focus_pane_right: None,
                focus_pane_right_label: None,
                split_vertical: (KeyCode::Char('v'), KeyModifiers::empty()),
                split_vertical_label: "v".into(),
                split_horizontal: (KeyCode::Char('-'), KeyModifiers::empty()),
                split_horizontal_label: "-".into(),
                close_pane: (KeyCode::Char('x'), KeyModifiers::empty()),
                close_pane_label: "x".into(),
                fullscreen: (KeyCode::Char('f'), KeyModifiers::empty()),
                fullscreen_label: "f".into(),
                resize_mode: (KeyCode::Char('r'), KeyModifiers::empty()),
                resize_mode_label: "r".into(),
                toggle_sidebar: (KeyCode::Char('b'), KeyModifiers::empty()),
                toggle_sidebar_label: "b".into(),
                toggle_right_sidebar: None,
                toggle_right_sidebar_label: None,
                custom_commands: Vec::new(),
            },
            spinner_tick: 0,
            palette: Palette::catppuccin(),
            global_palette: Palette::catppuccin(),
            theme_name: "catppuccin".to_string(),
            global_theme_name: "catppuccin".to_string(),
            global_theme_mode: ThemeMode::System,
            global_theme_custom: None,
            global_theme_use_legacy_ui_accent: false,
            settings: SettingsState {
                section: SettingsSection::Theme,
                list: SelectionListState::new(0),
                scroll: 0,
                original_palette: None,
                original_theme: None,
                pending_theme_name: None,
                pending_theme_mode: None,
                group_theme_target: None,
            },
            global_menu: MenuListState::new(0),
            group_menu: MenuListState::new(0),
            agent_menu: MenuListState::new(0),
            host_terminal_theme: TerminalTheme::default(),
            session_dirty: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn built_in_theme_names_resolve() {
        for name in THEME_NAMES {
            assert!(
                Palette::from_theme(name, ThemeAppearance::Dark).is_some(),
                "theme should resolve: {name}"
            );
        }
    }

    #[test]
    fn light_theme_aliases_resolve() {
        for name in ["light", "latte", "tokyo-day", "onelight", "lotus", "dawn"] {
            assert!(
                Palette::from_name(name).is_some(),
                "theme should resolve: {name}"
            );
        }
    }

    #[test]
    fn key_matches_requires_exact_modifiers() {
        assert!(key_matches(
            &KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
        ));

        assert!(!key_matches(
            &KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
        ));
    }

    #[test]
    fn key_matches_letters_case_insensitively() {
        assert!(key_matches(
            &KeyEvent::new(KeyCode::Char('B'), KeyModifiers::SHIFT),
            KeyCode::Char('b'),
            KeyModifiers::SHIFT,
        ));
    }

    #[test]
    fn dark_only_theme_uses_dark_palette_in_light_mode() {
        assert_eq!(
            Palette::from_theme("nord", ThemeAppearance::Light)
                .unwrap()
                .panel_bg,
            Palette::nord().panel_bg
        );
    }

    #[test]
    fn system_theme_uses_terminal_default_colors() {
        let host_theme = TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor {
                r: 220,
                g: 221,
                b: 222,
            }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 10,
                g: 11,
                b: 12,
            }),
        };

        let palette =
            Palette::from_theme_with_terminal("system", ThemeAppearance::Dark, host_theme)
                .expect("system theme resolves");

        assert_eq!(palette.panel_bg, Color::Reset);
        assert_eq!(palette.text, Color::Rgb(220, 221, 222));
        assert_eq!(palette.accent, Color::Rgb(220, 221, 222));
        assert_ne!(palette.surface0, Palette::catppuccin().surface0);
    }
}
