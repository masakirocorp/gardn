use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::{
    text::display_width,
    widgets::{panel_contrast_fg, render_panel_shell},
};
use crate::app::{
    state::ContextMenuState, AppState, ClientViewState, FilterMenuRow, GroupMenuAction,
};

fn menu_separator_bounds(width: u16) -> (u16, u16) {
    if width <= 2 {
        (0, width)
    } else {
        (1, width - 1)
    }
}

fn render_menu_separator(frame: &mut Frame, area: Rect, row_idx: usize, style: Style) {
    let y = area.y + row_idx as u16;
    if y >= area.y + area.height {
        return;
    }

    let (start, end) = menu_separator_bounds(area.width);
    let buf = frame.buffer_mut();
    for x in area.x + start..area.x + end {
        buf[(x, y)].set_symbol("─").set_style(style);
    }
}

fn context_menu_row_label(item: &str, items: &[&str]) -> String {
    let display = ContextMenuState::item_display_label(item);
    if ContextMenuState::item_is_section_header(item) || display.starts_with(" +") {
        return format!(" {display}");
    }
    if items
        .iter()
        .any(|item| ContextMenuState::item_is_section_header(item))
    {
        format!("  {display}")
    } else {
        format!(" {display}")
    }
}

pub(crate) fn global_menu_labels(app: &AppState) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if app.config_issue.is_some() {
        labels.push("Configuration Issue");
    }
    if app.update_available.is_some() {
        labels.push("Update Ready");
    }
    labels.push("Changelog");
    if app.integration_updates_available() {
        labels.push("Integrations");
    }
    labels.extend(["Settings", "Keybinds", "Reload Config", "Detach"]);
    labels
}

fn render_menu_row(
    frame: &mut Frame,
    area: Rect,
    row_idx: usize,
    line: Line<'static>,
    selected: bool,
    selected_style: Style,
    fallback_style: Style,
) {
    let y = area.y + row_idx as u16;
    if y >= area.y + area.height {
        return;
    }

    let rect = Rect::new(area.x, y, area.width, 1);
    if selected {
        let buf = frame.buffer_mut();
        for x in rect.x..rect.x + rect.width {
            buf[(x, y)].set_style(selected_style);
        }
    }
    let style = if selected {
        selected_style
    } else {
        fallback_style
    };
    frame.render_widget(Paragraph::new(line).style(style), rect);
}

fn right_aligned_count_gap(width: u16, left_width: usize, count_width: usize) -> String {
    let target_width = (width as usize).saturating_sub(1);
    let gap = target_width
        .saturating_sub(left_width.saturating_add(count_width))
        .max(1);
    " ".repeat(gap)
}

fn counted_menu_line(
    app: &AppState,
    label: &str,
    count: Option<usize>,
    selected: bool,
    accent: Option<Color>,
    width: u16,
) -> Line<'static> {
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let label_style = if selected {
        selected_style
    } else if let Some(accent) = accent {
        Style::default().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(app.palette.text)
    };
    let Some(count) = count else {
        return Line::from(Span::styled(label.to_string(), label_style));
    };
    if !app.show_counters {
        return Line::from(Span::styled(label.to_string(), label_style));
    }
    let count = count.to_string();
    let count_style = if selected {
        selected_style
    } else {
        Style::default().fg(app.palette.overlay0)
    };
    Line::from(vec![
        Span::styled(label.to_string(), label_style),
        Span::styled(
            right_aligned_count_gap(width, display_width(label), display_width(&count)),
            label_style,
        ),
        Span::styled(count, count_style),
    ])
}

fn keybind_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings.label().unwrap_or_else(|| "Unset".to_string())
}

fn render_bottom_bar(frame: &mut Frame, area: Rect, line: Line<'_>, bg: ratatui::style::Color) {
    frame.render_widget(Clear, area);
    let buf = frame.buffer_mut();
    for x in area.x..area.x + area.width {
        buf[(x, area.y)].set_style(Style::default().bg(bg));
    }
    frame.render_widget(Paragraph::new(line), area);
}

