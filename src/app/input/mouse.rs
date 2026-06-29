use bytes::Bytes;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Direction, Rect};
use tracing::warn;

use crate::{
    app::state::{
        AppState, ContextMenuKind, ContextMenuState, DragState, DragTarget, GroupPressState,
        MenuListState, Mode, RightClickPassthroughGesture, TabPressState, ViewLayout,
        WorkspacePressState,
    },
    layout::{PaneInfo, SplitBorder},
    selection::Selection,
    terminal::TerminalRuntimeRegistry,
};

#[cfg(test)]
use super::WheelRouting;
use super::{
    modal::{
        apply_context_menu_action, apply_global_menu_action, apply_rename_action,
        apply_worktree_directory_action, confirm_close_accept, confirm_close_cancel,
        confirm_delete_group_accept, confirm_delete_group_cancel, global_menu_actions, leave_modal,
        modal_action_from_buttons, request_new_tab_from_ui, ModalAction,
    },
    settings::SettingsAction,
    ScrollbarClickTarget, TAB_DRAG_THRESHOLD, WORKSPACE_DRAG_THRESHOLD,
};

impl AppState {
    pub(crate) fn handle_pane_mouse_only(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        mouse: MouseEvent,
    ) {
        if self.mode != Mode::Terminal {
            return;
        }
        let Some(info) = self.pane_at(mouse.column, mouse.row).cloned() else {
            return;
        };

        match mouse.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {
                self.forward_pane_reported_wheel(terminal_runtimes, &info, mouse);
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
                self.forward_pane_mouse_button(terminal_runtimes, &info, mouse);
            }
            MouseEventKind::Moved => {
                self.forward_pane_mouse_motion(terminal_runtimes, &info, mouse);
            }
        }
    }

    pub(super) fn handle_mouse(
        &mut self,
        terminal_runtimes: &mut TerminalRuntimeRegistry,
        mouse: MouseEvent,
    ) -> Option<SettingsAction> {
        if self.mode == Mode::Onboarding {
            self.handle_onboarding_mouse(mouse);
            return None;
        }

        if self.mode == Mode::Terminal
            && self.clickable_toast_at(mouse.column, mouse.row)
            && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        {
            self.focus_toast_target();
            return None;
        }

        if self.mode == Mode::Terminal
            && self.clickable_toast_at(mouse.column, mouse.row)
            && matches!(mouse.kind, MouseEventKind::Up(MouseButton::Left))
        {
            return None;
        }

        if self.mode == Mode::Settings {
            return self.handle_settings_mouse(mouse);
        }

        if matches!(mouse.kind, MouseEventKind::Moved) && self.mode == Mode::GlobalMenu {
            let actions = global_menu_actions(self);
            let hovered = self
                .global_menu_item_at(mouse.column, mouse.row)
                .and_then(|action| actions.iter().position(|item| *item == action));
            self.global_menu.hover(hovered);
            return None;
        }

        if matches!(mouse.kind, MouseEventKind::Moved) && self.mode == Mode::GroupMenu {
            self.group_menu.hover(
                self.group_menu_row_at(mouse.column, mouse.row)
                    .filter(|idx| self.group_menu_action_for_row(*idx).is_some()),
            );
            return None;
        }

        if matches!(mouse.kind, MouseEventKind::Moved) && self.mode == Mode::AgentMenu {
            self.agent_menu
                .hover(self.agent_menu_row_at(mouse.column, mouse.row));
            return None;
        }

        if self.mode == Mode::GlobalMenu {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                if let Some(action) = self.global_menu_item_at(mouse.column, mouse.row) {
                    apply_global_menu_action(self, action);
                } else {
                    leave_modal(self);
                }
            }
            return None;
        }

        if self.mode == Mode::GroupMenu {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                if let Some(action) = self.group_menu_item_at(mouse.column, mouse.row) {
                    match action {
                        super::sidebar::GroupMenuAction::AllSpaces => {
                            self.show_all_groups();
                            leave_modal(self);
                        }
                        super::sidebar::GroupMenuAction::Group(idx) => {
                            self.switch_group(idx);
                            leave_modal(self);
                        }
                        super::sidebar::GroupMenuAction::NewWorkspace => {
                            self.request_new_workspace = true;
                            leave_modal(self);
                        }
                        super::sidebar::GroupMenuAction::NewGroup => {
                            super::modal::open_new_group_dialog(self);
                        }
                    }
                } else {
                    let rect = self.group_menu_rect();
                    let inside_menu = mouse.column >= rect.x
                        && mouse.column < rect.x + rect.width
                        && mouse.row >= rect.y
                        && mouse.row < rect.y + rect.height;
                    if !inside_menu {
                        leave_modal(self);
                    }
                }
            }
            return None;
        }

        if self.mode == Mode::AgentMenu {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                if let Some(action) = self.agent_menu_item_at(mouse.column, mouse.row) {
                    super::modal::apply_agent_menu_action(self, action);
                    leave_modal(self);
                } else {
                    let rect = self.agent_menu_rect();
                    let inside_menu = mouse.column >= rect.x
                        && mouse.column < rect.x + rect.width
                        && mouse.row >= rect.y
                        && mouse.row < rect.y + rect.height;
                    if !inside_menu {
                        leave_modal(self);
                    }
                }
            }
            return None;
        }

        if self.mode == Mode::KeybindHelp {
            return None;
        }

        if matches!(
            self.mode,
            Mode::CommandPalette
                | Mode::AgentProfilePicker
                | Mode::DiffAgentPicker
                | Mode::GitRepoPicker
        ) {
            return None;
        }

        if self.view.layout == ViewLayout::Mobile && self.handle_mobile_mouse(mouse) {
            return None;
        }

        let sidebar = self.view.sidebar_rect;
        let in_sidebar = mouse.column >= sidebar.x
            && mouse.column < sidebar.x + sidebar.width
            && mouse.row >= sidebar.y
            && mouse.row < sidebar.y + sidebar.height;
        let right_sidebar = self.view.right_sidebar_rect;
        let in_right_sidebar = right_sidebar != Rect::default()
            && mouse.column >= right_sidebar.x
            && mouse.column < right_sidebar.x + right_sidebar.width
            && mouse.row >= right_sidebar.y
            && mouse.row < right_sidebar.y + right_sidebar.height;
        let in_chrome = in_sidebar || in_right_sidebar;

        if self.handle_right_click_passthrough(terminal_runtimes, mouse, in_chrome) {
            return None;
        }

        if matches!(mouse.kind, MouseEventKind::Moved) {
            self.hovered_tab = if self.mouse_capture && self.on_tab_bar(mouse.column, mouse.row) {
                self.tab_at(mouse.column, mouse.row)
            } else {
                None
            };
            if self.on_tab_bar(mouse.column, mouse.row) {
                return None;
            }
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.selection = None;
                self.selection_autoscroll = None;
                self.workspace_press = None;
                self.group_press = None;
                self.tab_press = None;

                if self.mode == Mode::ConfirmClose {
                    let popup = self.confirm_close_rect();
                    let inner = Rect::new(
                        popup.x + 1,
                        popup.y + 1,
                        popup.width.saturating_sub(2),
                        popup.height.saturating_sub(2),
                    );
                    let (confirm, cancel) = crate::ui::confirm_close_button_rects(inner);
                    match modal_action_from_buttons(
                        mouse.column,
                        mouse.row,
                        &[
                            (confirm, ModalAction::Confirm),
                            (cancel, ModalAction::Cancel),
                        ],
                    ) {
                        Some(ModalAction::Confirm) => confirm_close_accept(self),
                        Some(ModalAction::Cancel) | None => confirm_close_cancel(self),
                        _ => {}
                    }
                    return None;
                }

                if self.mode == Mode::ConfirmDeleteGroup {
                    let popup = self.confirm_close_rect();
                    let inner = Rect::new(
                        popup.x + 1,
                        popup.y + 1,
                        popup.width.saturating_sub(2),
                        popup.height.saturating_sub(2),
                    );
                    let (confirm, cancel) = crate::ui::confirm_close_button_rects(inner);
                    match modal_action_from_buttons(
                        mouse.column,
                        mouse.row,
                        &[
                            (confirm, ModalAction::Confirm),
                            (cancel, ModalAction::Cancel),
                        ],
                    ) {
                        Some(ModalAction::Confirm) => confirm_delete_group_accept(self),
                        Some(ModalAction::Cancel) | None => confirm_delete_group_cancel(self),
                        _ => {}
                    }
                    return None;
                }
                if self.mode == Mode::EditWorktreeDirectory {
                    let Some(inner) = self.rename_modal_inner() else {
                        apply_worktree_directory_action(self, ModalAction::Cancel);
                        return None;
                    };

                    let (save, clear, cancel) = crate::ui::rename_button_rects(inner);
                    if let Some(action) = modal_action_from_buttons(
                        mouse.column,
                        mouse.row,
                        &[
                            (save, ModalAction::Save),
                            (clear, ModalAction::Clear),
                            (cancel, ModalAction::Cancel),
                        ],
                    ) {
                        apply_worktree_directory_action(self, action);
                    } else if !rect_contains(inner, mouse.column, mouse.row) {
                        apply_worktree_directory_action(self, ModalAction::Cancel);
                    }
                    return None;
                }

                if matches!(
                    self.mode,
                    Mode::RenameWorkspace | Mode::RenameGroup | Mode::RenameTab | Mode::RenamePane
                ) {
                    let Some(inner) = self.rename_modal_inner() else {
                        apply_rename_action(self, ModalAction::Cancel);
                        return None;
                    };

                    if self.mode == Mode::RenameGroup {
                        if self.group_icon_picker_open {
                            for (rect, icon) in crate::ui::group_icon_picker_rects(self, inner) {
                                if rect_contains(rect, mouse.column, mouse.row) {
                                    self.group_icon_input = icon.to_string();
                                    self.group_icon_picker_open = false;
                                    return None;
                                }
                            }
                        }

                        if rect_contains(
                            crate::ui::group_icon_button_rect(self, inner),
                            mouse.column,
                            mouse.row,
                        ) {
                            self.group_icon_picker_open = !self.group_icon_picker_open;
                            return None;
                        }

                        if self.creating_new_group
                            && rect_contains(
                                crate::ui::group_default_directory_input_rect(self, inner),
                                mouse.column,
                                mouse.row,
                            )
                        {
                            self.group_modal_selected_field = 1;
                            self.name_input_replace_on_type = false;
                            return None;
                        }

                        if rect_contains(
                            crate::ui::group_name_input_rect(self, inner),
                            mouse.column,
                            mouse.row,
                        ) {
                            self.group_modal_selected_field = 0;
                            self.name_input_replace_on_type = false;
                            return None;
                        }
                    }

                    let (save, clear, cancel) = crate::ui::rename_button_rects(inner);
                    if let Some(action) = modal_action_from_buttons(
                        mouse.column,
                        mouse.row,
                        &[
                            (save, ModalAction::Save),
                            (clear, ModalAction::Clear),
                            (cancel, ModalAction::Cancel),
                        ],
                    ) {
                        apply_rename_action(self, action);
                    } else if !rect_contains(inner, mouse.column, mouse.row) {
                        apply_rename_action(self, ModalAction::Cancel);
                    }
                    return None;
                }

                if self.mode == Mode::ContextMenu {
                    let item_idx = self.context_menu_item_at(mouse.column, mouse.row);
                    if let Some(menu) = self.context_menu.take() {
                        if let Some(idx) = item_idx {
                            apply_context_menu_action(self, terminal_runtimes, menu, idx);
                        } else {
                            leave_modal(self);
                        }
                    }
                    return None;
                }

                if self.on_sidebar_divider(mouse.column, mouse.row) {
                    self.drag = Some(DragState {
                        target: DragTarget::SidebarDivider,
                    });
                    self.set_manual_sidebar_width(mouse.column);
                    return None;
                }

                if self.on_right_sidebar_divider(mouse.column, mouse.row)
                    && !self.on_right_sidebar_toggle(mouse.column, mouse.row)
                {
                    self.drag = Some(DragState {
                        target: DragTarget::RightSidebarDivider,
                    });
                    self.set_manual_right_sidebar_width(mouse.column);
                    return None;
                }

                if self.on_sidebar_section_divider(mouse.column, mouse.row) {
                    self.drag = Some(DragState {
                        target: DragTarget::SidebarSectionDivider,
                    });
                    self.set_sidebar_section_split(mouse.row);
                    return None;
                }

                if !in_chrome {
                    if let Some(border) = self.find_border_at(mouse.column, mouse.row) {
                        self.drag = Some(DragState {
                            target: DragTarget::PaneSplit {
                                path: border.path.clone(),
                                direction: border.direction,
                                area: border.area,
                            },
                        });
                        return None;
                    }

                    if let Some((pane_id, target)) =
                        self.scrollbar_target_at(terminal_runtimes, mouse.column, mouse.row)
                    {
                        self.focus_pane(pane_id);
                        match target {
                            ScrollbarClickTarget::Thumb { grab_row_offset } => {
                                self.drag = Some(DragState {
                                    target: DragTarget::PaneScrollbar {
                                        pane_id,
                                        grab_row_offset,
                                    },
                                });
                            }
                            ScrollbarClickTarget::Track { offset_from_bottom } => {
                                self.set_pane_scroll_offset(
                                    terminal_runtimes,
                                    pane_id,
                                    offset_from_bottom,
                                );
                            }
                        }
                        if self.mode != Mode::Terminal {
                            self.mode = Mode::Terminal;
                        }
                        return None;
                    }
                }

                if let Some(tab_idx) = self.tab_close_button_at(mouse.column, mouse.row) {
                    self.close_tab_at(tab_idx);
                    return None;
                }

                if self.on_tab_scroll_left_button(mouse.column, mouse.row) {
                    self.scroll_tabs_left();
                    return None;
                }
                if self.on_tab_scroll_right_button(mouse.column, mouse.row) {
                    self.scroll_tabs_right();
                    return None;
                }
                if let (Some(ws_idx), Some(tab_idx)) =
                    (self.active, self.tab_at(mouse.column, mouse.row))
                {
                    self.tab_press = Some(TabPressState {
                        ws_idx,
                        tab_idx,
                        start_col: mouse.column,
                        start_row: mouse.row,
                    });
                    return None;
                }
                if self.on_new_tab_button(mouse.column, mouse.row) {
                    if let Some(ws_idx) = self.active {
                        let can_diff = !self
                            .observed_git_repos_for_workspace(terminal_runtimes, ws_idx)
                            .is_empty();
                        self.context_menu = Some(ContextMenuState {
                            kind: ContextMenuKind::NewTabButton { ws_idx, can_diff },
                            x: mouse.column,
                            y: mouse.row,
                            list: MenuListState::new(1),
                        });
                        self.mode = Mode::ContextMenu;
                    }
                    return None;
                }

                if in_right_sidebar {
                    if self.on_right_sidebar_toggle(mouse.column, mouse.row) {
                        self.right_sidebar_collapsed = !self.right_sidebar_collapsed;
                        self.mark_session_dirty();
                        return None;
                    }

                    if self.right_sidebar_collapsed {
                        return None;
                    }

                    if self.handle_native_diff_right_sidebar_click(mouse) {
                        return None;
                    }

                    return None;
                } else if in_sidebar {
                    if self.on_sidebar_toggle(mouse.column, mouse.row) {
                        self.sidebar_collapsed = !self.sidebar_collapsed;
                        self.mark_session_dirty();
                        return None;
                    }

                    if self.sidebar_collapsed {
                        if self.on_group_selector(mouse.column, mouse.row) {
                            super::modal::open_group_menu(self);
                            return None;
                        }

                        if let Some(idx) = self.collapsed_workspace_at_row(mouse.row) {
                            self.switch_workspace(idx);
                            self.mode = Mode::Terminal;
                            return None;
                        }

                        if let Some((ws_idx, tab_idx, pane_id)) =
                            self.collapsed_agent_detail_target_at(mouse.row)
                        {
                            self.switch_workspace(ws_idx);
                            self.switch_tab(tab_idx);
                            self.focus_pane(pane_id);
                            self.mode = Mode::Terminal;
                        }
                        return None;
                    }

                    if self.on_group_selector(mouse.column, mouse.row) {
                        super::modal::open_group_menu(self);
                        return None;
                    }

                    if let Some(target) =
                        self.workspace_list_scrollbar_target_at(mouse.column, mouse.row)
                    {
                        match target {
                            ScrollbarClickTarget::Thumb { grab_row_offset } => {
                                self.drag = Some(DragState {
                                    target: DragTarget::WorkspaceListScrollbar { grab_row_offset },
                                });
                            }
                            ScrollbarClickTarget::Track { offset_from_bottom } => {
                                self.set_workspace_list_offset_from_bottom(offset_from_bottom);
                            }
                        }
                        return None;
                    }

                    if let Some(group_idx) = self.workspace_group_header_at_row(mouse.row) {
                        self.group_press = Some(GroupPressState {
                            group_idx,
                            start_col: mouse.column,
                            start_row: mouse.row,
                        });
                        return None;
                    }

                    if let Some(target) = self.agent_header_target_at(mouse.row) {
                        self.toggle_agent_section(target.section);
                        self.agent_panel_scroll = self.agent_panel_scroll.min(
                            crate::ui::agent_panel_scroll_metrics(
                                self,
                                self.agent_panel_rect(),
                                self.agent_panel_has_leading_separator(),
                            )
                            .max_offset_from_bottom,
                        );
                        return None;
                    }

                    if let Some(idx) = self.workspace_at_row(mouse.row) {
                        self.workspace_press = Some(WorkspacePressState {
                            ws_idx: idx,
                            start_col: mouse.column,
                            start_row: mouse.row,
                        });
                        return None;
                    }

                    if self.on_agent_panel_scope_toggle(mouse.column, mouse.row) {
                        super::modal::open_agent_menu(self);
                        return None;
                    }

                    if let Some(target) =
                        self.agent_panel_scrollbar_target_at(mouse.column, mouse.row)
                    {
                        match target {
                            ScrollbarClickTarget::Thumb { grab_row_offset } => {
                                self.drag = Some(DragState {
                                    target: DragTarget::AgentPanelScrollbar { grab_row_offset },
                                });
                            }
                            ScrollbarClickTarget::Track { offset_from_bottom } => {
                                self.set_agent_panel_offset_from_bottom(offset_from_bottom);
                            }
                        }
                        return None;
                    }

                    if let Some((ws_idx, tab_idx, pane_id)) = self.agent_detail_target_at(mouse.row)
                    {
                        self.switch_workspace(ws_idx);
                        self.switch_tab(tab_idx);
                        self.focus_pane(pane_id);
                        self.mode = Mode::Terminal;
                        return None;
                    }
                } else if let Some(info) = self.pane_at(mouse.column, mouse.row).cloned() {
                    self.focus_pane(info.id);
                    if self.mode != Mode::Terminal {
                        self.mode = Mode::Terminal;
                    }

                    if self.handle_native_diff_mouse_down(&info, mouse) {
                        self.selection = None;
                        self.selection_autoscroll = None;
                        return None;
                    }

                    if self.forward_pane_mouse_button(terminal_runtimes, &info, mouse) {
                        self.selection = None;
                        self.selection_autoscroll = None;
                        return None;
                    }

                    let (row, col) = (
                        mouse.row - info.inner_rect.y,
                        mouse.column - info.inner_rect.x,
                    );
                    self.selection = Some(Selection::anchor(
                        info.id,
                        row,
                        col,
                        self.pane_scroll_metrics(terminal_runtimes, info.id),
                    ));
                } else if let Some(info) = self.view.pane_infos.iter().find(|p| {
                    mouse.column >= p.rect.x
                        && mouse.column < p.rect.x + p.rect.width
                        && mouse.row >= p.rect.y
                        && mouse.row < p.rect.y + p.rect.height
                }) {
                    let id = info.id;
                    self.focus_pane(id);
                    if self.mode != Mode::Terminal {
                        self.mode = Mode::Terminal;
                    }
                }
            }

            MouseEventKind::Drag(MouseButton::Left) => {
                if self.selection.is_some() {
                    self.update_selection_drag(terminal_runtimes, mouse.column, mouse.row);
                    return None;
                }

                if self.drag.is_none() {
                    if let Some(info) = self.pane_mouse_target(mouse.column, mouse.row).cloned() {
                        if self.forward_pane_mouse_button(terminal_runtimes, &info, mouse) {
                            self.selection = None;
                            self.selection_autoscroll = None;
                            return None;
                        }
                    }
                }

                let dragging_workspace = self.workspace_press.is_some()
                    || matches!(
                        self.drag.as_ref().map(|drag| &drag.target),
                        Some(DragTarget::WorkspaceReorder { .. })
                    );
                if dragging_workspace {
                    if let Some(group_idx) = self.workspace_group_header_at_row(mouse.row) {
                        if self
                            .groups
                            .get(group_idx)
                            .is_some_and(|group| self.workspace_group_collapsed(&group.id))
                        {
                            self.toggle_workspace_group(group_idx);
                            self.workspace_scroll = self
                                .workspace_scroll
                                .min(crate::ui::workspace_list_entry_count(self).saturating_sub(1));
                        }
                    }
                }

                let workspace_drop_target = self.workspace_drop_target_at_row(mouse.row);
                let group_drag_source_idx = self
                    .group_press
                    .as_ref()
                    .map(|press| press.group_idx)
                    .or_else(|| match self.drag.as_ref().map(|drag| &drag.target) {
                        Some(DragTarget::GroupReorder {
                            source_group_idx, ..
                        }) => Some(*source_group_idx),
                        _ => None,
                    });
                let group_drop_target = group_drag_source_idx
                    .and_then(|source_idx| self.group_drop_target_at_row(mouse.row, source_idx));
                let tab_drop_index = self.tab_drop_index_at(mouse.column, mouse.row);
                if self.drag.is_none() {
                    if let Some(press) = &self.workspace_press {
                        let delta_col = mouse.column.abs_diff(press.start_col);
                        let delta_row = mouse.row.abs_diff(press.start_row);
                        if delta_col.max(delta_row) >= WORKSPACE_DRAG_THRESHOLD {
                            self.drag = Some(DragState {
                                target: DragTarget::WorkspaceReorder {
                                    source_ws_idx: press.ws_idx,
                                    insert_idx: workspace_drop_target
                                        .map(|target| target.insert_idx),
                                    target_group_idx: workspace_drop_target
                                        .and_then(|target| target.group_idx),
                                    indicator_row: workspace_drop_target
                                        .and_then(|target| target.indicator_row),
                                },
                            });
                        }
                    } else if let Some(press) = &self.group_press {
                        let delta_col = mouse.column.abs_diff(press.start_col);
                        let delta_row = mouse.row.abs_diff(press.start_row);
                        if delta_col.max(delta_row) >= WORKSPACE_DRAG_THRESHOLD {
                            self.drag = Some(DragState {
                                target: DragTarget::GroupReorder {
                                    source_group_idx: press.group_idx,
                                    insert_idx: group_drop_target.map(|target| target.insert_idx),
                                    indicator_row: group_drop_target
                                        .and_then(|target| target.indicator_row),
                                },
                            });
                        }
                    } else if let Some(press) = &self.tab_press {
                        let delta_col = mouse.column.abs_diff(press.start_col);
                        let delta_row = mouse.row.abs_diff(press.start_row);
                        if delta_col.max(delta_row) >= TAB_DRAG_THRESHOLD {
                            self.drag = Some(DragState {
                                target: DragTarget::TabReorder {
                                    ws_idx: press.ws_idx,
                                    source_tab_idx: press.tab_idx,
                                    insert_idx: tab_drop_index,
                                },
                            });
                        }
                    }
                }

                if let Some(DragState {
                    target:
                        DragTarget::WorkspaceReorder {
                            insert_idx,
                            target_group_idx,
                            indicator_row,
                            ..
                        },
                }) = &mut self.drag
                {
                    *insert_idx = workspace_drop_target.map(|target| target.insert_idx);
                    *target_group_idx = workspace_drop_target.and_then(|target| target.group_idx);
                    *indicator_row = workspace_drop_target.and_then(|target| target.indicator_row);
                } else if let Some(DragState {
                    target:
                        DragTarget::GroupReorder {
                            insert_idx,
                            indicator_row,
                            ..
                        },
                }) = &mut self.drag
                {
                    *insert_idx = group_drop_target.map(|target| target.insert_idx);
                    *indicator_row = group_drop_target.and_then(|target| target.indicator_row);
                } else if let Some(DragState {
                    target:
                        DragTarget::TabReorder {
                            ws_idx, insert_idx, ..
                        },
                }) = &mut self.drag
                {
                    if self.active == Some(*ws_idx) {
                        *insert_idx = tab_drop_index;
                    }
                } else if let Some(drag) = &self.drag {
                    match &drag.target {
                        DragTarget::WorkspaceReorder { .. }
                        | DragTarget::GroupReorder { .. }
                        | DragTarget::TabReorder { .. } => {}
                        DragTarget::WorkspaceListScrollbar { grab_row_offset } => {
                            if let Some(offset_from_bottom) =
                                self.workspace_list_offset_for_drag_row(mouse.row, *grab_row_offset)
                            {
                                self.set_workspace_list_offset_from_bottom(offset_from_bottom);
                            }
                        }
                        DragTarget::AgentPanelScrollbar { grab_row_offset } => {
                            if let Some(offset_from_bottom) =
                                self.agent_panel_offset_for_drag_row(mouse.row, *grab_row_offset)
                            {
                                self.set_agent_panel_offset_from_bottom(offset_from_bottom);
                            }
                        }
                        DragTarget::PaneSplit {
                            path,
                            direction,
                            area,
                        } => {
                            let ratio = match direction {
                                Direction::Horizontal => {
                                    (mouse.column.saturating_sub(area.x)) as f32
                                        / area.width.max(1) as f32
                                }
                                Direction::Vertical => {
                                    (mouse.row.saturating_sub(area.y)) as f32
                                        / area.height.max(1) as f32
                                }
                            };
                            let ratio = ratio.clamp(0.1, 0.9);
                            let path = path.clone();
                            if let Some(tab) = self
                                .active
                                .and_then(|i| self.workspaces.get_mut(i))
                                .and_then(|ws| ws.active_tab_mut())
                            {
                                tab.layout.set_ratio_at(&path, ratio);
                                self.mark_session_dirty();
                            }
                        }
                        DragTarget::PaneScrollbar {
                            pane_id,
                            grab_row_offset,
                        } => {
                            if let Some(offset_from_bottom) = self.scrollbar_offset_for_pane_row(
                                terminal_runtimes,
                                *pane_id,
                                mouse.row,
                                *grab_row_offset,
                            ) {
                                self.set_pane_scroll_offset(
                                    terminal_runtimes,
                                    *pane_id,
                                    offset_from_bottom,
                                );
                            }
                        }
                        DragTarget::SidebarDivider => {
                            self.set_manual_sidebar_width(mouse.column);
                        }
                        DragTarget::RightSidebarDivider => {
                            self.set_manual_right_sidebar_width(mouse.column);
                        }
                        DragTarget::SidebarSectionDivider => {
                            self.set_sidebar_section_split(mouse.row);
                        }
                        DragTarget::ReleaseNotesScrollbar { .. }
                        | DragTarget::ProductAnnouncementScrollbar { .. }
                        | DragTarget::KeybindHelpScrollbar { .. }
                        | DragTarget::CommandPaletteScrollbar { .. }
                        | DragTarget::AgentProfilePickerScrollbar { .. }
                        | DragTarget::SettingsThemeScrollbar { .. } => {}
                    }
                }
            }

            MouseEventKind::Up(MouseButton::Left) => {
                // Mouse-up either finishes a drag selection or releases after a
                // double-click copy; the latter is already copied.
                if let Some(selection) = self.selection.as_ref() {
                    let was_click = selection.was_just_click();
                    let was_already_copied = selection.is_done();

                    self.workspace_press = None;
                    self.group_press = None;
                    self.tab_press = None;
                    self.drag = None;
                    self.selection_autoscroll = None;
                    if was_click {
                        self.selection = None;
                    } else if !was_already_copied {
                        self.copy_selection(terminal_runtimes);
                    }
                    return None;
                }

                if self.drag.is_none() {
                    if let Some(info) = self.pane_mouse_target(mouse.column, mouse.row).cloned() {
                        if self.forward_pane_mouse_button(terminal_runtimes, &info, mouse) {
                            self.selection = None;
                            self.selection_autoscroll = None;
                            self.workspace_press = None;
                            self.group_press = None;
                            self.tab_press = None;
                            self.drag = None;
                            return None;
                        }
                    }
                }

                let workspace_press = self.workspace_press.take();
                let group_press = self.group_press.take();
                let tab_press = self.tab_press.take();
                match self.drag.take() {
                    Some(DragState {
                        target:
                            DragTarget::WorkspaceReorder {
                                source_ws_idx,
                                insert_idx: Some(insert_idx),
                                target_group_idx,
                                ..
                            },
                    }) => {
                        let workspace_id =
                            self.workspaces.get(source_ws_idx).map(|ws| ws.id.clone());
                        if let Some(group_idx) = target_group_idx {
                            self.move_workspace_to_group(source_ws_idx, group_idx);
                        }
                        let source_idx = workspace_id
                            .and_then(|id| self.workspaces.iter().position(|ws| ws.id == id))
                            .unwrap_or(source_ws_idx);
                        self.move_workspace(source_idx, insert_idx);
                    }
                    Some(DragState {
                        target:
                            DragTarget::GroupReorder {
                                source_group_idx,
                                insert_idx: Some(insert_idx),
                                ..
                            },
                    }) => {
                        self.move_group(source_group_idx, insert_idx);
                    }
                    Some(DragState {
                        target:
                            DragTarget::TabReorder {
                                ws_idx,
                                source_tab_idx,
                                insert_idx: Some(insert_idx),
                            },
                    }) => {
                        if self.active == Some(ws_idx) {
                            self.move_tab(source_tab_idx, insert_idx);
                            self.mode = Mode::Terminal;
                        }
                    }
                    Some(_) => {}
                    None => {
                        if let Some(press) = workspace_press {
                            self.switch_workspace(press.ws_idx);
                            self.mode = Mode::Terminal;
                            return None;
                        }
                        if let Some(press) = group_press {
                            self.toggle_workspace_group(press.group_idx);
                            self.workspace_scroll = self
                                .workspace_scroll
                                .min(crate::ui::workspace_list_entry_count(self).saturating_sub(1));
                            return None;
                        }
                        if let Some(press) = tab_press {
                            if self.active == Some(press.ws_idx) {
                                self.switch_tab(press.tab_idx);
                                self.mode = Mode::Terminal;
                                return None;
                            }
                        }
                    }
                }
            }

            MouseEventKind::Up(MouseButton::Middle) | MouseEventKind::Drag(MouseButton::Middle)
                if !in_chrome =>
            {
                if let Some(info) = self.pane_mouse_target(mouse.column, mouse.row).cloned() {
                    let _ = self.forward_pane_mouse_button(terminal_runtimes, &info, mouse);
                }
            }

            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                if self.on_tab_bar(mouse.column, mouse.row) =>
            {
                match mouse.kind {
                    MouseEventKind::ScrollUp => self.previous_tab(),
                    MouseEventKind::ScrollDown => self.next_tab(),
                    _ => {}
                }
            }

            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                if !in_chrome && self.scroll_selection_with_wheel(terminal_runtimes, mouse) => {}

            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight
                if !in_chrome =>
            {
                self.selection = None;
                self.selection_autoscroll = None;
                self.handle_terminal_wheel(terminal_runtimes, mouse);
            }

            MouseEventKind::ScrollUp if in_right_sidebar => {
                if !self.handle_native_diff_right_sidebar_wheel(-1) {
                    let agent_area = self.agent_panel_rect();
                    if crate::ui::should_show_scrollbar(crate::ui::agent_panel_scroll_metrics(
                        self,
                        agent_area,
                        self.agent_panel_has_leading_separator(),
                    )) {
                        self.scroll_agent_panel(-1);
                    }
                }
            }
            MouseEventKind::ScrollDown if in_right_sidebar => {
                if !self.handle_native_diff_right_sidebar_wheel(1) {
                    let agent_area = self.agent_panel_rect();
                    if crate::ui::should_show_scrollbar(crate::ui::agent_panel_scroll_metrics(
                        self,
                        agent_area,
                        self.agent_panel_has_leading_separator(),
                    )) {
                        self.scroll_agent_panel(1);
                    }
                }
            }

            MouseEventKind::ScrollUp if in_sidebar => {
                let agent_area = self.agent_panel_rect();
                let over_agent_panel = agent_area != Rect::default()
                    && mouse.row >= agent_area.y
                    && mouse.row < agent_area.y + agent_area.height;
                if over_agent_panel {
                    if crate::ui::should_show_scrollbar(crate::ui::agent_panel_scroll_metrics(
                        self,
                        agent_area,
                        self.agent_panel_has_leading_separator(),
                    )) {
                        self.scroll_agent_panel(-1);
                    }
                } else if crate::ui::should_show_scrollbar(
                    crate::ui::workspace_list_scroll_metrics(self, self.workspace_list_rect()),
                ) {
                    self.scroll_workspace_list(-1);
                } else {
                    let visible = self.sidebar_visible_workspace_indices();
                    if let Some(pos) = visible.iter().position(|idx| *idx == self.selected) {
                        if let Some(prev) = pos.checked_sub(1).and_then(|idx| visible.get(idx)) {
                            self.selected = *prev;
                            self.ensure_workspace_visible(self.selected);
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown if in_sidebar => {
                let agent_area = self.agent_panel_rect();
                let over_agent_panel = agent_area != Rect::default()
                    && mouse.row >= agent_area.y
                    && mouse.row < agent_area.y + agent_area.height;
                if over_agent_panel {
                    if crate::ui::should_show_scrollbar(crate::ui::agent_panel_scroll_metrics(
                        self,
                        agent_area,
                        self.agent_panel_has_leading_separator(),
                    )) {
                        self.scroll_agent_panel(1);
                    }
                } else if crate::ui::should_show_scrollbar(
                    crate::ui::workspace_list_scroll_metrics(self, self.workspace_list_rect()),
                ) {
                    self.scroll_workspace_list(1);
                } else {
                    let visible = self.sidebar_visible_workspace_indices();
                    if let Some(pos) = visible.iter().position(|idx| *idx == self.selected) {
                        if let Some(next) = visible.get(pos + 1) {
                            self.selected = *next;
                            self.ensure_workspace_visible(self.selected);
                        }
                    }
                }
            }

            MouseEventKind::Moved if self.mode == Mode::ContextMenu => {
                let hovered = self.context_menu_item_at(mouse.column, mouse.row);
                if let Some(menu) = &mut self.context_menu {
                    menu.hover(hovered);
                }
            }

            MouseEventKind::Moved if !in_chrome => {
                if let Some(info) = self.pane_at(mouse.column, mouse.row).cloned() {
                    let _ = self.forward_pane_mouse_motion(terminal_runtimes, &info, mouse);
                }
            }

            MouseEventKind::Down(MouseButton::Right) if in_sidebar && !self.sidebar_collapsed => {
                if self
                    .workspace_list_scrollbar_target_at(mouse.column, mouse.row)
                    .is_some()
                {
                    return None;
                }
                if let Some(group_idx) = self.workspace_group_header_at_row(mouse.row) {
                    self.context_menu = Some(ContextMenuState {
                        kind: ContextMenuKind::Group {
                            group_idx,
                            can_delete: self.groups.len() > 1,
                        },
                        x: mouse.column,
                        y: mouse.row,
                        list: MenuListState::new(1),
                    });
                    self.mode = Mode::ContextMenu;
                    return None;
                }
                if let Some(idx) = self.workspace_at_row(mouse.row) {
                    self.selected = idx;
                    let can_diff = !self
                        .observed_git_repos_for_workspace(terminal_runtimes, idx)
                        .is_empty();
                    self.context_menu = Some(ContextMenuState {
                        kind: ContextMenuKind::Workspace {
                            ws_idx: idx,
                            can_diff,
                        },
                        x: mouse.column,
                        y: mouse.row,
                        list: MenuListState::new(1),
                    });
                    self.mode = Mode::ContextMenu;
                }
            }

            MouseEventKind::Down(MouseButton::Right)
                if self.tab_at(mouse.column, mouse.row).is_some() =>
            {
                if let (Some(ws_idx), Some(tab_idx)) =
                    (self.active, self.tab_at(mouse.column, mouse.row))
                {
                    self.switch_tab(tab_idx);
                    let can_diff = !self
                        .observed_git_repos_for_workspace(terminal_runtimes, ws_idx)
                        .is_empty();
                    self.context_menu = Some(ContextMenuState {
                        kind: ContextMenuKind::Tab {
                            ws_idx,
                            tab_idx,
                            can_diff,
                        },
                        x: mouse.column,
                        y: mouse.row,
                        list: MenuListState::new(1),
                    });
                    self.mode = Mode::ContextMenu;
                }
            }

            MouseEventKind::Down(MouseButton::Right) if !in_chrome => {
                if let Some(info) = self.pane_mouse_target(mouse.column, mouse.row).cloned() {
                    self.focus_pane(info.id);
                    let ws_idx = self.active.unwrap_or(self.selected);
                    let is_native_diff = self
                        .workspaces
                        .get(ws_idx)
                        .and_then(|ws| ws.pane_state(info.id))
                        .and_then(|pane| pane.native_diff())
                        .is_some();
                    if is_native_diff {
                        let hunk_target = self.handle_native_diff_mouse_down(&info, mouse);
                        self.context_menu = Some(ContextMenuState {
                            kind: ContextMenuKind::NativeDiff {
                                ws_idx,
                                pane_id: info.id,
                                hunk_target,
                            },
                            x: mouse.column,
                            y: mouse.row,
                            list: MenuListState::new(0),
                        });
                        self.mode = Mode::ContextMenu;
                        return None;
                    }
                    let has_manual_label = self
                        .active
                        .and_then(|ws_idx| self.workspaces.get(ws_idx))
                        .and_then(|ws| ws.pane_state(info.id))
                        .and_then(|pane| self.terminals.get(&pane.attached_terminal_id))
                        .and_then(|terminal| terminal.manual_label.as_ref())
                        .is_some();
                    self.context_menu = Some(ContextMenuState {
                        kind: ContextMenuKind::Pane {
                            ws_idx,
                            pane_id: info.id,
                            has_manual_label,
                        },
                        x: mouse.column,
                        y: mouse.row,
                        list: MenuListState::new(0),
                    });
                    self.mode = Mode::ContextMenu;
                }
            }

            _ => {}
        }

        None
    }

    fn handle_mobile_mouse(&mut self, mouse: MouseEvent) -> bool {
        if self.mode == Mode::Navigate {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_mobile_switcher_at(mouse.column, mouse.row, -1);
                    return true;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_mobile_switcher_at(mouse.column, mouse.row, 1);
                    return true;
                }
                MouseEventKind::Down(MouseButton::Left) => {}
                _ => return true,
            }
        } else if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return false;
        }

        if self.mode != Mode::Navigate {
            if !matches!(self.mode, Mode::Terminal | Mode::Resize) {
                return false;
            }
            if rect_contains(self.view.mobile_menu_hit_area, mouse.column, mouse.row) {
                self.mobile_switcher_scroll = 0;
                self.mode = Mode::Navigate;
                return true;
            }
            return false;
        }

        let areas = crate::ui::mobile_switcher_areas(self);
        if rect_contains(areas.close, mouse.column, mouse.row) {
            self.mode = Mode::Terminal;
            return true;
        }

        match crate::ui::mobile_switcher_target_at(self, mouse.column, mouse.row) {
            Some(crate::ui::MobileSwitcherTarget::NewWorkspace) => {
                self.request_new_workspace = true;
            }
            Some(crate::ui::MobileSwitcherTarget::Workspace(ws_idx)) => {
                self.switch_workspace(ws_idx);
                self.mode = Mode::Terminal;
            }
            Some(crate::ui::MobileSwitcherTarget::NewTab) => {
                request_new_tab_from_ui(self);
            }
            Some(crate::ui::MobileSwitcherTarget::Tab(tab_idx)) => {
                self.switch_tab(tab_idx);
                self.mode = Mode::Terminal;
            }
            Some(crate::ui::MobileSwitcherTarget::Agent {
                ws_idx,
                tab_idx,
                pane_id,
            }) => {
                self.switch_workspace(ws_idx);
                self.switch_tab(tab_idx);
                self.focus_pane(pane_id);
                self.mode = Mode::Terminal;
            }
            Some(crate::ui::MobileSwitcherTarget::Menu(action_idx)) => {
                let actions = global_menu_actions(self);
                if let Some(action) = actions.get(action_idx).copied() {
                    apply_global_menu_action(self, action);
                }
            }
            None => {}
        }

        true
    }

    fn scroll_mobile_switcher_at(&mut self, _col: u16, _row: u16, delta: i16) {
        let max_scroll = crate::ui::mobile_switcher_max_scroll(self);
        apply_scroll(
            &mut self.mobile_switcher_scroll,
            delta.saturating_mul(2),
            max_scroll,
        );
    }

    pub(crate) fn screen_rect(&self) -> Rect {
        let sidebar = self.view.sidebar_rect;
        let right_sidebar = self.view.right_sidebar_rect;
        let terminal = self.view.terminal_area;
        let mobile_header = self.view.mobile_header_rect;
        let x = sidebar
            .x
            .min(terminal.x)
            .min(right_sidebar.x)
            .min(mobile_header.x);
        let y = sidebar
            .y
            .min(terminal.y)
            .min(right_sidebar.y)
            .min(mobile_header.y);
        let right = (sidebar.x + sidebar.width)
            .max(terminal.x + terminal.width)
            .max(right_sidebar.x + right_sidebar.width)
            .max(mobile_header.x + mobile_header.width);
        let bottom = (sidebar.y + sidebar.height)
            .max(terminal.y + terminal.height)
            .max(right_sidebar.y + right_sidebar.height)
            .max(mobile_header.y + mobile_header.height);
        Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }

    pub(crate) fn context_menu_rect(&self) -> Option<Rect> {
        let menu = self.context_menu.as_ref()?;
        let screen = self.screen_rect();
        let max_item_w = menu
            .items()
            .iter()
            .map(|item| item.len() as u16)
            .max()
            .unwrap_or(0);
        let menu_w = (max_item_w + 4).max(14).min(screen.width.max(1));
        let menu_h = (menu.items().len() as u16 + 2).min(screen.height.max(1));
        let x = menu.x.min(screen.x + screen.width.saturating_sub(menu_w));
        let y = menu.y.min(screen.y + screen.height.saturating_sub(menu_h));
        Some(Rect::new(x, y, menu_w, menu_h))
    }

    pub(crate) fn confirm_close_rect(&self) -> Rect {
        crate::ui::confirm_close_popup_rect(self.view.terminal_area).unwrap_or_default()
    }

    fn context_menu_item_at(&self, col: u16, row: u16) -> Option<usize> {
        let menu_rect = self.context_menu_rect()?;
        let inner_x = menu_rect.x + 1;
        let inner_y = menu_rect.y + 1;
        let inner_w = menu_rect.width.saturating_sub(2);
        let inner_h = menu_rect.height.saturating_sub(2);
        let item_count = self
            .context_menu
            .as_ref()
            .map(|menu| menu.items().len() as u16)
            .unwrap_or(0);
        if col >= inner_x
            && col < inner_x + inner_w
            && row >= inner_y
            && row < inner_y + inner_h.min(item_count)
        {
            Some((row - inner_y) as usize)
        } else {
            None
        }
    }

    pub(super) fn tab_at(&self, col: u16, row: u16) -> Option<usize> {
        self.view
            .tab_hit_areas
            .iter()
            .enumerate()
            .find_map(|(idx, area)| {
                (area.width > 0
                    && row >= area.y
                    && row < area.y + area.height
                    && col >= area.x
                    && col < area.x + area.width)
                    .then_some(idx)
            })
    }

    pub(super) fn on_tab_bar(&self, col: u16, row: u16) -> bool {
        let area = self.view.tab_bar_rect;
        area.width > 0
            && row >= area.y
            && row < area.y + area.height
            && col >= area.x
            && col < area.x + area.width
    }

    pub(super) fn on_tab_scroll_left_button(&self, col: u16, row: u16) -> bool {
        let area = self.view.tab_scroll_left_hit_area;
        area.width > 0
            && row >= area.y
            && row < area.y + area.height
            && col >= area.x
            && col < area.x + area.width
    }

    pub(super) fn on_tab_scroll_right_button(&self, col: u16, row: u16) -> bool {
        let area = self.view.tab_scroll_right_hit_area;
        area.width > 0
            && row >= area.y
            && row < area.y + area.height
            && col >= area.x
            && col < area.x + area.width
    }

    pub(super) fn tab_close_button_at(&self, col: u16, row: u16) -> Option<usize> {
        self.view
            .tab_close_hit_areas
            .iter()
            .enumerate()
            .find_map(|(idx, area)| {
                (area.width > 0
                    && row >= area.y
                    && row < area.y + area.height
                    && col >= area.x
                    && col < area.x + area.width)
                    .then_some(idx)
            })
    }

    pub(super) fn tab_drop_index_at(&self, col: u16, row: u16) -> Option<usize> {
        if !self.on_tab_bar(col, row) {
            return None;
        }

        let visible_tabs: Vec<_> = self
            .view
            .tab_hit_areas
            .iter()
            .enumerate()
            .filter(|(_, rect)| rect.width > 0)
            .collect();
        let (first_idx, first_rect) = *visible_tabs.first()?;
        let (last_idx, last_rect) = *visible_tabs.last()?;

        if self.on_tab_scroll_left_button(col, row) {
            return Some(0);
        }
        if self.on_tab_scroll_right_button(col, row) {
            return self
                .active
                .and_then(|idx| self.workspaces.get(idx))
                .map(|ws| ws.tabs.len());
        }

        let left_edge = if first_idx == 0 {
            first_rect.x
        } else {
            self.view.tab_scroll_left_hit_area.x + self.view.tab_scroll_left_hit_area.width
        };
        let right_edge = if self
            .active
            .and_then(|idx| self.workspaces.get(idx))
            .is_some_and(|ws| last_idx + 1 >= ws.tabs.len())
        {
            last_rect.x + last_rect.width
        } else {
            self.view.tab_scroll_right_hit_area.x.saturating_sub(1)
        };

        if col <= left_edge {
            return Some(first_idx);
        }
        if col >= right_edge {
            return Some(last_idx + 1);
        }

        for (idx, rect) in visible_tabs {
            let midpoint = rect.x + rect.width / 2;
            if col < midpoint {
                return Some(idx);
            }
            if col < rect.x + rect.width {
                return Some(idx + 1);
            }
        }

        Some(last_idx + 1)
    }

    pub(super) fn on_new_tab_button(&self, col: u16, row: u16) -> bool {
        let area = self.view.new_tab_hit_area;
        area.width > 0
            && row >= area.y
            && row < area.y + area.height
            && col >= area.x
            && col < area.x + area.width
    }

    pub(super) fn find_border_at(&self, col: u16, row: u16) -> Option<&SplitBorder> {
        self.view.split_borders.iter().find(|b| match b.direction {
            Direction::Horizontal => {
                col >= b.pos.saturating_sub(1)
                    && col <= b.pos
                    && row >= b.area.y
                    && row < b.area.y + b.area.height
            }
            Direction::Vertical => {
                row >= b.pos.saturating_sub(1)
                    && row <= b.pos
                    && col >= b.area.x
                    && col < b.area.x + b.area.width
            }
        })
    }

    pub(super) fn pane_at(&self, col: u16, row: u16) -> Option<&PaneInfo> {
        self.view.pane_infos.iter().find(|p| {
            col >= p.inner_rect.x
                && col < p.inner_rect.x + p.inner_rect.width
                && row >= p.inner_rect.y
                && row < p.inner_rect.y + p.inner_rect.height
        })
    }

    pub(super) fn pane_mouse_target(&self, col: u16, row: u16) -> Option<&PaneInfo> {
        self.pane_at(col, row)
            .or_else(|| self.pane_frame_at(col, row))
    }

    pub(crate) fn pane_info_by_id(&self, pane_id: crate::layout::PaneId) -> Option<&PaneInfo> {
        self.view.pane_infos.iter().find(|info| info.id == pane_id)
    }

    pub(super) fn pane_frame_at(&self, col: u16, row: u16) -> Option<&PaneInfo> {
        self.view.pane_infos.iter().find(|p| {
            col >= p.rect.x
                && col < p.rect.x + p.rect.width
                && row >= p.rect.y
                && row < p.rect.y + p.rect.height
        })
    }

    pub(crate) fn focus_pane(&mut self, pane_id: crate::layout::PaneId) {
        let Some(ws_idx) = self.active else {
            return;
        };
        let previous = self.current_pane_focus_target();
        if let Some(tab) = self
            .workspaces
            .get_mut(ws_idx)
            .and_then(|ws| ws.active_tab_mut())
        {
            if tab.layout.focused() != pane_id {
                tab.layout.focus_pane(pane_id);
                self.record_pane_focus_change(previous, ws_idx, pane_id);
                self.mark_session_dirty();
            }
        }
    }

    fn clickable_toast_at(&self, col: u16, row: u16) -> bool {
        self.toast
            .as_ref()
            .is_some_and(|toast| toast.target.is_some())
            && rect_contains(self.view.toast_hit_area, col, row)
    }

    pub(crate) fn focus_toast_target(&mut self) {
        let Some(target) = self.toast.as_ref().and_then(|toast| toast.target.clone()) else {
            return;
        };
        let Some(ws_idx) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == target.workspace_id)
        else {
            return;
        };
        let Some(tab_idx) = self.workspaces[ws_idx].find_tab_index_for_pane(target.pane_id) else {
            return;
        };

        self.switch_workspace(ws_idx);
        self.switch_tab(tab_idx);
        self.focus_pane(target.pane_id);
        self.toast = None;
        self.mode = Mode::Terminal;
    }

    pub(crate) fn scroll_pane_up(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        lines: usize,
    ) {
        if let Some(ws_idx) = self.active {
            if let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
            {
                rt.scroll_up(lines);
            }
        }
    }

    pub(crate) fn scroll_pane_down(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        lines: usize,
    ) {
        if let Some(ws_idx) = self.active {
            if let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
            {
                rt.scroll_down(lines);
            }
        }
    }

    pub(crate) fn pane_scroll_metrics(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
    ) -> Option<crate::pane::ScrollMetrics> {
        self.active
            .and_then(|i| self.runtime_for_pane_in_workspace(terminal_runtimes, i, pane_id))
            .and_then(crate::terminal::TerminalRuntime::scroll_metrics)
    }

    pub(super) fn handle_terminal_wheel(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        mouse: MouseEvent,
    ) {
        let lines_per_notch = self.mouse_scroll_lines;

        if let Some(info) = self.pane_at(mouse.column, mouse.row).cloned() {
            self.focus_pane(info.id);
            if self.handle_native_diff_wheel(&info, mouse) {
                return;
            }

            if self.forward_pane_wheel(terminal_runtimes, &info, mouse) {
                return;
            }
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_pane_up(terminal_runtimes, info.id, lines_per_notch)
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_pane_down(terminal_runtimes, info.id, lines_per_notch)
                }
                _ => {}
            }
            return;
        }

        if let Some(info) = self.pane_frame_at(mouse.column, mouse.row).cloned() {
            self.focus_pane(info.id);
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_pane_up(terminal_runtimes, info.id, lines_per_notch)
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_pane_down(terminal_runtimes, info.id, lines_per_notch)
                }
                _ => {}
            }
            return;
        }

        if let Some(ws_idx) = self.active {
            if let Some(rt) = self.focused_runtime_in_workspace(terminal_runtimes, ws_idx) {
                match mouse.kind {
                    MouseEventKind::ScrollUp => rt.scroll_up(lines_per_notch),
                    MouseEventKind::ScrollDown => rt.scroll_down(lines_per_notch),
                    _ => {}
                }
            }
        }
    }

    fn focused_native_diff_mut(&mut self) -> Option<&mut crate::native_diff::NativeDiffPaneState> {
        let ws_idx = self.active?;
        let workspace = self.workspaces.get_mut(ws_idx)?;
        let pane_id = workspace.focused_pane_id()?;
        workspace.pane_state_mut(pane_id)?.native_diff_mut()
    }

    fn handle_native_diff_right_sidebar_click(&mut self, mouse: MouseEvent) -> bool {
        let content = crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect);
        if content == Rect::default()
            || mouse.row <= content.y
            || mouse.row >= content.y + content.height
        {
            return false;
        }
        let row = mouse.row.saturating_sub(content.y + 1) as usize;
        self.focused_native_diff_mut()
            .is_some_and(|diff| diff.select_visible_file_row(row))
    }

    fn handle_native_diff_right_sidebar_wheel(&mut self, delta: isize) -> bool {
        let content = crate::ui::right_sidebar_content_rect(self.view.right_sidebar_rect);
        if content.height <= 1 {
            return false;
        }
        self.focused_native_diff_mut().is_some_and(|diff| {
            diff.scroll_file_list(delta, content.height.saturating_sub(1) as usize);
            true
        })
    }

    pub(super) fn handle_native_diff_mouse_down(
        &mut self,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(diff) = self
            .active
            .and_then(|ws_idx| self.workspaces.get_mut(ws_idx))
            .and_then(|workspace| workspace.pane_state_mut(info.id))
            .and_then(|pane| pane.native_diff_mut())
        else {
            return false;
        };
        let file_width = if diff.show_file_list && self.view.right_sidebar_rect == Rect::default() {
            crate::native_diff::native_diff_file_list_width(info.inner_rect.width)
        } else {
            0
        };
        let local_col = mouse.column.saturating_sub(info.inner_rect.x);
        let local_row = mouse.row.saturating_sub(info.inner_rect.y);
        let file_list_start = info.inner_rect.width.saturating_sub(file_width);
        if local_row >= info.inner_rect.height.saturating_sub(1) {
            return true;
        }
        if file_width > 0 && local_col >= file_list_start {
            let selected = diff.select_visible_file_row(local_row as usize);
            return selected && !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right));
        }
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right)) {
            let selected = diff.select_visible_diff_row(local_row as usize);
            if !selected {
                diff.clear_selected_hunk();
            }
            return selected;
        }
        if !diff.toggle_visible_context_row(local_row as usize) {
            diff.select_visible_diff_row(local_row as usize);
        }
        true
    }

    pub(super) fn handle_native_diff_wheel(&mut self, info: &PaneInfo, mouse: MouseEvent) -> bool {
        let line_numbers = self.native_diff_line_numbers;
        let Some(diff) = self
            .active
            .and_then(|ws_idx| self.workspaces.get_mut(ws_idx))
            .and_then(|workspace| workspace.pane_state_mut(info.id))
            .and_then(|pane| pane.native_diff_mut())
        else {
            return false;
        };
        let horizontal = matches!(
            mouse.kind,
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight
        ) || mouse
            .modifiers
            .contains(crossterm::event::KeyModifiers::SHIFT);
        let delta = match mouse.kind {
            MouseEventKind::ScrollUp if horizontal => -8,
            MouseEventKind::ScrollDown if horizontal => 8,
            MouseEventKind::ScrollUp => -3,
            MouseEventKind::ScrollDown => 3,
            MouseEventKind::ScrollLeft => -8,
            MouseEventKind::ScrollRight => 8,
            _ => return false,
        };
        let file_width = if diff.show_file_list && self.view.right_sidebar_rect == Rect::default() {
            crate::native_diff::native_diff_file_list_width(info.inner_rect.width)
        } else {
            0
        };
        let local_col = mouse.column.saturating_sub(info.inner_rect.x);
        let file_list_start = info.inner_rect.width.saturating_sub(file_width);
        if file_width > 0 && local_col >= file_list_start {
            if horizontal {
                return true;
            }
            diff.scroll_file_list(delta, info.inner_rect.height.saturating_sub(1) as usize);
        } else if horizontal {
            let viewport_cols = Self::native_diff_mouse_col_viewport(
                diff,
                info.inner_rect.width,
                info.inner_rect.height.saturating_sub(2) as usize,
                line_numbers,
                self.view.right_sidebar_rect == Rect::default(),
            );
            diff.scroll_diff_columns(delta, viewport_cols);
        } else {
            diff.scroll_diff(delta, info.inner_rect.height.saturating_sub(2) as usize);
        }
        true
    }

    fn native_diff_mouse_col_viewport(
        diff: &crate::native_diff::NativeDiffPaneState,
        pane_width: u16,
        diff_viewport_rows: usize,
        line_numbers: bool,
        show_inline_file_list: bool,
    ) -> usize {
        let file_width = if diff.show_file_list && show_inline_file_list {
            crate::native_diff::native_diff_file_list_width(pane_width)
        } else {
            0
        };
        let patch_width = pane_width
            .saturating_sub(file_width)
            .saturating_sub(u16::from(file_width > 0));
        let gutter_width = if line_numbers {
            diff.selected_file()
                .map(crate::ui::native_diff_line_number_gutter_width)
                .unwrap_or(4)
        } else {
            0
        };
        let body_rows = diff.visible_diff_rows().len().saturating_sub(1);
        let horizontal_scrollbar_rows = usize::from(diff.max_diff_col_scroll(1) > 0);
        let effective_rows = diff_viewport_rows
            .saturating_sub(horizontal_scrollbar_rows)
            .max(1);
        let patch_width = patch_width.saturating_sub(u16::from(body_rows > effective_rows));
        let split = match diff.view_mode {
            crate::native_diff::NativeDiffViewMode::Unified => false,
            crate::native_diff::NativeDiffViewMode::Split => true,
            crate::native_diff::NativeDiffViewMode::Auto => patch_width >= 110,
        };
        if split {
            let half = patch_width as usize / 2;
            let left = half.saturating_sub(gutter_width + 4);
            let right = (patch_width as usize)
                .saturating_sub(half)
                .saturating_sub(gutter_width + 5);
            left.min(right)
        } else {
            (patch_width as usize).saturating_sub(gutter_width * 2 + 4)
        }
        .max(1)
    }

    pub(super) fn forward_pane_mouse_button(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        let column = mouse.column.saturating_sub(info.inner_rect.x);
        let row = mouse.row.saturating_sub(info.inner_rect.y);
        let Some(bytes) = rt.encode_mouse_button(mouse.kind, column, row, mouse.modifiers) else {
            return false;
        };
        rt.scroll_reset();
        if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
            warn!(pane = info.id.raw(), err = %err, kind = ?mouse.kind, "failed to forward mouse button event");
        }
        true
    }

    pub(super) fn forward_pane_mouse_motion(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        let column = mouse.column.saturating_sub(info.inner_rect.x);
        let row = mouse.row.saturating_sub(info.inner_rect.y);
        let Some(bytes) = rt.encode_mouse_motion(mouse.kind, column, row, mouse.modifiers) else {
            return false;
        };
        if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
            warn!(pane = info.id.raw(), err = %err, kind = ?mouse.kind, "failed to forward mouse motion event");
        }
        true
    }

    fn forward_pane_reported_wheel(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        if !rt
            .input_state()
            .is_some_and(crate::pane::InputState::mouse_reporting_enabled)
        {
            return false;
        }
        rt.scroll_reset();
        let column = mouse.column.saturating_sub(info.inner_rect.x);
        let row = mouse.row.saturating_sub(info.inner_rect.y);
        let Some(bytes) = rt.encode_mouse_wheel(mouse.kind, column, row, mouse.modifiers) else {
            warn!(pane = info.id.raw(), kind = ?mouse.kind, "failed to encode mouse wheel event");
            return true;
        };
        if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
            warn!(pane = info.id.raw(), err = %err, "failed to forward mouse wheel event");
        }
        true
    }

    pub(super) fn forward_pane_wheel(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        info: &PaneInfo,
        mouse: MouseEvent,
    ) -> bool {
        let Some(ws_idx) = self.active else {
            return false;
        };
        let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)
        else {
            return false;
        };
        match rt.wheel_routing() {
            Some(crate::pane::WheelRouting::HostScroll) | None => false,
            Some(crate::pane::WheelRouting::MouseReport) => {
                rt.scroll_reset();
                let column = mouse.column.saturating_sub(info.inner_rect.x);
                let row = mouse.row.saturating_sub(info.inner_rect.y);
                let Some(bytes) = rt.encode_mouse_wheel(mouse.kind, column, row, mouse.modifiers)
                else {
                    warn!(pane = info.id.raw(), kind = ?mouse.kind, "failed to encode mouse wheel event");
                    return true;
                };
                if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
                    warn!(pane = info.id.raw(), err = %err, "failed to forward mouse wheel event");
                }
                true
            }
            Some(crate::pane::WheelRouting::AlternateScroll) => {
                rt.scroll_reset();
                let Some(bytes) = rt.encode_alternate_scroll(mouse.kind) else {
                    return true;
                };
                if let Err(err) = rt.try_send_bytes(Bytes::from(bytes)) {
                    warn!(pane = info.id.raw(), err = %err, "failed to forward alternate-scroll key");
                }
                true
            }
        }
    }

    fn handle_right_click_passthrough(
        &mut self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        mouse: MouseEvent,
        in_chrome: bool,
    ) -> bool {
        if let Some(gesture) = self.right_click_passthrough.clone() {
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Right)
                | MouseEventKind::Up(MouseButton::Right) => {
                    let forwarded_mouse =
                        self.strip_right_click_passthrough_modifiers(mouse, gesture.modifiers);
                    let _ = self.forward_pane_mouse_button(
                        terminal_runtimes,
                        &gesture.pane_info,
                        forwarded_mouse,
                    );
                    if matches!(mouse.kind, MouseEventKind::Up(MouseButton::Right)) {
                        self.right_click_passthrough = None;
                    }
                    return true;
                }
                _ => {
                    self.right_click_passthrough = None;
                }
            }
        }

        if self.mode != Mode::Terminal
            || in_chrome
            || !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Right))
        {
            return false;
        }

        let Some(modifiers) = self.right_click_passthrough_modifiers else {
            return false;
        };
        if mouse.modifiers != modifiers {
            return false;
        }

        let Some(info) = self.pane_at(mouse.column, mouse.row).cloned() else {
            return false;
        };

        self.focus_pane(info.id);
        let forwarded_mouse = self.strip_right_click_passthrough_modifiers(mouse, modifiers);
        if !self.forward_pane_mouse_button(terminal_runtimes, &info, forwarded_mouse) {
            return false;
        }

        self.selection = None;
        self.selection_autoscroll = None;
        self.workspace_press = None;
        self.group_press = None;
        self.tab_press = None;
        self.drag = None;
        self.context_menu = None;
        self.right_click_passthrough = Some(RightClickPassthroughGesture {
            pane_info: info,
            modifiers,
        });
        true
    }

    fn strip_right_click_passthrough_modifiers(
        &self,
        mouse: MouseEvent,
        modifiers: KeyModifiers,
    ) -> MouseEvent {
        MouseEvent {
            modifiers: mouse.modifiers.difference(modifiers),
            ..mouse
        }
    }
    pub(super) fn set_pane_scroll_offset(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        offset_from_bottom: usize,
    ) {
        if let Some(ws_idx) = self.active {
            if let Some(rt) = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)
            {
                rt.set_scroll_offset_from_bottom(offset_from_bottom);
            }
        }
    }

    pub(super) fn scrollbar_target_at(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        col: u16,
        row: u16,
    ) -> Option<(crate::layout::PaneId, ScrollbarClickTarget)> {
        let ws_idx = self.active?;
        let info = self.view.pane_infos.iter().find(|info| {
            crate::ui::pane_scrollbar_rect(info).is_some_and(|track| {
                col >= track.x
                    && col < track.x + track.width
                    && row >= track.y
                    && row < track.y + track.height
            })
        })?;
        let rt = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, info.id)?;
        let metrics = rt.scroll_metrics()?;
        if metrics.max_offset_from_bottom == 0 {
            return None;
        }
        let track = crate::ui::pane_scrollbar_rect(info)?;
        if let Some(grab_row_offset) = crate::ui::scrollbar_thumb_grab_offset(metrics, track, row) {
            Some((info.id, ScrollbarClickTarget::Thumb { grab_row_offset }))
        } else {
            Some((
                info.id,
                ScrollbarClickTarget::Track {
                    offset_from_bottom: crate::ui::scrollbar_offset_from_row(metrics, track, row),
                },
            ))
        }
    }

    pub(super) fn scrollbar_offset_for_pane_row(
        &self,
        terminal_runtimes: &TerminalRuntimeRegistry,
        pane_id: crate::layout::PaneId,
        row: u16,
        grab_row_offset: u16,
    ) -> Option<usize> {
        let ws_idx = self.active?;
        let info = self
            .view
            .pane_infos
            .iter()
            .find(|info| info.id == pane_id)?;
        let track = crate::ui::pane_scrollbar_rect(info)?;
        let rt = self.runtime_for_pane_in_workspace(terminal_runtimes, ws_idx, pane_id)?;
        let metrics = rt.scroll_metrics()?;
        if metrics.max_offset_from_bottom == 0 {
            return None;
        }
        Some(crate::ui::scrollbar_offset_from_drag_row(
            metrics,
            track,
            row,
            grab_row_offset,
        ))
    }
}

