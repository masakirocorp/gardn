use crate::config::{
    CustomThemeColors, Keybinds, NewTerminalCwdConfig, SoundConfig, TerminalAccent, ThemeConfig,
    ThemeMode, ToastConfig, ToastDelivery,
};
use crate::detect::AgentState;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Direction, Rect};
use ratatui::style::Color;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::layout::{PaneId, PaneInfo, SplitBorder};
use crate::selection::Selection;
use crate::terminal_theme::{TerminalTheme, ThemeAppearance};

// ---------------------------------------------------------------------------
// Selection autoscroll types
// ---------------------------------------------------------------------------

/// Direction of automatic scrolling during text selection drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionAutoscrollDirection {
    Up,
    Down,
}

/// State for automatic scrolling during text selection drag.
///
/// When the cursor hovers in the 1-row hot zone at the top or bottom edge
/// of a pane (or outside the pane), this struct captures the direction and
/// last known mouse position so a recurring 30ms tick can continue scrolling
/// and extending the selection even when the mouse is not moving.
#[derive(Clone, Debug)]
pub(crate) struct SelectionAutoscroll {
    pub direction: SelectionAutoscrollDirection,
    pub last_mouse_screen_col: u16,
    pub last_mouse_screen_row: u16,
    pub inner_rect: Rect,
}

#[derive(Clone)]
pub(crate) struct RightClickPassthroughGesture {
    pub pane_info: PaneInfo,
    pub modifiers: KeyModifiers,
}
use crate::workspace::Workspace;

static NEXT_GROUP_ID: AtomicU64 = AtomicU64::new(1);

pub const DEFAULT_GROUP_ICON: &str = "☀";
pub const GROUP_ICONS: &[&str] = &[
    "☀", "☁", "☂", "☕", "♥", "♪", "⚑", "⚙", "☎", "☄", "☘", "✉", "⚓", "✿", "✂", "✎", "✚", "⊕",
    "▥", "⌁",
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
    pub accent: Option<TerminalAccent>,
}

