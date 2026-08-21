use std::path::{Path, PathBuf};

use tracing::warn;

use super::snapshot::{
    parse_history_snapshot, parse_snapshot, snapshot_file_version, SessionHistorySnapshot,
    SessionSnapshot, SNAPSHOT_VERSION,
};

fn session_path() -> PathBuf {
    crate::session::data_dir().join("session.json")
}

fn session_history_path() -> PathBuf {
    crate::session::data_dir().join("session-history.json")
}

pub(super) fn save_to_path(path: &Path, snapshot: &SessionSnapshot) -> std::io::Result<()> {
    super::atomic_json::save_json(path, snapshot)
}

/// Atomically persist a session snapshot at an explicit path.
///
/// Used by cross-session connection retirement so dormant named sessions can be
/// rewritten without switching the process-wide active session directory.
pub(crate) fn try_save_snapshot_at(path: &Path, snapshot: &SessionSnapshot) -> std::io::Result<()> {
    save_to_path(path, snapshot)
}

/// Load and parse a session snapshot from an explicit path.
///
/// Missing files return `Ok(None)`. Unreadable or invalid snapshots return
/// `Err` so callers can fail closed.
pub(crate) fn try_load_snapshot_at(path: &Path) -> std::io::Result<Option<SessionSnapshot>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)?;
    let snapshot = parse_snapshot(&content).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("cannot parse session snapshot {}: {error}", path.display()),
        )
    })?;
    Ok(Some(snapshot))
}

fn save_json_to_path<T: serde::Serialize>(path: &Path, snapshot: &T) -> std::io::Result<()> {
    super::atomic_json::save_json(path, snapshot)
}

pub(super) fn save_to_paths(
    session_path: &Path,
    history_path: &Path,
    snapshot: &SessionSnapshot,
    history: Option<&SessionHistorySnapshot>,
) -> std::io::Result<()> {
    save_to_path(session_path, snapshot)?;
    if let Some(history) = history {
        save_json_to_path(history_path, history)?;
    } else {
        clear_path(history_path)?;
    }
    Ok(())
}

pub(super) fn clear_path(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn try_save(
    snapshot: &SessionSnapshot,
    history: Option<&SessionHistorySnapshot>,
) -> std::io::Result<()> {
    let path = session_path();
    let history_path = session_history_path();
    save_to_paths(&path, &history_path, snapshot, history)?;
    crate::logging::session_saved(&path, snapshot.workspaces.len());
    Ok(())
}

pub fn try_clear() -> std::io::Result<()> {
    let path = session_path();
    clear_path(&path)?;
    let history_path = session_history_path();
    // Best-effort history cleanup after the durable session tombstone file is gone.
    match clear_path(&history_path) {
        Ok(()) => {}
        Err(err) => {
            crate::logging::session_clear_failed(&history_path, &err.to_string());
        }
    }
    crate::logging::session_cleared(&path);
    Ok(())
}

pub fn clear_history() {
    let path = session_history_path();
    if let Err(err) = clear_path(&path) {
        crate::logging::session_clear_failed(&path, &err.to_string());
    }
}

pub fn load() -> Option<SessionSnapshot> {
    let path = session_path();
    if !path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            warn!(err = %err, "failed to read session file");
            return None;
        }
    };
    match parse_snapshot(&content) {
        Ok(snapshot) => Some(snapshot),
        Err(err) => {
            if let Some(version) = snapshot_file_version(&content) {
                if version > SNAPSHOT_VERSION {
                    warn!(
                        file_version = version,
                        supported = SNAPSHOT_VERSION,
                        "session file is from a newer omh version, ignoring"
                    );
                    return None;
                }
            }
            warn!(err = %err, "failed to parse session file, ignoring");
            None
        }
    }
}

