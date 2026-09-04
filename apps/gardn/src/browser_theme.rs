pub(crate) const BROWSER_COMMAND: &str = "terminal-browser";

pub(crate) fn command() -> String {
    r#"if command -v terminal-browser >/dev/null 2>&1; then
  exec terminal-browser
fi
printf '%s\n' \
  'terminal-browser is not installed.' \
  '' \
  'install with:' \
  '  curl -fsSL https://terminal-browser.sh/install | bash' \
  '' \
  'see https://github.com/zenbu-labs/terminal-browser' \
  '' \
  'press enter to close...'
read -r _
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn missing_browser_guidance_names_terminal_browser() {
        let script = command();
        assert!(script.contains("terminal-browser is not installed."));
        assert!(script.contains("curl -fsSL https://terminal-browser.sh/install | bash"));
    }
}
