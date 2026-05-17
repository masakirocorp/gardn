//! Pure state mutations on AppState.
//! These don't need channels, async, or PTY runtime.

use tracing::{info, warn};

use crate::detect::{Agent, AgentState};
use crate::events::AppEvent;
use crate::layout::{find_in_direction, NavDirection, PaneId};
use crate::terminal::EffectiveStateChange;
#[cfg(test)]
use crate::workspace::GitWorkSummary;
use crate::workspace::WorkspaceGitStatus;

use super::state::{AppState, Group, Mode, ToastKind, ToastNotification, ToastTarget, ViewLayout};

fn is_background_completion_transition(prev_state: AgentState, new_state: AgentState) -> bool {
    matches!(new_state, AgentState::Idle)
        && matches!(prev_state, AgentState::Working | AgentState::Blocked)
}

pub fn active_tab_suppresses_notifications(
    is_active_tab: bool,
    outer_terminal_focus: Option<bool>,
) -> bool {
    is_active_tab && outer_terminal_focus != Some(false)
}

pub fn notification_sound_for_state_change(
    suppress_active_tab_notifications: bool,
    prev_state: AgentState,
    new_state: AgentState,
) -> Option<crate::sound::Sound> {
    if new_state == prev_state {
        return None;
    }

    match new_state {
        AgentState::Blocked => Some(crate::sound::Sound::Request),
        AgentState::Idle
            if is_background_completion_transition(prev_state, new_state)
                && !suppress_active_tab_notifications =>
        {
            Some(crate::sound::Sound::Done)
        }
        _ => None,
    }
}

pub fn notification_toast_for_state_change(
    suppress_active_tab_notifications: bool,
    prev_state: AgentState,
    new_state: AgentState,
) -> Option<ToastKind> {
    if suppress_active_tab_notifications || new_state == prev_state {
        return None;
    }

    match new_state {
        AgentState::Blocked => Some(ToastKind::NeedsAttention),
        AgentState::Idle if is_background_completion_transition(prev_state, new_state) => {
            Some(ToastKind::Finished)
        }
        _ => None,
    }
}

fn toast_agent_label(agent_label: &str) -> &str {
    agent_label
}

