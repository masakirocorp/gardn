use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use super::widgets::{panel_contrast_fg, render_panel_shell};
use crate::app::AppState;

fn count_suffix(text: &str) -> Option<(&str, &str)> {
    let start = text.rfind(" (")?;
    text.ends_with(')')
        .then_some((&text[..start], &text[start..]))
}

fn prefix_rhs_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings
        .prefix_rhs_label()
        .unwrap_or_else(|| "unset".to_string())
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
    let line = Line::from(vec![
        Span::styled(" navigate ", mode_style),
        Span::raw(" "),
        Span::styled("esc", key),
        Span::styled(" back  ", dim),
        Span::styled("↑↓", key),
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

    let items: Vec<ListItem> = app
        .group_menu_labels()
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            if app.group_menu_action_for_row(idx).is_none() {
                if item == "---" {
                    ListItem::new(Line::from("-".repeat(inner.width as usize)))
                        .style(Style::default().fg(app.palette.overlay0))
                } else {
                    ListItem::new(Line::from(format!(" {item}")))
                        .style(Style::default().fg(app.palette.overlay0))
                }
            } else if let Some((name, count)) = count_suffix(item) {
                ListItem::new(Line::from(vec![
                    Span::styled(name.to_string(), Style::default().fg(app.palette.text)),
                    Span::styled(count.to_string(), Style::default().fg(app.palette.overlay0)),
                ]))
            } else {
                ListItem::new(Line::from(item.clone()))
            }
        })
        .collect();
    let list = List::new(items)
        .style(Style::default().fg(app.palette.text))
        .highlight_style(
            Style::default()
                .bg(app.palette.accent)
                .fg(panel_contrast_fg(&app.palette))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ");
    let mut state = ListState::default().with_selected(Some(app.group_menu.highlighted));
    frame.render_stateful_widget(list, inner, &mut state);
}

pub(super) fn render_agent_menu(app: &AppState, frame: &mut Frame) {
    let rect = app.agent_menu_rect();
    let Some(inner) = render_panel_shell(frame, rect, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };

    let items: Vec<ListItem> = app
        .agent_menu_labels()
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            if item == "---" {
                ListItem::new(Line::from("-".repeat(inner.width as usize)))
                    .style(Style::default().fg(app.palette.overlay0))
            } else if app.agent_menu_action_for_row(idx).is_none() {
                ListItem::new(Line::from(item.clone()))
                    .style(Style::default().fg(app.palette.overlay0))
            } else if let Some((name, count)) = count_suffix(item) {
                ListItem::new(Line::from(vec![
                    Span::styled(name.to_string(), Style::default().fg(app.palette.text)),
                    Span::styled(count.to_string(), Style::default().fg(app.palette.overlay0)),
                ]))
            } else {
                ListItem::new(Line::from(item.clone())).style(Style::default().fg(app.palette.text))
            }
        })
        .collect();
    let list = List::new(items)
        .style(Style::default().fg(app.palette.text))
        .highlight_style(
            Style::default()
                .bg(app.palette.accent)
                .fg(panel_contrast_fg(&app.palette))
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default().with_selected(Some(app.agent_menu.highlighted));
    frame.render_stateful_widget(list, inner, &mut state);
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

    let items: Vec<ListItem> = menu
        .items()
        .iter()
        .map(|item| ListItem::new(Line::from(*item)))
        .collect();
    let list = List::new(items)
        .style(Style::default().fg(p.text))
        .highlight_style(
            Style::default()
                .bg(p.accent)
                .fg(panel_contrast_fg(p))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" ");
    let mut state = ListState::default().with_selected(Some(menu.list.highlighted));
    frame.render_stateful_widget(list, inner, &mut state);
}
