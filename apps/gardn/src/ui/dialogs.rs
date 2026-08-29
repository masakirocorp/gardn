use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::{
    text::display_width_u16,
    widgets::{
        action_button_row_rects, centered_popup_rect, danger_action_style, panel_contrast_fg,
        primary_action_style, render_action_button, render_modal_description, render_modal_divider,
        render_modal_header_bar, render_modal_shell, render_modal_text_input, render_panel_shell,
        secondary_action_style, ActionButtonSpec,
    },
};

use crate::{
    app::{AppState, Mode},
    terminal::TerminalRuntimeRegistry,
};

pub(crate) fn rename_button_rects(inner: Rect) -> (Rect, Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: "Save",
            },
            ActionButtonSpec {
                hint: Some("^c"),
                label: "Clear",
            },
        ],
        2,
        inner.height.saturating_sub(1),
    );
    let close =
        super::widgets::modal_close_button_rect(Rect::new(inner.x, inner.y, inner.width, 1));
    (rects[0], rects[1], close)
}

pub(crate) fn rename_modal_size_for_view(mode: Mode, creating_new_group: bool) -> (u16, u16) {
    if matches!(mode, Mode::RenameGroup) && creating_new_group {
        (64, 22)
    } else if matches!(mode, Mode::RenameGroup) {
        (56, 17)
    } else {
        (56, 7)
    }
}

pub(crate) fn rename_modal_size(app: &AppState) -> (u16, u16) {
    rename_modal_size_for_view(app.mode, app.creating_new_group)
}

pub(crate) fn rename_name_input_rect(app: &AppState, inner: Rect) -> Rect {
    rename_name_input_rect_for_view(app.mode, app.creating_new_group, inner)
}

pub(crate) fn rename_name_input_rect_for_view(
    mode: Mode,
    creating_new_group: bool,
    inner: Rect,
) -> Rect {
    if matches!(mode, Mode::RenameGroup) {
        return group_name_input_rect_for_view(creating_new_group, inner);
    }
    if inner.width == 0 || inner.height < 3 {
        return Rect::default();
    }
    Rect::new(inner.x, inner.y + 2, inner.width, 1)
}

#[derive(Clone, Copy)]
struct GroupModalLayout {
    name_label_y: u16,
    name_input_y: u16,
    icon_y: u16,
    picker_y: Option<u16>,
    host_y: Option<u16>,
    directory_label_y: Option<u16>,
    directory_input_y: Option<u16>,
}

fn group_modal_layout(creating_new_group: bool, picker_open: bool) -> GroupModalLayout {
    let host_y = creating_new_group.then_some(if picker_open { 14 } else { 11 });
    GroupModalLayout {
        name_label_y: 6,
        name_input_y: 7,
        icon_y: 9,
        picker_y: picker_open.then_some(10),
        host_y,
        directory_label_y: host_y.map(|y| y + 2),
        directory_input_y: host_y.map(|y| y + 3),
    }
}

fn group_field_rect_for_view(creating_new_group: bool, inner: Rect, y: u16) -> Rect {
    if inner.width == 0 || inner.height <= y {
        return Rect::default();
    }
    let left = if creating_new_group { 1 } else { 2 };
    Rect::new(
        inner.x + left,
        inner.y + y,
        inner.width.saturating_sub(left),
        1,
    )
}

pub(crate) fn group_icon_button_rect_for_view(creating_new_group: bool, inner: Rect) -> Rect {
    group_field_rect_for_view(
        creating_new_group,
        inner,
        group_modal_layout(creating_new_group, false).icon_y,
    )
}

pub(crate) fn group_name_input_rect_for_view(creating_new_group: bool, inner: Rect) -> Rect {
    group_field_rect_for_view(
        creating_new_group,
        inner,
        group_modal_layout(creating_new_group, false).name_input_y,
    )
}

pub(crate) fn group_default_directory_input_rect_for_view(
    creating_new_group: bool,
    group_icon_picker_open: bool,
    inner: Rect,
) -> Rect {
    let Some(y) = group_modal_layout(creating_new_group, group_icon_picker_open).directory_input_y
    else {
        return Rect::default();
    };
    group_field_rect_for_view(creating_new_group, inner, y)
}

