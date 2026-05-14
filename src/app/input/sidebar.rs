use ratatui::layout::Rect;

use crate::app::state::{AppState, Mode, ViewLayout};

use super::ScrollbarClickTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GroupMenuAction {
    AllSpaces,
    Group(usize),
    NewGroup,
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
        crate::ui::workspace_list_rect(sidebar, self.sidebar_section_split)
    }

    pub(super) fn agent_panel_rect(&self) -> Rect {
        if self.view.right_sidebar_rect != Rect::default() {
            if self.right_sidebar_collapsed {
                return Rect::default();
            }
            return crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect);
        }
        let sidebar = self.view.sidebar_rect;
        if self.sidebar_collapsed || sidebar.width <= 1 || sidebar.height == 0 {
            return Rect::default();
        }
        let (_, detail_area) =
            crate::ui::expanded_sidebar_sections(sidebar, self.sidebar_section_split);
        detail_area
    }

    pub(super) fn agent_panel_has_leading_separator(&self) -> bool {
        self.view.right_sidebar_rect == Rect::default()
    }

    pub(super) fn workspace_list_scrollbar_target_at(
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

    pub(super) fn workspace_list_offset_for_drag_row(
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

    pub(super) fn set_workspace_list_offset_from_bottom(&mut self, offset_from_bottom: usize) {
        let area = self.workspace_list_rect();
        let metrics = crate::ui::workspace_list_scroll_metrics(self, area);
        self.workspace_scroll = metrics
            .max_offset_from_bottom
            .saturating_sub(offset_from_bottom);
    }

    pub(super) fn scroll_workspace_list(&mut self, delta: i16) {
        if delta.is_negative() {
            self.workspace_scroll = self
                .workspace_scroll
                .saturating_sub(delta.unsigned_abs() as usize);
            return;
        }

        for _ in 0..delta as usize {
            let cards = crate::ui::compute_workspace_card_areas(self, self.view.sidebar_rect);
            let Some(last) = cards.last() else {
                break;
            };
            let visible = self.visible_workspace_indices();
            if visible.last().is_some_and(|idx| *idx == last.ws_idx) {
                break;
            }
            self.workspace_scroll = self.workspace_scroll.saturating_add(1);
        }
    }

    pub(super) fn agent_panel_scrollbar_target_at(
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

    pub(super) fn agent_panel_offset_for_drag_row(
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

    pub(super) fn set_agent_panel_offset_from_bottom(&mut self, offset_from_bottom: usize) {
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

    pub(super) fn scroll_agent_panel(&mut self, delta: i16) {
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
        let ws_area = self.workspace_list_rect();
        if ws_area == Rect::default() {
            return Rect::default();
        }
        let y = ws_area.y + ws_area.height.saturating_sub(1);
        Rect::new(ws_area.x, y, ws_area.width, 1)
    }

    pub(crate) fn sidebar_new_button_rect(&self) -> Rect {
        let footer = self.sidebar_footer_rect();
        let width = 5u16.min(footer.width.max(1));
        Rect::new(footer.x, footer.y, width, footer.height)
    }

    pub(crate) fn global_launcher_rect(&self) -> Rect {
        if self.view.layout == ViewLayout::Mobile {
            return self.view.mobile_menu_hit_area;
        }

        let footer = self.sidebar_footer_rect();
        let width = if self.update_available.is_some() {
            8
        } else {
            6
        }
        .min(footer.width.max(1));
        let x = if !self.sidebar_collapsed && footer.width > width.saturating_add(2) {
            crate::ui::expanded_sidebar_toggle_rect(self.view.sidebar_rect)
                .x
                .saturating_sub(width + 1)
        } else {
            footer.x + footer.width.saturating_sub(width)
        };
        Rect::new(x, footer.y, width, footer.height)
    }

    pub(crate) fn global_menu_labels(&self) -> Vec<&'static str> {
        let mut labels = vec!["settings", "keybinds", "reload config"];
        if self.update_available.is_some() {
            labels.push("update ready");
        } else if self.latest_release_notes_available {
            labels.push("what's new");
        }
        labels.push(if self.quit_detaches { "detach" } else { "quit" });
        labels
    }

    pub(crate) fn global_menu_rect(&self) -> Rect {
        let screen = self.screen_rect();
        let launcher = self.global_launcher_rect();
        let labels = self.global_menu_labels();
        let content_width = labels
            .iter()
            .map(|label| {
                let extra = if *label == "update ready" { 2 } else { 0 };
                label.chars().count() as u16 + extra
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
        Rect::new(ws_area.x, ws_area.y, ws_area.width, 1)
    }

    pub(crate) fn group_menu_labels(&self) -> Vec<String> {
        let all_marker = if self.group_filter_enabled { " " } else { "*" };
        let mut labels = vec![format!("{all_marker} 0 all spaces"), "---".to_string()];
        labels.extend(self.groups.iter().enumerate().map(|(idx, group)| {
            let marker = if self.group_filter_enabled && idx == self.active_group {
                "*"
            } else {
                " "
            };
            format!("{marker} {} {} {}", idx + 1, group.icon, group.name)
        }));
        labels.push("---".to_string());
        labels.push("+ new group".to_string());
        labels
    }

    pub(crate) fn group_menu_action_for_row(&self, row_idx: usize) -> Option<GroupMenuAction> {
        if row_idx == 0 {
            return Some(GroupMenuAction::AllSpaces);
        }
        if row_idx == 1 {
            return None;
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

        let new_group_idx = separator_idx + 1;
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

    pub(super) fn on_sidebar_divider(&self, col: u16, row: u16) -> bool {
        if self.sidebar_collapsed {
            return false;
        }
        let sidebar = self.view.sidebar_rect;
        sidebar.width > 0
            && col == sidebar.x + sidebar.width.saturating_sub(1)
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
        let width = divider_col.saturating_sub(sidebar.x).saturating_add(1);
        self.sidebar_width =
            width.clamp(crate::ui::MIN_SIDEBAR_WIDTH, crate::ui::MAX_SIDEBAR_WIDTH);
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
        let rect = crate::ui::sidebar_section_divider_rect(
            self.view.sidebar_rect,
            self.sidebar_section_split,
        );
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
        let relative_y = row.saturating_sub(sidebar.y);
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

    pub(super) fn collapsed_workspace_at_row(&self, row: u16) -> Option<usize> {
        if !self.sidebar_collapsed {
            return None;
        }

        let show_agent_detail = self.view.right_sidebar_rect == Rect::default();
        let ws_area =
            crate::ui::collapsed_workspace_rows_rect(self.view.sidebar_rect, show_agent_detail);
        if ws_area == Rect::default() || row < ws_area.y || row >= ws_area.y + ws_area.height {
            return None;
        }

        let idx = (row - ws_area.y) as usize;
        self.visible_workspace_indices().get(idx).copied()
    }

    fn collapsed_detail_workspace_idx(&self) -> Option<usize> {
        if matches!(
            self.mode,
            Mode::Navigate
                | Mode::RenameWorkspace
                | Mode::Resize
                | Mode::ConfirmClose
                | Mode::ConfirmDeleteGroup
                | Mode::ContextMenu
                | Mode::Settings
                | Mode::GlobalMenu
                | Mode::GroupMenu
                | Mode::KeybindHelp
        ) {
            Some(self.selected)
        } else {
            self.active
        }
    }

    pub(super) fn collapsed_agent_detail_target_at(
        &self,
        row: u16,
    ) -> Option<(usize, usize, crate::layout::PaneId)> {
        if !self.sidebar_collapsed {
            return None;
        }

        if self.view.right_sidebar_rect != Rect::default() {
            return None;
        }

        let (_, _, detail_area) =
            crate::ui::collapsed_sidebar_sections(self.view.sidebar_rect, true);
        let detail_content_area = Rect::new(
            detail_area.x,
            detail_area.y,
            detail_area.width,
            detail_area.height.saturating_sub(1),
        );
        if detail_content_area == Rect::default()
            || row < detail_content_area.y
            || row >= detail_content_area.y + detail_content_area.height
        {
            return None;
        }

        let ws_idx = self.collapsed_detail_workspace_idx()?;
        let ws = self.workspaces.get(ws_idx)?;
        let detail_idx = (row - detail_content_area.y) as usize;
        let details = ws.pane_details();
        let detail = details.get(detail_idx)?;
        Some((ws_idx, detail.tab_idx, detail.pane_id))
    }

    pub(super) fn workspace_drop_index_at_row(&self, row: u16) -> Option<usize> {
        let area = self.workspace_list_rect();
        let footer = self.sidebar_footer_rect();
        if area == Rect::default() || row < area.y || row >= footer.y {
            return None;
        }

        let cards = if self.view.workspace_card_areas.is_empty() {
            crate::ui::compute_workspace_card_areas(self, self.view.sidebar_rect)
        } else {
            self.view.workspace_card_areas.clone()
        };
        if cards.is_empty() {
            return Some(0);
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
                        || (distance == best_distance && insert_idx < best_idx) => {}
                _ => best = Some((insert_idx, distance)),
            }
        }

        best.map(|(insert_idx, _)| insert_idx)
    }

    pub(super) fn on_agent_panel_scope_toggle(&self, col: u16, row: u16) -> bool {
        if self.sidebar_collapsed && self.view.right_sidebar_rect == Rect::default() {
            return false;
        }
        let detail_area = self.agent_panel_rect();
        let rect = crate::ui::agent_panel_toggle_rect(
            detail_area,
            self.agent_panel_scope,
            self.agent_panel_has_leading_separator(),
        );
        rect.width > 0
            && col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
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

        let mut row_y = body.y;
        for detail in crate::ui::agent_panel_entries(self)
            .into_iter()
            .skip(self.agent_panel_scroll)
        {
            if row_y.saturating_add(1) >= body.y + body.height {
                break;
            }
            if row == row_y || row == row_y + 1 {
                return Some((detail.ws_idx, detail.tab_idx, detail.pane_id));
            }
            row_y = row_y.saturating_add(2);
            if row_y < body.y + body.height {
                row_y = row_y.saturating_add(1);
            }
        }
        None
    }

    pub(super) fn collapsed_right_sidebar_agent_target_at(
        &self,
        row: u16,
    ) -> Option<(usize, usize, crate::layout::PaneId)> {
        if !self.right_sidebar_collapsed {
            return None;
        }

        let rows = crate::ui::collapsed_right_sidebar_agent_rows_rect(self.view.right_sidebar_rect);
        if rows == Rect::default() || row < rows.y || row >= rows.y + rows.height {
            return None;
        }

        let idx = (row - rows.y) as usize + self.agent_panel_scroll;
        let entries = crate::ui::agent_panel_entries(self);
        let detail = entries.get(idx)?;
        Some((detail.ws_idx, detail.tab_idx, detail.pane_id))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crossterm::event::{MouseButton, MouseEventKind};
    use ratatui::layout::Rect;

    use super::super::{app_for_mouse_test, capture_snapshot, mouse, unique_temp_path};
    use crate::{
        app::state::{AgentPanelScope, ContextMenuKind, DragTarget, Mode},
        detect::Agent,
        workspace::Workspace,
    };

    #[test]
    fn clicking_launcher_opens_global_menu() {
        let mut app = app_for_mouse_test();
        let rect = app.state.global_launcher_rect();

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rect.x + rect.width.saturating_sub(1),
            rect.y,
        ));

        assert_eq!(app.state.mode, Mode::GlobalMenu);
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
    fn group_menu_numbers_all_spaces_before_groups() {
        let mut app = app_for_mouse_test();
        app.state.create_group("Work".to_string());

        let labels = app.state.group_menu_labels();

        assert!(labels[0].contains("0 all spaces"));
        assert_eq!(labels[1], "---");
        assert!(labels[2].contains("1 "));
        assert!(labels[3].contains("2 "));
        assert_eq!(labels[4], "---");
        assert_eq!(labels[5], "+ new group");
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
            .position(|label| label.contains("new group"))
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
    fn right_clicking_group_menu_item_opens_group_context_menu() {
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

        assert_eq!(app.state.mode, Mode::ContextMenu);
        let context = app.state.context_menu.as_ref().unwrap();
        assert_eq!(context.items(), &["Rename", "Theme", "Delete"]);
        assert_eq!(
            context.kind,
            ContextMenuKind::Group {
                group_idx: work_group,
                can_delete: true
            }
        );
    }

    #[test]
    fn group_context_menu_renames_target_group_without_switching() {
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
            list: crate::app::state::MenuListState::new(0),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 1,
        ));

        assert_eq!(app.state.mode, Mode::RenameGroup);
        assert_eq!(app.state.rename_group_target, Some(work_group));
        assert_eq!(app.state.name_input, "Work");
        assert_eq!(app.state.active_group, 0);
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
            list: crate::app::state::MenuListState::new(2),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 3,
        ));

        assert_eq!(app.state.mode, Mode::ConfirmDeleteGroup);
        assert_eq!(app.state.confirm_delete_group, Some(work_group));
        assert_eq!(app.state.active_group, 0);
    }

    #[test]
    fn group_context_menu_theme_opens_theme_picker_for_target_group() {
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
            list: crate::app::state::MenuListState::new(1),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 2,
        ));

        assert_eq!(app.state.mode, Mode::Settings);
        assert_eq!(app.state.settings.group_theme_target, Some(work_group));
        assert_eq!(app.state.active_group, 0);
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
            .position(|label| label.contains("new group"))
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
            menu.y + 1,
        ));

        assert!(!app.state.group_filter_enabled);
        assert_eq!(app.state.visible_workspace_indices(), vec![0, 1]);
    }

    #[test]
    fn hovering_global_menu_updates_highlight() {
        let mut app = app_for_mouse_test();
        let launcher = app.state.global_launcher_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            launcher.x,
            launcher.y,
        ));

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(MouseEventKind::Moved, menu.x + 2, menu.y + 2));

        assert_eq!(app.state.global_menu.highlighted, 1);
    }

    #[test]
    fn clicking_keybinds_menu_item_opens_help() {
        let mut app = app_for_mouse_test();
        let launcher = app.state.global_launcher_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            launcher.x,
            launcher.y,
        ));

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 2,
        ));

        assert_eq!(app.state.mode, Mode::KeybindHelp);
    }

    #[test]
    fn clicking_settings_menu_item_opens_settings() {
        let mut app = app_for_mouse_test();
        let launcher = app.state.global_launcher_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            launcher.x,
            launcher.y,
        ));

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 1,
        ));

        assert_eq!(app.state.mode, Mode::Settings);
    }

    #[test]
    fn clicking_reload_config_menu_item_requests_reload() {
        let mut app = app_for_mouse_test();
        let launcher = app.state.global_launcher_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            launcher.x,
            launcher.y,
        ));

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 3,
        ));

        assert!(app.state.request_reload_config);
        assert_eq!(app.state.mode, Mode::Navigate);
    }

    #[test]
    fn update_pending_menu_surfaces_update_ready_entry() {
        let mut app = app_for_mouse_test();
        app.state.update_available = Some("0.3.2".into());
        app.state.latest_release_notes_available = true;

        let launcher = app.state.global_launcher_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            launcher.x,
            launcher.y,
        ));

        assert_eq!(
            app.state.global_menu_labels(),
            vec![
                "settings",
                "keybinds",
                "reload config",
                "update ready",
                "quit"
            ]
        );
        assert!(!app.state.should_quit);
    }

    #[test]
    fn persistence_mode_menu_surfaces_detach_action() {
        let mut app = app_for_mouse_test();
        app.state.quit_detaches = true;

        let launcher = app.state.global_launcher_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            launcher.x,
            launcher.y,
        ));

        assert_eq!(
            app.state.global_menu_labels(),
            vec!["settings", "keybinds", "reload config", "detach"]
        );

        let menu = app.state.global_menu_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 4,
        ));

        assert!(app.state.detach_requested);
        assert!(!app.state.should_quit);
        assert_ne!(app.state.mode, Mode::GlobalMenu);
    }

    #[test]
    fn whats_new_remains_in_menu_for_latest_installed_release_notes() {
        let mut app = app_for_mouse_test();
        app.state.latest_release_notes_available = true;

        assert_eq!(
            app.state.global_menu_labels(),
            vec![
                "settings",
                "keybinds",
                "reload config",
                "what's new",
                "quit"
            ]
        );
    }

    #[test]
    fn clicking_agent_detail_row_switches_to_correct_tab_and_pane() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        ws.tabs[0].set_custom_name("main".into());
        let first_pane = ws.tabs[0].root_pane;
        ws.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        ws.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 2, 16));

        assert_eq!(app.state.workspaces[0].active_tab, 1);
        assert_eq!(
            app.state.workspaces[0].tabs[1].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(snapshot.workspaces[0].active_tab, second_tab);
        assert_eq!(
            snapshot.workspaces[0].tabs[second_tab].focused,
            Some(second_pane.raw())
        );
    }

    #[test]
    fn clicking_agent_panel_toggle_switches_scope() {
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

        assert_eq!(
            app.state.agent_panel_scope,
            AgentPanelScope::CurrentWorkspace
        );
        assert_eq!(app.state.agent_panel_scroll, 0);
        let snapshot = capture_snapshot(&app.state);
        assert_eq!(
            snapshot.agent_panel_scope,
            AgentPanelScope::CurrentWorkspace
        );
    }

    #[test]
    fn clicking_all_workspaces_agent_row_switches_to_correct_workspace() {
        let mut app = app_for_mouse_test();
        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let mut second = Workspace::test_new("two");
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        app.state.workspaces = vec![first, second];
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
            detail_area.y + 6,
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
    fn clicking_right_sidebar_agent_row_switches_to_correct_workspace() {
        let mut app = app_for_mouse_test();
        let mut first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        first.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let mut second = Workspace::test_new("two");
        let second_pane = second.tabs[0].root_pane;
        second.tabs[0]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        app.state.workspaces = vec![first, second];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scope = AgentPanelScope::AllWorkspaces;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let detail_area = app.state.agent_panel_rect();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x + 2,
            detail_area.y + 6,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.selected, 1);
        assert_eq!(
            app.state.workspaces[1].tabs[0].layout.focused(),
            second_pane
        );
    }

    #[test]
    fn clicking_right_sidebar_toggle_collapses_and_expands() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let collapse =
            crate::ui::right_sidebar_toggle_rect(app.state.view.right_sidebar_rect, false);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            app.state.view.right_sidebar_rect.x,
            collapse.y,
        ));

        assert!(app.state.right_sidebar_collapsed);

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));
        let expand = crate::ui::right_sidebar_toggle_rect(app.state.view.right_sidebar_rect, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            expand.x,
            expand.y,
        ));

        assert!(!app.state.right_sidebar_collapsed);
    }

    #[test]
    fn dragging_right_sidebar_divider_resizes_width() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let divider_x = app.state.view.right_sidebar_rect.x;
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), divider_x, 5));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            divider_x.saturating_sub(4),
            5,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Up(MouseButton::Left),
            divider_x.saturating_sub(4),
            5,
        ));

        assert_eq!(app.state.right_sidebar_width, 32);
    }

    #[test]
    fn scrolling_agent_panel_with_wheel_updates_agent_panel_scroll() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        ws.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        for (tab_name, agent) in [
            ("logs", Agent::Claude),
            ("review", Agent::Codex),
            ("ops", Agent::Gemini),
        ] {
            let tab_idx = ws.test_add_tab(Some(tab_name));
            let pane_id = ws.tabs[tab_idx].root_pane;
            ws.tabs[tab_idx]
                .panes
                .get_mut(&pane_id)
                .unwrap()
                .detected_agent = Some(agent);
        }

        app.state.workspaces = vec![ws];
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
        ws.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);

        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        ws.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);

        for (tab_name, agent) in [("review", Agent::Codex), ("ops", Agent::Gemini)] {
            let tab_idx = ws.test_add_tab(Some(tab_name));
            let pane_id = ws.tabs[tab_idx].root_pane;
            ws.tabs[tab_idx]
                .panes
                .get_mut(&pane_id)
                .unwrap()
                .detected_agent = Some(agent);
        }

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.agent_panel_scroll = 1;

        let detail_area = app.state.agent_panel_rect();
        let body = crate::ui::agent_panel_body_rect(detail_area, true, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            body.x + 1,
            body.y,
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
        ws.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        ws.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 80, 20);

        let (_, _, detail_area) =
            crate::ui::collapsed_sidebar_sections(app.state.view.sidebar_rect, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            detail_area.x,
            detail_area.y + 1,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, 1);
        assert_eq!(
            app.state.workspaces[0].tabs[1].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn collapsed_left_sidebar_ignores_agent_rows_when_right_sidebar_exists() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        ws.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = true;
        app.state.view.sidebar_rect = Rect::new(0, 0, 4, 20);
        app.state.view.right_sidebar_rect = Rect::new(100, 0, 28, 20);
        app.state.view.terminal_area = Rect::new(4, 0, 96, 20);

        let (_, _, old_detail_area) =
            crate::ui::collapsed_sidebar_sections(app.state.view.sidebar_rect, true);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            old_detail_area.x,
            old_detail_area.y + 1,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, 0);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn clicking_right_sidebar_agent_row_works_when_left_sidebar_is_collapsed() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        ws.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        ws.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.sidebar_collapsed = true;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let detail_area = app.state.agent_panel_rect();
        let body = crate::ui::agent_panel_body_rect(detail_area, false, false);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            body.x + 1,
            body.y + 3,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, second_tab);
        assert_eq!(
            app.state.workspaces[0].tabs[second_tab].layout.focused(),
            second_pane
        );
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn clicking_collapsed_right_sidebar_agent_row_switches_pane() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        ws.tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        let second_tab = ws.test_add_tab(Some("logs"));
        let second_pane = ws.tabs[second_tab].root_pane;
        ws.tabs[second_tab]
            .panes
            .get_mut(&second_pane)
            .unwrap()
            .detected_agent = Some(Agent::Claude);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.right_sidebar_collapsed = true;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 140, 20));

        let rows =
            crate::ui::collapsed_right_sidebar_agent_rows_rect(app.state.view.right_sidebar_rect);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            rows.x + 1,
            rows.y + 1,
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
    fn expanded_sidebar_menu_stays_left_of_collapse_toggle() {
        let mut app = app_for_mouse_test();
        app.state.sidebar_collapsed = false;
        app.state.view.sidebar_rect = Rect::new(0, 0, 26, 20);
        app.state.view.terminal_area = Rect::new(26, 0, 80, 20);

        let menu = app.state.global_launcher_rect();
        let toggle = crate::ui::expanded_sidebar_toggle_rect(app.state.view.sidebar_rect);

        assert!(menu.x + menu.width < toggle.x);
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

        assert_eq!(rows_without_agents.y, area.y + 1);
        assert_eq!(rows_without_agents.height, area.height - 1);
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
        assert_eq!(labels, vec!["foo", "2", "3"]);
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
    fn top_drop_slot_is_distinct_from_gap_below_first_workspace() {
        let mut app = app_for_mouse_test();
        let first_repo = temp_git_repo("main");
        let second_repo = temp_git_repo("main");

        let mut first = Workspace::test_new("a");
        let first_root = first.tabs[0].root_pane;
        first.identity_cwd = first_repo.clone();
        first.tabs[0]
            .pane_cwds
            .insert(first_root, first_repo.clone());
        first.refresh_git_ahead_behind();

        let mut second = Workspace::test_new("b");
        let second_root = second.tabs[0].root_pane;
        second.identity_cwd = second_repo.clone();
        second.tabs[0]
            .pane_cwds
            .insert(second_root, second_repo.clone());
        second.refresh_git_ahead_behind();

        app.state.workspaces = vec![first, second];
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        assert_eq!(app.state.workspace_drop_index_at_row(1), Some(0));
        assert_eq!(app.state.workspace_drop_index_at_row(2), Some(0));
        assert_eq!(app.state.workspace_drop_index_at_row(3), Some(1));
        assert_eq!(app.state.workspace_drop_index_at_row(4), Some(1));

        let _ = fs::remove_dir_all(first_repo);
        let _ = fs::remove_dir_all(second_repo);
    }

    #[test]
    fn bottom_drop_slot_stays_below_last_workspace_not_footer() {
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
        assert!(bottom_slot < app.state.sidebar_footer_rect().y.saturating_sub(1));
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