pub(crate) fn snapshots_reference_host(
    host_id: &crate::execution_host::ExecutionHostId,
) -> std::io::Result<bool> {
    let config_dir = crate::config::config_dir();
    let mut paths = vec![config_dir.join("session.json")];
    match std::fs::read_dir(config_dir.join("sessions")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    paths.push(entry.path().join("session.json"));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    snapshots_reference_host_at(&paths, host_id)
}

fn snapshots_reference_host_at(
    paths: &[PathBuf],
    host_id: &crate::execution_host::ExecutionHostId,
) -> std::io::Result<bool> {
    for path in paths {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let snapshot = parse_snapshot(&content).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "cannot verify connection profile references in {}: {error}",
                    path.display()
                ),
            )
        })?;
        if snapshot_references_host(&snapshot, host_id) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn snapshot_references_host(
    snapshot: &SessionSnapshot,
    host_id: &crate::execution_host::ExecutionHostId,
) -> bool {
    snapshot.groups.iter().any(|group| {
        group
            .default_location
            .as_ref()
            .is_some_and(|location| &location.execution_host_id == host_id)
    }) || snapshot.workspaces.iter().any(|workspace| {
        &workspace.default_location.execution_host_id == host_id
            || workspace.tabs.iter().any(|tab| {
                tab.panes.values().any(|pane| {
                    pane.location
                        .as_ref()
                        .is_some_and(|location| &location.execution_host_id == host_id)
                })
            })
    }) || snapshot
        .remote_termination_tombstones
        .iter()
        .any(|tombstone| &tombstone.location.execution_host_id == host_id)
}

