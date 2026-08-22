use crate::app::state::Palette;
use crate::external_tool_theme::{background_fallback, foreground_fallback, palette_color, Rgb};
use crate::terminal_theme::{DefaultColorKind, TerminalTheme, ThemeAppearance};
use std::fmt;

pub(crate) const IDE_COMMAND: &str = "fresh .";

pub(crate) fn command(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
) -> String {
    let theme_setup = if passthrough_terminal {
        String::new()
    } else {
        format!(
            r#"fresh_themes_dir="$(
    fresh --cmd config paths 2>/dev/null | awk '
      /^[[:space:]]*themes\/:[[:space:]]*/ {{
        sub(/^[[:space:]]*themes\/:[[:space:]]*/, "")
        print
        exit
      }}
    '
  )"
  if [ -n "$fresh_themes_dir" ] && mkdir -p "$fresh_themes_dir"; then
    if theme_dir="$(mktemp -d "$fresh_themes_dir/.gardn-theme.XXXXXX")"; then
      if cat > "$theme_dir/theme.json" <<'GARDN_FRESH_THEME'
{theme}
GARDN_FRESH_THEME
      then
        theme_ref="file://$theme_dir/theme.json"
      else
        rm -rf "$theme_dir"
        theme_dir=""
      fi
    fi
  fi"#,
            theme = theme(palette, appearance, terminal_theme),
        )
    };
    format!(
        r#"if command -v fresh >/dev/null 2>&1; then
  config_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/gardn-fresh.XXXXXX")" || exec fresh .
  theme_dir=""
  cleanup() {{
    if [ -n "$theme_dir" ]; then
      rm -rf "$theme_dir"
    fi
    rm -rf "$config_dir"
  }}
  trap cleanup EXIT INT TERM
  fallback_theme_ref="builtin://terminal"
  theme_ref="$fallback_theme_ref"
  {theme_setup}
  if fresh --cmd config show > "$config_dir/config.json"; then
    awk -v theme_ref="$theme_ref" '
      /^[[:space:]]*"theme"[[:space:]]*:/ {{
        indent = $0
        sub(/[^[:space:]].*/, "", indent)
        comma = ($0 ~ /,[[:space:]]*$/) ? "," : ""
        printf "%s\"theme\": \"%s\"%s\n", indent, theme_ref, comma
        next
      }}
      {{ print }}
    ' "$config_dir/config.json" > "$config_dir/config.tmp" || {{
      cleanup
      exec fresh .
    }}
    mv "$config_dir/config.tmp" "$config_dir/config.json" || {{
      cleanup
      exec fresh .
    }}
  else
    cat > "$config_dir/config.json" <<GARDN_FRESH_CONFIG
{{
  "theme": "$fallback_theme_ref"
}}
GARDN_FRESH_CONFIG
  fi
  fresh --config "$config_dir/config.json" .
  status=$?
  cleanup
  exit "$status"
fi

printf '%s\n' \
  'Fresh is not installed.' \
  '' \
  'install with:' \
  '  brew install fresh-editor' \
  '  curl https://raw.githubusercontent.com/sinelaw/fresh/refs/heads/master/scripts/install.sh | sh' \
  '' \
  'see https://github.com/sinelaw/fresh' \
  '' \
  'press enter to close...'
read _
"#
    )
}

struct JsonColor(Rgb);

impl fmt::Display for JsonColor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}, {}, {}]", self.0.r, self.0.g, self.0.b)
    }
}

