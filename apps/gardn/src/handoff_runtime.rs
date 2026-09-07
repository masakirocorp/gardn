#[cfg(unix)]
use serde::{Deserialize, Serialize};

#[cfg(unix)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HandoffRuntimeState {
    pub pane_id: u32,
    pub child_pid: u32,
    pub rows: u16,
    pub cols: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    #[serde(default)]
    pub keyboard_protocol_flags: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard_protocol_ansi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_state: Option<crate::pane::InputState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_history_ansi: Option<String>,
}

#[cfg(unix)]
impl HandoffRuntimeState {
    pub fn with_pane_id(mut self, pane_id: crate::layout::PaneId) -> Self {
        self.pane_id = pane_id.raw();
        self
    }
}

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct ImportedHandoffRuntime {
    pub master_fd: std::os::fd::RawFd,
    pub state: HandoffRuntimeState,
}
#[cfg(unix)]
impl ImportedHandoffRuntime {
    pub(crate) fn close_imported_descriptor(self) {
        let _ = unsafe { libc::close(self.master_fd) };
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn discarding_imported_runtime_closes_only_duplicate_descriptor() {
        let mut fds = [-1; 2];
        let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
        assert_eq!(result, 0);
        let imported = ImportedHandoffRuntime {
            master_fd: fds[0],
            state: HandoffRuntimeState {
                pane_id: 7,
                child_pid: 42,
                rows: 24,
                cols: 80,
                cell_width_px: 0,
                cell_height_px: 0,
                keyboard_protocol_flags: 0,
                keyboard_protocol_ansi: None,
                input_state: None,
                initial_history_ansi: None,
            },
        };

        imported.close_imported_descriptor();

        assert_eq!(unsafe { libc::fcntl(fds[0], libc::F_GETFD) }, -1);
        assert_eq!(unsafe { libc::close(fds[1]) }, 0);
    }
}
