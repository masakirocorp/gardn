use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::{
    app::{
        state::{AppState, DragState, DragTarget, SettingsSection, THEME_NAMES},
        App, Mode,
    },
    config::{ThemeMode, ToastDelivery},
};

use super::ScrollbarClickTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
// The shared `Save` verb is semantic: these actions persist settings.
#[allow(clippy::enum_variant_names)]
pub(super) enum SettingsAction {
    SaveTheme {
        name: String,
        mode: ThemeMode,
    },
    SaveGroupTheme {
        group_idx: usize,
        name: Option<String>,
    },
    SaveSound(bool),
    SaveToastDelivery(ToastDelivery),
    SaveAgentBorderLabels(bool),
}

impl App {
    pub(crate) fn handle_settings_key(&mut self, key: KeyEvent) {
        if let Some(action) = update_settings_state(&mut self.state, key) {
            match action {
                SettingsAction::SaveTheme { name, mode } => self.save_theme(&name, mode),
                SettingsAction::SaveGroupTheme { group_idx, name } => {
                    self.state.set_group_theme(group_idx, name);
                }
                SettingsAction::SaveSound(enabled) => self.save_sound(enabled),
                SettingsAction::SaveToastDelivery(delivery) => self.save_toast_delivery(delivery),
                SettingsAction::SaveAgentBorderLabels(enabled) => {
                    self.save_agent_border_labels(enabled)
                }
            }
        }
    }
}

fn normalize_theme_name(name: &str) -> String {
    name.to_lowercase().replace([' ', '_'], "-")
}

fn current_theme_index(theme_name: &str) -> usize {
    let normalized = normalize_theme_name(theme_name);
    THEME_NAMES
        .iter()
        .position(|name| normalize_theme_name(name) == normalized)
        .unwrap_or(0)
}

fn toast_delivery_index(delivery: ToastDelivery) -> usize {
    match delivery {
        ToastDelivery::Off => 0,
        ToastDelivery::Herdr => 1,
        ToastDelivery::Terminal => 2,
    }
}

fn toast_delivery_for_index(idx: usize) -> ToastDelivery {
    match idx {
        0 => ToastDelivery::Off,
        1 => ToastDelivery::Herdr,
        _ => ToastDelivery::Terminal,
    }
}

fn theme_list_len(state: &AppState) -> usize {
    THEME_NAMES.len() + usize::from(state.settings.group_theme_target.is_some())
}

fn settings_theme_max_scroll(state: &AppState) -> usize {
    theme_list_len(state).saturating_sub(state.settings_content_rect().height as usize)
}

fn ensure_settings_selection_visible(state: &mut AppState) {
    let viewport_rows = state.settings_content_rect().height.max(1) as usize;
    let max_scroll = theme_list_len(state).saturating_sub(viewport_rows);
    state.settings.scroll = state.settings.scroll.min(max_scroll);

    if state.settings.list.selected < state.settings.scroll {
        state.settings.scroll = state.settings.list.selected;
    } else if state.settings.list.selected >= state.settings.scroll + viewport_rows {
        state.settings.scroll = state.settings.list.selected + 1 - viewport_rows;
    }
    state.settings.scroll = state.settings.scroll.min(max_scroll);
}

fn set_settings_theme_offset_from_bottom(state: &mut AppState, offset_from_bottom: usize) {
    let max_scroll = settings_theme_max_scroll(state);
    state.settings.scroll = max_scroll.saturating_sub(offset_from_bottom.min(max_scroll));
}

fn settings_theme_scroll_metrics(state: &AppState) -> crate::pane::ScrollMetrics {
    let viewport_rows = state.settings_content_rect().height.max(1) as usize;
    let max_offset_from_bottom = theme_list_len(state).saturating_sub(viewport_rows);
    let scroll = state.settings.scroll.min(max_offset_from_bottom);
    crate::pane::ScrollMetrics {
        offset_from_bottom: max_offset_from_bottom.saturating_sub(scroll),
        max_offset_from_bottom,
        viewport_rows,
    }
}

fn selected_group_theme_name(state: &AppState) -> Option<String> {
    if state.settings.group_theme_target.is_some() {
        if state.settings.list.selected == 0 {
            None
        } else {
            Some(THEME_NAMES[state.settings.list.selected - 1].to_string())
        }
    } else {
        Some(THEME_NAMES[state.settings.list.selected].to_string())
    }
}