pub(crate) fn group_default_host_rect_for_view(
    creating_new_group: bool,
    group_icon_picker_open: bool,
    inner: Rect,
) -> Rect {
    let Some(y) = group_modal_layout(creating_new_group, group_icon_picker_open).host_y else {
        return Rect::default();
    };
    group_field_rect_for_view(creating_new_group, inner, y)
}

pub(crate) fn group_icon_picker_row_count() -> u16 {
    let count = crate::app::state::GROUP_ICONS.len() as u16;
    count.div_ceil(5)
}

pub(crate) fn group_icon_picker_rects_at(start: Rect) -> Vec<(Rect, &'static str)> {
    if start.width < 3 || start.height == 0 {
        return Vec::new();
    }
    crate::app::state::GROUP_ICONS
        .iter()
        .enumerate()
        .filter_map(|(idx, icon)| {
            let col = (idx % 5) as u16;
            let row = (idx / 5) as u16;
            if row >= start.height {
                return None;
            }
            let x = start.x + col * 4;
            if x >= start.x + start.width {
                return None;
            }
            Some((Rect::new(x, start.y + row, 3, 1), *icon))
        })
        .collect()
}

pub(crate) fn group_icon_picker_rects_for_view(
    creating_new_group: bool,
    inner: Rect,
) -> Vec<(Rect, &'static str)> {
    let Some(y) = group_modal_layout(creating_new_group, true).picker_y else {
        return Vec::new();
    };
    let field = group_field_rect_for_view(creating_new_group, inner, y);
    let start = Rect::new(field.x, inner.y + y, field.width.min(24), 3);
    group_icon_picker_rects_at(start)
}

pub(crate) fn group_icon_button_rect(app: &AppState, inner: Rect) -> Rect {
    group_icon_button_rect_for_view(app.creating_new_group, inner)
}

pub(crate) fn group_name_input_rect(app: &AppState, inner: Rect) -> Rect {
    group_name_input_rect_for_view(app.creating_new_group, inner)
}

pub(crate) fn group_default_directory_input_rect(app: &AppState, inner: Rect) -> Rect {
    group_default_directory_input_rect_for_view(
        app.creating_new_group,
        app.group_icon_picker_open,
        inner,
    )
}

pub(crate) fn group_default_host_rect(app: &AppState, inner: Rect) -> Rect {
    group_default_host_rect_for_view(app.creating_new_group, app.group_icon_picker_open, inner)
}

pub(crate) fn group_icon_picker_rects(app: &AppState, inner: Rect) -> Vec<(Rect, &'static str)> {
    group_icon_picker_rects_for_view(app.creating_new_group, inner)
}

fn group_host_label(app: &AppState, host_id: &crate::execution_host::ExecutionHostId) -> String {
    if host_id.is_local() {
        "Local".to_string()
    } else {
        app.ssh_connection_profiles
            .iter()
            .find(|profile| profile.execution_host_id() == *host_id)
            .map(|profile| profile.name().to_string())
            .unwrap_or_else(|| host_id.as_str().to_string())
    }
}

fn render_group_status_row(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    selected: bool,
    palette: &crate::app::state::Palette,
) {
    if area.width == 0 {
        return;
    }
    let label_style = if selected {
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.overlay0)
    };
    let value_style = if selected {
        Style::default()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette.accent)
    };
    let value_text = format!("‹ {value} ›");
    let used = display_width_u16(label).saturating_add(display_width_u16(&value_text));
    let gap = area.width.saturating_sub(used) as usize;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(label.to_string(), label_style),
            Span::raw(" ".repeat(gap)),
            Span::styled(value_text, value_style),
        ])),
        area,
    );
}

fn render_group_text_bar(
    frame: &mut Frame,
    area: Rect,
    value: &str,
    palette: &crate::app::state::Palette,
    focused: bool,
) {
    if focused {
        render_modal_text_input(frame, area, value, palette);
        return;
    }
    if area.width == 0 {
        return;
    }
    let visible = super::text::truncate_end(value, area.width.saturating_sub(2) as usize);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!(" {visible}"))
            .style(Style::default().fg(palette.text).bg(palette.surface0)),
        area,
    );
}

