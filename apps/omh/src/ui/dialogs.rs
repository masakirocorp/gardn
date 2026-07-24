use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::widgets::{
    action_button_row_rects, centered_popup_rect, danger_action_style, modal_section_heading_style,
    panel_contrast_fg, primary_action_style, render_action_button, render_modal_description,
    render_modal_divider, render_modal_header_bar, render_modal_shell, render_modal_text_input,
    render_panel_shell, secondary_action_style, ActionButtonSpec,
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
                label: "save",
            },
            ActionButtonSpec {
                hint: Some("^c"),
                label: "clear",
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
        (64, 20)
    } else if matches!(mode, Mode::RenameGroup) {
        (56, 17)
    } else {
        (56, 7)
    }
}

pub(crate) fn rename_modal_size(app: &AppState) -> (u16, u16) {
    rename_modal_size_for_view(app.mode, app.creating_new_group)
}

fn group_field_rect_for_view(
    creating_new_group: bool,
    inner: Rect,
    y: u16,
    min_height: u16,
) -> Rect {
    if inner.width == 0 || inner.height < min_height {
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

fn group_field_rect(app: &AppState, inner: Rect, y: u16, min_height: u16) -> Rect {
    group_field_rect_for_view(app.creating_new_group, inner, y, min_height)
}

pub(crate) fn group_icon_button_rect_for_view(creating_new_group: bool, inner: Rect) -> Rect {
    let field = group_field_rect_for_view(creating_new_group, inner, 10, 4);
    if field.width < 3 {
        return Rect::default();
    }
    Rect::new(field.x, field.y, 3, 1)
}

pub(crate) fn group_name_input_rect_for_view(creating_new_group: bool, inner: Rect) -> Rect {
    group_field_rect_for_view(creating_new_group, inner, 7, 6)
}

pub(crate) fn group_default_directory_input_rect_for_view(
    creating_new_group: bool,
    group_icon_picker_open: bool,
    inner: Rect,
) -> Rect {
    // Keep Directory above the action row when popup height clamps on short
    // terminals (20-row screens → inner height 16).
    let y = if group_icon_picker_open { 16 } else { 13 };
    group_field_rect_for_view(creating_new_group, inner, y, 7)
}

pub(crate) fn group_icon_picker_rects_for_view(
    creating_new_group: bool,
    inner: Rect,
) -> Vec<(Rect, &'static str)> {
    let y = inner.y + 11;
    let picker_height = 3;
    let field = group_field_rect_for_view(creating_new_group, inner, 11, 4);
    let start = Rect::new(field.x, y, field.width.min(24), picker_height);
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

pub(crate) fn group_icon_picker_rects(app: &AppState, inner: Rect) -> Vec<(Rect, &'static str)> {
    group_icon_picker_rects_for_view(app.creating_new_group, inner)
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
            "new workspace"
        }
        Mode::RenameWorkspace => "rename workspace",
        Mode::RenameGroup if client_view.creating_new_group => "new group",
        Mode::RenameGroup => "rename group",
        Mode::RenameTab if client_view.creating_new_tab => "new tab",
        Mode::RenameTab => "rename tab",
        Mode::RenamePane => "rename pane",
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
        if matches!(client_view.mode, Mode::RenameGroup) && client_view.creating_new_group {
            (64, 20)
        } else if matches!(client_view.mode, Mode::RenameGroup) {
            (56, 17)
        } else {
            (56, 7)
        };
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
    }
    if matches!(client_view.mode, Mode::RenameGroup) {
        let section_description = if client_view.creating_new_group {
            "name + icon + Runs on + Directory"
        } else {
            "name + icon"
        };
        let group_left = if client_view.creating_new_group { 1 } else { 2 };
        let field = |y| {
            Rect::new(
                inner.x + group_left,
                inner.y + y,
                inner.width.saturating_sub(group_left),
                1,
            )
        };
        render_modal_description(
            frame,
            Rect::new(inner.x, inner.y + 3, inner.width, 1),
            "general",
            Style::default().fg(palette.accent),
        );
        render_modal_description(
            frame,
            Rect::new(inner.x, inner.y + 4, inner.width, 1),
            section_description,
            Style::default().fg(palette.overlay0),
        );
        let name_selected = client_view.group_modal_selected_field == 0;
        frame.render_widget(
            Paragraph::new("name").style(if name_selected {
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.overlay0)
            }),
            field(6),
        );
        let name_rect = field(7);
        if name_selected {
            render_modal_text_input(frame, name_rect, &client_view.name_input, &palette);
        } else {
            super::widgets::render_modal_text_value(
                frame,
                name_rect,
                &client_view.name_input,
                &palette,
            );
        }
        let picker_open = client_view.group_icon_picker_open;
        frame.render_widget(
            Paragraph::new("icon").style(if picker_open {
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.overlay0)
            }),
            field(9),
        );
        let icon_rect = Rect::new(field(10).x, field(10).y, 3, 1);
        frame.render_widget(
            Paragraph::new(format!(" {} ", client_view.group_icon_input))
                .style(if picker_open {
                    Style::default()
                        .fg(panel_contrast_fg(&palette))
                        .bg(palette.accent)
                } else {
                    Style::default().fg(palette.text).bg(palette.surface0)
                })
                .alignment(Alignment::Center),
            icon_rect,
        );
        if client_view.creating_new_group {
            // Single-line host keeps Directory at y=12/13 (15/16 with picker)
            // so save/clear stay hittable on clamped 20-row screens.
            let host_y = if picker_open { 14 } else { 11 };
            let host_selected = client_view.group_modal_selected_field == 1;
            let host_name = if client_view.group_default_execution_host_id.is_local() {
                "Local".to_string()
            } else {
                app.ssh_connection_profiles
                    .iter()
                    .find(|profile| {
                        profile.execution_host_id() == client_view.group_default_execution_host_id
                    })
                    .map(|profile| profile.name().to_string())
                    .unwrap_or_else(|| {
                        client_view
                            .group_default_execution_host_id
                            .as_str()
                            .to_string()
                    })
            };
            frame.render_widget(
                Paragraph::new(format!("Runs on · {host_name}")).style(if host_selected {
                    Style::default()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.overlay0)
                }),
                field(host_y),
            );
            let directory_y = host_y + 1;
            let directory_selected = client_view.group_modal_selected_field == 2;
            frame.render_widget(
                Paragraph::new("Directory").style(if directory_selected {
                    Style::default()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.overlay0)
                }),
                field(directory_y),
            );
            let directory_rect = field(directory_y + 1);
            if directory_selected {
                render_modal_text_input(
                    frame,
                    directory_rect,
                    &client_view.group_default_directory_input,
                    &palette,
                );
            } else {
                super::widgets::render_modal_text_value(
                    frame,
                    directory_rect,
                    &client_view.group_default_directory_input,
                    &palette,
                );
            }
        }
        if picker_open {
            let start = Rect::new(field(11).x, inner.y + 11, field(11).width.min(24), 3);
            for (index, icon) in crate::app::state::GROUP_ICONS.iter().enumerate() {
                let col = (index % 5) as u16;
                let row = (index / 5) as u16;
                if row >= start.height {
                    continue;
                }
                let rect = Rect::new(start.x + col * 4, start.y + row, 3, 1);
                if rect.x >= start.x + start.width {
                    continue;
                }
                let selected = client_view.group_icon_input == *icon;
                frame.render_widget(
                    Paragraph::new(format!(" {icon} "))
                        .style(if selected {
                            Style::default()
                                .fg(panel_contrast_fg(&palette))
                                .bg(palette.accent)
                        } else {
                            Style::default().fg(palette.text).bg(palette.surface0)
                        })
                        .alignment(Alignment::Center),
                    rect,
                );
            }
        }
    } else {
        render_modal_text_input(
            frame,
            Rect::new(rows[2].x, rows[2].y, rows[2].width, 1),
            &client_view.name_input,
            &palette,
        );
        if let Some(location) = client_view.pending_workspace_create_location.as_ref() {
            let host = if location.is_local() {
                "Local".to_string()
            } else {
                app.ssh_connection_profiles
                    .iter()
                    .find(|profile| profile.execution_host_id() == location.execution_host_id)
                    .map(|profile| profile.name().to_string())
                    .unwrap_or_else(|| location.execution_host_id.as_str().to_string())
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "Runs on {host} · Directory {}",
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
        "save",
        primary_action_style(&palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "clear",
        secondary_action_style(&palette),
    );
}

pub(super) fn render_rename_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    super::dim_background(frame, area);

    let title = match app.mode {
        Mode::RenameWorkspace if app.pending_workspace_create_location.is_some() => "new workspace",
        Mode::RenameWorkspace => "rename workspace",
        Mode::RenameGroup if app.creating_new_group => "new group",
        Mode::RenameGroup => "rename group",
        Mode::RenameTab if app.creating_new_tab => "new tab",
        Mode::RenameTab => "rename tab",
        Mode::RenamePane => "rename pane",
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
    }

    if matches!(app.mode, Mode::RenameGroup) {
        let section_description = if app.creating_new_group {
            "name + icon + Runs on + Directory"
        } else {
            "name + icon"
        };
        if app.creating_new_group {
            render_modal_description(
                frame,
                Rect::new(inner.x, inner.y + 3, inner.width, 1),
                "general",
                Style::default().fg(palette.accent),
            );
            render_modal_description(
                frame,
                Rect::new(inner.x, inner.y + 4, inner.width, 1),
                section_description,
                Style::default().fg(palette.overlay0),
            );
        } else {
            frame.render_widget(
                Paragraph::new("general").style(modal_section_heading_style(&palette)),
                Rect::new(inner.x, inner.y + 3, inner.width, 1),
            );
            frame.render_widget(
                Paragraph::new(section_description).style(Style::default().fg(palette.overlay0)),
                Rect::new(inner.x, inner.y + 4, inner.width, 1),
            );
        }

        let name_label_style = if app.group_modal_selected_field == 0 {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay0)
        };
        frame.render_widget(
            Paragraph::new("name").style(name_label_style),
            group_field_rect(app, inner, 6, 6),
        );
        let name_rect = group_name_input_rect(app, inner);
        if app.group_modal_selected_field == 0 {
            render_modal_text_input(frame, name_rect, &app.name_input, &palette);
        } else {
            super::widgets::render_modal_text_value(frame, name_rect, &app.name_input, &palette);
        }

        let icon_label_style = if app.group_icon_picker_open {
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay0)
        };
        frame.render_widget(
            Paragraph::new("icon").style(icon_label_style),
            group_field_rect(app, inner, 9, 4),
        );
        let icon_rect = group_icon_button_rect(app, inner);
        let icon_style = if app.group_icon_picker_open {
            Style::default()
                .fg(panel_contrast_fg(&palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(palette.text)
                .bg(palette.surface0)
                .add_modifier(Modifier::BOLD)
        };
        frame.render_widget(
            Paragraph::new(format!(" {} ", app.group_icon_input))
                .style(icon_style)
                .alignment(ratatui::layout::Alignment::Center),
            icon_rect,
        );

        if app.creating_new_group {
            let host_y = if app.group_icon_picker_open { 14 } else { 11 };
            let host_style = if app.group_modal_selected_field == 1 {
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.overlay0)
            };
            let host_name = if app.group_default_execution_host_id.is_local() {
                "Local".to_string()
            } else {
                app.ssh_connection_profiles
                    .iter()
                    .find(|profile| {
                        profile.execution_host_id() == app.group_default_execution_host_id
                    })
                    .map(|profile| profile.name().to_string())
                    .unwrap_or_else(|| app.group_default_execution_host_id.as_str().to_string())
            };
            frame.render_widget(
                Paragraph::new(format!("Runs on · {host_name}")).style(host_style),
                group_field_rect(app, inner, host_y, 7),
            );
            let directory_y = host_y + 1;
            let directory_label_style = if app.group_modal_selected_field == 2 {
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.overlay0)
            };
            frame.render_widget(
                Paragraph::new("Directory").style(directory_label_style),
                group_field_rect(app, inner, directory_y, 7),
            );
            let directory_rect = group_default_directory_input_rect(app, inner);
            if app.group_modal_selected_field == 2 {
                render_modal_text_input(
                    frame,
                    directory_rect,
                    &app.group_default_directory_input,
                    &palette,
                );
            } else {
                super::widgets::render_modal_text_value(
                    frame,
                    directory_rect,
                    &app.group_default_directory_input,
                    &palette,
                );
            }
        }
    } else {
        let input_rect = Rect::new(rows[2].x, rows[2].y, rows[2].width, 1);
        render_modal_text_input(frame, input_rect, &app.name_input, &palette);
        if let Some(location) = app.pending_workspace_create_location.as_ref() {
            let host = if location.is_local() {
                "Local".to_string()
            } else {
                app.ssh_connection_profiles
                    .iter()
                    .find(|profile| profile.execution_host_id() == location.execution_host_id)
                    .map(|profile| profile.name().to_string())
                    .unwrap_or_else(|| location.execution_host_id.as_str().to_string())
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "Runs on {host} · Directory {}",
                    location.path.as_path().display()
                ))
                .style(Style::default().fg(palette.overlay0)),
                Rect::new(rows[2].x, rows[2].y + 2, rows[2].width, 1),
            );
        }
    }

    if matches!(app.mode, Mode::RenameGroup) && app.group_icon_picker_open {
        for (rect, icon) in group_icon_picker_rects(app, inner) {
            let selected = app.group_icon_input == icon;
            let style = if selected {
                Style::default()
                    .fg(panel_contrast_fg(&palette))
                    .bg(palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(palette.text).bg(palette.surface0)
            };
            frame.render_widget(
                Paragraph::new(format!(" {icon} "))
                    .style(style)
                    .alignment(Alignment::Center),
                rect,
            );
        }
    }

    let (save_rect, clear_rect, _) = rename_button_rects(inner);

    render_action_button(
        frame,
        save_rect,
        Some("↵"),
        "save",
        primary_action_style(&palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "clear",
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
        "1 pane".to_string()
    } else {
        format!("{pane_count} panes")
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

    let title_line = Line::from(vec![Span::styled(" close workspace?", warn)]);

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
            "confirm",
            danger_action_style(&palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
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
        "1 space".to_string()
    } else {
        format!("{space_count} spaces")
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
    let title_line = Line::from(vec![Span::styled(" delete group?", warn)]);
    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {group_name}"),
            Style::default()
                .fg(app.group_accent_color(group_idx))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — closes {spaces}"), dim),
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
            "confirm",
            danger_action_style(&palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
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
                label: "confirm",
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
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
    render_modal_header_bar(frame, inner, "SSH authentication required", &palette, true);
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
        "Y confirm host key  ·  N / Esc cancel"
    } else {
        "Enter submit  ·  Esc cancel"
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

        assert!(text.contains("SSH authentication required"));
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

        assert!(text.contains("Y confirm host key"));
        assert!(text.contains("N / Esc cancel"));
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
        assert!(text.contains("close workspace?"));
        assert!(text.contains("empty"));
        assert!(text.contains("0 panes"));
        assert!(text.contains("↵"));
        assert!(text.contains("confirm"));
        assert!(text.contains("esc"));
        assert!(text.contains("cancel"));
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
        assert!(text.contains("new group"));
        assert!(text.contains("name + icon + Runs on + Directory"));
        assert!(text.contains("Directory"));
        assert!(text.contains("/tmp/work"));
        assert!(text.contains("save"));
        assert!(text.contains("clear"));

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
        assert_eq!(buffer[(inner.x + 1, inner.y + 3)].symbol(), "g");
        assert_eq!(
            buffer[(inner.x + 1, inner.y + 3)].style().fg,
            Some(app.palette.accent)
        );
        assert_eq!(
            buffer[(inner.x + 1, inner.y + 4)].style().fg,
            Some(app.palette.overlay0)
        );
        assert_eq!(buffer[(inner.x + 1, inner.y + 6)].symbol(), "n");
        assert_eq!(group_name_input_rect(&app, inner).x, inner.x + 1);
        assert_eq!(
            group_default_directory_input_rect(&app, inner),
            Rect::new(inner.x + 1, inner.y + 13, inner.width.saturating_sub(1), 1)
        );

        app.group_icon_picker_open = true;
        assert_eq!(
            group_default_directory_input_rect(&app, inner),
            Rect::new(inner.x + 1, inner.y + 16, inner.width.saturating_sub(1), 1)
        );
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