fn target_theme_index(state: &AppState) -> usize {
    let Some(group_idx) = state.settings.group_theme_target else {
        return current_theme_index(&state.global_theme_name);
    };
    state
        .groups
        .get(group_idx)
        .and_then(|group| group.theme_name.as_deref())
        .map(|theme_name| current_theme_index(theme_name) + 1)
        .unwrap_or(0)
}

fn current_theme_mode_index(mode: ThemeMode) -> usize {
    ThemeMode::ALL
        .iter()
        .position(|candidate| *candidate == mode)
        .unwrap_or(0)
}

fn preview_selected_theme(state: &mut AppState) {
    if let Some(name) = selected_group_theme_name(state) {
        state.settings.pending_theme_name = Some(name.clone());
        let mode = pending_theme_mode(state);
        state.preview_theme_with_mode(&name, mode);
    } else {
        let theme_name = state.global_theme_name.clone();
        state.preview_theme(&theme_name);
    }
}

fn preview_selected_theme_mode(state: &mut AppState) {
    let mode = ThemeMode::ALL
        .get(state.settings.list.selected)
        .copied()
        .unwrap_or(state.global_theme_mode);
    state.settings.pending_theme_mode = Some(mode);
    let name = state
        .settings
        .pending_theme_name
        .clone()
        .unwrap_or_else(|| state.global_theme_name.clone());
    let mode = state
        .settings
        .pending_theme_mode
        .unwrap_or(state.global_theme_mode);
    state.preview_theme_with_mode(&name, mode);
}

fn pending_theme_name(state: &AppState) -> String {
    state
        .settings
        .pending_theme_name
        .clone()
        .unwrap_or_else(|| state.global_theme_name.clone())
}

fn pending_theme_mode(state: &AppState) -> ThemeMode {
    state
        .settings
        .pending_theme_mode
        .unwrap_or(state.global_theme_mode)
}

fn preview_group_theme(state: &mut AppState) {
    if let Some(name) = selected_group_theme_name(state) {
        state.preview_theme(&name);
    } else {
        let theme_name = state.global_theme_name.clone();
        state.preview_theme(&theme_name);
    }
}

fn cancel_settings(state: &mut AppState) {
    if let Some(palette) = state.settings.original_palette.take() {
        state.palette = palette;
    }
    if let Some(theme_name) = state.settings.original_theme.take() {
        state.theme_name = theme_name;
    }
    state.settings.pending_theme_name = None;
    state.settings.pending_theme_mode = None;
    state.settings.group_theme_target = None;
    super::modal::leave_modal(state);
}

fn apply_settings(state: &mut AppState) -> Option<SettingsAction> {
    match state.settings.section {
        SettingsSection::ThemeMode => {
            let theme_name = pending_theme_name(state);
            let theme_mode = pending_theme_mode(state);
            state.settings.original_palette = None;
            state.settings.original_theme = None;
            state.settings.pending_theme_name = None;
            state.settings.pending_theme_mode = None;
            super::modal::leave_modal(state);
            Some(SettingsAction::SaveTheme {
                name: theme_name,
                mode: theme_mode,
            })
        }
        SettingsSection::Theme => {
            let theme_name = pending_theme_name(state);
            let theme_mode = pending_theme_mode(state);
            let group_theme_name = state
                .settings
                .group_theme_target
                .and_then(|_| selected_group_theme_name(state));
            let group_theme_target = state.settings.group_theme_target.take();
            state.settings.original_palette = None;
            state.settings.original_theme = None;
            state.settings.pending_theme_name = None;
            state.settings.pending_theme_mode = None;
            super::modal::leave_modal(state);
            if let Some(group_idx) = group_theme_target {
                return Some(SettingsAction::SaveGroupTheme {
                    group_idx,
                    name: group_theme_name,
                });
            }
            Some(SettingsAction::SaveTheme {
                name: theme_name,
                mode: theme_mode,
            })
        }
        _ => {
            state.settings.group_theme_target = None;
            super::modal::leave_modal(state);
            None
        }
    }
}

