use crate::app::{App, Mode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalKeyTarget {
    pub(crate) workspace_id: String,
    pub(crate) pane_id: crate::layout::PaneId,
}

impl App {
    pub(crate) fn host_keyboard_report_all_requested(&self) -> bool {
        let runtime = if self.default_client_view.popup_pane.is_some() {
            self.popup_runtime_for_view(&self.default_client_view)
        } else if self.default_client_view.mode == Mode::Terminal {
            self.default_client_view
                .active_workspace
                .and_then(|ws_idx| {
                    self.default_client_view
                        .focused_pane_for_workspace(&self.state, ws_idx)
                        .and_then(|(_, pane_id)| {
                            self.state.runtime_for_pane_in_workspace(
                                &self.terminal_runtimes,
                                ws_idx,
                                pane_id,
                            )
                        })
                })
        } else {
            None
        };

        runtime.is_some_and(|runtime| {
            let protocol = runtime.keyboard_protocol();
            protocol.reports_all_keys()
                || (protocol.reports_event_types()
                    && runtime
                        .input_state()
                        .is_some_and(|state| state.modify_other_keys))
        })
    }
}