fn active_workspace_accent_for_view(
    app: &AppState,
    view: &ClientViewState,
) -> ratatui::style::Color {
    if !view.group_filter_enabled {
        if let Some(group_idx) = view
            .active_workspace
            .and_then(|idx| app.workspaces.get(idx))
            .and_then(|workspace| app.group_index_by_id(&workspace.group_id))
        {
            return app.group_accent_color(group_idx);
        }
    }
    app.group_accent_color(view.active_group)
}

pub(super) fn render_prefix_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let accent = active_workspace_accent_for_view(app, view);
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(accent)
        .add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::styled(" Prefix ", mode_style),
        Span::raw(" "),
        Span::styled("Esc", key),
        Span::styled(" Cancel  ", dim),
        Span::styled(
            crate::config::format_key_combo((app.prefix_code, app.prefix_mods)),
            key,
        ),
        Span::styled(" Send", dim),
    ]);
    let y = area.y + area.height.saturating_sub(1);
    render_bottom_bar(
        frame,
        Rect::new(area.x, y, area.width, 1),
        line,
        app.palette.panel_bg,
    );
}

pub(super) fn render_copy_mode_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let accent = active_workspace_accent_for_view(app, view);
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(accent)
        .add_modifier(Modifier::BOLD);
    let Some(copy_mode) = view.copy_mode.as_ref() else {
        return;
    };
    let line = if let Some(prompt) = copy_mode.search.prompt.as_ref() {
        let marker = match prompt.direction {
            crate::app::state::CopyModeSearchDirection::Forward => "/",
            crate::app::state::CopyModeSearchDirection::Backward => "?",
        };
        Line::from(vec![
            Span::styled(" Copy ", mode_style),
            Span::raw(" "),
            Span::styled(marker, key),
            Span::styled(prompt.query.clone(), Style::default().fg(app.palette.text)),
            Span::styled("█", key),
            Span::styled("  Enter Search  Esc Cancel", dim),
        ])
    } else {
        let select = if copy_mode.selection.is_some() {
            "Selecting"
        } else {
            "Select"
        };
        let match_status = copy_mode
            .search
            .current
            .map(|current| format!(" {}/{}", current + 1, copy_mode.search.matches.len()))
            .or_else(|| (!copy_mode.search.query.is_empty()).then(|| " 0/0".to_string()))
            .unwrap_or_default();
        Line::from(vec![
            Span::styled(" Copy ", mode_style),
            Span::raw(" "),
            Span::styled("h/j/k/l w/b/e { }", key),
            Span::styled(" Move  ", dim),
            Span::styled("/ ?", key),
            Span::styled(" Search  ", dim),
            Span::styled("n/N", key),
            Span::styled(format!(" Repeat{match_status}  "), dim),
            Span::styled("v/Space", key),
            Span::styled(format!(" {select}  "), dim),
            Span::styled("y/Enter", key),
            Span::styled(" Copy  q/Esc Exit", dim),
        ])
    };
    let y = area.y + area.height.saturating_sub(1);
    render_bottom_bar(
        frame,
        Rect::new(area.x, y, area.width, 1),
        line,
        app.palette.panel_bg,
    );
}

pub(super) fn render_navigate_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let accent = active_workspace_accent_for_view(app, view);
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(accent)
        .add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::styled(" Navigate ", mode_style),
        Span::raw(" "),
        Span::styled("Esc", key),
        Span::styled(" Back  ", dim),
        Span::styled(
            format!(
                "{} / {}",
                keybind_label(&app.keybinds.navigate.workspace_up),
                keybind_label(&app.keybinds.navigate.workspace_down)
            ),
            key,
        ),
        Span::styled(" Space  ↵ Open  ⇥ Pane", dim),
    ]);
    let y = area.y + area.height.saturating_sub(1);
    render_bottom_bar(
        frame,
        Rect::new(area.x, y, area.width, 1),
        line,
        app.palette.panel_bg,
    );
    if app.update_available.is_some() {
        let status_area = Rect::new(
            area.x + area.width.saturating_sub(13),
            y,
            13.min(area.width),
            1,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                " Update Ready",
                Style::default()
                    .fg(app.palette.accent)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            status_area,
        );
    }
}