pub(super) fn update_settings_state(state: &mut AppState, key: KeyEvent) -> Option<SettingsAction> {
    if state.settings.group_theme_target.is_some() {
        state.settings.section = SettingsSection::Theme;
    }

    match state.settings.section {
        SettingsSection::Theme => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let previous = state.settings.list.selected;
                state.settings.list.move_prev();
                ensure_settings_selection_visible(state);
                if state.settings.list.selected != previous {
                    preview_selected_theme(state);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let previous = state.settings.list.selected;
                state.settings.list.move_next(theme_list_len(state));
                ensure_settings_selection_visible(state);
                if state.settings.list.selected != previous {
                    preview_selected_theme(state);
                }
            }
            KeyCode::PageUp => {
                state.settings.scroll = state
                    .settings
                    .scroll
                    .saturating_sub(state.settings_content_rect().height.max(1) as usize);
            }
            KeyCode::PageDown => {
                let step = state.settings_content_rect().height.max(1) as usize;
                state.settings.scroll = state
                    .settings
                    .scroll
                    .saturating_add(step)
                    .min(settings_theme_max_scroll(state));
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                if state.settings.group_theme_target.is_none() {
                    state.settings.section = SettingsSection::Sound;
                    state.settings.list.selected = usize::from(!state.sound_enabled());
                }
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                if state.settings.group_theme_target.is_none() {
                    state.settings.section = SettingsSection::ThemeMode;
                    state.settings.list.selected =
                        current_theme_mode_index(pending_theme_mode(state));
                }
            }
            _ => match super::modal::modal_action_from_key(&key, super::modal::SETTINGS_ACTIONS) {
                Some(super::modal::ModalAction::Apply) => return apply_settings(state),
                Some(super::modal::ModalAction::Close) => cancel_settings(state),
                _ => {}
            },
        },
        SettingsSection::ThemeMode => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let previous = state.settings.list.selected;
                state.settings.list.move_prev();
                if state.settings.list.selected != previous {
                    preview_selected_theme_mode(state);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let previous = state.settings.list.selected;
                state.settings.list.move_next(ThemeMode::ALL.len());
                if state.settings.list.selected != previous {
                    preview_selected_theme_mode(state);
                }
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Theme;
                state.settings.list.selected = target_theme_index(state);
                ensure_settings_selection_visible(state);
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::PaneLabels;
                state.settings.list.selected = usize::from(!state.agent_border_labels_enabled());
            }
            _ => match super::modal::modal_action_from_key(&key, super::modal::SETTINGS_ACTIONS) {
                Some(super::modal::ModalAction::Apply) => return apply_settings(state),
                Some(super::modal::ModalAction::Close) => cancel_settings(state),
                _ => {}
            },
        },
        SettingsSection::Sound => match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') => {
                state.settings.list.selected = 1 - state.settings.list.selected.min(1);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let enabled = state.settings.list.selected == 0;
                return Some(SettingsAction::SaveSound(enabled));
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::Toast;
                state.settings.list.selected = toast_delivery_index(state.toast_delivery());
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Theme;
                state.settings.list.selected = target_theme_index(state);
                ensure_settings_selection_visible(state);
            }
            _ => {
                if let Some(super::modal::ModalAction::Close) =
                    super::modal::modal_action_from_key(&key, super::modal::SETTINGS_ACTIONS)
                {
                    cancel_settings(state);
                }
            }
        },
        SettingsSection::Toast => match key.code {
            KeyCode::Up | KeyCode::Char('k') => state.settings.list.move_prev(),
            KeyCode::Down | KeyCode::Char('j') => state.settings.list.move_next(3),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let delivery = toast_delivery_for_index(state.settings.list.selected);
                return Some(SettingsAction::SaveToastDelivery(delivery));
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Sound;
                state.settings.list.selected = usize::from(!state.sound_enabled());
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::PaneLabels;
                state.settings.list.selected = usize::from(!state.agent_border_labels_enabled());
            }
            _ => {
                if let Some(super::modal::ModalAction::Close) =
                    super::modal::modal_action_from_key(&key, super::modal::SETTINGS_ACTIONS)
                {
                    cancel_settings(state);
                }
            }
        },
        SettingsSection::PaneLabels => match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') => {
                state.settings.list.selected = 1 - state.settings.list.selected.min(1);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                let enabled = state.settings.list.selected == 0;
                return Some(SettingsAction::SaveAgentBorderLabels(enabled));
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                state.settings.section = SettingsSection::Toast;
                state.settings.list.selected = toast_delivery_index(state.toast_delivery());
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                state.settings.section = SettingsSection::ThemeMode;
                state.settings.list.selected = current_theme_mode_index(pending_theme_mode(state));
            }
            _ => {
                if let Some(super::modal::ModalAction::Close) =
                    super::modal::modal_action_from_key(&key, super::modal::SETTINGS_ACTIONS)
                {
                    cancel_settings(state);
                }
            }
        },
    }

    None
}