pub fn notification_context(
    ws: &crate::workspace::Workspace,
    ws_idx: usize,
    pane_id: PaneId,
) -> String {
    let mut context = format!("{} · {}", ws.display_name(), ws_idx + 1);
    if ws.tabs.len() > 1 {
        if let Some(tab_idx) = ws.find_tab_index_for_pane(pane_id) {
            let tab = &ws.tabs[tab_idx];
            context.push_str(&format!(" · {}", tab.display_name()));
        }
    }
    context
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneStateUpdate {
    pub pane_id: PaneId,
    pub ws_idx: usize,
    pub previous_agent_label: Option<String>,
    pub previous_known_agent: Option<Agent>,
    pub previous_state: AgentState,
    pub agent_label: Option<String>,
    pub known_agent: Option<Agent>,
    pub state: AgentState,
    pub custom_status: Option<String>,
}

// ---------------------------------------------------------------------------
// Workspace operations
// ---------------------------------------------------------------------------

impl AppState {
    pub(crate) fn command_scope_workspace_indices(&self) -> Vec<usize> {
        match self.agent_panel_scope {
            super::state::AgentPanelScope::CurrentWorkspace => {
                let idx = if matches!(self.mode, Mode::Navigate) {
                    Some(self.selected)
                } else {
                    self.active
                };
                idx.filter(|idx| self.workspaces.get(*idx).is_some())
                    .into_iter()
                    .collect()
            }
            super::state::AgentPanelScope::CurrentGroup => self
                .workspaces
                .iter()
                .enumerate()
                .filter(|(_, ws)| ws.group_id == self.active_group_id())
                .map(|(idx, _)| idx)
                .collect(),
            super::state::AgentPanelScope::AllWorkspaces => (0..self.workspaces.len()).collect(),
        }
    }

    pub(crate) fn refresh_command_catalog(&mut self) -> bool {
        let mut roots = self
            .command_scope_workspace_indices()
            .into_iter()
            .filter_map(|ws_idx| self.workspaces.get(ws_idx))
            .flat_map(|ws| {
                ws.tabs.iter().flat_map(|tab| {
                    tab.layout.pane_ids().into_iter().filter_map(|pane_id| {
                        tab.cwd_for_pane(pane_id, &self.terminals, &self.terminal_runtimes)
                    })
                })
            })
            .map(|cwd| crate::commands::project_root_from_cwd(&cwd))
            .collect::<Vec<_>>();
        roots.sort();
        roots.dedup();

        let mut catalog = roots
            .into_iter()
            .flat_map(|root| crate::commands::discover_project_commands(&root))
            .collect::<Vec<_>>();
        catalog.sort_by_key(|command| {
            (
                command.root.clone(),
                command.confidence,
                command.source,
                command.name.clone(),
            )
        });

        let changed = self.command_catalog != catalog;
        self.command_catalog = catalog;
        changed
    }

    fn group_index_for_id(&self, group_id: &str) -> Option<usize> {
        self.groups.iter().position(|group| group.id == group_id)
    }

    fn switch_to_group_index(&mut self, group_idx: usize) {
        if group_idx >= self.groups.len() {
            return;
        }

        self.active_group = group_idx;
        self.group_filter_enabled = true;
        self.apply_effective_theme();
        self.select_first_visible_workspace();
        self.mark_session_dirty();
    }

    pub fn apply_effective_theme(&mut self) {
        let Some(theme_name) = self
            .groups
            .get(self.active_group)
            .and_then(|group| group.theme_name.as_deref())
        else {
            self.palette = self.global_palette.clone();
            self.theme_name = self.global_theme_name.clone();
            return;
        };

        if let Some(palette) = self.palette_for_theme(theme_name) {
            self.palette = palette;
            self.theme_name = theme_name.to_string();
        } else {
            self.palette = self.global_palette.clone();
            self.theme_name = self.global_theme_name.clone();
        }
    }

    pub fn preview_theme(&mut self, theme_name: &str) -> bool {
        self.preview_theme_with_mode(theme_name, self.global_theme_mode)
    }

    pub fn preview_theme_with_mode(
        &mut self,
        theme_name: &str,
        mode: crate::config::ThemeMode,
    ) -> bool {
        let Some(palette) = self.palette_for_theme_mode(theme_name, mode) else {
            return false;
        };
        self.palette = palette;
        self.theme_name = theme_name.to_string();
        true
    }

    pub fn set_group_theme(&mut self, group_idx: usize, theme_name: Option<String>) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.theme_name = theme_name;
        self.mark_session_dirty();
        self.apply_effective_theme();
        true
    }

    pub fn show_all_groups(&mut self) {
        self.group_filter_enabled = false;
        self.workspace_scroll = 0;
        self.agent_panel_scroll = 0;
        if self.active.is_none() {
            self.active = self.first_visible_workspace();
            self.selected = self.active.unwrap_or(0);
        }
        self.mark_session_dirty();
        self.ensure_workspace_visible(self.selected);
    }

    pub fn toggle_group_filter(&mut self) {
        if self.group_filter_enabled {
            self.show_all_groups();
        } else {
            self.group_filter_enabled = true;
            self.select_first_visible_workspace();
            self.mark_session_dirty();
        }
    }

    fn select_first_visible_workspace(&mut self) {
        self.workspace_scroll = 0;
        self.agent_panel_scroll = 0;
        self.active = self.first_visible_workspace();
        self.selected = self.active.unwrap_or(0);
        self.tab_scroll_follow_active = true;
        if self.active.is_none() {
            self.tab_scroll = 0;
            if self.mode == Mode::Terminal {
                self.mode = Mode::Navigate;
            }
        }
        self.refresh_tab_bar_view();
    }

    pub fn switch_group(&mut self, group_idx: usize) {
        self.switch_to_group_index(group_idx);
    }

    pub fn previous_group(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        let prev = if self.active_group == 0 {
            self.groups.len() - 1
        } else {
            self.active_group - 1
        };
        self.switch_group(prev);
    }

    pub fn next_group(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        self.switch_group((self.active_group + 1) % self.groups.len());
    }

    pub fn create_group(&mut self, name: String) -> usize {
        self.create_group_with_icon(name, super::state::DEFAULT_GROUP_ICON.to_string())
    }

    pub fn create_group_with_icon(&mut self, name: String, icon: String) -> usize {
        self.groups.push(Group {
            id: super::state::generate_group_id(),
            name,
            icon: super::state::normalize_group_icon(&icon),
            theme_name: None,
        });
        self.mark_session_dirty();
        self.groups.len() - 1
    }

    pub fn rename_group(&mut self, group_idx: usize, name: String) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.name = name;
        self.mark_session_dirty();
        true
    }

    pub fn set_group_icon(&mut self, group_idx: usize, icon: String) -> bool {
        let Some(group) = self.groups.get_mut(group_idx) else {
            return false;
        };
        group.icon = super::state::normalize_group_icon(&icon);
        self.mark_session_dirty();
        true
    }

    pub fn delete_group(&mut self, group_idx: usize) -> Result<(), &'static str> {
        if self.groups.len() <= 1 {
            return Err("cannot delete the last group");
        }
        let Some(group) = self.groups.get(group_idx) else {
            return Err("group not found");
        };
        let deleted_group_id = group.id.clone();
        let active_id = self.active.map(|idx| self.workspaces[idx].id.clone());
        let selected_id = self.workspaces.get(self.selected).map(|ws| ws.id.clone());

        let deleting_active = self.active_group == group_idx;
        let terminal_ids = self
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == deleted_group_id)
            .flat_map(|workspace| {
                workspace
                    .tabs
                    .iter()
                    .flat_map(|tab| tab.panes.values())
                    .map(|pane| pane.attached_terminal_id.clone())
            })
            .collect::<Vec<_>>();
        for workspace in self
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == deleted_group_id)
        {
            crate::logging::workspace_closed(&workspace.id);
        }
        self.workspaces
            .retain(|workspace| workspace.group_id != deleted_group_id);
        self.remove_unattached_terminal_ids(terminal_ids);
        self.groups.remove(group_idx);
        if deleting_active {
            self.active_group = self.active_group.min(self.groups.len().saturating_sub(1));
        } else if self.active_group > group_idx {
            self.active_group = self.active_group.saturating_sub(1);
        }

        self.active = active_id.and_then(|id| self.workspaces.iter().position(|ws| ws.id == id));
        self.selected = selected_id
            .and_then(|id| self.workspaces.iter().position(|ws| ws.id == id))
            .or(self.active)
            .or_else(|| self.first_visible_workspace())
            .unwrap_or(0);
        if self.active.is_none() {
            self.active = self.first_visible_workspace();
        }
        if self.active.is_none() && self.mode == Mode::Terminal {
            self.mode = Mode::Navigate;
        }
        self.workspace_scroll = 0;
        self.agent_panel_scroll = 0;
        self.tab_scroll_follow_active = true;
        self.refresh_tab_bar_view();
        self.mark_session_dirty();
        Ok(())
    }

    pub fn move_workspace_to_group(&mut self, ws_idx: usize, group_idx: usize) -> bool {
        let was_active = self.active == Some(ws_idx);
        let Some(group_id) = self.groups.get(group_idx).map(|group| group.id.clone()) else {
            return false;
        };
        let Some(workspace) = self.workspaces.get_mut(ws_idx) else {
            return false;
        };
        workspace.group_id = group_id;
        self.mark_session_dirty();
        if was_active && !self.workspace_in_active_group(ws_idx) {
            self.select_first_visible_workspace();
        }
        true
    }

    pub(crate) fn pane_is_in_active_tab(&self, ws_idx: usize, pane_id: PaneId) -> bool {
        let Some(active_ws_idx) = self.active else {
            return false;
        };
        if active_ws_idx != ws_idx {
            return false;
        }
        self.workspaces[ws_idx]
            .find_tab_index_for_pane(pane_id)
            .is_some_and(|tab_idx| tab_idx == self.workspaces[ws_idx].active_tab)
    }

    pub fn switch_workspace(&mut self, idx: usize) {
        if idx < self.workspaces.len() {
            let group_id = self.workspaces[idx].group_id.clone();
            if let Some(group_idx) = self.group_index_for_id(&group_id) {
                self.active_group = group_idx;
            }
            self.active = Some(idx);
            self.selected = idx;
            let workspace_id = self.workspaces[idx].id.clone();
            crate::logging::workspace_focused(&workspace_id);
            self.mark_session_dirty();
            if matches!(
                self.agent_panel_scope,
                crate::app::state::AgentPanelScope::CurrentWorkspace
            ) {
                self.agent_panel_scroll = 0;
            }
            self.ensure_workspace_visible(idx);
            if let Some(ws) = self.workspaces.get_mut(idx) {
                let active_tab = ws.active_tab;
                ws.switch_tab(active_tab);
                let tab_id = format!("{}:{}", workspace_id, active_tab + 1);
                crate::logging::tab_focused(&workspace_id, &tab_id);
            }
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
        }
    }

    pub(crate) fn ensure_workspace_visible(&mut self, idx: usize) {
        if idx >= self.workspaces.len() {
            return;
        }

        if self.view.layout == ViewLayout::Mobile && self.mode == Mode::Navigate {
            self.ensure_mobile_workspace_visible(idx);
            return;
        }

        if self.sidebar_collapsed {
            return;
        }

        let Some(target_pos) = crate::ui::workspace_list_position_for_workspace(self, idx) else {
            return;
        };

        let workspace_area = if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
            crate::ui::left_sidebar_workspace_rect(self.view.sidebar_rect)
        } else {
            self.view.sidebar_rect
        };
        let mut cards = if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
            crate::ui::compute_workspace_card_areas_in_list(self, workspace_area)
        } else {
            crate::ui::compute_workspace_card_areas(self, workspace_area)
        };
        if cards.is_empty() {
            self.workspace_scroll = target_pos;
            return;
        }

        let first_pos = cards
            .first()
            .and_then(|card| crate::ui::workspace_list_position_for_workspace(self, card.ws_idx))
            .unwrap_or(0);
        if target_pos < first_pos {
            self.workspace_scroll = target_pos;
            return;
        }

        while cards
            .last()
            .and_then(|card| crate::ui::workspace_list_position_for_workspace(self, card.ws_idx))
            .unwrap_or(target_pos)
            < target_pos
        {
            let previous_scroll = self.workspace_scroll;
            self.workspace_scroll = self.workspace_scroll.saturating_add(1);
            if self.workspace_scroll == previous_scroll {
                break;
            }
            cards = if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
                crate::ui::compute_workspace_card_areas_in_list(self, workspace_area)
            } else {
                crate::ui::compute_workspace_card_areas(self, workspace_area)
            };
            if cards.is_empty() {
                break;
            }
        }
    }

    fn ensure_mobile_workspace_visible(&mut self, idx: usize) {
        let viewport = crate::ui::mobile_switcher_areas(self).viewport;
        if viewport.height == 0 {
            return;
        }

        let visible = self.visible_workspace_indices();
        let Some(visible_idx) = visible.iter().position(|ws_idx| *ws_idx == idx) else {
            return;
        };
        let row_range = crate::ui::mobile_switcher_workspace_doc_range(visible_idx);
        let visible_start = self.mobile_switcher_scroll;
        let visible_end = visible_start.saturating_add(viewport.height as usize);
        if row_range.start < visible_start {
            self.mobile_switcher_scroll = row_range.start;
        } else if row_range.end > visible_end {
            self.mobile_switcher_scroll = row_range.end.saturating_sub(viewport.height as usize);
        }
        self.mobile_switcher_scroll = self
            .mobile_switcher_scroll
            .min(crate::ui::mobile_switcher_max_scroll(self));
    }

    pub fn switch_tab(&mut self, idx: usize) {
        if let Some(ws_idx) = self.active {
            let Some(ws) = self.workspaces.get_mut(ws_idx) else {
                return;
            };
            ws.switch_tab(idx);
            let workspace_id = ws.id.clone();
            let tab_id = format!("{}:{}", workspace_id, idx + 1);
            crate::logging::tab_focused(&workspace_id, &tab_id);
            self.mark_session_dirty();
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
        }
    }

    pub(crate) fn mark_active_tab_seen(&mut self) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        let Some(tab) = self
            .workspaces
            .get_mut(ws_idx)
            .and_then(crate::workspace::Workspace::active_tab_mut)
        else {
            return false;
        };

        let mut changed = false;
        for pane in tab.panes.values_mut() {
            if !pane.seen {
                pane.seen = true;
                changed = true;
            }
        }
        changed
    }

    pub fn next_workspace(&mut self) {
        let visible = self.visible_workspace_indices();
        if !visible.is_empty() {
            let current = self.active.unwrap_or(self.selected);
            let current_pos = visible.iter().position(|idx| *idx == current).unwrap_or(0);
            let next = visible[(current_pos + 1) % visible.len()];
            self.switch_workspace(next);
        }
    }

    pub fn previous_workspace(&mut self) {
        let visible = self.visible_workspace_indices();
        if !visible.is_empty() {
            let current = self.active.unwrap_or(self.selected);
            let current_pos = visible.iter().position(|idx| *idx == current).unwrap_or(0);
            let prev = if current_pos == 0 {
                visible[visible.len() - 1]
            } else {
                visible[current_pos - 1]
            };
            self.switch_workspace(prev);
        }
    }

    pub fn move_workspace(&mut self, source_idx: usize, insert_idx: usize) {
        if source_idx >= self.workspaces.len() || insert_idx > self.workspaces.len() {
            return;
        }

        self.mark_session_dirty();

        let active_id = self.active.map(|idx| self.workspaces[idx].id.clone());
        let selected_id = self
            .workspaces
            .get(self.selected)
            .map(|workspace| workspace.id.clone());

        let workspace = self.workspaces.remove(source_idx);
        let target_idx = if source_idx < insert_idx {
            insert_idx.saturating_sub(1)
        } else {
            insert_idx
        }
        .min(self.workspaces.len());
        self.workspaces.insert(target_idx, workspace);

        self.active = active_id.and_then(|id| self.workspaces.iter().position(|ws| ws.id == id));
        self.selected = selected_id
            .and_then(|id| self.workspaces.iter().position(|ws| ws.id == id))
            .unwrap_or(0);
        self.ensure_workspace_visible(self.selected);
    }

    pub fn scroll_tabs_left(&mut self) {
        self.tab_scroll_follow_active = false;
        self.tab_scroll = self.tab_scroll.saturating_sub(1);
        self.refresh_tab_bar_view();
    }

    pub fn scroll_tabs_right(&mut self) {
        self.tab_scroll_follow_active = false;
        self.tab_scroll = self.tab_scroll.saturating_add(1);
        self.refresh_tab_bar_view();
    }

    pub fn move_tab(&mut self, source_idx: usize, insert_idx: usize) {
        if let Some(ws) = self.active.and_then(|i| self.workspaces.get_mut(i)) {
            if ws.move_tab(source_idx, insert_idx) {
                self.mark_session_dirty();
                self.tab_scroll_follow_active = true;
                self.refresh_tab_bar_view();
            }
        }
    }

    pub fn next_tab(&mut self) {
        if let Some(ws) = self.active.and_then(|i| self.workspaces.get(i)) {
            if !ws.tabs.is_empty() {
                let next = (ws.active_tab + 1) % ws.tabs.len();
                self.switch_tab(next);
            }
        }
    }

    pub fn previous_tab(&mut self) {
        if let Some(ws) = self.active.and_then(|i| self.workspaces.get(i)) {
            if !ws.tabs.is_empty() {
                let prev = if ws.active_tab == 0 {
                    ws.tabs.len() - 1
                } else {
                    ws.active_tab - 1
                };
                self.switch_tab(prev);
            }
        }
    }

    pub fn next_agent(&mut self) {
        self.cycle_agent_entry(true);
    }

    pub fn previous_agent(&mut self) {
        self.cycle_agent_entry(false);
    }

    pub fn focus_agent_entry(&mut self, idx: usize) -> bool {
        let entries = crate::ui::agent_panel_entries(self);
        let Some(target) = entries.get(idx) else {
            return false;
        };
        let ws_idx = target.ws_idx;
        let tab_idx = target.tab_idx;
        let pane_id = target.pane_id;

        self.switch_workspace(ws_idx);
        self.switch_tab(tab_idx);
        if let Some(tab) = self
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.tabs.get_mut(tab_idx))
        {
            if tab.panes.contains_key(&pane_id) {
                tab.layout.focus_pane(pane_id);
                self.mark_session_dirty();
                self.ensure_agent_panel_entry_visible(idx);
                return true;
            }
        }
        false
    }

    fn cycle_agent_entry(&mut self, forward: bool) {
        let entries = crate::ui::agent_panel_entries(self);
        if entries.is_empty() {
            return;
        }

        let focused = self
            .active
            .and_then(|idx| self.workspaces.get(idx))
            .and_then(crate::workspace::Workspace::focused_pane_id);
        let current_idx =
            focused.and_then(|pane_id| entries.iter().position(|entry| entry.pane_id == pane_id));
        let target_idx = match (current_idx, forward) {
            (Some(idx), true) => (idx + 1) % entries.len(),
            (Some(0), false) => entries.len() - 1,
            (Some(idx), false) => idx - 1,
            (None, true) => 0,
            (None, false) => entries.len() - 1,
        };

        self.focus_agent_entry(target_idx);
    }

    fn ensure_agent_panel_entry_visible(&mut self, idx: usize) {
        if self.sidebar_collapsed {
            return;
        }

        let (detail_area, leading_separator) =
            if self.view.right_sidebar_rect != ratatui::layout::Rect::default() {
                if self.right_sidebar_collapsed {
                    return;
                }
                (
                    crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect),
                    false,
                )
            } else {
                let (_, detail_area) = crate::ui::expanded_sidebar_sections(
                    self.view.sidebar_rect,
                    self.sidebar_section_split,
                );
                (detail_area, true)
            };
        let metrics = crate::ui::agent_panel_scroll_metrics(self, detail_area, leading_separator);
        let visible = metrics.viewport_rows;
        if visible == 0 {
            return;
        }

        if idx < self.agent_panel_scroll {
            self.agent_panel_scroll = idx;
        } else if idx >= self.agent_panel_scroll.saturating_add(visible) {
            self.agent_panel_scroll = idx.saturating_add(1).saturating_sub(visible);
        }

        let max_scroll =
            crate::ui::agent_panel_scroll_metrics(self, detail_area, leading_separator)
                .max_offset_from_bottom;
        self.agent_panel_scroll = self.agent_panel_scroll.min(max_scroll);
    }

    pub(crate) fn terminal_ids_for_workspace(
        &self,
        ws_idx: usize,
    ) -> Vec<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)
            .into_iter()
            .flat_map(|ws| &ws.tabs)
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect()
    }

    pub(crate) fn terminal_ids_for_tab(
        &self,
        ws_idx: usize,
        tab_idx: usize,
    ) -> Vec<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)
            .and_then(|ws| ws.tabs.get(tab_idx))
            .into_iter()
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect()
    }

    pub(crate) fn terminal_id_for_pane(
        &self,
        ws_idx: usize,
        pane_id: PaneId,
    ) -> Option<crate::terminal::TerminalId> {
        self.workspaces
            .get(ws_idx)?
            .pane_state(pane_id)
            .map(|pane| pane.attached_terminal_id.clone())
    }

    pub(crate) fn remove_unattached_terminal_ids(
        &mut self,
        terminal_ids: impl IntoIterator<Item = crate::terminal::TerminalId>,
    ) {
        for terminal_id in terminal_ids {
            let still_attached = self.workspaces.iter().any(|ws| {
                ws.tabs.iter().any(|tab| {
                    tab.panes
                        .values()
                        .any(|pane| pane.attached_terminal_id == terminal_id)
                })
            });
            if !still_attached {
                self.terminals.remove(&terminal_id);
                if let Some(runtime) = self.terminal_runtimes.remove(&terminal_id) {
                    runtime.shutdown();
                }
            }
        }
    }

    pub fn close_selected_workspace(&mut self) {
        if self.workspaces.is_empty() {
            return;
        }
        self.mark_session_dirty();
        let closed_idx = self.selected;
        let terminal_ids = self.terminal_ids_for_workspace(self.selected);
        let workspace_id = self.workspaces[self.selected].id.clone();
        crate::logging::workspace_closed(&workspace_id);
        self.workspaces.remove(self.selected);
        self.remove_unattached_terminal_ids(terminal_ids);
        if self.workspaces.is_empty() {
            self.active = None;
            self.selected = 0;
            self.workspace_scroll = 0;
            self.tab_scroll = 0;
            self.tab_scroll_follow_active = true;
        } else {
            let visible = self.visible_workspace_indices();
            let target = visible
                .iter()
                .copied()
                .find(|idx| *idx >= closed_idx)
                .or_else(|| visible.last().copied());
            self.active = target;
            self.selected = target.unwrap_or(0);
            if self.active.is_none() && self.mode == Mode::Terminal {
                self.mode = Mode::Navigate;
            }
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
            self.ensure_workspace_visible(self.selected);
        }
    }

    fn refresh_tab_bar_view(&mut self) {
        let area = self.view.tab_bar_rect;
        let Some(ws) = self.active.and_then(|idx| self.workspaces.get(idx)) else {
            self.tab_scroll = 0;
            self.view.tab_hit_areas.clear();
            self.view.tab_scroll_left_hit_area = ratatui::layout::Rect::default();
            self.view.tab_scroll_right_hit_area = ratatui::layout::Rect::default();
            self.view.new_tab_hit_area = ratatui::layout::Rect::default();
            return;
        };

        let layout = crate::ui::compute_tab_bar_view(
            ws,
            area,
            self.tab_scroll,
            self.tab_scroll_follow_active,
            self.mouse_capture,
        );
        self.tab_scroll = layout.scroll;
        self.view.tab_hit_areas = layout.tab_hit_areas;
        self.view.tab_scroll_left_hit_area = layout.scroll_left_hit_area;
        self.view.tab_scroll_right_hit_area = layout.scroll_right_hit_area;
        self.view.new_tab_hit_area = layout.new_tab_hit_area;
    }
}

