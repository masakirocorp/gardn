pub(crate) const GITHUB_COMMAND: &str = "ghui";

pub(crate) fn command() -> String {
    let config = config();
    format!(
        r#"if command -v ghui >/dev/null 2>&1; then
  override_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/gardn-ghui.XXXXXX")" || exit 1
  cleanup() {{
    rm -rf "$override_dir"
  }}
  trap cleanup EXIT INT TERM
  cat > "$override_dir/config.json" <<'GARDN_GHUI_CONFIG'
{config}GARDN_GHUI_CONFIG
  GHUI_CONFIG_DIR="$override_dir" ghui
  status=$?
  cleanup
  exit "$status"
fi
printf '%s\n' \
  'ghui is not installed.' \
  '' \
  'install with:' \
  '  brew install kitlangton/tap/ghui' \
  '' \
  'see https://github.com/kitlangton/ghui' \
  '' \
  'press enter to close...'
read -r _
"#
    )
}

fn config() -> &'static str {
    r#"{
  "themeMode": "fixed",
  "theme": "system",
  "systemThemeAutoReload": false,
  "showScrollbars": true
}
"#
}

#[cfg(test)]
mod tests {
    use super::{command, config};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generated_override_uses_terminal_theme_and_visible_scrollbars() {
        let output = config();

        assert!(output.contains(r#""theme": "system""#));
        assert!(output.contains(r#""showScrollbars": true"#));
        assert!(output.contains(r#""systemThemeAutoReload": false"#));
    }

    #[test]
    fn missing_ghui_guidance_names_homebrew_formula() {
        let output = command();

        assert!(output.contains("brew install kitlangton/tap/ghui"));
    }

    #[cfg(unix)]
    #[test]
    fn launch_passes_isolated_config_and_cleans_up_after_exit() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("gardn-ghui-theme-{}-{nonce}", std::process::id()));
        let bin_dir = root.join("bin");
        let tmp_dir = root.join("tmp");
        std::fs::create_dir_all(&bin_dir).expect("create fake ghui bin directory");
        std::fs::create_dir_all(&tmp_dir).expect("create temporary directory");
        let capture_path = root.join("config-dir");
        let ghui = bin_dir.join("ghui");
        std::fs::write(
            &ghui,
            r#"#!/bin/sh
printf '%s' "$GHUI_CONFIG_DIR" > "$CAPTURE_PATH"
cat "$GHUI_CONFIG_DIR/config.json"
exit 7
"#,
        )
        .expect("write fake ghui");
        let mut permissions = std::fs::metadata(&ghui)
            .expect("read fake ghui metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&ghui, permissions).expect("make fake ghui executable");

        let output = Command::new("sh")
            .arg("-c")
            .arg(command())
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("TMPDIR", &tmp_dir)
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run ghui wrapper");
        let generated_dir =
            std::path::PathBuf::from(std::fs::read_to_string(&capture_path).expect("config path"));
        assert_eq!(output.status.code(), Some(7));
        assert!(output.stderr.is_empty(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("ghui config is UTF-8");
        assert!(rendered.contains(r#""theme": "system""#));
        assert!(rendered.contains(r#""showScrollbars": true"#));
        assert!(
            !generated_dir.exists(),
            "wrapper should remove its override"
        );
        std::fs::remove_dir_all(root).expect("remove fake ghui directory");
    }
}
