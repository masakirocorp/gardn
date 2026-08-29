use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::{
    text::display_width,
    widgets::{panel_contrast_fg, render_panel_shell},
};
use crate::app::{state::ContextMenuState, AppState, ClientViewState};

fn count_suffix(text: &str) -> Option<(&str, &str)> {
    let start = text.rfind(" (")?;
    text.ends_with(')')
        .then_some((&text[..start], &text[start..]))
}

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

fn group_menu_group_index(app: &AppState, row_idx: usize) -> Option<usize> {
    let group_start = 2;
    if (group_start..group_start + app.groups.len()).contains(&row_idx) {
        Some(row_idx - group_start)
    } else {
        None
    }
}

fn right_aligned_count_gap(width: u16, left_width: usize, count_width: usize) -> String {
    let target_width = (width as usize).saturating_sub(1);
    let gap = target_width
        .saturating_sub(left_width.saturating_add(count_width))
        .max(1);
    " ".repeat(gap)
}

fn group_menu_all_line(app: &AppState, selected: bool, width: u16) -> Line<'static> {
    let marker = if app.group_filter_enabled { " " } else { "✓" };
    let left = format!("{marker} All");
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let text_style = if selected {
        selected_style
    } else {
        Style::default().fg(app.palette.text)
    };
    let mut spans = vec![Span::styled(left.clone(), text_style)];
    if app.show_counters {
        let count = app.workspaces.len().to_string();
        let count_style = if selected {
            selected_style
        } else {
            Style::default().fg(app.palette.overlay0)
        };
        spans.push(Span::styled(
            right_aligned_count_gap(width, display_width(&left), display_width(&count)),
            text_style,
        ));
        spans.push(Span::styled(count, count_style));
    }
    Line::from(spans)
}

fn group_menu_group_line(
    app: &AppState,
    group_idx: usize,
    selected: bool,
    width: u16,
) -> Line<'static> {
    group_menu_group_line_for_selection(
        app,
        app.group_filter_enabled,
        app.active_group,
        group_idx,
        selected,
        width,
    )
}

fn group_menu_group_line_for_selection(
    app: &AppState,
    group_filter_enabled: bool,
    active_group: usize,
    group_idx: usize,
    selected: bool,
    width: u16,
) -> Line<'static> {
    let Some(group) = app.groups.get(group_idx) else {
        return Line::raw("");
    };
    let marker = if group_filter_enabled && group_idx == active_group {
        "✓"
    } else {
        " "
    };
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let group_style = if selected {
        selected_style
    } else {
        Style::default()
            .fg(app.group_accent_color(group_idx))
            .add_modifier(Modifier::BOLD)
    };
    let icon_width = display_width(&group.icon);
    let icon_padding = " ".repeat(2usize.saturating_sub(icon_width));
    let icon_padding_width = icon_padding.len();
    let mut spans = vec![
        Span::styled(format!("{marker} "), group_style),
        Span::styled(group.icon.clone(), group_style),
        Span::styled(icon_padding, group_style),
        Span::styled(group.name.clone(), group_style),
    ];
    if app.show_counters {
        let count = app
            .workspaces
            .iter()
            .filter(|workspace| workspace.group_id == group.id)
            .count()
            .to_string();
        let count_style = if selected {
            selected_style
        } else {
            Style::default().fg(app.palette.overlay0)
        };
        let left_width = display_width(marker)
            + 1
            + icon_width
            + icon_padding_width
            + display_width(&group.name);
        spans.push(Span::styled(
            right_aligned_count_gap(width, left_width, display_width(&count)),
            group_style,
        ));
        spans.push(Span::styled(count, count_style));
    }
    Line::from(spans)
}

fn prefix_rhs_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings
        .prefix_rhs_label()
        .unwrap_or_else(|| "Unset".to_string())
}

fn indexed_prefix_rhs_label(bindings: &[crate::config::IndexedKeybind]) -> String {
    let labels: Vec<&str> = bindings
        .iter()
        .filter(|binding| binding.trigger.is_prefix())
        .map(|binding| {
            binding
                .label
                .strip_prefix("Prefix+")
                .unwrap_or(&binding.label)
        })
        .collect();

    if labels.is_empty() {
        return "Unset".to_string();
    }

    if let (Some(first), Some(last)) = (labels.first(), labels.last()) {
        let last_key = if last.ends_with('0') {
            "0"
        } else if last.ends_with('9') {
            "9"
        } else {
            ""
        };
        if !last_key.is_empty() {
            if let (Some(first_prefix), Some(last_prefix)) =
                (first.strip_suffix('1'), last.strip_suffix(last_key))
            {
                if first_prefix == last_prefix {
                    return format!("{first_prefix}1..{last_key}");
                }
            }
        }
    }

    labels.join(" / ")
}

