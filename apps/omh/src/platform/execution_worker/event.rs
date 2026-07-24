//! Worker-native runtime events.
//!
//! The PTY engine still emits app-layer events at the adapter edge; this module
//! owns the worker-facing event vocabulary so worker core never depends on
//! `AppEvent` or UI concepts.

use crate::detect::{Agent, AgentState};

/// Local handle used only inside the worker to correlate PTY adapter events
/// with a live runtime before the runtime id is known to outer protocol code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct RuntimeLocalId(u64);

impl RuntimeLocalId {
    pub(super) fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// Events produced by worker-owned runtimes for the connection loop.
#[derive(Debug)]
pub(super) enum WorkerEvent {
    /// Detector/state observation for a live terminal runtime.
    StateChanged {
        local_id: RuntimeLocalId,
        agent: Option<Agent>,
        state: AgentState,
        visible_blocker: bool,
        visible_idle: bool,
        visible_working: bool,
        process_exited: bool,
    },
    /// Child process exited for a live terminal runtime.
    RuntimeExit {
        local_id: RuntimeLocalId,
        exit_code: Option<i32>,
        exit_signal: Option<i32>,
    },
}
