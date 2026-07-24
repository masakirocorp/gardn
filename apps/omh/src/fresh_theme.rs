pub(crate) const IDE_COMMAND: &str = "fresh .";

pub(crate) fn command() -> String {
    let theme = "terminal";
    format!(
        r#"if command -v fresh >/dev/null 2>&1; then
  config_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/omh-fresh.XXXXXX")" || exec fresh .
  cleanup() {{
    rm -rf "$config_dir"
  }}
  trap cleanup EXIT INT TERM
  if fresh --cmd config show > "$config_dir/config.json"; then
    sed 's|^  "theme":.*|  "theme": "builtin://{theme}",|' \
      "$config_dir/config.json" > "$config_dir/config.tmp" || exec fresh .
    mv "$config_dir/config.tmp" "$config_dir/config.json" || exec fresh .
  else
    cat > "$config_dir/config.json" <<'OMH_FRESH_CONFIG'
{{
  "theme": "builtin://{theme}"
}}
OMH_FRESH_CONFIG
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
        let command = command();

        assert!(command.contains("fresh --config \"$config_dir/config.json\" ."));
        assert!(command.contains("\"theme\": \"builtin://terminal\""));
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
            std::env::temp_dir().join(format!("omh-fresh-theme-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&bin_dir).expect("create fake Fresh bin directory");
        let fresh = bin_dir.join("fresh");
        std::fs::write(
            &fresh,
            r#"#!/bin/sh
if [ "$1" = "--cmd" ]; then
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
cat "$2"
"#,
        )
        .expect("write fake Fresh");
        let mut permissions = std::fs::metadata(&fresh)
            .expect("read fake Fresh metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&fresh, permissions).expect("make fake Fresh executable");

        let output = Command::new("sh")
            .arg("-c")
            .arg(command())
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .output()
            .expect("run Fresh wrapper");
        std::fs::remove_dir_all(&bin_dir).expect("remove fake Fresh bin directory");

        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("Fresh config is UTF-8");
        assert!(rendered.contains("\"theme\": \"builtin://terminal\""));
        assert!(rendered.contains("\"line_numbers\": false"));
    }

    #[cfg(unix)]
    #[test]
    fn terminal_theme_falls_back_when_config_show_is_unavailable() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let bin_dir = std::env::temp_dir().join(format!(
            "omh-fresh-theme-fallback-{}-{nonce}",
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
            .arg(command())
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .output()
            .expect("run Fresh fallback wrapper");
        std::fs::remove_dir_all(&bin_dir).expect("remove fake Fresh bin directory");

        assert!(output.status.success(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("Fresh config is UTF-8");
        assert!(rendered.contains("\"theme\": \"builtin://terminal\""));
    }

    #[test]
    fn fresh_always_uses_the_terminal_builtin() {
        assert!(command().contains("\"theme\": \"builtin://terminal\""));
    }

    #[test]
    fn missing_fresh_screen_is_rendered_with_install_source() {
        let rendered = render_missing_screen(command());

        assert_eq!(
            rendered,
            "Fresh is not installed.\n\ninstall with:\n  brew install fresh-editor\n  curl https://raw.githubusercontent.com/sinelaw/fresh/refs/heads/master/scripts/install.sh | sh\n\nsee https://github.com/sinelaw/fresh\n\npress enter to close...\n"
        );
    }
}