pub(crate) fn open_settings(state: &mut AppState) {
    state.settings.original_palette = Some(state.palette.clone());
    state.settings.original_theme = Some(state.theme_name.clone());
    state.settings.pending_theme_name = Some(state.global_theme_name.clone());
    state.settings.pending_theme_mode = Some(state.global_theme_mode);
    state.settings.group_theme_target = None;
    state.settings.section = SettingsSection::Theme;
    let theme_name = state.global_theme_name.clone();
    state.settings.list.selected = current_theme_index(&theme_name);
    state.settings.scroll = 0;
    ensure_settings_selection_visible(state);
    state.preview_theme(&theme_name);
    state.mode = Mode::Settings;
}

pub(crate) fn open_group_theme_settings(state: &mut AppState, group_idx: usize) {
    let Some(group) = state.groups.get(group_idx) else {
        return;
    };
    state.settings.original_palette = Some(state.palette.clone());
    state.settings.original_theme = Some(state.theme_name.clone());
    state.settings.pending_theme_name = None;
    state.settings.pending_theme_mode = None;
    state.settings.group_theme_target = Some(group_idx);
    state.settings.section = SettingsSection::Theme;

    let theme_name = group.theme_name.clone();
    state.settings.list.selected = theme_name
        .as_deref()
        .map(|name| current_theme_index(name) + 1)
        .unwrap_or(0);
    state.settings.scroll = 0;
    ensure_settings_selection_visible(state);
    preview_group_theme(state);
    state.mode = Mode::Settings;
}

impl AppState {
    fn settings_popup_rect(&self) -> Rect {
        let (width, height) = if self.settings.group_theme_target.is_some() {
            (56, 20)
        } else {
            (76, 22)
        };
        crate::ui::centered_popup_rect(self.screen_rect(), width, height).unwrap_or_default()
    }