fn append_prefix_hint(
    spans: &mut Vec<Span<'static>>,
    used_width: &mut usize,
    max_width: usize,
    key_style: Style,
    dim_style: Style,
    key_label: String,
    label: &'static str,
    optional: bool,
) {
    if key_label == "Unset" {
        return;
    }
    let label = format!(" {label} ");
    let width = key_label.chars().count() + label.chars().count();
    if optional && used_width.saturating_add(width) > max_width {
        return;
    }

    *used_width = used_width.saturating_add(width);
    spans.push(Span::styled(key_label, key_style));
    spans.push(Span::styled(label, dim_style));
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

pub(super) fn render_prefix_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let accent = app.active_workspace_accent_color();
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(accent)
        .add_modifier(Modifier::BOLD);

    let mut spans = vec![Span::styled(" PREFIX ", mode_style), Span::raw(" ")];
    let mut used_width = " PREFIX  ".chars().count();
    let max_width = area.width as usize;
    let prefix = crate::config::format_key_combo((app.prefix_code, app.prefix_mods));

    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        "Esc".to_string(),
        "Cancel",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix,
        "Send",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix_rhs_label(&app.keybinds.command_palette),
        "Commands",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix_rhs_label(&app.keybinds.workspace_picker),
        "Spaces",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix_rhs_label(&app.keybinds.help),
        "Keys",
        false,
    );

    for (key_label, label) in [
        (indexed_prefix_rhs_label(&app.keybinds.switch_tab), "Tabs"),
        (
            indexed_prefix_rhs_label(&app.keybinds.switch_workspace),
            "Spaces",
        ),
        (
            indexed_prefix_rhs_label(&app.keybinds.switch_group),
            "Groups",
        ),
    ] {
        append_prefix_hint(
            &mut spans,
            &mut used_width,
            max_width,
            key,
            dim,
            key_label,
            label,
            true,
        );
    }
    for (key_label, label) in [
        (prefix_rhs_label(&app.keybinds.new_tab), "Tab"),
        (prefix_rhs_label(&app.keybinds.split_vertical), "Split│"),
        (prefix_rhs_label(&app.keybinds.split_horizontal), "Split─"),
        (prefix_rhs_label(&app.keybinds.close_pane), "Close"),
        (prefix_rhs_label(&app.keybinds.detach), "Detach"),
    ] {
        append_prefix_hint(
            &mut spans,
            &mut used_width,
            max_width,
            key,
            dim,
            key_label,
            label,
            true,
        );
    }

    let line = Line::from(spans);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_copy_mode_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let accent = app.active_workspace_accent_color();
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(accent)
        .add_modifier(Modifier::BOLD);
    let Some(copy_mode) = app.copy_mode.as_ref() else {
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
        let (exit_keys, exit_label) =
            if copy_mode.search.query.is_empty() && copy_mode.selection.is_none() {
                ("q/Esc", " Exit")
            } else {
                ("Esc", " Clear  q Exit")
            };
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
            Span::styled(" Copy  ", dim),
            Span::styled(exit_keys, key),
            Span::styled(exit_label, dim),
        ])
    };
    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_navigate_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let accent = app.active_workspace_accent_color();
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(accent)
        .add_modifier(Modifier::BOLD);

    let kb = &app.keybinds;
    let new_tab = prefix_rhs_label(&kb.new_tab);
    let split_vertical = prefix_rhs_label(&kb.split_vertical);
    let split_horizontal = prefix_rhs_label(&kb.split_horizontal);
    let close_pane = prefix_rhs_label(&kb.close_pane);
    let zoom = prefix_rhs_label(&kb.zoom);
    let resize = prefix_rhs_label(&kb.resize_mode);
    let command_palette = prefix_rhs_label(&kb.command_palette);
    let help = prefix_rhs_label(&kb.help);
    let settings = prefix_rhs_label(&kb.settings);
    let detach = prefix_rhs_label(&kb.detach);
    let workspace_nav = format!(
        "{} / {}",
        keybind_label(&kb.navigate.workspace_up),
        keybind_label(&kb.navigate.workspace_down)
    );
    let line = Line::from(vec![
        Span::styled(" Navigate ", mode_style),
        Span::raw(" "),
        Span::styled("Esc", key),
        Span::styled(" Back  ", dim),
        Span::styled(workspace_nav, key),
        Span::styled(" Space  ", dim),
        Span::styled("↵", key),
        Span::styled(" Open  ", dim),
        Span::styled("⇥", key),
        Span::styled(" Pane  ", dim),
        Span::styled(command_palette, key),
        Span::styled(" Commands  ", dim),
        Span::styled(new_tab, key),
        Span::styled(" New Tab  ", dim),
        Span::styled(split_vertical, key),
        Span::styled(" Split│  ", dim),
        Span::styled(split_horizontal, key),
        Span::styled(" Split─  ", dim),
        Span::styled(close_pane, key),
        Span::styled(" Close  ", dim),
        Span::styled(zoom, key),
        Span::styled(" Zoom  ", dim),
        Span::styled(resize, key),
        Span::styled(" Resize  ", dim),
        Span::styled(help, key),
        Span::styled(" Keybinds  ", dim),
        Span::styled(settings, key),
        Span::styled(" Settings  ", dim),
        Span::styled(detach, key),
        Span::styled(" Detach", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);

    if app.update_available.is_some() {
        let status = Line::from(vec![Span::styled(
            " Update Ready",
            Style::default()
                .fg(app.palette.accent)
                .add_modifier(Modifier::BOLD),
        )]);
        let width = 13u16.min(overlay_area.width);
        let status_area = Rect::new(
            overlay_area.x + overlay_area.width.saturating_sub(width),
            overlay_area.y,
            width,
            overlay_area.height,
        );
        frame.render_widget(Clear, status_area);
        frame.render_widget(
            Paragraph::new(status).alignment(Alignment::Right),
            status_area,
        );
    }
}

pub(super) fn render_global_launcher_menu(app: &AppState, frame: &mut Frame) {
    let rect = app.global_menu_rect();
    let Some(inner) = render_panel_shell(frame, rect, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };

    let items = app.global_menu_labels();
    let visible = app.global_menu.visible();
    for (idx, item) in items.iter().enumerate() {
        let y = inner.y + idx as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let selected = visible == Some(idx);
        let rect = Rect::new(inner.x, y, inner.width, 1);

        let selected_style = Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.palette.accent)
            .add_modifier(Modifier::BOLD);
        let item_style = if selected {
            selected_style
        } else {
            Style::default().fg(app.palette.text)
        };
        let badge_style = if selected {
            selected_style
        } else {
            Style::default()
                .fg(app.palette.accent)
                .add_modifier(Modifier::BOLD)
        };

        let line = if app.global_menu_item_has_badge(item) {
            let label = format!(" {item}");
            let label_width = label.chars().count() as u16;
            let gap_width = rect.width.saturating_sub(label_width.saturating_add(1)) as usize;
            Line::from(vec![
                Span::styled(label, item_style),
                Span::styled(" ".repeat(gap_width), item_style),
                Span::styled("●", badge_style),
            ])
        } else {
            Line::from(Span::styled(format!(" {item}"), item_style))
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

pub(super) fn render_group_menu(app: &AppState, frame: &mut Frame) {
    let rect = app.group_menu_rect();
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
    let visible = app.group_menu.visible();

    for (idx, item) in app.group_menu_labels().iter().enumerate() {
        let selected = visible == Some(idx);
        if idx == 1 {
            render_menu_row(
                frame,
                inner,
                idx,
                group_menu_all_line(app, selected, inner.width),
                selected,
                selected_style,
                text_style,
            );
            continue;
        }
        if let Some(group_idx) = group_menu_group_index(app, idx) {
            render_menu_row(
                frame,
                inner,
                idx,
                group_menu_group_line(app, group_idx, selected, inner.width),
                selected,
                selected_style,
                text_style,
            );
        } else if app.group_menu_action_for_row(idx).is_none() {
            if item == "---" {
                render_menu_separator(frame, inner, idx, dim_style);
            } else {
                render_menu_row(
                    frame,
                    inner,
                    idx,
                    Line::from(format!(" {item}")),
                    false,
                    selected_style,
                    dim_style,
                );
            }
        } else if let Some((name, count)) = count_suffix(item) {
            let line_style = if selected { selected_style } else { text_style };
            let count_style = if selected { selected_style } else { dim_style };
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(vec![
                    Span::styled(name.to_string(), line_style),
                    Span::styled(count.to_string(), count_style),
                ]),
                selected,
                selected_style,
                text_style,
            );
        } else {
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(item.clone()),
                selected,
                selected_style,
                text_style,
            );
        }
    }
}

pub(super) fn render_agent_menu(app: &AppState, frame: &mut Frame) {
    let rect = app.agent_menu_rect();
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
    let visible = app.agent_menu.visible();

    for (idx, item) in app.agent_menu_labels().iter().enumerate() {
        let selected = visible == Some(idx);
        if app.agent_menu_action_for_row(idx).is_none() {
            if item == "---" {
                render_menu_separator(frame, inner, idx, dim_style);
            } else {
                render_menu_row(
                    frame,
                    inner,
                    idx,
                    Line::from(format!(" {item}")),
                    false,
                    selected_style,
                    dim_style,
                );
            }
        } else if let Some((name, count)) = count_suffix(item) {
            let line_style = if selected { selected_style } else { text_style };
            let count_style = if selected { selected_style } else { dim_style };
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(vec![
                    Span::styled(name.to_string(), line_style),
                    Span::styled(count.to_string(), count_style),
                ]),
                selected,
                selected_style,
                text_style,
            );
        } else {
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(item.clone()),
                selected,
                selected_style,
                text_style,
            );
        }
    }
}

