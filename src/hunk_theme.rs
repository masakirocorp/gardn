use crate::app::state::Palette;
use crate::terminal_theme::{DefaultColorKind, RgbColor, TerminalTheme, ThemeAppearance};
use ratatui::style::Color;

pub(crate) fn command(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
) -> String {
    let config = config(palette, appearance, terminal_theme, passthrough_terminal);
    format!(
        r#"if command -v hunk >/dev/null 2>&1; then
  hunk_version="$(hunk --version 2>/dev/null)"
  case "$hunk_version" in
    *" 0."[0-9].*|*" 0.1"[0-3].*|"0."[0-9].*|"0.1"[0-3].*) exec hunk diff --watch --theme graphite ;;
  esac

  config_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/hako-hunk.XXXXXX")" || exec hunk diff --watch --theme graphite
  cleanup() {{
    rm -rf "$config_dir"
  }}
  trap cleanup EXIT INT TERM
  mkdir -p "$config_dir/hunk" || exec hunk diff --watch --theme graphite
  cat > "$config_dir/hunk/config.toml" <<'HAKO_HUNK_CONFIG'
{config}
HAKO_HUNK_CONFIG
  XDG_CONFIG_HOME="$config_dir" hunk diff --watch
  status=$?
  cleanup
  exit "$status"
fi

printf '%s\n' \
  'hunk is not installed.' \
  '' \
  'install with:' \
  '  brew install modem-dev/tap/hunk' \
  '  npm i -g hunkdiff' \
  '' \
  'press enter to close...'
read _
"#
    )
}

pub(crate) fn config(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
) -> String {
    let colors =
        HunkThemeColors::from_palette(palette, appearance, terminal_theme, passthrough_terminal);
    let transparent_background = if passthrough_terminal {
        "transparent_background = true\n"
    } else {
        ""
    };
    format!(
        r#"theme = "custom"
{transparent_background}
[custom_theme]
base = "{}"
label = "Hako"
background = "{}"
panel = "{}"
panelAlt = "{}"
border = "{}"
accent = "{}"
accentMuted = "{}"
text = "{}"
muted = "{}"
addedBg = "{}"
removedBg = "{}"
contextBg = "{}"
addedContentBg = "{}"
removedContentBg = "{}"
contextContentBg = "{}"
addedSignColor = "{}"
removedSignColor = "{}"
lineNumberBg = "{}"
lineNumberFg = "{}"
selectedHunk = "{}"
badgeAdded = "{}"
badgeRemoved = "{}"
badgeNeutral = "{}"
fileNew = "{}"
fileDeleted = "{}"
fileRenamed = "{}"
fileModified = "{}"
fileUntracked = "{}"
noteBorder = "{}"
noteBackground = "{}"
noteTitleBackground = "{}"
noteTitleText = "{}"

[custom_theme.syntax]
default = "{}"
keyword = "{}"
string = "{}"
comment = "{}"
number = "{}"
function = "{}"
property = "{}"
type = "{}"
punctuation = "{}"
"#,
        colors.base,
        colors.background,
        colors.panel,
        colors.panel_alt,
        colors.border,
        colors.accent,
        colors.accent_muted,
        colors.text,
        colors.muted,
        colors.added_bg,
        colors.removed_bg,
        colors.context_bg,
        colors.added_content_bg,
        colors.removed_content_bg,
        colors.context_content_bg,
        colors.added_sign_color,
        colors.removed_sign_color,
        colors.line_number_bg,
        colors.line_number_fg,
        colors.selected_hunk,
        colors.badge_added,
        colors.badge_removed,
        colors.badge_neutral,
        colors.file_new,
        colors.file_deleted,
        colors.file_renamed,
        colors.file_modified,
        colors.file_untracked,
        colors.note_border,
        colors.note_background,
        colors.note_title_background,
        colors.note_title_text,
        colors.syntax_default,
        colors.syntax_keyword,
        colors.syntax_string,
        colors.syntax_comment,
        colors.syntax_number,
        colors.syntax_function,
        colors.syntax_property,
        colors.syntax_type,
        colors.syntax_punctuation,
    )
}

