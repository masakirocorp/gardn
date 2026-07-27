//! Session persistence — save/restore workspaces, layouts, and working directories.
//!
//! Stored at `~/.config/omh/session.json`.
//! Optional pane screen history is stored separately at `session-history.json`.
//! Installed plugins are persisted globally at `~/.config/omh/plugins.json`.
//! SSH connection profiles are persisted globally at `~/.config/omh/ssh-profiles.json`.

pub(crate) mod atomic_json;
pub(crate) mod installation;
mod io;
pub mod plugin_registry;
mod restore;
mod snapshot;
pub mod ssh_profiles;

pub use self::io::{clear_history, load, load_history, try_clear, try_save};
pub(crate) use self::io::{snapshots_reference_host, try_load_snapshot_at, try_save_snapshot_at};
pub use self::restore::restore;
#[cfg(unix)]
pub use self::restore::{handoff_pane_aliases, restore_handoff};
#[cfg(unix)]
pub use self::snapshot::capture_handoff;
pub use self::snapshot::SessionUiSnapshot;
pub use self::snapshot::{
    capture, capture_history, DirectionSnapshot, LayoutSnapshot, SessionHistorySnapshot,
    SessionSnapshot, TabSnapshot, WorkspaceSnapshot,
};
#[cfg(test)]
pub use self::snapshot::{
    GroupSnapshot, PaneSnapshot, RemoteTerminationTombstoneSnapshot, SessionDefaultViewSnapshot,
};
