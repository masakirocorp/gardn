use std::time::Instant;

use super::{App, SESSION_SAVE_DEBOUNCE};

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

    pub(crate) fn save_session_now(&mut self) {
        if self.no_session {
            self.session_save_deadline = None;
            return;
        }

        let has_only_default_group = self.state.groups.len() == 1
            && self.state.active_group == 0
            && self.state.groups[0].id == crate::workspace::DEFAULT_GROUP_ID
            && self.state.groups[0].name == "group 1";
        if self.state.workspaces.is_empty()
            && has_only_default_group
            && self.state.has_default_sidebar_state()
        {
            crate::persist::clear();
        } else {
            let snap = crate::persist::capture(
                &self.state.groups,
                self.state.active_group,
                &self.state.workspaces,
                &self.state.terminals,
                &self.terminal_runtimes,
                self.state.active,
                self.state.selected,
                self.state.agent_panel_scope,
                self.state.sidebar_width,
                self.state.sidebar_collapsed,
                self.state.sidebar_section_split,
                self.state.right_sidebar_width,
                self.state.right_sidebar_collapsed,
            );
            let history = self.persist_pane_history.then(|| {
                crate::persist::capture_history(&self.state.workspaces, &self.terminal_runtimes)
            });
            crate::persist::save(&snap, history.as_ref());
        }

        self.session_save_deadline = None;
    }
}

impl super::AppState {
    fn has_default_sidebar_state(&self) -> bool {
        !self.sidebar_collapsed
            && !self.right_sidebar_collapsed
            && self.right_sidebar_width == 28
            && (self.sidebar_section_split - 0.5).abs() < f32::EPSILON
            && self.sidebar_width == self.default_sidebar_width
    }
}
