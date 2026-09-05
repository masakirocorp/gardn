use crate::github::ResolvedGithubScope;

pub(crate) const GITHUB_COMMAND: &str = "ghui";
pub(crate) const REQUIRED_VERSION: &str = "0.10.0-masakiro.6";
pub(crate) const FORK_URL: &str = "https://github.com/masakirocorp/ghui";

fn shell_escape(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

pub(crate) fn command_with_scope(workspace_name: &str, scope: &ResolvedGithubScope) -> String {
    let repositories = if scope.repositories.is_empty() {
        String::new()
    } else {
        let repositories = scope
            .repositories
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        serde_json::to_string(&repositories).expect("GitHub repositories serialize")
    };
    let repository_paths =
        serde_json::to_string(&scope.repository_paths).expect("GitHub repository paths serialize");
    let organization = scope
        .organization
        .as_ref()
        .map_or("", |organization| organization.as_str());
    let organization = shell_escape(organization);
    let repositories = shell_escape(&repositories);
    let repository_paths = shell_escape(&repository_paths);
    let workspace_name = shell_escape(workspace_name);
    format!(
        r#"if command -v ghui >/dev/null 2>&1; then
  installed_version="$(ghui --version 2>/dev/null)"
  if [ "$installed_version" = "{REQUIRED_VERSION}" ]; then
    trap '' USR2 2>/dev/null || true
    GHUI_THEME=system GHUI_SHOW_SCROLLBARS=true GHUI_SYSTEM_THEME_AUTO_RELOAD=true GHUI_ORG='{organization}' GHUI_REPOSITORIES='{repositories}' GHUI_REPOSITORY_PATHS='{repository_paths}' GHUI_WORKSPACE_NAME='{workspace_name}' exec ghui
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
    use super::{command_with_scope, REQUIRED_VERSION};
    use crate::app::state::GithubOrganization;
    use crate::github::{GithubRepository, ResolvedGithubScope};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    fn organization_scope() -> ResolvedGithubScope {
        ResolvedGithubScope {
            repositories: Vec::new(),
            repository_paths: std::collections::BTreeMap::new(),
            organization: Some(
                GithubOrganization::parse("masakirocorp")
                    .expect("valid organization")
                    .expect("organization"),
            ),
        }
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
printf '%s\n%s\n%s\n%s\n%s\n%s\n%s\n' "$GHUI_THEME" "$GHUI_SHOW_SCROLLBARS" "$GHUI_SYSTEM_THEME_AUTO_RELOAD" "$GHUI_ORG" "$GHUI_REPOSITORIES" "$GHUI_REPOSITORY_PATHS" "$GHUI_WORKSPACE_NAME" > "$CAPTURE_PATH"
exit 7
"#
            ),
        )
        .expect("write fake ghui");
        make_executable(&ghui);
        let mut scope = organization_scope();
        scope.repositories = vec![GithubRepository::parse("Acme/One").expect("repository")];
        scope.organization = None;

        let output = Command::new("sh")
            .arg("-c")
            .arg(command_with_scope("Space's Home", &scope))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run ghui wrapper");

        assert_eq!(output.status.code(), Some(7));
        assert_eq!(
            std::fs::read_to_string(&capture_path).expect("captured launch environment"),
            "system\ntrue\ntrue\n\n[\"acme/one\"]\n{}\nSpace's Home\n"
        );
        let output = Command::new("sh")
            .arg("-c")
            .arg(command_with_scope(
                "Organization Space",
                &organization_scope(),
            ))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("CAPTURE_PATH", &capture_path)
            .env("GHUI_REPOSITORIES", "[\"stale/repository\"]")
            .output()
            .expect("run organization-scoped ghui wrapper");
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(
            std::fs::read_to_string(&capture_path).expect("captured organization environment"),
            "system\ntrue\ntrue\nmasakirocorp\n\n{}\nOrganization Space\n"
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
            .arg(command_with_scope("Space", &organization_scope()))
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
