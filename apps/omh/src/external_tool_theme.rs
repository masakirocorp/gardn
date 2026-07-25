use crate::app::state::Palette;
use crate::terminal_theme::{
    AnsiPalette, DefaultColorKind, ResolvedTerminalTheme, RgbColor, TerminalTheme, ThemeAppearance,
};
use ratatui::style::Color;

#[derive(Clone, Copy)]
pub(crate) struct Rgb {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
}

impl Rgb {
    pub(crate) const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub(crate) fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub(crate) fn blend(self, overlay: Self, amount: f32) -> Self {
        let mix = |base: u8, overlay: u8| {
            let base = base as f32;
            let overlay = overlay as f32;
            (base + (overlay - base) * amount).round().clamp(0.0, 255.0) as u8
        };
        Self::new(
            mix(self.r, overlay.r),
            mix(self.g, overlay.g),
            mix(self.b, overlay.b),
        )
    }
}

pub(crate) fn foreground_fallback(appearance: ThemeAppearance) -> Rgb {
    match appearance {
        ThemeAppearance::Light => Rgb::new(31, 35, 40),
        ThemeAppearance::Dark => Rgb::new(230, 237, 243),
    }
}

pub(crate) fn background_fallback(appearance: ThemeAppearance) -> Rgb {
    match appearance {
        ThemeAppearance::Light => Rgb::new(255, 255, 255),
        ThemeAppearance::Dark => Rgb::new(13, 17, 23),
    }
}

pub(crate) fn is_terminal_passthrough(theme_name: &str) -> bool {
    matches!(
        theme_name.to_lowercase().replace([' ', '_'], "-").as_str(),
        "system" | "terminal"
    )
}
pub(crate) fn resolved_terminal_theme(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
) -> ResolvedTerminalTheme {
    let foreground = terminal_color(palette.text, terminal_theme, appearance);
    let background = palette_color(
        palette.panel_bg,
        terminal_theme,
        DefaultColorKind::Background,
        background_fallback(appearance),
    );
    let cursor = terminal_color(palette.accent, terminal_theme, appearance);
    ResolvedTerminalTheme {
        foreground,
        background: RgbColor {
            r: background.r,
            g: background.g,
            b: background.b,
        },
        cursor,
        palette: ansi_palette(palette, appearance, terminal_theme),
    }
}

pub(crate) fn ansi_palette(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
) -> AnsiPalette {
    let (black, white, bright_black, bright_white) = match appearance {
        ThemeAppearance::Dark => (
            palette.surface_dim,
            palette.subtext0,
            palette.overlay0,
            palette.text,
        ),
        ThemeAppearance::Light => (
            palette.text,
            palette.overlay0,
            palette.subtext0,
            palette.surface0,
        ),
    };
    [
        terminal_color(black, terminal_theme, appearance),
        terminal_color(palette.red, terminal_theme, appearance),
        terminal_color(palette.green, terminal_theme, appearance),
        terminal_color(palette.yellow, terminal_theme, appearance),
        terminal_color(palette.blue, terminal_theme, appearance),
        terminal_color(palette.mauve, terminal_theme, appearance),
        terminal_color(palette.teal, terminal_theme, appearance),
        terminal_color(white, terminal_theme, appearance),
        terminal_color(bright_black, terminal_theme, appearance),
        terminal_color(palette.red, terminal_theme, appearance),
        terminal_color(palette.green, terminal_theme, appearance),
        terminal_color(palette.yellow, terminal_theme, appearance),
        terminal_color(palette.blue, terminal_theme, appearance),
        terminal_color(palette.mauve, terminal_theme, appearance),
        terminal_color(palette.teal, terminal_theme, appearance),
        terminal_color(bright_white, terminal_theme, appearance),
    ]
}

fn terminal_color(
    color: Color,
    terminal_theme: TerminalTheme,
    appearance: ThemeAppearance,
) -> RgbColor {
    let fallback = foreground_fallback(appearance);
    let color = palette_color(
        color,
        terminal_theme,
        DefaultColorKind::Foreground,
        fallback,
    );
    RgbColor {
        r: color.r,
        g: color.g,
        b: color.b,
    }
}

pub(crate) fn palette_color(
    color: Color,
    terminal_theme: TerminalTheme,
    reset_kind: DefaultColorKind,
    fallback: Rgb,
) -> Rgb {
    match color {
        Color::Reset => match reset_kind {
            DefaultColorKind::Foreground => {
                terminal_theme.foreground.map(rgb_color).unwrap_or(fallback)
            }
            DefaultColorKind::Background => {
                terminal_theme.background.map(rgb_color).unwrap_or(fallback)
            }
        },
        Color::Black => terminal_palette_color(terminal_theme, 0, Rgb::new(0, 0, 0)),
        Color::Red => terminal_palette_color(terminal_theme, 1, Rgb::new(205, 49, 49)),
        Color::Green => terminal_palette_color(terminal_theme, 2, Rgb::new(13, 188, 121)),
        Color::Yellow => terminal_palette_color(terminal_theme, 3, Rgb::new(229, 229, 16)),
        Color::Blue => terminal_palette_color(terminal_theme, 4, Rgb::new(36, 114, 200)),
        Color::Magenta => terminal_palette_color(terminal_theme, 5, Rgb::new(188, 63, 188)),
        Color::Cyan => terminal_palette_color(terminal_theme, 6, Rgb::new(17, 168, 205)),
        Color::Gray => terminal_palette_color(terminal_theme, 7, Rgb::new(229, 229, 229)),
        Color::DarkGray => terminal_palette_color(terminal_theme, 8, Rgb::new(102, 102, 102)),
        Color::LightRed => terminal_palette_color(terminal_theme, 9, Rgb::new(241, 76, 76)),
        Color::LightGreen => terminal_palette_color(terminal_theme, 10, Rgb::new(35, 209, 139)),
        Color::LightYellow => terminal_palette_color(terminal_theme, 11, Rgb::new(245, 245, 67)),
        Color::LightBlue => terminal_palette_color(terminal_theme, 12, Rgb::new(59, 142, 234)),
        Color::LightMagenta => terminal_palette_color(terminal_theme, 13, Rgb::new(214, 112, 214)),
        Color::LightCyan => terminal_palette_color(terminal_theme, 14, Rgb::new(41, 184, 219)),
        Color::White => terminal_palette_color(terminal_theme, 15, Rgb::new(255, 255, 255)),
        Color::Rgb(r, g, b) => Rgb::new(r, g, b),
        Color::Indexed(index) => terminal_palette_color(terminal_theme, index as usize, fallback),
    }
}

fn rgb_color(color: RgbColor) -> Rgb {
    Rgb::new(color.r, color.g, color.b)
}

fn terminal_palette_color(theme: TerminalTheme, index: usize, fallback: Rgb) -> Rgb {
    theme
        .palette
        .get(index)
        .copied()
        .flatten()
        .map(rgb_color)
        .unwrap_or(fallback)
}