fn render_group_modal_fields(
    app: &AppState,
    frame: &mut Frame,
    inner: Rect,
    creating_new_group: bool,
    picker_open: bool,
    selected_field: usize,
    name: &str,
    icon: &str,
    directory: &str,
    host_id: &crate::execution_host::ExecutionHostId,
    palette: &crate::app::state::Palette,
) {
    let layout = group_modal_layout(creating_new_group, picker_open);
    let field = |y| group_field_rect_for_view(creating_new_group, inner, y);
    let description = if creating_new_group {
        "Set this group's name, icon, and default location"
    } else {
        "Rename this group or change its icon"
    };
    render_modal_description(
        frame,
        Rect::new(inner.x, inner.y + 3, inner.width, 1),
        "General",
        Style::default().fg(palette.accent),
    );
    render_modal_description(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width, 1),
        description,
        Style::default().fg(palette.overlay0),
    );

    let name_selected = selected_field == 0;
    frame.render_widget(
        Paragraph::new("Name").style(if name_selected {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay0)
        }),
        field(layout.name_label_y),
    );
    render_group_text_bar(
        frame,
        field(layout.name_input_y),
        name,
        palette,
        name_selected,
    );

    render_group_status_row(
        frame,
        field(layout.icon_y),
        "Icon",
        icon,
        picker_open,
        palette,
    );

    if let Some(host_y) = layout.host_y {
        render_group_status_row(
            frame,
            field(host_y),
            "Default Location for New Spaces",
            &group_host_label(app, host_id),
            selected_field == 1,
            palette,
        );
    }
    if let (Some(label_y), Some(input_y)) = (layout.directory_label_y, layout.directory_input_y) {
        let directory_selected = selected_field == 2;
        frame.render_widget(
            Paragraph::new("Directory").style(if directory_selected {
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.overlay0)
            }),
            field(label_y),
        );
        render_group_text_bar(
            frame,
            field(input_y),
            directory,
            palette,
            directory_selected,
        );
    }

    if picker_open {
        for (rect, picker_icon) in group_icon_picker_rects_for_view(creating_new_group, inner) {
            let selected = icon == picker_icon;
            frame.render_widget(
                Paragraph::new(format!(" {picker_icon} "))
                    .style(if selected {
                        Style::default()
                            .fg(panel_contrast_fg(palette))
                            .bg(palette.accent)
                    } else {
                        Style::default().fg(palette.text).bg(palette.surface0)
                    })
                    .alignment(Alignment::Center),
                rect,
            );
        }
    }
}

fn rename_palette(app: &AppState) -> crate::app::state::Palette {
    match app.mode {
        Mode::RenameWorkspace | Mode::RenameTab | Mode::RenamePane => app
            .active
            .map(|ws_idx| app.palette_for_workspace(ws_idx))
            .unwrap_or_else(|| app.palette.clone()),
        Mode::RenameGroup if !app.creating_new_group => app
            .rename_group_target
            .map(|group_idx| app.palette_for_group(group_idx))
            .unwrap_or_else(|| app.palette_for_group(app.active_group)),
        _ => app.palette.clone(),
    }
}

pub(super) fn render_rename_overlay_for_view(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    render_rename_overlay_with_view_state(app, client_view, frame, area);
}

