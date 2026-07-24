use crate::app::state::Palette;
use crate::external_tool_theme::{background_fallback, foreground_fallback, palette_color};
use crate::terminal_theme::{DefaultColorKind, TerminalTheme, ThemeAppearance};

pub(crate) const DIFF_COMMAND: &str = "hunk diff --watch";

pub(crate) fn command(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
) -> String {
    let launch = if passthrough_terminal {
        "exec hunk diff --watch --theme auto".to_string()
    } else {
        let config = config(palette, appearance, terminal_theme);
        format!(
            r#"theme_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/omh-hunk.XXXXXX")" || exec hunk diff --watch --theme auto
config_lock=""
config_lock_candidate=""
cleanup() {{
  rm -f "$config_lock_candidate"
  if [ -n "$config_lock" ] &&
    [ "$(cat "$config_lock" 2>/dev/null)" = "$$" ]; then
    rm -f "$config_lock"
  fi
  rm -rf "$theme_dir"
}}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$theme_dir/hunk" || exec hunk diff --watch --theme auto

user_config_home="${{XDG_CONFIG_HOME:-}}"
if [ -z "$user_config_home" ] && [ -n "${{HOME:-}}" ]; then
  user_config_home="$HOME/.config"
fi
had_user_config=0
if [ -n "$user_config_home" ] && [ -f "$user_config_home/hunk/config.toml" ]; then
  had_user_config=1
  cp "$user_config_home/hunk/config.toml" "$theme_dir/original-config.toml" ||
    exec hunk diff --watch --theme auto
  awk -v theme_file="$theme_dir/original-custom-theme.toml" '
    /^[[:space:]]*[^[:alnum:]_]?custom_theme[^[:alnum:]_]?[[:space:]]*[.=]/ {{ print > theme_file; next }}
    /^[[:space:]]*\[[[:space:]]*[^[:alnum:]_]?custom_theme[^[:alnum:]_]?([.][^]]*)?][[:space:]]*(#.*)?$/ {{
      custom_theme = 1
      print > theme_file
      next
    }}
    /^[[:space:]]*\[/ {{ custom_theme = 0 }}
    custom_theme {{ print > theme_file; next }}
    {{ print }}
  ' "$user_config_home/hunk/config.toml" > "$theme_dir/hunk/config.toml" ||
    exec hunk diff --watch --theme auto
fi
cat >> "$theme_dir/hunk/config.toml" <<'OMH_HUNK_THEME'
{config}
OMH_HUNK_THEME

if [ -n "$user_config_home" ]; then
  mkdir -p "$user_config_home/hunk"
  ln -s "$user_config_home/hunk/state.json" "$theme_dir/hunk/state.json"
fi

XDG_CONFIG_HOME="$theme_dir" hunk diff --watch --theme custom
status=$?
if [ -n "$user_config_home" ] && [ -f "$theme_dir/hunk/config.toml" ]; then
  saved_preferences="$theme_dir/saved-config.toml"
  awk '
    /^[[:space:]]*[^[:alnum:]_]?custom_theme[^[:alnum:]_]?[[:space:]]*[.=]/ {{ next }}
    /^[[:space:]]*\[[[:space:]]*[^[:alnum:]_]?custom_theme[^[:alnum:]_]?([.][^]]*)?][[:space:]]*(#.*)?$/ {{ skip = 1; next }}
    /^[[:space:]]*\[/ {{ skip = 0 }}
    !skip {{ print }}
  ' "$theme_dir/hunk/config.toml" > "$saved_preferences"
  config_lock="$user_config_home/hunk/.omh-theme-config.lock"
  config_lock_candidate="$config_lock.$$"
  printf '%s\n' "$$" > "$config_lock_candidate"
  lock_acquired=0
  if ln "$config_lock_candidate" "$config_lock" 2>/dev/null; then
    lock_acquired=1
  else
    lock_owner="$(cat "$config_lock" 2>/dev/null)"
    stale_lock=0
    case "$lock_owner" in
      ""|*[!0-9]*) stale_lock=1 ;;
      *)
        if ! kill -0 "$lock_owner" 2>/dev/null; then
          stale_lock=1
        fi
        ;;
    esac
    if [ "$stale_lock" -eq 1 ]; then
      rm -f "$config_lock"
      if ln "$config_lock_candidate" "$config_lock" 2>/dev/null; then
        lock_acquired=1
      fi
    fi
  fi
  rm -f "$config_lock_candidate"
  config_lock_candidate=""
  if [ "$lock_acquired" -eq 1 ]; then
    can_restore=0
    if [ "$had_user_config" -eq 1 ]; then
      if [ -f "$user_config_home/hunk/config.toml" ] &&
        cmp -s "$theme_dir/original-config.toml" "$user_config_home/hunk/config.toml"; then
        can_restore=1
      fi
    elif [ ! -e "$user_config_home/hunk/config.toml" ]; then
      can_restore=1
    fi
    if [ "$can_restore" -eq 1 ] &&
      {{ [ "$had_user_config" -eq 1 ] || grep -q '[^[:space:]]' "$saved_preferences"; }}; then
      if [ -s "$theme_dir/original-custom-theme.toml" ]; then
        printf '\n' >> "$saved_preferences"
        cat "$theme_dir/original-custom-theme.toml" >> "$saved_preferences"
      fi
      config_path="$user_config_home/hunk/config.toml"
      config_destination="$config_path"
      if [ -L "$config_path" ]; then
        config_destination=""
        if command -v realpath >/dev/null 2>&1; then
          config_destination="$(realpath "$config_path" 2>/dev/null)"
        fi
      fi
      if [ -n "$config_destination" ]; then
        config_temp="$(dirname "$config_destination")/.omh-config.$$"
        if [ "$had_user_config" -eq 1 ]; then
          seeded="$(cp -p "$config_destination" "$config_temp" 2>/dev/null && printf yes)"
        else
          seeded=yes
        fi
        if [ "$seeded" = yes ] &&
          cat "$saved_preferences" > "$config_temp" &&
          mv "$config_temp" "$config_destination"; then
          :
        else
          rm -f "$config_temp"
        fi
      fi
    fi
    if [ "$(cat "$config_lock" 2>/dev/null)" = "$$" ]; then
      rm -f "$config_lock"
    fi
    config_lock=""
  fi
fi
exit "$status""#
        )
    };

    missing_tool_wrapper(&launch, HUNK_INSTALL_GUIDANCE)
}

