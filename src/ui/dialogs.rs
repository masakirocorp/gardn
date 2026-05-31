use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::widgets::{
    action_button_row_rects, centered_popup_rect, danger_action_style, panel_contrast_fg,
    primary_action_style, render_action_button, render_modal_header_bar, render_modal_shell,
    render_modal_subtitle, render_modal_text_input, render_panel_shell, secondary_action_style,
    ActionButtonSpec,
};
use crate::app::{state::WorktreeOpenState, AppState, Mode};

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

pub(crate) fn rename_modal_size(app: &AppState) -> (u16, u16) {
    if matches!(app.mode, Mode::RenameGroup) {
        (56, 12)
    } else {
        (56, 7)
    }
}

pub(crate) fn group_icon_button_rect(inner: Rect) -> Rect {
    if inner.width < 5 || inner.height < 4 {
        return Rect::default();
    }
    Rect::new(inner.x, inner.y + 2, 3, 1)
}

pub(crate) fn group_name_input_rect(inner: Rect) -> Rect {
    let icon = group_icon_button_rect(inner);
    if icon == Rect::default() {
        return Rect::new(inner.x, inner.y + 2, inner.width, 1);
    }
    Rect::new(
        icon.x + icon.width + 1,
        icon.y,
        inner.width.saturating_sub(icon.width + 1),
        1,
    )
}

pub(crate) fn group_icon_picker_rects(inner: Rect) -> Vec<(Rect, &'static str)> {
    let start = Rect::new(inner.x, inner.y + 4, inner.width.min(24), 4);
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

pub(super) fn render_rename_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    super::dim_background(frame, area);

    let title = match app.mode {
        Mode::RenameWorkspace => "rename workspace",
        Mode::RenameGroup if app.creating_new_group => "new group",
        Mode::RenameGroup => "rename group",
        Mode::RenameTab if app.creating_new_tab => "new tab",
        Mode::RenameTab => "rename tab",
        Mode::RenamePane => "rename pane",
        Mode::EditWorktreeDirectory => "worktree directory",
        _ => return,
    };

    let (popup_w, popup_h) = rename_modal_size(app);
    let Some(inner) = render_modal_shell(frame, area, popup_w, popup_h, &app.palette) else {
        return;
    };
    if inner.height < 4 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<5>(inner);

    render_modal_header_bar(frame, rows[0], title, &app.palette, true);
    if matches!(app.mode, Mode::RenameGroup) {
        render_modal_subtitle(frame, rows[1], " name + icon", &app.palette);
    }

    let input_rect = if matches!(app.mode, Mode::RenameGroup) {
        let icon_rect = group_icon_button_rect(inner);
        let icon_style = if app.group_icon_picker_open {
            Style::default()
                .fg(panel_contrast_fg(&app.palette))
                .bg(app.palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.surface0)
                .add_modifier(Modifier::BOLD)
        };
        frame.render_widget(
            Paragraph::new(format!(" {} ", app.group_icon_input))
                .style(icon_style)
                .alignment(ratatui::layout::Alignment::Center),
            icon_rect,
        );
        group_name_input_rect(inner)
    } else {
        Rect::new(rows[2].x, rows[2].y, rows[2].width, 1)
    };
    render_modal_text_input(frame, input_rect, &app.name_input, &app.palette);

    if matches!(app.mode, Mode::RenameGroup) && app.group_icon_picker_open {
        for (rect, icon) in group_icon_picker_rects(inner) {
            let selected = app.group_icon_input == icon;
            let style = if selected {
                Style::default()
                    .fg(panel_contrast_fg(&app.palette))
                    .bg(app.palette.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(app.palette.text)
                    .bg(app.palette.surface0)
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
        primary_action_style(&app.palette),
    );
    render_action_button(
        frame,
        clear_rect,
        Some("^c"),
        "clear",
        secondary_action_style(&app.palette),
    );
}

pub(crate) fn new_linked_worktree_inner_rect(area: Rect) -> Option<Rect> {
    centered_popup_rect(area, 68, 10).map(|popup| {
        Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        )
    })
}

pub(crate) fn new_linked_worktree_button_rects(inner: Rect) -> (Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: "create and open",
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
            },
        ],
        2,
        inner.height.saturating_sub(1),
    );
    (rects[0], rects[1])
}