pub(super) fn render_resize_overlay_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let key = Style::default()
        .fg(active_workspace_accent_for_view(app, view))
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let line = Line::from(vec![
        Span::styled(
            " Resize ",
            Style::default()
                .fg(panel_contrast_fg(&app.palette))
                .bg(app.palette.mauve)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled("h/l", key),
        Span::styled(" Width  ", dim),
        Span::styled("j/k", key),
        Span::styled(" Height  Esc/↵ Done", dim),
    ]);
    let y = area.y + area.height.saturating_sub(1);
    render_bottom_bar(
        frame,
        Rect::new(area.x, y, area.width, 1),
        line,
        app.palette.panel_bg,
    );
}

fn client_context_menu_rect(view: &ClientViewState, menu: &ContextMenuState) -> Rect {
    let screen = view.screen_rect();
    let max_width = menu
        .items()
        .iter()
        .map(|item| ContextMenuState::item_display_label(item).len() as u16)
        .max()
        .unwrap_or(0);
    let width = (max_width + 4).max(14).min(screen.width.max(1));
    let height = (menu.items().len() as u16 + 2).min(screen.height.max(1));
    Rect::new(
        menu.x.min(screen.x + screen.width.saturating_sub(width)),
        menu.y.min(screen.y + screen.height.saturating_sub(height)),
        width,
        height,
    )
}

fn context_menu_palette(app: &AppState, menu: &ContextMenuState) -> crate::app::state::Palette {
    match menu.kind {
        crate::app::state::ContextMenuKind::Sidebar { group_idx }
        | crate::app::state::ContextMenuKind::Group { group_idx, .. } => {
            app.palette_for_group(group_idx)
        }
        crate::app::state::ContextMenuKind::Workspace { ws_idx, .. }
        | crate::app::state::ContextMenuKind::Tab { ws_idx, .. }
        | crate::app::state::ContextMenuKind::Agent { ws_idx, .. }
        | crate::app::state::ContextMenuKind::NewTabButton { ws_idx, .. }
        | crate::app::state::ContextMenuKind::Pane { ws_idx, .. } => {
            app.palette_for_workspace(ws_idx)
        }
    }
}

pub(super) fn render_context_menu_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let Some(menu) = &view.context_menu else {
        return;
    };
    let palette = context_menu_palette(app, menu);
    let Some(inner) = render_panel_shell(
        frame,
        client_context_menu_rect(view, menu),
        palette.accent,
        palette.panel_bg,
    ) else {
        return;
    };
    let selected = Style::default()
        .bg(palette.accent)
        .fg(panel_contrast_fg(&palette))
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(palette.text);
    let dim = Style::default().fg(palette.overlay0);
    let visible = menu.list.visible();
    let visible_range = menu.visible_item_range(inner.height as usize);
    let items = menu.items();
    for (row, item) in items[visible_range.clone()].iter().enumerate() {
        let idx = visible_range.start + row;
        let display_item = ContextMenuState::item_display_label(item);
        if ContextMenuState::item_is_separator(item) {
            render_menu_separator(frame, inner, row, dim);
        } else {
            let header = ContextMenuState::item_is_section_header(item);
            render_menu_row(
                frame,
                inner,
                row,
                Line::from(if header {
                    format!(" {display_item}")
                } else {
                    context_menu_row_label(item, items)
                }),
                !header && visible == Some(idx),
                selected,
                if header { dim } else { text },
            );
        }
    }
}

