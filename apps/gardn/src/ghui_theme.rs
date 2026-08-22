pub(crate) const GITHUB_COMMAND: &str = "ghui";

pub(crate) fn command() -> String {
    r#"if command -v ghui >/dev/null 2>&1; then
  GHUI_THEME=system exec ghui
fi
printf '%s\n' \
  'ghui is not installed.' \
  '' \
  'install the Gardn-compatible build from:' \
  '  https://github.com/masakirocorp/ghui' \
  '' \
  'press enter to close...'
read -r _"#
        .to_string()
}