pub fn load_history() -> Option<SessionHistorySnapshot> {
    let path = session_history_path();
    if !path.exists() {
        return None;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            warn!(err = %err, "failed to read session history file");
            return None;
        }
    };
    match parse_history_snapshot(&content) {
        Ok(snapshot) => Some(snapshot),
        Err(err) => {
            if let Some(version) = snapshot_file_version(&content) {
                if version > SNAPSHOT_VERSION {
                    warn!(
                        file_version = version,
                        supported = SNAPSHOT_VERSION,
                        "session history file is from a newer omh version, ignoring"
                    );
                    return None;
                }
            }
            warn!(err = %err, "failed to parse session history file, ignoring");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::AgentPanelScope;
    use crate::persist::snapshot::{
        PaneHistorySnapshot, TabHistorySnapshot, WorkspaceHistorySnapshot,
    };

    fn temp_session_path(name: &str) -> PathBuf {
        let unique = format!(
            "omh-session-tests-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("session.json")
    }

    fn temp_session_paths(name: &str) -> (PathBuf, PathBuf) {
        let session = temp_session_path(name);
        let history = session.with_file_name("session-history.json");
        (session, history)
    }

    fn empty_snapshot() -> SessionSnapshot {
        SessionSnapshot {
            version: SNAPSHOT_VERSION,
            session_namespace_id: "session-test".to_string(),
            remote_termination_tombstones: Vec::new(),
            groups: vec![crate::persist::snapshot::GroupSnapshot {
                id: crate::workspace::DEFAULT_GROUP_ID.to_string(),
                name: "group 1".to_string(),
                icon: crate::app::state::DEFAULT_GROUP_ICON.to_string(),
                accent: None,
                default_location: None,
                favorite_agent_profile_ids: Vec::new(),
                default_agent_profile_id: None,
            }],
            active_group: 0,
            group_filter_enabled: true,
            default_view: crate::persist::snapshot::SessionDefaultViewSnapshot::default(),
            workspaces: vec![],
            active: None,
            selected: 0,
            agent_panel_scope: AgentPanelScope::CurrentWorkspace,
            sidebar_width: Some(26),
            sidebar_collapsed: false,
            sidebar_section_split: Some(0.5),
            right_sidebar_width: Some(28),
            right_sidebar_collapsed: false,
            ui: crate::persist::snapshot::SessionUiSnapshot::default(),
            agent_follow_up: Vec::new(),
            pane_id_aliases: std::collections::HashMap::new(),
        }
    }

    fn history_snapshot(secret: &str) -> SessionHistorySnapshot {
        SessionHistorySnapshot {
            version: SNAPSHOT_VERSION,
            workspaces: vec![WorkspaceHistorySnapshot {
                tabs: vec![TabHistorySnapshot {
                    panes: std::collections::HashMap::from([(
                        0,
                        PaneHistorySnapshot {
                            ansi: secret.to_string(),
                            lines: 1,
                        },
                    )]),
                }],
            }],
        }
    }

    #[test]
    fn persisted_reference_in_another_session_is_detected() {
        let default_path = temp_session_path("host-reference-default");
        let named_path = temp_session_path("host-reference-named");
        let mut named_snapshot = empty_snapshot();
        let host_id = crate::execution_host::ExecutionHostId::new("ssh:workbox:1").unwrap();
        named_snapshot.groups[0].default_location =
            Some(crate::execution_host::ResourceLocation::new(
                host_id.clone(),
                crate::execution_host::HostPath::new("/srv/work").unwrap(),
            ));
        save_to_path(&default_path, &empty_snapshot()).unwrap();
        save_to_path(&named_path, &named_snapshot).unwrap();

        assert!(
            snapshots_reference_host_at(&[default_path.clone(), named_path.clone()], &host_id)
                .unwrap()
        );

        std::fs::remove_dir_all(default_path.parent().unwrap()).unwrap();
        std::fs::remove_dir_all(named_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn save_to_paths_writes_pane_history_only_to_history_file() {
        let (session_path, history_path) = temp_session_paths("split-history");

        save_to_paths(
            &session_path,
            &history_path,
            &empty_snapshot(),
            Some(&history_snapshot("split-secret")),
        )
        .unwrap();

        let session = std::fs::read_to_string(&session_path).unwrap();
        let history = std::fs::read_to_string(&history_path).unwrap();
        assert!(!session.contains("split-secret"));
        assert!(!session.contains("history"));
        assert!(history.contains("split-secret"));
    }

    #[test]
    fn save_to_paths_removes_stale_history_when_history_is_disabled() {
        let (session_path, history_path) = temp_session_paths("clear-history");
        save_to_paths(
            &session_path,
            &history_path,
            &empty_snapshot(),
            Some(&history_snapshot("stale-secret")),
        )
        .unwrap();

        save_to_paths(&session_path, &history_path, &empty_snapshot(), None).unwrap();

        assert!(session_path.exists());
        assert!(!history_path.exists());
    }

    #[test]
    fn clear_path_removes_existing_session_file() {
        let path = temp_session_path("clear-existing");
        save_to_path(&path, &empty_snapshot()).unwrap();

        clear_path(&path).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn clear_path_ignores_missing_session_file() {
        let path = temp_session_path("clear-missing");

        clear_path(&path).unwrap();

        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn save_to_path_preserves_existing_symlink() {
        let target = temp_session_path("symlink-target");
        let link = target.with_file_name("link.json");
        save_to_path(&target, &empty_snapshot()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let mut snap = empty_snapshot();
        snap.selected = 7;
        snap.default_view.selected = 7;
        save_to_path(&link, &snap).unwrap();

        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        let parsed = parse_snapshot(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(parsed.selected, 7);
    }

    #[cfg(unix)]
    #[test]
    fn save_to_path_writes_through_dangling_symlink() {
        let target = temp_session_path("dangling-target");
        let link = target.with_file_name("link.json");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        save_to_path(&link, &empty_snapshot()).unwrap();

        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn save_to_path_resolves_relative_symlink() {
        let session = temp_session_path("relative-symlink");
        let dir = session.parent().unwrap();
        std::fs::create_dir_all(dir).unwrap();
        let target = dir.join("real.json");
        let link = dir.join("link.json");
        std::os::unix::fs::symlink("real.json", &link).unwrap();

        save_to_path(&link, &empty_snapshot()).unwrap();

        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(target.exists());
    }
}
