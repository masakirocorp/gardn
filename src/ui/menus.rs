use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::widgets::{panel_contrast_fg, render_panel_shell};
use crate::app::{state::ContextMenuState, AppState};

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
    let group_start = 3;
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
    let marker = if app.group_filter_enabled { " " } else { "*" };
    let count = app.workspaces.len().to_string();
    let left = format!("{marker} all");
    let selected_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let text_style = if selected {
        selected_style
    } else {
        Style::default().fg(app.palette.text)
    };
    let count_style = if selected {
        selected_style
    } else {
        Style::default().fg(app.palette.overlay0)
    };

    Line::from(vec![
        Span::styled(left.clone(), text_style),
        Span::styled(
            right_aligned_count_gap(width, left.chars().count(), count.chars().count()),
            text_style,
        ),
        Span::styled(count, count_style),
    ])
}

fn group_menu_group_line(
    app: &AppState,
    group_idx: usize,
    selected: bool,
    width: u16,
) -> Line<'static> {
    let Some(group) = app.groups.get(group_idx) else {
        return Line::raw("");
    };
    let marker = if app.group_filter_enabled && group_idx == app.active_group {
        "*"
    } else {
        " "
    };
    let count = app
        .workspaces
        .iter()
        .filter(|workspace| workspace.group_id == group.id)
        .count()
        .to_string();
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
    let count_style = if selected {
        selected_style
    } else {
        Style::default().fg(app.palette.overlay0)
    };
    let left_width =
        marker.chars().count() + 1 + group.icon.chars().count() + 1 + group.name.chars().count();
    Line::from(vec![
        Span::styled(format!("{marker} "), group_style),
        Span::styled(group.icon.clone(), group_style),
        Span::styled(" ", group_style),
        Span::styled(group.name.clone(), group_style),
        Span::styled(
            right_aligned_count_gap(width, left_width, count.chars().count()),
            group_style,
        ),
        Span::styled(count, count_style),
    ])
}

fn prefix_rhs_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings
        .prefix_rhs_label()
        .unwrap_or_else(|| "unset".to_string())
}

