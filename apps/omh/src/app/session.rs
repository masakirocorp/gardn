use std::time::{Duration, Instant};

use super::{App, SESSION_SAVE_DEBOUNCE};

enum SessionSaveJob {
    Clear,
    Save {
        snapshot: Box<crate::persist::SessionSnapshot>,
        history: Option<crate::persist::SessionHistorySnapshot>,
    },
}

impl App {
    pub(super) fn schedule_session_save(&mut self) {
        if !self.no_session {
            self.session_save_deadline = Some(Instant::now() + SESSION_SAVE_DEBOUNCE);
        }
    }

    pub(crate) fn sync_session_save_schedule(&mut self) {
        if self.state.session_dirty {
            self.state.session_dirty = false;
            self.schedule_session_save();
        }
    }

    fn reap_finished_session_save(&mut self) {
        if self
            .session_save_thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            if let Some(thread) = self.session_save_thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn capture_session_save_job(&self) -> SessionSaveJob {
        let default_view = self.default_client_view.clone_reconciled(&self.state);
        let has_only_default_group = self.state.groups.len() == 1
            && default_view.active_group == 0
            && self.state.groups[0].id == crate::workspace::DEFAULT_GROUP_ID
            && self.state.groups[0].name == "group 1";
        if self.state.workspaces.is_empty()
            && has_only_default_group
            && self.state.has_default_sidebar_state(&default_view)
            && self.state.remote_termination_tombstones.is_empty()
        {
            SessionSaveJob::Clear
        } else {
            let mut snapshot = crate::persist::capture(
                &self.state.groups,
                default_view.active_group,
                default_view.group_filter_enabled,
                &self.state.session_namespace_id,
                &self.state.remote_termination_tombstones,
                &self.state.workspaces,
                &self.state.terminals,
                &self.terminal_runtimes,
                default_view.active_workspace,
                default_view.selected_workspace,
                default_view.agent_panel_scope,
                self.state.sidebar_width,
                default_view.sidebar_collapsed,
                self.state.sidebar_section_split,
                self.state.right_sidebar_width,
                default_view.right_sidebar_collapsed,
                &self.state.agent_follow_up,
            );
            snapshot.default_view.ui = crate::persist::SessionUiSnapshot {
                workspace_scroll: default_view.workspace_scroll,
                agent_panel_scroll: default_view.agent_panel_scroll,
                tab_scroll: default_view.tab_scroll,
                mobile_switcher_scroll: default_view.mobile_switcher_scroll,
                activity_agents_expanded: default_view.activity_agents_expanded,
                activity_commands_expanded: default_view.activity_commands_expanded,
                activity_ports_expanded: default_view.activity_ports_expanded,
                collapsed_agent_sections: default_view.collapsed_agent_sections.clone(),
                collapsed_command_groups: default_view.collapsed_command_groups.clone(),
                collapsed_command_status_groups: default_view
                    .collapsed_command_status_groups
                    .clone(),
                collapsed_workspace_groups: default_view.collapsed_workspace_groups.clone(),
            };
            snapshot.ui = snapshot.default_view.ui.clone();
            let history = self.persist_pane_history.then(|| {
                crate::persist::capture_history(&self.state.workspaces, &self.terminal_runtimes)
            });
            SessionSaveJob::Save {
                snapshot: Box::new(snapshot),
                history,
            }
        }
    }

    pub(crate) fn start_background_session_save(&mut self) {
        if self.no_session {
            self.session_save_deadline = None;
            return;
        }

        self.reap_finished_session_save();
        if self.session_save_thread.is_some() {
            self.session_save_deadline = Some(Instant::now() + Duration::from_millis(250));
            return;
        }

        let job = self.capture_session_save_job();
        self.session_save_deadline = None;
        match std::thread::Builder::new()
            .name("omh-session-save".into())
            .spawn(move || {
                let _ = run_session_save_job(job);
            }) {
            Ok(thread) => self.session_save_thread = Some(thread),
            Err(err) => {
                tracing::warn!(err = %err, "failed to spawn session save thread; saving inline");
                let _ = run_session_save_job(self.capture_session_save_job());
            }
        }
    }

    pub(crate) fn save_session_now(&mut self) {
        let _ = self.try_save_session_now();
    }

    /// Synchronously persist the current session snapshot.
    ///
    /// Used for write-ahead durability (for example forgetting a remote
    /// termination tombstone) where a debounced background save is not enough.
    pub(crate) fn try_save_session_now(&mut self) -> std::io::Result<()> {
        if let Some(thread) = self.session_save_thread.take() {
            let _ = thread.join();
        }

        if self.no_session {
            self.session_save_deadline = None;
            return Ok(());
        }

        let result = run_session_save_job(self.capture_session_save_job());
        if result.is_ok() {
            self.session_save_deadline = None;
            self.state.session_dirty = false;
        }
        result
    }
}

fn run_session_save_job(job: SessionSaveJob) -> std::io::Result<()> {
    match job {
        SessionSaveJob::Clear => crate::persist::try_clear(),
        SessionSaveJob::Save { snapshot, history } => {
            crate::persist::try_save(&snapshot, history.as_ref())
        }
    }
}

impl super::AppState {
    fn has_default_sidebar_state(&self, default_view: &super::ClientViewState) -> bool {
        !default_view.sidebar_collapsed
            && !default_view.right_sidebar_collapsed
            && self.right_sidebar_width == 28
            && (self.sidebar_section_split - 0.5).abs() < f32::EPSILON
            && default_view.group_filter_enabled
            && self.sidebar_width == self.default_sidebar_width
    }
}
