//! Who owns this Gardn install and how it updates.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const GARDN_UPDATE_COMMAND: &str = "gardn update";
const MISE_UPDATE_COMMAND: &str = "mise upgrade gardn";
const MACOS_APP_CLI_PATH: &str = "/Applications/Gardn.app/Contents/MacOS/gardn";
const MISE_INSTALLS_DIR_ENV: &str = "MISE_INSTALLS_DIR";
const MACOS_APP_UPDATE_ERROR: &str = "Gardn is already installed as an app. Use Check for Updates in Gardn, or uninstall the app first.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateInstallAction {
    Direct,
    Mise,
    Nix,
    MacosApp,
}

impl UpdateInstallAction {
    pub(crate) fn current() -> Self {
        let current_exe = env::current_exe().ok();
        let app_present =
            cfg!(target_os = "macos") && is_macos_app_present_at(Path::new(MACOS_APP_CLI_PATH));
        install_kind_for(current_exe.as_deref(), app_present)
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Direct => GARDN_UPDATE_COMMAND,
            Self::Mise => MISE_UPDATE_COMMAND,
            Self::Nix => "update through Nix",
            Self::MacosApp => "Check for Updates in Gardn",
        }
    }

    pub(crate) fn instruction(self) -> String {
        match self {
            Self::Direct => {
                "Detach, run `gardn update`, then follow its restart guidance".to_string()
            }
            Self::Mise => {
                "Detach, run `mise upgrade gardn`, then restart this Gardn session when ready"
                    .to_string()
            }
            Self::Nix => {
                "Detach, update through Nix, then restart this Gardn session when ready".to_string()
            }
            Self::MacosApp => {
                "Detach, use Check for Updates in Gardn, then restart this Gardn session when ready"
                    .to_string()
            }
        }
    }

    pub(crate) fn availability_notification_detail(self) -> String {
        match self {
            Self::MacosApp => "Detach, then use Check for Updates in Gardn".to_string(),
            Self::Nix => "Detach, then update through Nix".to_string(),
            Self::Direct | Self::Mise => format!("Detach, then run `{}`", self.command()),
        }
    }

    pub(crate) fn availability_notification_body(self, version: &str) -> String {
        format!(
            "v{version} Available: {}",
            self.availability_notification_detail()
        )
    }

    pub(crate) fn self_update_error(self) -> Option<String> {
        match self {
            Self::MacosApp => Some(MACOS_APP_UPDATE_ERROR.to_string()),
            Self::Mise => Some(format!(
                "self-update is disabled for mise installs; run `{MISE_UPDATE_COMMAND}`"
            )),
            Self::Nix => Some(
                "self-update is disabled for Nix installs; update with `nix profile upgrade` or update the flake input that provides Gardn".into(),
            ),
            Self::Direct => None,
        }
    }
}

pub(crate) fn install_kind_for(
    current_exe: Option<&Path>,
    macos_app_present: bool,
) -> UpdateInstallAction {
    let Some(current_exe) = current_exe else {
        return UpdateInstallAction::Direct;
    };
    if is_macos_app_bundle_cli(current_exe) {
        return UpdateInstallAction::MacosApp;
    }
    if is_mise_managed_exe_path_following_links(current_exe) {
        return UpdateInstallAction::Mise;
    }
    if is_nix_managed_exe_path_following_links(current_exe) {
        return UpdateInstallAction::Nix;
    }
    if macos_app_present && is_stable_direct_cli(current_exe) {
        return UpdateInstallAction::MacosApp;
    }
    UpdateInstallAction::Direct
}

fn is_stable_direct_cli(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == "gardn" || name == "gardn.exe")
}

fn is_macos_app_bundle_cli(path: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let Some(macos_directory) = path.parent() else {
        return false;
    };
    let Some(contents_directory) = macos_directory.parent() else {
        return false;
    };
    let Some(app_bundle) = contents_directory.parent() else {
        return false;
    };
    path.file_name() == Some("gardn".as_ref())
        && macos_directory.file_name() == Some("MacOS".as_ref())
        && contents_directory.file_name() == Some("Contents".as_ref())
        && app_bundle.extension() == Some("app".as_ref())
}