struct HunkThemeColors {
    base: &'static str,
    background: String,
    panel: String,
    panel_alt: String,
    border: String,
    accent: String,
    accent_muted: String,
    text: String,
    muted: String,
    added_bg: String,
    removed_bg: String,
    context_bg: String,
    added_content_bg: String,
    removed_content_bg: String,
    context_content_bg: String,
    added_sign_color: String,
    removed_sign_color: String,
    line_number_bg: String,
    line_number_fg: String,
    selected_hunk: String,
    badge_added: String,
    badge_removed: String,
    badge_neutral: String,
    file_new: String,
    file_deleted: String,
    file_renamed: String,
    file_modified: String,
    file_untracked: String,
    note_border: String,
    note_background: String,
    note_title_background: String,
    note_title_text: String,
    syntax_default: String,
    syntax_keyword: String,
    syntax_string: String,
    syntax_comment: String,
    syntax_number: String,
    syntax_function: String,
    syntax_property: String,
    syntax_type: String,
    syntax_punctuation: String,
}

impl HunkThemeColors {
    fn from_palette(
        palette: &Palette,
        appearance: ThemeAppearance,
        terminal_theme: TerminalTheme,
        passthrough_terminal: bool,
    ) -> Self {
        let fallback_background = background_fallback(appearance);
        let fallback_foreground = foreground_fallback(appearance);
        let background = palette_color(
            palette.panel_bg,
            terminal_theme,
            DefaultColorKind::Background,
            fallback_background,
        );
        let text = palette_color(
            palette.text,
            terminal_theme,
            DefaultColorKind::Foreground,
            fallback_foreground,
        );
        let default_panel = palette_color(
            palette.surface_dim,
            terminal_theme,
            DefaultColorKind::Background,
            background,
        );
        let default_panel_alt = palette_color(
            palette.surface0,
            terminal_theme,
            DefaultColorKind::Background,
            default_panel,
        );
        let default_context_bg = palette_color(
            palette.surface_dim,
            terminal_theme,
            DefaultColorKind::Background,
            default_panel,
        );
        let default_context_content_bg = palette_color(
            palette.panel_bg,
            terminal_theme,
            DefaultColorKind::Background,
            background,
        );
        let (panel, panel_alt, context_bg, context_content_bg, diff_bg_amount, diff_content_amount) =
            if passthrough_terminal {
                (background, background, background, background, 0.10, 0.16)
            } else {
                (
                    default_panel,
                    default_panel_alt,
                    default_context_bg,
                    default_context_content_bg,
                    0.16,
                    0.24,
                )
            };
        let default_muted = palette_color(
            palette.subtext0,
            terminal_theme,
            DefaultColorKind::Foreground,
            text,
        );
        let default_overlay = palette_color(
            palette.overlay0,
            terminal_theme,
            DefaultColorKind::Foreground,
            default_muted,
        );
        let (muted, overlay) = if passthrough_terminal {
            // ANSI gray is often tuned for terminal text, not for Hunk's solid
            // panel fills. Derive neutral UI text from the actual terminal
            // foreground/background so light terminal themes do not become
            // gray-on-white.
            (blend(text, background, 0.35), blend(text, background, 0.45))
        } else {
            (default_muted, default_overlay)
        };
        let border = if passthrough_terminal {
            overlay
        } else {
            palette_color(
                palette.surface1,
                terminal_theme,
                DefaultColorKind::Background,
                overlay,
            )
        };
        let accent = palette_color(
            palette.accent,
            terminal_theme,
            DefaultColorKind::Foreground,
            palette_color(
                Color::Blue,
                terminal_theme,
                DefaultColorKind::Foreground,
                fallback_foreground,
            ),
        );
        let green = palette_color(
            palette.green,
            terminal_theme,
            DefaultColorKind::Foreground,
            accent,
        );
        let red = palette_color(
            palette.red,
            terminal_theme,
            DefaultColorKind::Foreground,
            accent,
        );
        let yellow = palette_color(
            palette.yellow,
            terminal_theme,
            DefaultColorKind::Foreground,
            accent,
        );
        let teal = palette_color(
            palette.teal,
            terminal_theme,
            DefaultColorKind::Foreground,
            accent,
        );
        let peach = palette_color(
            palette.peach,
            terminal_theme,
            DefaultColorKind::Foreground,
            yellow,
        );

        Self {
            // Hunk custom themes must inherit from one built-in theme. Use the
            // least-branded light/dark bases so any future Hunk role we do not
            // override stays neutral instead of leaking another product palette.
            base: match appearance {
                ThemeAppearance::Light => "paper",
                ThemeAppearance::Dark => "graphite",
            },
            background: background.hex(),
            panel: panel.hex(),
            panel_alt: panel_alt.hex(),
            border: border.hex(),
            accent: accent.hex(),
            accent_muted: accent.hex(),
            text: text.hex(),
            muted: muted.hex(),
            added_bg: blend(background, green, diff_bg_amount).hex(),
            removed_bg: blend(background, red, diff_bg_amount).hex(),
            context_bg: context_bg.hex(),
            added_content_bg: blend(context_content_bg, green, diff_content_amount).hex(),
            removed_content_bg: blend(context_content_bg, red, diff_content_amount).hex(),
            context_content_bg: context_content_bg.hex(),
            added_sign_color: green.hex(),
            removed_sign_color: red.hex(),
            line_number_bg: context_bg.hex(),
            line_number_fg: overlay.hex(),
            selected_hunk: blend(background, accent, 0.24).hex(),
            badge_added: green.hex(),
            badge_removed: red.hex(),
            badge_neutral: overlay.hex(),
            file_new: green.hex(),
            file_deleted: red.hex(),
            file_renamed: yellow.hex(),
            file_modified: accent.hex(),
            file_untracked: peach.hex(),
            note_border: accent.hex(),
            note_background: blend(background, accent, 0.12).hex(),
            note_title_background: blend(background, accent, 0.22).hex(),
            note_title_text: text.hex(),
            syntax_default: text.hex(),
            syntax_keyword: accent.hex(),
            syntax_string: green.hex(),
            syntax_comment: overlay.hex(),
            syntax_number: peach.hex(),
            syntax_function: accent.hex(),
            syntax_property: teal.hex(),
            syntax_type: yellow.hex(),
            syntax_punctuation: muted.hex(),
        }
    }
}

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

