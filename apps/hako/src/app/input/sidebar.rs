use ratatui::layout::Rect;

use crate::app::state::{AgentPanelScope, AppState, ViewLayout};

use super::ScrollbarClickTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceDropTarget {
    pub insert_idx: usize,
    pub group_idx: Option<usize>,
    pub indicator_row: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupDropTarget {
    pub insert_idx: usize,
    pub indicator_row: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupMenuAction {
    AllSpaces,
    Group(usize),
    NewWorkspace,
    NewGroup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentMenuAction {
    ThisSpace,
    ThisGroup,
    AllAgents,
}

impl AppState {
    pub(super) fn workspace_list_rect(&self) -> Rect {
        let sidebar = self.view.sidebar_rect;
        if self.sidebar_collapsed || sidebar.width <= 1 || sidebar.height == 0 {
            return Rect::default();
        }
        if self.view.right_sidebar_rect != Rect::default() {
            return crate::ui::left_sidebar_workspace_rect(sidebar);
        }
        if self.sidebar_is_combined_right() {
            return crate::ui::right_aligned_workspace_list_rect(
                sidebar,
                self.sidebar_section_split,
            );
        }
        crate::ui::workspace_list_rect(sidebar, self.sidebar_section_split)
    }

    pub(super) fn agent_panel_rect(&self) -> Rect {
        let sidebar = self.view.sidebar_rect;
        if self.sidebar_collapsed || sidebar.width <= 1 || sidebar.height == 0 {
            return Rect::default();
        }
        if self.view.right_sidebar_rect != Rect::default() {
            if self.right_sidebar_collapsed {
                return Rect::default();
            }
            return crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect);
        }
        if self.sidebar_is_combined_right() {
            let (_, detail_area) = crate::ui::right_aligned_expanded_sidebar_sections(
                sidebar,
                self.sidebar_section_split,
            );
            return detail_area;
        }
        let (_, detail_area) =
            crate::ui::expanded_sidebar_sections(sidebar, self.sidebar_section_split);
        detail_area
    }

    pub(super) fn agent_panel_has_leading_separator(&self) -> bool {
        self.view.right_sidebar_rect == Rect::default()
    }
    pub(crate) fn workspace_list_scrollbar_target_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<ScrollbarClickTarget> {
        let area = self.workspace_list_rect();
        let metrics = crate::ui::workspace_list_scroll_metrics(self, area);
        let track = crate::ui::workspace_list_scrollbar_rect(self, area)?;
        if col < track.x
            || col >= track.x + track.width
            || row < track.y
            || row >= track.y + track.height
        {
            return None;
        }
        if let Some(grab_row_offset) = crate::ui::scrollbar_thumb_grab_offset(metrics, track, row) {
            Some(ScrollbarClickTarget::Thumb { grab_row_offset })
        } else {
            Some(ScrollbarClickTarget::Track {
                offset_from_bottom: crate::ui::scrollbar_offset_from_row(metrics, track, row),
            })
        }
    }

    pub(crate) fn workspace_list_offset_for_drag_row(
        &self,
        row: u16,
        grab_row_offset: u16,
    ) -> Option<usize> {
        let area = self.workspace_list_rect();
        let metrics = crate::ui::workspace_list_scroll_metrics(self, area);
        let track = crate::ui::workspace_list_scrollbar_rect(self, area)?;
        Some(crate::ui::scrollbar_offset_from_drag_row(
            metrics,
            track,
            row,
            grab_row_offset,
        ))
    }

    pub(crate) fn set_workspace_list_offset_from_bottom(&mut self, offset_from_bottom: usize) {
        let area = self.workspace_list_rect();
        let metrics = crate::ui::workspace_list_scroll_metrics(self, area);
        self.workspace_scroll = metrics
            .max_offset_from_bottom
            .saturating_sub(offset_from_bottom);
    }

    pub(crate) fn scroll_workspace_list(&mut self, delta: i16) {
        if delta.is_negative() {
            self.workspace_scroll = self
                .workspace_scroll
                .saturating_sub(delta.unsigned_abs() as usize);
            return;
        }

        let area = self.workspace_list_rect();
        let max_scroll =
            crate::ui::workspace_list_scroll_metrics(self, area).max_offset_from_bottom;
        self.workspace_scroll = self
            .workspace_scroll
            .saturating_add(delta as usize)
            .min(max_scroll);
    }

    pub(crate) fn agent_panel_scrollbar_target_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<ScrollbarClickTarget> {
        let area = self.agent_panel_rect();
        let leading_separator = self.agent_panel_has_leading_separator();
        let metrics = crate::ui::agent_panel_scroll_metrics(self, area, leading_separator);
        let track = crate::ui::agent_panel_scrollbar_rect(self, area, leading_separator)?;
        if col < track.x
            || col >= track.x + track.width
            || row < track.y
            || row >= track.y + track.height
        {
            return None;
        }
        if let Some(grab_row_offset) = crate::ui::scrollbar_thumb_grab_offset(metrics, track, row) {
            Some(ScrollbarClickTarget::Thumb { grab_row_offset })
        } else {
            Some(ScrollbarClickTarget::Track {
                offset_from_bottom: crate::ui::scrollbar_offset_from_row(metrics, track, row),
            })
        }
    }

    pub(crate) fn agent_panel_offset_for_drag_row(
        &self,
        row: u16,
        grab_row_offset: u16,
    ) -> Option<usize> {
        let area = self.agent_panel_rect();
        let leading_separator = self.agent_panel_has_leading_separator();
        let metrics = crate::ui::agent_panel_scroll_metrics(self, area, leading_separator);
        let track = crate::ui::agent_panel_scrollbar_rect(self, area, leading_separator)?;
        Some(crate::ui::scrollbar_offset_from_drag_row(
            metrics,
            track,
            row,
            grab_row_offset,
        ))
    }

    pub(crate) fn set_agent_panel_offset_from_bottom(&mut self, offset_from_bottom: usize) {
        let area = self.agent_panel_rect();
        let metrics = crate::ui::agent_panel_scroll_metrics(
            self,
            area,
            self.agent_panel_has_leading_separator(),
        );
        self.agent_panel_scroll = metrics
            .max_offset_from_bottom
            .saturating_sub(offset_from_bottom);
    }

    pub(crate) fn scroll_agent_panel(&mut self, delta: i16) {
        let area = self.agent_panel_rect();
        let max_scroll = crate::ui::agent_panel_scroll_metrics(
            self,
            area,
            self.agent_panel_has_leading_separator(),
        )
        .max_offset_from_bottom;
        if delta.is_negative() {
            self.agent_panel_scroll = self
                .agent_panel_scroll
                .saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.agent_panel_scroll = self
                .agent_panel_scroll
                .saturating_add(delta as usize)
                .min(max_scroll);
        }
    }

    pub(crate) fn sidebar_footer_rect(&self) -> Rect {
        let sidebar = self.view.sidebar_rect;
        if self.sidebar_collapsed || sidebar.width <= 1 || sidebar.height == 0 {
            return Rect::default();
        }
        let content = crate::ui::left_sidebar_workspace_rect(sidebar);
        if content == Rect::default() {
            return Rect::default();
        }
        Rect::new(
            content.x,
            content.y + content.height.saturating_sub(1),
            content.width,
            1,
        )
    }

    pub(crate) fn global_launcher_rect(&self) -> Rect {
        if self.view.layout == ViewLayout::Mobile {
            return self.view.mobile_menu_hit_area;
        }

        let footer = self.sidebar_footer_rect();
        if footer == Rect::default() {
            return Rect::default();
        }

        Rect::new(footer.x, footer.y, 1.min(footer.width), footer.height)
    }

    pub(crate) fn global_menu_labels(&self) -> Vec<&'static str> {
        let mut labels = vec!["changelog"];
        if self.integration_updates_available() {
            labels.push("integrations");
        }
        labels.extend(["settings", "keybinds", "reload config", "detach"]);
        labels
    }

    pub(crate) fn global_menu_rect(&self) -> Rect {
        let screen = self.screen_rect();
        let launcher = self.global_launcher_rect();
        let labels = self.global_menu_labels();
        let content_width = labels
            .iter()
            .map(|label| {
                let badge_width = if self.global_menu_item_has_badge(label) {
                    2
                } else {
                    0
                };
                label.chars().count() as u16 + badge_width
            })
            .max()
            .unwrap_or(8)
            .saturating_add(2);
        let menu_w = content_width.saturating_add(2).min(screen.width.max(1));
        let menu_h = (labels.len() as u16 + 2).min(screen.height.max(1));
        let max_x = screen.x + screen.width.saturating_sub(menu_w);
        let desired_x = launcher.x + launcher.width.saturating_sub(menu_w);
        let x = desired_x.min(max_x);
        let y = launcher.y.saturating_sub(menu_h);
        Rect::new(x, y, menu_w, menu_h)
    }

    pub(crate) fn group_selector_rect(&self) -> Rect {
        if self.view.layout == ViewLayout::Mobile {
            return Rect::default();
        }

        if self.sidebar_collapsed {
            return crate::ui::collapsed_group_header_rect(self.view.sidebar_rect);
        }

        let ws_area =
            crate::ui::workspace_list_rect(self.view.sidebar_rect, self.sidebar_section_split);
        if ws_area == Rect::default() {
            return Rect::default();
        }
        let width = (self.group_selector_label().chars().count() as u16 + 2).min(ws_area.width);
        Rect::new(
            ws_area.x + ws_area.width.saturating_sub(width),
            ws_area.y,
            width,
            1,
        )
    }

    pub(crate) fn group_selector_label(&self) -> String {
        if self.group_filter_enabled {
            format!("{} {}", self.active_group_icon(), self.active_group_name())
        } else {
            "all".to_string()
        }
    }

    pub(crate) fn group_menu_labels(&self) -> Vec<String> {
        let all_marker = if self.group_filter_enabled {
            " "
        } else {
            "✓"
        };
        let mut labels = vec![
            "filter".to_string(),
            format!("{all_marker} all {}", self.workspaces.len()),
        ];
        labels.extend(self.groups.iter().enumerate().map(|(idx, group)| {
            let marker = if self.group_filter_enabled && idx == self.active_group {
                "✓"
            } else {
                " "
            };
            let count = self
                .workspaces
                .iter()
                .filter(|ws| ws.group_id == group.id)
                .count();
            format!("{marker} {} {} {count}", group.icon, group.name)
        }));
        labels.push("---".to_string());
        labels.push("new".to_string());
        labels.push("  space".to_string());
        labels.push("  group".to_string());
        labels
    }

    pub(crate) fn group_menu_action_for_row(&self, row_idx: usize) -> Option<GroupMenuAction> {
        if row_idx == 1 {
            return Some(GroupMenuAction::AllSpaces);
        }

        let group_start = 2;
        let group_end = group_start + self.groups.len();
        if (group_start..group_end).contains(&row_idx) {
            return Some(GroupMenuAction::Group(row_idx - group_start));
        }

        let separator_idx = group_end;
        if row_idx == separator_idx {
            return None;
        }

        let create_idx = separator_idx + 1;
        if row_idx == create_idx {
            return None;
        }

        let new_workspace_idx = create_idx + 1;
        if row_idx == new_workspace_idx {
            return Some(GroupMenuAction::NewWorkspace);
        }

        let new_group_idx = new_workspace_idx + 1;
        if row_idx == new_group_idx {
            return Some(GroupMenuAction::NewGroup);
        }

        None
    }

    pub(crate) fn group_menu_rect(&self) -> Rect {
        let screen = self.screen_rect();
        let selector = self.group_selector_rect();
        let labels = self.group_menu_labels();
        let content_width = labels
            .iter()
            .map(|label| label.chars().count() as u16)
            .max()
            .unwrap_or(8)
            .saturating_add(2);
        let menu_w = if self.sidebar_collapsed {
            content_width.saturating_add(2).min(screen.width.max(1))
        } else {
            content_width
                .saturating_add(2)
                .min(self.view.sidebar_rect.width.max(1))
                .min(screen.width.max(1))
        };
        let menu_h = (labels.len() as u16 + 2).min(screen.height.max(1));
        let x = selector
            .x
            .min(screen.x + screen.width.saturating_sub(menu_w));
        let max_y = screen.y + screen.height.saturating_sub(menu_h);
        let y = selector.y.saturating_add(1).min(max_y);
        Rect::new(x, y, menu_w, menu_h)
    }

    pub(crate) fn agent_menu_labels(&self) -> Vec<String> {
        let all_marker = if matches!(self.agent_panel_scope, AgentPanelScope::AllWorkspaces) {
            "✓"
        } else {
            " "
        };
        let space_marker = if matches!(self.agent_panel_scope, AgentPanelScope::CurrentWorkspace) {
            "✓"
        } else {
            " "
        };
        let group_marker = if matches!(self.agent_panel_scope, AgentPanelScope::CurrentGroup) {
            "✓"
        } else {
            " "
        };
        vec![
            "filter".to_string(),
            format!("{all_marker} all"),
            format!("{space_marker} space"),
            format!("{group_marker} group"),
        ]
    }

    pub(crate) fn agent_menu_action_for_row(&self, row_idx: usize) -> Option<AgentMenuAction> {
        match row_idx {
            1 => Some(AgentMenuAction::AllAgents),
            2 => Some(AgentMenuAction::ThisSpace),
            3 => Some(AgentMenuAction::ThisGroup),
            _ => None,
        }
    }

    pub(crate) fn agent_menu_rect(&self) -> Rect {
        let screen = self.screen_rect();
        let header = self.agent_menu_anchor_rect();
        let labels = self.agent_menu_labels();
        let content_width = labels
            .iter()
            .enumerate()
            .filter_map(|(idx, label)| {
                self.agent_menu_action_for_row(idx)
                    .is_some()
                    .then_some(label.chars().count() as u16)
            })
            .max()
            .unwrap_or(8)
            .saturating_add(2);
        let menu_w = content_width.saturating_add(2).min(screen.width.max(1));
        let menu_h = (labels.len() as u16 + 2).min(screen.height.max(1));
        let desired_x = header.x;
        let x = desired_x.min(screen.x + screen.width.saturating_sub(menu_w));
        let max_y = screen.y + screen.height.saturating_sub(menu_h);
        let y = header.y.saturating_add(1).min(max_y);
        Rect::new(x, y, menu_w, menu_h)
    }

    fn agent_menu_anchor_rect(&self) -> Rect {
        if self.sidebar_collapsed && self.view.right_sidebar_rect == Rect::default() {
            let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
                self.view.sidebar_rect,
                true,
                self.sidebar_section_split,
            );
            return crate::ui::collapsed_agent_panel_toggle_rect(detail_area);
        }

        crate::ui::agent_panel_toggle_rect(
            self.agent_panel_rect(),
            self.agent_panel_scope,
            self.agent_panel_has_leading_separator(),
        )
    }

    fn sidebar_is_combined_right(&self) -> bool {
        self.view.right_sidebar_rect == Rect::default()
            && self.sidebar_arrangement == crate::config::SidebarArrangementConfig::CombinedRight
    }

    pub(super) fn on_sidebar_divider(&self, col: u16, row: u16) -> bool {
        if self.sidebar_collapsed {
            return false;
        }
        let sidebar = self.view.sidebar_rect;
        let divider_x = if self.sidebar_is_combined_right() {
            sidebar.x
        } else {
            sidebar.x + sidebar.width.saturating_sub(1)
        };
        sidebar.width > 0
            && col == divider_x
            && row >= sidebar.y
            && row < sidebar.y + sidebar.height
    }

    pub(super) fn on_group_selector(&self, col: u16, row: u16) -> bool {
        let rect = self.group_selector_rect();
        rect.width > 0
            && col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
    }

    pub(super) fn on_sidebar_toggle(&self, col: u16, row: u16) -> bool {
        if self.view.layout == ViewLayout::Mobile {
            return false;
        }
        let rect = if self.sidebar_collapsed {
            crate::ui::collapsed_sidebar_toggle_rect(self.view.sidebar_rect)
        } else {
            crate::ui::expanded_sidebar_toggle_rect(self.view.sidebar_rect)
        };
        rect.width > 0
            && col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
    }

    pub(super) fn on_global_launcher(&self, col: u16, row: u16) -> bool {
        let rect = self.global_launcher_rect();
        rect.width > 0
            && col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
    }

    pub(super) fn on_right_sidebar_toggle(&self, col: u16, row: u16) -> bool {
        if self.view.layout == ViewLayout::Mobile || self.view.right_sidebar_rect == Rect::default()
        {
            return false;
        }
        let rect = crate::ui::right_sidebar_toggle_rect(
            self.view.right_sidebar_rect,
            self.right_sidebar_collapsed,
        );
        let on_divider_at_toggle = row == rect.y && col == self.view.right_sidebar_rect.x;
        rect.width > 0
            && ((col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height)
                || on_divider_at_toggle)
    }

    pub(super) fn set_manual_sidebar_width(&mut self, divider_col: u16) {
        let sidebar = self.view.sidebar_rect;
        let width = if self.sidebar_is_combined_right() {
            sidebar
                .x
                .saturating_add(sidebar.width)
                .saturating_sub(divider_col)
        } else {
            divider_col.saturating_sub(sidebar.x).saturating_add(1)
        };
        self.sidebar_width = width.clamp(self.sidebar_min_width, self.sidebar_max_width);
        self.sidebar_width_source = crate::app::state::SidebarWidthSource::Manual;
        self.mark_session_dirty();
    }

    pub(super) fn on_right_sidebar_divider(&self, col: u16, row: u16) -> bool {
        if self.right_sidebar_collapsed {
            return false;
        }
        let sidebar = self.view.right_sidebar_rect;
        sidebar.width > 0
            && col == sidebar.x
            && row >= sidebar.y
            && row < sidebar.y + sidebar.height
    }

    pub(super) fn set_manual_right_sidebar_width(&mut self, divider_col: u16) {
        let sidebar = self.view.right_sidebar_rect;
        let right_edge = sidebar.x.saturating_add(sidebar.width);
        let width = right_edge.saturating_sub(divider_col);
        self.right_sidebar_width = width.clamp(
            crate::ui::MIN_RIGHT_SIDEBAR_WIDTH,
            crate::ui::MAX_RIGHT_SIDEBAR_WIDTH,
        );
        self.mark_session_dirty();
    }

    pub(super) fn on_sidebar_section_divider(&self, col: u16, row: u16) -> bool {
        if self.sidebar_collapsed || self.view.right_sidebar_rect != Rect::default() {
            return false;
        }
        let rect = if self.sidebar_is_combined_right() {
            crate::ui::right_aligned_sidebar_section_divider_rect(
                self.view.sidebar_rect,
                self.sidebar_section_split,
            )
        } else {
            crate::ui::sidebar_section_divider_rect(
                self.view.sidebar_rect,
                self.sidebar_section_split,
            )
        };
        rect.width > 0
            && col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
    }

    pub(super) fn set_sidebar_section_split(&mut self, row: u16) {
        if self.view.right_sidebar_rect != Rect::default() {
            return;
        }
        let sidebar = self.view.sidebar_rect;
        let content_height = sidebar.height;
        if content_height < 6 {
            return;
        }
        let relative_y = row.saturating_sub(sidebar.y).saturating_sub(1);
        let ratio = (relative_y as f32) / (content_height as f32);
        self.sidebar_section_split = ratio.clamp(0.1, 0.9);
        self.mark_session_dirty();
    }

    pub(super) fn workspace_at_row(&self, row: u16) -> Option<usize> {
        let footer = self.sidebar_footer_rect();
        if footer == Rect::default() {
            return None;
        }

        let cards = if self.view.workspace_card_areas.is_empty() {
            crate::ui::compute_workspace_card_areas(self, self.view.sidebar_rect)
        } else {
            self.view.workspace_card_areas.clone()
        };

        cards.iter().find_map(|card| {
            (row >= card.rect.y && row < card.rect.y + card.rect.height).then_some(card.ws_idx)
        })
    }

    pub(super) fn workspace_group_header_at_row(&self, row: u16) -> Option<usize> {
        if self.sidebar_collapsed || self.group_filter_enabled {
            return None;
        }

        let headers = if self.view.workspace_group_header_areas.is_empty() {
            crate::ui::compute_workspace_group_header_areas(self, self.view.sidebar_rect)
        } else {
            self.view.workspace_group_header_areas.clone()
        };

        headers.iter().find_map(|header| {
            (row >= header.rect.y && row < header.rect.y + header.rect.height)
                .then_some(header.group_idx)
        })
    }
    pub(crate) fn group_drop_target_at_row(
        &self,
        row: u16,
        source_group_idx: usize,
    ) -> Option<GroupDropTarget> {
        if self.sidebar_collapsed || self.group_filter_enabled || self.groups.is_empty() {
            return None;
        }

        let area = self.workspace_list_rect();
        let footer = self.sidebar_footer_rect();
        if area == Rect::default() || row < area.y || row >= footer.y {
            return None;
        }

        let headers = if self.view.workspace_group_header_areas.is_empty() {
            crate::ui::compute_workspace_group_header_areas(self, self.view.sidebar_rect)
        } else {
            self.view.workspace_group_header_areas.clone()
        };

        headers.iter().find_map(|header| {
            if row < header.rect.y || row >= header.rect.y + header.rect.height {
                return None;
            }
            if header.group_idx == source_group_idx {
                return None;
            }

            let moving_down = source_group_idx < header.group_idx;
            let insert_idx = if moving_down {
                header.group_idx + 1
            } else {
                header.group_idx
            };
            let indicator_row = if moving_down {
                header.rect.y + header.rect.height
            } else {
                header.rect.y
            };

            Some(GroupDropTarget {
                insert_idx,
                indicator_row: Some(indicator_row),
            })
        })
    }

    pub(super) fn collapsed_workspace_at_row(&self, row: u16) -> Option<usize> {
        if !self.sidebar_collapsed {
            return None;
        }
        crate::ui::collapsed_workspace_at_row(self, self.view.sidebar_rect, row)
    }

    pub(super) fn collapsed_agent_detail_target_at(
        &self,
        row: u16,
    ) -> Option<(usize, usize, crate::layout::PaneId)> {
        if !self.sidebar_collapsed {
            return None;
        }

        let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
            self.view.sidebar_rect,
            true,
            self.sidebar_section_split,
        );
        let detail = crate::ui::collapsed_agent_panel_entry_at_row(self, detail_area, row)?;
        Some((detail.ws_idx, detail.tab_idx, detail.pane_id))
    }

    #[cfg(test)]
    pub(super) fn workspace_drop_index_at_row(&self, row: u16) -> Option<usize> {
        self.workspace_drop_target_at_row(row)
            .map(|target| target.insert_idx)
    }

    pub(crate) fn workspace_drop_target_at_row(&self, row: u16) -> Option<WorkspaceDropTarget> {
        let area = self.workspace_list_rect();
        let footer = self.sidebar_footer_rect();
        if area == Rect::default() || row < area.y || row >= footer.y {
            return None;
        }

        if !self.sidebar_collapsed && !self.group_filter_enabled {
            let list = self.workspace_list_rect();
            let headers = crate::ui::compute_workspace_group_header_areas_in_list(self, list);
            if headers
                .iter()
                .any(|header| row >= header.rect.y && row < header.rect.y + header.rect.height)
            {
                return None;
            }

            let empties = crate::ui::compute_workspace_group_empty_areas_in_list(self, list);
            if let Some(group_idx) = empties.iter().find_map(|empty| {
                (row >= empty.rect.y && row < empty.rect.y + empty.rect.height)
                    .then_some(empty.group_idx)
            }) {
                return Some(WorkspaceDropTarget {
                    insert_idx: self.group_insert_end(group_idx),
                    group_idx: Some(group_idx),
                    indicator_row: Some(row),
                });
            }

            let drops = crate::ui::compute_workspace_group_drop_areas_in_list(self, list);

            return drops
                .iter()
                .filter(|drop| row >= drop.rect.y && row < drop.rect.y + drop.rect.height)
                .min_by_key(|drop| drop.insert_idx)
                .map(|drop| WorkspaceDropTarget {
                    insert_idx: drop.insert_idx,
                    group_idx: Some(drop.group_idx),
                    indicator_row: Some(drop.rect.y),
                });
        }

        let cards = if self.view.workspace_card_areas.is_empty() {
            crate::ui::compute_workspace_card_areas(self, self.view.sidebar_rect)
        } else {
            self.view.workspace_card_areas.clone()
        };

        if cards.is_empty() {
            return Some(WorkspaceDropTarget {
                insert_idx: 0,
                group_idx: None,
                indicator_row: None,
            });
        }

        let mut insert_indices = Vec::with_capacity(cards.len() + 1);
        insert_indices.push(cards[0].ws_idx);
        insert_indices.extend(cards.iter().skip(1).map(|card| card.ws_idx));
        insert_indices.push(cards.last().map(|card| card.ws_idx + 1).unwrap_or(0));

        let mut best: Option<(usize, u16)> = None;
        for insert_idx in insert_indices {
            let Some(slot_row) = crate::ui::workspace_drop_indicator_row(&cards, area, insert_idx)
            else {
                continue;
            };
            let distance = row.abs_diff(slot_row);
            match best {
                Some((best_idx, best_distance))
                    if distance > best_distance
                        || (distance == best_distance && insert_idx > best_idx) => {}
                _ => best = Some((insert_idx, distance)),
            }
        }

        best.map(|(insert_idx, _)| WorkspaceDropTarget {
            insert_idx,
            group_idx: self.group_idx_for_insert_idx(insert_idx),
            indicator_row: crate::ui::workspace_drop_indicator_row(&cards, area, insert_idx),
        })
    }

    fn group_insert_end(&self, group_idx: usize) -> usize {
        let Some(group_id) = self.groups.get(group_idx).map(|group| group.id.as_str()) else {
            return self.workspaces.len();
        };
        self.workspaces
            .iter()
            .rposition(|workspace| workspace.group_id == group_id)
            .map(|idx| idx + 1)
            .unwrap_or(self.workspaces.len())
    }

    fn group_idx_for_insert_idx(&self, insert_idx: usize) -> Option<usize> {
        let group_id = self
            .workspaces
            .get(insert_idx)
            .or_else(|| {
                insert_idx
                    .checked_sub(1)
                    .and_then(|idx| self.workspaces.get(idx))
            })
            .map(|workspace| workspace.group_id.as_str())?;
        self.groups.iter().position(|group| group.id == group_id)
    }

    pub(super) fn on_agent_panel_scope_toggle(&self, col: u16, row: u16) -> bool {
        if self.sidebar_collapsed && self.view.right_sidebar_rect == Rect::default() {
            let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
                self.view.sidebar_rect,
                true,
                self.sidebar_section_split,
            );
            let rect = crate::ui::collapsed_agent_panel_toggle_rect(detail_area);
            return rect.width > 0
                && col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height;
        }

        let (detail_area, leading_separator) = if self.view.right_sidebar_rect != Rect::default() {
            if self.right_sidebar_collapsed {
                return false;
            }
            (
                crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect),
                false,
            )
        } else {
            (
                self.agent_panel_rect(),
                self.agent_panel_has_leading_separator(),
            )
        };
        let rect = crate::ui::agent_panel_toggle_rect(
            detail_area,
            self.agent_panel_scope,
            leading_separator,
        );
        rect.width > 0
            && col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
    }

    pub(super) fn agent_header_target_at(
        &self,
        row: u16,
    ) -> Option<crate::ui::AgentPanelHeaderTarget> {
        if self.sidebar_collapsed && self.view.right_sidebar_rect == Rect::default() {
            let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
                self.view.sidebar_rect,
                true,
                self.sidebar_section_split,
            );
            return crate::ui::collapsed_agent_panel_header_target_at_row(self, detail_area, row);
        }

        let area = self.agent_panel_rect();
        let leading_separator = self.agent_panel_has_leading_separator();
        let metrics = crate::ui::agent_panel_scroll_metrics(self, area, leading_separator);
        let body = crate::ui::agent_panel_body_rect(
            area,
            crate::ui::should_show_scrollbar(metrics),
            leading_separator,
        );
        crate::ui::agent_panel_header_target_at_row(self, body, row)
    }

    pub(super) fn agent_detail_target_at(
        &self,
        row: u16,
    ) -> Option<(usize, usize, crate::layout::PaneId)> {
        if self.sidebar_collapsed && self.view.right_sidebar_rect == Rect::default() {
            return None;
        }

        let detail_area = self.agent_panel_rect();
        let leading_separator = self.agent_panel_has_leading_separator();
        let metrics = crate::ui::agent_panel_scroll_metrics(self, detail_area, leading_separator);
        let body = crate::ui::agent_panel_body_rect(
            detail_area,
            crate::ui::should_show_scrollbar(metrics),
            leading_separator,
        );
        if body.height < 2 || row < body.y || row >= body.y + body.height {
            return None;
        }

        crate::ui::agent_panel_entry_at_row(self, body, row)
            .map(|detail| (detail.ws_idx, detail.tab_idx, detail.pane_id))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::{backend::TestBackend, buffer::Buffer, layout::Rect, Terminal};

    use super::super::{app_for_mouse_test, capture_snapshot, mouse, unique_temp_path};
    use crate::{
        app::state::{AgentPanelScope, ContextMenuKind, DragTarget, Group, Mode, SettingsSection},
        detect::{Agent, AgentState},
        workspace::Workspace,
    };

    fn render_app(app: &mut crate::app::App, width: u16, height: u16) -> (String, Buffer) {
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, width, height));
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render app");
        let buffer = terminal.backend().buffer().clone();
        let text = buffer_text(&buffer, width, height);
        (text, buffer)
    }

    fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    fn buffer_rect_text(buffer: &Buffer, rect: Rect) -> String {
        let mut text = String::new();
        for x in rect.x..rect.x + rect.width {
            text.push_str(buffer[(x, rect.y)].symbol());
        }
        text
    }

    fn mark_root_pane_as_working_agent(
        app: &mut crate::app::App,
        ws_idx: usize,
        name: &str,
    ) -> crate::layout::PaneId {
        let pane_id = app.state.workspaces[ws_idx].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[ws_idx].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal = app
            .state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal");
        terminal.set_agent_name(name.to_string());
        terminal.set_manual_label(name.to_string());
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Working);
        terminal.state = AgentState::Working;
        pane_id
    }

    #[test]
    fn clicking_group_selector_opens_group_menu() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());
        app.state.switch_group(1);
        app.state.mode = Mode::Terminal;
        let rect = app.state.group_selector_rect();

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rect.x + 1,
            rect.y,
        ));

        assert_eq!(app.state.mode, Mode::GroupMenu);
        assert_eq!(app.state.group_menu.highlighted, 3);
    }

    #[test]
    fn spaces_dropdown_keeps_active_agent_visible_and_clickable() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());
        let mut active = Workspace::test_new("agent-space");
        active.tabs[0].set_custom_name("planner".into());
        let scratch = Workspace::test_new("scratch");
        app.state.workspaces = vec![active, scratch];
        app.state.ensure_test_terminals();
        let pane_id = mark_root_pane_as_working_agent(&mut app, 0, "planner");
        app.state.active = Some(0);
        app.state.selected = 1;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::CurrentWorkspace;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 120, 30));

        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        assert_eq!(app.state.mode, Mode::GroupMenu);
        let (rendered, _) = render_app(&mut app, 120, 30);
        assert!(
            rendered.contains("planner"),
            "opening the spaces dropdown should not hide the active agent label; rendered UI:\n{rendered}"
        );
        assert!(
            rendered.contains("working"),
            "opening the spaces dropdown should not hide the active agent status; rendered UI:\n{rendered}"
        );

        let detail_area = app.state.agent_panel_rect();
        let metrics = crate::ui::agent_panel_scroll_metrics(
            &app.state,
            detail_area,
            app.state.agent_panel_has_leading_separator(),
        );
        let body = crate::ui::agent_panel_body_rect(
            detail_area,
            crate::ui::should_show_scrollbar(metrics),
            app.state.agent_panel_has_leading_separator(),
        );
        let active_agent_row = (body.y..body.y + body.height)
            .find(|row| app.state.agent_detail_target_at(*row) == Some((0, 0, pane_id)))
            .expect("active agent row should remain accessible while spaces dropdown is open");
        assert!(
            active_agent_row >= body.y,
            "active agent row should be inside the rendered agent panel"
        );
    }

    #[test]
    fn active_agent_tab_renders_label_and_status() {
        let mut app = app_for_mouse_test();
        let mut workspace = Workspace::test_new("agent-space");
        workspace.tabs[0].set_custom_name("planner".into());
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        mark_root_pane_as_working_agent(&mut app, 0, "planner");
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let (rendered, buffer) = render_app(&mut app, 120, 30);
        let active_tab = app.state.view.tab_hit_areas[0];
        let active_tab_text = buffer_rect_text(&buffer, active_tab);

        assert!(
            active_tab_text.contains("planner"),
            "active agent tab should render its label instead of a blank tab; tab text: {active_tab_text:?}"
        );
        assert!(
            rendered.contains("working"),
            "active agent tab should render the agent status in the visible agent panel; rendered UI:\n{rendered}"
        );
        assert!(
            rendered.contains("planner"),
            "active agent tab should render the agent label in the visible agent panel; rendered UI:\n{rendered}"
        );
    }

    #[test]
    fn clicking_all_spaces_group_header_toggles_group_rows() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.state.workspaces[1].group_id = "work".into();
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let header = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 1)
            .copied()
            .expect("work group header");

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            header.rect.x,
            header.rect.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            header.rect.x,
            header.rect.y,
        ));

        assert!(app.state.workspace_group_collapsed("work"));
        assert_eq!(app.state.sidebar_visible_workspace_indices(), vec![0]);
    }

    #[test]
    fn collapsing_group_moves_selection_to_visible_workspace() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.state.workspaces[1].group_id = "work".into();
        app.state.selected = 1;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));
        let header = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 1)
            .copied()
            .expect("work group header");

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            header.rect.x,
            header.rect.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            header.rect.x,
            header.rect.y,
        ));

        assert!(app.state.workspace_group_collapsed("work"));
        assert_eq!(app.state.selected, 0);
    }

    #[test]
    fn dragging_group_header_reorders_groups() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        let work_group = app.state.create_group("work".to_string());
        let ops_group = app.state.create_group("ops".to_string());
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();
        app.state.workspaces[2].group_id = app.state.groups[ops_group].id.clone();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 80));
        let source = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == work_group)
            .copied()
            .expect("work group header");
        let target = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == ops_group)
            .copied()
            .expect("ops group header");
        let drop_row = target.rect.y;

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            source.rect.x,
            source.rect.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            source.rect.x,
            drop_row,
        ));
        assert!(matches!(
            app.state.drag.as_ref().map(|drag| &drag.target),
            Some(DragTarget::GroupReorder {
                source_group_idx,
                insert_idx: Some(3),
                ..
            }) if *source_group_idx == work_group
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            source.rect.x,
            drop_row,
        ));

        let names: Vec<_> = app
            .state
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect();
        assert_eq!(names, vec!["group 1", "ops", "work"]);
        let snapshot = capture_snapshot(&app.state);
        let captured_names: Vec<_> = snapshot
            .groups
            .iter()
            .map(|group| group.name.as_str())
            .collect();
        assert_eq!(captured_names, vec!["group 1", "ops", "work"]);
    }

    #[test]
    fn dragging_group_header_over_space_rows_has_no_drop_preview() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        let work_group = app.state.create_group("work".to_string());
        let ops_group = app.state.create_group("ops".to_string());
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();
        app.state.workspaces[2].group_id = app.state.groups[ops_group].id.clone();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 80));
        let source = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == work_group)
            .copied()
            .expect("work group header");
        let space_row = app
            .state
            .view
            .workspace_card_areas
            .iter()
            .find(|card| card.ws_idx == 2)
            .map(|card| card.rect.y)
            .expect("ops workspace row");

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            source.rect.x,
            source.rect.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            source.rect.x,
            space_row,
        ));

        assert!(matches!(
            app.state.drag.as_ref().map(|drag| &drag.target),
            Some(DragTarget::GroupReorder {
                source_group_idx,
                insert_idx: None,
                indicator_row: None,
            }) if *source_group_idx == work_group
        ));
    }

    #[test]
    fn mouse_wheel_workspace_selection_skips_collapsed_group_workspaces() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.state.workspaces[1].group_id = "work".into();
        app.state.toggle_workspace_group(1);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 24));

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 2, 5));

        assert_eq!(app.state.selected, 2);
    }

    #[test]
    fn clicking_footer_help_opens_global_menu() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        let rect = app.state.global_launcher_rect();

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rect.x,
            rect.y,
        ));

        assert_eq!(app.state.mode, Mode::GlobalMenu);
    }

    #[test]
    fn clicking_old_footer_new_area_does_not_request_workspace() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::Terminal;
        app.state.mouse_capture = true;
        let rect = app.state.sidebar_footer_rect();

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rect.x + 2,
            rect.y,
        ));

        assert!(!app.state.request_new_workspace);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn group_menu_lists_all_spaces_before_groups() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = app.state.groups[1].id.clone();

        let labels = app.state.group_menu_labels();

        assert_eq!(labels[0], "filter");
        assert!(labels[1].contains("all 2"));
        assert!(labels[2].contains("group 1 1"));
        assert!(labels[3].contains("Work 1"));
        assert_eq!(labels[4], "---");
        assert_eq!(labels[5], "new");
        assert_eq!(labels[6], "  space");
        assert_eq!(labels[7], "  group");
    }

    #[test]
    fn clicking_group_menu_item_switches_group() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());
        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        let menu = app.state.group_menu_rect();
        let work_row = app
            .state
            .group_menu_labels()
            .iter()
            .position(|label| label.contains("Work"))
            .unwrap() as u16;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 1 + work_row,
        ));

        assert_eq!(app.state.active_group, 1);
        assert_ne!(app.state.mode, Mode::GroupMenu);
    }

    #[test]
    fn clicking_new_group_menu_item_opens_new_group_modal() {
        let mut app = app_for_mouse_test();
        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        let menu = app.state.group_menu_rect();
        let new_group_row = app
            .state
            .group_menu_labels()
            .iter()
            .position(|label| label == "  group")
            .unwrap() as u16;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 1 + new_group_row,
        ));

        assert_eq!(app.state.mode, Mode::RenameGroup);
        assert!(app.state.creating_new_group);
        assert_eq!(app.state.name_input, "group 2");
    }

    #[test]
    fn clicking_new_space_group_menu_item_requests_workspace() {
        let mut app = app_for_mouse_test();
        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        let menu = app.state.group_menu_rect();
        let new_space_row = app
            .state
            .group_menu_labels()
            .iter()
            .position(|label| label == "  space")
            .unwrap() as u16;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 1 + new_space_row,
        ));

        assert!(app.state.request_new_workspace);
        assert_ne!(app.state.mode, Mode::GroupMenu);
    }

    #[test]
    fn right_clicking_group_menu_item_does_not_open_group_context_menu() {
        let mut app = app_for_mouse_test();
        let work_group = app.state.create_group("Work".to_string());
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();

        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        let menu = app.state.group_menu_rect();
        let work_row = app
            .state
            .group_menu_labels()
            .iter()
            .position(|label| label.contains("Work"))
            .unwrap() as u16;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Right),
            menu.x + 2,
            menu.y + 1 + work_row,
        ));

        assert_eq!(app.state.mode, Mode::GroupMenu);
        assert!(app.state.context_menu.is_none());
    }

    #[test]
    fn right_clicking_group_header_opens_group_context_menu() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        let work_group = app.state.groups.len() - 1;
        app.state.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.state.workspaces[1].group_id = "work".into();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));
        let header = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == work_group)
            .copied()
            .expect("work group header");

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Right),
            header.rect.x + 2,
            header.rect.y,
        ));

        assert_eq!(app.state.mode, Mode::ContextMenu);
        let context = app.state.context_menu.as_ref().unwrap();
        assert_eq!(
            context.items(),
            &["new", "space", "group", "---", "manage", "settings", "---", "danger", "delete"]
        );
        assert_eq!(
            context.kind,
            ContextMenuKind::Group {
                group_idx: work_group,
                can_delete: true,
            }
        );
    }

    #[test]
    fn group_context_menu_delete_opens_confirmation_for_target_group() {
        let mut app = app_for_mouse_test();
        let work_group = app.state.create_group("Work".to_string());
        app.state.active_group = 0;
        app.state.context_menu = Some(crate::app::state::ContextMenuState {
            kind: ContextMenuKind::Group {
                group_idx: work_group,
                can_delete: true,
            },
            x: 2,
            y: 2,
            list: crate::app::state::MenuListState::new(8),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 9,
        ));

        assert_eq!(app.state.mode, Mode::ConfirmDeleteGroup);
        assert_eq!(app.state.confirm_delete_group, Some(work_group));
        assert_eq!(app.state.active_group, 0);
    }

    #[test]
    fn group_context_menu_settings_opens_settings_for_target_group() {
        let mut app = app_for_mouse_test();
        let work_group = app.state.create_group("Work".to_string());
        app.state.active_group = 0;
        app.state.context_menu = Some(crate::app::state::ContextMenuState {
            kind: ContextMenuKind::Group {
                group_idx: work_group,
                can_delete: true,
            },
            x: 2,
            y: 2,
            list: crate::app::state::MenuListState::new(5),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 6,
        ));

        assert_eq!(app.state.mode, Mode::Settings);
        assert_eq!(app.state.settings.group_settings_target, Some(work_group));
        assert_eq!(app.state.active_group, 0);
    }

    #[test]
    fn workspace_context_menu_settings_opens_settings_for_target_space() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.state.context_menu = Some(crate::app::state::ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 1,
                can_diff: false,
            },
            x: 2,
            y: 2,
            list: crate::app::state::MenuListState::new(6),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 7,
        ));

        assert_eq!(app.state.mode, Mode::Settings);
        assert_eq!(app.state.settings.workspace_settings_target, Some(1));
        assert_eq!(app.state.settings.group_settings_target, None);
        assert_eq!(
            app.state.settings.section,
            SettingsSection::WorkspaceGeneral
        );
    }

    #[test]
    fn group_menu_separator_is_not_selectable() {
        let mut app = app_for_mouse_test();
        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        let menu = app.state.group_menu_rect();
        let separator_row = app
            .state
            .group_menu_labels()
            .iter()
            .position(|label| label == "---")
            .unwrap() as u16;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 1 + separator_row,
        ));

        assert_eq!(app.state.mode, Mode::GroupMenu);
    }

    #[test]
    fn hovering_group_menu_action_rows_highlights_visual_row() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());
        app.state.switch_group(1);
        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x + 1,
            selector.y,
        ));

        let menu = app.state.group_menu_rect();
        let new_group_row = app
            .state
            .group_menu_labels()
            .iter()
            .position(|label| label == "  group")
            .unwrap() as u16;
        app.handle_mouse(mouse(
            MouseEventKind::Moved,
            menu.x + 2,
            menu.y + 1 + new_group_row,
        ));
        assert_eq!(app.state.group_menu.highlighted, new_group_row as usize);
        assert!(!app
            .state
            .group_menu_labels()
            .iter()
            .any(|label| label.contains("delete")));
    }

    #[test]
    fn confirming_group_delete_deletes_group_and_its_workspaces() {
        let mut app = app_for_mouse_test();
        let work_group = app.state.create_group("Work".to_string());
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();
        app.state.switch_group(work_group);
        app.state.confirm_delete_group = Some(work_group);
        app.state.mode = Mode::ConfirmDeleteGroup;

        let popup = app.state.confirm_close_rect();
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let (confirm, _) = crate::ui::confirm_close_button_rects(inner);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            confirm.x,
            confirm.y,
        ));

        assert_eq!(app.state.groups.len(), 1);
        assert_eq!(app.state.groups[0].name, "group 1");
        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].display_name(), "a");
        assert_eq!(app.state.active_group, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn clicking_all_spaces_group_menu_item_shows_every_workspace() {
        let mut app = app_for_mouse_test();
        let work_group = app.state.create_group("Work".to_string());
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();
        app.state.switch_group(work_group);
        assert_eq!(app.state.visible_workspace_indices(), vec![1]);

        let selector = app.state.group_selector_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            selector.x,
            selector.y,
        ));
        let menu = app.state.group_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 2,
        ));

        assert!(!app.state.group_filter_enabled);
        assert_eq!(app.state.visible_workspace_indices(), vec![0, 1]);
    }

    #[test]
    fn hovering_global_menu_updates_highlight() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations.clear();
        app.state.mode = Mode::GlobalMenu;

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(MouseEventKind::Moved, menu.x + 2, menu.y + 2));

        assert_eq!(app.state.global_menu.highlighted, 1);
    }

    #[test]
    fn clicking_keybinds_menu_item_opens_help() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations.clear();
        app.state.mode = Mode::GlobalMenu;

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 3,
        ));

        assert_eq!(app.state.mode, Mode::KeybindHelp);
    }

    #[test]
    fn clicking_settings_menu_item_opens_settings() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations.clear();
        app.state.mode = Mode::GlobalMenu;

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 2,
        ));

        assert_eq!(app.state.mode, Mode::Settings);
    }

    #[test]
    fn clicking_reload_config_menu_item_requests_reload() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations.clear();
        app.state.mode = Mode::GlobalMenu;

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 4,
        ));

        assert!(app.state.request_reload_config);
        assert_eq!(app.state.mode, Mode::Navigate);
    }

    #[test]
    fn integration_updates_add_global_menu_entry_opening_integrations() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations =
            vec![crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Omp,
                label: "omp",
                command: "omp",
                available: true,
                path: std::path::PathBuf::from("/tmp/hako-test-omp"),
                state: crate::integration::IntegrationStatusKind::Outdated,
            }];
        app.state.mode = Mode::GlobalMenu;

        assert_eq!(
            app.state.global_menu_labels(),
            vec![
                "changelog",
                "integrations",
                "settings",
                "keybinds",
                "reload config",
                "detach"
            ]
        );

        let (text, buffer) = render_app(&mut app, 120, 30);
        let menu = app.state.global_menu_rect();
        let marker_x = menu.x + menu.width.saturating_sub(2);
        assert_eq!(buffer[(marker_x, menu.y + 2)].symbol(), "●");
        assert!(text.contains("integrations"));

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 2,
        ));

        assert_eq!(app.state.mode, Mode::Settings);
        assert_eq!(app.state.settings.section, SettingsSection::Integrations);
    }

    #[test]
    fn outdated_integration_marks_footer_help_without_replacing_it() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = false;
        app.state.integration_recommendations =
            vec![crate::integration::IntegrationRecommendation {
                target: crate::api::schema::IntegrationTarget::Omp,
                label: "omp",
                command: "omp",
                available: true,
                path: std::path::PathBuf::from("/tmp/hako-test-omp"),
                state: crate::integration::IntegrationStatusKind::Outdated,
            }];

        let (text, buffer) = render_app(&mut app, 120, 30);
        let launcher = app.state.global_launcher_rect();
        let launcher_text = buffer_rect_text(&buffer, launcher);

        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(
            launcher.y,
            app.state.view.sidebar_rect.y + app.state.view.sidebar_rect.height.saturating_sub(1)
        );
        assert_eq!(launcher.x, app.state.sidebar_footer_rect().x);
        assert_eq!(launcher_text, "?");
        assert!(
            !text.contains("integrations"),
            "the footer should only expose the help affordance before the menu is open; rendered app:\n{text}"
        );
    }

    #[test]
    fn update_pending_marks_changelog_entry() {
        let mut app = app_for_mouse_test();
        app.state.update_available = Some("0.3.2".into());
        app.state.latest_release_notes_available = true;
        app.state.integration_recommendations.clear();

        assert_eq!(
            app.state.global_menu_labels(),
            vec![
                "changelog",
                "settings",
                "keybinds",
                "reload config",
                "detach"
            ]
        );
        assert!(!app.state.should_quit);
    }

    #[test]
    fn persistence_mode_menu_surfaces_detach_action() {
        let mut app = app_for_mouse_test();
        app.state.integration_recommendations.clear();
        app.state.detach_exits = false;
        app.state.mode = Mode::GlobalMenu;

        assert_eq!(
            app.state.global_menu_labels(),
            vec![
                "changelog",
                "settings",
                "keybinds",
                "reload config",
                "detach"
            ]
        );

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 5,
        ));

        assert!(app.state.detach_requested);
        assert!(!app.state.should_quit);
        assert_ne!(app.state.mode, Mode::GlobalMenu);
    }

    #[test]
    fn changelog_remains_in_menu_for_latest_installed_release_notes() {
        let mut app = app_for_mouse_test();
        app.state.latest_release_notes_available = true;
        app.state.integration_recommendations.clear();

        assert_eq!(
            app.state.global_menu_labels(),
            vec![
                "changelog",
                "settings",
                "keybinds",
                "reload config",
                "detach"
            ]
        );
    }

    #[test]
    fn clicking_agent_detail_row_switches_to_correct_tab_and_pane() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("main".into());
        let first_pane = ws.tabs[0].root_pane;
        let first_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[first_tab].root_pane;
        app.state.workspaces = vec![ws];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_terminal_id = app.state.workspaces[0].tabs[first_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let (_, detail_area) = crate::ui::expanded_sidebar_sections(
            app.state.view.sidebar_rect,
            app.state.sidebar_section_split,
        );
        let body = crate::ui::agent_panel_body_rect(detail_area, false, true);
        let second_agent_row = (body.y..body.y + body.height)
            .find(|row| app.state.agent_detail_target_at(*row) == Some((0, first_tab, second_pane)))
            .expect("second agent row should be visible");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            body.x + 2,
            second_agent_row,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, 1);
        assert_eq!(
            app.state.workspaces[0].tabs[1].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(snapshot.workspaces[0].active_tab, first_tab);
        assert_eq!(
            snapshot.workspaces[0].tabs[first_tab].focused,
            Some(second_pane.raw())
        );
    }

    #[test]
    fn clicking_agent_panel_toggle_opens_scope_menu() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scroll = 3;

        let (_, detail_area) = crate::ui::expanded_sidebar_sections(
            app.state.view.sidebar_rect,
            app.state.sidebar_section_split,
        );
        let toggle =
            crate::ui::agent_panel_toggle_rect(detail_area, app.state.agent_panel_scope, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            toggle.x,
            toggle.y,
        ));

        assert_eq!(app.state.mode, Mode::AgentMenu);
        assert_eq!(app.state.agent_menu.highlighted, 2);
        assert_eq!(app.state.agent_panel_scroll, 3);
    }

    #[test]
    fn clicking_collapsed_agent_scope_and_agent_row_uses_collapsed_controls() {
        let mut app = app_for_mouse_test();
        let mut workspace = Workspace::test_new("test");
        let first_tab = 0;
        let first_pane = workspace.tabs[first_tab].root_pane;
        let second_tab = workspace.test_add_tab(Some("build"));
        let second_pane = workspace.tabs[second_tab].root_pane;
        let first_state = workspace.tabs[first_tab]
            .panes
            .get_mut(&first_pane)
            .unwrap();
        first_state.detected_agent = Some(Agent::Pi);
        first_state.state = AgentState::Idle;
        first_state.seen = false;
        let second_state = workspace.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap();
        second_state.detected_agent = Some(Agent::Codex);
        second_state.state = AgentState::Working;

        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        app.state.sidebar_collapsed = true;
        app.state.workspaces[0].switch_tab(second_tab);
        app.state.workspaces[0].tabs[second_tab]
            .layout
            .focus_pane(second_pane);
        app.state.view.sidebar_rect = Rect::new(0, 0, 8, 24);
        app.state.view.right_sidebar_rect = Rect::default();
        app.state.view.terminal_area = Rect::new(8, 0, 80, 24);

        let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
            app.state.view.sidebar_rect,
            true,
            app.state.sidebar_section_split,
        );
        let scope_toggle = crate::ui::collapsed_agent_panel_toggle_rect(detail_area);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            scope_toggle.x,
            scope_toggle.y,
        ));
        let mode_after_scope_click = app.state.mode;
        let highlighted_scope_row = app.state.agent_menu.highlighted;
        let menu_rect_after_scope_click = app.state.agent_menu_rect();

        app.state.mode = Mode::Terminal;
        let triage_agent_row = detail_area.y + 2;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x,
            triage_agent_row,
        ));

        assert_eq!(
            (
                mode_after_scope_click,
                highlighted_scope_row,
                menu_rect_after_scope_click.y,
                app.state.workspaces[0].active_tab,
                app.state.workspaces[0].tabs[first_tab].layout.focused(),
                app.state.mode,
            ),
            (
                Mode::AgentMenu,
                1,
                scope_toggle.y + scope_toggle.height,
                first_tab,
                first_pane,
                Mode::Terminal,
            ),
            "collapsed scope clicks should open the scope menu at the scope row, and collapsed agent rows should jump to the represented pane"
        );
    }

    #[test]
    fn clicking_agent_status_header_toggles_section_rows() {
        let mut app = app_for_mouse_test();
        let mut workspace = Workspace::test_new("test");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Claude);
        pane_state.state = AgentState::Blocked;
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let detail_area = app.state.agent_panel_rect();
        let triage_row = (detail_area.y..detail_area.y + detail_area.height)
            .find(|row| {
                app.state
                    .agent_header_target_at(*row)
                    .is_some_and(|target| target.section == "triage")
            })
            .expect("triage header should be visible");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x + 2,
            triage_row,
        ));

        assert!(app.state.agent_section_collapsed("triage"));
        assert!(crate::ui::agent_panel_entry_at_row(
            &app.state,
            crate::ui::agent_panel_body_rect(
                app.state.agent_panel_rect(),
                false,
                app.state.agent_panel_has_leading_separator(),
            ),
            triage_row + 1,
        )
        .is_none());

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x + 2,
            triage_row,
        ));

        assert!(!app.state.agent_section_collapsed("triage"));
    }

    #[test]
    fn clicking_embedded_agent_status_header_toggles_section_rows() {
        let mut app = app_for_mouse_test();
        let mut workspace = Workspace::test_new("test");
        let pane = workspace.tabs[0].root_pane;
        let pane_state = workspace.tabs[0].panes.get_mut(&pane).unwrap();
        pane_state.detected_agent = Some(Agent::Claude);
        pane_state.state = AgentState::Blocked;
        app.state.workspaces = vec![workspace];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        assert_eq!(app.state.view.right_sidebar_rect, Rect::default());
        let detail_area = app.state.agent_panel_rect();
        let triage_row = (detail_area.y..detail_area.y + detail_area.height)
            .find(|row| {
                app.state
                    .agent_header_target_at(*row)
                    .is_some_and(|target| target.section == "triage")
            })
            .expect("embedded triage header should be visible");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x + 2,
            triage_row,
        ));

        assert!(app.state.agent_section_collapsed("triage"));
        assert_eq!(app.state.agent_detail_target_at(triage_row + 1), None);
    }

    #[test]
    fn agent_scope_menu_labels_count_scoped_non_triage_agents() {
        let mut app = app_for_mouse_test();
        let work_group = app.state.create_group("Work".to_string());

        let mut triage = Workspace::test_new("triage");
        let triage_pane = triage.tabs[0].root_pane;
        let triage_state = triage.tabs[0].panes.get_mut(&triage_pane).unwrap();
        triage_state.detected_agent = Some(Agent::Pi);
        triage_state.state = AgentState::Idle;
        triage_state.seen = false;

        let mut working = Workspace::test_new("working");
        let working_pane = working.tabs[0].root_pane;
        let working_state = working.tabs[0].panes.get_mut(&working_pane).unwrap();
        working_state.detected_agent = Some(Agent::Claude);
        working_state.state = AgentState::Working;

        let mut idle = Workspace::test_new("idle");
        idle.group_id = app.state.groups[work_group].id.clone();
        let idle_pane = idle.tabs[0].root_pane;
        let idle_state = idle.tabs[0].panes.get_mut(&idle_pane).unwrap();
        idle_state.detected_agent = Some(Agent::Codex);
        idle_state.state = AgentState::Idle;
        idle_state.seen = true;

        app.state.workspaces = vec![triage, working, idle];
        app.state.active = Some(0);
        app.state.selected = 0;

        assert_eq!(
            app.state.agent_menu_labels(),
            vec!["filter", "  all", "✓ space", "  group"]
        );
    }

    #[test]
    fn agent_scope_menu_uses_short_scope_labels() {
        let app = app_for_mouse_test();

        assert_eq!(
            app.state.agent_menu_labels(),
            vec!["filter", "  all", "✓ space", "  group"]
        );
        assert_eq!(
            app.state.agent_menu_rect().width,
            "  space".len() as u16 + 4
        );
    }

    #[test]
    fn clicking_agent_scope_menu_item_switches_scope() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scroll = 3;

        let (_, detail_area) = crate::ui::expanded_sidebar_sections(
            app.state.view.sidebar_rect,
            app.state.sidebar_section_split,
        );
        let toggle =
            crate::ui::agent_panel_toggle_rect(detail_area, app.state.agent_panel_scope, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            toggle.x,
            toggle.y,
        ));

        let menu = app.state.agent_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 2,
        ));

        assert_eq!(app.state.agent_panel_scope, AgentPanelScope::AllWorkspaces);
        assert_eq!(app.state.agent_panel_scroll, 0);
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(snapshot.agent_panel_scope, AgentPanelScope::AllWorkspaces);
    }

    #[test]
    fn clicking_all_workspaces_agent_row_switches_to_correct_workspace() {
        let mut app = app_for_mouse_test();
        let first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;

        let second = Workspace::test_new("two");
        let second_pane = second.tabs[0].root_pane;

        app.state.workspaces = vec![first, second];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_terminal_id = app.state.workspaces[1].tabs[0].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let (_, detail_area) = crate::ui::expanded_sidebar_sections(
            app.state.view.sidebar_rect,
            app.state.sidebar_section_split,
        );
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x + 2,
            crate::ui::agent_panel_body_rect(detail_area, false, true).y + 4,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.selected, 1);
        assert_eq!(app.state.workspaces[1].active_tab, 0);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            second_pane
        );
    }

    #[test]
    fn clicking_hidden_group_agent_reveals_that_group() {
        let mut app = app_for_mouse_test();
        let hidden_group = app.state.create_group("Work".to_string());
        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let mut second = Workspace::test_new("two");
        second.group_id = app.state.groups[hidden_group].id.clone();
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        app.state.workspaces = vec![first, second];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.active_group = 0;
        app.state.group_filter_enabled = true;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        assert_eq!(app.state.visible_workspace_indices(), vec![0]);
        let detail_area = app.state.agent_panel_rect();
        let second_agent_row = (detail_area.y..detail_area.y + detail_area.height)
            .find(|row| app.state.agent_detail_target_at(*row) == Some((1, 0, second_pane)))
            .expect("second agent row should be visible");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x + 2,
            second_agent_row,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.selected, 1);
        assert_eq!(app.state.active_group, hidden_group);
        assert_eq!(app.state.visible_workspace_indices(), vec![1]);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            second_pane
        );
    }

    #[test]
    fn clicking_all_scope_triage_agent_reveals_hidden_group() {
        let mut app = app_for_mouse_test();
        let hidden_group = app.state.create_group("Work".to_string());

        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        let first_pane_state = first.tabs[0].panes.get_mut(&first_pane).unwrap();
        first_pane_state.detected_agent = Some(Agent::Pi);
        first_pane_state.state = crate::detect::AgentState::Idle;
        first_pane_state.seen = true;

        let mut second = Workspace::test_new("two");
        second.group_id = app.state.groups[hidden_group].id.clone();
        let second_pane = second.tabs[0].root_pane;
        let second_pane_state = second.tabs[0].panes.get_mut(&second_pane).unwrap();
        second_pane_state.detected_agent = Some(Agent::Claude);
        second_pane_state.state = crate::detect::AgentState::Blocked;

        app.state.workspaces = vec![first, second];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.active_group = 0;
        app.state.group_filter_enabled = true;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        assert_eq!(app.state.visible_workspace_indices(), vec![0]);
        let detail_area = app.state.agent_panel_rect();
        let leading_separator = app.state.agent_panel_has_leading_separator();
        let metrics =
            crate::ui::agent_panel_scroll_metrics(&app.state, detail_area, leading_separator);
        let body = crate::ui::agent_panel_body_rect(
            detail_area,
            crate::ui::should_show_scrollbar(metrics),
            leading_separator,
        );

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            body.x + 2,
            body.y + 1,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.selected, 1);
        assert_eq!(app.state.active_group, hidden_group);
        assert_eq!(app.state.visible_workspace_indices(), vec![1]);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            second_pane
        );
    }

    #[test]
    fn scrolling_agent_panel_with_wheel_updates_agent_panel_scroll() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;

        let mut tabs = Vec::new();
        for (tab_name, agent) in [
            ("logs", Agent::Claude),
            ("review", Agent::Codex),
            ("ops", Agent::Gemini),
            ("cursor", Agent::Cursor),
            ("cline", Agent::Cline),
            ("opencode", Agent::OpenCode),
            ("copilot", Agent::GithubCopilot),
            ("kimi", Agent::Kimi),
        ] {
            let tab_idx = ws.test_add_tab(Some(tab_name));
            let pane_id = ws.tabs[tab_idx].root_pane;
            tabs.push((tab_idx, pane_id, agent));
        }

        app.state.workspaces = vec![ws];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        for (tab_idx, pane_id, agent) in tabs {
            let terminal_id = app.state.workspaces[0].tabs[tab_idx].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.state
                .terminals
                .get_mut(&terminal_id)
                .unwrap()
                .detected_agent = Some(agent);
        }
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        let detail_area = app.state.agent_panel_rect();
        assert!(crate::ui::should_show_scrollbar(
            crate::ui::agent_panel_scroll_metrics(&app.state, detail_area, true)
        ));

        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            detail_area.x + 1,
            detail_area.y + 4,
        ));

        assert_eq!(app.state.agent_panel_scroll, 1);
        assert_eq!(app.state.selected, 0);
    }

    #[test]
    fn clicking_scrolled_agent_detail_row_switches_to_correct_tab_and_pane() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        let mut extra_tabs = Vec::new();
        for (tab_name, agent) in [("review", Agent::Codex), ("ops", Agent::Gemini)] {
            let tab_idx = ws.test_add_tab(Some(tab_name));
            let pane_id = ws.tabs[tab_idx].root_pane;
            extra_tabs.push((tab_idx, pane_id, agent));
        }

        app.state.workspaces = vec![ws];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_terminal_id = app.state.workspaces[0].tabs[second_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        for (tab_idx, pane_id, agent) in extra_tabs {
            let terminal_id = app.state.workspaces[0].tabs[tab_idx].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.state
                .terminals
                .get_mut(&terminal_id)
                .unwrap()
                .detected_agent = Some(agent);
        }
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scroll = 1;

        let detail_area = app.state.agent_panel_rect();
        let body = crate::ui::agent_panel_body_rect(detail_area, true, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            body.x + 1,
            body.y + 1,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, second_tab);
        assert_eq!(
            app.state.workspaces[0].tabs[second_tab].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn clicking_collapsed_agent_row_switches_to_correct_tab_and_pane() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        app.state.workspaces = vec![ws];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_terminal_id = app.state.workspaces[0].tabs[second_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 80, 20);

        let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
            app.state.view.sidebar_rect,
            true,
            app.state.sidebar_section_split,
        );
        let target_row = (detail_area.y..detail_area.y + detail_area.height)
            .find(|row| {
                app.state
                    .collapsed_agent_detail_target_at(*row)
                    .is_some_and(|(_, _, pane_id)| pane_id == second_pane)
            })
            .expect("second pane row");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x,
            target_row,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, 1);
        assert_eq!(
            app.state.workspaces[0].tabs[1].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn collapsed_left_sidebar_agent_rows_work_when_right_sidebar_exists() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        app.state.workspaces = vec![ws];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_terminal_id = app.state.workspaces[0].tabs[second_tab].panes[&second_pane]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.right_sidebar_rect = Rect::new(100, 0, 28, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 96, 20);

        let (_, _, detail_area) = crate::ui::collapsed_sidebar_sections_for_split(
            app.state.view.sidebar_rect,
            true,
            app.state.sidebar_section_split,
        );
        let target_row = (detail_area.y..detail_area.y + detail_area.height)
            .find(|row| {
                app.state
                    .collapsed_agent_detail_target_at(*row)
                    .is_some_and(|(_, _, pane_id)| pane_id == second_pane)
            })
            .expect("second pane row");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x,
            target_row,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, second_tab);
        assert_eq!(
            app.state.workspaces[0].tabs[second_tab].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn clicking_collapsed_sidebar_toggle_expands_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 80, 20);

        let toggle = crate::ui::collapsed_sidebar_toggle_rect(app.state.view.sidebar_rect);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            toggle.x,
            toggle.y,
        ));

        assert!(!app.state.sidebar_collapsed);
    }

    #[test]
    fn clicking_expanded_sidebar_toggle_collapses_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.sidebar_collapsed = false;
        app.state.view.sidebar_rect = Rect::new(0, 0, 26, 20);
        app.state.view.terminal_area = Rect::new(26, 0, 80, 20);

        let toggle = crate::ui::expanded_sidebar_toggle_rect(app.state.view.sidebar_rect);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            toggle.x,
            toggle.y,
        ));

        assert!(app.state.sidebar_collapsed);
        assert_eq!(
            toggle.x,
            app.state.view.sidebar_rect.x + app.state.view.sidebar_rect.width - 2
        );
    }

    #[test]
    fn clicking_collapsed_group_header_opens_group_menu() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());
        app.state.switch_group(1);
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 80, 20);

        let header = crate::ui::collapsed_group_header_rect(app.state.view.sidebar_rect);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            header.x,
            header.y,
        ));

        assert_eq!(app.state.mode, Mode::GroupMenu);
        assert_eq!(app.state.group_menu.highlighted, 3);
        assert!(app.state.group_menu_rect().width > app.state.view.sidebar_rect.width);
    }

    #[test]
    fn clicking_collapsed_all_groups_group_header_toggles_group_rows() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        let work_group = app.state.create_group("work".to_string());
        let work_group_id = app.state.groups[work_group].id.clone();
        app.state.workspaces = vec![Workspace::test_new("home"), Workspace::test_new("api")];
        app.state.workspaces[1].group_id = work_group_id.clone();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 80, 20);

        let rows = crate::ui::collapsed_workspace_rows_rect(app.state.view.sidebar_rect, true);
        let default_workspace_row = rows.y + 1;
        let work_header_row = rows.y + 2;
        let work_workspace_row = rows.y + 3;

        assert_eq!(
            app.state.collapsed_workspace_at_row(default_workspace_row),
            Some(0)
        );
        assert_eq!(app.state.collapsed_workspace_at_row(work_header_row), None);
        assert_eq!(
            app.state.collapsed_workspace_at_row(work_workspace_row),
            Some(1)
        );
        assert!(!app.state.workspace_group_collapsed(&work_group_id));

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rows.x,
            work_header_row,
        ));

        assert!(app.state.workspace_group_collapsed(&work_group_id));
        assert_eq!(
            app.state.collapsed_workspace_at_row(work_workspace_row),
            None,
            "collapsed group should hide its workspace row"
        );

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rows.x,
            work_header_row,
        ));

        assert!(!app.state.workspace_group_collapsed(&work_group_id));
        assert_eq!(
            app.state.collapsed_workspace_at_row(work_workspace_row),
            Some(1),
            "expanded group should show its workspace row again"
        );
    }

    #[test]
    fn collapsed_workspace_rows_start_below_group_header() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 80, 20);

        let rows = crate::ui::collapsed_workspace_rows_rect(app.state.view.sidebar_rect, true);

        assert_eq!(app.state.collapsed_workspace_at_row(rows.y), Some(0));
        assert_eq!(
            app.state
                .collapsed_workspace_at_row(rows.y.saturating_sub(1)),
            None
        );

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rows.x,
            rows.y + 1,
        ));

        assert_eq!(app.state.active, Some(1));
    }

    #[test]
    fn collapsed_workspace_rows_fill_sidebar_when_right_sidebar_has_agents() {
        let area = Rect::new(0, 0, 4, 20);

        let rows_with_agents = crate::ui::collapsed_workspace_rows_rect(area, true);
        let rows_without_agents = crate::ui::collapsed_workspace_rows_rect(area, false);

        assert_eq!(rows_without_agents.y, area.y + 2);
        assert_eq!(rows_without_agents.height, area.height - 2);
        assert!(rows_without_agents.height > rows_with_agents.height);
    }

    #[test]
    fn clicking_workspace_switches_on_mouse_up() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let target_row = app.state.view.workspace_card_areas[1].rect.y;

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            2,
            target_row,
        ));
        assert_eq!(app.state.active, Some(0));
        assert!(app.state.workspace_press.is_some());

        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 2, target_row));
        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.selected, 1);
        assert!(app.state.workspace_press.is_none());
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(snapshot.active, Some(1));
        assert_eq!(snapshot.selected, 1);
    }

    #[test]
    fn dragging_workspace_reorders_without_changing_identity() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        let active_id = app.state.workspaces[1].id.clone();
        let selected_id = app.state.workspaces[2].id.clone();
        app.state.active = Some(1);
        app.state.selected = 2;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let source_row = app.state.view.workspace_card_areas[1].rect.y;
        let target_row = crate::ui::workspace_drop_indicator_row(
            &app.state.view.workspace_card_areas,
            app.state.workspace_list_rect(),
            0,
        )
        .unwrap();

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            2,
            source_row,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            2,
            target_row,
        ));
        assert!(matches!(
            app.state.drag.as_ref().map(|drag| &drag.target),
            Some(DragTarget::WorkspaceReorder {
                source_ws_idx: 1,
                insert_idx: Some(0),
                target_group_idx: Some(0),
                ..
            })
        ));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 2, target_row));

        let names: Vec<_> = app
            .state
            .workspaces
            .iter()
            .map(|ws| ws.display_name())
            .collect();
        assert_eq!(names, vec!["b", "a", "c"]);
        assert_eq!(app.state.active, Some(0));
        assert_eq!(app.state.selected, 2);
        assert_eq!(app.state.workspaces[0].id, active_id);
        assert_eq!(app.state.workspaces[2].id, selected_id);
        let snapshot = capture_snapshot(&app.state);
        let captured_names: Vec<_> = snapshot
            .workspaces
            .iter()
            .map(|ws| ws.custom_name.clone().unwrap())
            .collect();
        assert_eq!(captured_names, vec!["b", "a", "c"]);
    }

    #[test]
    fn dragging_workspace_to_empty_group_moves_it_into_group() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![Workspace::test_new("a")];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 24));
        let source = app.state.view.workspace_card_areas[0].rect;
        let empty = app.state.view.workspace_group_empty_areas[0].rect;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, source.y));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 2, empty.y));
        assert!(matches!(
            app.state.drag.as_ref().map(|drag| &drag.target),
            Some(DragTarget::WorkspaceReorder {
                source_ws_idx: 0,
                insert_idx: Some(1),
                target_group_idx: Some(1),
                ..
            })
        ));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 2, empty.y));

        assert_eq!(app.state.workspaces[0].group_id, "work");
    }

    #[test]
    fn dragging_workspace_to_group_top_moves_it_before_group_spaces() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.state.workspaces[2].group_id = "work".into();
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 80));
        let source = app.state.view.workspace_card_areas[1].rect;
        let drop_areas = crate::ui::compute_workspace_group_drop_areas_in_list(
            &app.state,
            app.state.workspace_list_rect(),
        );
        let target = drop_areas
            .iter()
            .find(|area| area.group_idx == 1 && area.insert_idx == 2)
            .copied()
            .expect("work top drop slot");

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, source.y));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            2,
            target.rect.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            2,
            target.rect.y,
        ));

        let work_names: Vec<_> = app
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == "work")
            .map(|workspace| workspace.display_name())
            .collect();
        assert_eq!(work_names, vec!["b", "c"]);
    }

    #[test]
    fn dragging_workspace_between_groups_moves_it_to_previous_group_end() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.state.workspaces[2].group_id = "work".into();
        app.state.active = Some(2);
        app.state.selected = 2;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 80));
        let source = app.state.view.workspace_card_areas[2].rect;
        let drop_areas = crate::ui::compute_workspace_group_drop_areas_in_list(
            &app.state,
            app.state.workspace_list_rect(),
        );
        let target = drop_areas
            .iter()
            .find(|area| area.group_idx == 0 && area.insert_idx == 2)
            .copied()
            .expect("default group end drop slot");

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, source.y));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            2,
            target.rect.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            2,
            target.rect.y,
        ));

        let default_names: Vec<_> = app
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id != "work")
            .map(|workspace| workspace.display_name())
            .collect();
        assert_eq!(default_names, vec!["a", "b", "c"]);
    }

    #[test]
    fn expanded_group_header_is_not_a_drop_target() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = "work".into();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let header = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 1)
            .copied()
            .expect("work group header");

        assert_eq!(app.state.workspace_drop_target_at_row(header.rect.y), None);
    }

    #[test]
    fn dragging_over_collapsed_group_header_expands_it() {
        let mut app = app_for_mouse_test();
        app.state.group_filter_enabled = false;
        app.state.groups.push(Group {
            id: "work".into(),
            name: "work".into(),
            icon: "■".into(),
            accent: None,
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = "work".into();
        app.state.toggle_workspace_group(1);
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let source = app.state.view.workspace_card_areas[0].rect;
        let header = app
            .state
            .view
            .workspace_group_header_areas
            .iter()
            .find(|header| header.group_idx == 1)
            .copied()
            .expect("work group header");

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, source.y));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            2,
            header.rect.y,
        ));

        assert!(!app.state.workspace_group_collapsed("work"));
    }

    #[test]
    fn clicking_tab_scroll_button_reveals_hidden_tabs_without_renaming() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        ws.test_add_tab(Some("logs"));
        ws.test_add_tab(Some("review"));
        ws.test_add_tab(Some("ops"));
        ws.test_add_tab(Some("notes"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 65, 20));

        let right = app.state.view.tab_scroll_right_hit_area;
        assert!(right.width > 0);

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            right.x + 1,
            right.y,
        ));

        assert_eq!(app.state.tab_scroll, 1);
        assert!(!app.state.tab_scroll_follow_active);
        assert_eq!(app.state.workspaces[0].active_tab, 0);
        assert_eq!(app.state.view.tab_hit_areas[0].width, 0);
        assert!(app.state.workspaces[0].tabs[0].custom_name.is_none());
        assert_eq!(
            app.state.workspaces[0].tabs[1].custom_name.as_deref(),
            Some("logs")
        );
    }

    #[test]
    fn clicking_last_visible_tab_at_right_edge_does_not_overscroll() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        for name in [
            "one", "two", "three", "four", "five", "six", "seven", "eight",
        ] {
            ws.test_add_tab(Some(name));
        }
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.tab_scroll = usize::MAX;
        app.state.tab_scroll_follow_active = false;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 65, 20));

        let last_idx = app.state.workspaces[0].tabs.len() - 1;
        let target = app.state.view.tab_hit_areas[last_idx];
        let clamped_scroll = app.state.tab_scroll;
        assert!(target.width > 0, "last tab should already be visible");

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            target.x + 1,
            target.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            target.x + 1,
            target.y,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, last_idx);
        assert_eq!(app.state.tab_scroll, clamped_scroll);
        assert!(app.state.view.tab_hit_areas[last_idx].width > 0);
    }

    #[test]
    fn dragging_tab_reorders_auto_and_custom_names_without_materializing_numbers() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        ws.test_add_tab(Some("foo"));
        ws.test_add_tab(None);
        let moved_root = ws.tabs[0].root_pane;
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let source = app.state.view.tab_hit_areas[0];
        let last = app.state.view.tab_hit_areas[2];
        let drop_col = last.x + last.width;

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            source.x + 1,
            source.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            drop_col,
            source.y,
        ));
        assert!(matches!(
            app.state.drag.as_ref().map(|drag| &drag.target),
            Some(DragTarget::TabReorder {
                ws_idx: 0,
                source_tab_idx: 0,
                insert_idx: Some(3),
            })
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            drop_col,
            source.y,
        ));

        let labels: Vec<_> = app.state.workspaces[0]
            .tabs
            .iter()
            .map(|tab| tab.display_name())
            .collect();
        assert_eq!(labels, vec!["foo", "3", "1"]);
        assert_eq!(
            app.state.workspaces[0].tabs[0].custom_name.as_deref(),
            Some("foo")
        );
        assert!(app.state.workspaces[0].tabs[1].custom_name.is_none());
        assert!(app.state.workspaces[0].tabs[2].custom_name.is_none());
        assert_eq!(app.state.workspaces[0].tabs[2].root_pane, moved_root);
        assert_eq!(app.state.workspaces[0].active_tab, 2);
    }

    fn temp_git_repo(branch: &str) -> std::path::PathBuf {
        let repo = unique_temp_path("sidebar-drop-slot-repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(
            repo.join(".git/HEAD"),
            format!("ref: refs/heads/{branch}\n"),
        )
        .unwrap();
        repo
    }

    #[test]
    fn compact_workspace_rows_still_have_distinct_drop_targets() {
        let mut app = app_for_mouse_test();
        let first_repo = temp_git_repo("main");
        let second_repo = temp_git_repo("main");

        let mut first = Workspace::test_new("a");
        let first_root = first.tabs[0].root_pane;
        first.identity_cwd = first_repo.clone();
        first.refresh_git_ahead_behind();
        first.cached_git_work_summary = Some(crate::workspace::GitWorkSummary::default());

        let mut second = Workspace::test_new("b");
        let second_root = second.tabs[0].root_pane;
        second.identity_cwd = second_repo.clone();
        second.refresh_git_ahead_behind();
        second.cached_git_work_summary = Some(crate::workspace::GitWorkSummary::default());

        app.state.workspaces = vec![first, second];
        app.state.ensure_test_terminals();
        let first_terminal_id = app.state.workspaces[0].tabs[0].panes[&first_root]
            .attached_terminal_id
            .clone();
        app.state.terminals.get_mut(&first_terminal_id).unwrap().cwd = first_repo.clone();
        let second_terminal_id = app.state.workspaces[1].tabs[0].panes[&second_root]
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&second_terminal_id)
            .unwrap()
            .cwd = second_repo.clone();
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let cards = &app.state.view.workspace_card_areas;
        let first = cards[0].rect;
        let second = cards[1].rect;
        assert_eq!(second.y, first.y + first.height);
        assert_eq!(
            app.state
                .workspace_drop_index_at_row(first.y.saturating_sub(1)),
            Some(0)
        );
        assert_eq!(app.state.workspace_drop_index_at_row(first.y), Some(0));
        assert_eq!(
            app.state
                .workspace_drop_index_at_row(first.y + first.height),
            Some(1)
        );
        assert_eq!(app.state.workspace_drop_index_at_row(second.y), Some(1));
        assert_eq!(
            app.state
                .workspace_drop_index_at_row(second.y + second.height),
            Some(2)
        );

        let _ = fs::remove_dir_all(first_repo);
        let _ = fs::remove_dir_all(second_repo);
    }

    #[test]
    fn bottom_drop_slot_stays_on_last_workspace_not_footer() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let cards = &app.state.view.workspace_card_areas;
        let bottom_slot = crate::ui::workspace_drop_indicator_row(
            cards,
            app.state.workspace_list_rect(),
            cards.len(),
        )
        .unwrap();

        let last = cards.last().unwrap().rect;
        assert_eq!(bottom_slot, last.y + last.height);
        assert!(bottom_slot < app.state.sidebar_footer_rect().y);
    }

    #[test]
    fn dragging_sidebar_divider_sets_manual_width() {
        let mut app = app_for_mouse_test();

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 25, 5));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 30, 5));

        assert_eq!(app.state.sidebar_width, 31);
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(snapshot.sidebar_width, Some(31));
    }

    #[test]
    fn dragging_past_max_clamps_to_configured_max() {
        let mut app = app_for_mouse_test();
        app.state.sidebar_max_width = 30;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 25, 5));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 50, 5));

        assert_eq!(app.state.sidebar_width, 30);
    }

    #[test]
    fn dragging_below_min_clamps_to_configured_min() {
        let mut app = app_for_mouse_test();
        app.state.sidebar_min_width = 22;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 25, 5));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 5, 5));

        assert_eq!(app.state.sidebar_width, 22);
    }

    #[test]
    fn dragging_sidebar_section_divider_sets_split_ratio() {
        let mut app = app_for_mouse_test();
        let divider = crate::ui::sidebar_section_divider_rect(
            app.state.view.sidebar_rect,
            app.state.sidebar_section_split,
        );

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            divider.x + 1,
            divider.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            divider.x + 1,
            divider.y + 4,
        ));

        assert!(app.state.sidebar_section_split > 0.5);
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(
            snapshot.sidebar_section_split,
            Some(app.state.sidebar_section_split)
        );
    }

    #[test]
    fn double_clicking_sidebar_divider_resets_default_width() {
        let mut app = app_for_mouse_test();
        app.state.default_sidebar_width = 26;
        app.state.sidebar_width = 30;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 25, 5));
        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 25, 5));
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 25, 5));

        assert_eq!(app.state.sidebar_width, 26);
        assert!(app.state.drag.is_none());
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(snapshot.sidebar_width, Some(26));
    }
}