const HUNK_INSTALL_GUIDANCE: &str = "  'hunk is not installed.' \\\n  '' \\\n  'install with:' \\\n  '  brew install hunk' \\\n  '  npm i -g hunkdiff' \\\n  '' \\\n  'see https://github.com/modem-dev/hunk' \\\n  '' \\\n  'press enter to close...'";

fn missing_tool_wrapper(launch: &str, guidance: &str) -> String {
    format!(
        r#"if command -v hunk >/dev/null 2>&1; then
  {launch}
fi

printf '%s\n' \
{guidance}
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
        ThemeAppearance::Light => "github-light-default",
        ThemeAppearance::Dark => "github-dark-default",
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

    format!(
        r#"
[custom_theme]
base = "{base}"
label = "Oh My Herdr"
background = "{background}"
panel = "{panel}"
panelAlt = "{panel_alt}"
border = "{border}"
accent = "{accent}"
accentMuted = "{accent_muted}"
text = "{text}"
muted = "{muted}"
addedBg = "{added_bg}"
removedBg = "{removed_bg}"
movedAddedBg = "{moved_added_bg}"
movedRemovedBg = "{moved_removed_bg}"
contextBg = "{panel}"
addedContentBg = "{added_content_bg}"
removedContentBg = "{removed_content_bg}"
contextContentBg = "{background}"
addedSignColor = "{green}"
removedSignColor = "{red}"
lineNumberBg = "{panel}"
lineNumberFg = "{muted}"
selectedHunk = "{panel_alt}"
badgeAdded = "{green}"
badgeRemoved = "{red}"
badgeNeutral = "{accent_muted}"
fileNew = "{green}"
fileDeleted = "{red}"
fileRenamed = "{blue}"
fileModified = "{yellow}"
fileUntracked = "{mauve}"
noteBorder = "{accent}"
noteBackground = "{panel}"
noteTitleBackground = "{panel_alt}"
noteTitleText = "{text}"

[custom_theme.syntax_scopes]
"comment" = "{muted}"
"constant" = "{yellow}"
"entity.name.function" = "{blue}"
"keyword" = "{mauve}"
"markup.changed" = "{yellow}"
"markup.deleted" = "{red}"
"markup.inserted" = "{green}"
"string" = "{green}"
"variable" = "{text}"
"#,
        background = canvas.hex(),
        panel = panel.hex(),
        panel_alt = panel_alt.hex(),
        border = border.hex(),
        accent = accent.hex(),
        accent_muted = accent_muted.hex(),
        text = text.hex(),
        muted = muted.hex(),
        green = green.hex(),
        red = red.hex(),
        yellow = yellow.hex(),
        blue = blue.hex(),
        mauve = mauve.hex(),
        added_bg = canvas.blend(green, 0.18).hex(),
        removed_bg = canvas.blend(red, 0.18).hex(),
        moved_added_bg = canvas.blend(green, 0.28).hex(),
        moved_removed_bg = canvas.blend(red, 0.28).hex(),
        added_content_bg = canvas.blend(green, 0.32).hex(),
        removed_content_bg = canvas.blend(red, 0.32).hex(),
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
            .expect("spawn hunk wrapper");
        child
            .stdin
            .take()
            .expect("wrapper stdin")
            .write_all(b"\n")
            .expect("close missing-tool prompt");
        let output = child.wait_with_output().expect("wait for hunk wrapper");
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        String::from_utf8(output.stdout).expect("missing screen is UTF-8")
    }

    #[test]
    fn named_theme_uses_palette_overlay_and_preserves_hunk_state_path() {
        let command = command(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
        );

        assert!(command.contains("XDG_CONFIG_HOME=\"$theme_dir\" hunk diff --watch --theme custom"));
        assert!(command.contains("user_config_home/hunk/config.toml"));
        assert!(command.contains("user_config_home/hunk/state.json"));
        assert!(command.contains("[custom_theme.syntax_scopes]"));
        assert!(command.contains("accent = \"#7aa2f7\""));
        assert!(command.contains("brew install hunk"));
        assert!(command.contains("https://github.com/modem-dev/hunk"));
    }

    #[cfg(unix)]
    #[test]
    fn named_theme_wrapper_preserves_user_options_and_persistent_state() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("omh-hunk-theme-{}-{nonce}", std::process::id()));
        let bin_dir = root.join("bin");
        let config_dir = root.join("home/.config/hunk");
        std::fs::create_dir_all(&bin_dir).expect("create fake Hunk bin directory");
        std::fs::create_dir_all(&config_dir).expect("create fake Hunk config directory");
        std::fs::write(
            config_dir.join("config.toml"),
            "line_numbers = false\n\n['custom_theme'] # my colors\naccent = \"#000000\"\n",
        )
        .expect("write existing Hunk config");
        let mut config_permissions = std::fs::metadata(config_dir.join("config.toml"))
            .expect("read Hunk config metadata")
            .permissions();
        config_permissions.set_mode(0o600);
        std::fs::set_permissions(config_dir.join("config.toml"), config_permissions)
            .expect("restrict Hunk config");
        let stale_lock = config_dir.join(".omh-theme-config.lock");
        std::fs::write(&stale_lock, "2147483647\n").expect("write stale Hunk lock owner");
        let hunk = bin_dir.join("hunk");
        std::fs::write(
            &hunk,
            r#"#!/bin/sh
cat "$XDG_CONFIG_HOME/hunk/config.toml"
config="$XDG_CONFIG_HOME/hunk/config.toml"
{ printf 'wrap_lines = true\n'; cat "$config"; } > "$config.saved"
mv "$config.saved" "$config"
printf 'saved-state' > "$XDG_CONFIG_HOME/hunk/state.json"
"#,
        )
        .expect("write fake Hunk");
        let mut permissions = std::fs::metadata(&hunk)
            .expect("read fake Hunk metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&hunk, permissions).expect("make fake Hunk executable");

        let wrapper = command(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
        );
        let output = Command::new("sh")
            .arg("-c")
            .arg(&wrapper)
            .env("HOME", root.join("home"))
            .env_remove("XDG_CONFIG_HOME")
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .output()
            .expect("run Hunk wrapper");

        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("Hunk config is UTF-8");
        assert!(rendered.contains("line_numbers = false"));
        assert!(rendered.contains("accent = \"#7aa2f7\""));
        assert!(!rendered.contains("accent = \"#000000\""));
        assert_eq!(rendered.matches("[custom_theme]\n").count(), 1);
        assert_eq!(
            std::fs::read_to_string(config_dir.join("state.json")).expect("read Hunk state"),
            "saved-state"
        );
        let saved_config =
            std::fs::read_to_string(config_dir.join("config.toml")).expect("read Hunk config");
        assert!(saved_config.contains("line_numbers = false"));
        assert!(saved_config.contains("wrap_lines = true"));
        assert!(saved_config.contains("accent = \"#000000\""));
        assert!(!saved_config.contains("accent = \"#7aa2f7\""));
        assert_eq!(
            std::fs::metadata(config_dir.join("config.toml"))
                .expect("read saved Hunk config metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(!stale_lock.exists());

        std::fs::write(
            config_dir.join("config.toml"),
            "line_numbers = false\ncustom_theme.accent = \"#000000\"\n",
        )
        .expect("reset Hunk config");
        std::fs::write(
            &hunk,
            r#"#!/bin/sh
config="$XDG_CONFIG_HOME/hunk/config.toml"
{ printf 'wrap_lines = true\n'; cat "$config"; } > "$config.saved"
cat "$config"
mv "$config.saved" "$config"
printf 'line_numbers = true\n' > "$HOME/.config/hunk/config.toml"
"#,
        )
        .expect("write concurrent-edit fake Hunk");
        let concurrent_output = Command::new("sh")
            .arg("-c")
            .arg(&wrapper)
            .env("HOME", root.join("home"))
            .env_remove("XDG_CONFIG_HOME")
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .output()
            .expect("run Hunk wrapper with concurrent edit");
        assert!(concurrent_output.status.success(), "{concurrent_output:?}");
        let concurrent_rendered =
            String::from_utf8(concurrent_output.stdout).expect("concurrent Hunk config is UTF-8");
        assert!(concurrent_rendered.contains("accent = \"#7aa2f7\""));
        assert!(!concurrent_rendered.contains("accent = \"#000000\""));
        assert_eq!(
            std::fs::read_to_string(config_dir.join("config.toml"))
                .expect("read concurrently edited Hunk config"),
            "line_numbers = true\n"
        );

        let linked_config = root.join("dotfiles/hunk.toml");
        std::fs::create_dir_all(linked_config.parent().expect("linked config parent"))
            .expect("create dotfiles directory");
        std::fs::write(
            &linked_config,
            "line_numbers = false\ncustom_theme.accent = \"#000000\"\n",
        )
        .expect("write linked Hunk config");
        let mut linked_permissions = std::fs::metadata(&linked_config)
            .expect("read linked config metadata")
            .permissions();
        linked_permissions.set_mode(0o600);
        std::fs::set_permissions(&linked_config, linked_permissions)
            .expect("restrict linked Hunk config");
        std::fs::remove_file(config_dir.join("config.toml")).expect("remove regular Hunk config");
        std::os::unix::fs::symlink(&linked_config, config_dir.join("config.toml"))
            .expect("link Hunk config");
        std::fs::write(
            &hunk,
            r#"#!/bin/sh
config="$XDG_CONFIG_HOME/hunk/config.toml"
{ printf 'wrap_lines = true\n'; cat "$config"; } > "$config.saved"
mv "$config.saved" "$config"
"#,
        )
        .expect("write symlink fake Hunk");
        let symlink_output = Command::new("sh")
            .arg("-c")
            .arg(&wrapper)
            .env("HOME", root.join("home"))
            .env_remove("XDG_CONFIG_HOME")
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .output()
            .expect("run Hunk wrapper with linked config");
        assert!(symlink_output.status.success(), "{symlink_output:?}");
        assert!(std::fs::symlink_metadata(config_dir.join("config.toml"))
            .expect("read linked Hunk config metadata")
            .file_type()
            .is_symlink());
        let linked_saved =
            std::fs::read_to_string(&linked_config).expect("read saved linked Hunk config");
        assert!(linked_saved.contains("wrap_lines = true"));
        assert!(linked_saved.contains("accent = \"#000000\""));
        assert!(!linked_saved.contains("accent = \"#7aa2f7\""));
        assert_eq!(
            std::fs::metadata(&linked_config)
                .expect("read saved linked config metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        std::fs::remove_dir_all(&root).expect("remove fake Hunk environment");
    }

    #[test]
    fn terminal_theme_uses_hunks_native_auto_theme() {
        let command = command(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
        );

        assert!(command.contains("exec hunk diff --watch --theme auto"));
        assert!(!command.contains("XDG_CONFIG_HOME"));
    }

    #[test]
    fn generated_theme_uses_light_and_dark_hunk_bases() {
        assert!(config(
            &Palette::tokyo_night(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
        )
        .contains("base = \"github-dark-default\""));
        assert!(config(
            &Palette::tokyo_night_day(),
            ThemeAppearance::Light,
            TerminalTheme::default(),
        )
        .contains("base = \"github-light-default\""));
    }

    #[test]
    fn missing_hunk_screen_is_rendered_with_install_source() {
        let rendered = render_missing_screen(command(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
        ));

        assert_eq!(
            rendered,
            "hunk is not installed.\n\ninstall with:\n  brew install hunk\n  npm i -g hunkdiff\n\nsee https://github.com/modem-dev/hunk\n\npress enter to close...\n"
        );
    }
}
