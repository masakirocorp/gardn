use crate::app::state::GithubOrganization;

pub(crate) const GITHUB_COMMAND: &str = "ghui";
pub(crate) const REQUIRED_VERSION: &str = "0.10.0-masakiro.2";
pub(crate) const FORK_URL: &str = "https://github.com/masakirocorp/ghui";

pub(crate) fn command(organization: Option<&GithubOrganization>) -> String {
    let organization = organization.map_or("", GithubOrganization::as_str);
    format!(
        r#"if command -v ghui >/dev/null 2>&1; then
  installed_version="$(ghui --version 2>/dev/null)"
  if [ "$installed_version" = "{REQUIRED_VERSION}" ]; then
    GHUI_THEME=system GHUI_SHOW_SCROLLBARS=true GHUI_ORG='{organization}' exec ghui
  fi
  printf '%s\n' \
    'Gardn requires its pinned ghui companion release.' \
    '' \
    "installed: $installed_version" \
    'required:  {REQUIRED_VERSION}' \
    ''
else
  printf '%s\n' \
    'ghui is not installed.' \
    ''
fi
printf '%s\n' \
  'install or upgrade with:' \
  '  brew install masakirocorp/tap/ghui' \
  '' \
  'release: https://github.com/masakirocorp/ghui/releases/tag/v{REQUIRED_VERSION}' \
  'source:  {FORK_URL}' \
  '' \
  'press enter to close...'
read -r _ || true
"#
    )
}

#[cfg(test)]
mod tests {
    use super::{command, FORK_URL, REQUIRED_VERSION};
    use crate::app::state::GithubOrganization;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn launch_uses_pinned_fork_controls() {
        let organization = GithubOrganization::parse("masakirocorp")
            .expect("valid organization")
            .expect("organization");
        let output = command(Some(&organization));

        assert!(output.contains(&format!("required:  {REQUIRED_VERSION}")));
        assert!(output.contains("GHUI_THEME=system"));
        assert!(output.contains("GHUI_SHOW_SCROLLBARS=true"));
        assert!(output.contains("GHUI_ORG='masakirocorp'"));
    }

    #[test]
    fn missing_ghui_guidance_names_masakiro_distribution() {
        let output = command(None);

        assert!(output.contains("brew install masakirocorp/tap/ghui"));
        assert!(output.contains(&format!("releases/tag/v{REQUIRED_VERSION}")));
        assert!(output.contains(FORK_URL));
    }

    #[cfg(unix)]
    #[test]
    fn matching_fork_release_receives_launch_scope() {
        let root = test_root("matching");
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("create fake ghui bin directory");
        let capture_path = root.join("environment");
        let ghui = bin_dir.join("ghui");
        std::fs::write(
            &ghui,
            format!(
                r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' '{REQUIRED_VERSION}'
  exit 0
fi
printf '%s\n%s\n%s\n' "$GHUI_THEME" "$GHUI_SHOW_SCROLLBARS" "$GHUI_ORG" > "$CAPTURE_PATH"
exit 7
"#
            ),
        )
        .expect("write fake ghui");
        make_executable(&ghui);
        let organization = GithubOrganization::parse("masakirocorp")
            .expect("valid organization")
            .expect("organization");

        let output = Command::new("sh")
            .arg("-c")
            .arg(command(Some(&organization)))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run ghui wrapper");

        assert_eq!(output.status.code(), Some(7));
        assert_eq!(
            std::fs::read_to_string(&capture_path).expect("captured launch environment"),
            "system\ntrue\nmasakirocorp\n"
        );
        std::fs::remove_dir_all(root).expect("remove fake ghui directory");
    }

    #[cfg(unix)]
    #[test]
    fn mismatched_release_fails_closed() {
        let root = test_root("mismatch");
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("create fake ghui bin directory");
        let executed_path = root.join("executed");
        let ghui = bin_dir.join("ghui");
        std::fs::write(
            &ghui,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' '0.9.1'
  exit 0
fi
touch "$EXECUTED_PATH"
"#,
        )
        .expect("write fake ghui");
        make_executable(&ghui);

        let output = Command::new("sh")
            .arg("-c")
            .arg(command(None))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("EXECUTED_PATH", &executed_path)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run ghui wrapper");

        assert!(output.status.success());
        assert!(!executed_path.exists(), "mismatched ghui must not launch");
        let rendered = String::from_utf8(output.stdout).expect("guidance is UTF-8");
        assert!(rendered.contains("installed: 0.9.1"));
        assert!(rendered.contains(&format!("required:  {REQUIRED_VERSION}")));
        std::fs::remove_dir_all(root).expect("remove fake ghui directory");
    }

    #[cfg(unix)]
    fn test_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("gardn-ghui-{label}-{}-{nonce}", std::process::id()))
    }

    #[cfg(unix)]
    fn make_executable(path: &std::path::Path) {
        let mut permissions = std::fs::metadata(path)
            .expect("read fake ghui metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).expect("make fake ghui executable");
    }
}
