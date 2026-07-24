use crate::app::state::Palette;
use crate::external_tool_theme::{background_fallback, foreground_fallback, palette_color};
use crate::terminal_theme::{DefaultColorKind, TerminalTheme, ThemeAppearance};

pub(crate) const DIFF_COMMAND: &str = "lazygit";

pub(crate) fn command(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
) -> String {
    let launch = if passthrough_terminal {
        "exec lazygit".to_string()
    } else {
        let config = config(palette, appearance, terminal_theme);
        format!(
            r#"theme_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/omh-lazygit.XXXXXX")" || exec lazygit
cleanup() {{
  rm -rf "$theme_dir"
}}
trap cleanup EXIT INT TERM
cat > "$theme_dir/theme.yml" <<'OMH_LAZYGIT_THEME'
{config}
OMH_LAZYGIT_THEME

if [ -n "${{LG_CONFIG_FILE:-}}" ]; then
  config_files="$LG_CONFIG_FILE,$theme_dir/theme.yml"
else
  user_config_dir="$(lazygit --print-config-dir 2>/dev/null)"
  if [ -n "$user_config_dir" ] && [ -f "$user_config_dir/config.yml" ]; then
    config_files="$user_config_dir/config.yml,$theme_dir/theme.yml"
  else
    config_files="$theme_dir/theme.yml"
  fi
fi

LG_CONFIG_FILE="$config_files" lazygit
status=$?
cleanup
exit "$status""#
        )
    };

    format!(
        r#"if command -v lazygit >/dev/null 2>&1; then
  {launch}
fi

printf '%s\n' \
  'lazygit is not installed.' \
  '' \
  'install with:' \
  '  brew install lazygit' \
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
) -> String {
    let foreground_fallback = foreground_fallback(appearance);
    let background_fallback = background_fallback(appearance);
    let foreground = |color| {
        palette_color(
            color,
            terminal_theme,
            DefaultColorKind::Foreground,
            foreground_fallback,
        )
        .hex()
    };
    let background = |color| {
        palette_color(
            color,
            terminal_theme,
            DefaultColorKind::Background,
            background_fallback,
        )
        .hex()
    };
    let accent = foreground(palette.accent);
    let border = foreground(palette.overlay0);
    let searching = foreground(palette.yellow);
    let selected = background(palette.surface1);
    let inactive_selected = background(palette.surface0);
    let green = foreground(palette.green);
    let red = foreground(palette.red);
    let text = foreground(palette.text);

    format!(
        r#"gui:
  theme:
    activeBorderColor:
      - "{accent}"
      - bold
    inactiveBorderColor:
      - "{border}"
    searchingActiveBorderColor:
      - "{searching}"
      - bold
    optionsTextColor:
      - "{accent}"
    selectedLineBgColor:
      - "{selected}"
    inactiveViewSelectedLineBgColor:
      - "{inactive_selected}"
    cherryPickedCommitFgColor:
      - "{accent}"
    cherryPickedCommitBgColor:
      - "{green}"
    markedBaseCommitFgColor:
      - "{accent}"
    markedBaseCommitBgColor:
      - "{searching}"
    unstagedChangesColor:
      - "{red}"
    defaultFgColor:
      - "{text}"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_theme_launch_merges_user_config_before_oh_my_herdr_overlay() {
        let command = command(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
        );

        assert!(command.contains("LG_CONFIG_FILE=\"$config_files\" lazygit"));
        assert!(command.contains("config_files=\"$LG_CONFIG_FILE,$theme_dir/theme.yml\""));
        assert!(
            command.contains("config_files=\"$user_config_dir/config.yml,$theme_dir/theme.yml\"")
        );
        assert!(command.contains("activeBorderColor:\n      - \"#7aa2f7\""));
        assert!(command.contains("defaultFgColor:\n      - \"#a9b1d6\""));
        assert!(command.contains("brew install lazygit"));
    }

    #[test]
    fn terminal_theme_launch_keeps_lazygit_terminal_palette() {
        let command = command(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
        );

        assert!(command.contains("exec lazygit"));
        assert!(!command.contains("LG_CONFIG_FILE"));
        assert!(!command.contains("theme.yml"));
    }
}
