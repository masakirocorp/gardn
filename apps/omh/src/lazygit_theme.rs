pub(crate) const GIT_COMMAND: &str = "lazygit";

pub(crate) fn command() -> String {
    r#"if command -v lazygit >/dev/null 2>&1; then
  exec lazygit
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
    .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn launch_command_uses_native_terminal_palette() {
        let command = super::command();

        assert!(command.contains("exec lazygit"));
        assert!(!command.contains("LG_CONFIG_FILE"));
    }
}