fn render_rename_overlay_with_view_state(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    // The shared data model owns workspace/group metadata and palettes; all modal
    // state below is selected from the requesting client's view.
    let title = match client_view.mode {
        Mode::RenameWorkspace if client_view.pending_workspace_create_location.is_some() => {
            "New Workspace"
        }
        Mode::RenameWorkspace => "Rename Workspace",
        Mode::RenameGroup if client_view.creating_new_group => "New Group",
        Mode::RenameGroup => "Rename Group",
        Mode::RenameTab if client_view.creating_new_tab => "New Tab",
        Mode::RenameTab => "Rename Tab",
        Mode::RenamePane => "Rename Pane",
        _ => return,
    };
    let palette = match client_view.mode {
        Mode::RenameWorkspace | Mode::RenameTab | Mode::RenamePane => client_view
            .active_workspace
            .map(|workspace| app.palette_for_workspace(workspace))
            .unwrap_or_else(|| app.palette.clone()),
        Mode::RenameGroup if !client_view.creating_new_group => client_view
            .rename_group_target
            .map(|group| app.palette_for_group(group))
            .unwrap_or_else(|| app.palette_for_group(client_view.active_group)),
        _ => app.palette.clone(),
    };
    let (popup_w, popup_h) =
        rename_modal_size_for_view(client_view.mode, client_view.creating_new_group);
    super::dim_background(frame, area);
    let Some(inner) = render_modal_shell(frame, area, popup_w, popup_h, &palette) else {
        return;
    };
    if inner.height < 4 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<3>(inner);
    render_modal_header_bar(frame, rows[0], title, &palette, true);
    if matches!(client_view.mode, Mode::RenameGroup) {
        render_modal_divider(frame, rows[1], &palette);
        render_group_modal_fields(
            app,
            frame,
            inner,
            client_view.creating_new_group,
            client_view.group_icon_picker_open,
            client_view.group_modal_selected_field,
            &client_view.name_input,
            &client_view.group_icon_input,
            &client_view.group_default_directory_input,
            &client_view.group_default_execution_host_id,
            &palette,
        );
    } else {
        render_modal_text_input(
            frame,
            rename_name_input_rect_for_view(
                client_view.mode,
                client_view.creating_new_group,
                inner,
            ),
            &client_view.name_input,
            &palette,
        );
        if let Some(location) = client_view.pending_workspace_create_location.as_ref() {
            frame.render_widget(
                Paragraph::new(format!(
                    "Runs On {} · Directory {}",
                    group_host_label(app, &location.execution_host_id),
                    location.path.as_path().display()
                ))
                .style(Style::default().fg(palette.overlay0)),
                Rect::new(rows[2].x, rows[2].y + 2, rows[2].width, 1),
            );
        }
    }

    let (save_rect, clear_rect, _) = rename_button_rects(inner);
    render_action_button(
        frame,
        save_rect,
        Some("↵"),
        "Save",
        primary_action_style(&palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "Clear",
        secondary_action_style(&palette),
    );
}

pub(super) fn render_rename_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    super::dim_background(frame, area);

    let title = match app.mode {
        Mode::RenameWorkspace if app.pending_workspace_create_location.is_some() => "New Workspace",
        Mode::RenameWorkspace => "Rename Workspace",
        Mode::RenameGroup if app.creating_new_group => "New Group",
        Mode::RenameGroup => "Rename Group",
        Mode::RenameTab if app.creating_new_tab => "New Tab",
        Mode::RenameTab => "Rename Tab",
        Mode::RenamePane => "Rename Pane",
        _ => return,
    };

    let palette = rename_palette(app);
    let (popup_w, popup_h) = rename_modal_size(app);
    let Some(inner) = render_modal_shell(frame, area, popup_w, popup_h, &palette) else {
        return;
    };
    if inner.height < 4 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<3>(inner);

    render_modal_header_bar(frame, rows[0], title, &palette, true);
    if matches!(app.mode, Mode::RenameGroup) {
        render_modal_divider(frame, rows[1], &palette);
        render_group_modal_fields(
            app,
            frame,
            inner,
            app.creating_new_group,
            app.group_icon_picker_open,
            app.group_modal_selected_field,
            &app.name_input,
            &app.group_icon_input,
            &app.group_default_directory_input,
            &app.group_default_execution_host_id,
            &palette,
        );
    } else {
        let input_rect = rename_name_input_rect(app, inner);
        render_modal_text_input(frame, input_rect, &app.name_input, &palette);
        if let Some(location) = app.pending_workspace_create_location.as_ref() {
            frame.render_widget(
                Paragraph::new(format!(
                    "Runs On {} · Directory {}",
                    group_host_label(app, &location.execution_host_id),
                    location.path.as_path().display()
                ))
                .style(Style::default().fg(palette.overlay0)),
                Rect::new(rows[2].x, rows[2].y + 2, rows[2].width, 1),
            );
        }
    }

    let (save_rect, clear_rect, _) = rename_button_rects(inner);

    render_action_button(
        frame,
        save_rect,
        Some("↵"),
        "Save",
        primary_action_style(&palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "Clear",
        secondary_action_style(&palette),
    );
}

pub(super) fn render_confirm_close_overlay_for_view(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    render_confirm_close_overlay_with(
        app,
        client_view.selected_workspace,
        terminal_runtimes,
        frame,
        area,
    );
}

fn render_confirm_close_overlay_with(
    app: &AppState,
    selected_workspace: usize,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let ws_name = app
        .workspaces
        .get(selected_workspace)
        .map(|ws| ws.display_name_from(&app.terminals, terminal_runtimes))
        .unwrap_or_else(|| "?".to_string());
    let pane_count = app
        .workspaces
        .get(selected_workspace)
        .map(|ws| ws.tabs.iter().map(|tab| tab.layout.pane_count()).sum())
        .unwrap_or(0);

    let pane_text = if pane_count == 1 {
        "1 Pane".to_string()
    } else {
        format!("{pane_count} Panes")
    };

    super::dim_background(frame, area);

    let Some(popup) = confirm_close_popup_rect(area) else {
        return;
    };

    let palette = app.palette_for_workspace(selected_workspace);
    let warn = Style::default()
        .fg(palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" Close Workspace?", warn)]);

    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {ws_name}"),
            Style::default()
                .fg(palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — {pane_text}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, palette.red, palette.panel_bg) else {
        return;
    };

    if inner.height >= 3 {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<3>(inner);

        frame.render_widget(Paragraph::new(title_line), rows[0]);
        frame.render_widget(Paragraph::new(detail_line), rows[1]);

        let (confirm_rect, cancel_rect) = confirm_close_button_rects(inner);
        render_action_button(
            frame,
            confirm_rect,
            Some("↵"),
            "Confirm",
            danger_action_style(&palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("Esc"),
            "Cancel",
            secondary_action_style(&palette),
        );
    }
}