pub(crate) fn remove_worktree_popup_rect(area: Rect) -> Option<Rect> {
    centered_popup_rect(area, 72, 10)
}

pub(crate) fn remove_worktree_button_rects(inner: Rect, force_confirmation: bool) -> (Rect, Rect) {
    let primary_label = if force_confirmation {
        "delete anyway"
    } else {
        "remove"
    };
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: primary_label,
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
            },
        ],
        2,
        inner.height.saturating_sub(1),
    );
    (rects[0], rects[1])
}

pub(crate) fn open_existing_worktree_inner_rect(area: Rect, entry_count: usize) -> Option<Rect> {
    let height = (entry_count as u16)
        .saturating_mul(2)
        .saturating_add(7)
        .clamp(12, 26);
    centered_popup_rect(area, 96, height).map(|popup| {
        Rect::new(
            popup.x + 1,
            popup.y + 1,
            popup.width.saturating_sub(2),
            popup.height.saturating_sub(2),
        )
    })
}

pub(crate) fn open_existing_worktree_max_visible_rows(inner: Rect) -> usize {
    usize::from(inner.height.saturating_sub(5) / 2)
}

pub(crate) fn open_existing_worktree_visible_start(
    open: &WorktreeOpenState,
    max_rows: usize,
) -> usize {
    let filtered = open.filtered_indices();
    let selected = open.selected_entry_index().unwrap_or(open.selected);
    let selected_pos = filtered
        .iter()
        .position(|idx| *idx == selected)
        .unwrap_or(0);
    selected_pos.saturating_sub(max_rows.saturating_sub(1))
}

pub(crate) fn open_existing_worktree_button_rects(inner: Rect) -> (Rect, Rect) {
    let rects = action_button_row_rects(
        inner,
        &[
            ActionButtonSpec {
                hint: Some("↵"),
                label: "open",
            },
            ActionButtonSpec {
                hint: Some("esc"),
                label: "cancel",
            },
        ],
        2,
        inner.height.saturating_sub(1),
    );
    (rects[0], rects[1])
}

pub(super) fn render_new_linked_worktree_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let Some(create) = app.worktree_create.as_ref() else {
        return;
    };

    super::dim_background(frame, area);
    let Some(inner) = render_modal_shell(frame, area, 68, 10, &app.palette) else {
        return;
    };
    if inner.height < 7 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<8>(inner);

    render_modal_header(frame, rows[0], "new worktree", &app.palette);

    frame.render_widget(
        Paragraph::new(" branch").style(Style::default().fg(app.palette.overlay0)),
        rows[1],
    );
    let input_rect = Rect::new(rows[2].x, rows[2].y, rows[2].width, 1);
    frame.render_widget(Clear, input_rect);
    frame.render_widget(
        Paragraph::new(format!(" {}█", app.name_input)).style(
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.surface0),
        ),
        input_rect,
    );

    let checkout = create.checkout_path.display().to_string();
    frame.render_widget(
        Paragraph::new(" checkout").style(Style::default().fg(app.palette.overlay0)),
        rows[3],
    );
    frame.render_widget(
        Paragraph::new(format!(" {checkout}")).style(Style::default().fg(app.palette.subtext0)),
        rows[4],
    );

    if create.creating {
        frame.render_widget(
            Paragraph::new(" creating…").style(Style::default().fg(app.palette.overlay0)),
            rows[5],
        );
    } else if let Some(error) = &create.error {
        frame.render_widget(
            Paragraph::new(format!(" {error}")).style(Style::default().fg(app.palette.red)),
            rows[5],
        );
    }

    let (create_rect, cancel_rect) = new_linked_worktree_button_rects(inner);
    render_action_button(
        frame,
        create_rect,
        Some("↵"),
        "create and open",
        Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.palette.accent)
            .add_modifier(Modifier::BOLD),
    );
    render_action_button(
        frame,
        cancel_rect,
        Some("esc"),
        "cancel",
        Style::default()
            .fg(app.palette.text)
            .bg(app.palette.surface0)
            .add_modifier(Modifier::BOLD),
    );
}