impl Group {
    pub fn default_group() -> Self {
        Self {
            id: crate::workspace::DEFAULT_GROUP_ID.to_string(),
            name: "group 1".to_string(),
            icon: DEFAULT_GROUP_ICON.to_string(),
            accent: None,
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
    #[allow(clippy::too_many_arguments)]
    fn catppuccin_palette(
        accent: Color,
        panel_bg: Color,
        surface0: Color,
        surface1: Color,
        surface_dim: Color,
        overlay0: Color,
        overlay1: Color,
        text: Color,
        subtext0: Color,
        mauve: Color,
        green: Color,
        yellow: Color,
        red: Color,
        blue: Color,
        teal: Color,
        peach: Color,
    ) -> Self {
        Self {
            accent,
            panel_bg,
            surface0,
            surface1,
            surface_dim,
            overlay0,
            overlay1,
            text,
            subtext0,
            mauve,
            green,
            yellow,
            red,
            blue,
            teal,
            peach,
        }
    }

    /// Catppuccin Mocha.
    pub fn catppuccin() -> Self {
        Self::catppuccin_palette(
            Self::rgb(137, 180, 250),
            Self::rgb(30, 30, 46),
            Self::rgb(49, 50, 68),
            Self::rgb(69, 71, 90),
            Self::rgb(24, 24, 37),
            Self::rgb(108, 112, 134),
            Self::rgb(127, 132, 156),
            Self::rgb(205, 214, 244),
            Self::rgb(166, 173, 200),
            Self::rgb(203, 166, 247),
            Self::rgb(166, 227, 161),
            Self::rgb(249, 226, 175),
            Self::rgb(243, 139, 168),
            Self::rgb(137, 180, 250),
            Self::rgb(148, 226, 213),
            Self::rgb(250, 179, 135),
        )
    }

    /// Catppuccin Latte.
    pub fn catppuccin_light() -> Self {
        Self::catppuccin_palette(
            Self::rgb(30, 102, 245),
            Self::rgb(239, 241, 245),
            Self::rgb(204, 208, 218),
            Self::rgb(188, 192, 204),
            Self::rgb(230, 233, 239),
            Self::rgb(156, 160, 176),
            Self::rgb(140, 143, 161),
            Self::rgb(76, 79, 105),
            Self::rgb(108, 111, 133),
            Self::rgb(136, 57, 239),
            Self::rgb(64, 160, 43),
            Self::rgb(223, 142, 29),
            Self::rgb(210, 15, 57),
            Self::rgb(30, 102, 245),
            Self::rgb(23, 146, 153),
            Self::rgb(254, 100, 11),
        )
    }

    pub fn catppuccin_latte() -> Self {
        Self::catppuccin_light()
    }

    /// Catppuccin Frappé.
    pub fn catppuccin_frappe() -> Self {
        Self::catppuccin_palette(
            Self::rgb(140, 170, 238),
            Self::rgb(48, 52, 70),
            Self::rgb(65, 69, 89),
            Self::rgb(81, 87, 109),
            Self::rgb(41, 44, 60),
            Self::rgb(115, 121, 148),
            Self::rgb(131, 139, 167),
            Self::rgb(198, 208, 245),
            Self::rgb(165, 173, 206),
            Self::rgb(202, 158, 230),
            Self::rgb(166, 209, 137),
            Self::rgb(229, 200, 144),
            Self::rgb(231, 130, 132),
            Self::rgb(140, 170, 238),
            Self::rgb(129, 200, 190),
            Self::rgb(239, 159, 118),
        )
    }

    /// Catppuccin Macchiato.
    pub fn catppuccin_macchiato() -> Self {
        Self::catppuccin_palette(
            Self::rgb(138, 173, 244),
            Self::rgb(36, 39, 58),
            Self::rgb(54, 58, 79),
            Self::rgb(73, 77, 100),
            Self::rgb(30, 32, 48),
            Self::rgb(110, 115, 141),
            Self::rgb(128, 135, 162),
            Self::rgb(202, 211, 245),
            Self::rgb(165, 173, 203),
            Self::rgb(198, 160, 246),
            Self::rgb(166, 218, 149),
            Self::rgb(238, 212, 159),
            Self::rgb(237, 135, 150),
            Self::rgb(138, 173, 244),
            Self::rgb(139, 213, 202),
            Self::rgb(245, 169, 127),
        )
    }

    /// System — respect the host terminal defaults and ANSI palette.
    pub fn system(
        host_theme: TerminalTheme,
        appearance: crate::terminal_theme::ThemeAppearance,
        accent: TerminalAccent,
    ) -> Self {
        let host_fg = host_theme.foreground.map(Self::terminal_color);
        let host_bg = host_theme.background.map(Self::terminal_color);

        let text = host_fg.unwrap_or(Color::Reset);
        let overlay0 = Self::neutral_from_foreground(host_fg, host_bg, appearance, 0.45);
        let overlay1 = Self::neutral_from_foreground(host_fg, host_bg, appearance, 0.20);
        let subtext0 = Self::neutral_from_foreground(host_fg, host_bg, appearance, 0.35);

        Self {
            accent: Self::terminal_palette_color(
                host_theme,
                accent.ansi_index(),
                accent.fallback_color(),
            ),
            panel_bg: Color::Reset,
            surface0: host_bg
                .map(|color| Self::surface_from_background(color, appearance, 0.08))
                .unwrap_or(Color::Reset),
            surface1: host_bg
                .map(|color| Self::surface_from_background(color, appearance, 0.14))
                .unwrap_or(Color::DarkGray),
            surface_dim: host_bg
                .map(|color| Self::surface_from_background(color, appearance, 0.05))
                .unwrap_or(Color::DarkGray),
            overlay0,
            overlay1,
            text,
            subtext0,
            mauve: Self::terminal_palette_color(host_theme, 5, Color::Magenta),
            green: Self::terminal_palette_color(host_theme, 2, Color::Green),
            yellow: Self::terminal_palette_color(host_theme, 3, Color::Yellow),
            red: Self::terminal_palette_color(host_theme, 1, Color::LightRed),
            blue: Self::terminal_palette_color(host_theme, 4, Color::Blue),
            teal: Self::terminal_palette_color(host_theme, 6, Color::Cyan),
            peach: Self::terminal_palette_color(host_theme, 3, Color::Yellow),
        }
    }

    fn terminal_color(color: crate::terminal_theme::RgbColor) -> Color {
        Color::Rgb(color.r, color.g, color.b)
    }

    pub fn terminal_accent_color(theme: TerminalTheme, accent: TerminalAccent) -> Color {
        Self::terminal_palette_color(theme, accent.ansi_index(), accent.fallback_color())
    }

    fn terminal_palette_color(theme: TerminalTheme, index: usize, fallback: Color) -> Color {
        theme
            .palette
            .get(index)
            .and_then(|color| color.map(Self::terminal_color))
            .unwrap_or(fallback)
    }

    fn neutral_from_foreground(
        foreground: Option<Color>,
        background: Option<Color>,
        appearance: crate::terminal_theme::ThemeAppearance,
        amount_toward_background: f32,
    ) -> Color {
        let Some(Color::Rgb(fr, fg, fb)) = foreground else {
            return match appearance {
                ThemeAppearance::Light => Color::DarkGray,
                ThemeAppearance::Dark => Color::Gray,
            };
        };
        let Some(Color::Rgb(br, bg, bb)) = background else {
            return Color::Rgb(fr, fg, fb);
        };

        let blend = |fg: u8, bg: u8| -> u8 {
            let value = fg as f32 + (bg as f32 - fg as f32) * amount_toward_background;
            value.round().clamp(0.0, 255.0) as u8
        };

        Color::Rgb(blend(fr, br), blend(fg, bg), blend(fb, bb))
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
    /// Terminal 16-color theme.
    pub fn terminal() -> Self {
        Self {
            accent: Color::Blue,
            panel_bg: Color::Reset,
            surface0: Color::Reset,
            surface1: Color::DarkGray,
            surface_dim: Color::DarkGray,
            overlay0: Color::Gray,
            overlay1: Color::White,
            text: Color::Reset,
            subtext0: Color::Gray,
            mauve: Color::Gray,
            green: Color::Green,
            yellow: Color::Yellow,
            red: Color::LightRed,
            blue: Color::Blue,
            teal: Color::Cyan,
            peach: Color::Yellow,
        }
    }

    /// Tokyo Night.
    pub fn tokyo_night() -> Self {
        Self::omarchy_palette(
            Self::rgb(122, 162, 247),
            Self::rgb(169, 177, 214),
            Self::rgb(26, 27, 38),
            Self::rgb(50, 52, 74),
            Self::rgb(247, 118, 142),
            Self::rgb(158, 206, 106),
            Self::rgb(224, 175, 104),
            Self::rgb(122, 162, 247),
            Self::rgb(173, 142, 230),
            Self::rgb(68, 157, 171),
            Self::rgb(120, 124, 153),
            Self::rgb(68, 75, 106),
        )
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

    /// Nord.
    pub fn nord() -> Self {
        Self::omarchy_palette(
            Self::rgb(129, 161, 193),
            Self::rgb(216, 222, 233),
            Self::rgb(46, 52, 64),
            Self::rgb(59, 66, 82),
            Self::rgb(191, 97, 106),
            Self::rgb(163, 190, 140),
            Self::rgb(235, 203, 139),
            Self::rgb(129, 161, 193),
            Self::rgb(180, 142, 173),
            Self::rgb(136, 192, 208),
            Self::rgb(229, 233, 240),
            Self::rgb(76, 86, 106),
        )
    }

    /// Gruvbox.
    pub fn gruvbox() -> Self {
        Self::omarchy_palette(
            Self::rgb(125, 174, 163),
            Self::rgb(212, 190, 152),
            Self::rgb(40, 40, 40),
            Self::rgb(60, 56, 54),
            Self::rgb(234, 105, 98),
            Self::rgb(169, 182, 101),
            Self::rgb(216, 166, 87),
            Self::rgb(125, 174, 163),
            Self::rgb(211, 134, 155),
            Self::rgb(137, 180, 130),
            Self::rgb(212, 190, 152),
            Self::rgb(60, 56, 54),
        )
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

    /// Kanagawa.
    pub fn kanagawa() -> Self {
        Self::omarchy_palette(
            Self::rgb(126, 156, 216),
            Self::rgb(220, 215, 186),
            Self::rgb(31, 31, 40),
            Self::rgb(9, 6, 24),
            Self::rgb(195, 64, 67),
            Self::rgb(118, 148, 106),
            Self::rgb(192, 163, 110),
            Self::rgb(126, 156, 216),
            Self::rgb(149, 127, 184),
            Self::rgb(106, 149, 137),
            Self::rgb(200, 192, 147),
            Self::rgb(114, 113, 105),
        )
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
    pub fn rose_pine_dawn() -> Self {
        Self::omarchy_palette(
            Self::rgb(86, 148, 159),
            Self::rgb(87, 82, 121),
            Self::rgb(250, 244, 237),
            Self::rgb(242, 233, 225),
            Self::rgb(180, 99, 122),
            Self::rgb(40, 105, 131),
            Self::rgb(234, 157, 52),
            Self::rgb(86, 148, 159),
            Self::rgb(144, 122, 169),
            Self::rgb(215, 130, 126),
            Self::rgb(87, 82, 121),
            Self::rgb(152, 147, 165),
        )
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
    fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgb(r, g, b)
    }

    // Monokai variants share the same token layout; named arguments keep
    // the copied upstream palette values auditable.
    #[allow(clippy::too_many_arguments)]
    fn monokai_palette(
        accent: Color,
        panel_bg: Color,
        surface0: Color,
        surface1: Color,
        surface_dim: Color,
        overlay0: Color,
        overlay1: Color,
        text: Color,
        subtext0: Color,
        red: Color,
        green: Color,
        yellow: Color,
        peach: Color,
        mauve: Color,
        teal: Color,
    ) -> Self {
        Self {
            accent,
            panel_bg,
            surface0,
            surface1,
            surface_dim,
            overlay0,
            overlay1,
            text,
            subtext0,
            mauve,
            green,
            yellow,
            red,
            blue: teal,
            teal,
            peach,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn omarchy_palette(
        accent: Color,
        foreground: Color,
        background: Color,
        color0: Color,
        color1: Color,
        color2: Color,
        color3: Color,
        color4: Color,
        color5: Color,
        color6: Color,
        color7: Color,
        color8: Color,
    ) -> Self {
        Self {
            accent,
            panel_bg: background,
            surface0: color0,
            surface1: color8,
            surface_dim: background,
            overlay0: color8,
            overlay1: color7,
            text: foreground,
            subtext0: color7,
            mauve: color5,
            green: color2,
            yellow: color3,
            red: color1,
            blue: color4,
            teal: color6,
            peach: color3,
        }
    }

    /// Monokai Pro.
    pub fn monokai_pro() -> Self {
        Self::monokai_palette(
            Self::rgb(255, 216, 102),
            Self::rgb(34, 31, 34),
            Self::rgb(45, 42, 46),
            Self::rgb(64, 62, 65),
            Self::rgb(25, 24, 26),
            Self::rgb(114, 112, 114),
            Self::rgb(147, 146, 147),
            Self::rgb(252, 252, 250),
            Self::rgb(193, 192, 192),
            Self::rgb(255, 97, 136),
            Self::rgb(169, 220, 118),
            Self::rgb(255, 216, 102),
            Self::rgb(252, 152, 103),
            Self::rgb(171, 157, 242),
            Self::rgb(120, 220, 232),
        )
    }

    /// Monokai Pro Light.
    pub fn monokai_pro_light() -> Self {
        Self::monokai_palette(
            Self::rgb(225, 71, 117),
            Self::rgb(237, 231, 229),
            Self::rgb(250, 244, 242),
            Self::rgb(211, 205, 204),
            Self::rgb(224, 218, 217),
            Self::rgb(165, 159, 160),
            Self::rgb(145, 140, 142),
            Self::rgb(41, 36, 42),
            Self::rgb(112, 107, 110),
            Self::rgb(225, 71, 117),
            Self::rgb(38, 157, 105),
            Self::rgb(204, 122, 10),
            Self::rgb(225, 96, 50),
            Self::rgb(112, 88, 190),
            Self::rgb(28, 140, 168),
        )
    }

    /// Monokai Pro Light Sun.
    pub fn monokai_pro_light_sun() -> Self {
        Self::monokai_palette(
            Self::rgb(206, 71, 112),
            Self::rgb(238, 229, 222),
            Self::rgb(248, 239, 231),
            Self::rgb(210, 201, 196),
            Self::rgb(222, 213, 208),
            Self::rgb(165, 156, 156),
            Self::rgb(146, 137, 138),
            Self::rgb(44, 35, 46),
            Self::rgb(114, 105, 109),
            Self::rgb(206, 71, 112),
            Self::rgb(33, 136, 113),
            Self::rgb(177, 104, 3),
            Self::rgb(212, 87, 43),
            Self::rgb(104, 81, 162),
            Self::rgb(36, 115, 182),
        )
    }

    /// Monokai Pro Spectrum.
    pub fn monokai_pro_spectrum() -> Self {
        Self::monokai_palette(
            Self::rgb(252, 229, 102),
            Self::rgb(25, 25, 25),
            Self::rgb(34, 34, 34),
            Self::rgb(54, 53, 55),
            Self::rgb(19, 19, 19),
            Self::rgb(105, 103, 108),
            Self::rgb(139, 136, 143),
            Self::rgb(247, 241, 255),
            Self::rgb(186, 182, 192),
            Self::rgb(252, 97, 141),
            Self::rgb(123, 216, 143),
            Self::rgb(252, 229, 102),
            Self::rgb(253, 147, 83),
            Self::rgb(148, 138, 227),
            Self::rgb(90, 212, 230),
        )
    }

    /// Monokai Pro Ristretto.
    pub fn monokai_pro_ristretto() -> Self {
        Self::monokai_palette(
            Self::rgb(249, 204, 108),
            Self::rgb(33, 28, 28),
            Self::rgb(44, 37, 37),
            Self::rgb(64, 56, 56),
            Self::rgb(25, 21, 21),
            Self::rgb(114, 105, 106),
            Self::rgb(148, 138, 139),
            Self::rgb(255, 241, 243),
            Self::rgb(195, 183, 184),
            Self::rgb(253, 104, 131),
            Self::rgb(173, 218, 120),
            Self::rgb(249, 204, 108),
            Self::rgb(243, 141, 112),
            Self::rgb(168, 169, 235),
            Self::rgb(133, 218, 204),
        )
    }

    /// Monokai Pro Octagon.
    pub fn monokai_pro_octagon() -> Self {
        Self::monokai_palette(
            Self::rgb(255, 215, 109),
            Self::rgb(30, 31, 43),
            Self::rgb(40, 42, 58),
            Self::rgb(58, 61, 75),
            Self::rgb(22, 24, 33),
            Self::rgb(105, 109, 119),
            Self::rgb(136, 141, 148),
            Self::rgb(234, 242, 241),
            Self::rgb(178, 185, 189),
            Self::rgb(255, 101, 122),
            Self::rgb(186, 215, 97),
            Self::rgb(255, 215, 109),
            Self::rgb(255, 155, 94),
            Self::rgb(195, 154, 201),
            Self::rgb(156, 209, 187),
        )
    }

    /// Monokai Pro Machine.
    pub fn monokai_pro_machine() -> Self {
        Self::monokai_palette(
            Self::rgb(255, 237, 114),
            Self::rgb(29, 37, 40),
            Self::rgb(39, 49, 54),
            Self::rgb(58, 68, 73),
            Self::rgb(22, 27, 30),
            Self::rgb(107, 118, 120),
            Self::rgb(139, 151, 152),
            Self::rgb(242, 255, 252),
            Self::rgb(184, 196, 195),
            Self::rgb(255, 109, 126),
            Self::rgb(162, 229, 123),
            Self::rgb(255, 237, 114),
            Self::rgb(255, 178, 112),
            Self::rgb(186, 160, 248),
            Self::rgb(124, 213, 241),
        )
    }

    /// Monokai Classic.
    pub fn monokai_classic() -> Self {
        Self::monokai_palette(
            Self::rgb(230, 219, 116),
            Self::rgb(29, 30, 25),
            Self::rgb(39, 40, 34),
            Self::rgb(59, 60, 53),
            Self::rgb(22, 22, 19),
            Self::rgb(110, 112, 102),
            Self::rgb(145, 146, 136),
            Self::rgb(253, 255, 241),
            Self::rgb(192, 193, 181),
            Self::rgb(249, 38, 114),
            Self::rgb(166, 226, 46),
            Self::rgb(230, 219, 116),
            Self::rgb(253, 151, 31),
            Self::rgb(174, 129, 255),
            Self::rgb(102, 217, 239),
        )
    }

    /// Omarchy Ethereal.
    pub fn ethereal() -> Self {
        Self::omarchy_palette(
            Self::rgb(125, 130, 217),
            Self::rgb(255, 206, 173),
            Self::rgb(6, 11, 30),
            Self::rgb(60, 72, 109),
            Self::rgb(237, 91, 90),
            Self::rgb(146, 165, 147),
            Self::rgb(233, 187, 79),
            Self::rgb(125, 130, 217),
            Self::rgb(200, 157, 193),
            Self::rgb(163, 191, 209),
            Self::rgb(249, 153, 87),
            Self::rgb(109, 125, 182),
        )
    }

    /// Omarchy Everforest.
    pub fn everforest() -> Self {
        Self::omarchy_palette(
            Self::rgb(127, 187, 179),
            Self::rgb(211, 198, 170),
            Self::rgb(45, 53, 59),
            Self::rgb(71, 82, 88),
            Self::rgb(230, 126, 128),
            Self::rgb(167, 192, 128),
            Self::rgb(219, 188, 127),
            Self::rgb(127, 187, 179),
            Self::rgb(214, 153, 182),
            Self::rgb(131, 192, 146),
            Self::rgb(211, 198, 170),
            Self::rgb(71, 82, 88),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn flexoki_palette(
        accent: Color,
        panel_bg: Color,
        surface0: Color,
        surface1: Color,
        surface_dim: Color,
        overlay0: Color,
        overlay1: Color,
        text: Color,
        subtext0: Color,
        mauve: Color,
        green: Color,
        yellow: Color,
        red: Color,
        blue: Color,
        teal: Color,
        peach: Color,
    ) -> Self {
        Self {
            accent,
            panel_bg,
            surface0,
            surface1,
            surface_dim,
            overlay0,
            overlay1,
            text,
            subtext0,
            mauve,
            green,
            yellow,
            red,
            blue,
            teal,
            peach,
        }
    }

    /// Flexoki Light.
    pub fn flexoki_light() -> Self {
        Self::flexoki_palette(
            Self::rgb(36, 131, 123),
            Self::rgb(255, 252, 240),
            Self::rgb(230, 228, 217),
            Self::rgb(206, 205, 195),
            Self::rgb(242, 240, 229),
            Self::rgb(183, 181, 172),
            Self::rgb(111, 110, 105),
            Self::rgb(16, 15, 15),
            Self::rgb(111, 110, 105),
            Self::rgb(94, 64, 157),
            Self::rgb(102, 128, 11),
            Self::rgb(173, 131, 1),
            Self::rgb(175, 48, 41),
            Self::rgb(32, 94, 166),
            Self::rgb(36, 131, 123),
            Self::rgb(188, 82, 21),
        )
    }

    /// Flexoki.
    pub fn flexoki() -> Self {
        Self::flexoki_palette(
            Self::rgb(58, 169, 159),
            Self::rgb(16, 15, 15),
            Self::rgb(40, 39, 38),
            Self::rgb(64, 62, 60),
            Self::rgb(28, 27, 26),
            Self::rgb(87, 86, 83),
            Self::rgb(135, 133, 128),
            Self::rgb(206, 205, 195),
            Self::rgb(135, 133, 128),
            Self::rgb(139, 126, 200),
            Self::rgb(135, 154, 57),
            Self::rgb(208, 162, 21),
            Self::rgb(209, 77, 65),
            Self::rgb(67, 133, 190),
            Self::rgb(58, 169, 159),
            Self::rgb(218, 112, 44),
        )
    }

    /// Omarchy Hackerman.
    pub fn hackerman() -> Self {
        Self::omarchy_palette(
            Self::rgb(130, 251, 156),
            Self::rgb(221, 247, 255),
            Self::rgb(11, 12, 22),
            Self::rgb(62, 64, 88),
            Self::rgb(80, 248, 114),
            Self::rgb(79, 232, 143),
            Self::rgb(80, 247, 212),
            Self::rgb(130, 157, 212),
            Self::rgb(134, 167, 223),
            Self::rgb(124, 248, 247),
            Self::rgb(133, 225, 251),
            Self::rgb(106, 110, 149),
        )
    }

    /// Omarchy Last Horizon.
    pub fn last_horizon() -> Self {
        Self::omarchy_palette(
            Self::rgb(181, 151, 144),
            Self::rgb(250, 252, 251),
            Self::rgb(12, 11, 12),
            Self::rgb(12, 11, 12),
            Self::rgb(195, 139, 123),
            Self::rgb(135, 169, 176),
            Self::rgb(107, 94, 115),
            Self::rgb(181, 151, 144),
            Self::rgb(196, 216, 226),
            Self::rgb(165, 160, 182),
            Self::rgb(207, 211, 205),
            Self::rgb(88, 78, 81),
        )
    }

    /// Omarchy Lumon.
    pub fn lumon() -> Self {
        Self::omarchy_palette(
            Self::rgb(139, 201, 235),
            Self::rgb(214, 226, 238),
            Self::rgb(22, 36, 45),
            Self::rgb(27, 45, 64),
            Self::rgb(77, 134, 176),
            Self::rgb(94, 149, 188),
            Self::rgb(111, 164, 201),
            Self::rgb(111, 184, 227),
            Self::rgb(139, 201, 235),
            Self::rgb(180, 228, 246),
            Self::rgb(214, 226, 238),
            Self::rgb(48, 72, 96),
        )
    }

    /// Omarchy Matte Black.
    pub fn matte_black() -> Self {
        Self::omarchy_palette(
            Self::rgb(230, 142, 13),
            Self::rgb(190, 190, 190),
            Self::rgb(18, 18, 18),
            Self::rgb(51, 51, 51),
            Self::rgb(211, 95, 95),
            Self::rgb(255, 193, 7),
            Self::rgb(185, 28, 28),
            Self::rgb(230, 142, 13),
            Self::rgb(211, 95, 95),
            Self::rgb(190, 190, 190),
            Self::rgb(190, 190, 190),
            Self::rgb(138, 138, 141),
        )
    }

    /// Omarchy Miasma.
    pub fn miasma() -> Self {
        Self::omarchy_palette(
            Self::rgb(120, 130, 75),
            Self::rgb(194, 194, 176),
            Self::rgb(34, 34, 34),
            Self::rgb(0, 0, 0),
            Self::rgb(104, 87, 66),
            Self::rgb(95, 135, 95),
            Self::rgb(179, 109, 67),
            Self::rgb(120, 130, 75),
            Self::rgb(187, 119, 68),
            Self::rgb(201, 165, 84),
            Self::rgb(215, 196, 131),
            Self::rgb(102, 102, 102),
        )
    }

    /// Omarchy Osaka Jade.
    pub fn osaka_jade() -> Self {
        Self::omarchy_palette(
            Self::rgb(80, 148, 117),
            Self::rgb(193, 196, 151),
            Self::rgb(17, 28, 24),
            Self::rgb(35, 55, 43),
            Self::rgb(255, 83, 69),
            Self::rgb(84, 158, 106),
            Self::rgb(69, 148, 81),
            Self::rgb(80, 148, 117),
            Self::rgb(210, 104, 156),
            Self::rgb(45, 213, 183),
            Self::rgb(246, 245, 221),
            Self::rgb(83, 104, 91),
        )
    }

    /// Omarchy Retro 82.
    pub fn retro_82() -> Self {
        Self::omarchy_palette(
            Self::rgb(250, 169, 104),
            Self::rgb(246, 220, 172),
            Self::rgb(5, 24, 46),
            Self::rgb(48, 52, 66),
            Self::rgb(248, 85, 37),
            Self::rgb(2, 131, 145),
            Self::rgb(233, 123, 60),
            Self::rgb(250, 169, 104),
            Self::rgb(63, 143, 138),
            Self::rgb(140, 191, 184),
            Self::rgb(167, 201, 198),
            Self::rgb(19, 78, 90),
        )
    }

    /// Omarchy Solitude.
    pub fn solitude() -> Self {
        Self::omarchy_palette(
            Self::rgb(121, 129, 134),
            Self::rgb(202, 204, 204),
            Self::rgb(16, 19, 21),
            Self::rgb(16, 19, 21),
            Self::rgb(86, 93, 96),
            Self::rgb(159, 165, 169),
            Self::rgb(217, 219, 220),
            Self::rgb(121, 129, 134),
            Self::rgb(174, 174, 174),
            Self::rgb(112, 112, 112),
            Self::rgb(203, 194, 190),
            Self::rgb(75, 78, 85),
        )
    }

    /// Omarchy Vantablack.
    pub fn vantablack() -> Self {
        Self::omarchy_palette(
            Self::rgb(141, 141, 141),
            Self::rgb(255, 255, 255),
            Self::rgb(0, 0, 0),
            Self::rgb(64, 64, 64),
            Self::rgb(164, 164, 164),
            Self::rgb(182, 182, 182),
            Self::rgb(206, 206, 206),
            Self::rgb(141, 141, 141),
            Self::rgb(155, 155, 155),
            Self::rgb(176, 176, 176),
            Self::rgb(236, 236, 236),
            Self::rgb(92, 92, 92),
        )
    }

    /// Omarchy White.
    pub fn white() -> Self {
        Self::omarchy_palette(
            Self::rgb(110, 110, 110),
            Self::rgb(0, 0, 0),
            Self::rgb(255, 255, 255),
            Self::rgb(192, 192, 192),
            Self::rgb(42, 42, 42),
            Self::rgb(58, 58, 58),
            Self::rgb(74, 74, 74),
            Self::rgb(26, 26, 26),
            Self::rgb(46, 46, 46),
            Self::rgb(62, 62, 62),
            Self::rgb(0, 0, 0),
            Self::rgb(192, 192, 192),
        )
    }

    /// Resolve a theme by name. Returns None for unknown names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().replace([' ', '_'], "-").as_str() {
            "catppuccin" | "catppuccin-mocha" | "mocha" => Some(Self::catppuccin()),
            "catppuccin-latte" | "latte" | "light" => Some(Self::catppuccin_latte()),
            "catppuccin-frappe" | "frappe" => Some(Self::catppuccin_frappe()),
            "catppuccin-macchiato" | "macchiato" => Some(Self::catppuccin_macchiato()),
            "terminal" => Some(Self::terminal()),
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
            "monokai-pro" | "monokai" => Some(Self::monokai_pro()),
            "monokai-pro-light" | "monokai-light" => Some(Self::monokai_pro_light()),
            "monokai-pro-light-sun" | "monokai-pro-sun" | "monokai-sun" | "sun" => {
                Some(Self::monokai_pro_light_sun())
            }
            "monokai-pro-spectrum" | "monokai-spectrum" | "spectrum" => {
                Some(Self::monokai_pro_spectrum())
            }
            "monokai-pro-ristretto" | "monokai-ristretto" | "ristretto" => {
                Some(Self::monokai_pro_ristretto())
            }
            "monokai-pro-octagon" | "monokai-octagon" | "octagon" => {
                Some(Self::monokai_pro_octagon())
            }
            "monokai-pro-machine" | "monokai-machine" | "machine" => {
                Some(Self::monokai_pro_machine())
            }
            "monokai-classic" | "classic" => Some(Self::monokai_classic()),
            "ethereal" => Some(Self::ethereal()),
            "everforest" => Some(Self::everforest()),
            "flexoki" => Some(Self::flexoki()),
            "flexoki-light" => Some(Self::flexoki_light()),
            "hackerman" => Some(Self::hackerman()),
            "last-horizon" => Some(Self::last_horizon()),
            "lumon" => Some(Self::lumon()),
            "matte-black" => Some(Self::matte_black()),
            "miasma" => Some(Self::miasma()),
            "osaka-jade" => Some(Self::osaka_jade()),
            "retro-82" => Some(Self::retro_82()),
            "solitude" => Some(Self::solitude()),
            "vantablack" => Some(Self::vantablack()),
            "white" => Some(Self::white()),
            _ => None,
        }
    }

    pub fn from_theme(
        name: &str,
        appearance: crate::terminal_theme::ThemeAppearance,
    ) -> Option<Self> {
        Self::from_theme_with_terminal(name, appearance, TerminalTheme::default())
    }

    pub fn from_theme_with_terminal_accent(
        name: &str,
        appearance: crate::terminal_theme::ThemeAppearance,
        host_theme: TerminalTheme,
        terminal_accent: TerminalAccent,
    ) -> Option<Self> {
        let theme_name = theme_name_for_appearance(name, appearance)?;
        if theme_name == "system" {
            return Some(Self::system(host_theme, appearance, terminal_accent));
        }
        Self::from_name(theme_name)
    }

    pub fn from_theme_with_terminal(
        name: &str,
        appearance: crate::terminal_theme::ThemeAppearance,
        host_theme: TerminalTheme,
    ) -> Option<Self> {
        Self::from_theme_with_terminal_accent(name, appearance, host_theme, TerminalAccent::Blue)
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
    ProductAnnouncement,
    Navigate,
    Prefix,
    Copy,
    Terminal,
    RenameWorkspace,
    RenameGroup,
    RenameTab,
    RenamePane,
    EditWorktreeDirectory,
    Resize,
    ConfirmClose,
    ConfirmDeleteGroup,
    ContextMenu,
    Settings,
    GlobalMenu,
    GroupMenu,
    AgentMenu,
    KeybindHelp,
    Navigator,
    CommandPalette,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NavigatorTarget {
    Workspace {
        ws_idx: usize,
    },
    Tab {
        ws_idx: usize,
        tab_idx: usize,
    },
    Pane {
        ws_idx: usize,
        tab_idx: usize,
        pane_id: PaneId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavigatorRow {
    pub target: NavigatorTarget,
    pub depth: u8,
    pub label: String,
    pub meta: String,
    pub status: AgentState,
    pub seen: bool,
    pub is_current: bool,
    pub is_workspace: bool,
    pub is_tab: bool,
    pub expanded: bool,
    pub search_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavigatorStateFilter {
    Blocked,
    Working,
    Idle,
    Done,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NavigatorState {
    pub query: String,
    pub selected: usize,
    pub scroll: usize,
    pub search_focused: bool,
    pub state_filter: Option<NavigatorStateFilter>,
    pub expanded_workspaces: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CopyModeState {
    pub pane_id: PaneId,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub entry_offset_from_bottom: usize,
    pub selection: Option<CopyModeSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyModeSelection {
    Character,
    Linewise { anchor_row: u32 },
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
    Theme,
    Layout,
    Sound,
    Toast,
    PaneLabels,
    Experiments,
    Integrations,
}

impl SettingsSection {
    pub const ALL: &[Self] = &[
        Self::Theme,
        Self::Layout,
        Self::Sound,
        Self::Toast,
        Self::PaneLabels,
        Self::Integrations,
        Self::Experiments,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "theme",
            Self::Layout => "layout",
            Self::Sound => "sound",
            Self::Toast => "toasts",
            Self::PaneLabels => "behavior",
            Self::Experiments => "experiments",
            Self::Integrations => "integrations",
        }
    }
}

pub const DEFAULT_DARK_THEME_NAME: &str = "catppuccin";
pub const DEFAULT_LIGHT_THEME_NAME: &str = "catppuccin-latte";

/// Legacy theme-family display order used where a single theme override is stored.
pub const THEME_NAMES: &[&str] = &[
    "system",
    DEFAULT_DARK_THEME_NAME,
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "dracula",
    "ethereal",
    "everforest",
    "flexoki",
    "gruvbox",
    "hackerman",
    "kanagawa",
    "last-horizon",
    "lumon",
    "matte-black",
    "miasma",
    "monokai-classic",
    "monokai-pro",
    "monokai-pro-machine",
    "monokai-pro-octagon",
    "monokai-pro-ristretto",
    "monokai-pro-spectrum",
    "nord",
    "one-dark",
    "osaka-jade",
    "retro-82",
    "rose-pine",
    "solarized",
    "solitude",
    "terminal",
    "tokyo-night",
    "vantablack",
    "vesper",
];

/// Built-in concrete themes that can render a light appearance.
pub const LIGHT_THEME_NAMES: &[&str] = &[
    DEFAULT_LIGHT_THEME_NAME,
    "flexoki-light",
    "gruvbox-light",
    "kanagawa-lotus",
    "monokai-pro-light",
    "monokai-pro-light-sun",
    "one-light",
    "rose-pine-dawn",
    "solarized-light",
    "tokyo-night-day",
    "white",
];

/// Built-in concrete themes that can render a dark appearance.
pub const DARK_THEME_NAMES: &[&str] = &[
    DEFAULT_DARK_THEME_NAME,
    "catppuccin-frappe",
    "catppuccin-macchiato",
    "dracula",
    "ethereal",
    "everforest",
    "flexoki",
    "gruvbox",
    "hackerman",
    "kanagawa",
    "last-horizon",
    "lumon",
    "matte-black",
    "miasma",
    "monokai-classic",
    "monokai-pro",
    "monokai-pro-machine",
    "monokai-pro-octagon",
    "monokai-pro-ristretto",
    "monokai-pro-spectrum",
    "nord",
    "one-dark",
    "osaka-jade",
    "retro-82",
    "rose-pine",
    "solarized",
    "solitude",
    "tokyo-night",
    "vantablack",
    "vesper",
];

pub fn normalize_theme_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_'], "-")
}

pub fn theme_names_for_appearance(appearance: ThemeAppearance) -> &'static [&'static str] {
    match appearance {
        ThemeAppearance::Light => LIGHT_THEME_NAMES,
        ThemeAppearance::Dark => DARK_THEME_NAMES,
    }
}

pub fn default_theme_name_for_appearance(appearance: ThemeAppearance) -> &'static str {
    match appearance {
        ThemeAppearance::Light => DEFAULT_LIGHT_THEME_NAME,
        ThemeAppearance::Dark => DEFAULT_DARK_THEME_NAME,
    }
}

pub fn theme_name_for_appearance(name: &str, appearance: ThemeAppearance) -> Option<&'static str> {
    let normalized = normalize_theme_name(name);
    match appearance {
        ThemeAppearance::Light => match normalized.as_str() {
            "system" => Some("system"),
            "terminal" => Some("terminal"),
            "catppuccin" | "catppuccin-mocha" | "catppuccin-latte" | "latte" | "light"
            | "mocha" => Some("catppuccin-latte"),
            "tokyo-night" | "tokyonight" | "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => {
                Some("tokyo-night-day")
            }
            "gruvbox" | "gruvbox-dark" | "gruvbox-light" => Some("gruvbox-light"),
            "one-dark" | "onedark" | "one-light" | "onelight" => Some("one-light"),
            "solarized" | "solarized-dark" | "solarized-light" => Some("solarized-light"),
            "kanagawa" | "kanagawa-lotus" | "lotus" => Some("kanagawa-lotus"),
            "rose-pine" | "rosepine" | "rose-pine-dawn" | "rosepine-dawn" | "dawn" => {
                Some("rose-pine-dawn")
            }
            "monokai-pro" | "monokai" | "monokai-pro-light" | "monokai-light" => {
                Some("monokai-pro-light")
            }
            "monokai-pro-light-sun" | "monokai-pro-sun" | "monokai-sun" | "sun" => {
                Some("monokai-pro-light-sun")
            }
            "flexoki" | "flexoki-light" => Some("flexoki-light"),
            "white" => Some("white"),
            "dracula"
            | "nord"
            | "vesper"
            | "catppuccin-frappe"
            | "frappe"
            | "catppuccin-macchiato"
            | "macchiato"
            | "monokai-pro-spectrum"
            | "monokai-spectrum"
            | "spectrum"
            | "monokai-pro-ristretto"
            | "monokai-ristretto"
            | "ristretto"
            | "monokai-pro-octagon"
            | "monokai-octagon"
            | "octagon"
            | "monokai-pro-machine"
            | "monokai-machine"
            | "machine"
            | "monokai-classic"
            | "classic"
            | "ethereal"
            | "everforest"
            | "hackerman"
            | "last-horizon"
            | "lumon"
            | "matte-black"
            | "miasma"
            | "osaka-jade"
            | "retro-82"
            | "solitude"
            | "vantablack" => None,
            _ => None,
        },
        ThemeAppearance::Dark => match normalized.as_str() {
            "system" => Some("system"),
            "terminal" => Some("terminal"),
            "catppuccin" | "catppuccin-mocha" | "mocha" | "catppuccin-latte" | "latte"
            | "light" => Some("catppuccin"),
            "catppuccin-frappe" | "frappe" => Some("catppuccin-frappe"),
            "catppuccin-macchiato" | "macchiato" => Some("catppuccin-macchiato"),
            "tokyo-night" | "tokyonight" | "tokyo-night-day" | "tokyo-day" | "tokyonight-day" => {
                Some("tokyo-night")
            }
            "dracula" => Some("dracula"),
            "nord" => Some("nord"),
            "gruvbox" | "gruvbox-dark" | "gruvbox-light" => Some("gruvbox"),
            "one-dark" | "onedark" | "one-light" | "onelight" => Some("one-dark"),
            "solarized" | "solarized-dark" | "solarized-light" => Some("solarized"),
            "kanagawa" | "kanagawa-lotus" | "lotus" => Some("kanagawa"),
            "rose-pine" | "rosepine" | "rose-pine-dawn" | "rosepine-dawn" | "dawn" => {
                Some("rose-pine")
            }
            "vesper" => Some("vesper"),
            "monokai-pro" | "monokai" | "monokai-pro-light" | "monokai-light" => {
                Some("monokai-pro")
            }
            "monokai-pro-spectrum" | "monokai-spectrum" | "spectrum" => {
                Some("monokai-pro-spectrum")
            }
            "monokai-pro-ristretto" | "monokai-ristretto" | "ristretto" => {
                Some("monokai-pro-ristretto")
            }
            "monokai-pro-octagon" | "monokai-octagon" | "octagon" => Some("monokai-pro-octagon"),
            "monokai-pro-machine" | "monokai-machine" | "machine" => Some("monokai-pro-machine"),
            "monokai-classic" | "classic" => Some("monokai-classic"),
            "ethereal" => Some("ethereal"),
            "everforest" => Some("everforest"),
            "flexoki" | "flexoki-light" => Some("flexoki"),
            "hackerman" => Some("hackerman"),
            "last-horizon" => Some("last-horizon"),
            "lumon" => Some("lumon"),
            "matte-black" => Some("matte-black"),
            "miasma" => Some("miasma"),
            "osaka-jade" => Some("osaka-jade"),
            "retro-82" => Some("retro-82"),
            "solitude" => Some("solitude"),
            "vantablack" => Some("vantablack"),
            _ => None,
        },
    }
}

pub fn theme_config_names(config: &ThemeConfig) -> (String, String) {
    let light = config
        .light
        .as_deref()
        .or(config.name.as_deref())
        .and_then(|name| theme_name_for_appearance(name, ThemeAppearance::Light))
        .unwrap_or(DEFAULT_LIGHT_THEME_NAME)
        .to_string();
    let dark = config
        .dark
        .as_deref()
        .or(config.name.as_deref())
        .and_then(|name| theme_name_for_appearance(name, ThemeAppearance::Dark))
        .unwrap_or(DEFAULT_DARK_THEME_NAME)
        .to_string();
    (light, dark)
}

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
    /// Pending light theme while settings is open.
    pub pending_light_theme_name: Option<String>,
    /// Pending dark theme while settings is open.
    pub pending_dark_theme_name: Option<String>,
    /// Pending terminal light accent while settings is open.
    pub pending_terminal_light_accent: Option<TerminalAccent>,
    /// Pending terminal dark accent while settings is open.
    pub pending_terminal_dark_accent: Option<TerminalAccent>,
    /// Pending sound setting while settings is open.
    pub pending_sound_enabled: Option<bool>,
    /// Pending toast delivery while settings is open.
    pub pending_toast_delivery: Option<ToastDelivery>,
    /// Pending workspace close confirmation setting while settings is open.
    pub pending_confirm_close: Option<bool>,
    /// Pending new-tab naming prompt setting while settings is open.
    pub pending_prompt_new_tab_name: Option<bool>,
    /// Pending new-terminal cwd policy while settings is open.
    pub pending_new_terminal_cwd: Option<NewTerminalCwdConfig>,
    /// Pending mouse wheel scroll amount while settings is open.
    pub pending_mouse_scroll_lines: Option<usize>,
    /// Pending default sidebar width while settings is open.
    pub pending_sidebar_width: Option<u16>,
    /// Pending minimum expanded sidebar width while settings is open.
    pub pending_sidebar_min_width: Option<u16>,
    /// Pending maximum expanded sidebar width while settings is open.
    pub pending_sidebar_max_width: Option<u16>,
    /// Pending worktree checkout parent directory while settings is open.
    pub pending_worktree_directory: Option<String>,
    /// Pending agent border label setting while settings is open.
    pub pending_agent_border_labels: Option<bool>,
    /// Pending native agent resume setting while settings is open.
    pub pending_resume_agents_on_restore: Option<bool>,
    /// Pending macOS prefix input source switching setting while settings is open.
    pub pending_switch_ascii_input_source_in_prefix: Option<bool>,
    /// Checked group accent while group settings is open; hover cursor is separate.
    pub pending_group_accent_choice: Option<Option<TerminalAccent>>,
    /// Group whose settings are being edited, if settings was opened from a group menu.
    pub group_settings_target: Option<usize>,
}

pub(crate) enum DragTarget {
    WorkspaceReorder {
        source_ws_idx: usize,
        insert_idx: Option<usize>,
        target_group_idx: Option<usize>,
        indicator_row: Option<u16>,
    },
    GroupReorder {
        source_group_idx: usize,
        insert_idx: Option<usize>,
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
    ProductAnnouncementScrollbar {
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

pub(crate) struct GroupPressState {
    pub group_idx: usize,
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
            } => &["settings", "rename", "---", "delete"],
            ContextMenuKind::Group {
                can_delete: false, ..
            } => &["settings", "rename"],
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
                "zoom",
                "close pane",
            ],
            ContextMenuKind::Pane {
                has_manual_label: false,
                ..
            } => &[
                "rename pane",
                "split vertical",
                "split horizontal",
                "zoom",
                "close pane",
            ],
        }
    }

    pub fn item_is_selectable(&self, idx: usize) -> bool {
        self.items()
            .get(idx)
            .is_some_and(|item| !Self::item_is_separator(item))
    }

    pub fn item_is_separator(item: &str) -> bool {
        item == "---"
    }

    pub fn move_prev(&mut self) {
        if self.list.highlighted == 0 {
            return;
        }

        let mut idx = self.list.highlighted - 1;
        loop {
            if self.item_is_selectable(idx) {
                self.list.highlighted = idx;
                return;
            }
            if idx == 0 {
                return;
            }
            idx -= 1;
        }
    }

    pub fn move_next(&mut self) {
        let item_count = self.items().len();
        let mut idx = self.list.highlighted.saturating_add(1);
        while idx < item_count {
            if self.item_is_selectable(idx) {
                self.list.highlighted = idx;
                return;
            }
            idx += 1;
        }
    }

    pub fn hover(&mut self, idx: Option<usize>) {
        if let Some(idx) = idx {
            if self.item_is_selectable(idx) {
                self.list.highlighted = idx;
            }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyFeedback {
    pub message: String,
}

pub struct ReleaseNotesState {
    pub version: String,
    pub body: String,
    pub scroll: u16,
    pub preview: bool,
}

pub struct ProductAnnouncementState {
    pub version: String,
    pub id: String,
    pub title: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPanelAction {
    RunOrFocus(String),
    Stop(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarWidthSource {
    ConfigDefault,
    Persisted,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneFocusTarget {
    pub workspace_id: String,
    pub pane_id: PaneId,
}

/// All application state — pure data, no channels or async runtime.
/// Testable without PTYs or a tokio runtime.
pub struct AppState {
    pub groups: Vec<Group>,
    pub active_group: usize,
    pub group_filter_enabled: bool,
    pub terminals:
        std::collections::HashMap<crate::terminal::TerminalId, crate::terminal::TerminalState>,
    /// Terminal ids whose size is currently owned by a direct attach client.
    pub direct_attach_resize_locks: std::collections::HashSet<crate::terminal::TerminalId>,
    pub(crate) pane_id_aliases: std::collections::HashMap<u32, PaneId>,
    pub workspaces: Vec<Workspace>,
    pub active: Option<usize>,
    pub(crate) previous_pane_focus: Option<PaneFocusTarget>,
    pub selected: usize,
    pub mode: Mode,
    pub should_quit: bool,
    /// In monolithic --no-session mode, detach exits the app because there is no server to detach from.
    pub detach_exits: bool,
    /// Set when the current client should detach from the persistent session.
    /// The server's event loop checks this and handles client detach.
    pub detach_requested: bool,
    pub request_new_workspace: bool,
    pub request_new_tab: bool,
    pub request_reload_config: bool,
    pub request_open_git_diff: bool,
    /// Set when the headless server should ask attached clients to reload
    /// their client-local sound config from disk.
    pub request_client_config_reload: bool,
    /// Set when UI interaction requested a clipboard write that must be
    /// handled by the outer App/event loop instead of directly from AppState.
    pub request_clipboard_write: Option<Vec<u8>>,
    pub request_command_action: Option<CommandPanelAction>,
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
    pub product_announcement: Option<ProductAnnouncementState>,
    pub keybind_help: KeybindHelpState,
    pub navigator: NavigatorState,
    pub command_palette: CommandPaletteState,
    pub command_catalog: Vec<crate::commands::ProjectCommand>,
    pub command_runs: std::collections::HashMap<String, crate::commands::CommandRun>,
    pub port_registry: crate::ports::PortRegistry,
    pub copy_mode: Option<CopyModeState>,
    pub workspace_scroll: usize,
    pub agent_panel_scroll: usize,
    pub tab_scroll: usize,
    pub tab_scroll_follow_active: bool,
    pub mobile_switcher_scroll: usize,
    // View geometry (computed before render, consumed by render + mouse)
    pub view: ViewState,
    pub(crate) drag: Option<DragState>,
    pub(crate) workspace_press: Option<WorkspacePressState>,
    pub(crate) group_press: Option<GroupPressState>,
    pub(crate) tab_press: Option<TabPressState>,
    pub selection: Option<Selection>,
    pub selection_autoscroll: Option<SelectionAutoscroll>,
    pub context_menu: Option<ContextMenuState>,
    // Notifications
    pub update_available: Option<String>,
    pub update_install_command: String,
    pub latest_release_notes_available: bool,
    pub update_dismissed: bool,
    pub config_diagnostic: Option<String>,
    pub toast: Option<ToastNotification>,
    pub copy_feedback: Option<CopyFeedback>,
    /// Last reported focus state for the outer terminal hosting hako.
    /// None means unsupported or not yet reported, which preserves active-pane suppression.
    pub outer_terminal_focus: Option<bool>,
    // Config
    pub prefix_code: KeyCode,
    pub prefix_mods: KeyModifiers,
    pub default_sidebar_width: u16,
    pub sidebar_width: u16,
    pub sidebar_min_width: u16,
    pub sidebar_max_width: u16,
    pub mobile_width_threshold: u16,
    pub sidebar_width_source: SidebarWidthSource,
    pub sidebar_width_auto: bool,
    pub sidebar_collapsed: bool,
    pub right_sidebar_width: u16,
    pub right_sidebar_collapsed: bool,
    /// Ratio of sidebar height allocated to the workspaces section.
    pub sidebar_section_split: f32,
    pub activity_agents_expanded: bool,
    pub activity_commands_expanded: bool,
    pub activity_ports_expanded: bool,
    pub collapsed_agent_sections: Vec<String>,
    pub collapsed_command_groups: Vec<String>,
    pub collapsed_command_status_groups: Vec<String>,
    pub collapsed_workspace_groups: Vec<String>,
    pub agent_panel_scope: AgentPanelScope,
    /// Capture mouse input for Hako's own mouse UI. When false, Hako only
    /// captures mouse while the focused pane app requests mouse reporting.
    pub mouse_capture: bool,
    pub right_click_passthrough_modifiers: Option<KeyModifiers>,
    pub right_click_passthrough: Option<RightClickPassthroughGesture>,
    pub redraw_on_focus_gained: bool,
    pub mouse_scroll_lines: usize,
    pub confirm_close: bool,
    pub prompt_new_tab_name: bool,
    pub show_agent_labels_on_pane_borders: bool,
    pub pane_history_persistence: bool,
    pub resume_agents_on_restore: bool,
    /// Expose the focused pane's cursor anchor to the outer terminal even when
    /// the pane requested `?25l`. See `[experimental] reveal_hidden_cursor_for_cjk_ime`.
    pub reveal_hidden_cursor_for_cjk_ime: bool,
    /// Restrict cursor reveal to focused panes whose detected agent matches
    /// one of these. When false, apply to any focused pane.
    pub cjk_ime_agent_filter_configured: bool,
    pub cjk_ime_agents: Vec<crate::detect::Agent>,
    /// DECSCUSR shape parameter (1–6) for the IME anchor cursor.
    pub cjk_ime_cursor_shape: u8,
    /// While prefix mode is active, switch the macOS host input source to an
    /// ASCII-capable layout so prefix commands register as ASCII even when a
    /// CJK IME is active. macOS only; a no-op elsewhere. See
    /// `[experimental] switch_ascii_input_source_in_prefix`.
    pub switch_ascii_input_source_in_prefix: bool,
    pub kitty_graphics_enabled: bool,
    pub default_shell: String,
    pub shell_mode: crate::config::ShellModeConfig,
    pub new_terminal_cwd: NewTerminalCwdConfig,
    pub pane_scrollback_limit_bytes: usize,
    pub worktree_directory: PathBuf,
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
    /// Default app light theme from config.
    pub global_light_theme_name: String,
    /// Default app dark theme from config.
    pub global_dark_theme_name: String,
    /// ANSI color used for the app accent when terminal colors resolve light.
    pub global_terminal_light_accent: TerminalAccent,
    /// ANSI color used for the app accent when terminal colors resolve dark.
    pub global_terminal_dark_accent: TerminalAccent,
    /// Custom color overrides from config, applied only to the global fallback theme.
    pub global_theme_custom: Option<CustomThemeColors>,
    /// Whether legacy `ui.accent` should override the global theme accent.
    pub global_theme_use_legacy_ui_accent: bool,
    /// Settings panel state.
    pub settings: SettingsState,
    /// Cached integration recommendations for onboarding/settings UI.
    pub integration_recommendations: Vec<crate::integration::IntegrationRecommendation>,
    /// Result messages from the latest integration install action.
    pub integration_install_messages: Vec<String>,
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
    /// Terminal runtimes that should be shut down by the app/runtime layer
    /// after state has detached their terminal metadata.
    pub(crate) terminal_runtime_shutdowns: Vec<crate::terminal::TerminalId>,
}

impl AppState {
    pub fn theme_appearance_for_mode(&self, mode: ThemeMode) -> ThemeAppearance {
        mode.resolve(self.host_terminal_theme)
    }
    pub fn global_theme_name_for_appearance(&self, appearance: ThemeAppearance) -> &str {
        match appearance {
            ThemeAppearance::Light => &self.global_light_theme_name,
            ThemeAppearance::Dark => &self.global_dark_theme_name,
        }
    }

    pub fn global_theme_name_for_mode(&self, mode: ThemeMode) -> &str {
        self.global_theme_name_for_appearance(self.theme_appearance_for_mode(mode))
    }

    pub fn palette_for_theme_mode(&self, theme_name: &str, mode: ThemeMode) -> Option<Palette> {
        self.palette_for_theme_mode_with_terminal_accents(
            theme_name,
            mode,
            self.global_terminal_light_accent,
            self.global_terminal_dark_accent,
        )
    }

    pub fn terminal_accent_for_mode(&self, mode: ThemeMode) -> TerminalAccent {
        match self.theme_appearance_for_mode(mode) {
            crate::terminal_theme::ThemeAppearance::Light => self.global_terminal_light_accent,
            crate::terminal_theme::ThemeAppearance::Dark => self.global_terminal_dark_accent,
        }
    }

    pub fn palette_for_theme_mode_with_terminal_accents(
        &self,
        theme_name: &str,
        mode: ThemeMode,
        terminal_light_accent: TerminalAccent,
        terminal_dark_accent: TerminalAccent,
    ) -> Option<Palette> {
        let appearance = self.theme_appearance_for_mode(mode);
        let accent = match appearance {
            crate::terminal_theme::ThemeAppearance::Light => terminal_light_accent,
            crate::terminal_theme::ThemeAppearance::Dark => terminal_dark_accent,
        };
        Palette::from_theme_with_terminal_accent(
            theme_name,
            appearance,
            self.host_terminal_theme,
            accent,
        )
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
        let theme_name = self
            .global_theme_name_for_mode(self.global_theme_mode)
            .to_string();
        if let Some(palette) = self.configured_global_palette(&theme_name, self.global_theme_mode) {
            self.global_palette = palette;
            self.global_theme_name = theme_name;
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

    pub fn group_accent_color(&self, group_idx: usize) -> Color {
        self.groups
            .get(group_idx)
            .and_then(|group| group.accent)
            .map(|accent| Palette::terminal_accent_color(self.host_terminal_theme, accent))
            .unwrap_or(self.global_palette.accent)
    }

    pub fn group_index_by_id(&self, group_id: &str) -> Option<usize> {
        self.groups.iter().position(|group| group.id == group_id)
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

    pub fn command_group_collapsed(&self, group_key: &str) -> bool {
        self.collapsed_command_groups
            .iter()
            .any(|key| key == group_key)
    }

    pub fn command_status_group_collapsed(&self, group_key: &str) -> bool {
        self.collapsed_command_status_groups
            .iter()
            .any(|key| key == group_key)
    }

    pub fn agent_section_collapsed(&self, section_key: &str) -> bool {
        self.collapsed_agent_sections
            .iter()
            .any(|key| key == section_key)
    }

    pub fn toggle_command_group(&mut self, group_key: String) {
        toggle_string_key(&mut self.collapsed_command_groups, group_key);
    }

    pub fn toggle_command_status_group(&mut self, group_key: String) {
        toggle_string_key(&mut self.collapsed_command_status_groups, group_key);
    }

    pub fn toggle_agent_section(&mut self, section_key: String) {
        toggle_string_key(&mut self.collapsed_agent_sections, section_key);
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
        let previous_selected = self.selected;
        if let Some(idx) = self
            .collapsed_workspace_groups
            .iter()
            .position(|id| id == &group_id)
        {
            self.collapsed_workspace_groups.remove(idx);
        } else {
            self.collapsed_workspace_groups.push(group_id);
        }
        self.workspace_scroll = self
            .workspace_scroll
            .min(crate::ui::workspace_list_entry_count(self).saturating_sub(1));
        if !self
            .sidebar_visible_workspace_indices()
            .contains(&self.selected)
        {
            let visible = self.sidebar_visible_workspace_indices();
            if let Some(next) = visible
                .iter()
                .copied()
                .find(|idx| *idx > previous_selected)
                .or_else(|| {
                    visible
                        .iter()
                        .rev()
                        .copied()
                        .find(|idx| *idx < previous_selected)
                })
                .or_else(|| visible.first().copied())
            {
                self.selected = next;
                self.ensure_workspace_visible(next);
            }
        }
        self.mark_session_dirty();
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

    pub(crate) fn remove_alias_shadowed_by_new_pane(&mut self, pane_id: PaneId) {
        self.pane_id_aliases.remove(&pane_id.raw());
    }

    pub fn sound_enabled(&self) -> bool {
        self.sound.enabled
    }

    pub fn toast_delivery(&self) -> ToastDelivery {
        self.toast_config.delivery
    }

    pub fn confirm_close_enabled(&self) -> bool {
        self.confirm_close
    }

    pub fn prompt_new_tab_name_enabled(&self) -> bool {
        self.prompt_new_tab_name
    }

    pub fn agent_border_labels_enabled(&self) -> bool {
        self.show_agent_labels_on_pane_borders
    }

    pub fn pane_history_persistence_enabled(&self) -> bool {
        self.pane_history_persistence
    }

    pub fn resume_agents_on_restore_enabled(&self) -> bool {
        self.resume_agents_on_restore
    }

    pub fn switch_ascii_input_source_in_prefix_enabled(&self) -> bool {
        self.switch_ascii_input_source_in_prefix
    }

    pub(crate) fn integration_updates_available(&self) -> bool {
        self.integration_recommendations
            .iter()
            .any(|item| item.state == crate::integration::IntegrationStatusKind::Outdated)
    }

    pub(crate) fn global_menu_attention_badge_visible(&self) -> bool {
        self.update_available.is_some() || self.integration_updates_available()
    }

    pub(crate) fn global_menu_item_has_badge(&self, item: &str) -> bool {
        (item == "update ready" && self.update_available.is_some())
            || (item == "settings" && self.integration_updates_available())
    }

    pub(crate) fn settings_section_has_badge(&self, section: SettingsSection) -> bool {
        section == SettingsSection::Integrations && self.integration_updates_available()
    }

    pub(crate) fn focused_pane_requests_mouse_capture_from(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> bool {
        self.mode == Mode::Terminal
            && self
                .active
                .and_then(|idx| self.focused_runtime_in_workspace(terminal_runtimes, idx))
                .and_then(crate::terminal::TerminalRuntime::input_state)
                .is_some_and(crate::pane::InputState::mouse_reporting_enabled)
    }

    pub(crate) fn should_capture_host_mouse_from(
        &self,
        terminal_runtimes: &crate::terminal::TerminalRuntimeRegistry,
    ) -> bool {
        self.mouse_capture || self.focused_pane_requests_mouse_capture_from(terminal_runtimes)
    }

    pub fn is_prefix_key(&self, key: crate::input::TerminalKey) -> bool {
        crate::config::terminal_key_matches_combo(key, (self.prefix_code, self.prefix_mods))
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
    pub(crate) fn runtime_for_pane_in_workspace<'a>(
        &'a self,
        terminal_runtimes: &'a crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
        pane_id: crate::layout::PaneId,
    ) -> Option<&'a crate::terminal::TerminalRuntime> {
        #[cfg(test)]
        if let Some(runtime) = self.workspaces.get(ws_idx)?.test_runtimes.get(&pane_id) {
            return Some(runtime);
        }
        #[cfg(test)]
        if let Some(runtime) = self
            .workspaces
            .get(ws_idx)?
            .tabs
            .iter()
            .find_map(|tab| tab.runtimes.get(&pane_id))
        {
            return Some(runtime);
        }
        let terminal_id = self.workspaces.get(ws_idx)?.terminal_id(pane_id)?;
        terminal_runtimes.get(terminal_id)
    }

    #[cfg(test)]
    pub(crate) fn runtime_for_pane<'a>(
        &'a self,
        terminal_runtimes: &'a crate::terminal::TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
    ) -> Option<&'a crate::terminal::TerminalRuntime> {
        self.workspaces.iter().find_map(|ws| {
            #[cfg(test)]
            if let Some(runtime) = ws.test_runtimes.get(&pane_id) {
                return Some(runtime);
            }
            #[cfg(test)]
            if let Some(runtime) = ws.tabs.iter().find_map(|tab| tab.runtimes.get(&pane_id)) {
                return Some(runtime);
            }
            let terminal_id = ws.terminal_id(pane_id)?;
            terminal_runtimes.get(terminal_id)
        })
    }

    pub(crate) fn focused_runtime_in_workspace<'a>(
        &'a self,
        terminal_runtimes: &'a crate::terminal::TerminalRuntimeRegistry,
        ws_idx: usize,
    ) -> Option<&'a crate::terminal::TerminalRuntime> {
        let ws = self.workspaces.get(ws_idx)?;
        let pane_id = ws.focused_pane_id()?;
        self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
    }

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

fn toggle_string_key(keys: &mut Vec<String>, key: String) {
    if let Some(idx) = keys.iter().position(|existing| existing == &key) {
        keys.remove(idx);
    } else {
        keys.push(key);
    }
}

#[allow(dead_code)]
pub fn key_matches(
    key: &crossterm::event::KeyEvent,
    expected_code: KeyCode,
    expected_mods: KeyModifiers,
) -> bool {
    crate::config::terminal_key_matches_combo(
        crate::input::TerminalKey::from(*key),
        (expected_code, expected_mods),
    )
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
            terminals: std::collections::HashMap::new(),
            direct_attach_resize_locks: std::collections::HashSet::new(),
            pane_id_aliases: std::collections::HashMap::new(),
            workspaces: Vec::new(),
            active: None,
            selected: 0,
            mode: Mode::Navigate,
            should_quit: false,
            detach_exits: false,
            detach_requested: false,
            request_new_workspace: false,
            request_new_tab: false,
            request_reload_config: false,
            request_open_git_diff: false,
            request_client_config_reload: false,
            request_clipboard_write: None,
            request_command_action: None,
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
            product_announcement: None,
            keybind_help: KeybindHelpState { scroll: 0 },
            command_palette: CommandPaletteState {
                query: String::new(),
                selected: 0,
                scroll: 0,
            },
            navigator: NavigatorState::default(),
            previous_pane_focus: None,
            command_catalog: Vec::new(),
            command_runs: std::collections::HashMap::new(),
            port_registry: crate::ports::PortRegistry::default(),
            copy_mode: None,
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
            group_press: None,
            tab_press: None,
            selection: None,
            selection_autoscroll: None,
            context_menu: None,
            update_available: None,
            update_install_command: "hako update".into(),
            latest_release_notes_available: false,
            update_dismissed: false,
            config_diagnostic: None,
            toast: None,
            outer_terminal_focus: None,
            prefix_code: KeyCode::Char('b'),
            prefix_mods: KeyModifiers::CONTROL,
            default_sidebar_width: 26,
            sidebar_width: 26,
            sidebar_min_width: 18,
            sidebar_max_width: 36,
            sidebar_width_source: SidebarWidthSource::ConfigDefault,
            sidebar_width_auto: false,
            sidebar_collapsed: false,
            right_sidebar_width: 28,
            right_sidebar_collapsed: false,
            sidebar_section_split: 0.5,
            activity_agents_expanded: true,
            activity_commands_expanded: false,
            activity_ports_expanded: false,
            collapsed_agent_sections: Vec::new(),
            collapsed_command_groups: Vec::new(),
            collapsed_command_status_groups: Vec::new(),
            collapsed_workspace_groups: Vec::new(),
            agent_panel_scope: AgentPanelScope::CurrentWorkspace,
            mouse_capture: true,
            right_click_passthrough_modifiers: None,
            right_click_passthrough: None,
            redraw_on_focus_gained: true,
            mouse_scroll_lines: crate::config::DEFAULT_MOUSE_SCROLL_LINES,
            confirm_close: true,
            prompt_new_tab_name: true,
            copy_feedback: None,
            show_agent_labels_on_pane_borders: false,
            mobile_width_threshold: crate::config::DEFAULT_MOBILE_WIDTH_THRESHOLD,
            pane_history_persistence: false,
            resume_agents_on_restore: false,
            reveal_hidden_cursor_for_cjk_ime: false,
            cjk_ime_agent_filter_configured: false,
            cjk_ime_agents: Vec::new(),
            cjk_ime_cursor_shape: 2, // steady_block
            switch_ascii_input_source_in_prefix: false,
            kitty_graphics_enabled: false,
            default_shell: String::new(),
            shell_mode: crate::config::ShellModeConfig::Auto,
            new_terminal_cwd: NewTerminalCwdConfig::Follow,
            pane_scrollback_limit_bytes: crate::config::DEFAULT_SCROLLBACK_LIMIT_BYTES,
            worktree_directory: PathBuf::from("/tmp/hako-worktrees"),
            accent: Color::Cyan,
            sound: SoundConfig {
                enabled: false,
                ..SoundConfig::default()
            },
            local_sound_playback: false,
            toast_config: ToastConfig::default(),
            keybinds: Keybinds::default(),
            spinner_tick: 0,
            palette: Palette::catppuccin(),
            global_palette: Palette::catppuccin(),
            theme_name: "catppuccin".to_string(),
            global_theme_name: "catppuccin".to_string(),
            global_theme_mode: ThemeMode::System,
            global_light_theme_name: DEFAULT_LIGHT_THEME_NAME.to_string(),
            global_dark_theme_name: DEFAULT_DARK_THEME_NAME.to_string(),
            global_terminal_light_accent: TerminalAccent::Blue,
            global_terminal_dark_accent: TerminalAccent::Blue,
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
                pending_light_theme_name: None,
                pending_dark_theme_name: None,
                pending_terminal_light_accent: None,
                pending_terminal_dark_accent: None,
                pending_sound_enabled: None,
                pending_toast_delivery: None,
                pending_confirm_close: None,
                pending_prompt_new_tab_name: None,
                pending_new_terminal_cwd: None,
                pending_mouse_scroll_lines: None,
                pending_sidebar_width: None,
                pending_sidebar_min_width: None,
                pending_sidebar_max_width: None,
                pending_worktree_directory: None,
                pending_agent_border_labels: None,
                pending_resume_agents_on_restore: None,
                pending_switch_ascii_input_source_in_prefix: None,
                pending_group_accent_choice: None,
                group_settings_target: None,
            },
            integration_recommendations: Vec::new(),
            integration_install_messages: Vec::new(),
            global_menu: MenuListState::new(0),
            group_menu: MenuListState::new(0),
            agent_menu: MenuListState::new(0),
            host_terminal_theme: TerminalTheme::default(),
            session_dirty: false,
            terminal_runtime_shutdowns: Vec::new(),
        }
    }

    /// Populate missing `TerminalState` entries for every pane so tests that
    /// read or write terminal metadata don't need to manually create them.
    pub fn ensure_test_terminals(&mut self) {
        use crate::terminal::TerminalState;
        for ws in &self.workspaces {
            for tab in &ws.tabs {
                for pane in tab.panes.values() {
                    if !self.terminals.contains_key(&pane.attached_terminal_id) {
                        let cwd = ws.identity_cwd.clone();
                        self.terminals.insert(
                            pane.attached_terminal_id.clone(),
                            TerminalState::new(pane.attached_terminal_id.clone(), cwd),
                        );
                    }
                }
            }
        }
    }

    pub fn insert_test_runtime(
        &mut self,
        pane_id: crate::layout::PaneId,
        runtime: crate::terminal::TerminalRuntime,
    ) {
        if let Some(ws) = self
            .workspaces
            .iter_mut()
            .find(|ws| ws.terminal_id(pane_id).is_some())
        {
            ws.insert_test_runtime(pane_id, runtime);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    #[test]
    fn group_icons_are_fun_distinct_set() {
        assert_eq!(
            GROUP_ICONS,
            &[
                "☀", "☁", "☂", "☕", "♥", "♪", "⚑", "⚙", "☎", "☄", "☘", "✉", "⚓", "✿", "✂", "✎",
                "✚", "⊕", "▥", "⌁",
            ]
        );
        assert_eq!(DEFAULT_GROUP_ICON, GROUP_ICONS[0]);
    }

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
    fn light_theme_names_resolve_to_light_appearance() {
        for name in LIGHT_THEME_NAMES {
            assert!(
                Palette::from_theme(name, ThemeAppearance::Light).is_some(),
                "light theme should resolve: {name}"
            );
        }
    }
    #[test]
    fn monokai_pro_variants_resolve() {
        for name in [
            "monokai-pro",
            "monokai-pro-light",
            "monokai-pro-light-sun",
            "monokai-pro-spectrum",
            "monokai-pro-ristretto",
            "monokai-pro-octagon",
            "monokai-pro-machine",
            "monokai-classic",
        ] {
            assert!(
                Palette::from_name(name).is_some(),
                "monokai variant should resolve: {name}"
            );
        }
    }
    #[test]
    fn catppuccin_flavors_resolve() {
        for name in [
            "catppuccin-latte",
            "catppuccin-frappe",
            "catppuccin-macchiato",
            "catppuccin",
            "catppuccin-mocha",
        ] {
            assert!(
                Palette::from_name(name).is_some(),
                "catppuccin flavor should resolve: {name}"
            );
        }
    }

    #[test]
    fn catppuccin_flavors_use_official_palette_values() {
        let latte = Palette::from_name("catppuccin-latte").expect("latte");
        assert_eq!(latte.panel_bg, Color::Rgb(239, 241, 245));
        assert_eq!(latte.surface0, Color::Rgb(204, 208, 218));
        assert_eq!(latte.text, Color::Rgb(76, 79, 105));

        let frappe = Palette::from_name("catppuccin-frappe").expect("frappe");
        assert_eq!(frappe.panel_bg, Color::Rgb(48, 52, 70));
        assert_eq!(frappe.surface0, Color::Rgb(65, 69, 89));
        assert_eq!(frappe.text, Color::Rgb(198, 208, 245));

        let macchiato = Palette::from_name("catppuccin-macchiato").expect("macchiato");
        assert_eq!(macchiato.panel_bg, Color::Rgb(36, 39, 58));
        assert_eq!(macchiato.surface0, Color::Rgb(54, 58, 79));
        assert_eq!(macchiato.text, Color::Rgb(202, 211, 245));

        let mocha = Palette::from_name("catppuccin").expect("mocha");
        assert_eq!(mocha.panel_bg, Color::Rgb(30, 30, 46));
        assert_eq!(mocha.surface0, Color::Rgb(49, 50, 68));
        assert_eq!(mocha.text, Color::Rgb(205, 214, 244));
    }
    #[test]
    fn flexoki_variants_use_official_website_values() {
        let light = Palette::from_name("flexoki-light").expect("flexoki light");
        assert_eq!(light.accent, Color::Rgb(36, 131, 123));
        assert_eq!(light.panel_bg, Color::Rgb(255, 252, 240));
        assert_eq!(light.surface_dim, Color::Rgb(242, 240, 229));
        assert_eq!(light.surface0, Color::Rgb(230, 228, 217));
        assert_eq!(light.surface1, Color::Rgb(206, 205, 195));
        assert_eq!(light.text, Color::Rgb(16, 15, 15));

        let dark = Palette::from_name("flexoki").expect("flexoki");
        assert_eq!(dark.accent, Color::Rgb(58, 169, 159));
        assert_eq!(dark.panel_bg, Color::Rgb(16, 15, 15));
        assert_eq!(dark.surface_dim, Color::Rgb(28, 27, 26));
        assert_eq!(dark.surface0, Color::Rgb(40, 39, 38));
        assert_eq!(dark.surface1, Color::Rgb(64, 62, 60));
        assert_eq!(dark.text, Color::Rgb(206, 205, 195));
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
    fn appearance_theme_lists_do_not_include_terminal_color_sources() {
        for names in [
            theme_names_for_appearance(ThemeAppearance::Light),
            theme_names_for_appearance(ThemeAppearance::Dark),
        ] {
            assert!(!names.contains(&"system"));
            assert!(!names.contains(&"terminal"));
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
    fn dark_only_theme_is_not_valid_in_light_mode() {
        assert!(Palette::from_theme("nord", ThemeAppearance::Light).is_none());
    }

    #[test]
    fn theme_config_names_derive_appearance_pair_from_legacy_name() {
        let config = ThemeConfig {
            name: Some("gruvbox".to_string()),
            ..ThemeConfig::default()
        };

        assert_eq!(
            theme_config_names(&config),
            ("gruvbox-light".to_string(), "gruvbox".to_string())
        );
    }

    #[test]
    fn system_theme_uses_terminal_defaults_and_ansi_palette() {
        let mut host_theme = TerminalTheme {
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
            ..Default::default()
        };
        host_theme.palette[1] = Some(crate::terminal_theme::RgbColor {
            r: 180,
            g: 40,
            b: 50,
        });
        host_theme.palette[2] = Some(crate::terminal_theme::RgbColor {
            r: 30,
            g: 160,
            b: 80,
        });
        host_theme.palette[3] = Some(crate::terminal_theme::RgbColor {
            r: 210,
            g: 170,
            b: 30,
        });
        host_theme.palette[4] = Some(crate::terminal_theme::RgbColor {
            r: 80,
            g: 130,
            b: 230,
        });
        host_theme.palette[5] = Some(crate::terminal_theme::RgbColor {
            r: 160,
            g: 90,
            b: 200,
        });
        host_theme.palette[6] = Some(crate::terminal_theme::RgbColor {
            r: 30,
            g: 180,
            b: 170,
        });
        host_theme.palette[7] = Some(crate::terminal_theme::RgbColor {
            r: 210,
            g: 211,
            b: 212,
        });
        host_theme.palette[8] = Some(crate::terminal_theme::RgbColor {
            r: 120,
            g: 121,
            b: 122,
        });

        let palette =
            Palette::from_theme_with_terminal("system", ThemeAppearance::Dark, host_theme)
                .expect("system theme resolves");

        assert_eq!(palette.panel_bg, Color::Reset);
        assert_eq!(palette.text, Color::Rgb(220, 221, 222));
        assert_eq!(palette.overlay0, Color::Rgb(126, 127, 128));
        assert_eq!(palette.overlay1, Color::Rgb(178, 179, 180));
        assert_eq!(palette.subtext0, Color::Rgb(147, 148, 149));
        assert_eq!(palette.accent, Color::Rgb(80, 130, 230));
        assert_eq!(palette.green, Color::Rgb(30, 160, 80));
        assert_eq!(palette.yellow, Color::Rgb(210, 170, 30));
        assert_eq!(palette.red, Color::Rgb(180, 40, 50));
        assert_eq!(palette.blue, Color::Rgb(80, 130, 230));
        assert_eq!(palette.teal, Color::Rgb(30, 180, 170));
        assert_eq!(palette.mauve, Color::Rgb(160, 90, 200));
        assert_ne!(palette.accent, palette.text);
        assert_ne!(palette.surface0, Palette::catppuccin().surface0);
    }

    #[test]
    fn system_theme_uses_selected_terminal_accent() {
        let mut host_theme = TerminalTheme::default();
        host_theme.palette[5] = Some(crate::terminal_theme::RgbColor {
            r: 160,
            g: 90,
            b: 200,
        });

        let palette = Palette::from_theme_with_terminal_accent(
            "system",
            ThemeAppearance::Dark,
            host_theme,
            crate::config::TerminalAccent::Magenta,
        )
        .expect("system theme resolves");

        assert_eq!(palette.accent, Color::Rgb(160, 90, 200));
        assert_eq!(palette.blue, Color::Blue);
    }

    #[test]
    fn system_theme_derives_neutral_text_from_terminal_foreground() {
        let mut host_theme = TerminalTheme {
            foreground: Some(crate::terminal_theme::RgbColor { r: 0, g: 0, b: 0 }),
            background: Some(crate::terminal_theme::RgbColor {
                r: 255,
                g: 255,
                b: 255,
            }),
            ..Default::default()
        };
        host_theme.palette[7] = Some(crate::terminal_theme::RgbColor {
            r: 250,
            g: 250,
            b: 250,
        });
        host_theme.palette[8] = Some(crate::terminal_theme::RgbColor {
            r: 230,
            g: 230,
            b: 230,
        });

        let palette =
            Palette::from_theme_with_terminal("system", ThemeAppearance::Light, host_theme)
                .expect("system theme resolves");

        assert_eq!(palette.text, Color::Rgb(0, 0, 0));
        assert_eq!(palette.overlay0, Color::Rgb(115, 115, 115));
        assert_eq!(palette.overlay1, Color::Rgb(51, 51, 51));
        assert_eq!(palette.subtext0, Color::Rgb(89, 89, 89));
        assert_ne!(palette.overlay0, Color::Rgb(230, 230, 230));
        assert_ne!(palette.overlay1, Color::Rgb(250, 250, 250));
    }

    #[test]
    fn system_theme_falls_back_to_ansi_colors_not_catppuccin() {
        let palette = Palette::from_theme_with_terminal(
            "system",
            ThemeAppearance::Dark,
            TerminalTheme::default(),
        )
        .expect("system theme resolves");

        assert_eq!(palette.panel_bg, Color::Reset);
        assert_eq!(palette.surface0, Color::Reset);
        assert_eq!(palette.accent, Color::Blue);
        assert_eq!(palette.green, Color::Green);
        assert_eq!(palette.yellow, Color::Yellow);
        assert_eq!(palette.red, Color::LightRed);
        assert_eq!(palette.teal, Color::Cyan);
        assert_eq!(palette.mauve, Color::Magenta);
    }
}