pub(super) fn render_confirm_close_overlay(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    render_confirm_close_overlay_with(app, app.selected, terminal_runtimes, frame, area);
}
pub(super) fn render_confirm_delete_group_overlay_for_view(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    render_confirm_delete_group_overlay_with(
        app,
        client_view
            .confirm_delete_group
            .unwrap_or(client_view.active_group),
        frame,
        area,
    );
}

fn render_confirm_delete_group_overlay_with(
    app: &AppState,
    group_idx: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let group = app.groups.get(group_idx);
    let group_name = group.map(|group| group.name.as_str()).unwrap_or("?");
    let space_count = app
        .groups
        .get(group_idx)
        .map(|group| {
            app.workspaces
                .iter()
                .filter(|workspace| workspace.group_id == group.id)
                .count()
        })
        .unwrap_or(0);
    let spaces = if space_count == 1 {
        "1 Space".to_string()
    } else {
        format!("{space_count} Spaces")
    };

    super::dim_background(frame, area);
    let Some(popup) = confirm_close_popup_rect(area) else {
        return;
    };

    let palette = app.palette_for_group(group_idx);
    let warn = Style::default()
        .fg(palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(palette.overlay0);
    let title_line = Line::from(vec![Span::styled(" Delete Group?", warn)]);
    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {group_name}"),
            Style::default()
                .fg(app.group_accent_color(group_idx))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — Closes {spaces}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, palette.red, palette.panel_bg) else {
        return;
    };
    if inner.height >= 3 {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<3>(inner);
        frame.render_widget(Paragraph::new(title_line), rows[0]);
        frame.render_widget(Paragraph::new(detail_line), rows[1]);
        let (confirm_rect, cancel_rect) = confirm_close_button_rects(inner);
        render_action_button(
            frame,
            confirm_rect,
            Some("↵"),
            "Confirm",
            danger_action_style(&palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("Esc"),
            "Cancel",
            secondary_action_style(&palette),
        );
    }
}

pub(super) fn render_confirm_delete_group_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    render_confirm_delete_group_overlay_with(
        app,
        app.confirm_delete_group.unwrap_or(app.active_group),
        frame,
        area,
    );
}

