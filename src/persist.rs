//! Session persistence — save/restore workspaces, layouts, and working directories.
//!
//! Stored at `~/.config/hako/session.json`.
//! Optional pane screen history is stored separately at `session-history.json`.

mod io;
mod restore;
mod snapshot;

pub use self::io::{clear, clear_history, load, load_history, save};
pub use self::restore::restore;
#[cfg(unix)]
pub use self::restore::{handoff_pane_aliases, restore_handoff};
#[cfg(test)]
pub use self::snapshot::GroupSnapshot;
pub use self::snapshot::{
    capture, capture_handoff, capture_history, DirectionSnapshot, LayoutSnapshot,
    SessionHistorySnapshot, SessionSnapshot, SessionUiSnapshot, TabSnapshot, WorkspaceSnapshot,
};