pub(super) fn render_remove_worktree_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let Some(remove) = app.worktree_remove.as_ref() else {
        return;
    };

    super::dim_background(frame, area);
    let Some(popup) = remove_worktree_popup_rect(area) else {
        return;
    };
    let Some(inner) = render_panel_shell(frame, popup, app.palette.red, app.palette.panel_bg)
    else {
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas::<8>(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            " delete worktree checkout?",
            Style::default()
                .fg(app.palette.red)
                .add_modifier(Modifier::BOLD),
        )])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(" This removes the checkout folder:")
            .style(Style::default().fg(app.palette.overlay0)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!(" {}", remove.path.display()))
            .style(Style::default().fg(app.palette.text)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(" The branch is not deleted. The Hako workspace will close.")
            .style(Style::default().fg(app.palette.overlay0)),
        rows[3],
    );
    if remove.force_confirmation {
        frame.render_widget(
            Paragraph::new(" Dirty or untracked files will be permanently deleted.")
                .style(Style::default().fg(app.palette.red)),
            rows[4],
        );
    }
    if remove.removing {
        frame.render_widget(
            Paragraph::new(" removing…").style(Style::default().fg(app.palette.overlay0)),
            rows[5],
        );
    } else if let Some(error) = &remove.error {
        frame.render_widget(
            Paragraph::new(format!(" {error}")).style(Style::default().fg(app.palette.red)),
            rows[5],
        );
    }

    let (remove_rect, cancel_rect) = remove_worktree_button_rects(inner, remove.force_confirmation);
    let remove_label = if remove.force_confirmation {
        "delete anyway"
    } else {
        "remove"
    };
    render_action_button(
        frame,
        remove_rect,
        Some("↵"),
        remove_label,
        Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.palette.red)
            .add_modifier(Modifier::BOLD),
    );
    render_action_button(
        frame,
        cancel_rect,
        Some("esc"),
        "cancel",
        Style::default()
            .fg(app.palette.text)
            .bg(app.palette.surface0)
            .add_modifier(Modifier::BOLD),
    );
}

pub(super) fn render_open_existing_worktree_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let Some(open) = app.worktree_open.as_ref() else {
        return;
    };

    super::dim_background(frame, area);
    let height = (open.entries.len() as u16)
        .saturating_mul(2)
        .saturating_add(7)
        .clamp(12, 26);
    let Some(inner) = render_modal_shell(frame, area, 96, height, &app.palette) else {
        return;
    };
    if inner.height < 8 {
        return;
    }

    render_modal_header(
        frame,
        Rect::new(inner.x, inner.y, inner.width, 1),
        "open worktree",
        &app.palette,
    );
    render_open_worktree_search(
        app,
        frame,
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
        open,
    );
    frame.render_widget(
        Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(app.palette.surface1)),
        Rect::new(inner.x, inner.y.saturating_add(2), inner.width, 1),
    );

    let filtered = open.filtered_indices();
    let max_rows = open_existing_worktree_max_visible_rows(inner);
    let start = open_existing_worktree_visible_start(open, max_rows);
    for (visible_idx, entry_idx) in filtered.iter().skip(start).take(max_rows).enumerate() {
        let Some(entry) = open.entries.get(*entry_idx) else {
            continue;
        };
        let selected = Some(*entry_idx) == open.selected_entry_index();
        let y = inner.y.saturating_add(3 + (visible_idx as u16 * 2));
        let marker = if selected { "›" } else { " " };
        let row_style = if selected {
            Style::default()
                .fg(app.palette.text)
                .bg(app.palette.surface0)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.palette.subtext0)
        };
        let path_style = if selected {
            Style::default()
                .fg(app.palette.subtext0)
                .bg(app.palette.surface0)
        } else {
            Style::default().fg(app.palette.overlay0)
        };
        let status = entry.status_label();
        let title_width = inner
            .width
            .saturating_sub(status.len() as u16)
            .saturating_sub(4) as usize;
        let mut title = format!(
            "{marker} {}",
            truncate_text(&entry.display_name(), title_width)
        );
        if !status.is_empty() {
            let pad = inner
                .width
                .saturating_sub(title.chars().count() as u16)
                .saturating_sub(status.len() as u16)
                .max(1);
            title.push_str(&" ".repeat(pad as usize));
            title.push_str(status);
        }
        frame.render_widget(
            Paragraph::new(truncate_text(&title, inner.width as usize)).style(row_style),
            Rect::new(inner.x, y, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(truncate_text(
                &format!("  {}", entry.path.display()),
                inner.width as usize,
            ))
            .style(path_style),
            Rect::new(inner.x, y.saturating_add(1), inner.width, 1),
        );
    }

    if filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(" no matching worktrees")
                .style(Style::default().fg(app.palette.overlay0)),
            Rect::new(inner.x, inner.y.saturating_add(3), inner.width, 1),
        );
    }

    if let Some(error) = &open.error {
        frame.render_widget(
            Paragraph::new(format!(" {error}")).style(Style::default().fg(app.palette.red)),
            Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(2),
                inner.width,
                1,
            ),
        );
    }

    let (open_rect, cancel_rect) = open_existing_worktree_button_rects(inner);
    render_action_button(
        frame,
        open_rect,
        Some("↵"),
        "open",
        Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.palette.accent)
            .add_modifier(Modifier::BOLD),
    );
    render_action_button(
        frame,
        cancel_rect,
        Some("esc"),
        "cancel",
        Style::default()
            .fg(app.palette.text)
            .bg(app.palette.surface0)
            .add_modifier(Modifier::BOLD),
    );
}