pub(crate) fn confirm_close_popup_rect(area: Rect) -> Option<Rect> {
    centered_popup_rect(area, 44, 5)
}

pub(crate) fn confirm_close_button_rects(inner: Rect) -> (Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: "Confirm",
            },
            ActionButtonSpec {
                hint: Some("Esc"),
                label: "Cancel",
            },
        ],
        2,
        2,
    );
    (rects[0], rects[1])
}

pub(crate) fn render_authentication_overlay_for_view(
    app: &AppState,
    client_view: &crate::app::ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let Some(prompt) = client_view.authentication_prompt.as_ref() else {
        return;
    };
    super::dim_background(frame, area);
    let palette = client_view
        .active_workspace
        .map(|workspace| app.palette_for_workspace(workspace))
        .unwrap_or_else(|| app.palette.clone());
    let Some(inner) = render_modal_shell(frame, area, 64, 11, &palette) else {
        return;
    };
    render_modal_header_bar(frame, inner, "SSH Authentication Required", &palette, true);
    render_modal_divider(
        frame,
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
        &palette,
    );
    frame.render_widget(
        Paragraph::new(prompt.execution_host_id.as_str())
            .style(Style::default().fg(palette.overlay0)),
        Rect::new(inner.x + 1, inner.y + 3, inner.width.saturating_sub(2), 1),
    );
    frame.render_widget(
        Paragraph::new(prompt.prompt.clone())
            .wrap(ratatui::widgets::Wrap { trim: true })
            .style(Style::default().fg(palette.text)),
        Rect::new(inner.x + 1, inner.y + 4, inner.width.saturating_sub(2), 2),
    );
    let guidance = if prompt.host_key_confirmation {
        "Y Confirm Host Key  ·  N / Esc Cancel"
    } else {
        "Enter Submit  ·  Esc Cancel"
    };
    frame.render_widget(
        Paragraph::new(guidance).style(Style::default().fg(palette.overlay0)),
        Rect::new(inner.x + 1, inner.y + 7, inner.width.saturating_sub(2), 1),
    );
    if !prompt.host_key_confirmation {
        let masked = "•".repeat(prompt.response.chars().count());
        frame.render_widget(
            Paragraph::new(masked).style(Style::default().fg(palette.text).bg(palette.surface0)),
            Rect::new(inner.x + 1, inner.y + 8, inner.width.saturating_sub(2), 1),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn authentication_overlay_masks_secret_input() {
        let app = AppState::test_new();
        let mut view = crate::app::ClientViewState::from_default_client_state(&app);
        view.authentication_prompt = Some(crate::app::view_state::ClientAuthenticationPrompt {
            challenge_id: 7,
            execution_host_id: crate::execution_host::ExecutionHostId::new("ssh:workbox:1")
                .unwrap(),
            prompt: "Password:".to_string(),
            response: "hunter2".to_string(),
            host_key_confirmation: false,
        });
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_authentication_overlay_for_view(&app, &view, frame, Rect::new(0, 0, 80, 24))
            })
            .unwrap();
        let text = buffer_text(terminal.backend().buffer(), 80, 24);

        assert!(text.contains("SSH Authentication Required"));
        assert!(text.contains("•••••••"));
        assert!(!text.contains("hunter2"));
    }

    #[test]
    fn host_key_overlay_requires_explicit_yes_or_no() {
        let app = AppState::test_new();
        let mut view = crate::app::ClientViewState::from_default_client_state(&app);
        view.authentication_prompt = Some(crate::app::view_state::ClientAuthenticationPrompt {
            challenge_id: 8,
            execution_host_id: crate::execution_host::ExecutionHostId::new("ssh:workbox:1")
                .unwrap(),
            prompt: "Continue connecting (yes/no)?".to_string(),
            response: String::new(),
            host_key_confirmation: true,
        });
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_authentication_overlay_for_view(&app, &view, frame, Rect::new(0, 0, 80, 24))
            })
            .unwrap();
        let text = buffer_text(terminal.backend().buffer(), 80, 24);

        assert!(text.contains("Y Confirm Host Key"));
        assert!(text.contains("N / Esc Cancel"));
    }

    #[test]
    fn confirm_close_overlay_renders_empty_workspace() {
        let mut app = AppState::test_new();
        app.workspaces = vec![Workspace::test_new("empty")];
        app.workspaces[0].tabs.clear();
        app.selected = 0;
        app.active = Some(0);
        app.mode = Mode::ConfirmClose;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_confirm_close_overlay(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    Rect::new(0, 0, 80, 24),
                )
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("Close Workspace?"));
        assert!(text.contains("empty"));
        assert!(text.contains("0 Panes"));
        assert!(text.contains("↵"));
        assert!(text.contains("Confirm"));
        assert!(text.contains("Esc"));
        assert!(text.contains("Cancel"));
    }

    #[test]
    fn confirm_close_overlay_uses_live_cwd_of_keyboard_or_mouse_target() {
        let mut app = AppState::test_new();
        let active = Workspace::test_new("active");
        let mut target = Workspace::test_new("original");
        target.custom_name = None;
        target.identity_cwd = "/projects/original".into();
        let target_pane = target.tabs[0].root_pane;
        let target_terminal_id = target.tabs[0].panes[&target_pane]
            .attached_terminal_id
            .clone();
        app.workspaces = vec![active, target];
        app.ensure_test_terminals();
        app.terminals
            .get_mut(&target_terminal_id)
            .expect("target terminal")
            .cwd = "/projects/current".into();
        app.active = Some(0);
        app.selected = 1;
        app.mode = Mode::ConfirmClose;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_confirm_close_overlay(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    Rect::new(0, 0, 80, 24),
                )
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(text.contains("current"), "close target copy: {text}");
        assert!(
            !text.contains("original"),
            "stale close target copy: {text}"
        );
    }

    #[test]
    fn confirm_close_overlay_for_view_uses_client_target_name() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("active"),
            Workspace::test_new("selected"),
        ];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.selected = 0;
        let mut client_view = crate::app::ClientViewState::from_default_client_state(&app);
        client_view.selected_workspace = 1;
        client_view.mode = Mode::ConfirmClose;

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_confirm_close_overlay_for_view(
                    &app,
                    &client_view,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    Rect::new(0, 0, 80, 24),
                )
            })
            .unwrap();

        let text = buffer_text(terminal.backend().buffer(), 80, 24);
        assert!(
            text.contains("selected"),
            "client close target copy: {text}"
        );
        assert!(
            !text.contains("active"),
            "wrong client close target copy: {text}"
        );
    }

    #[test]

    fn confirm_delete_group_uses_group_accent_for_name() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Yellow));
        app.confirm_delete_group = Some(group_idx);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_confirm_delete_group_overlay(&app, frame, Rect::new(0, 0, 80, 24)))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_symbol(buffer, 80, 24, "w").expect("group name");
        assert_eq!(
            buffer[(x, y)].style().fg,
            Some(app.group_accent_color(group_idx))
        );
    }

    #[test]
    fn new_group_overlay_renders_optional_default_directory_field() {
        let mut app = AppState::test_new();
        app.mode = Mode::RenameGroup;
        app.creating_new_group = true;
        app.name_input = "Work".to_string();
        app.group_icon_input = "✿".to_string();
        app.group_default_directory_input = "/tmp/work".to_string();
        app.group_modal_selected_field = 2;

        let backend = TestBackend::new(90, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_rename_overlay(&app, frame, Rect::new(0, 0, 90, 28)))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 90, 28);
        assert!(text.contains("New Group"));
        assert!(text.contains("Set this group's name, icon, and default location"));
        assert!(text.contains("Default Location for New Spaces"));
        assert!(text.contains("‹ Local ›"));
        assert!(text.contains("Directory"));
        assert!(text.contains("/tmp/work"));
        assert!(text.contains("Save"));
        assert!(text.contains("Clear"));
        assert!(!text.contains("Name + Icon + Runs On + Directory"));
        assert!(!text.contains("Runs On ·"));

        let (popup_w, popup_h) = rename_modal_size(&app);
        let popup = centered_popup_rect(Rect::new(0, 0, 90, 28), popup_w, popup_h).unwrap();
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        assert!(row_text(buffer, inner.x, inner.width, inner.y + 1).contains("─"));
        assert!(row_text(buffer, inner.x, inner.width, inner.y + 2)
            .trim()
            .is_empty());
        assert!(row_text(buffer, inner.x, inner.width, inner.y + 5)
            .trim()
            .is_empty());
        assert!(row_text(buffer, inner.x, inner.width, inner.y + 8)
            .trim()
            .is_empty());
        assert!(row_text(buffer, inner.x, inner.width, inner.y + 10)
            .trim()
            .is_empty());
        assert!(row_text(buffer, inner.x, inner.width, inner.y + 12)
            .trim()
            .is_empty());
        assert_eq!(buffer[(inner.x + 1, inner.y + 3)].symbol(), "G");
        assert_eq!(
            buffer[(inner.x + 1, inner.y + 3)].style().fg,
            Some(app.palette.accent)
        );
        assert_eq!(
            buffer[(inner.x + 1, inner.y + 4)].style().fg,
            Some(app.palette.overlay0)
        );
        assert_eq!(buffer[(inner.x + 1, inner.y + 6)].symbol(), "N");
        assert_eq!(group_name_input_rect(&app, inner).x, inner.x + 1);
        assert_eq!(
            group_default_directory_input_rect(&app, inner),
            Rect::new(inner.x + 1, inner.y + 14, inner.width.saturating_sub(1), 1)
        );

        app.group_icon_picker_open = true;
        assert_eq!(
            group_default_directory_input_rect(&app, inner),
            Rect::new(inner.x + 1, inner.y + 17, inner.width.saturating_sub(1), 1)
        );
    }

    #[test]
    fn rename_overlay_caret_reaches_the_frame_the_server_sends() {
        let mut app = AppState::test_new();
        app.mode = Mode::RenameWorkspace;
        app.name_input = "Work".to_string();

        let area = Rect::new(0, 0, 90, 28);
        let (_buffer, cursor) = crate::server::render_stream::render_virtual(&mut app, area, false);
        let cursor = cursor.expect("rename overlay should anchor the host cursor");
        let (popup_w, popup_h) = rename_modal_size(&app);
        let popup = centered_popup_rect(area, popup_w, popup_h).expect("rename popup fits");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let name_rect = rename_name_input_rect(&app, inner);
        assert_eq!(cursor.y, name_rect.y);
        assert_eq!(cursor.x, name_rect.x + 1 + 4);
        assert!(cursor.visible);
    }

    #[test]
    fn rename_overlay_caret_tracks_cjk_display_width() {
        let mut app = AppState::test_new();
        app.mode = Mode::RenameWorkspace;
        app.name_input = "\u{4f5c}\u{696d}".to_string();

        let area = Rect::new(0, 0, 90, 28);
        let (_buffer, cursor) = crate::server::render_stream::render_virtual(&mut app, area, false);
        let cursor = cursor.expect("rename overlay should anchor the host cursor");
        let (popup_w, popup_h) = rename_modal_size(&app);
        let popup = centered_popup_rect(area, popup_w, popup_h).expect("rename popup fits");
        let inner = Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        );
        let name_rect = rename_name_input_rect(&app, inner);
        assert_eq!(cursor.y, name_rect.y);
        // Two wide glyphs occupy four columns, so the caret lands four cells in.
        assert_eq!(cursor.x, name_rect.x + 1 + 4);
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

    fn row_text(buffer: &Buffer, x: u16, width: u16, y: u16) -> String {
        let mut text = String::new();
        for col in x..x + width {
            text.push_str(buffer[(col, y)].symbol());
        }
        text
    }

    fn first_cell_with_symbol(
        buffer: &Buffer,
        width: u16,
        height: u16,
        symbol: &str,
    ) -> Option<(u16, u16)> {
        for y in 0..height {
            for x in 0..width {
                if buffer[(x, y)].symbol() == symbol {
                    return Some((x, y));
                }
            }
        }
        None
    }
}