fn render_client_filter_menu<A>(
    app: &AppState,
    frame: &mut Frame,
    rect: Rect,
    rows: &[FilterMenuRow<A>],
    selection_anchor: usize,
    visible: Option<usize>,
    accent_for: impl Fn(&A) -> Option<Color>,
) {
    let Some(inner) = render_panel_shell(frame, rect, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(app.palette.text);
    let dim_style = Style::default().fg(app.palette.overlay0);
    let offset = crate::app::connection_scope::menu_scroll_offset(
        selection_anchor,
        rows.len(),
        inner.height as usize,
    );
    for (row_idx, (idx, row)) in rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(inner.height as usize)
        .enumerate()
    {
        let selected = visible == Some(idx) && row.action().is_some();
        match row {
            FilterMenuRow::Separator => {
                render_menu_separator(frame, inner, row_idx, dim_style);
            }
            FilterMenuRow::Heading(label) => {
                render_menu_row(
                    frame,
                    inner,
                    row_idx,
                    Line::from(format!(" {label}")),
                    false,
                    selected_style,
                    dim_style,
                );
            }
            FilterMenuRow::Item {
                label,
                count,
                action,
            } => {
                render_menu_row(
                    frame,
                    inner,
                    row_idx,
                    counted_menu_line(
                        app,
                        label,
                        *count,
                        selected,
                        accent_for(action),
                        inner.width,
                    ),
                    selected,
                    selected_style,
                    text_style,
                );
            }
        }
    }
}

pub(super) fn render_global_launcher_menu_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let rect = crate::app::client_global_menu_rect(app, view);
    let Some(inner) = render_panel_shell(frame, rect, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(app.palette.text);
    let visible = view.global_menu.visible();
    for (idx, label) in global_menu_labels(app).iter().enumerate() {
        let selected = visible == Some(idx);
        let item_style = if selected { selected_style } else { text_style };
        let badge_style = if selected {
            selected_style
        } else {
            Style::default()
                .fg(app.palette.accent)
                .add_modifier(Modifier::BOLD)
        };
        let line = if app.global_menu_item_has_badge(label) {
            let text = format!(" {label}");
            let gap = inner.width.saturating_sub(text.chars().count() as u16 + 1) as usize;
            Line::from(vec![
                Span::styled(text, item_style),
                Span::styled(" ".repeat(gap), item_style),
                Span::styled("●", badge_style),
            ])
        } else {
            Line::from(Span::styled(format!(" {label}"), item_style))
        };
        render_menu_row(
            frame,
            inner,
            idx,
            line,
            selected,
            selected_style,
            item_style,
        );
    }
}

pub(super) fn render_group_menu_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let rows = crate::app::client_group_menu_rows(app, view);
    render_client_filter_menu(
        app,
        frame,
        crate::app::client_group_menu_rect(app, view),
        &rows,
        view.group_menu.selected,
        view.group_menu.visible(),
        |action| match action {
            GroupMenuAction::Group(group_idx) => Some(app.group_accent_color(*group_idx)),
            _ => None,
        },
    );
}

