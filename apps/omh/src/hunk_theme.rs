pub(crate) const DIFF_COMMAND: &str = "hunk diff --watch";

pub(crate) fn command() -> String {
    r#"if command -v hunk >/dev/null 2>&1; then
  exec hunk diff --watch --theme auto
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
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn launch_command_uses_terminal_detected_theme() {
        let command = super::command();

        assert!(command.contains("exec hunk diff --watch --theme auto"));
        assert!(!command.contains("XDG_CONFIG_HOME"));
    }
}