// ---------------------------------------------------------------------------
// Pane operations
// ---------------------------------------------------------------------------

impl AppState {
    pub fn navigate_pane(&mut self, direction: NavDirection) {
        let panes = &self.view.pane_infos;
        if let Some(focused) = panes.iter().find(|p| p.is_focused) {
            if let Some(target) = find_in_direction(focused, direction, panes) {
                if let Some(tab) = self
                    .active
                    .and_then(|i| self.workspaces.get_mut(i))
                    .and_then(|ws| ws.active_tab_mut())
                {
                    tab.layout.focus_pane(target);
                    self.mark_session_dirty();
                }
            }
        }
    }

    pub fn resize_pane(&mut self, direction: NavDirection) {
        if let Some(first) = self.view.pane_infos.first() {
            let area = self
                .view
                .pane_infos
                .iter()
                .fold(first.rect, |acc, p| acc.union(p.rect));
            if let Some(tab) = self
                .active
                .and_then(|i| self.workspaces.get_mut(i))
                .and_then(|ws| ws.active_tab_mut())
            {
                tab.layout.resize_focused(direction, 0.05, area);
                self.mark_session_dirty();
            }
        }
    }

    pub fn cycle_pane(&mut self, reverse: bool) {
        if let Some(tab) = self
            .active
            .and_then(|i| self.workspaces.get_mut(i))
            .and_then(|ws| ws.active_tab_mut())
        {
            if reverse {
                tab.layout.focus_prev();
            } else {
                tab.layout.focus_next();
            }
            self.mark_session_dirty();
        }
    }