    fn settings_inner_rect(&self) -> Rect {
        let popup = self.settings_popup_rect();
        Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        )
    }

    fn settings_tab_at(&self, col: u16, row: u16) -> Option<SettingsSection> {
        if self.settings.group_theme_target.is_some() {
            return None;
        }

        let inner = self.settings_inner_rect();
        let tab_y = inner.y + 1;
        if row != tab_y {
            return None;
        }
        let mut x = inner.x;
        for section in SettingsSection::ALL {
            let width = section.label().len() as u16 + 2;
            if col >= x && col < x + width {
                return Some(*section);
            }
            x += width + 1;
        }
        None
    }

    pub(crate) fn settings_content_rect(&self) -> Rect {
        let inner = self.settings_inner_rect();
        crate::ui::modal_stack_areas(inner, 3, 2, 0, 1).content
    }

    fn settings_list_index_at(&self, col: u16, row: u16) -> Option<usize> {
        let area = self.settings_content_rect();
        if row < area.y || row >= area.y + area.height || col < area.x || col >= area.x + area.width
        {
            return None;
        }

        match self.settings.section {
            SettingsSection::ThemeMode => {
                let list_y = area.y + 3;
                if row >= list_y && row < list_y + ThemeMode::ALL.len() as u16 * 2 {
                    Some(((row - list_y) / 2) as usize)
                } else {
                    None
                }
            }
            SettingsSection::Theme => {
                let idx = self.settings.scroll + (row - area.y) as usize;
                (idx < theme_list_len(self)).then_some(idx)
            }
            SettingsSection::Sound => {
                let list_y = area.y + 3;
                if row >= list_y && row < list_y + 4 {
                    Some(((row - list_y) / 2) as usize)
                } else {
                    None
                }
            }
            SettingsSection::Toast => {
                let list_y = area.y + 3;
                if row >= list_y && row < list_y + 6 {
                    Some(((row - list_y) / 2) as usize)
                } else {
                    None
                }
            }
            SettingsSection::PaneLabels => {
                let list_y = area.y + 3;
                if row >= list_y && row < list_y + 4 {
                    Some(((row - list_y) / 2) as usize)
                } else {
                    None
                }
            }
        }
    }

    fn settings_theme_scrollbar_target_at(
        &self,
        col: u16,
        row: u16,
    ) -> Option<ScrollbarClickTarget> {
        if self.settings.section != SettingsSection::Theme {
            return None;
        }
        let metrics = settings_theme_scroll_metrics(self);
        let track = crate::ui::modal_scrollbar_rect(self.settings_content_rect(), metrics)?;
        if !(col >= track.x
            && col < track.x + track.width
            && row >= track.y
            && row < track.y + track.height)
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

    fn settings_theme_offset_for_drag_row(&self, row: u16, grab_row_offset: u16) -> Option<usize> {
        if self.settings.section != SettingsSection::Theme {
            return None;
        }
        let metrics = settings_theme_scroll_metrics(self);
        let track = crate::ui::modal_scrollbar_rect(self.settings_content_rect(), metrics)?;
        Some(crate::ui::scrollbar_offset_from_drag_row(
            metrics,
            track,
            row,
            grab_row_offset,
        ))
    }

    pub(super) fn handle_settings_mouse(&mut self, mouse: MouseEvent) -> Option<SettingsAction> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(target) =
                    self.settings_theme_scrollbar_target_at(mouse.column, mouse.row)
                {
                    match target {
                        ScrollbarClickTarget::Thumb { grab_row_offset } => {
                            self.drag = Some(DragState {
                                target: DragTarget::SettingsThemeScrollbar { grab_row_offset },
                            });
                        }
                        ScrollbarClickTarget::Track { offset_from_bottom } => {
                            set_settings_theme_offset_from_bottom(self, offset_from_bottom);
                        }
                    }
                    return None;
                }

                if let Some(section) = self.settings_tab_at(mouse.column, mouse.row) {
                    self.settings.section = section;
                    self.settings.list.select(match section {
                        SettingsSection::ThemeMode => {
                            current_theme_mode_index(pending_theme_mode(self))
                        }
                        SettingsSection::Theme => target_theme_index(self),
                        SettingsSection::Sound => usize::from(!self.sound_enabled()),
                        SettingsSection::Toast => toast_delivery_index(self.toast_delivery()),
                        SettingsSection::PaneLabels => {
                            usize::from(!self.agent_border_labels_enabled())
                        }
                    });
                    if section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                    return None;
                }
                if let Some(idx) = self.settings_list_index_at(mouse.column, mouse.row) {
                    self.settings.list.select(idx);
                    if self.settings.section == SettingsSection::Theme {
                        ensure_settings_selection_visible(self);
                    }
                    return match self.settings.section {
                        SettingsSection::ThemeMode => {
                            preview_selected_theme_mode(self);
                            None
                        }
                        SettingsSection::Theme => {
                            preview_selected_theme(self);
                            None
                        }
                        SettingsSection::Sound => {
                            let enabled = idx == 0;
                            Some(SettingsAction::SaveSound(enabled))
                        }
                        SettingsSection::Toast => {
                            let delivery = toast_delivery_for_index(idx);
                            Some(SettingsAction::SaveToastDelivery(delivery))
                        }
                        SettingsSection::PaneLabels => {
                            let enabled = idx == 0;
                            Some(SettingsAction::SaveAgentBorderLabels(enabled))
                        }
                    };
                }

                let inner = self.settings_inner_rect();
                let (apply, close) = crate::ui::settings_button_rects(inner);
                match super::modal::modal_action_from_buttons(
                    mouse.column,
                    mouse.row,
                    &[
                        (apply, super::modal::ModalAction::Apply),
                        (close, super::modal::ModalAction::Close),
                    ],
                ) {
                    Some(super::modal::ModalAction::Apply) => apply_settings(self),
                    Some(super::modal::ModalAction::Close) => {
                        cancel_settings(self);
                        None
                    }
                    _ => {
                        cancel_settings(self);
                        None
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(DragState {
                    target: DragTarget::SettingsThemeScrollbar { grab_row_offset },
                }) = &self.drag
                {
                    if let Some(offset_from_bottom) =
                        self.settings_theme_offset_for_drag_row(mouse.row, *grab_row_offset)
                    {
                        set_settings_theme_offset_from_bottom(self, offset_from_bottom);
                    }
                }
                None
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.drag.as_ref().is_some_and(|drag| {
                    matches!(drag.target, DragTarget::SettingsThemeScrollbar { .. })
                }) {
                    self.drag = None;
                }
                None
            }
            MouseEventKind::ScrollUp if self.settings.section == SettingsSection::Theme => {
                self.settings.scroll = self.settings.scroll.saturating_sub(3);
                None
            }
            MouseEventKind::ScrollDown if self.settings.section == SettingsSection::Theme => {
                self.settings.scroll = self
                    .settings
                    .scroll
                    .saturating_add(3)
                    .min(settings_theme_max_scroll(self));
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

    use super::super::{app_for_mouse_test, mouse, state_with_workspaces};
    use super::*;

    #[test]
    fn settings_cancel_restores_previewed_theme_from_other_sections() {
        let mut state = state_with_workspaces(&["test"]);
        let original_palette = state.palette.clone();
        let original_theme = state.theme_name.clone();

        open_settings(&mut state);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        assert_ne!(state.theme_name, original_theme);

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );
        assert_eq!(
            state.settings.section,
            crate::app::state::SettingsSection::Sound
        );

        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        );

        assert_eq!(state.mode, Mode::Terminal);
        assert_eq!(state.theme_name, original_theme);
        assert_eq!(state.palette.accent, original_palette.accent);
        assert_eq!(state.palette.panel_bg, original_palette.panel_bg);
    }

    #[test]
    fn group_theme_settings_apply_returns_group_theme_action() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_theme_settings(&mut state, group_idx);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        let theme_name = state.theme_name.clone();
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveGroupTheme {
                group_idx,
                name: Some(theme_name),
            })
        );
        assert_eq!(state.settings.group_theme_target, None);
    }

    #[test]
    fn group_theme_settings_default_keeps_group_on_global_theme() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_theme_settings(&mut state, group_idx);
        assert_eq!(state.settings.list.selected, 0);
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveGroupTheme {
                group_idx,
                name: None,
            })
        );
    }

    #[test]
    fn group_theme_settings_does_not_switch_to_other_settings_sections() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());

        open_group_theme_settings(&mut state, group_idx);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        );

        assert_eq!(state.settings.section, SettingsSection::Theme);
        assert_eq!(state.settings.group_theme_target, Some(group_idx));
    }

    #[test]
    fn group_theme_settings_uses_smaller_theme_only_modal() {
        let mut state = state_with_workspaces(&["test"]);
        let group_idx = state.create_group("Side".to_string());
        state.view.terminal_area = Rect::new(0, 0, 100, 40);

        open_group_theme_settings(&mut state, group_idx);
        let group_rect = state.settings_popup_rect();

        open_settings(&mut state);
        let settings_rect = state.settings_popup_rect();

        assert_eq!(group_rect.width, 56);
        assert_eq!(group_rect.height, 20);
        assert!(group_rect.width < settings_rect.width);
        assert!(group_rect.height < settings_rect.height);
    }

    #[test]
    fn group_theme_apply_uses_full_screen_geometry_with_right_sidebar() {
        let mut app = app_for_mouse_test();
        app.state.view.right_sidebar_rect = Rect::new(106, 0, 34, 20);
        let group_idx = app.state.create_group("Side".to_string());

        open_group_theme_settings(&mut app.state, group_idx);
        app.state.settings.list.selected = 1;
        let inner = app.state.settings_inner_rect();
        let (apply, _) = crate::ui::settings_button_rects(inner);
        app.handle_mouse(mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            apply.x,
            apply.y,
        ));

        assert_eq!(
            app.state.groups[group_idx].theme_name.as_deref(),
            Some("system")
        );
    }

    #[test]
    fn global_theme_settings_apply_returns_theme_family() {
        let mut state = state_with_workspaces(&["test"]);

        open_settings(&mut state);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveTheme {
                name: "tokyo-night".to_string(),
                mode: ThemeMode::System,
            })
        );
    }

    #[test]
    fn global_mode_settings_apply_returns_theme_mode() {
        let mut state = state_with_workspaces(&["test"]);

        open_settings(&mut state);
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Left, KeyModifiers::empty()),
        );
        update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        );
        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(
            action,
            Some(SettingsAction::SaveTheme {
                name: "catppuccin".to_string(),
                mode: ThemeMode::Light,
            })
        );
    }

    #[test]
    fn settings_sound_toggle_returns_save_action() {
        let mut state = state_with_workspaces(&["test"]);
        open_settings(&mut state);
        state.settings.section = crate::app::state::SettingsSection::Sound;
        state.settings.list.selected = 0;

        let action = update_settings_state(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        );

        assert_eq!(action, Some(SettingsAction::SaveSound(true)));
        assert!(!state.sound.enabled);
        assert_eq!(state.mode, Mode::Settings);
    }

    #[test]
    fn settings_hover_does_not_change_selection() {
        let mut app = app_for_mouse_test();
        open_settings(&mut app.state);
        app.state.settings.list.select(0);

        let area = app.state.settings_content_rect();
        app.handle_mouse(mouse(MouseEventKind::Moved, area.x + 2, area.y + 2));

        assert_eq!(app.state.settings.list.selected, 0);
    }
}