fn indexed_prefix_rhs_label(bindings: &[crate::config::IndexedKeybind]) -> String {
    let labels: Vec<&str> = bindings
        .iter()
        .filter(|binding| binding.trigger.is_prefix())
        .map(|binding| {
            binding
                .label
                .strip_prefix("prefix+")
                .unwrap_or(&binding.label)
        })
        .collect();

    if labels.is_empty() {
        return "unset".to_string();
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
    if key_label == "unset" {
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
    bindings.label().unwrap_or_else(|| "unset".to_string())
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
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
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
        "esc".to_string(),
        "cancel",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix,
        "send",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix_rhs_label(&app.keybinds.command_palette),
        "cmds",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix_rhs_label(&app.keybinds.workspace_picker),
        "spaces",
        false,
    );
    append_prefix_hint(
        &mut spans,
        &mut used_width,
        max_width,
        key,
        dim,
        prefix_rhs_label(&app.keybinds.help),
        "keys",
        false,
    );

    for (key_label, label) in [
        (indexed_prefix_rhs_label(&app.keybinds.switch_tab), "tabs"),
        (
            indexed_prefix_rhs_label(&app.keybinds.switch_workspace),
            "spaces",
        ),
        (
            indexed_prefix_rhs_label(&app.keybinds.switch_group),
            "groups",
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
        (prefix_rhs_label(&app.keybinds.new_tab), "tab"),
        (prefix_rhs_label(&app.keybinds.split_vertical), "split│"),
        (prefix_rhs_label(&app.keybinds.split_horizontal), "split─"),
        (prefix_rhs_label(&app.keybinds.close_pane), "close"),
        (prefix_rhs_label(&app.keybinds.detach), "detach"),
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
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);

    let select = if app
        .copy_mode
        .is_some_and(|copy_mode| copy_mode.selection.is_some())
    {
        "selecting"
    } else {
        "select"
    };
    let line = Line::from(vec![
        Span::styled(" COPY ", mode_style),
        Span::raw(" "),
        Span::styled("h/j/k/l w/b/e { }", key),
        Span::styled(" move  ", dim),
        Span::styled("v/space", key),
        Span::styled(format!(" {select}  "), dim),
        Span::styled("y/enter", key),
        Span::styled(" copy  ", dim),
        Span::styled("q/esc", key),
        Span::styled(" exit", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_navigate_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
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
        Span::styled(" navigate ", mode_style),
        Span::raw(" "),
        Span::styled("esc", key),
        Span::styled(" back  ", dim),
        Span::styled(workspace_nav, key),
        Span::styled(" space  ", dim),
        Span::styled("↵", key),
        Span::styled(" open  ", dim),
        Span::styled("⇥", key),
        Span::styled(" pane  ", dim),
        Span::styled(command_palette, key),
        Span::styled(" commands  ", dim),
        Span::styled(new_tab, key),
        Span::styled(" new tab  ", dim),
        Span::styled(split_vertical, key),
        Span::styled(" split│  ", dim),
        Span::styled(split_horizontal, key),
        Span::styled(" split─  ", dim),
        Span::styled(close_pane, key),
        Span::styled(" close  ", dim),
        Span::styled(zoom, key),
        Span::styled(" zoom  ", dim),
        Span::styled(resize, key),
        Span::styled(" resize  ", dim),
        Span::styled(help, key),
        Span::styled(" keybinds  ", dim),
        Span::styled(settings, key),
        Span::styled(" settings  ", dim),
        Span::styled(detach, key),
        Span::styled(" detach", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);

    if app.update_available.is_some() {
        let status = Line::from(vec![Span::styled(
            " update ready",
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
    for (idx, item) in items.iter().enumerate() {
        let y = inner.y + idx as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let selected = idx == app.global_menu.highlighted;
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
            Line::from(vec![
                Span::styled(" ●", badge_style),
                Span::styled(format!(" {item} "), item_style),
            ])
        } else {
            Line::from(Span::styled(format!(" {item} "), item_style))
        };
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), rect);
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

    for (idx, item) in app.group_menu_labels().iter().enumerate() {
        let selected = idx == app.group_menu.highlighted;
        if idx == 0 {
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

    for (idx, item) in app.agent_menu_labels().iter().enumerate() {
        let selected = idx == app.agent_menu.highlighted;
        if item == "---" {
            render_menu_separator(frame, inner, idx, dim_style);
        } else if app.agent_menu_action_for_row(idx).is_none() {
            if idx == 5 {
                if let Some(group_idx) = app.agent_menu_group_context_idx() {
                    render_menu_row(
                        frame,
                        inner,
                        idx,
                        Line::from(vec![
                            Span::styled("  ", dim_style),
                            Span::styled(
                                item.trim_start().to_string(),
                                Style::default()
                                    .fg(app.group_accent_color(group_idx))
                                    .add_modifier(Modifier::BOLD),
                            ),
                        ]),
                        false,
                        selected_style,
                        dim_style,
                    );
                } else {
                    render_menu_row(
                        frame,
                        inner,
                        idx,
                        Line::from(item.clone()),
                        false,
                        selected_style,
                        dim_style,
                    );
                }
            } else {
                render_menu_row(
                    frame,
                    inner,
                    idx,
                    Line::from(item.clone()),
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
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.mauve)
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::styled(" resize ", mode_style),
        Span::raw("  "),
        Span::styled("h/l", key),
        Span::styled(" width  ", dim),
        Span::styled("j/k", key),
        Span::styled(" height  ", dim),
        Span::styled("esc/↵", key),
        Span::styled(" done", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_context_menu(app: &AppState, frame: &mut Frame) {
    let Some(menu) = &app.context_menu else {
        return;
    };

    let p = &app.palette;
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

    for (idx, item) in menu.items().iter().enumerate() {
        if ContextMenuState::item_is_separator(item) {
            render_menu_separator(frame, inner, idx, dim_style);
        } else if ContextMenuState::item_is_section_header(item) {
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(format!(" {item}")),
                false,
                selected_style,
                dim_style,
            );
        } else {
            render_menu_row(
                frame,
                inner,
                idx,
                Line::from(format!(" {item}")),
                idx == menu.list.highlighted,
                selected_style,
                text_style,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_separator_bounds_inset_rule_when_roomy() {
        assert_eq!(menu_separator_bounds(5), (1, 4));
        assert_eq!(menu_separator_bounds(2), (0, 2));
    }

    #[test]
    fn group_menu_group_line_uses_group_accent() {
        let mut app = AppState::test_new();
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
    fn group_menu_count_lines_right_align_counts() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.groups[group_idx].icon = "■".to_string();
        app.workspaces = vec![crate::workspace::Workspace::test_new("a")];
        app.workspaces[0].group_id = app.groups[group_idx].id.clone();

        let all = group_menu_all_line(&app, false, 12);
        assert_eq!(all.spans[1].content.as_ref(), "     ");
        assert_eq!(all.spans[2].content.as_ref(), "1");

        let group = group_menu_group_line(&app, group_idx, false, 12);
        assert_eq!(group.spans[4].content.as_ref(), "  ");
        assert_eq!(group.spans[5].content.as_ref(), "1");
    }

    #[test]
    fn agent_menu_group_detail_uses_group_accent() {
        let mut app = AppState::test_new();
        let group_idx = app.create_group("work".to_string());
        app.set_group_accent(group_idx, Some(crate::config::TerminalAccent::Blue));
        app.active_group = group_idx;
        app.group_filter_enabled = true;

        let item = app.agent_menu_labels()[5].clone();
        let line = if let Some(group_idx) = app.agent_menu_group_context_idx() {
            Line::from(vec![
                Span::styled("  ", Style::default().fg(app.palette.overlay0)),
                Span::styled(
                    item.trim_start().to_string(),
                    Style::default()
                        .fg(app.group_accent_color(group_idx))
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::raw(item)
        };

        assert_eq!(line.spans[1].content.as_ref(), "work");
        assert_eq!(
            line.spans[1].style.fg,
            Some(app.group_accent_color(group_idx))
        );
    }
}
