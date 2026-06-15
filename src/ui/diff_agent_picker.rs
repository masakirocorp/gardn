use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};

use super::widgets::{
    modal_hint_line_count, modal_stack_areas, panel_contrast_fg, render_modal_divider,
    render_modal_header_bar, render_modal_hint_lines, render_panel_shell,
};
use crate::app::{state::Mode, AppState};

const DIFF_AGENT_PICKER_HINTS: &[(&str, &str)] = &[
    ("move", "↑↓"),
    ("send", "↵"),
    ("close", "esc"),
];

pub(crate) fn render_diff_agent_picker_overlay(app: &AppState, frame: &mut Frame) {
    let Some(picker) = app.diff_agent_picker.as_ref() else {
        return;
    };
    let Some(popup) = diff_agent_picker_popup_rect(frame.area()) else {
        return;
    };

    super::dim_background(frame, frame.area());
    let Some(inner) = render_panel_shell(frame, popup, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };
    if inner.height < 8 || inner.width < 20 {
        return;
    }

    let stack = diff_agent_picker_stack(inner);
    let header_rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas::<4>(stack.header);

    render_modal_header_bar(frame, header_rows[0], "send diff to agent", &app.palette, true);
    frame.render_widget(
        Paragraph::new(" choose an existing agent or start a new one")
            .style(Style::default().fg(app.palette.overlay1))
            .wrap(Wrap { trim: true }),
        header_rows[2],
    );
    render_modal_divider(frame, header_rows[3], &app.palette);

    render_diff_agent_picker_options(app, picker.selected, frame, stack.content);

    if let Some(footer_area) = stack.footer {
        render_modal_hint_lines(frame, footer_area, &app.palette, DIFF_AGENT_PICKER_HINTS, 2);
    }
}

pub(crate) fn diff_agent_picker_contains_point(app: &AppState, col: u16, row: u16) -> bool {
    diff_agent_picker_popup_rect(app.view.terminal_area)
        .is_some_and(|area| point_in_rect(col, row, area))
}

pub(crate) fn diff_agent_picker_index_at(app: &AppState, col: u16, row: u16) -> Option<usize> {
    let popup = diff_agent_picker_popup_rect(app.view.terminal_area)?;
    let inner = Rect::new(
        popup.x.saturating_add(1),
        popup.y.saturating_add(1),
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );
    let content = diff_agent_picker_stack(inner).content;
    if !point_in_rect(col, row, content) {
        return None;
    }
    let local_row = row.saturating_sub(content.y) as usize;
    let mut visual_row = 0usize;
    let mut selectable_idx = 0usize;
    for option in diff_agent_picker_options(app) {
        if option.header {
            if visual_row > 0 {
                visual_row += 1;
            }
            visual_row += 1;
            continue;
        }
        if visual_row == local_row {
            return Some(selectable_idx);
        }
        selectable_idx += 1;
        visual_row += 1;
    }
    None
}

fn diff_agent_picker_popup_rect(area: Rect) -> Option<Rect> {
    super::centered_popup_rect(area, 68, 22)
}

fn diff_agent_picker_stack(inner: Rect) -> super::widgets::ModalStackAreas {
    let footer_rows = modal_hint_line_count(inner.width, DIFF_AGENT_PICKER_HINTS, 2);
    modal_stack_areas(inner, 4, footer_rows, 0, 1)
}

fn point_in_rect(col: u16, row: u16, area: Rect) -> bool {
    col >= area.x
        && col < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn render_diff_agent_picker_options(
    app: &AppState,
    selected: usize,
    frame: &mut Frame,
    area: ratatui::layout::Rect,
) {
    let options = diff_agent_picker_options(app);
    let mut lines = Vec::new();
    let mut selectable_idx = 0;
    for option in &options {
        if option.header {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                option.label.clone(),
                Style::default()
                    .fg(app.palette.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        let is_selected = selectable_idx == selected;
        selectable_idx += 1;
        let bg = if is_selected {
            app.palette.accent
        } else {
            app.palette.panel_bg
        };
        let label_fg = if is_selected {
            panel_contrast_fg(&app.palette)
        } else {
            app.palette.text
        };
        let meta_fg = if is_selected {
            panel_contrast_fg(&app.palette)
        } else {
            app.palette.overlay0
        };
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(option.label.clone(), Style::default().fg(label_fg).bg(bg)),
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(option.meta.clone(), Style::default().fg(meta_fg).bg(bg)),
            Span::styled(
                " ".repeat(area.width.saturating_sub(4) as usize),
                Style::default().bg(bg),
            ),
        ]));
    }

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(app.palette.panel_bg)),
        area,
    );
}

