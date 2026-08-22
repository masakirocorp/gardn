use std::path::PathBuf;

use crate::app::{App, ClientViewState, Mode};
use crate::layout::PaneId;
use crate::pane::PaneLaunchEnv;
use crate::popup_size::{resolve_popup_geometry, PopupSize};
use crate::terminal::{TerminalId, TerminalRuntime, TerminalState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PopupGeometry {
    pub width: Option<PopupSize>,
    pub height: Option<PopupSize>,
}

impl App {
    pub(crate) fn close_popup_pane_for_view(&mut self, view: &mut ClientViewState) -> bool {
        let Some(pane_id) = view.popup_pane.take() else {
            return false;
        };
        self.close_popup_pane_by_id(pane_id)
    }

    pub(crate) fn close_popup_pane_by_id(&mut self, pane_id: PaneId) -> bool {
        let Some(popup) = self.state.popup_panes.remove(&pane_id) else {
            return false;
        };
        self.state.plugin_panes.remove(&pane_id);
        self.state
            .direct_attach_resize_locks
            .remove(&popup.terminal_id);
        self.state.terminals.remove(&popup.terminal_id);
        if let Some(runtime) = self.terminal_runtimes.remove(&popup.terminal_id) {
            runtime.shutdown();
        }
        if self.default_client_view.popup_pane == Some(pane_id) {
            self.default_client_view.popup_pane = None;
            self.default_client_view.mode = if self.default_client_view.active_workspace.is_some() {
                Mode::Terminal
            } else {
                Mode::Navigate
            };
        }
        self.render_dirty.request_generic();
        self.render_notify.notify_one();
        true
    }

    pub(crate) fn spawn_popup_argv_command_for_view(
        &mut self,
        view: &mut ClientViewState,
        argv: &[String],
        cwd: Option<PathBuf>,
        extra_env: Vec<(String, String)>,
        geometry: PopupGeometry,
    ) -> std::io::Result<(PaneId, TerminalId)> {
        if view.popup_pane.is_some() {
            return Err(std::io::Error::other("popup already open for client"));
        }
        let Some(ws_idx) = view.active_workspace else {
            return Err(std::io::Error::other("no active workspace"));
        };
        let ws = self
            .state
            .workspaces
            .get(ws_idx)
            .ok_or_else(|| std::io::Error::other("active workspace disappeared"))?;
        let tab_idx = view
            .active_tab_index_for_workspace(&self.state, ws_idx)
            .ok_or_else(|| std::io::Error::other("active tab disappeared"))?;
        let tab = ws
            .tabs
            .get(tab_idx)
            .ok_or_else(|| std::io::Error::other("active tab disappeared"))?;
        let focused_pane = view
            .focused_pane_for_workspace(&self.state, ws_idx)
            .map(|(_, pane_id)| pane_id)
            .ok_or_else(|| std::io::Error::other("active tab has no focused pane"))?;
        let cwd = cwd
            .or_else(|| {
                tab.cwd_for_pane(focused_pane, &self.state.terminals, &self.terminal_runtimes)
            })
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
        let area =
            if view.computed.terminal_area.width >= 4 && view.computed.terminal_area.height >= 4 {
                view.computed.terminal_area
            } else if self.state.view.terminal_area.width >= 4
                && self.state.view.terminal_area.height >= 4
            {
                self.state.view.terminal_area
            } else {
                let (rows, cols) = self.state.estimate_pane_size();
                ratatui::layout::Rect::new(0, 0, cols, rows)
            };
        let geometry = resolve_popup_geometry(geometry.width, geometry.height, area)
            .ok_or_else(|| std::io::Error::other("terminal area too small for popup"))?;
        let pane_id = PaneId::alloc();
        let terminal_id = TerminalId::alloc();
        let launch_env = PaneLaunchEnv::from_extra(extra_env).without_pane_identity();
        let runtime = TerminalRuntime::spawn_argv_command(
            pane_id,
            geometry.inner.height,
            geometry.inner.width,
            cwd.clone(),
            argv,
            &launch_env,
            self.state.pane_scrollback_limit_bytes,
            self.state.host_terminal_theme,
            self.event_tx.clone(),
            self.render_notify.clone(),
            self.render_dirty.clone(),
        )?;
        // Popup surfaces are not workspace panes, so agent detection and lifecycle
        // ownership stay disabled until the pane is explicitly closed.
        runtime.set_full_lifecycle_authority_active(true);
        let terminal = TerminalState::new(terminal_id.clone(), cwd).with_launch_argv(argv.to_vec());
        self.terminal_runtimes.insert(terminal_id.clone(), runtime);
        self.state.terminals.insert(terminal_id.clone(), terminal);
        self.state.popup_panes.insert(
            pane_id,
            crate::app::state::PopupPaneState {
                pane_id,
                terminal_id: terminal_id.clone(),
                width: Some(PopupSize::Cells(geometry.outer.width)),
                height: Some(PopupSize::Cells(geometry.outer.height)),
                owner: Some(view.id()),
            },
        );
        view.popup_pane = Some(pane_id);
        view.mode = Mode::Terminal;
        Ok((pane_id, terminal_id))
    }
}

#[cfg(test)]
impl App {
    pub(crate) fn install_test_popup_runtime(
        &mut self,
        view: &mut ClientViewState,
        runtime: TerminalRuntime,
    ) -> (PaneId, TerminalId) {
        let pane_id = PaneId::alloc();
        let terminal_id = TerminalId::alloc();
        self.terminal_runtimes.insert(terminal_id.clone(), runtime);
        self.state.terminals.insert(
            terminal_id.clone(),
            TerminalState::new(terminal_id.clone(), PathBuf::from("/popup")),
        );
        self.state.popup_panes.insert(
            pane_id,
            crate::app::state::PopupPaneState {
                pane_id,
                terminal_id: terminal_id.clone(),
                width: None,
                height: None,
                owner: Some(view.id()),
            },
        );
        view.popup_pane = Some(pane_id);
        view.mode = Mode::Terminal;
        (pane_id, terminal_id)
    }
}
impl App {
    pub(crate) fn popup_public_pane_id(&self, pane_id: PaneId) -> String {
        format!("popup:{}", pane_id.raw())
    }

    pub(crate) fn parse_popup_public_pane_id(&self, value: &str) -> Option<PaneId> {
        let raw = value.strip_prefix("popup:")?.parse::<u32>().ok()?;
        let pane_id = PaneId::from_raw(raw);
        self.state
            .popup_panes
            .contains_key(&pane_id)
            .then_some(pane_id)
    }

    pub(crate) fn popup_pane_info_for_view(
        &self,
        view: &ClientViewState,
        pane_id: PaneId,
    ) -> Option<crate::api::schema::PaneInfo> {
        let popup = self.state.popup_panes.get(&pane_id)?;
        if popup.owner.is_some_and(|owner| owner != view.id()) {
            return None;
        }
        let terminal = self.state.terminals.get(&popup.terminal_id)?;
        let ws_idx = view.active_workspace?;
        let tab_idx = view.active_tab_index_for_workspace(&self.state, ws_idx)?;
        let runtime = self.terminal_runtimes.get(&popup.terminal_id)?;
        let scroll = runtime
            .scroll_metrics()
            .map(|metrics| crate::api::schema::PaneScrollInfo {
                offset_from_bottom: metrics.offset_from_bottom as u64,
                max_offset_from_bottom: metrics.max_offset_from_bottom as u64,
                viewport_rows: metrics.viewport_rows as u64,
            });
        let presentation = terminal.effective_presentation();
        Some(crate::api::schema::PaneInfo {
            pane_id: self.popup_public_pane_id(pane_id),
            terminal_id: terminal.id.to_string(),
            location: crate::api::schema::resource_location_params_from(&terminal.location),
            workspace_id: self.public_workspace_id(ws_idx),
            tab_id: self.public_tab_id(ws_idx, tab_idx)?,
            focused: view.popup_pane == Some(pane_id),
            cwd: runtime.cwd().map(|cwd| cwd.display().to_string()),
            foreground_cwd: runtime
                .foreground_cwd()
                .map(|cwd| cwd.display().to_string()),
            label: terminal.manual_label.clone(),
            agent: terminal.effective_agent_label().map(str::to_string),
            title: presentation.title,
            display_agent: presentation.display_agent,
            agent_status: crate::api::schema::AgentStatus::Unknown,
            custom_status: presentation.custom_status,
            state_labels: presentation.state_labels,
            tokens: presentation.tokens,
            agent_session: None,
            scroll,
            revision: terminal.revision,
        })
    }
}