#[cfg(test)]
pub(super) fn wheel_routing(input_state: crate::pane::InputState) -> WheelRouting {
    if input_state.mouse_protocol_mode.reporting_enabled() {
        WheelRouting::MouseReport
    } else if input_state.alternate_screen && input_state.mouse_alternate_scroll {
        WheelRouting::AlternateScroll
    } else {
        WheelRouting::HostScroll
    }
}

fn rect_contains(rect: Rect, col: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && col >= rect.x
        && col < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

fn apply_scroll(scroll: &mut usize, delta: i16, max_scroll: usize) {
    if delta.is_negative() {
        *scroll = scroll.saturating_sub(delta.unsigned_abs() as usize);
    } else {
        *scroll = scroll.saturating_add(delta as usize).min(max_scroll);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
    use ratatui::{
        backend::TestBackend,
        layout::{Direction, Rect},
        Terminal,
    };

    use super::super::{
        app_for_mouse_test, capture_snapshot, handle_context_menu_key, mouse, numbered_lines_bytes,
        root_layout_ratio,
    };
    use super::*;
    use crate::{
        app::state::{ContextMenuKind, ContextMenuState, MenuListState, Mode, ViewLayout},
        detect::{Agent, AgentState},
        workspace::Workspace,
    };

    fn native_diff_file(path: &str) -> crate::native_diff::NativeDiffFile {
        crate::native_diff::NativeDiffFile {
            bucket: crate::native_diff::DiffBucket::Changed,
            old_path: Some(std::path::PathBuf::from(path)),
            new_path: Some(std::path::PathBuf::from(path)),
            status: crate::native_diff::DiffFileStatus::Modified,
            added: 1,
            deleted: 0,
            hunks: Vec::new(),
            binary: false,
        }
    }

    #[tokio::test]
    async fn native_diff_click_selects_visible_file_row() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        ws.create_native_diff_tab(crate::native_diff::NativeDiffSession {
            repo_root: std::path::PathBuf::from("/tmp/repo"),
            files: vec![native_diff_file("first.rs"), native_diff_file("second.rs")],
        })
        .unwrap();
        let pane_id = ws.active_tab().unwrap().root_pane;
        let pane_infos = ws
            .active_tab()
            .unwrap()
            .layout
            .panes(Rect::new(26, 2, 80, 18));
        let info = pane_infos[0].clone();
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.view.pane_infos = pane_infos;

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            info.inner_rect.x + info.inner_rect.width
                - crate::native_diff::native_diff_file_list_width(info.inner_rect.width)
                + 2,
            info.inner_rect.y + 2,
        ));

        let diff = app.state.workspaces[0]
            .pane_state(pane_id)
            .unwrap()
            .native_diff()
            .unwrap();
        assert_eq!(
            diff.selected_path().unwrap(),
            std::path::PathBuf::from("second.rs")
        );
        assert!(app.state.selection.is_none());
    }

    #[tokio::test]
    async fn terminal_wheel_uses_configured_mouse_scroll_lines() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let pane_infos = ws.tabs[0].layout.panes(Rect::new(26, 2, 80, 18));
        let info = pane_infos[0].clone();
        ws.tabs[0].runtimes.insert(
            pane_id,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                info.inner_rect.width,
                info.inner_rect.height,
                16 * 1024,
                &numbered_lines_bytes(64),
            ),
        );

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.view.pane_infos = pane_infos;
        app.state.mouse_scroll_lines = 7;

        app.handle_mouse(mouse(
            MouseEventKind::ScrollUp,
            info.inner_rect.x + 1,
            info.inner_rect.y + 1,
        ));

        let metrics = app
            .state
            .runtime_for_pane_in_workspace(&app.terminal_runtimes, 0, pane_id)
            .and_then(crate::terminal::TerminalRuntime::scroll_metrics)
            .expect("scroll metrics after wheel");
        assert_eq!(metrics.offset_from_bottom, 7);
    }

    #[test]
    fn hovering_context_menu_updates_highlight() {
        let mut app = app_for_mouse_test();
        app.state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 0,
                can_diff: false,
            },
            x: 2,
            y: 2,
            list: MenuListState::new(0),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(MouseEventKind::Moved, menu.x + 2, menu.y + 2));

        assert_eq!(app.state.context_menu.unwrap().list.highlighted, 1);
    }

    #[test]
    fn command_palette_mouse_wheel_scrolls_rows_without_changing_selection() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
        assert_eq!(app.state.command_palette.selected, 0);
        assert_eq!(app.state.command_palette.scroll, 3);

        app.handle_mouse(mouse(MouseEventKind::ScrollUp, 40, 8));
        assert_eq!(app.state.command_palette.selected, 0);
        assert_eq!(app.state.command_palette.scroll, 0);
    }

    #[test]
    fn command_palette_mouse_wheel_clamps_scroll_at_bounds() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        app.handle_mouse(mouse(MouseEventKind::ScrollUp, 40, 8));
        assert_eq!(app.state.command_palette.selected, 0);
        assert_eq!(app.state.command_palette.scroll, 0);

        for _ in 0..100 {
            app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
        }
        let scroll = app.state.command_palette.scroll;
        assert!(scroll > 0);

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
        assert_eq!(app.state.command_palette.scroll, scroll);
        assert_eq!(app.state.command_palette.selected, 0);
    }

    fn rendered_text_point(app: &crate::app::App, text: &str) -> (u16, u16) {
        let screen = app.state.screen_rect();
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render app");
        let buffer = terminal.backend().buffer();
        let symbols = text.chars().map(|ch| ch.to_string()).collect::<Vec<_>>();
        let text_width = symbols.len() as u16;

        for y in 0..screen.height {
            for x in 0..=screen.width.saturating_sub(text_width) {
                if symbols
                    .iter()
                    .enumerate()
                    .all(|(idx, symbol)| buffer[(x + idx as u16, y)].symbol() == symbol.as_str())
                {
                    return (x, y);
                }
            }
        }

        panic!("rendered text not found: {text}");
    }

    #[test]
    fn command_palette_hover_selects_visible_command() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        let first = rendered_text_point(&app, "new space");
        app.handle_mouse(mouse(MouseEventKind::Moved, first.0, first.1));
        assert_eq!(app.state.command_palette.selected, 0);

        let second = rendered_text_point(&app, "rename selected space");
        app.handle_mouse(mouse(MouseEventKind::Moved, second.0, second.1));
        assert_eq!(app.state.command_palette.selected, 1);
    }

    #[test]
    fn command_palette_hover_takes_precedence_after_scroll() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
        let scroll = app.state.command_palette.scroll;
        assert_eq!(app.state.command_palette.selected, 0);

        app.handle_mouse(mouse(MouseEventKind::Moved, 18, 6));
        assert!(app.state.command_palette.selected > 0);
        assert_eq!(app.state.command_palette.scroll, scroll);

        let selected = app.state.command_palette.selected;
        app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
        assert_eq!(app.state.command_palette.selected, selected);
        assert_eq!(app.state.command_palette.scroll, scroll + 3);
    }

    #[test]
    fn command_palette_hover_does_not_shift_scrolled_page() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        for _ in 0..10 {
            app.handle_mouse(mouse(MouseEventKind::ScrollDown, 40, 8));
        }
        assert!(app.state.command_palette.scroll > 0);
        let scroll = app.state.command_palette.scroll;

        app.handle_mouse(mouse(MouseEventKind::Moved, 18, 6));

        assert_eq!(app.state.command_palette.scroll, scroll);
    }

    #[test]
    fn command_palette_clicking_outside_closes() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0));

        assert_eq!(app.state.mode, Mode::Navigate);
    }

    #[test]
    fn command_palette_scrollbar_drag_moves_options() {
        let mut app = app_for_mouse_test();
        app.state.mode = Mode::CommandPalette;

        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 89, 5));
        app.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 89, 17));

        assert!(app.state.command_palette.scroll > 0);
        assert_eq!(app.state.command_palette.selected, 0);

        app.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 89, 17));
        assert!(app.state.drag.is_none());
    }

    #[test]
    fn clicking_agent_toast_focuses_target_pane() {
        let mut app = app_for_mouse_test();
        let active = Workspace::test_new("active");
        let mut background = Workspace::test_new("background");
        let first_pane = background.tabs[0].root_pane;
        let target_pane = background.test_split(Direction::Horizontal);
        background.tabs[0].layout.focus_pane(first_pane);

        app.state.workspaces = vec![active, background];
        app.state.ensure_test_terminals();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.toast_config.delivery = crate::config::ToastDelivery::Hako;
        let target_terminal_id = app.state.workspaces[1]
            .panes
            .get(&target_pane)
            .unwrap()
            .attached_terminal_id
            .clone();
        app.state
            .terminals
            .get_mut(&target_terminal_id)
            .unwrap()
            .state = AgentState::Working;

        app.state
            .handle_app_event(crate::events::AppEvent::StateChanged {
                pane_id: target_pane,
                agent: Some(Agent::Pi),
                state: AgentState::Idle,
                visible_blocker: false,
                visible_idle: false,
                visible_working: false,
                process_exited: false,
                observed_at: std::time::Instant::now(),
            });
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let hit = app.state.view.toast_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            hit.x + 1,
            hit.y + 1,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.workspaces[1].focused_pane_id(), Some(target_pane));
        assert!(app.state.toast.is_none());
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn toast_click_does_not_steal_mouse_from_settings_overlay() {
        let mut app = app_for_mouse_test();
        let active = Workspace::test_new("active");
        let background = Workspace::test_new("background");
        let target_pane = background.tabs[0].root_pane;
        let workspace_id = background.id.clone();

        app.state.workspaces = vec![active, background];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.toast = Some(crate::app::state::ToastNotification {
            kind: crate::app::state::ToastKind::Finished,
            title: "pi finished".into(),
            context: "background · 2".into(),
            position: None,
            target: Some(crate::app::state::ToastTarget {
                workspace_id,
                pane_id: target_pane,
            }),
        });
        app.state.mode = Mode::Settings;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let hit = app.state.view.toast_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            hit.x + 1,
            hit.y + 1,
        ));

        assert_eq!(app.state.active, Some(0));
        assert!(app.state.toast.is_some());
    }

    #[test]
    fn clicking_confirm_close_accepts_workspace_close() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.active = Some(0);
        app.state.selected = 1;
        app.state.mode = Mode::ConfirmClose;

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

        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn group_icon_picker_sets_icon_for_new_group() {
        let mut app = app_for_mouse_test();
        super::super::modal::open_new_group_dialog(&mut app.state);
        assert_eq!(
            app.state.group_icon_input,
            crate::app::state::DEFAULT_GROUP_ICON
        );

        let inner = app.state.rename_modal_inner().unwrap();
        let icon_button = crate::ui::group_icon_button_rect(&app.state, inner);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            icon_button.x,
            icon_button.y,
        ));

        assert!(app.state.group_icon_picker_open);

        let (coffee, _) = crate::ui::group_icon_picker_rects(&app.state, inner)
            .into_iter()
            .find(|(_, icon)| *icon == "☕")
            .expect("coffee icon should be offered");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            coffee.x,
            coffee.y,
        ));

        assert_eq!(app.state.group_icon_input, "☕");
        assert!(!app.state.group_icon_picker_open);

        app.state.name_input = "showcode".to_string();
        app.state.name_input_replace_on_type = false;
        let (save, _, _) = crate::ui::rename_button_rects(inner);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            save.x,
            save.y,
        ));

        assert_eq!(app.state.groups[1].name, "showcode");
        assert_eq!(app.state.groups[1].icon, "☕");
        assert_eq!(app.state.active_group, 1);
    }

    #[test]
    fn new_group_modal_body_click_does_not_close() {
        let mut app = app_for_mouse_test();
        super::super::modal::open_new_group_dialog(&mut app.state);

        let inner = app.state.rename_modal_inner().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            inner.x + 5,
            inner.y + 2,
        ));

        assert_eq!(app.state.mode, Mode::RenameGroup);
        assert!(app.state.creating_new_group);
    }

    #[test]
    fn group_modal_save_clicks_rendered_button_with_right_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.view.right_sidebar_rect = Rect::new(106, 0, 34, 20);
        super::super::modal::open_new_group_dialog(&mut app.state);
        app.state.name_input = "Work".to_string();
        app.state.group_icon_input = "☕".to_string();

        let save = rendered_text_point(&app, "save");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            save.0,
            save.1,
        ));

        assert_eq!(app.state.groups[1].name, "Work");
        assert_eq!(app.state.groups[1].icon, "☕");
        assert_eq!(app.state.mode, Mode::Navigate);
    }

    #[test]
    fn group_modal_clear_clicks_rendered_button_with_right_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.view.right_sidebar_rect = Rect::new(106, 0, 34, 20);
        super::super::modal::open_new_group_dialog(&mut app.state);
        app.state.name_input = "Work".to_string();

        let clear = rendered_text_point(&app, "clear");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            clear.0,
            clear.1,
        ));

        assert_eq!(app.state.name_input, "");
        assert_eq!(app.state.mode, Mode::RenameGroup);
    }

    #[test]
    fn group_modal_close_clicks_rendered_button_with_right_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.view.right_sidebar_rect = Rect::new(106, 0, 34, 20);
        super::super::modal::open_new_group_dialog(&mut app.state);

        let close = rendered_text_point(&app, "close");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            close.0,
            close.1,
        ));

        assert_eq!(app.state.mode, Mode::Navigate);
        assert!(!app.state.creating_new_group);
    }

    #[test]
    fn group_modal_icon_clicks_rendered_button_with_right_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.view.right_sidebar_rect = Rect::new(106, 0, 34, 20);
        super::super::modal::open_new_group_dialog(&mut app.state);

        let inner = app.state.rename_modal_inner().unwrap();
        let icon_rect = crate::ui::group_icon_button_rect(&app.state, inner);
        let screen = app.state.screen_rect();
        let backend = TestBackend::new(screen.width, screen.height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| crate::ui::render(&app.state, frame))
            .expect("render app");
        let buffer = terminal.backend().buffer();
        let icon_symbol = app.state.group_icon_input.as_str();
        assert!(
            (icon_rect.x..icon_rect.x + icon_rect.width)
                .any(|x| buffer[(x, icon_rect.y)].symbol() == icon_symbol),
            "rendered group icon should be inside the icon button"
        );
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            icon_rect.x + 1,
            icon_rect.y,
        ));

        assert!(app.state.group_icon_picker_open);
    }

    #[test]
    fn group_icon_picker_updates_icon_for_existing_group() {
        let mut app = app_for_mouse_test();
        let group_idx = app.state.create_group("Work".to_string());
        app.state.set_group_icon(group_idx, "☀".to_string());
        super::super::modal::open_rename_group_at(&mut app.state, group_idx);

        let inner = app.state.rename_modal_inner().unwrap();
        let icon_button = crate::ui::group_icon_button_rect(&app.state, inner);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            icon_button.x,
            icon_button.y,
        ));

        let (anchor, _) = crate::ui::group_icon_picker_rects(&app.state, inner)
            .into_iter()
            .find(|(_, icon)| *icon == "⚓")
            .expect("anchor icon should be offered");
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            anchor.x,
            anchor.y,
        ));

        app.state.name_input = "Work renamed".to_string();
        let (save, _, _) = crate::ui::rename_button_rects(inner);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            save.x,
            save.y,
        ));

        assert_eq!(app.state.groups[group_idx].name, "Work renamed");
        assert_eq!(app.state.groups[group_idx].icon, "⚓");
    }

    #[test]
    fn clicking_confirm_close_accepts_after_workspace_context_menu_close() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 1,
                can_diff: false,
            },
            x: 2,
            y: 2,
            list: MenuListState::new(9),
        });
        app.state.mode = Mode::ContextMenu;
        handle_context_menu_key(
            &mut app.state,
            &mut app.terminal_runtimes,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(app.state.mode, Mode::ConfirmClose);
        assert_eq!(app.state.selected, 1);

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
            confirm.x + 1,
            confirm.y,
        ));

        assert_eq!(app.state.workspaces.len(), 1);
        assert_eq!(app.state.workspaces[0].display_name(), "a");
    }

    #[test]
    fn clicking_confirm_close_on_last_workspace_deletes_space() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("only")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        app.state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 0,
                can_diff: false,
            },
            x: 2,
            y: 2,
            list: MenuListState::new(9),
        });
        app.state.mode = Mode::ContextMenu;
        handle_context_menu_key(
            &mut app.state,
            &mut app.terminal_runtimes,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );
        assert_eq!(app.state.mode, Mode::ConfirmClose);
        assert_eq!(app.state.selected, 0);

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
            confirm.x + 1,
            confirm.y,
        ));

        assert_eq!(app.state.mode, Mode::Navigate);
        assert!(app.state.workspaces.is_empty());
        assert_eq!(app.state.active, None);
        assert_eq!(app.state.selected, 0);
    }

    #[test]
    fn mouse_clicking_workspace_context_close_on_last_workspace_deletes_space() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("only")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 0,
                can_diff: false,
            },
            x: 2,
            y: 2,
            list: MenuListState::new(9),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 10,
        ));

        assert_eq!(app.state.mode, Mode::ConfirmClose);

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
            confirm.x + 1,
            confirm.y,
        ));

        assert_eq!(app.state.mode, Mode::Navigate);
        assert!(app.state.workspaces.is_empty());
        assert_eq!(app.state.active, None);
    }

    #[test]
    fn mouse_clicking_workspace_context_close_without_confirmation_deletes_space() {
        let mut app = app_for_mouse_test();
        app.state.confirm_close = false;
        app.state.workspaces = vec![Workspace::test_new("only")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Workspace {
                ws_idx: 0,
                can_diff: false,
            },
            x: 2,
            y: 2,
            list: MenuListState::new(9),
        });
        app.state.mode = Mode::ContextMenu;

        let menu = app.state.context_menu_rect().unwrap();
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            menu.x + 2,
            menu.y + 10,
        ));

        assert_eq!(app.state.mode, Mode::Navigate);
        assert!(app.state.workspaces.is_empty());
        assert_eq!(app.state.active, None);
        assert_eq!(app.state.selected, 0);
    }

    #[tokio::test]
    async fn keyboard_context_menu_split_keeps_new_runtime() {
        let mut app = app_for_mouse_test();
        app.state.default_shell = "/usr/bin/true".into();
        app.state.shell_mode = crate::config::ShellModeConfig::NonLogin;
        let (workspace, terminal, runtime) = Workspace::new(
            std::env::current_dir().unwrap_or_else(|_| "/".into()),
            24,
            80,
            app.state.pane_scrollback_limit_bytes,
            app.state.host_terminal_theme,
            crate::pane::PaneShellConfig::new(&app.state.default_shell, app.state.shell_mode),
            app.event_tx.clone(),
            app.render_notify.clone(),
            app.render_dirty.clone(),
        )
        .expect("workspace should spawn");
        app.state.workspaces = vec![workspace];
        app.terminal_runtimes.insert(terminal.id.clone(), runtime);
        app.state.terminals.insert(terminal.id.clone(), terminal);
        app.state.active = Some(0);
        app.state.selected = 0;
        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let runtime_count = app.terminal_runtimes.len();
        app.state.context_menu = Some(ContextMenuState {
            kind: ContextMenuKind::Pane {
                ws_idx: 0,
                pane_id,
                has_manual_label: false,
            },
            x: 2,
            y: 2,
            list: MenuListState::new(1),
        });
        app.state.mode = Mode::ContextMenu;

        handle_context_menu_key(
            &mut app.state,
            &mut app.terminal_runtimes,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(app.state.mode, Mode::Terminal);
        assert_eq!(app.state.workspaces[0].tabs[0].layout.pane_count(), 2);
        assert_eq!(app.terminal_runtimes.len(), runtime_count + 1);

        let runtimes: Vec<_> = app.terminal_runtimes.drain().collect();
        for (_terminal_id, runtime) in runtimes {
            runtime.shutdown();
        }
    }

    #[test]
    fn dragging_pane_split_updates_captured_layout_ratio() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.workspaces[0].test_split(Direction::Horizontal);
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let border = app.state.view.split_borders[0].clone();
        let before = capture_snapshot(&app.state);
        let drag_row = border.area.y.saturating_add(1);

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            border.pos,
            drag_row,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            border.pos.saturating_add(6),
            drag_row,
        ));

        let after = capture_snapshot(&app.state);
        assert_ne!(root_layout_ratio(&before), root_layout_ratio(&after));
    }

    #[test]
    fn pane_split_hitbox_does_not_overlap_right_pane_content() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.workspaces[0].test_split(Direction::Horizontal);
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let border = app.state.view.split_borders[0].clone();
        let row = border.area.y.saturating_add(1);

        assert!(app
            .state
            .find_border_at(border.pos.saturating_sub(1), row)
            .is_some());
        assert!(app.state.find_border_at(border.pos, row).is_some());
        assert!(app
            .state
            .find_border_at(border.pos.saturating_add(1), row)
            .is_none());
    }

    #[test]
    fn pane_split_hitbox_does_not_overlap_bottom_pane_content() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("test")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.workspaces[0].test_split(Direction::Vertical);
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let border = app.state.view.split_borders[0].clone();
        let col = border.area.x.saturating_add(1);

        assert!(app
            .state
            .find_border_at(col, border.pos.saturating_sub(1))
            .is_some());
        assert!(app.state.find_border_at(col, border.pos).is_some());
        assert!(app
            .state
            .find_border_at(col, border.pos.saturating_add(1))
            .is_none());
    }

    #[test]
    fn selecting_from_right_pane_first_content_column_starts_selection() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let second_pane = ws.test_split(Direction::Horizontal);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let second_info = app
            .state
            .view
            .pane_infos
            .iter()
            .find(|info| info.id == second_pane)
            .expect("second pane info")
            .clone();
        let col = second_info.inner_rect.x;
        let row = second_info.inner_rect.y;

        assert!(app.state.find_border_at(col, row).is_none());
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), col, row));

        assert!(app.state.drag.is_none());
        assert_eq!(
            app.state
                .selection
                .as_ref()
                .map(|selection| selection.pane_id),
            Some(second_pane)
        );
    }

    #[test]
    fn selecting_from_bottom_pane_first_content_row_starts_selection() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let second_pane = ws.test_split(Direction::Vertical);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let second_info = app
            .state
            .view
            .pane_infos
            .iter()
            .find(|info| info.id == second_pane)
            .expect("second pane info")
            .clone();
        let col = second_info.inner_rect.x;
        let row = second_info.inner_rect.y;

        assert!(app.state.find_border_at(col, row).is_none());
        app.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), col, row));

        assert!(app.state.drag.is_none());
        assert_eq!(
            app.state
                .selection
                .as_ref()
                .map(|selection| selection.pane_id),
            Some(second_pane)
        );
    }

    #[tokio::test]
    async fn configured_right_click_passthrough_forwards_full_gesture_to_pane() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let pane_infos = ws.tabs[0].layout.panes(Rect::new(26, 2, 80, 18));
        let info = pane_infos[0].clone();
        let (runtime, mut input_rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_scrollback_bytes(
                info.inner_rect.width,
                info.inner_rect.height,
                0,
                b"\x1b[?1002h\x1b[?1006h",
                4,
            );
        ws.insert_test_runtime(pane_id, runtime);

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.view.pane_infos = pane_infos;
        app.state.right_click_passthrough_modifiers = Some(KeyModifiers::CONTROL);

        let col = info.inner_rect.x + 2;
        let row = info.inner_rect.y + 3;
        app.handle_mouse(MouseEvent {
            modifiers: KeyModifiers::CONTROL,
            ..mouse(MouseEventKind::Down(MouseButton::Right), col, row)
        });
        app.handle_mouse(MouseEvent {
            modifiers: KeyModifiers::CONTROL,
            ..mouse(MouseEventKind::Drag(MouseButton::Right), col + 1, row + 1)
        });
        app.handle_mouse(MouseEvent {
            modifiers: KeyModifiers::CONTROL,
            ..mouse(MouseEventKind::Up(MouseButton::Right), col + 1, row + 1)
        });

        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(app.state.context_menu.is_none());
        assert!(app.state.right_click_passthrough.is_none());
        assert_eq!(
            input_rx.try_recv().expect("forwarded right mouse down"),
            Bytes::from_static(b"\x1b[<2;3;4M")
        );
        assert_eq!(
            input_rx.try_recv().expect("forwarded right mouse drag"),
            Bytes::from_static(b"\x1b[<34;4;5M")
        );
        assert_eq!(
            input_rx.try_recv().expect("forwarded right mouse up"),
            Bytes::from_static(b"\x1b[<2;4;5m")
        );
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn unset_right_click_passthrough_keeps_modified_right_click_as_hako_menu() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let pane_infos = ws.tabs[0].layout.panes(Rect::new(26, 2, 80, 18));
        let info = pane_infos[0].clone();
        let (runtime, mut input_rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_scrollback_bytes(
                info.inner_rect.width,
                info.inner_rect.height,
                0,
                b"\x1b[?1002h\x1b[?1006h",
                4,
            );
        ws.insert_test_runtime(pane_id, runtime);

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.view.pane_infos = pane_infos;
        app.state.right_click_passthrough_modifiers = None;

        app.handle_mouse(MouseEvent {
            modifiers: KeyModifiers::CONTROL,
            ..mouse(
                MouseEventKind::Down(MouseButton::Right),
                info.inner_rect.x + 2,
                info.inner_rect.y + 3,
            )
        });

        assert_eq!(app.state.mode, Mode::ContextMenu);
        assert!(app.state.context_menu.is_some());
        assert!(app.state.right_click_passthrough.is_none());
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn right_click_passthrough_requires_exact_modifier_match() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let pane_infos = ws.tabs[0].layout.panes(Rect::new(26, 2, 80, 18));
        let info = pane_infos[0].clone();
        let (runtime, mut input_rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_scrollback_bytes(
                info.inner_rect.width,
                info.inner_rect.height,
                0,
                b"\x1b[?1002h\x1b[?1006h",
                4,
            );
        ws.insert_test_runtime(pane_id, runtime);

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.view.pane_infos = pane_infos;
        app.state.right_click_passthrough_modifiers = Some(KeyModifiers::CONTROL);

        app.handle_mouse(MouseEvent {
            modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ..mouse(
                MouseEventKind::Down(MouseButton::Right),
                info.inner_rect.x + 2,
                info.inner_rect.y + 3,
            )
        });

        assert_eq!(app.state.mode, Mode::ContextMenu);
        assert!(app.state.context_menu.is_some());
        assert!(app.state.right_click_passthrough.is_none());
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn right_click_passthrough_does_not_forward_pane_frame_clicks() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        let other_pane = ws.test_split(Direction::Vertical);
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.right_click_passthrough_modifiers = Some(KeyModifiers::CONTROL);
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let info = app
            .state
            .view
            .pane_infos
            .iter()
            .find(|info| info.id == pane_id)
            .expect("pane info")
            .clone();
        let (runtime, mut input_rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_scrollback_bytes(
                info.inner_rect.width,
                info.inner_rect.height,
                0,
                b"\x1b[?1002h\x1b[?1006h",
                4,
            );
        app.state.insert_test_runtime(pane_id, runtime);
        app.state.insert_test_runtime(
            other_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(10, 5, b""),
        );

        assert!(app.state.pane_at(info.rect.x, info.rect.y).is_none());
        assert!(app
            .state
            .pane_mouse_target(info.rect.x, info.rect.y)
            .is_some());
        app.handle_mouse(MouseEvent {
            modifiers: KeyModifiers::CONTROL,
            ..mouse(
                MouseEventKind::Down(MouseButton::Right),
                info.rect.x,
                info.rect.y,
            )
        });

        assert_eq!(app.state.mode, Mode::ContextMenu);
        assert!(app.state.context_menu.is_some());
        assert!(app.state.right_click_passthrough.is_none());
        assert!(input_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn dragging_vertical_pane_split_still_resizes_when_pane_mouse_reporting_is_enabled() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        let second_pane = ws.test_split(Direction::Vertical);

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let pane_infos = app.state.view.pane_infos.clone();
        let first_info = pane_infos
            .iter()
            .find(|info| info.id == first_pane)
            .expect("first pane info")
            .clone();
        let second_info = pane_infos
            .iter()
            .find(|info| info.id == second_pane)
            .expect("second pane info")
            .clone();

        app.state.insert_test_runtime(
            first_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                first_info.inner_rect.width.max(1),
                first_info.inner_rect.height.max(1),
                b"\x1b[?1002h",
            ),
        );
        app.state.insert_test_runtime(
            second_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                second_info.inner_rect.width.max(1),
                second_info.inner_rect.height.max(1),
                b"\x1b[?1002h",
            ),
        );

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let border = app
            .state
            .view
            .split_borders
            .iter()
            .find(|border| border.direction == Direction::Vertical)
            .expect("vertical split border")
            .clone();
        let before = capture_snapshot(&app.state);
        let drag_col = border.area.x.saturating_add(1);

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            drag_col,
            border.pos,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            drag_col,
            border.pos.saturating_add(4),
        ));

        let after = capture_snapshot(&app.state);
        assert_ne!(root_layout_ratio(&before), root_layout_ratio(&after));
    }

    #[tokio::test]
    async fn dragging_horizontal_pane_split_still_resizes_when_pane_mouse_reporting_is_enabled() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("test");
        let first_pane = ws.tabs[0].root_pane;
        let second_pane = ws.test_split(Direction::Horizontal);

        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let pane_infos = app.state.view.pane_infos.clone();
        let first_info = pane_infos
            .iter()
            .find(|info| info.id == first_pane)
            .expect("first pane info")
            .clone();
        let second_info = pane_infos
            .iter()
            .find(|info| info.id == second_pane)
            .expect("second pane info")
            .clone();

        app.state.insert_test_runtime(
            first_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                first_info.inner_rect.width.max(1),
                first_info.inner_rect.height.max(1),
                b"\x1b[?1002h",
            ),
        );
        app.state.insert_test_runtime(
            second_pane,
            crate::terminal::TerminalRuntime::test_with_screen_bytes(
                second_info.inner_rect.width.max(1),
                second_info.inner_rect.height.max(1),
                b"\x1b[?1002h",
            ),
        );

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let border = app
            .state
            .view
            .split_borders
            .iter()
            .find(|border| border.direction == Direction::Horizontal)
            .expect("horizontal split border")
            .clone();
        let before = capture_snapshot(&app.state);
        let drag_row = border.area.y.saturating_add(1);

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            border.pos,
            drag_row,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::Drag(MouseButton::Left),
            border.pos.saturating_add(6),
            drag_row,
        ));

        let after = capture_snapshot(&app.state);
        assert_ne!(root_layout_ratio(&before), root_layout_ratio(&after));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mouse_move_forwards_to_pane_that_requested_any_motion() {
        let mut app = app_for_mouse_test();
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let info = app.state.view.pane_infos[0].clone();
        let (runtime, mut rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_screen_bytes(
                info.inner_rect.width.max(1),
                info.inner_rect.height.max(1),
                b"\x1b[?1003h\x1b[?1006h",
            );
        app.state.workspaces[0].insert_test_runtime(pane_id, runtime);

        app.handle_mouse(mouse(
            MouseEventKind::Moved,
            info.inner_rect.x + 3,
            info.inner_rect.y + 2,
        ));

        let bytes = rx.try_recv().expect("motion event forwarded to pane");
        assert!(!bytes.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn mouse_move_is_not_forwarded_for_button_motion_mode() {
        let mut app = app_for_mouse_test();
        let ws = Workspace::test_new("test");
        let pane_id = ws.tabs[0].root_pane;
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));

        let info = app.state.view.pane_infos[0].clone();
        let (runtime, mut rx) =
            crate::terminal::TerminalRuntime::test_with_channel_and_screen_bytes(
                info.inner_rect.width.max(1),
                info.inner_rect.height.max(1),
                b"\x1b[?1002h\x1b[?1006h",
            );
        app.state.workspaces[0].insert_test_runtime(pane_id, runtime);

        app.handle_mouse(mouse(
            MouseEventKind::Moved,
            info.inner_rect.x + 3,
            info.inner_rect.y + 2,
        ));

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn wheel_routing_prefers_mouse_reporting() {
        let input_state = crate::pane::InputState {
            alternate_screen: true,
            application_cursor: false,
            bracketed_paste: false,
            focus_reporting: false,
            mouse_protocol_mode: crate::input::MouseProtocolMode::ButtonMotion,
            mouse_protocol_encoding: crate::input::MouseProtocolEncoding::Sgr,
            mouse_alternate_scroll: true,
            modify_other_keys: false,
        };

        assert_eq!(wheel_routing(input_state), WheelRouting::MouseReport);
    }

    #[test]
    fn wheel_over_tab_bar_switches_tabs() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.test_add_tab(Some("two"));
        ws.test_add_tab(Some("three"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let tab_bar = app.state.view.tab_bar_rect;

        app.handle_mouse(mouse(MouseEventKind::ScrollDown, tab_bar.x + 1, tab_bar.y));
        assert_eq!(app.state.workspaces[0].active_tab, 1);

        app.handle_mouse(mouse(MouseEventKind::ScrollUp, tab_bar.x + 1, tab_bar.y));
        assert_eq!(app.state.workspaces[0].active_tab, 0);

        app.handle_mouse(mouse(MouseEventKind::ScrollUp, tab_bar.x + 1, tab_bar.y));
        assert_eq!(app.state.workspaces[0].active_tab, 2);

        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            tab_bar.x + tab_bar.width.saturating_sub(1),
            tab_bar.y,
        ));
        assert_eq!(app.state.workspaces[0].active_tab, 0);
    }

    #[test]
    fn clicking_hovered_tab_close_icon_closes_that_tab() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.test_add_tab(Some("two"));
        ws.test_add_tab(Some("three"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let second_tab = app.state.view.tab_hit_areas[1];
        app.handle_mouse(mouse(
            MouseEventKind::Moved,
            second_tab.x + second_tab.width.saturating_sub(1),
            second_tab.y,
        ));
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let close = app.state.view.tab_close_hit_areas[1];

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            close.x,
            close.y,
        ));

        assert_eq!(app.state.workspaces[0].tabs.len(), 2);
        assert_eq!(app.state.workspaces[0].tabs[0].display_name(), "1");
        assert_eq!(app.state.workspaces[0].tabs[1].display_name(), "three");
        assert_eq!(app.state.workspaces[0].active_tab, 0);
        assert!(app.state.session_dirty);
    }

    #[test]
    fn wheel_over_overflowing_tab_bar_switches_tabs() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.tabs[0].set_custom_name("very-long-one".into());
        ws.test_add_tab(Some("very-long-two"));
        ws.test_add_tab(Some("very-long-three"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 65, 20));
        assert!(app.state.view.tab_scroll_right_hit_area.width > 0);
        let tab_bar = app.state.view.tab_bar_rect;

        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            tab_bar.x + tab_bar.width.saturating_sub(2),
            tab_bar.y,
        ));
        assert_eq!(app.state.workspaces[0].active_tab, 1);

        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            tab_bar.x + tab_bar.width.saturating_sub(2),
            tab_bar.y,
        ));
        assert_eq!(app.state.workspaces[0].active_tab, 2);
    }

    #[test]
    fn wheel_outside_tab_bar_does_not_switch_tabs() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.test_add_tab(Some("two"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 106, 20));
        let terminal = app.state.view.terminal_area;

        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            terminal.x + 1,
            terminal.y + 1,
        ));

        assert_eq!(app.state.workspaces[0].active_tab, 0);
    }

    #[test]
    fn mobile_switch_button_opens_switcher_and_workspace_row_switches_workspace() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        assert_eq!(app.state.view.layout, ViewLayout::Mobile);

        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));

        assert_eq!(app.state.mode, Mode::Navigate);

        let viewport = crate::ui::mobile_switcher_areas(&app.state).viewport;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.x + 2,
            viewport.y + 4,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn mobile_workspace_panel_scroll_reaches_extra_workspaces() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = (0..12)
            .map(|idx| Workspace::test_new(&format!("ws-{idx}")))
            .collect();
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));
        assert_eq!(app.state.mode, Mode::Navigate);

        let viewport = crate::ui::mobile_switcher_areas(&app.state).viewport;
        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            viewport.x + 2,
            viewport.y,
        ));
        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        assert_eq!(app.state.mobile_switcher_scroll, 2);

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.x + 2,
            viewport.y + 2,
        ));

        assert_eq!(app.state.active, Some(1));
        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn mobile_global_scroll_reaches_tabs_and_switches_tab() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.test_add_tab(Some("two"));
        ws.test_add_tab(Some("three"));
        ws.test_add_tab(Some("four"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 12));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));

        let viewport = crate::ui::mobile_switcher_areas(&app.state).viewport;

        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            viewport.x + 2,
            viewport.y,
        ));
        app.handle_mouse(mouse(
            MouseEventKind::ScrollDown,
            viewport.x + 2,
            viewport.y,
        ));
        assert_eq!(app.state.mobile_switcher_scroll, 4);
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.x + 2,
            viewport.y + 4,
        ));
        assert_eq!(app.state.workspaces[0].active_tab, 2);
    }

    #[test]
    fn mobile_switcher_action_rows_create_workspace_and_open_tab_dialog() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.test_add_tab(Some("logs"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));
        let viewport = crate::ui::mobile_switcher_areas(&app.state).viewport;

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.x + 2,
            viewport.y + 1,
        ));
        assert!(app.state.request_new_workspace);

        app.state.request_new_workspace = false;
        app.state.mode = Mode::Navigate;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.x + 2,
            viewport.y + 5,
        ));
        assert_eq!(app.state.mode, Mode::RenameTab);
        assert!(app.state.creating_new_tab);
    }

    #[test]
    fn mobile_switcher_new_tab_skips_dialog_when_prompt_disabled() {
        let mut app = app_for_mouse_test();
        let mut ws = Workspace::test_new("one");
        ws.test_add_tab(Some("logs"));
        app.state.workspaces = vec![ws];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.prompt_new_tab_name = false;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));
        let viewport = crate::ui::mobile_switcher_areas(&app.state).viewport;

        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.x + 2,
            viewport.y + 5,
        ));
        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(!app.state.creating_new_tab);
        assert!(app.state.request_new_tab);
        assert!(app.state.requested_new_tab_name.is_none());
    }

    #[test]
    fn desktop_new_tab_context_action_skips_dialog_when_prompt_disabled() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;
        app.state.prompt_new_tab_name = false;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 120, 40));
        let new_tab_area = app.state.view.new_tab_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            new_tab_area.x + 1,
            new_tab_area.y,
        ));
        assert_eq!(app.state.mode, Mode::ContextMenu);

        handle_context_menu_key(
            &mut app.state,
            &mut app.terminal_runtimes,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(!app.state.creating_new_tab);
        assert!(app.state.request_new_tab);
        assert!(app.state.requested_new_tab_name.is_none());
    }

    #[test]
    fn mobile_switcher_swallows_non_left_mouse_events() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));
        assert_eq!(app.state.mode, Mode::Navigate);

        let viewport = crate::ui::mobile_switcher_areas(&app.state).viewport;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Right),
            viewport.x + 2,
            viewport.y + 2,
        ));

        assert_eq!(app.state.mode, Mode::Navigate);
        assert!(app.state.context_menu.is_none());
    }

    #[test]
    fn mobile_switch_button_does_not_bypass_rename_modal() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::RenameTab;
        app.state.creating_new_tab = true;
        app.state.name_input = "new tab".into();

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));

        assert_eq!(app.state.mode, Mode::Terminal);
        assert!(!app.state.creating_new_tab);
        assert!(!app.state.request_new_tab);
    }

    #[test]
    fn mobile_switcher_close_returns_to_terminal() {
        let mut app = app_for_mouse_test();
        app.state.workspaces = vec![Workspace::test_new("one")];
        app.state.active = Some(0);
        app.state.selected = 0;
        app.state.mode = Mode::Terminal;

        crate::ui::compute_view(&mut app.state, Rect::new(0, 0, 44, 20));
        let switch = app.state.view.mobile_menu_hit_area;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            switch.x + 1,
            switch.y + 1,
        ));
        assert_eq!(app.state.mode, Mode::Navigate);

        let close = crate::ui::mobile_switcher_areas(&app.state).close;
        app.handle_mouse(mouse(
            MouseEventKind::Down(MouseButton::Left),
            close.x + 1,
            close.y,
        ));

        assert_eq!(app.state.mode, Mode::Terminal);
    }

    #[test]
    fn wheel_routing_uses_alternate_scroll_in_fullscreen_without_mouse_reporting() {
        let input_state = crate::pane::InputState {
            alternate_screen: true,
            application_cursor: false,
            bracketed_paste: false,
            focus_reporting: false,
            mouse_protocol_mode: crate::input::MouseProtocolMode::None,
            mouse_protocol_encoding: crate::input::MouseProtocolEncoding::Default,
            mouse_alternate_scroll: true,
            modify_other_keys: false,
        };

        assert_eq!(wheel_routing(input_state), WheelRouting::AlternateScroll);
    }

    #[test]
    fn wheel_routing_falls_back_to_host_scrollback() {
        let input_state = crate::pane::InputState {
            alternate_screen: false,
            application_cursor: false,
            bracketed_paste: false,
            focus_reporting: false,
            mouse_protocol_mode: crate::input::MouseProtocolMode::None,
            mouse_protocol_encoding: crate::input::MouseProtocolEncoding::Default,
            mouse_alternate_scroll: true,
            modify_other_keys: false,
        };

        assert_eq!(wheel_routing(input_state), WheelRouting::HostScroll);
    }
}