    pub fn toggle_zoom(&mut self) {
        if let Some(tab) = self
            .active
            .and_then(|i| self.workspaces.get_mut(i))
            .and_then(|ws| ws.active_tab_mut())
        {
            if tab.layout.pane_count() > 1 {
                tab.zoomed = !tab.zoomed;
                self.mark_session_dirty();
            }
        }
    }

    pub fn close_pane(&mut self) {
        self.mark_session_dirty();
        let active = self.active;
        let terminal_ids = active
            .and_then(|i| {
                self.workspaces
                    .get(i)
                    .and_then(|ws| ws.focused_pane_id().map(|pane_id| (i, pane_id)))
            })
            .and_then(|(i, pane_id)| self.terminal_id_for_pane(i, pane_id))
            .into_iter()
            .collect::<Vec<_>>();
        let should_close_workspace = active
            .and_then(|i| self.workspaces.get_mut(i))
            .is_some_and(|ws| ws.close_focused());
        if should_close_workspace {
            if let Some(active) = active {
                self.selected = active;
            }
            self.close_selected_workspace();
        } else {
            self.remove_unattached_terminal_ids(terminal_ids);
        }
    }

    pub fn close_tab(&mut self) {
        let Some(ws_idx) = self.active else {
            return;
        };
        self.mark_session_dirty();
        let Some(ws) = self.workspaces.get(ws_idx) else {
            return;
        };
        let should_close_workspace = ws.tabs.len() <= 1;
        if should_close_workspace {
            self.selected = ws_idx;
            if self.confirm_close {
                self.mode = Mode::ConfirmClose;
                return;
            }
            self.close_selected_workspace();
            return;
        }
        if let Some(ws_idx) = self.active {
            let terminal_ids = self
                .workspaces
                .get(ws_idx)
                .map(|ws| self.terminal_ids_for_tab(ws_idx, ws.active_tab))
                .unwrap_or_default();
            let Some(ws) = self.workspaces.get_mut(ws_idx) else {
                return;
            };
            let workspace_id = ws.id.clone();
            let closing_tab_id = format!("{}:{}", workspace_id, ws.active_tab + 1);
            ws.close_active_tab();
            self.remove_unattached_terminal_ids(terminal_ids);
            crate::logging::tab_closed(&workspace_id, &closing_tab_id);
            self.tab_scroll_follow_active = true;
            self.refresh_tab_bar_view();
        }
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

impl AppState {
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub fn copy_selection(&mut self) {
        let mut sel = match self.selection.take() {
            Some(sel) => sel,
            None => return,
        };
        if !sel.finish() {
            return;
        }

        let ws_idx = match self.active {
            Some(ws_idx) if self.workspaces.get(ws_idx).is_some() => ws_idx,
            _ => return,
        };

        let text = self
            .runtime_for_pane_in_workspace(ws_idx, sel.pane_id)
            .and_then(|rt| rt.extract_selection(&sel));

        if let Some(text) = text {
            if !text.is_empty() {
                self.request_clipboard_write = Some(text.into_bytes());
                info!("copied selection to clipboard");
            }
        }

        self.selection = None;
    }
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

impl AppState {
    pub fn apply_workspace_git_statuses(&mut self, results: Vec<WorkspaceGitStatus>) -> bool {
        let mut changed = false;
        for result in results {
            let Some(ws_idx) = self
                .workspaces
                .iter()
                .position(|ws| ws.id == result.workspace_id)
            else {
                continue;
            };

            if self.workspaces[ws_idx]
                .resolved_identity_cwd_from(&self.terminals, &self.terminal_runtimes)
                .as_ref()
                != Some(&result.resolved_identity_cwd)
            {
                continue;
            }
            if self.workspaces[ws_idx]
                .git_status_cwds_from(&self.terminals, &self.terminal_runtimes)
                != result.cwd_fingerprint
            {
                continue;
            }

            let ws = &mut self.workspaces[ws_idx];
            if ws.cached_git_branch != result.branch {
                ws.cached_git_branch = result.branch;
                changed = true;
            }
            if ws.cached_git_ahead_behind != result.ahead_behind {
                ws.cached_git_ahead_behind = result.ahead_behind;
                changed = true;
            }
            if ws.cached_git_work_summary != result.work_summary {
                ws.cached_git_work_summary = result.work_summary;
                changed = true;
            }
        }
        changed
    }

    pub fn handle_app_event(&mut self, event: AppEvent) -> Vec<PaneStateUpdate> {
        match event {
            AppEvent::PaneDied { pane_id } => {
                self.handle_pane_died(pane_id);
                Vec::new()
            }
            AppEvent::UpdateReady {
                version,
                install_command,
            } => {
                self.update_available = Some(version.clone());
                self.update_install_command = install_command.clone();
                self.latest_release_notes_available = true;
                self.update_dismissed = true;
                if matches!(
                    self.toast_config.delivery,
                    crate::config::ToastDelivery::Herdr
                ) {
                    self.toast = Some(ToastNotification {
                        kind: ToastKind::UpdateInstalled,
                        title: format!("v{version} available"),
                        context: format!("detach, then run `{install_command}`"),
                        target: None,
                    });
                }
                Vec::new()
            }
            AppEvent::StateChanged {
                pane_id,
                agent,
                state,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.set_detected_state(agent, state)
                })
                .into_iter()
                .collect(),
            AppEvent::HookStateReported {
                pane_id,
                source,
                agent_label,
                state,
                message,
                custom_status,
                seq,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.set_hook_authority_with_custom_status(
                        source,
                        agent_label,
                        state,
                        message,
                        custom_status,
                        seq,
                    )
                })
                .into_iter()
                .collect(),
            AppEvent::HookAuthorityCleared {
                pane_id,
                source,
                seq,
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.clear_hook_authority(source.as_deref(), seq)
                })
                .into_iter()
                .collect(),
            AppEvent::HookAgentReleased {
                pane_id,
                source,
                agent_label,
                seq,
                ..
            } => self
                .update_terminal_state(pane_id, |terminal| {
                    terminal.release_agent(&source, &agent_label, seq)
                })
                .into_iter()
                .collect(),
            // Intercepted in App::handle_internal_event before reaching this
            // dispatch; never touches AppState.
            AppEvent::ClipboardWrite { .. } => Vec::new(),
            AppEvent::GitStatusRefreshed { results } => {
                self.apply_workspace_git_statuses(results);
                Vec::new()
            }
        }
    }