fn is_macos_app_present_at(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn is_nix_managed_exe_path_following_links(path: &Path) -> bool {
    if is_nix_store_exe_path(path) {
        return true;
    }
    path.canonicalize()
        .is_ok_and(|path| is_nix_store_exe_path(&path))
}

fn is_mise_managed_exe_path_following_links(path: &Path) -> bool {
    if is_mise_managed_exe_path(path) {
        return true;
    }
    path.canonicalize()
        .is_ok_and(|path| is_mise_managed_exe_path(&path))
}

fn is_nix_store_exe_path(path: &Path) -> bool {
    path.starts_with("/nix/store")
}

fn is_mise_managed_exe_path(path: &Path) -> bool {
    mise_install_root(path).is_some()
}

fn mise_install_root(path: &Path) -> Option<PathBuf> {
    if let Some(root) = mise_install_root_under_configured_installs_dir(path) {
        return Some(root);
    }
    mise_install_root_under_named_installs_dir(path)
}

fn mise_install_root_under_configured_installs_dir(path: &Path) -> Option<PathBuf> {
    let installs_dir = env::var_os(MISE_INSTALLS_DIR_ENV)
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())?;
    let version_dir = mise_tool_version_dir(path)?;
    let tool_dir = version_dir.parent()?;
    paths_match(tool_dir.parent()?, &installs_dir).then_some(version_dir.to_path_buf())
}

fn mise_install_root_under_named_installs_dir(path: &Path) -> Option<PathBuf> {
    let version_dir = mise_tool_version_dir(path)?;
    let tool_dir = version_dir.parent()?;
    let installs_dir = tool_dir.parent()?;
    if installs_dir.file_name()? != "installs" {
        return None;
    }
    Some(version_dir.to_path_buf())
}

