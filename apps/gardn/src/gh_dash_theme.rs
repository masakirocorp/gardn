use crate::app::state::{GithubOrganization, Palette};
use crate::external_tool_theme::{background_fallback, foreground_fallback, palette_color};
use crate::terminal_theme::{DefaultColorKind, TerminalTheme, ThemeAppearance};

pub(crate) const GITHUB_COMMAND: &str = "gh dash";

pub(crate) fn command(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
    organization: Option<&GithubOrganization>,
) -> String {
    let config = config(
        palette,
        appearance,
        terminal_theme,
        passthrough_terminal,
        organization,
    );
    format!(
        r#"if command -v gh >/dev/null 2>&1 && gh dash --version >/dev/null 2>&1; then
  override_dir="$(mktemp -d "${{TMPDIR:-/tmp}}/gardn-gh-dash.XXXXXX")" || exit 1
  cleanup() {{
    rm -rf "$override_dir"
  }}
  trap cleanup EXIT INT TERM
  cat > "$override_dir/config.yml" <<'GARDN_GH_DASH_CONFIG'
{config}GARDN_GH_DASH_CONFIG
  gh dash --config "$override_dir/config.yml"
  status=$?
  cleanup
  exit "$status"
fi
printf '%s\n' \
  'gh-dash is not installed.' \
  '' \
  'install with:' \
  '  brew install gh' \
  '  gh extension install dlvhdr/gh-dash' \
  '' \
  'see https://github.com/dlvhdr/gh-dash' \
  '' \
  'press enter to close...'
read -r _
"#
    )
}

fn config(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
    passthrough_terminal: bool,
    organization: Option<&GithubOrganization>,
) -> String {
    let sections = organization.map_or_else(String::new, |organization| {
        format!(
            "prSections:\n  - title: Open Pull Requests\n    filters: org:{} is:open\nissuesSections:\n  - title: Open Issues\n    filters: org:{} is:open\n",
            organization.as_str(),
            organization.as_str()
        )
    });
    let theme = if passthrough_terminal {
        String::new()
    } else {
        format!(
            "theme:\n  colors:\n{}",
            theme_colors(palette, appearance, terminal_theme)
        )
    };
    format!("smartFilteringAtLaunch: false\n{sections}{theme}")
}

fn theme_colors(
    palette: &Palette,
    appearance: ThemeAppearance,
    terminal_theme: TerminalTheme,
) -> String {
    let foreground = |color| {
        palette_color(
            color,
            terminal_theme,
            DefaultColorKind::Foreground,
            foreground_fallback(appearance),
        )
        .hex()
    };
    let background = |color| {
        palette_color(
            color,
            terminal_theme,
            DefaultColorKind::Background,
            background_fallback(appearance),
        )
        .hex()
    };
    format!(
        "    text:\n      primary: \"{}\"\n      secondary: \"{}\"\n      inverted: \"{}\"\n      faint: \"{}\"\n      warning: \"{}\"\n      success: \"{}\"\n      actor: \"{}\"\n    background:\n      selected: \"{}\"\n    border:\n      primary: \"{}\"\n      secondary: \"{}\"\n      faint: \"{}\"\n",
        foreground(palette.text),
        foreground(palette.subtext0),
        foreground(palette.panel_bg),
        foreground(palette.overlay0),
        foreground(palette.peach),
        foreground(palette.green),
        foreground(palette.blue),
        background(palette.surface1),
        foreground(palette.accent),
        foreground(palette.overlay1),
        foreground(palette.surface_dim),
    )
}

#[cfg(test)]
mod tests {
    use super::{command, config};
    use crate::app::state::{GithubOrganization, Palette};
    use crate::terminal_theme::{TerminalTheme, ThemeAppearance};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn generated_override_scopes_org_and_disables_smart_filtering() {
        let organization = GithubOrganization::parse("masakirocorp")
            .expect("valid organization")
            .expect("organization present");
        let output = config(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
            Some(&organization),
        );
        assert!(output.contains("smartFilteringAtLaunch: false"));
        assert!(output.contains("filters: org:masakirocorp is:open"));
        assert!(output.contains("theme:\n  colors:"));
        assert!(output.contains("primary: \"#"));
        assert!(output.contains("issuesSections:\n"));
        assert!(!output.contains("issueSections:\n"));
    }

    #[test]
    fn no_organization_omits_pull_request_and_issue_sections() {
        let output = config(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
            None,
        );

        assert!(!output.contains("prSections:"));
        assert!(!output.contains("issuesSections:"));
    }

    #[test]
    fn missing_gh_dash_guidance_names_extension() {
        let output = command(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            false,
            None,
        );

        assert!(output.contains("gh extension install dlvhdr/gh-dash"));
    }

    #[test]
    fn passthrough_keeps_scope_without_theme() {
        let organization = GithubOrganization::parse("masakirocorp")
            .expect("valid organization")
            .expect("organization present");
        let output = config(
            &Palette::terminal(),
            ThemeAppearance::Dark,
            TerminalTheme::default(),
            true,
            Some(&organization),
        );
        assert!(output.contains("smartFilteringAtLaunch: false"));
        assert!(output.contains("org:masakirocorp"));
        assert!(!output.contains("theme:"));
    }

    #[cfg(unix)]
    #[test]
    fn launch_passes_scoped_theme_config_and_cleans_up_after_exit() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "gardn-gh-dash-theme-{}-{nonce}",
            std::process::id()
        ));
        let bin_dir = root.join("bin");
        let tmp_dir = root.join("tmp");
        std::fs::create_dir_all(&bin_dir).expect("create fake gh bin directory");
        std::fs::create_dir_all(&tmp_dir).expect("create temporary directory");
        let capture_path = root.join("config-dir");
        let gh = bin_dir.join("gh");
        std::fs::write(
            &gh,
            r#"#!/bin/sh
if [ "$1" = "dash" ] && [ "$2" = "--version" ]; then
  exit 0
fi
if [ "$1" = "dash" ] && [ "$2" = "--config" ]; then
  printf '%s' "${3%/*}" > "$CAPTURE_PATH"
  cat "$3"
  exit 7
fi
exit 9
"#,
        )
        .expect("write fake gh");
        let mut permissions = std::fs::metadata(&gh)
            .expect("read fake gh metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&gh, permissions).expect("make fake gh executable");
        let organization = GithubOrganization::parse("masakirocorp")
            .expect("valid organization")
            .expect("organization present");

        let output = Command::new("sh")
            .arg("-c")
            .arg(command(
                &Palette::tokyo_night(),
                ThemeAppearance::Dark,
                TerminalTheme::default(),
                false,
                Some(&organization),
            ))
            .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
            .env("TMPDIR", &tmp_dir)
            .env("CAPTURE_PATH", &capture_path)
            .output()
            .expect("run gh-dash wrapper");
        let generated_dir =
            std::path::PathBuf::from(std::fs::read_to_string(&capture_path).expect("config path"));
        assert_eq!(output.status.code(), Some(7));
        assert!(output.stderr.is_empty(), "{output:?}");
        let rendered = String::from_utf8(output.stdout).expect("gh-dash config is UTF-8");
        assert!(rendered.contains("filters: org:masakirocorp is:open"));
        assert!(rendered.contains("primary: \"#7aa2f7\""));
        assert!(
            !generated_dir.exists(),
            "wrapper should remove its override"
        );
        std::fs::remove_dir_all(root).expect("remove fake gh directory");
    }
}