    fn update_terminal_state<F>(&mut self, pane_id: PaneId, update: F) -> Option<PaneStateUpdate>
    where
        F: FnOnce(&mut crate::terminal::TerminalState) -> Option<EffectiveStateChange>,
    {
        let ws_idx = self
            .workspaces
            .iter()
            .position(|ws| ws.pane_state(pane_id).is_some())?;
        let terminal_id = self.workspaces[ws_idx]
            .pane_state(pane_id)?
            .attached_terminal_id
            .clone();
        let change = {
            let terminal = self.terminals.get_mut(&terminal_id)?;
            update(terminal)?
        };
        let update = PaneStateUpdate {
            pane_id,
            ws_idx,
            previous_agent_label: change.previous_agent_label.clone(),
            previous_known_agent: change.previous_known_agent,
            previous_state: change.previous_state,
            agent_label: change.agent_label.clone(),
            known_agent: change.known_agent,
            state: change.state,
            custom_status: change.custom_status.clone(),
        };
        self.apply_pane_state_change(ws_idx, pane_id, &change);
        Some(update)
    }

    fn apply_pane_state_change(
        &mut self,
        ws_idx: usize,
        pane_id: PaneId,
        change: &EffectiveStateChange,
    ) {
        let is_active_tab = self.pane_is_in_active_tab(ws_idx, pane_id);
        let suppress_active_tab_notifications =
            active_tab_suppresses_notifications(is_active_tab, self.outer_terminal_focus);
        let Some(pane) = self.workspaces[ws_idx]
            .tabs
            .iter_mut()
            .find_map(|tab| tab.panes.get_mut(&pane_id))
        else {
            return;
        };

        if change.state != AgentState::Idle {
            pane.seen = true;
        } else if is_background_completion_transition(change.previous_state, change.state) {
            pane.seen = suppress_active_tab_notifications;
        }

        if self.local_sound_playback && self.sound.allows(change.known_agent) {
            if let Some(sound) = notification_sound_for_state_change(
                suppress_active_tab_notifications,
                change.previous_state,
                change.state,
            ) {
                crate::sound::play(sound, &self.sound);
            }
        }

        if matches!(
            self.toast_config.delivery,
            crate::config::ToastDelivery::Herdr
        ) {
            if let (Some(agent_label), Some(kind)) = (
                change.agent_label.as_deref(),
                notification_toast_for_state_change(
                    is_active_tab,
                    change.previous_state,
                    change.state,
                ),
            ) {
                let event_text = match kind {
                    ToastKind::NeedsAttention => "needs attention",
                    ToastKind::Finished => "finished",
                    ToastKind::UpdateInstalled => "updated",
                };
                let context = notification_context(&self.workspaces[ws_idx], ws_idx, pane_id);
                self.toast = Some(ToastNotification {
                    kind,
                    title: format!("{} {}", toast_agent_label(agent_label), event_text),
                    context,
                    target: Some(ToastTarget {
                        workspace_id: self.workspaces[ws_idx].id.clone(),
                        pane_id,
                    }),
                });
            }
        }
    }

    fn handle_pane_died(&mut self, pane_id: PaneId) {
        let ws_idx = self
            .workspaces
            .iter()
            .position(|ws| ws.find_tab_index_for_pane(pane_id).is_some());

        let Some(ws_idx) = ws_idx else {
            warn!(pane = pane_id.raw(), "PaneDied for unknown pane");
            return;
        };

        let pane_terminal_id = self.terminal_id_for_pane(ws_idx, pane_id);
        let workspace_terminal_ids = self.terminal_ids_for_workspace(ws_idx);
        let should_close_workspace = {
            let ws = &mut self.workspaces[ws_idx];
            ws.remove_pane(pane_id)
        };
        self.mark_session_dirty();

        if should_close_workspace {
            self.workspaces.remove(ws_idx);
            self.remove_unattached_terminal_ids(workspace_terminal_ids);
            if self.workspaces.is_empty() {
                self.active = None;
                self.selected = 0;
                if self.mode == Mode::Terminal {
                    self.mode = Mode::Navigate;
                }
            } else {
                if let Some(active) = self.active {
                    if active >= self.workspaces.len() {
                        self.active = Some(self.workspaces.len() - 1);
                    }
                }
                if self.selected >= self.workspaces.len() {
                    self.selected = self.workspaces.len() - 1;
                }
            }
        } else {
            self.remove_unattached_terminal_ids(pane_terminal_id);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::state::Palette;
    use crate::config::ThemeMode;
    use crate::detect::{Agent, AgentState};
    use crate::terminal_theme::{DefaultColorKind, RgbColor, TerminalTheme};
    use crate::workspace::Workspace;
    use ratatui::layout::Direction;

    fn app_with_workspaces(names: &[&str]) -> AppState {
        let mut state = AppState::test_new();
        for name in names {
            let ws = Workspace::test_new(name);
            state.workspaces.push(ws);
        }
        state.ensure_test_terminals();
        if !state.workspaces.is_empty() {
            state.active = Some(0);
            state.mode = Mode::Terminal;
        }
        state
    }

    fn temp_project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "herdr-app-commands-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn command_catalog_refresh_uses_pane_cwd_project_roots_in_scope() {
        let project = temp_project("scope");
        std::fs::write(
            project.join("package.json"),
            r#"{"scripts":{"dev":"vite"}}"#,
        )
        .unwrap();
        let nested = project.join("apps/web");
        std::fs::create_dir_all(&nested).unwrap();
        let mut state = app_with_workspaces(&["web"]);
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, root_pane).unwrap();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = nested;

        assert!(state.refresh_command_catalog());

        assert_eq!(state.command_catalog.len(), 1);
        assert_eq!(state.command_catalog[0].name, "dev");
        assert_eq!(state.command_catalog[0].root, project);
    }

    #[test]
    fn visible_workspace_indices_only_include_active_group() {
        let mut state = app_with_workspaces(&["one", "two", "three"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);

        assert_eq!(state.visible_workspace_indices(), vec![0, 2]);

        state.switch_group(side_group);

        assert_eq!(state.visible_workspace_indices(), vec![1]);
        assert_eq!(state.active, Some(1));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn switching_group_applies_group_theme_override() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.set_group_theme(side_group, Some("nord".to_string()));
        state.switch_group(0);

        assert_eq!(state.theme_name, state.global_theme_name);

        state.switch_group(side_group);

        assert_eq!(state.theme_name, "nord");
        assert_eq!(state.palette.accent, Palette::nord().accent);
    }

    #[test]
    fn group_theme_override_inherits_global_light_mode() {
        let mut state = app_with_workspaces(&["one", "two"]);
        state.global_theme_mode = ThemeMode::Light;
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.set_group_theme(side_group, Some("gruvbox".to_string()));

        state.switch_group(side_group);

        assert_eq!(state.theme_name, "gruvbox");
        assert_eq!(state.palette.panel_bg, Palette::gruvbox_light().panel_bg);
    }

    #[test]
    fn system_theme_mode_uses_terminal_background() {
        let mut state = app_with_workspaces(&["one"]);
        state.global_theme_name = "gruvbox".to_string();
        state.global_theme_mode = ThemeMode::System;
        state.host_terminal_theme = TerminalTheme::default().with_color(
            DefaultColorKind::Background,
            RgbColor {
                r: 245,
                g: 245,
                b: 245,
            },
        );

        state.refresh_global_palette();
        state.apply_effective_theme();

        assert_eq!(state.theme_name, "gruvbox");
        assert_eq!(state.palette.panel_bg, Palette::gruvbox_light().panel_bg);
    }

    #[test]
    fn clearing_group_theme_follows_global_theme() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.set_group_theme(side_group, Some("nord".to_string()));
        state.switch_group(side_group);

        state.global_palette = Palette::dracula();
        state.global_theme_name = "dracula".to_string();
        state.set_group_theme(side_group, None);

        assert_eq!(state.theme_name, "dracula");
        assert_eq!(state.palette.accent, Palette::dracula().accent);
    }