fn foreground_fallback(appearance: ThemeAppearance) -> Rgb {
    match appearance {
        ThemeAppearance::Light => Rgb::new(31, 35, 40),
        ThemeAppearance::Dark => Rgb::new(230, 237, 243),
    }
}

fn background_fallback(appearance: ThemeAppearance) -> Rgb {
    match appearance {
        ThemeAppearance::Light => Rgb::new(255, 255, 255),
        ThemeAppearance::Dark => Rgb::new(13, 17, 23),
    }
}

fn rgb_color(color: RgbColor) -> Rgb {
    Rgb::new(color.r, color.g, color.b)
}

fn palette_color(
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

fn terminal_palette_color(theme: TerminalTheme, index: usize, fallback: Rgb) -> Rgb {
    theme
        .palette
        .get(index)
        .copied()
        .flatten()
        .map(rgb_color)
        .unwrap_or(fallback)
}

fn blend(base: Rgb, overlay: Rgb, amount: f32) -> Rgb {
    let mix = |base: u8, overlay: u8| {
        let base = base as f32;
        let overlay = overlay as f32;
        (base + (overlay - base) * amount).round().clamp(0.0, 255.0) as u8
    };
    Rgb::new(
        mix(base.r, overlay.r),
        mix(base.g, overlay.g),
        mix(base.b, overlay.b),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunk_command_uses_hako_theme_for_hunk_014_and_falls_back_for_old_hunk() {
        let command = command(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
        );

        assert!(command.contains("theme = \"custom\""));
        assert!(command.contains("label = \"Hako\""));
        assert!(command.contains("base = \"graphite\""));
        assert!(command.contains("accent = \"#7aa2f7\""));
        assert!(command.contains("text = \"#a9b1d6\""));
        assert!(command.contains("XDG_CONFIG_HOME=\"$config_dir\" hunk diff --watch"));
        assert!(command.contains("exec hunk diff --watch --theme graphite"));
        assert!(command.contains("brew install modem-dev/tap/hunk"));
        assert!(command.contains("npm i -g hunkdiff"));
    }

    #[test]
    fn hunk_config_uses_light_base_for_light_appearance() {
        let config = config(
            &Palette::tokyo_night_day(),
            ThemeAppearance::Light,
            TerminalTheme::default(),
            false,
        );

        assert!(config.contains("base = \"paper\""));
        assert!(config.contains("accent = \"#34548a\""));
        assert!(config.contains("text = \"#343b58\""));
    }

    #[test]
    fn terminal_passthrough_enables_hunk_transparent_background() {
        let config = config(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
        );

        assert!(config.contains("transparent_background = true"));
    }

    #[test]
    fn branded_theme_keeps_opaque_hunk_background() {
        let config = config(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
        );

        assert!(!config.contains("transparent_background = true"));
    }

    #[test]
    fn terminal_theme_uses_host_defaults_and_palette_colors() {
        let host = TerminalTheme::default()
            .with_color(
                DefaultColorKind::Foreground,
                RgbColor {
                    r: 17,
                    g: 34,
                    b: 51,
                },
            )
            .with_color(
                DefaultColorKind::Background,
                RgbColor {
                    r: 250,
                    g: 251,
                    b: 252,
                },
            )
            .with_palette_color(
                4,
                RgbColor {
                    r: 10,
                    g: 20,
                    b: 30,
                },
            )
            .with_palette_color(
                7,
                RgbColor {
                    r: 100,
                    g: 110,
                    b: 120,
                },
            )
            .with_palette_color(
                8,
                RgbColor {
                    r: 80,
                    g: 90,
                    b: 100,
                },
            );
        let config = config(&Palette::terminal(), ThemeAppearance::Light, host, true);

        assert!(config.contains("background = \"#fafbfc\""));
        assert!(config.contains("text = \"#112233\""));
        assert!(config.contains("accent = \"#0a141e\""));
        assert!(config.contains("panel = \"#fafbfc\""));
        assert!(config.contains("muted = \"#636e79\""));
        assert!(config.contains("lineNumberBg = \"#fafbfc\""));
        assert!(config.contains("lineNumberFg = \"#7a848d\""));
        assert!(!config.contains("#eff1f5"));
        assert!(!config.contains("#181825"));
    }

    #[test]
    fn terminal_passthrough_avoids_ansi_gray_surfaces() {
        let host = TerminalTheme::default()
            .with_color(
                DefaultColorKind::Foreground,
                RgbColor {
                    r: 20,
                    g: 30,
                    b: 40,
                },
            )
            .with_color(
                DefaultColorKind::Background,
                RgbColor {
                    r: 245,
                    g: 246,
                    b: 247,
                },
            )
            .with_palette_color(
                8,
                RgbColor {
                    r: 80,
                    g: 90,
                    b: 100,
                },
            );
        let config = config(&Palette::terminal(), ThemeAppearance::Light, host, true);

        assert!(config.contains("panel = \"#f5f6f7\""));
        assert!(config.contains("panelAlt = \"#f5f6f7\""));
        assert!(config.contains("contextBg = \"#f5f6f7\""));
        assert!(config.contains("lineNumberBg = \"#f5f6f7\""));
        assert!(config.contains("muted = \"#636a70\""));
        assert!(config.contains("lineNumberFg = \"#797f85\""));
        assert!(!config.contains("panel = \"#505a64\""));
    }

    #[test]
    fn system_terminal_source_uses_host_palette_for_hunk_semantics() {
        let host = TerminalTheme::default()
            .with_color(
                DefaultColorKind::Foreground,
                RgbColor {
                    r: 20,
                    g: 30,
                    b: 40,
                },
            )
            .with_color(
                DefaultColorKind::Background,
                RgbColor {
                    r: 245,
                    g: 246,
                    b: 247,
                },
            )
            .with_palette_color(
                2,
                RgbColor {
                    r: 10,
                    g: 160,
                    b: 20,
                },
            )
            .with_palette_color(
                4,
                RgbColor {
                    r: 30,
                    g: 90,
                    b: 220,
                },
            );
        let palette = Palette::system(
            host,
            ThemeAppearance::Light,
            crate::config::TerminalAccent::Blue,
        );
        let config = config(&palette, ThemeAppearance::Light, host, true);

        assert!(config.contains("background = \"#f5f6f7\""));
        assert!(config.contains("panel = \"#f5f6f7\""));
        assert!(config.contains("accent = \"#1e5adc\""));
        assert!(config.contains("accentMuted = \"#1e5adc\""));
        assert!(config.contains("fileModified = \"#1e5adc\""));
        assert!(config.contains("badgeAdded = \"#0aa014\""));
        assert!(config.contains("fileNew = \"#0aa014\""));
        assert!(config.contains("addedSignColor = \"#0aa014\""));
    }

    #[test]
    fn hako_accent_drives_prominent_hunk_accent_fields() {
        let config = config(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
        );

        assert!(config.contains("accent = \"#7aa2f7\""));
        assert!(config.contains("accentMuted = \"#7aa2f7\""));
        assert!(config.contains("fileModified = \"#7aa2f7\""));
        assert!(config.contains("noteBorder = \"#7aa2f7\""));
        assert!(config.contains("keyword = \"#7aa2f7\""));
        assert!(config.contains("function = \"#7aa2f7\""));
    }
}
