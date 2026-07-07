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

        let default_view = self.default_client_view.clone_reconciled(&self.state);

        let has_only_default_group = self.state.groups.len() == 1
            && default_view.active_group == 0
            && self.state.groups[0].id == crate::workspace::DEFAULT_GROUP_ID
            && self.state.groups[0].name == "group 1";
        if self.state.workspaces.is_empty()
            && has_only_default_group
            && self.state.has_default_sidebar_state(&default_view)
        {
            crate::persist::clear();
        } else {
            let mut snap = crate::persist::capture(
                &self.state.groups,
                default_view.active_group,
                default_view.group_filter_enabled,
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
            );
            snap.default_view.ui = crate::persist::SessionUiSnapshot {
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
            snap.ui = snap.default_view.ui.clone();
            let history = self.persist_pane_history.then(|| {
                crate::persist::capture_history(&self.state.workspaces, &self.terminal_runtimes)
            });
            crate::persist::save(&snap, history.as_ref());
        }

        self.session_save_deadline = None;
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