    #[test]
    fn workspace_navigation_stays_inside_active_group() {
        let mut state = app_with_workspaces(&["one", "two", "three"]);
        let side_group = state.create_group("Side".to_string());
        state.move_workspace_to_group(1, side_group);
        state.switch_workspace(0);

        state.next_workspace();
        assert_eq!(state.active, Some(2));

        state.next_workspace();
        assert_eq!(state.active, Some(0));

        state.previous_workspace();
        assert_eq!(state.active, Some(2));
    }

    #[test]
    fn apply_workspace_git_statuses_updates_matching_workspace() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let first_id = state.workspaces[0].id.clone();
        let first_cwd = state.workspaces[0].resolved_identity_cwd().unwrap();
        let first_cwd_fingerprint = state.workspaces[0].git_status_cwds();
        let second_id = state.workspaces[1].id.clone();

        let changed = state.apply_workspace_git_statuses(vec![WorkspaceGitStatus {
            workspace_id: first_id,
            resolved_identity_cwd: first_cwd,
            cwd_fingerprint: first_cwd_fingerprint,
            branch: Some("main".into()),
            ahead_behind: Some((2, 1)),
            work_summary: Some(GitWorkSummary {
                repo_count: 1,
                modified: 2,
                ..GitWorkSummary::default()
            }),
        }]);