fn mise_tool_version_dir(path: &Path) -> Option<&Path> {
    if path.file_name()? != "gardn" {
        return None;
    }
    let bin_dir = path.parent()?;
    if bin_dir.file_name()? != "bin" {
        return None;
    }
    let version_dir = bin_dir.parent()?;
    let tool_dir = version_dir.parent()?;
    if tool_dir.file_name()? != "gardn" {
        return None;
    }
    Some(version_dir)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    let Ok(left) = left.canonicalize() else {
        return false;
    };
    let Ok(right) = right.canonicalize() else {
        return false;
    };
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestEnvVar;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn macos_app_bundle_cli_path_is_detected() {
        assert!(is_macos_app_bundle_cli(Path::new(
            "/Applications/Gardn.app/Contents/MacOS/gardn"
        )));
        assert_eq!(
            install_kind_for(
                Some(Path::new("/Applications/Gardn.app/Contents/MacOS/gardn")),
                false,
            ),
            UpdateInstallAction::MacosApp
        );
    }

    #[test]
    fn local_path_is_not_a_macos_app_bundle_cli() {
        assert!(!is_macos_app_bundle_cli(Path::new(
            "/Users/test/.local/bin/gardn"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn macos_app_bundle_cli_path_follows_symlinks() {
        let root = std::env::temp_dir().join(format!(
            "gardn-macos-app-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bundled_cli = root.join("Gardn.app/Contents/MacOS/gardn");
        let linked_cli = root.join("bin/gardn");
        fs::create_dir_all(bundled_cli.parent().unwrap()).unwrap();
        fs::create_dir_all(linked_cli.parent().unwrap()).unwrap();
        fs::write(&bundled_cli, b"gardn").unwrap();
        std::os::unix::fs::symlink(&bundled_cli, &linked_cli).unwrap();

        assert!(is_macos_app_bundle_cli(&linked_cli));
        assert_eq!(
            install_kind_for(Some(linked_cli.as_path()), false),
            UpdateInstallAction::MacosApp
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stable_direct_cli_defers_to_installed_app() {
        assert_eq!(
            install_kind_for(Some(Path::new("/Users/test/.local/bin/gardn")), true),
            UpdateInstallAction::MacosApp
        );
        assert_eq!(
            UpdateInstallAction::MacosApp.self_update_error().as_deref(),
            Some(MACOS_APP_UPDATE_ERROR)
        );
    }

    #[test]
    fn beta_and_dev_binaries_keep_direct_updates_when_app_is_present() {
        assert_eq!(
            install_kind_for(Some(Path::new("/Users/test/.local/bin/gardn-beta")), true),
            UpdateInstallAction::Direct
        );
        assert_eq!(
            install_kind_for(Some(Path::new("/Users/test/.local/bin/gardn-dev")), true),
            UpdateInstallAction::Direct
        );
    }

    #[test]
    fn mise_and_nix_keep_their_owners_when_app_is_present() {
        assert_eq!(
            install_kind_for(
                Some(Path::new(
                    "/home/user/.local/share/mise/installs/gardn/0.6.6/bin/gardn"
                )),
                true,
            ),
            UpdateInstallAction::Mise
        );
        assert_eq!(
            install_kind_for(
                Some(Path::new("/nix/store/abc123-gardn-0.6.1/bin/gardn")),
                true,
            ),
            UpdateInstallAction::Nix
        );
    }

    #[test]
    fn missing_exe_does_not_infer_app_ownership_from_presence() {
        assert_eq!(install_kind_for(None, true), UpdateInstallAction::Direct);
    }

    #[test]
    fn macos_app_presence_is_checked_at_an_injected_path() {
        let root = std::env::temp_dir().join(format!(
            "gardn-macos-app-present-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app_cli = root.join("Gardn.app/Contents/MacOS/gardn");
        fs::create_dir_all(app_cli.parent().unwrap()).unwrap();
        fs::write(&app_cli, b"gardn").unwrap();

        assert!(is_macos_app_present_at(&app_cli));
        fs::remove_file(&app_cli).unwrap();
        assert!(!is_macos_app_present_at(&app_cli));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn macos_app_install_routes_updates_to_the_app() {
        assert_eq!(
            UpdateInstallAction::MacosApp.command(),
            "Check for Updates in Gardn"
        );
        assert_eq!(
            UpdateInstallAction::MacosApp.instruction(),
            "Detach, use Check for Updates in Gardn, then restart this Gardn session when ready"
        );
        assert_eq!(
            UpdateInstallAction::MacosApp.availability_notification_body("1.2.3"),
            "v1.2.3 Available: Detach, then use Check for Updates in Gardn"
        );
    }

    #[test]
    fn update_instructions_match_install_owner() {
        assert_eq!(
            UpdateInstallAction::Direct.instruction(),
            "Detach, run `gardn update`, then follow its restart guidance"
        );
        assert_eq!(
            UpdateInstallAction::Mise.instruction(),
            "Detach, run `mise upgrade gardn`, then restart this Gardn session when ready"
        );
        assert_eq!(
            UpdateInstallAction::Nix.instruction(),
            "Detach, update through Nix, then restart this Gardn session when ready"
        );
    }
    #[test]
    fn nix_store_path_is_detected() {
        let path = Path::new("/nix/store/abc123-gardn-0.6.1/bin/gardn");
        assert!(is_nix_store_exe_path(path));
    }

    #[test]
    fn non_nix_store_path_is_not_detected() {
        let path = Path::new("/usr/local/bin/gardn");
        assert!(!is_nix_store_exe_path(path));
    }

    #[test]
    fn mise_install_path_is_detected() {
        let path = Path::new("/home/user/.local/share/mise/installs/gardn/0.6.6/bin/gardn");
        assert!(is_mise_managed_exe_path(path));
        assert_eq!(
            mise_install_root(path).unwrap(),
            PathBuf::from("/home/user/.local/share/mise/installs/gardn/0.6.6")
        );
    }

    #[test]
    fn mise_alias_install_path_is_detected() {
        let path = Path::new("/home/user/.local/share/mise/installs/gardn/latest/bin/gardn");
        assert!(is_mise_managed_exe_path(path));
    }

    #[test]
    fn mise_configured_installs_dir_path_is_detected() {
        let _guard = env_lock().lock().unwrap();
        let _mise_installs_dir_env = TestEnvVar::set(MISE_INSTALLS_DIR_ENV, "/opt/mise-tools");
        let path = Path::new("/opt/mise-tools/gardn/0.6.6/bin/gardn");
        assert!(is_mise_managed_exe_path(path));
        assert_eq!(
            mise_install_root(path).unwrap(),
            PathBuf::from("/opt/mise-tools/gardn/0.6.6")
        );
    }

    #[test]
    fn non_mise_install_path_is_not_detected() {
        let path = Path::new("/home/user/.local/bin/gardn");
        assert!(!is_mise_managed_exe_path(path));
    }
}