fn render_open_worktree_search(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    open: &WorktreeOpenState,
) {
    let focus_style = if open.search_focused {
        Style::default()
            .fg(app.palette.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.palette.overlay0)
    };
    let filtered_count = open.filtered_indices().len();
    let count = if open.query.trim().is_empty() {
        format!("{} checkouts", open.entries.len())
    } else {
        format!("{filtered_count}/{} checkouts", open.entries.len())
    };
    let mut spans = vec![Span::styled(" / ", focus_style)];
    if open.query.trim().is_empty() {
        spans.push(Span::styled(
            "filter worktrees",
            Style::default().fg(app.palette.overlay0),
        ));
    } else {
        spans.push(Span::styled(
            open.query.clone(),
            Style::default().fg(app.palette.text),
        ));
    }
    spans.push(Span::styled(
        format!(
            "{count:>width$}",
            width = area.width.saturating_sub(18) as usize
        ),
        Style::default().fg(app.palette.overlay0),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn confirm_close_overlay_text(app: &AppState) -> (String, String) {
    let ws_name = app
        .workspaces
        .get(app.selected)
        .map(|ws| ws.display_name())
        .unwrap_or_else(|| "?".to_string());
    let pane_count = app
        .workspaces
        .get(app.selected)
        .map(|ws| ws.layout.pane_count())
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

    let warn = Style::default()
        .fg(app.palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" close workspace?", warn)]);

    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {ws_name}"),
            Style::default()
                .fg(app.palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — {pane_text}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, app.palette.red, app.palette.panel_bg)
    else {
        return;
    };

    if inner.height >= 3 {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas::<4>(inner);

        frame.render_widget(Paragraph::new(title_line), rows[0]);
        frame.render_widget(Paragraph::new(detail_line), rows[1]);

        let (confirm_rect, cancel_rect) = confirm_close_button_rects(inner);
        render_action_button(
            frame,
            confirm_rect,
            Some("↵"),
            "confirm",
            danger_action_style(&app.palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
            secondary_action_style(&app.palette),
        );
    }
}

pub(super) fn render_confirm_delete_group_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let group_idx = app.confirm_delete_group.unwrap_or(app.active_group);
    let group_name = app
        .groups
        .get(group_idx)
        .map(|group| group.name.as_str())
        .unwrap_or("?");
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

    let warn = Style::default()
        .fg(app.palette.red)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let title_line = Line::from(vec![Span::styled(" delete group?", warn)]);
    let detail_line = Line::from(vec![
        Span::styled(
            format!(" {group_name}"),
            Style::default()
                .fg(app.palette.text)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" — closes {spaces}"), dim),
    ]);

    let Some(inner) = render_panel_shell(frame, popup, app.palette.red, app.palette.panel_bg)
    else {
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
            danger_action_style(&app.palette),
        );
        render_action_button(
            frame,
            cancel_rect,
            Some("esc"),
            "cancel",
            secondary_action_style(&app.palette),
        );
    }
}

pub(crate) fn confirm_close_popup_rect(area: Rect) -> Option<Rect> {
    centered_popup_rect(area, 64, 6)
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
        3,
    );
    (rects[0], rects[1])
}