        assert!(changed);
        assert_eq!(state.workspaces[0].branch().as_deref(), Some("main"));
        assert_eq!(state.workspaces[0].git_ahead_behind(), Some((2, 1)));
        assert_eq!(state.workspaces[0].git_work_summary_label(), "~2");
        assert_eq!(state.workspaces[1].id, second_id);
        assert_eq!(state.workspaces[1].git_ahead_behind(), None);
    }

    #[test]
    fn apply_workspace_git_statuses_ignores_stale_cwd() {
        let mut state = app_with_workspaces(&["one"]);
        let workspace_id = state.workspaces[0].id.clone();
        state.workspaces[0].cached_git_branch = Some("old".into());
        state.workspaces[0].cached_git_ahead_behind = Some((1, 0));

        let changed = state.apply_workspace_git_statuses(vec![WorkspaceGitStatus {
            workspace_id,
            resolved_identity_cwd: std::path::PathBuf::from("/definitely/not/current"),
            cwd_fingerprint: state.workspaces[0].git_status_cwds(),
            branch: Some("main".into()),
            ahead_behind: Some((0, 1)),
            work_summary: Some(GitWorkSummary {
                repo_count: 1,
                added: 1,
                ..GitWorkSummary::default()
            }),
        }]);

        assert!(!changed);
        assert_eq!(state.workspaces[0].branch().as_deref(), Some("old"));
        assert_eq!(state.workspaces[0].git_ahead_behind(), Some((1, 0)));
    }

    #[test]
    fn apply_workspace_git_statuses_clears_missing_git_status() {
        let mut state = app_with_workspaces(&["one"]);
        let workspace_id = state.workspaces[0].id.clone();
        let cwd = state.workspaces[0].resolved_identity_cwd().unwrap();
        let cwd_fingerprint = state.workspaces[0].git_status_cwds();
        state.workspaces[0].cached_git_branch = Some("main".into());
        state.workspaces[0].cached_git_ahead_behind = Some((1, 2));
        state.workspaces[0].cached_git_work_summary = Some(GitWorkSummary {
            repo_count: 1,
            modified: 1,
            ..GitWorkSummary::default()
        });

        let changed = state.apply_workspace_git_statuses(vec![WorkspaceGitStatus {
            workspace_id,
            resolved_identity_cwd: cwd,
            cwd_fingerprint,
            branch: None,
            ahead_behind: None,
            work_summary: None,
        }]);

        assert!(changed);
        assert_eq!(state.workspaces[0].branch(), None);
        assert_eq!(state.workspaces[0].git_ahead_behind(), None);
        assert_eq!(state.workspaces[0].git_work_summary_label(), "shell");
    }

    #[test]
    fn update_ready_sets_explicit_upgrade_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;

        let updates = state.handle_app_event(crate::events::AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install_command: "herdr update".into(),
        });

        assert!(updates.is_empty());
        assert_eq!(state.update_available.as_deref(), Some("0.5.0"));
        assert!(state.latest_release_notes_available);
        let toast = state.toast.as_ref().expect("update toast");
        assert_eq!(toast.title, "v0.5.0 available");
        assert_eq!(toast.context, "detach, then run `herdr update`");
    }

    fn mark_agent(state: &mut AppState, ws_idx: usize, tab_idx: usize, pane_id: PaneId) {
        state.ensure_test_terminals();
        let terminal_id = state.workspaces[ws_idx].tabs[tab_idx]
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        if let Some(terminal) = state.terminals.get_mut(&terminal_id) {
            terminal.set_detected_state(Some(Agent::Pi), AgentState::Idle);
        }
    }

    #[test]
    fn next_agent_cycles_agent_panel_entries_in_all_scope() {
        let mut first = Workspace::test_new("one");
        let first_root = first.tabs[0].root_pane;
        let first_second = first.test_split(Direction::Horizontal);
        first.tabs[0].layout.focus_pane(first_root);
        let second = Workspace::test_new("two");
        let second_root = second.tabs[0].root_pane;

        let mut state = AppState::test_new();
        state.workspaces = vec![first, second];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        mark_agent(&mut state, 0, 0, first_root);
        mark_agent(&mut state, 0, 0, first_second);
        mark_agent(&mut state, 1, 0, second_root);

        state.next_agent();
        assert_eq!(state.active, Some(0));
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_second));

        state.next_agent();
        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].focused_pane_id(), Some(second_root));

        state.previous_agent();
        assert_eq!(state.active, Some(0));
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_second));
    }

    #[test]
    fn focus_agent_entry_uses_agent_panel_order() {
        let mut first = Workspace::test_new("one");
        let first_root = first.tabs[0].root_pane;
        let first_second = first.test_split(Direction::Horizontal);
        first.tabs[0].layout.focus_pane(first_root);
        let second = Workspace::test_new("two");
        let second_root = second.tabs[0].root_pane;

        let mut state = AppState::test_new();
        state.workspaces = vec![first, second];
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::AllWorkspaces;
        mark_agent(&mut state, 0, 0, first_root);
        mark_agent(&mut state, 0, 0, first_second);
        mark_agent(&mut state, 1, 0, second_root);

        assert!(state.focus_agent_entry(2));

        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].focused_pane_id(), Some(second_root));
    }

    #[test]
    fn next_agent_cycles_only_current_scope_entries() {
        let mut first = Workspace::test_new("one");
        let first_root = first.tabs[0].root_pane;
        let first_second = first.test_split(Direction::Horizontal);
        first.tabs[0].layout.focus_pane(first_second);
        let second = Workspace::test_new("two");
        let second_root = second.tabs[0].root_pane;

        let mut state = AppState::test_new();
        state.workspaces = vec![first, second];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::CurrentWorkspace;
        mark_agent(&mut state, 0, 0, first_root);
        mark_agent(&mut state, 0, 0, first_second);
        mark_agent(&mut state, 1, 0, second_root);

        state.next_agent();

        assert_eq!(state.active, Some(0));
        assert_eq!(state.workspaces[0].focused_pane_id(), Some(first_root));
    }

    #[test]
    fn previous_agent_keeps_wrapped_target_visible_in_agent_panel() {
        let mut workspace = Workspace::test_new("one");
        let root = workspace.tabs[0].root_pane;
        for idx in 1..8 {
            workspace.test_add_tab(Some(&format!("tab-{idx}")));
        }

        let mut state = AppState::test_new();
        state.workspaces = vec![workspace];
        state.ensure_test_terminals();
        state.active = Some(0);
        state.selected = 0;
        state.mode = Mode::Terminal;
        state.agent_panel_scope = crate::app::state::AgentPanelScope::CurrentWorkspace;
        for tab_idx in 0..state.workspaces[0].tabs.len() {
            let pane_id = state.workspaces[0].tabs[tab_idx].root_pane;
            mark_agent(&mut state, 0, tab_idx, pane_id);
        }
        state.workspaces[0].tabs[0].layout.focus_pane(root);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 14));

        state.previous_agent();

        let last_idx = state.workspaces[0].tabs.len() - 1;
        assert_eq!(state.workspaces[0].active_tab, last_idx);
        assert!(state.agent_panel_scroll > 0);
    }

    #[test]
    fn switch_workspace_updates_active_and_selected() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        state.switch_workspace(2);
        assert_eq!(state.active, Some(2));
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn switch_workspace_keeps_selected_visible_in_scrolled_sidebar() {
        let mut state = app_with_workspaces(&["a", "b", "c", "d", "e", "f", "g", "h"]);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 14));

        state.switch_workspace(7);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 80, 14));

        assert!(state
            .view
            .workspace_card_areas
            .iter()
            .any(|card| card.ws_idx == 7));
    }

    #[test]
    fn switching_workspace_keeps_all_mode_group_headers_visible() {
        let mut state = app_with_workspaces(&["charliezugasti", "herdr", "herdr 2"]);
        let group_two = state.create_group("group 2".to_string());
        state.move_workspace_to_group(1, group_two);
        state.move_workspace_to_group(2, group_two);
        state.group_filter_enabled = false;
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 140, 20));

        state.switch_workspace(2);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 140, 20));

        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == 0));
        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == group_two));

        state.switch_workspace(0);
        crate::ui::compute_view(&mut state, ratatui::layout::Rect::new(0, 0, 140, 20));

        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == 0));
        assert!(state
            .view
            .workspace_group_header_areas
            .iter()
            .any(|header| header.group_idx == group_two));
    }

    #[test]
    fn switch_workspace_marks_panes_seen() {
        let mut state = app_with_workspaces(&["a", "b"]);
        // Mark a pane in workspace 1 as unseen
        let id = *state.workspaces[1].panes.keys().next().unwrap();
        state.workspaces[1].panes.get_mut(&id).unwrap().seen = false;

        state.switch_workspace(1);
        assert!(state.workspaces[1].panes.get(&id).unwrap().seen);
    }

    #[test]
    fn switch_workspace_out_of_bounds_is_noop() {
        let mut state = app_with_workspaces(&["a"]);
        state.switch_workspace(5);
        assert_eq!(state.active, Some(0));
    }

    #[test]
    fn move_workspace_reorders_without_changing_logical_selection() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        let active_id = state.workspaces[1].id.clone();
        let selected_id = state.workspaces[2].id.clone();
        state.active = Some(1);
        state.selected = 2;

        state.move_workspace(1, 0);

        let names: Vec<_> = state
            .workspaces
            .iter()
            .map(|ws| ws.display_name())
            .collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        assert_eq!(state.active, Some(0));
        assert_eq!(state.selected, 2);
        assert_eq!(state.workspaces[state.active.unwrap()].id, active_id);
        assert_eq!(state.workspaces[state.selected].id, selected_id);
    }

    #[test]
    fn move_workspace_accepts_insert_at_end() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);

        state.move_workspace(0, state.workspaces.len());

        let names: Vec<_> = state
            .workspaces
            .iter()
            .map(|ws| ws.display_name())
            .collect();
        assert_eq!(names, vec!["b", "c", "a"]);
    }

    #[test]
    fn close_workspace_adjusts_indices() {
        let mut state = app_with_workspaces(&["a", "b", "c"]);
        state.selected = 1;
        state.active = Some(1);

        state.close_selected_workspace();

        assert_eq!(state.workspaces.len(), 2);
        assert_eq!(state.selected, 1);
        assert_eq!(state.active, Some(1));
        assert_eq!(state.workspaces[1].custom_name.as_deref(), Some("c"));
    }

    #[test]
    fn close_last_workspace_clears_active() {
        let mut state = app_with_workspaces(&["only"]);
        state.selected = 0;
        state.close_selected_workspace();

        assert!(state.workspaces.is_empty());
        assert_eq!(state.active, None);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn close_workspace_at_end_adjusts_selected() {
        let mut state = app_with_workspaces(&["a", "b"]);
        state.selected = 1;
        state.active = Some(1);

        state.close_selected_workspace();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.selected, 0);
        assert_eq!(state.active, Some(0));
    }

    #[test]
    fn closing_last_tab_prompts_to_close_active_workspace() {
        let mut state = app_with_workspaces(&["a", "b"]);
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = true;

        state.close_tab();

        assert_eq!(state.mode, Mode::ConfirmClose);
        assert_eq!(state.selected, 1);
        assert_eq!(state.workspaces.len(), 2);
    }

    #[test]
    fn closing_last_tab_without_confirmation_closes_active_workspace() {
        let mut state = app_with_workspaces(&["a", "b"]);
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = false;

        state.close_tab();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "a");
    }

    #[test]
    fn pane_died_last_pane_removes_workspace() {
        let mut state = app_with_workspaces(&["a", "b"]);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_pane_died(pane_id);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].custom_name.as_deref(), Some("b"));
    }

    #[test]
    fn pane_died_last_workspace_enters_navigate() {
        let mut state = app_with_workspaces(&["only"]);
        state.mode = Mode::Terminal;
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_pane_died(pane_id);

        assert!(state.workspaces.is_empty());
        assert_eq!(state.mode, Mode::Navigate);
    }

    #[test]
    fn pane_died_multi_pane_keeps_workspace() {
        let mut state = app_with_workspaces(&["test"]);
        let second_id = state.workspaces[0].test_split(Direction::Horizontal);

        state.handle_pane_died(second_id);

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].panes.len(), 1);
    }

    #[test]
    fn pane_died_unknown_pane_is_noop() {
        let mut state = app_with_workspaces(&["test"]);
        let fake_id = PaneId::from_raw(9999);

        state.handle_pane_died(fake_id);

        assert_eq!(state.workspaces.len(), 1);
    }

    #[test]
    fn state_changed_updates_pane() {
        let mut state = app_with_workspaces(&["test"]);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Working,
        });

        let terminal_id = state.workspaces[0]
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Working);
        assert_eq!(terminal.detected_agent, Some(Agent::Pi));
    }

    #[test]
    fn state_changed_idle_in_background_marks_unseen() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        // First set it to Working
        let bg_terminal_id = state.workspaces[1]
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Working;

        // Now transition to Idle while in background
        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
        });

        let pane = state.workspaces[1].panes.get(&bg_pane_id).unwrap();
        assert!(!pane.seen);
    }

    #[test]
    fn active_tab_completion_marks_pane_seen() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.outer_terminal_focus = Some(true);
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();
        let terminal_id = state.workspaces[0]
            .panes
            .get(&pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&terminal_id).unwrap().state = AgentState::Working;
        state.workspaces[0].panes.get_mut(&pane_id).unwrap().seen = false;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
        });

        let terminal = state.terminals.get(&terminal_id).unwrap();
        assert_eq!(terminal.state, AgentState::Idle);
        let pane = state.workspaces[0].panes.get(&pane_id).unwrap();
        assert!(pane.seen);
    }

    #[test]
    fn initial_idle_in_background_stays_seen() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Idle,
        });

        let pane = state.workspaces[1].panes.get(&bg_pane_id).unwrap();
        assert!(pane.seen);
    }

    #[test]
    fn waiting_sound_plays_even_in_active_workspace() {
        assert_eq!(
            notification_sound_for_state_change(true, AgentState::Working, AgentState::Blocked),
            Some(crate::sound::Sound::Request)
        );
    }

    #[test]
    fn done_sound_only_plays_in_background() {
        assert_eq!(
            notification_sound_for_state_change(false, AgentState::Working, AgentState::Idle),
            Some(crate::sound::Sound::Done)
        );
        assert_eq!(
            notification_sound_for_state_change(true, AgentState::Working, AgentState::Idle),
            None
        );
        assert_eq!(
            notification_sound_for_state_change(false, AgentState::Unknown, AgentState::Idle),
            None
        );
    }

    #[test]
    fn background_waiting_sets_attention_toast() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "background · 2");
    }

    #[test]
    fn hook_reported_unknown_agent_sets_toast_title_from_label() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::HookStateReported {
            pane_id: bg_pane_id,
            source: "custom:hermes".into(),
            agent_label: "hermes".into(),
            state: AgentState::Blocked,
            message: None,
            custom_status: None,
            seq: None,
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "hermes needs attention");
        assert_eq!(toast.context, "background · 2");
    }

    #[test]
    fn background_idle_sets_finished_toast() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        let bg_pane_id = *state.workspaces[1].panes.keys().next().unwrap();
        let bg_terminal_id = state.workspaces[1]
            .panes
            .get(&bg_pane_id)
            .unwrap()
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&bg_terminal_id).unwrap().state = AgentState::Working;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Droid),
            state: AgentState::Idle,
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::Finished);
        assert_eq!(toast.title, "droid finished");
        assert_eq!(toast.context, "background · 2");
        let target = toast.target.as_ref().expect("toast target");
        assert_eq!(&target.workspace_id, &state.workspaces[1].id);
        assert_eq!(target.pane_id, bg_pane_id);
    }

    #[test]
    fn background_toast_includes_tab_name_when_workspace_has_multiple_tabs() {
        let mut state = app_with_workspaces(&["active", "background"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        state.workspaces[1].tabs[0].set_custom_name("main".into());
        let second_tab = state.workspaces[1].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        let bg_pane_id = state.workspaces[1].tabs[second_tab].root_pane;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "background · 2 · logs");
    }

    #[test]
    fn background_tab_in_active_workspace_still_sets_toast() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        state.workspaces[0].tabs[0].set_custom_name("main".into());
        let second_tab = state.workspaces[0].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        let bg_pane_id = state.workspaces[0].tabs[second_tab].root_pane;

        state.handle_app_event(AppEvent::StateChanged {
            pane_id: bg_pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
        });

        let toast = state.toast.as_ref().unwrap();
        assert_eq!(toast.kind, ToastKind::NeedsAttention);
        assert_eq!(toast.title, "pi needs attention");
        assert_eq!(toast.context, "active · 1 · logs");
    }

    #[test]
    fn active_workspace_active_tab_does_not_set_toast() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
        });

        assert!(state.toast.is_none());
    }

    #[test]
    fn active_workspace_active_tab_keeps_herdr_toast_suppressed_when_outer_terminal_is_unfocused() {
        let mut state = app_with_workspaces(&["active"]);
        state.active = Some(0);
        state.outer_terminal_focus = Some(false);
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;
        let pane_id = *state.workspaces[0].panes.keys().next().unwrap();

        state.handle_app_event(AppEvent::StateChanged {
            pane_id,
            agent: Some(Agent::Pi),
            state: AgentState::Blocked,
        });

        assert!(state.toast.is_none());
    }

    #[test]
    fn active_tab_suppression_preserves_unknown_focus_behavior() {
        assert!(active_tab_suppresses_notifications(true, None));
        assert!(active_tab_suppresses_notifications(true, Some(true)));
        assert!(!active_tab_suppresses_notifications(true, Some(false)));
        assert!(!active_tab_suppresses_notifications(false, None));
    }

    #[test]
    fn update_ready_sets_manual_update_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;

        let updates = state.handle_app_event(AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install_command: "herdr update".into(),
        });

        assert!(updates.is_empty());
        assert_eq!(state.update_available.as_deref(), Some("0.5.0"));
        assert!(state.latest_release_notes_available);
        assert!(state.update_dismissed);
        let toast = state.toast.as_ref().expect("update toast");
        assert_eq!(toast.kind, ToastKind::UpdateInstalled);
        assert_eq!(toast.title, "v0.5.0 available");
        assert_eq!(toast.context, "detach, then run `herdr update`");
    }

    #[test]
    fn update_ready_uses_event_install_command_in_toast() {
        let mut state = AppState::test_new();
        state.toast_config.delivery = crate::config::ToastDelivery::Herdr;

        state.handle_app_event(AppEvent::UpdateReady {
            version: "0.5.0".into(),
            install_command: "brew update && brew upgrade herdr".into(),
        });

        assert_eq!(
            state.update_install_command,
            "brew update && brew upgrade herdr"
        );
        let toast = state.toast.as_ref().expect("update toast");
        assert_eq!(
            toast.context,
            "detach, then run `brew update && brew upgrade herdr`"
        );
    }

    #[test]
    fn toggle_zoom_works() {
        let mut state = app_with_workspaces(&["test"]);
        state.workspaces[0].test_split(Direction::Horizontal);

        assert!(!state.workspaces[0].zoomed);
        state.toggle_zoom();
        assert!(state.workspaces[0].zoomed);
        state.toggle_zoom();
        assert!(!state.workspaces[0].zoomed);
    }

    #[test]
    fn toggle_zoom_single_pane_noop() {
        let mut state = app_with_workspaces(&["test"]);
        state.toggle_zoom();
        assert!(!state.workspaces[0].zoomed);
    }

    #[test]
    fn close_pane_removes_from_workspace() {
        let mut state = app_with_workspaces(&["test"]);
        state.workspaces[0].test_split(Direction::Horizontal);
        assert_eq!(state.workspaces[0].panes.len(), 2);

        state.close_pane();
        assert_eq!(state.workspaces[0].panes.len(), 1);
    }

    #[test]
    fn close_pane_removes_unattached_terminal_state() {
        let mut state = app_with_workspaces(&["test"]);
        let pane_id = state.workspaces[0].test_split(Direction::Horizontal);
        state.ensure_test_terminals();
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();

        state.close_pane();

        assert!(!state.terminals.contains_key(&terminal_id));
    }

    #[test]
    fn close_tab_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["test"]);
        let tab_idx = state.workspaces[0].test_add_tab(Some("logs"));
        state.ensure_test_terminals();
        state.workspaces[0].switch_tab(tab_idx);
        let pane_id = state.workspaces[0].tabs[tab_idx].root_pane;
        let terminal_id = state.terminal_id_for_pane(0, pane_id).unwrap();
        state.session_dirty = false;

        state.close_tab();

        assert!(!state.terminals.contains_key(&terminal_id));
        assert!(state.session_dirty);
    }

    #[test]
    fn close_workspace_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["one", "two"]);
        let terminal_id = state
            .terminal_id_for_pane(0, state.workspaces[0].tabs[0].root_pane)
            .unwrap();

        state.close_selected_workspace();

        assert!(!state.terminals.contains_key(&terminal_id));
    }

    #[test]
    fn delete_group_removes_unattached_terminal_states() {
        let mut state = app_with_workspaces(&["keep", "drop"]);
        let group_idx = state.create_group("work".into());
        state.move_workspace_to_group(1, group_idx);
        let dropped_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        let kept_terminal_id = state
            .terminal_id_for_pane(0, state.workspaces[0].tabs[0].root_pane)
            .unwrap();

        state.delete_group(group_idx).unwrap();

        assert!(state.terminals.contains_key(&kept_terminal_id));
        assert!(!state.terminals.contains_key(&dropped_terminal_id));
    }

    #[test]
    fn close_tab_last_tab_closes_active_workspace_not_selected_workspace() {
        let mut state = app_with_workspaces(&["selected", "active"]);
        let active_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        state.active = Some(1);
        state.selected = 0;
        state.confirm_close = false;

        state.close_tab();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "selected");
        assert!(!state.terminals.contains_key(&active_terminal_id));
    }

    #[test]
    fn close_pane_last_pane_closes_active_workspace_not_selected_workspace() {
        let mut state = app_with_workspaces(&["selected", "active"]);
        let active_terminal_id = state
            .terminal_id_for_pane(1, state.workspaces[1].tabs[0].root_pane)
            .unwrap();
        state.active = Some(1);
        state.selected = 0;

        state.close_pane();

        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.workspaces[0].display_name(), "selected");
        assert!(!state.terminals.contains_key(&active_terminal_id));
    }
}