pub(super) fn render_agent_menu_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let rows = crate::app::client_agent_menu_rows(app, view);
    render_client_filter_menu(
        app,
        frame,
        crate::app::client_agent_menu_rect(app, view),
        &rows,
        view.agent_menu.selected,
        view.agent_menu.visible(),
        |_| None,
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn menu_separator_bounds_inset_rule_when_roomy() {
        assert_eq!(menu_separator_bounds(5), (1, 4));
        assert_eq!(menu_separator_bounds(2), (0, 2));
    }

    #[test]
    fn client_group_menu_group_line_uses_group_accent() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Magenta));
        let expected_accent = app.group_accent_color(group_idx);
        let mut view = ClientViewState::from_default_client_state(&app);
        view.computed.sidebar_rect = Rect::new(0, 0, 24, 20);
        view.computed.terminal_area = Rect::new(24, 0, 56, 20);
        view.group_menu = crate::app::state::ModalListState::new(0);

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("test backend");
        terminal
            .draw(|frame| render_group_menu_for_view(&app, &view, frame))
            .expect("render client group menu");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_text(buffer, 80, 20, "work").expect("work group row");
        assert_eq!(buffer[(x, y)].style().fg, Some(expected_accent));
    }

    #[test]
    fn client_agent_menu_preserves_connections_heading_with_short_host_name() {
        let mut app = AppState::test_new();
        app.host_display =
            crate::app::host_label::HostDisplayNameOverlay::from_config_or_hostname("mac", None);
        app.ssh_connection_profiles.clear();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.computed.sidebar_rect = Rect::new(0, 0, 24, 20);
        view.computed.terminal_area = Rect::new(24, 0, 56, 20);
        view.agent_menu = crate::app::state::ModalListState::new(0);

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("test backend");
        terminal
            .draw(|frame| render_agent_menu_for_view(&app, &view, frame))
            .expect("render client agent menu");

        assert!(
            first_cell_with_text(terminal.backend().buffer(), 80, 20, "Connections").is_some(),
            "connections heading should remain fully visible"
        );
    }

    #[test]
    fn client_context_menu_renders_new_actions_in_order() {
        let app = AppState::test_new();
        let mut view = ClientViewState::from_default_client_state(&app);
        view.computed.terminal_area = Rect::new(0, 0, 40, 24);
        view.context_menu = Some(ContextMenuState {
            kind: crate::app::state::ContextMenuKind::NewTabButton {
                ws_idx: 0,
                project_commands: crate::app::state::ProjectCommandAvailability::ALL,
            },
            x: 2,
            y: 2,
            list: crate::app::state::ModalListState::new(1),
        });

        let mut terminal = Terminal::new(TestBackend::new(40, 24)).expect("test backend");
        terminal
            .draw(|frame| render_context_menu_for_view(&app, &view, frame))
            .expect("render context menu");

        let buffer = terminal.backend().buffer();
        let rows = ["Terminal", "Agent", "Editor", "Review", "GitHub", "Browser"].map(|label| {
            first_cell_with_text(buffer, 40, 24, label)
                .expect("new action should render")
                .1
        });
        assert!(
            rows.windows(2).all(|pair| pair[0] + 1 == pair[1]),
            "new actions should render consecutively: {rows:?}"
        );
    }

    #[test]
    fn client_prefix_overlay_uses_active_group_accent() {
        let mut app = AppState::test_new();
        app.palette.accent = Color::Rgb(1, 2, 3);
        let group_idx = app.create_group("work".to_string());
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Cyan));
        let expected_accent = app.group_accent_color(group_idx);
        let mut view = ClientViewState::from_default_client_state(&app);
        view.active_group = group_idx;
        view.group_filter_enabled = true;
        view.active_workspace = None;

        let mut terminal = Terminal::new(TestBackend::new(96, 8)).expect("test backend");
        terminal
            .draw(|frame| {
                render_prefix_overlay_for_view(&app, &view, frame, Rect::new(0, 0, 96, 8))
            })
            .expect("render prefix overlay");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_text(buffer, 96, 8, "Esc").expect("esc hint");
        assert_eq!(buffer[(x, y)].style().fg, Some(expected_accent));
        assert_eq!(buffer[(x, y)].style().bg, Some(app.palette.panel_bg));
    }

    fn first_cell_with_text(
        buffer: &Buffer,
        width: u16,
        height: u16,
        text: &str,
    ) -> Option<(u16, u16)> {
        let target: Vec<char> = text.chars().collect();
        for y in 0..height {
            for x in 0..width.saturating_sub(target.len().saturating_sub(1) as u16) {
                let matches = target.iter().enumerate().all(|(idx, ch)| {
                    let mut encoded = [0; 4];
                    buffer[(x + idx as u16, y)].symbol() == ch.encode_utf8(&mut encoded)
                });
                if matches {
                    return Some((x, y));
                }
            }
        }
        None
    }
}