#[derive(Clone)]
pub(crate) struct DiffAgentPickerOption {
    pub label: String,
    pub meta: String,
    pub target: Option<(usize, crate::layout::PaneId)>,
    pub new_agent: bool,
    pub header: bool,
}

pub(crate) fn diff_agent_picker_options(app: &AppState) -> Vec<DiffAgentPickerOption> {
    if !matches!(app.mode, Mode::DiffAgentPicker) {
        return Vec::new();
    }
    let Some(picker) = app.diff_agent_picker.as_ref() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    out.push(header("new"));
    out.push(DiffAgentPickerOption {
        label: "new agent".to_string(),
        meta: "choose profile".to_string(),
        target: None,
        new_agent: true,
        header: false,
    });
    let Some(source_ws) = app.workspaces.get(picker.ws_idx) else {
        return out;
    };
    let source_group = source_ws.group_id.clone();
    let source_tab = source_ws.find_tab_index_for_pane(picker.source_pane_id);
    let focused = source_ws.focused_pane_id();
    push_agent_section(app, &mut out, "active pane", |ws_idx, tab_idx, pane_id| {
        ws_idx == picker.ws_idx && Some(pane_id) == focused && Some(tab_idx) == source_tab
    });
    push_agent_section(app, &mut out, "current tab", |ws_idx, tab_idx, pane_id| {
        ws_idx == picker.ws_idx && Some(tab_idx) == source_tab && Some(pane_id) != focused
    });
    push_agent_section(app, &mut out, "current space", |ws_idx, tab_idx, _| {
        ws_idx == picker.ws_idx && Some(tab_idx) != source_tab
    });
    push_agent_section(app, &mut out, "current group", |ws_idx, _, _| {
        ws_idx != picker.ws_idx
            && app
                .workspaces
                .get(ws_idx)
                .is_some_and(|workspace| workspace.group_id == source_group)
    });
    push_agent_section(app, &mut out, "other groups", |ws_idx, _, _| {
        app.workspaces
            .get(ws_idx)
            .is_some_and(|workspace| workspace.group_id != source_group)
    });
    out
}

fn header(label: &str) -> DiffAgentPickerOption {
    DiffAgentPickerOption {
        label: label.to_string(),
        meta: String::new(),
        target: None,
        new_agent: false,
        header: true,
    }
}

fn push_agent_section<F>(
    app: &AppState,
    out: &mut Vec<DiffAgentPickerOption>,
    label: &str,
    mut include: F,
) where
    F: FnMut(usize, usize, crate::layout::PaneId) -> bool,
{
    let start = out.len();
    out.push(header(label));
    for (ws_idx, workspace) in app.workspaces.iter().enumerate() {
        for (tab_idx, tab) in workspace.tabs.iter().enumerate() {
            for pane_id in tab.panes.keys().copied() {
                if !include(ws_idx, tab_idx, pane_id) {
                    continue;
                }
                let Some(pane) = workspace.pane_state(pane_id) else {
                    continue;
                };
                let Some(terminal) = app.terminals.get(&pane.attached_terminal_id) else {
                    continue;
                };
                if !terminal.is_agent_terminal() {
                    continue;
                }
                let name = terminal
                    .effective_agent_label()
                    .or(terminal.agent_name.as_deref())
                    .unwrap_or("agent")
                    .to_string();
                let status = crate::detect::manifest::agent_state_label(terminal.state);
                let space = workspace
                    .custom_name
                    .as_deref()
                    .unwrap_or(workspace.id.as_str());
                out.push(DiffAgentPickerOption {
                    label: name,
                    meta: format!("{status} · {space}"),
                    target: Some((ws_idx, pane_id)),
                    new_agent: false,
                    header: false,
                });
            }
        }
    }
    if out.len() == start + 1 {
        out.pop();
    }
}
