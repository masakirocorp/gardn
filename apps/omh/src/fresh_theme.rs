pub(crate) const IDE_COMMAND: &str = "fresh .";

pub(crate) fn command() -> String {
    r#"if command -v fresh >/dev/null 2>&1; then
  config_dir="$(mktemp -d "${TMPDIR:-/tmp}/omh-fresh.XXXXXX")" || exec fresh .
  cleanup() {
    rm -rf "$config_dir"
  }
  trap cleanup EXIT INT TERM
  cat > "$config_dir/config.json" <<'OMH_FRESH_CONFIG'
{
  "theme": "terminal"
}
OMH_FRESH_CONFIG
  fresh --config "$config_dir/config.json" .
  status=$?
  cleanup
  exit "$status"
fi

printf '%s\n' \
  'Fresh is not installed.' \
  '' \
  'install with:' \
  '  brew install sinelaw/fresh/fresh-editor' \
  '' \
  'see https://getfresh.dev/' \
  '' \
  'press enter to close...'
read _
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn launch_command_forces_terminal_theme_and_cleans_up() {
        let command = super::command();

        assert!(command.contains("fresh --config \"$config_dir/config.json\" ."));
        assert!(command.contains("\"theme\": \"terminal\""));
        assert!(command.contains("rm -rf \"$config_dir\""));
        assert!(command.contains("https://getfresh.dev/"));
    }
}