fn theme(palette: &Palette, appearance: ThemeAppearance, terminal_theme: TerminalTheme) -> String {
    let foreground_fallback = foreground_fallback(appearance);
    let background_fallback = background_fallback(appearance);
    let foreground = |color| {
        palette_color(
            color,
            terminal_theme,
            DefaultColorKind::Foreground,
            foreground_fallback,
        )
    };
    let background = |color| {
        palette_color(
            color,
            terminal_theme,
            DefaultColorKind::Background,
            background_fallback,
        )
    };
    let base = match appearance {
        ThemeAppearance::Light => "light",
        ThemeAppearance::Dark => "dark",
    };
    let canvas = background(palette.panel_bg);
    let panel = background(palette.surface0);
    let panel_alt = background(palette.surface1);
    let border = foreground(palette.overlay0);
    let accent = foreground(palette.accent);
    let accent_muted = foreground(palette.overlay1);
    let text = foreground(palette.text);
    let muted = foreground(palette.subtext0);
    let green = foreground(palette.green);
    let red = foreground(palette.red);
    let yellow = foreground(palette.yellow);
    let blue = foreground(palette.blue);
    let mauve = foreground(palette.mauve);
    let teal = foreground(palette.teal);
    let peach = foreground(palette.peach);

    format!(
        r#"{{
  "name": "Gardn",
  "extends": "builtin://{base}",
  "editor": {{
    "bg": {canvas},
    "fg": {text},
    "cursor": {accent},
    "inactive_cursor": {accent_muted},
    "selection_bg": {selection},
    "current_line_bg": {panel},
    "line_number_fg": {muted},
    "line_number_bg": {canvas},
    "diff_add_bg": {diff_add},
    "diff_remove_bg": {diff_remove},
    "diff_modify_bg": {diff_modify}
  }},
  "ui": {{
    "tab_active_fg": {text},
    "tab_active_bg": {panel_alt},
    "tab_inactive_fg": {muted},
    "tab_inactive_bg": {canvas},
    "tab_separator_bg": {canvas},
    "menu_bar_bg": {panel},
    "menu_bar_fg": {text},
    "status_bar_bg": {accent},
    "status_bar_fg": {canvas},
    "prompt_fg": {text},
    "prompt_bg": {panel},
    "prompt_selection_fg": {text},
    "prompt_selection_bg": {selection},
    "popup_border_fg": {border},
    "popup_bg": {panel},
    "popup_selection_bg": {selection},
    "popup_text_fg": {text},
    "suggestion_bg": {panel},
    "suggestion_selected_bg": {panel_alt},
    "help_bg": {panel},
    "help_fg": {text},
    "help_key_fg": {accent},
    "help_separator_fg": {border},
    "help_indicator_fg": {red},
    "help_indicator_bg": {panel},
    "split_separator_fg": {border}
  }},
  "search": {{
    "match_bg": {yellow},
    "match_fg": {canvas}
  }},
  "diagnostic": {{
    "error_fg": {red},
    "error_bg": {error_bg},
    "warning_fg": {yellow},
    "warning_bg": {warning_bg},
    "info_fg": {blue},
    "info_bg": {info_bg},
    "hint_fg": {muted},
    "hint_bg": {panel}
  }},
  "syntax": {{
    "keyword": {mauve},
    "string": {green},
    "comment": {accent_muted},
    "function": {blue},
    "type": {teal},
    "variable": {text},
    "constant": {peach},
    "operator": {muted}
  }}
}}"#,
        canvas = JsonColor(canvas),
        panel = JsonColor(panel),
        panel_alt = JsonColor(panel_alt),
        border = JsonColor(border),
        accent = JsonColor(accent),
        accent_muted = JsonColor(accent_muted),
        text = JsonColor(text),
        muted = JsonColor(muted),
        green = JsonColor(green),
        red = JsonColor(red),
        yellow = JsonColor(yellow),
        blue = JsonColor(blue),
        mauve = JsonColor(mauve),
        teal = JsonColor(teal),
        peach = JsonColor(peach),
        selection = JsonColor(canvas.blend(accent, 0.32)),
        diff_add = JsonColor(canvas.blend(green, 0.18)),
        diff_remove = JsonColor(canvas.blend(red, 0.18)),
        diff_modify = JsonColor(canvas.blend(yellow, 0.18)),
        error_bg = JsonColor(canvas.blend(red, 0.18)),
        warning_bg = JsonColor(canvas.blend(yellow, 0.18)),
        info_bg = JsonColor(canvas.blend(blue, 0.18)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    fn render_missing_screen(script: String) -> String {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(format!("PATH=''; export PATH\n{script}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn Fresh wrapper");
        child
            .stdin
            .take()
            .expect("wrapper stdin")
            .write_all(b"\n")
            .expect("close missing-tool prompt");
        let output = child.wait_with_output().expect("wait for Fresh wrapper");
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        String::from_utf8(output.stdout).expect("missing screen is UTF-8")
    }

    #[test]
    fn terminal_theme_uses_fresh_builtin_terminal_theme_and_cleans_up() {
        let command = command(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
        );

        assert!(command.contains("fresh --config \"$config_dir/config.json\" ."));
        assert!(command.contains("theme_ref=\"builtin://terminal\""));
        assert!(!command.contains("theme.json"));
        assert!(command.contains("rm -rf \"$config_dir\""));
    }
    #[cfg(unix)]
    #[test]
    fn themed_launch_preserves_effective_fresh_config() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let bin_dir =
            std::env::temp_dir().join(format!("gardn-fresh-theme-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&bin_dir).expect("create fake Fresh bin directory");
        let fresh = bin_dir.join("fresh");
        std::fs::write(
            &fresh,
            r#"#!/bin/sh
if [ "$1" = "--cmd" ] && [ "$2" = "config" ] && [ "$3" = "paths" ]; then
  printf '  themes/:      %s\n' "$FAKE_FRESH_THEMES_DIR"
  exit 0
fi
if [ "$1" = "--cmd" ] && [ "$2" = "config" ] && [ "$3" = "show" ]; then
  cat <<'JSON'
{
  "version": 2,
  "theme": "terminal",
  "editor": {
    "line_numbers": false
  }
}
JSON
  exit 0
fi
config="$2"
cat "$config"
theme_path="$(sed -n 's|.*"theme": "file://\([^"]*\)".*|\1|p' "$config")"
case "$theme_path" in
  "$FAKE_FRESH_THEMES_DIR"/*/theme.json) ;;
  *) exit 9 ;;
esac
cat "$theme_path"
"#,
        )
        .expect("write fake Fresh");
        let mut permissions = std::fs::metadata(&fresh)
            .expect("read fake Fresh metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fresh, permissions).expect("make fake Fresh executable");
        let themes_dir = bin_dir.join("themes");
        std::fs::create_dir_all(&themes_dir).expect("create fake Fresh themes directory");

        let output = Command::new("sh")
            .arg("-c")
            .arg(command(
                &Palette::tokyo_night(),
                ThemeAppearance::Dark,
                TerminalTheme::default(),
                false,
            ))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("FAKE_FRESH_THEMES_DIR", &themes_dir)
            .output()
            .expect("run Fresh wrapper");
        assert!(
            std::fs::read_dir(&themes_dir)
                .expect("read fake Fresh themes directory")
                .next()
                .is_none(),
            "wrapper should remove its registered theme directory"
        );
        std::fs::remove_dir_all(&bin_dir).expect("remove fake Fresh bin directory");

        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("Fresh config is UTF-8");
        assert!(rendered.contains("\"theme\": \"file://"));
        assert!(rendered.contains("\"line_numbers\": false"));
        assert!(rendered.contains("\"name\": \"Gardn\""));
        assert!(rendered.contains("\"extends\": \"builtin://dark\""));
        assert!(rendered.contains("\"bg\": [26, 27, 38]"));
        assert!(rendered.contains("\"cursor\": [122, 162, 247]"));
        assert!(rendered.contains("\"fg\": [169, 177, 214]"));
    }

    #[cfg(unix)]
    #[test]
    fn named_theme_falls_back_to_terminal_when_config_show_is_unavailable() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let bin_dir = std::env::temp_dir().join(format!(
            "gardn-fresh-theme-fallback-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&bin_dir).expect("create fake Fresh bin directory");
        let fresh = bin_dir.join("fresh");
        std::fs::write(
            &fresh,
            r#"#!/bin/sh
if [ "$1" = "--cmd" ]; then
  exit 2
fi
cat "$2"
"#,
        )
        .expect("write old fake Fresh");
        let mut permissions = std::fs::metadata(&fresh)
            .expect("read fake Fresh metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fresh, permissions).expect("make fake Fresh executable");

        let output = Command::new("sh")
            .arg("-c")
            .arg(command(
                &Palette::tokyo_night(),
                ThemeAppearance::Dark,
                TerminalTheme::default(),
                false,
            ))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .output()
            .expect("run Fresh fallback wrapper");
        std::fs::remove_dir_all(&bin_dir).expect("remove fake Fresh bin directory");

        assert!(output.status.success(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("Fresh config is UTF-8");
        assert!(rendered.contains("\"theme\": \"builtin://terminal\""));
    }

    #[test]
    fn named_theme_generates_a_fresh_theme_file() {
        let command = command(
            &Palette::tokyo_night_day(),
            ThemeAppearance::Light,
            TerminalTheme::default(),
            false,
        );

        assert!(command.contains("theme_ref=\"file://$theme_dir/theme.json\""));
        assert!(command.contains("fresh --cmd config paths"));
        assert!(command.contains("\"extends\": \"builtin://light\""));
        assert!(command.contains("\"name\": \"Gardn\""));
    }

    #[test]
    fn missing_fresh_screen_is_rendered_with_install_source() {
        let rendered = render_missing_screen(command(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
        ));

        assert_eq!(
            rendered,
            "Fresh is not installed.\n\ninstall with:\n  brew install fresh-editor\n  curl https://raw.githubusercontent.com/sinelaw/fresh/refs/heads/master/scripts/install.sh | sh\n\nsee https://github.com/sinelaw/fresh\n\npress enter to close...\n"
        );
    }
}