pub(super) fn render_resize_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let accent = app.active_workspace_accent_color();
    let key = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.mauve)
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::styled(" Resize ", mode_style),
        Span::raw("  "),
        Span::styled("h/l", key),
        Span::styled(" Width  ", dim),
        Span::styled("j/k", key),
        Span::styled(" Height  ", dim),
        Span::styled("Esc/↵", key),
        Span::styled(" Done", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
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

pub(super) fn render_context_menu(app: &AppState, frame: &mut Frame) {
    let Some(menu) = &app.context_menu else {
        return;
    };

    let palette = context_menu_palette(app, menu);
    let p = &palette;
    let Some(menu_rect) = app.context_menu_rect() else {
        return;
    };
    let Some(inner) = render_panel_shell(frame, menu_rect, p.accent, p.panel_bg) else {
        return;
    };

    let selected_style = Style::default()
        .bg(p.accent)
        .fg(panel_contrast_fg(p))
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(p.text);
    let dim_style = Style::default().fg(p.overlay0);

    let visible = menu.list.visible();
    let visible_range = menu.visible_item_range(inner.height as usize);
    let items = menu.items();
    for (row, item) in items[visible_range.clone()].iter().enumerate() {
        let idx = visible_range.start + row;
        let display_item = ContextMenuState::item_display_label(item);
        if ContextMenuState::item_is_separator(item) {
            render_menu_separator(frame, inner, row, dim_style);
        } else if ContextMenuState::item_is_section_header(item) {
            render_menu_row(
                frame,
                inner,
                row,
                Line::from(format!(" {display_item}")),
                false,
                selected_style,
                dim_style,
            );
        } else {
            render_menu_row(
                frame,
                inner,
                row,
                Line::from(context_menu_row_label(item, items)),
                visible == Some(idx),
                selected_style,
                text_style,
            );
        }
    }
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

fn render_client_list_menu(
    app: &AppState,
    frame: &mut Frame,
    rect: Rect,
    labels: &[String],
    visible: Option<usize>,
) {
    let Some(inner) = render_panel_shell(frame, rect, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };
    let selected = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(app.palette.text);
    let dim = Style::default().fg(app.palette.overlay0);
    for (idx, label) in labels.iter().enumerate() {
        if label == "---" {
            render_menu_separator(frame, inner, idx, dim);
        } else {
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(format!(" {label}")),
                visible == Some(idx),
                selected,
                text,
            );
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
    for (idx, label) in app.global_menu_labels().iter().enumerate() {
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
    let labels = crate::app::client_group_menu_labels(app, view);
    let Some(inner) = render_panel_shell(
        frame,
        crate::app::client_group_menu_rect(app, view),
        app.palette.accent,
        app.palette.panel_bg,
    ) else {
        return;
    };
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(app.palette.text);
    let dim_style = Style::default().fg(app.palette.overlay0);
    let visible = view.group_menu.visible();
    for (idx, label) in labels.iter().enumerate() {
        if label == "---" {
            render_menu_separator(frame, inner, idx, dim_style);
            continue;
        }
        let selected = visible == Some(idx) && app.group_menu_action_for_row(idx).is_some();
        let line = if let Some(group_idx) = group_menu_group_index(app, idx) {
            group_menu_group_line_for_selection(
                app,
                view.group_filter_enabled,
                view.active_group,
                group_idx,
                selected,
                inner.width,
            )
        } else {
            Line::from(format!(" {label}"))
        };
        render_menu_row(
            frame,
            inner,
            idx,
            line,
            selected,
            selected_style,
            text_style,
        );
    }
}

pub(super) fn render_agent_menu_for_view(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
) {
    let labels = crate::app::client_agent_menu_labels(view);
    render_client_list_menu(
        app,
        frame,
        crate::app::client_agent_menu_rect(app, view),
        &labels,
        view.agent_menu
            .visible()
            .filter(|idx| app.agent_menu_action_for_row(*idx).is_some()),
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
    fn group_menu_group_line_uses_group_accent() {
        let mut app = AppState::test_new();
        app.show_counters = true;
        let group_idx = app.create_group("work".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Magenta));

        let line = group_menu_group_line(&app, group_idx, false, 18);

        assert_eq!(line.spans[1].content.as_ref(), "■");
        assert_eq!(
            line.spans[1].style.fg,
            Some(app.group_accent_color(group_idx))
        );
        assert_eq!(
            line.spans[3].style.fg,
            Some(app.group_accent_color(group_idx))
        );
        assert_eq!(line.spans[5].content.as_ref(), "0");
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

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_group_menu_for_view(&app, &view, frame))
            .expect("render client group menu");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_text(buffer, 80, 20, "work").expect("work group row");
        assert_eq!(buffer[(x, y)].style().fg, Some(expected_accent));
    }

    #[test]
    fn prefix_overlay_key_hint_uses_active_group_accent() {
        let mut app = AppState::test_new();
        app.palette.accent = ratatui::style::Color::Rgb(1, 2, 3);
        let group_idx = app.create_group("work".to_string());
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Cyan));
        app.active_group = group_idx;
        app.group_filter_enabled = true;
        app.active = None;
        let expected_accent = app.group_accent_color(group_idx);
        assert_ne!(expected_accent, app.palette.accent);

        let backend = TestBackend::new(96, 8);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_prefix_overlay(&app, frame, Rect::new(0, 0, 96, 8)))
            .expect("render prefix overlay");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_text(buffer, 96, 8, "Esc").expect("esc hint");
        assert_eq!(buffer[(x, y)].style().fg, Some(expected_accent));
        assert_eq!(buffer[(x, y)].style().bg, Some(app.palette.panel_bg));
    }

    #[test]
    fn group_menu_count_lines_right_align_counts() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.group_filter_enabled = false;
        app.workspaces = vec![crate::workspace::Workspace::test_new("a")];
        app.workspaces[0].group_id = app.groups[group_idx].id.clone();
        assert_eq!(line_text(&group_menu_all_line(&app, false, 12)), "✓ All");
        assert_eq!(
            line_text(&group_menu_group_line(&app, group_idx, false, 12)),
            "  ■ work"
        );

        app.show_counters = true;

        let all = group_menu_all_line(&app, false, 12);
        let all_text = line_text(&all);
        assert!(all_text.starts_with("✓ All"));
        assert_eq!(all_text.chars().last(), Some('1'));
        assert_eq!(all_text.chars().count(), 11);
        let all_count = all
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "1")
            .expect("all count span");
        assert_eq!(all_count.style.fg, Some(app.palette.overlay0));

        let group = group_menu_group_line(&app, group_idx, false, 12);
        let group_text = line_text(&group);
        assert!(group_text.starts_with("  ■ work"));
        assert_eq!(group_text.chars().last(), Some('1'));
        assert_eq!(group_text.chars().count(), 11);
        let group_count = group
            .spans
            .iter()
            .find(|span| span.content.as_ref() == "1")
            .expect("group count span");
        assert_eq!(group_count.style.fg, Some(app.palette.overlay0));
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn agent_menu_renders_short_scope_labels() {
        let mut app = AppState::test_new();
        app.view.sidebar_rect = Rect::new(0, 0, 24, 20);
        app.view.terminal_area = Rect::new(24, 0, 56, 20);
        app.agent_menu = crate::app::state::ModalListState::new(0);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_agent_menu(&app, frame))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let text = buffer_text(buffer, 80, 20);
        assert!(text.contains("All"));
        assert!(text.contains("Space"));
        assert!(text.contains("Group"));
        assert!(!text.contains("follow"));

        let (agent_filter_x, agent_filter_y) =
            first_cell_with_text(buffer, 80, 20, "Filter").expect("agent filter row");
        assert_eq!(
            buffer[(agent_filter_x, agent_filter_y)].style().fg,
            Some(app.palette.overlay0)
        );
        assert_ne!(
            buffer[(agent_filter_x, agent_filter_y)].style().bg,
            Some(app.palette.accent),
            "agent filter row should not use selected background"
        );

        for label in ["All", "Space", "Group"] {
            let (x, y) = first_cell_with_text(buffer, 80, 20, label)
                .unwrap_or_else(|| panic!("{label} agent scope row"));
            assert_eq!(
                buffer[(x, y)].style().fg,
                Some(app.palette.text),
                "{label} agent scope row should use normal action text"
            );
            assert_ne!(
                buffer[(x, y)].style().bg,
                Some(app.palette.accent),
                "{label} agent scope row should not be selected when only the filter row is highlighted"
            );
        }

        let mut group_app = AppState::test_new();
        group_app.view.sidebar_rect = app.view.sidebar_rect;
        group_app.view.terminal_area = app.view.terminal_area;
        group_app.group_menu = crate::app::state::ModalListState::new(0);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_group_menu(&group_app, frame))
            .unwrap();

        let group_buffer = terminal.backend().buffer();
        let (group_filter_x, group_filter_y) =
            first_cell_with_text(group_buffer, 80, 20, "Filter").expect("group filter row");
        assert_eq!(
            buffer[(agent_filter_x, agent_filter_y)].style(),
            group_buffer[(group_filter_x, group_filter_y)].style(),
            "agent filter row should match spaces filter row muted styling"
        );
    }

    #[test]
    fn global_menu_selected_row_background_fills_inner_width() {
        let mut app = AppState::test_new();
        app.integration_recommendations.clear();
        app.update_available = None;
        app.latest_release_notes_available = false;
        app.global_menu = crate::app::state::ModalListState::new(1);
        app.view.sidebar_rect = Rect::new(0, 0, 24, 20);
        app.view.terminal_area = Rect::new(24, 0, 56, 20);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_global_launcher_menu(&app, frame))
            .unwrap();

        let rect = app.global_menu_rect();
        let inner = Rect::new(
            rect.x + 1,
            rect.y + 1,
            rect.width.saturating_sub(2),
            rect.height.saturating_sub(2),
        );
        let selected_y = inner.y + app.global_menu.visible().expect("visible row") as u16;
        let buffer = terminal.backend().buffer();
        for x in inner.x..inner.x + inner.width {
            assert_eq!(
                buffer[(x, selected_y)].style().bg,
                Some(app.palette.accent),
                "selected global menu row background should fill the inner width at x={x}"
            );
        }
    }

    #[test]
    fn agent_follow_up_item_uses_one_leading_space() {
        let mut app = AppState::test_new();
        let workspace = crate::workspace::Workspace::test_new("api");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.active = Some(0);
        app.mode = crate::app::state::Mode::ContextMenu;
        app.context_menu = Some(crate::app::state::ContextMenuState {
            kind: crate::app::state::ContextMenuKind::Agent {
                ws_idx: 0,
                pane_id,
                in_follow_up: false,
            },
            x: 4,
            y: 4,
            list: crate::app::state::ModalListState::hidden(0),
        });
        crate::ui::compute_view(&mut app, Rect::new(0, 0, 80, 20));

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| render_context_menu(&app, frame))
            .expect("render context menu");

        let buffer = terminal.backend().buffer();
        let (x, y) = first_cell_with_text(buffer, 80, 20, "Add to Follow Up")
            .expect("add to follow up label");
        let rect = app.context_menu_rect().expect("menu rect");
        assert_eq!(
            x,
            rect.x + 2,
            "unsectioned items should use one space after the border"
        );
        assert_eq!(buffer[(x - 1, y)].symbol(), " ");
        assert_ne!(buffer[(x - 2, y)].symbol(), " ");
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
